import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { mocked } from '../lib/api';
import { etaStable, overallEta, pushEtaSample, type EtaSample } from '../lib/scan.mjs';
import type { ScanProgress } from '../lib/types';

/** The newest `scan-progress` snapshot of an album, and whether its ETA has settled enough to print. */
export type ScanLive = { progress: ScanProgress; stable: boolean; history: EtaSample[]; at: number };

/** Every album's latest scan snapshot, keyed by job id. Finished snapshots stay so the summary card can be read after the scan. */
export function useScanProgress(): Record<number, ScanLive> {
  const [live, setLive] = useState<Record<number, ScanLive>>({});
  useEffect(() => {
    let alive = true;
    let off: (() => void) | undefined;
    const apply = (p: ScanProgress) => {
      if (!alive) return;
      setLive((all) => {
        const now = Date.now();
        const history = p.state === 'running' ? pushEtaSample(all[p.job_id]?.history, now, overallEta(p.pipeline.stages), p.done, p.total) : [];
        return { ...all, [p.job_id]: { progress: p, history, stable: p.state === 'running' && etaStable(history), at: now } };
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
