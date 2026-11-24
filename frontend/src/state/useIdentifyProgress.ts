import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { mocked } from '../lib/api';
import { rememberProgress } from '../lib/progressCache.mjs';
import type { IdentifyLive } from '../lib/types';

type Cache = { all: Record<number, IdentifyLive>; order: number[] };

/**
 * The newest `identify-progress` snapshot of the albums touched lately, keyed by job id. A finished
 * snapshot stays so its summary can be read after the run ends, but only the last few albums are
 * kept (finished and oldest go first), so a long session does not grow without bound.
 */
export function useIdentifyProgress(): Record<number, IdentifyLive> {
  const [cache, setCache] = useState<Cache>({ all: {}, order: [] });
  useEffect(() => {
    let alive = true;
    let off: (() => void) | undefined;
    const apply = (p: IdentifyLive) => {
      if (!alive || !p || typeof p.job_id !== 'number') return;
      setCache((c) => rememberProgress(c.all, c.order, p));
    };
    if (mocked) {
      void import('../lib/mock').then((m) => { if (alive) off = m.subscribeIdentifyProgress(apply); });
    } else {
      listen<IdentifyLive>('identify-progress', (e) => apply(e.payload)).then((u) => { if (alive) off = u; else u(); }).catch(() => undefined);
    }
    return () => { alive = false; off?.(); };
  }, []);
  return cache.all;
}
