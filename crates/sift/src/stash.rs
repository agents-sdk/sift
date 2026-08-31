//! stash 卸载存储(原文暂存,凭标记取回)。
//! 有损压缩的恢复通道——原文存入 store，
//! 压缩输出只留一个标记，下游可按标记取回原文，端到端无损。
//!
//! 存储后端：
//! - [`FileStashStore`]：落盘文件（生产默认）。每个 key 一个文件，重启不丢，
//!   多进程共享同一目录即可互见（集群需挂载共享文件系统或改用外部 store）。
//! - [`InMemoryStashStore`]：内存实现（测试用），进程退出即丢。

use std::collections::HashMap;
use std::io::BufRead;
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

/// stash 原文的按行切片。行号从 1 开始，`text` 保留命中行的原始换行字节。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashSlice {
    pub text: String,
    pub start_line: usize,
    pub line_count: usize,
    pub total_lines: usize,
    pub has_more: bool,
}

fn slice_content_lines(content: &str, start_line: usize, line_count: usize) -> Option<StashSlice> {
    if start_line == 0 || line_count == 0 || content.is_empty() {
        return None;
    }
    // `split_inclusive` 保留 LF/CRLF；结尾换行不额外制造一个空行。
    let lines: Vec<&str> = content.split_inclusive('\n').collect();
    let total_lines = lines.len();
    if start_line > total_lines {
        return None;
    }
    let start = start_line - 1;
    let end = start.saturating_add(line_count).min(total_lines);
    Some(StashSlice {
        text: lines[start..end].concat(),
        start_line,
        line_count: end - start,
        total_lines,
        has_more: end < total_lines,
    })
}

/// 卸载存储 trait。
pub trait StashStore: Send + Sync {
    /// 原文必须确认写入成功后，调用方才可以发布带 marker 的有损结果。
    fn put(&self, key: &str, content: &str) -> std::io::Result<()>;
    fn get(&self, key: &str) -> Option<String>;
    /// 读取 stash 原文中的连续行；默认实现适用于内存或自定义后端。
    fn get_lines(&self, key: &str, start_line: usize, line_count: usize) -> Option<StashSlice> {
        let content = self.get(key)?;
        slice_content_lines(&content, start_line, line_count)
    }
    /// 返回 Coding Agent 可直接读取的本地 stash 文件路径。
    /// 内存、远程或不共享文件系统的后端保持 `None`，不得伪造路径。
    fn file_path(&self, _key: &str) -> Option<PathBuf> {
        None
    }
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
        let dir = if dir.as_ref().is_absolute() {
            dir.as_ref().to_path_buf()
        } else {
            std::env::current_dir()?.join(dir)
        };
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            dir: std::fs::canonicalize(dir)?,
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

    fn get_lines(&self, key: &str, start_line: usize, line_count: usize) -> Option<StashSlice> {
        if !is_valid_key(key) || start_line == 0 || line_count == 0 {
            return None;
        }
        let path = self.dir.join(key);
        if self.is_expired(&path) {
            let _ = std::fs::remove_file(&path);
            return None;
        }

        let file = std::fs::File::open(path).ok()?;
        let mut reader = std::io::BufReader::new(file);
        let mut buf = Vec::new();
        let mut selected = Vec::new();
        let mut total_lines = 0usize;
        loop {
            buf.clear();
            let read = reader.read_until(b'\n', &mut buf).ok()?;
            if read == 0 {
                break;
            }
            total_lines += 1;
            if total_lines >= start_line && total_lines < start_line.saturating_add(line_count) {
                selected.extend_from_slice(&buf);
            }
        }
        if start_line > total_lines {
            return None;
        }
        let actual = total_lines.saturating_sub(start_line - 1).min(line_count);
        Some(StashSlice {
            text: String::from_utf8(selected).ok()?,
            start_line,
            line_count: actual,
            total_lines,
            has_more: start_line - 1 + actual < total_lines,
        })
    }

    fn file_path(&self, key: &str) -> Option<PathBuf> {
        is_valid_key(key).then(|| self.dir.join(key))
    }

    fn len(&self) -> usize {
        std::fs::read_dir(&self.dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path().is_file() && !e.file_name().to_string_lossy().starts_with('.')
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
    fn file_store_exposes_absolute_entry_path_for_direct_reads() {
        let dir = test_dir("entry-path");
        let store = FileStashStore::new(&dir).unwrap();
        let key = compute_key("directly readable");

        let path = store.file_path(&key).unwrap();
        assert!(path.is_absolute());
        assert_eq!(path, store.dir().join(key));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn in_memory_store_reads_exact_line_slice() {
        let store = InMemoryStashStore::new();
        let content = "alpha\r\nbeta\n\ngamma";
        let key = compute_key(content);
        store.put(&key, content).unwrap();

        let slice = store.get_lines(&key, 2, 2).unwrap();
        assert_eq!(slice.text, "beta\n\n");
        assert_eq!(slice.start_line, 2);
        assert_eq!(slice.line_count, 2);
        assert_eq!(slice.total_lines, 4);
        assert!(slice.has_more);
    }

    #[test]
    fn file_store_line_slice_preserves_bytes_and_trailing_newline_semantics() {
        let dir = test_dir("line-slice");
        let store = FileStashStore::new(&dir).unwrap();
        let content = "一\r\n二\n三\n";
        let key = compute_key(content);
        store.put(&key, content).unwrap();

        let slice = store.get_lines(&key, 2, 10).unwrap();
        assert_eq!(slice.text, "二\n三\n");
        assert_eq!(slice.line_count, 2);
        assert_eq!(slice.total_lines, 3);
        assert!(!slice.has_more);
        assert!(store.get_lines(&key, 4, 1).is_none());

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
