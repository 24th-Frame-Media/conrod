import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { mocked } from '../lib/api';
import { etaStable, overallEta, pushEtaSample, type EtaSample } from '../lib/scan.mjs';
import type { ScanProgress } from '../lib/types';

/** The newest `scan-progress` snapshot of an album, and whether its ETA has settled enough to print. */
export type ScanLive = { progress: ScanProgress; stable: boolean; history: EtaSample[]; at: number };

/** Albums kept in memory at once: the running one and the most recent summaries. */
export const LIVE_LIMIT = 8;

/** 
 * Every album's latest scan snapshot, keyed by job id. Finished snapshots stay so the summary card can be read after the scan,
 * but only the newest `LIVE_LIMIT` albums: the oldest finished one is evicted first, so a day of albums does not pile up.
 */
export function useScanProgress(): Record<number, ScanLive> {
  const [live, setLive] = useState<Record<number, ScanLive>>({});
  useEffect(() => {
    let alive = true;
    let off: (() => void) | undefined;
    const apply = (p: ScanProgress, at = Date.now()) => {
      if (!alive) return;
      setLive((all) => {
        // An older read never replaces a newer event for the same album.
        if (all[p.job_id] && at < all[p.job_id].at) return all;
        const history = p.state === 'running' ? pushEtaSample(all[p.job_id]?.history, at, overallEta(p.pipeline.stages), p.done, p.total) : [];
        const next: Record<number, ScanLive> = { ...all, [p.job_id]: { progress: p, history, stable: p.state === 'running' && etaStable(history), at } };
        const ids = Object.keys(next).map(Number);
        if (ids.length > LIVE_LIMIT) {
          const finished = ids.filter((id) => next[id].progress.state !== 'running');
          const oldest = (finished.length ? finished : ids).sort((a, b) => next[a].at - next[b].at)[0];
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
  }, []);
  return live;
}
