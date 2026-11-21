import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { mocked } from '../lib/api';
import type { DeciderLive } from '../lib/types';

/** The newest `decider-progress` snapshot (the engine sends one whenever the model's state or activity changes), or null before the first. */
export function useDeciderProgress(): DeciderLive | null {
  const [live, setLive] = useState<DeciderLive | null>(null);
  useEffect(() => {
    let alive = true;
    let off: (() => void) | undefined;
    const apply = (p: DeciderLive) => { if (alive && p && typeof p.state === 'string') setLive(p); };
    if (mocked) {
      void import('../lib/mock').then((m) => { if (alive) off = m.subscribeDeciderProgress(apply); });
    } else {
      listen<DeciderLive>('decider-progress', (e) => apply(e.payload)).then((u) => { if (alive) off = u; else u(); }).catch(() => undefined);
    }
    return () => { alive = false; off?.(); };
  }, []);
  return live;
}