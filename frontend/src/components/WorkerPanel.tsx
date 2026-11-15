import { useEffect, useState } from 'react';
import { engine } from '../lib/api';
import { clampWorkers, WORKER_POOLS } from '../lib/scan.mjs';
import type { Settings, WorkerLimits } from '../lib/types';
import type { Runner } from '../state/hooks';

/** Worker counts per pool, within the engine's own limits. Saved at once; a pool is sized when a scan starts. */
export function WorkerPanel({ run, onSaved, scanning }: { run: Runner; onSaved: (s: Settings) => void; scanning: boolean }) {
  const [limits, setLimits] = useState<WorkerLimits | null>(null);
  useEffect(() => { let live = true; void run(async () => { const l = await engine.workerLimits(); if (live) setLimits(l); }); return () => { live = false; }; }, [run]);
  if (!limits) return null;
  const set = (key: string, pool: (typeof WORKER_POOLS)[number]['id'], value: number) => {
    const limit = limits[pool];
    const next = clampWorkers(value, limit);
    if (next === limit.current) return;
    setLimits({ ...limits, [pool]: { ...limit, current: next } });
    void run(async () => { onSaved(await engine.saveSettings({ [key]: next })); });
  };
  const later = WORKER_POOLS.some((p) => !limits[p.id].live);
  return (
    <div className="rail-card worker-panel">
      <h4>Workers</h4>
      <p className="muted">{later ? (scanning ? 'Applies next scan. This one keeps the pools it started with.' : 'Applies to the next scan.') : 'Changes apply right away.'} {limits.cores} cores.</p>
      <ul>
        {WORKER_POOLS.map((p) => {
          const l = limits[p.id];
          return (
            <li key={p.id}>
              <span className="wp-name"><b>{p.label}</b><small className="muted">{p.hint}</small></span>
              <span className="wp-step">
                <button className="ghost small" aria-label={`Fewer ${p.label.toLowerCase()} workers`} disabled={l.current <= l.min} onClick={() => set(p.key, p.id, l.current - 1)}>−</button>
                <span className="mono wp-n" aria-live="polite" title={`${l.min}–${l.max}, default ${l.default}`}>{l.current}</span>
                <button className="ghost small" aria-label={`More ${p.label.toLowerCase()} workers`} disabled={l.current >= l.max} onClick={() => set(p.key, p.id, l.current + 1)}>+</button>
              </span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
