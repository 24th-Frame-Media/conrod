import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { mocked } from '../lib/api';
import { clock, orderWins } from '../lib/snapshotOrder.mjs';
import { etaStable, overallEta, pushEtaSample, type EtaSample } from '../lib/scan.mjs';
import type { ScanProgress } from '../lib/types';

/** The newest `scan-progress` snapshot of an album, and whether its ETA has settled enough to print. */
export type ScanLive = { progress: ScanProgress; stable: boolean; history: EtaSample[]; at: number };

/** Albums kept in memory at once: the running one and the most recent summaries. */
export const LIVE_LIMIT = 8;

/**
 * Every album's latest scan snapshot, keyed by job id. Finished snapshots stay so the summary card can be read after the scan,
 * but only the newest `LIVE_LIMIT` albums: the oldest finished one is evicted first, so a day of albums does not pile up.
 *
 * `at` comes from the ONE shared monotonic clock (so it is comparable to the
 * scan_status hydration stamp), and an older read can never replace a newer
 * event (audit 4-fe-state P2-6). The active album is never evicted (P3-1).
 */
export function useScanProgress(activeJob: number | null = null): Record<number, ScanLive> {
  const [live, setLive] = useState<Record<number, ScanLive>>({});
  useEffect(() => {
    let alive = true;
    let off: (() => void) | undefined;
    const apply = (p: ScanProgress, at = clock.now()) => {
      if (!alive) return;
      setLive((all) => {
        const current = all[p.job_id] ? { at: all[p.job_id].at, source: 'live' } : null;
        const incoming = { at, source: 'live' };
        // A slow/older read never replaces a newer event for the same album.
        if (!orderWins(current, incoming)) return all;
        const history = p.state === 'running' ? pushEtaSample(all[p.job_id]?.history, at, overallEta(p.pipeline.stages), p.done, p.total) : [];
        const next: Record<number, ScanLive> = { ...all, [p.job_id]: { progress: p, history, stable: p.state === 'running' && etaStable(history), at } };
        const ids = Object.keys(next).map(Number);
        if (ids.length > LIVE_LIMIT) {
          // Never evict the album the engine is actively scanning (P3-1).
          const evictable = ids.filter((id) => id !== activeJob);
          const finished = evictable.filter((id) => next[id].progress.state !== 'running');
          const pool = finished.length ? finished : evictable;
          const oldest = (pool.length ? pool : ids).sort((a, b) => next[a].at - next[b].at)[0];
          delete next[oldest];
        }
        return next;
      });
    };
    if (mocked) {
      void import('../lib/mock').then((m) => { if (alive) off = m.subscribeScanProgress(apply); });
    } else {
      listen<ScanProgress>('scan-progress', (e) => apply(e.payload)).then((u) => { if (alive) off = u; else u(); }).catch(() => undefined);
    }
    return () => { alive = false; off?.(); };
  }, [activeJob]);
  return live;
}
