import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { mocked } from '../lib/api';
import type { IdentifyLive } from '../lib/types';

/**
 * Every album's newest `identify-progress` snapshot, keyed by job id. A finished snapshot stays
 * so its summary can be read after the run ends.
 */
export function useIdentifyProgress(): Record<number, IdentifyLive> {
  const [live, setLive] = useState<Record<number, IdentifyLive>>({});
  useEffect(() => {
    let alive = true;
    let off: (() => void) | undefined;
    const apply = (p: IdentifyLive) => {
      if (!alive || !p || typeof p.job_id !== 'number') return;
      setLive((all) => ({ ...all, [p.job_id]: p }));
    };
    if (mocked) {
      void import('../lib/mock').then((m) => { if (alive) off = m.subscribeIdentifyProgress(apply); });
    } else {
      listen<IdentifyLive>('identify-progress', (e) => apply(e.payload)).then((u) => { if (alive) off = u; else u(); }).catch(() => undefined);
    }
    return () => { alive = false; off?.(); };
  }, []);
  return live;
}
