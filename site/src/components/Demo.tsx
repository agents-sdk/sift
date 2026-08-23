// 首页压缩演示岛:类型选择 → 左右对比;自定义粘贴走 /api/compress。
// 统计区用"比例条"呈现:陶土红 = 原始体量,工程绿 = 压缩后。
import { useMemo, useState } from 'react';
import samples from '../data/samples.json';

interface Sample {
  type: string;
  label: string;
  desc: string;
  query: string;
  detected: string;
  changed: boolean;
  lossy: boolean;
  ccrKey: string | null;
  tokensSaved: number;
  original: string;
  compressed: string;
}

interface ApiResult {
  detected: string;
  changed: boolean;
  lossy: boolean;
  ccrKey: string | null;
  tokensSaved: number;
  text: string;
}

type Mode = 'builtin' | 'custom';

function bytes(n: number): string {
  return n >= 1024 ? `${(n / 1024).toFixed(1)} KB` : `${n} B`;
}

export default function Demo() {
  const list = samples as Sample[];
  const [mode, setMode] = useState<Mode>('builtin');
  const [idx, setIdx] = useState(0);

  // 自定义输入状态
  const [input, setInput] = useState('');
  const [query, setQuery] = useState('');
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
      ccrKey: mode === 'builtin' ? sample.ccrKey : result?.ccrKey,
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
        body: JSON.stringify({ text: input, query }),
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

      {mode === 'custom' && (
        <div className="custom-input">
          <textarea
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="粘贴任意内容:JSON 数组、构建日志、grep 结果、git diff、普通文本……(检测自动分发)"
            rows={7}
          />
          <div className="custom-actions">
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="可选:相关性 query(帮助保留关键行)"
            />
            <button className="run" onClick={run} disabled={busy || !input.trim()}>
              {busy ? '压缩中…' : '压缩'}
            </button>
          </div>
          {error && <p className="error">{error}</p>}
          {result && !result.changed && (
            <p className="note">内容太短或无需压缩(单 block 最小 512 字节),原样返回。</p>
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
            <div className="pane-head">原始输入</div>
            <pre>{orig}</pre>
          </div>
          <div className="pane compressed">
            <div className="pane-head">
              压缩输出
              {stats.ccrKey && <code className="ccr">«ccr:{stats.ccrKey.slice(0, 12)}…»</code>}
            </div>
            <pre>{comp}</pre>
          </div>
        </div>
      )}
    </section>
  );
}
