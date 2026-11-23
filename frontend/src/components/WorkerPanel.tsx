import { useEffect, useState } from 'react';
import { engine } from '../lib/api';
import { clampWorkers, WORKER_POOLS } from '../lib/scan.mjs';
import type { Settings, WorkerLimits } from '../lib/types';
import type { Runner } from '../state/hooks';

type PoolId = 'cull' | 'detect' | 'preview' | 'identify';

export type LocalConcurrency = {
  visible?: boolean;
  seats?: {
    decision: number;
    vision: number;
    decisionKey: string;
    visionKey: string;
  };
};

/** Worker counts per pool, within the engine's own limits. Saved at once; a pool is sized when a scan starts. */
export function WorkerPanel({
  run,
  onSaved,
  scanning,
  concurrency,
}: {
  run: Runner;
  onSaved: (s: Settings) => void;
  scanning: boolean;
  concurrency?: LocalConcurrency | null;
}) {
  const [limits, setLimits] = useState<WorkerLimits | null>(null);
  const [seats, setSeats] = useState<{ decision: number; vision: number }>({
    decision: concurrency?.seats?.decision ?? 1,
    vision: concurrency?.seats?.vision ?? 2,
  });

  useEffect(() => {
    let live = true;
    void run(async () => {
      const l = await engine.workerLimits();
      if (live) setLimits(l);
    });
    return () => { live = false; };
  }, [run]);

  const seatDecision = concurrency?.seats?.decision;
  const seatVision = concurrency?.seats?.vision;
  useEffect(() => {
    if (seatDecision == null && seatVision == null) return;
    setSeats((prev) => ({
      decision: seatDecision ?? prev.decision,
      vision: seatVision ?? prev.vision,
    }));
  }, [seatDecision, seatVision]);

  if (!limits) return null;

  const set = (key: string, pool: PoolId, value: number) => {
    const limit = limits[pool];
    const next = clampWorkers(value, limit);
    if (next === limit.current) return;
    setLimits({ ...limits, [pool]: { ...limit, current: next } });
    void run(async () => { onSaved(await engine.saveSettings({ [key]: next })); });
  };

  const maxIdentify = Math.max(1, limits.identify.max);
  const seatLimit = (current: number, fallback: number) => ({
    ...limits.identify,
    min: 1,
    max: maxIdentify,
    current,
    default: fallback,
  });
  const setSeat = (field: 'decision' | 'vision', key: string, value: number) => {
    const fallback = field === 'decision' ? 1 : 2;
    const cur = clampWorkers(seats[field], seatLimit(seats[field], fallback));
    const next = clampWorkers(value, seatLimit(cur, fallback));
    if (next === cur) return;
    setSeats((prev) => ({ ...prev, [field]: next }));
    void run(async () => { onSaved(await engine.saveSettings({ [key]: next })); });
  };

  const bothLocal = Boolean(concurrency?.visible && concurrency?.seats);
  const pools = bothLocal ? WORKER_POOLS.filter((p) => p.id !== 'identify') : WORKER_POOLS;
  const localRows = bothLocal
    ? [
        {
          id: 'decision' as const,
          key: concurrency?.seats?.decisionKey ?? 'identify_decision_workers',
          label: 'Decision',
          hint: 'Decision model queue',
          defaultCount: 1,
          current: clampWorkers(seats.decision, seatLimit(seats.decision, 1)),
        },
        {
          id: 'vision' as const,
          key: concurrency?.seats?.visionKey ?? 'identify_vision_workers',
          label: 'Vision',
          hint: 'Vision model workers',
          defaultCount: 2,
          current: clampWorkers(seats.vision, seatLimit(seats.vision, 2)),
        },
      ]
    : [];

  const later = WORKER_POOLS.some((p) => !limits[p.id as PoolId].live);
  return (
    <div className="rail-card worker-panel">
      <h4>Workers</h4>
      <p className="muted">{later ? (scanning ? 'Applies next scan. This one keeps the pools it started with.' : 'Applies to the next scan.') : 'Changes apply right away.'} {limits.cores} cores.</p>
      <ul>
        {pools.map((p) => {
          const poolId = p.id as PoolId;
          const l = limits[poolId];
          return (
            <li key={p.id}>
              <span className="wp-name"><b>{p.label}</b><small className="muted">{p.hint}</small></span>
              <span className="wp-step">
                <button className="ghost small" aria-label={`Fewer ${p.label.toLowerCase()} workers`} disabled={l.current <= l.min} onClick={() => set(p.key, poolId, l.current - 1)}>−</button>
                <span className="mono wp-n" aria-live="polite" title={`${l.min}–${l.max}, default ${l.default}`}>{l.current}</span>
                <button className="ghost small" aria-label={`More ${p.label.toLowerCase()} workers`} disabled={l.current >= l.max} onClick={() => set(p.key, poolId, l.current + 1)}>+</button>
              </span>
            </li>
          );
        })}
        {localRows.map((row) => (
          <li key={row.id}>
            <span className="wp-name"><b>{row.label}</b><small className="muted">{row.hint}</small></span>
            <span className="wp-step">
              <button className="ghost small" aria-label={`Fewer ${row.label.toLowerCase()} workers`} disabled={row.current <= 1} onClick={() => setSeat(row.id, row.key, row.current - 1)}>−</button>
              <span className="mono wp-n" aria-live="polite" title={`1–${maxIdentify}, default ${row.defaultCount}`}>{row.current}</span>
              <button className="ghost small" aria-label={`More ${row.label.toLowerCase()} workers`} disabled={row.current >= maxIdentify} onClick={() => setSeat(row.id, row.key, row.current + 1)}>+</button>
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
