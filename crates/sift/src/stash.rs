//! stash 卸载存储(原文暂存,凭标记取回)。
//! 有损压缩的恢复通道——原文存入 store，
//! 压缩输出只留一个标记，下游可按标记取回原文，端到端无损。
//!
//! 存储后端：
//! - [`FileStashStore`]：落盘文件（生产默认）。每个 key 一个文件，重启不丢，
//!   多进程共享同一目录即可互见（集群需挂载共享文件系统或改用外部 store）。
//! - [`InMemoryStashStore`]：内存实现（测试用），进程退出即丢。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// 卸载键：BLAKE3 哈希前 24 个 hex 字符。
pub fn compute_key(content: &str) -> String {
    blake3::hash(content.as_bytes()).to_hex()[..24].to_string()
}

/// 压缩输出中的取回标记格式。
pub fn marker_for(key: &str) -> String {
    format!("<<stash:{key}>>")
}

/// stash key 的公开边界校验。
///
/// 生产 key 固定为 BLAKE3 前 24 个十六进制字符。`retrieve` 的 key 来自调用方，
/// 不能直接拿来拼接文件路径，否则 `..` 或绝对路径会逃逸 stash 目录。
pub fn is_valid_key(key: &str) -> bool {
    key.len() == 24 && key.bytes().all(|b| b.is_ascii_hexdigit())
}

/// 文本是否已经含有一个合法 stash marker。
///
/// 已压缩文本再次进入管线时必须保持幂等；否则会形成 marker 链，调用方需要
/// 多次递归取回才能得到真正原文。
pub fn contains_marker(text: &str) -> bool {
    let mut offset = 0usize;
    while let Some(relative) = text[offset..].find("<<stash:") {
        let key_start = offset + relative + "<<stash:".len();
        let tail = &text.as_bytes()[key_start..];
        if tail.len() >= 26
            && tail[..24].iter().all(u8::is_ascii_hexdigit)
            && &tail[24..26] == b">>"
        {
            return true;
        }
        if key_start >= text.len() {
            break;
        }
        offset = key_start + text[key_start..].chars().next().unwrap().len_utf8();
    }
    false
}

/// 卸载存储 trait。
pub trait StashStore: Send + Sync {
    /// 原文必须确认写入成功后，调用方才可以发布带 marker 的有损结果。
    fn put(&self, key: &str, content: &str) -> std::io::Result<()>;
    fn get(&self, key: &str) -> Option<String>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 落盘文件实现（生产默认）。
///
/// 每个 key 一个文件（key 为 24 hex，天然是安全文件名，无路径穿越风险）。
/// 写入用「临时文件 + 原子 rename」避免读到写一半的内容；TTL 用文件 mtime
/// 判定，`get` 时惰性删除过期项。
pub struct FileStashStore {
    dir: PathBuf,
    ttl: Duration,
}

impl FileStashStore {
    pub fn new(dir: impl AsRef<Path>) -> std::io::Result<Self> {
        Self::with_ttl(dir, DEFAULT_TTL)
    }

    pub fn with_ttl(dir: impl AsRef<Path>, ttl: Duration) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir.as_ref())?;
        Ok(Self {
            dir: dir.as_ref().to_path_buf(),
            ttl,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// 清理所有过期项（`get` 之外的主动清理入口）。
    pub fn purge_expired(&self) -> std::io::Result<usize> {
        let mut removed = 0;
        for entry in std::fs::read_dir(&self.dir)? {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            // 跳过临时文件（`.` 前缀）
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if self.is_expired(&path) {
                std::fs::remove_file(&path).ok();
                removed += 1;
            }
        }
        Ok(removed)
    }

    fn is_expired(&self, path: &Path) -> bool {
        let Ok(meta) = std::fs::metadata(path) else {
            return true;
        };
        match meta.modified().ok().and_then(|m| m.elapsed().ok()) {
            Some(elapsed) => elapsed > self.ttl,
            None => true,
        }
    }
}

impl StashStore for FileStashStore {
    fn put(&self, key: &str, content: &str) -> std::io::Result<()> {
        if !is_valid_key(key) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid stash key",
            ));
        }
        let dst = self.dir.join(key);
        // 临时文件（`.` 前缀，purge 时跳过）写完再原子 rename 到目标。
        // 序号避免同一 key 并发写入时争用同一个临时文件。
        static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let seq = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let tmp = self
            .dir
            .join(format!(".{key}.{}.{}.tmp", std::process::id(), seq));
        std::fs::write(&tmp, content)?;
        if let Err(err) = std::fs::rename(&tmp, &dst) {
            let _ = std::fs::remove_file(&tmp);
            return Err(err);
        }
        Ok(())
    }

    fn get(&self, key: &str) -> Option<String> {
        if !is_valid_key(key) {
            return None;
        }
        let path = self.dir.join(key);
        if self.is_expired(&path) {
            let _ = std::fs::remove_file(&path);
            return None;
        }
        std::fs::read_to_string(&path).ok()
    }

    fn len(&self) -> usize {
        std::fs::read_dir(&self.dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path().is_file()
                            && !e.file_name().to_string_lossy().starts_with('.')
                    })
                    .count()
            })
            .unwrap_or(0)
    }
}

const DEFAULT_TTL: Duration = Duration::from_secs(1800);

/// 内存实现（测试用）。固定 TTL 1800s，进程退出即丢。
#[derive(Default)]
pub struct InMemoryStashStore {
    ttl: Duration,
    entries: std::sync::Mutex<HashMap<String, (String, Instant)>>,
}

impl InMemoryStashStore {
    pub fn new() -> Self {
        Self {
            ttl: DEFAULT_TTL,
            entries: Default::default(),
        }
    }
}

impl StashStore for InMemoryStashStore {
    fn put(&self, key: &str, content: &str) -> std::io::Result<()> {
        if !is_valid_key(key) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid stash key",
            ));
        }
        self.entries
            .lock()
            .map_err(|_| std::io::Error::other("stash lock poisoned"))?
            .insert(key.to_string(), (content.to_string(), Instant::now()));
        Ok(())
    }

    fn get(&self, key: &str) -> Option<String> {
        if !is_valid_key(key) {
            return None;
        }
        let mut map = self.entries.lock().unwrap();
        match map.get(key) {
            Some((content, at)) if at.elapsed() < self.ttl => Some(content.clone()),
            Some(_) => {
                map.remove(key);
                None
            }
            None => None,
        }
    }

    fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_stable_24_hex() {
        let k = compute_key("hello");
        assert_eq!(k.len(), 24);
        assert_eq!(k, compute_key("hello"));
        assert_ne!(k, compute_key("world"));
    }

    #[test]
    fn marker_roundtrip() {
        let store = InMemoryStashStore::new();
        let k = compute_key("original content");
        store.put(&k, "original content").unwrap();
        let marker = marker_for(&k);
        assert_eq!(marker, format!("<<stash:{k}>>"));
        let fetched = store.get(&k).unwrap();
        assert_eq!(fetched, "original content");
        assert!(contains_marker(&format!("summary {marker}")));
        assert!(!contains_marker("<<stash:not-a-valid-key>>"));
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stash-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn file_store_roundtrip_survives_reopen() {
        let dir = test_dir("reopen");
        let k = compute_key("persist me");
        {
            let store = FileStashStore::new(&dir).unwrap();
            store.put(&k, "persist me").unwrap();
        }
        // 重新打开（模拟重启）后仍能取回
        let store2 = FileStashStore::new(&dir).unwrap();
        assert_eq!(store2.get(&k).unwrap(), "persist me");
        assert_eq!(store2.len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn file_store_ttl_expires() {
        let dir = test_dir("ttl");
        let store = FileStashStore::with_ttl(&dir, Duration::from_millis(40)).unwrap();
        let k = compute_key("expire me");
        store.put(&k, "expire me").unwrap();
        assert_eq!(store.get(&k).unwrap(), "expire me");
        std::thread::sleep(Duration::from_millis(60));
        assert!(store.get(&k).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn file_store_purge_removes_expired() {
        let dir = test_dir("purge");
        let store = FileStashStore::with_ttl(&dir, Duration::from_millis(30)).unwrap();
        store.put(&compute_key("a"), "a").unwrap();
        store.put(&compute_key("b"), "b").unwrap();
        std::thread::sleep(Duration::from_millis(50));
        let removed = store.purge_expired().unwrap();
        assert_eq!(removed, 2);
        assert_eq!(store.len(), 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_keys_that_can_escape_the_stash_directory() {
        let dir = test_dir("invalid-key");
        let store = FileStashStore::new(&dir).unwrap();

        let invalid = [
            "../outside".to_string(),
            "/tmp/outside".to_string(),
            "abc".to_string(),
            "g".repeat(24),
        ];
        for key in &invalid {
            assert!(store.put(key, "must not be written").is_err());
            assert!(store.get(key).is_none());
        }

        std::fs::remove_dir_all(&dir).ok();
    }
}
