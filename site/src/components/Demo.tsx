// 首页压缩演示岛:类型选择 → 左右对比;自定义粘贴走 /api/compress。
// 统计区用"比例条"呈现:陶土红 = 原始体量,工程绿 = 压缩后。
import { useMemo, useState } from 'react';
import samples from '../data/samples.json';

function CopyButton({ text }: { text: string }) {
  const [done, setDone] = useState(false);
  return (
    <button
      className={done ? 'copy-btn done' : 'copy-btn'}
      onClick={() => {
        navigator.clipboard?.writeText(text).then(() => {
          setDone(true);
          setTimeout(() => setDone(false), 1400);
        });
      }}
      aria-label="复制内容"
    >
      {done ? (
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M3 8.5 6.5 12 13 4.5"/></svg>
      ) : (
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><rect x="5" y="4.5" width="8" height="10" rx="1.5"/><path d="M10.5 4.5v-1a1.5 1.5 0 0 0-1.5-1.5H4A1.5 1.5 0 0 0 2.5 3.5v8A1.5 1.5 0 0 0 4 13h1"/></svg>
      )}
    </button>
  );
}

interface Sample {
  type: string;
  label: string;
  desc: string;
  query: string;
  detected: string;
  changed: boolean;
  lossy: boolean;
  stashKey: string | null;
  tokensSaved: number;
  original: string;
  compressed: string;
}

interface ApiResult {
  detected: string;
  changed: boolean;
  lossy: boolean;
  stashKey: string | null;
  tokensSaved: number;
  text: string;
}

type Mode = 'builtin' | 'custom';

function bytes(n: number): string {
  return n >= 1024 ? `${(n / 1024).toFixed(1)} KB` : `${n} B`;
}

// 压缩输出的差异高亮:
//  - «/<<stash:HASH>>  恢复标记 → 金色
//  - [N lines omitted: …] / [... N more …] / [N lines compressed …] → 灰色折叠标记
//  - // ... N lines omitted from file "...", starting at line M → 灰色折叠标记
function highlightCompressed(text: string) {
  const parts: Array<{ t: string; k: 'stash' | 'fold' | 'stat' | null }> = [];
  // 顺序匹配:stash 标记 | 括号折叠行 | 代码折叠注释
  const re =
    /(<<stash:[a-f0-9]+>>|«stash:[a-f0-9]+»)|(\[\.\.\. \d+ lines omitted from file "(?:\\.|[^"\\\n])*", starting at line \d+\]|\[[^\]\n]{0,120}(?:omitted|compressed|more|changed|Retrieve)[^\]\n]{0,120}\])|((?:\/\/|#)\s*\.\.\.\s*\d+\s+lines?\s+omitted(?:\s+from\s+file\s+"(?:\\.|[^"\\\n])*",\s+starting\s+at\s+line\s+\d+)?)/g;
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text))) {
    if (m.index > last) parts.push({ t: text.slice(last, m.index), k: null });
    parts.push({ t: m[0], k: m[1] ? 'stash' : 'fold' });
    last = m.index + m[0].length;
  }
  if (last < text.length) parts.push({ t: text.slice(last), k: null });
  return parts.map((p, i) =>
    p.k ? <span key={i} className={`hl-${p.k}`}>{p.t}</span> : <span key={i}>{p.t}</span>
  );
}

export default function Demo() {
  const list = samples as Sample[];
  const [mode, setMode] = useState<Mode>('builtin');
  const [idx, setIdx] = useState(0);

  // 自定义输入状态
  const [input, setInput] = useState('');
  const [query, setQuery] = useState('');
  const [sourcePath, setSourcePath] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<ApiResult | null>(null);

  const sample = list[idx];

  const orig = mode === 'builtin' ? sample.original : input;
  const comp = mode === 'builtin' ? sample.compressed : (result?.text ?? '');

  const stats = useMemo(() => {
    if (mode === 'custom' && !result) return null;
    const oB = new TextEncoder().encode(orig).length;
    const cB = new TextEncoder().encode(comp).length;
    return {
      oB,
      cB,
      ratio: oB > 0 ? Math.max(0, 1 - cB / oB) : 0,
      tokensSaved: mode === 'builtin' ? sample.tokensSaved : (result?.tokensSaved ?? 0),
      lossy: mode === 'builtin' ? sample.lossy : (result?.lossy ?? false),
      detected: mode === 'builtin' ? sample.detected : result?.detected,
      stashKey: mode === 'builtin' ? sample.stashKey : result?.stashKey,
      changed: mode === 'builtin' ? sample.changed : result?.changed,
    };
  }, [mode, orig, comp, result, sample]);

  async function run() {
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      const res = await fetch('/api/compress', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ text: input, query, sourcePath }),
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error ?? `HTTP ${res.status}`);
      setResult(data as ApiResult);
    } catch (e) {
      setError(String((e as Error).message ?? e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="demo">
      <div className="demo-tabs">
        <button
          className={mode === 'builtin' ? 'tab active' : 'tab'}
          onClick={() => setMode('builtin')}
        >
          内置样本
        </button>
        <button
          className={mode === 'custom' ? 'tab active' : 'tab'}
          onClick={() => setMode('custom')}
        >
          粘贴自己的内容
        </button>
      </div>

      {mode === 'builtin' && (
        <div className="type-chips">
          {list.map((s, i) => (
            <button
              key={s.type}
              className={i === idx ? 'chip active' : 'chip'}
              onClick={() => setIdx(i)}
            >
              {s.label}
            </button>
          ))}
        </div>
      )}

      {mode === 'builtin' && <p className="sample-desc">{sample.desc}</p>}
      {mode === 'builtin' && <p className="note">相关性 query：<code>{sample.query || '未设置'}</code></p>}
      {((mode === 'builtin' && sample.type === 'plain_text') || (mode === 'custom' && result?.detected === 'plain_text')) && (
        <p className="note">纯文本默认保守去重：保留完整段落或发言块，仅折叠同章节完全相同的块。不按 query 或目标比例删除独有内容；没有明确重复时原样返回。</p>
      )}
      {mode === 'builtin' && sample.lossy && (
        <p className="note">内置样例是预生成结果，stash 路径属于样例生成环境；本地接入时会显示实际落盘路径。</p>
      )}
      {mode === 'builtin' && !sample.lossy && sample.changed && (
        <p className="note">本例仅做无损重排，没有省略片段，也没有外置 stash 文件，因此不显示行号提示。</p>
      )}

      {mode === 'custom' && (
        <div className="custom-input">
          <textarea
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="粘贴任意内容:源码、JSON 数组、构建日志、grep 结果、git diff、普通文本……"
            rows={7}
          />
          <div className="custom-actions">
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="可选:相关性 query(帮助保留关键行)"
            />
            <input
              value={sourcePath}
              onChange={(e) => setSourcePath(e.target.value)}
              placeholder="可选:源码路径(仅用于按扩展名识别语言)"
            />
            <button className="run" onClick={run} disabled={busy || !input.trim()}>
              {busy ? '压缩中…' : '压缩'}
            </button>
          </div>
          <p className="note">源码、搜索结果、日志、Diff 和整行纯文本的省略处会写明 stash 绝对文件路径、省略行数和起始行。本地 Agent 可直接分片读取；演示站显示的是服务端路径，不能在你的本地读取。JSON 结构采样等未确认行映射的情况暂不标行号。</p>
          {error && <p className="error">{error}</p>}
          {result && !result.changed && (
            <p className="note">未发现值得压缩的内容，原样返回。可能是内容不足 512 字节、没有可折叠的重复块，或省略提示抵消了压缩收益。</p>
          )}
        </div>
      )}

      {stats && (
        <div className="stats">
          <div className="stat-row">
            <span className="stat-label">原始输入</span>
            <div className="stat-bar-track">
              <div className="stat-bar clay" style={{ width: '100%' }} />
            </div>
            <span className="stat-num clay">{bytes(stats.oB)}</span>
          </div>
          <div className="stat-row">
            <span className="stat-label">压缩输出</span>
            <div className="stat-bar-track">
              <div
                className="stat-bar green"
                style={{ width: `${Math.max(1.5, (1 - stats.ratio) * 100)}%` }}
              />
            </div>
            <span className="stat-num green">{bytes(stats.cB)}</span>
          </div>
          <div className="stat-meta">
            <span>
              体积 <b className="big">−{(stats.ratio * 100).toFixed(0)}%</b>
            </span>
            <span>
              节省 <b className="big">{stats.tokensSaved}</b> token
            </span>
            <span>
              类型 <b>{stats.detected}</b>
            </span>
            <span className={stats.lossy ? 'badge lossy' : 'badge'}>
              {stats.lossy ? '有损 · 可恢复' : '无损'}
            </span>
          </div>
        </div>
      )}

      {stats && (
        <div className="panes">
          <div className="pane">
            <div className="pane-head">
              <span className="pane-title"><span className="dot" />原始输入</span>
              <CopyButton text={orig} />
            </div>
            <pre>{orig}</pre>
          </div>
          <div className="pane compressed">
            <div className="pane-head">
              <span className="pane-title"><span className="dot" />压缩输出</span>
              {stats.stashKey && <code className="ccr">«stash:{stats.stashKey.slice(0, 12)}…»</code>}
              <CopyButton text={comp} />
            </div>
            <pre>{highlightCompressed(comp)}</pre>
          </div>
        </div>
      )}
    </section>
  );
}
