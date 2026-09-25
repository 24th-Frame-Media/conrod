import { useEffect, useState } from 'react';
import { engine, asset, call } from '../lib/api';
import type { OllamaModel, Settings, Status, Toaster, VlmService } from '../lib/types';
import { usePreview, type Runner } from '../state/hooks';

type BenchItem = { imageId: number; path: string; thumb: string | null; sharp: boolean };
type Truth = { make?: string; model?: string; colour?: string; number?: string };
type ModelSpec = { service_id: string; model: string };
type Answer = { make?: string; model?: string; colour?: string; number?: string } | string;
type ResultRow = {
  serviceId: string; provider: string; model: string; score: number; sharpScore: number; blurryScore: number;
  avgSecs: number; errors: number; answers: Answer[];
};

const TRUTH_KEY = 'conrod.bench.truth';
const FIELDS: (keyof Truth)[] = ['make', 'model', 'colour', 'number'];

function getVlmServices(settings: Settings): VlmService[] {
  const v = settings.vlm_services;
  return Array.isArray(v) ? (v as unknown as VlmService[]) : [];
}

function loadTruth(): Record<string, Truth> {
  try { return JSON.parse(localStorage.getItem(TRUTH_KEY) || '{}'); } catch { return {}; }
}
function saveTruth(truth: Record<string, Truth>) {
  try { localStorage.setItem(TRUTH_KEY, JSON.stringify(truth)); } catch { /* not persisted */ }
}

function isAnswer(a: Answer): a is Exclude<Answer, string> {
  return typeof a !== 'string';
}

/** One enabled server's checkboxes: its installed vision models (Ollama) or its one configured model (cloud). */
function ServerModelGroup({ service, selected, onToggle }: {
  service: VlmService;
  selected: ModelSpec[];
  onToggle: (spec: ModelSpec) => void;
}) {
  const [models, setModels] = useState<string[]>(service.provider === 'ollama' ? [] : [service.model]);

  useEffect(() => {
    if (service.provider !== 'ollama') { setModels([service.model]); return; }
    let live = true;
    void engine.ollamaModels(service.host)
      .then((res) => { if (live) setModels(res.models.filter((m: OllamaModel) => m.vision).map((m) => m.name)); })
      .catch(() => { if (live) setModels([]); });
    return () => { live = false; };
  }, [service.provider, service.host, service.model]);

  return (
    <div className="bench-server-group">
      <b>{service.provider} · {service.host ? service.host.replace(/^https?:\/\//, '') : service.model}</b>
      <div className="steps" style={{ flexWrap: 'wrap' }}>
        {models.map((m) => (
          <label key={m} className="step" style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <input
              type="checkbox"
              checked={selected.some((sel) => sel.service_id === service.id && sel.model === m)}
              onChange={() => onToggle({ service_id: service.id, model: m })}
            />
            {m}
          </label>
        ))}
        {!models.length && <span className="muted">No vision models found on this server.</span>}
      </div>
    </div>
  );
}

type Props = { run: Runner; toast: Toaster; settings: Settings; status?: Status };

/** Pick a sharp-only sample, mark ground truth, and score vision models on it. */
export function Benchmark({ run, toast, settings, status }: Props) {
  const [items, setItems] = useState<BenchItem[]>([]);
  const [truth, setTruth] = useState<Record<string, Truth>>(() => loadTruth());
  const [selected, setSelected] = useState<ModelSpec[]>([]);
  const [picking, setPicking] = useState(false);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [rows, setRows] = useState<ResultRow[]>([]);
  const [expanded, setExpanded] = useState<number | null>(null);
  const [zoomId, setZoomId] = useState<number | null>(null);

  const enabledServices = getVlmServices(settings).filter((s) => s.enabled);

  const pick = () => void run(async () => {
    setPicking(true);
    try {
      const picked = await call<BenchItem[]>('bench_pick', null);
      setItems(picked);
    } finally { setPicking(false); }
  });

  // ponytail: reuses bench_pick and keeps one unused photo; a dedicated
  // single-pick command if that ever comes back empty too often.
  const repickOne = (index: number) => void run(async () => {
    const old = items[index];
    const fresh = (await call<BenchItem[]>('bench_pick', null))
      .find((c) => !items.some((it) => it.imageId === c.imageId));
    if (!fresh) { toast('No other photo found, try again', { tone: 'error' }); return; }
    setItems((prev) => prev.map((it, i) => (i === index ? fresh : it)));
  });

  // Seeds the blank truth fields from the model set in Settings; you still check them.
  const [asking, setAsking] = useState<string | null>(null);
  const askModel = (path: string) => void run(async () => {
    setAsking(path);
    try {
      const got = await call<Truth>('describe_image', { path });
      setTruth((prev) => {
        const cur = prev[path] || {};
        const filled = Object.fromEntries(FIELDS.map((f) => [f, cur[f]?.trim() ? cur[f] : got[f] || '']));
        const next = { ...prev, [path]: filled };
        saveTruth(next);
        return next;
      });
    } finally { setAsking(null); }
  });

  const setField = (path: string, field: keyof Truth, value: string) => {
    setTruth((prev) => {
      const next = { ...prev, [path]: { ...prev[path], [field]: value } };
      saveTruth(next);
      return next;
    });
  };

  const toggleModel = (spec: ModelSpec) => {
    setSelected((prev) => prev.some((m) => m.service_id === spec.service_id && m.model === spec.model)
      ? prev.filter((m) => !(m.service_id === spec.service_id && m.model === spec.model))
      : [...prev, spec]);
  };

  const hasTruth = items.some((it) => FIELDS.some((f) => (truth[it.path]?.[f] || '').trim()));
  const canRun = selected.length > 0 && hasTruth && items.length > 0 && !running;

  const runBench = () => void run(async () => {
    setRunning(true);
    setError(null);
    try {
      const bench = await call<ResultRow[]>('bench_run', {
        items: items.map((it) => ({ path: it.path, sharp: it.sharp, truth: truth[it.path] || {} })),
        models: selected,
      });
      setRows(bench);
      toast('Benchmark complete', { tone: 'ok' });
    } catch (e) {
      setError(String(e));
    } finally { setRunning(false); }
  });

  const zoomItem = items.find((it) => it.imageId === zoomId) || null;

  return (
    <div>
      <div className="bench-intro">
        <div className="new-scan-head">
          <div>
            <p>Pick a sample of sharp photos, mark the truth, then score vision models against it.</p>
          </div>
          <button className="primary" disabled={picking} onClick={pick}>{picking ? 'Picking…' : items.length ? 'Repick' : 'Pick images'}</button>
        </div>
      </div>

      {items.length > 0 && (
        <div className="bench-thumbs">
          {items.map((it, i) => (
            <div className="job-card" key={it.imageId} style={{ cursor: 'default' }}>
              <div
                className="bench-thumb-img"
                style={{ backgroundImage: `url("${asset(it.thumb)}")` }}
                onClick={() => setZoomId(it.imageId)}
              >
                <button className="iconbtn" title="Repick this photo" aria-label="Repick this photo" onClick={(e) => { e.stopPropagation(); repickOne(i); }}
                  style={{ position: 'absolute', top: 6, right: 6 }}>↻</button>
                <button className="iconbtn" title="Fill blanks from the current model" aria-label="Fill blanks from the current model"
                  disabled={asking !== null} onClick={(e) => { e.stopPropagation(); askModel(it.path); }}
                  style={{ position: 'absolute', top: 6, right: 44 }}>{asking === it.path ? '…' : '✦'}</button>
              </div>
              <div className="steps" style={{ flexDirection: 'column', alignItems: 'stretch', gap: 4, marginTop: 8 }}>
                {FIELDS.map((f) => (
                  <input key={f} placeholder={f} value={truth[it.path]?.[f] || ''} onChange={(e) => setField(it.path, f, e.target.value)} />
                ))}
              </div>
            </div>
          ))}
        </div>
      )}

      {items.length > 0 && (
        <div className="bench-intro">
          <h3>Models</h3>
          {enabledServices.map((s) => (
            <ServerModelGroup key={s.id} service={s} selected={selected} onToggle={toggleModel} />
          ))}
          {!enabledServices.length && <span className="muted">No vision providers enabled above.</span>}
        </div>
      )}

      {items.length > 0 && (
        <div className="actions-row">
        <button className="primary" disabled={!canRun} onClick={runBench}>{running ? 'Running…' : 'Run benchmark'}</button>
        {running && (() => {
          const task = status?.tasks.find((t) => t.label === 'Benchmarking models' && t.state === 'running');
          return (
            <div className="bench-progress" role="status">
              <progress max={task?.total || 1} value={task?.done ?? 0} />
              <span className="muted small">{task?.detail || 'Starting…'}</span>
            </div>
          );
        })()}
        </div>
      )}
      {error && <p className="muted" style={{ color: 'var(--danger, #e0605a)' }}>{error}</p>}

      {rows.length > 0 && (
        <table className="mono" style={{ width: '100%', marginTop: 22 }}>
          <thead>
            <tr><th>#</th><th>Provider/model</th><th>Score</th><th>s/image</th><th>Errors</th></tr>
          </thead>
          <tbody>
            {rows.map((r, i) => (
              <>
                <tr key={`${r.serviceId}:${r.model}`} onClick={() => setExpanded(expanded === i ? null : i)} style={{ cursor: 'pointer' }}>
                  <td>{i + 1}</td>
                  <td>{r.provider}/{r.model}</td>
                  <td>{Math.round(r.score * 100)}%</td>
                  <td>{r.avgSecs.toFixed(1)}</td>
                  <td>{r.errors}</td>
                </tr>
                {expanded === i && (
                  <tr key={`${r.serviceId}:${r.model}-detail`}>
                    <td colSpan={5}>
                      <table className="mono" style={{ width: '100%' }}>
                        <thead><tr><th>Image</th>{FIELDS.map((f) => <th key={f}>{f}</th>)}</tr></thead>
                        <tbody>
                          {items.map((it, idx) => {
                            const ans = r.answers[idx];
                            const t = truth[it.path] || {};
                            return (
                              <tr key={it.imageId}>
                                <td>{it.path.split(/[\\/]/).pop()}</td>
                                {FIELDS.map((f) => {
                                  const got = isAnswer(ans) ? (ans[f] || '') : `error: ${ans}`;
                                  const mismatch = isAnswer(ans) && (t[f] || '').trim() && got.trim().toLowerCase() !== (t[f] || '').trim().toLowerCase();
                                  return <td key={f} style={mismatch ? { color: 'var(--danger, #e0605a)' } : undefined}>{got || '—'}</td>;
                                })}
                              </tr>
                            );
                          })}
                        </tbody>
                      </table>
                    </td>
                  </tr>
                )}
              </>
            ))}
          </tbody>
        </table>
      )}

      {zoomItem && <BenchZoom item={zoomItem} run={run} onClose={() => setZoomId(null)} />}
    </div>
  );
}

/** Full-size overlay for one bench thumbnail; reuses the cached preview jpg by image id. Esc or a click closes it. */
function BenchZoom({ item, run, onClose }: { item: BenchItem; run: Runner; onClose: () => void }) {
  const preview = usePreview(item.imageId, run);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);
  return (
    <div className="bench-zoom-overlay" onClick={onClose}>
      <img src={asset(preview) ?? asset(item.thumb)} alt="" onClick={(e) => e.stopPropagation()} />
    </div>
  );
}
