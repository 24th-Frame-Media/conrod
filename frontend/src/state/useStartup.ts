import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { call, mocked } from '../lib/api';
import { STARTUP_LIMIT_MS, mockStartup, newerStartup, normalizeStartup, startupSettled } from '../lib/startup.mjs';
import type { StartupStatus } from '../lib/types';

/** `?mock=1&splash=hold` freezes the mock mid-way (on the model check) so the splash can be screenshotted. */
const HOLD = import.meta.env.DEV && mocked && new URLSearchParams(location.search).get('splash') === 'hold';
const HOLD_AT_MS = 1000;

/**
 * Engine start-up for the splash. The shell pushes `startup-progress` as steps happen, but its
 * first steps run before the window can listen, so the current state is also read on mount
 * (`startup_status`) and re-read until ready. Never blocks: when the status cannot be read, or
 * start-up takes too long, `settled` turns true and the app opens anyway.
 */
export function useStartup(): { status: StartupStatus | null; settled: boolean } {
  const [status, setStatus] = useState<StartupStatus | null>(null);
  const [unavailable, setUnavailable] = useState(false);
  const [waitedMs, setWaitedMs] = useState(0);

  useEffect(() => {
    let live = true;
    const began = Date.now();
    const timeout = window.setTimeout(() => live && setWaitedMs(STARTUP_LIMIT_MS), STARTUP_LIMIT_MS);
    const take = (raw: unknown) => {
      const next = normalizeStartup(raw);
      if (live && next) setStatus((prev) => newerStartup(prev, next));
      return next;
    };
    if (import.meta.env.DEV && mocked) {
      const tick = () => { take(mockStartup(HOLD ? HOLD_AT_MS : Date.now() - began)); };
      tick();
      const timer = window.setInterval(tick, 80);
      return () => { live = false; window.clearTimeout(timeout); window.clearInterval(timer); };
    }
    let unlisten: (() => void) | undefined;
    listen<unknown>('startup-progress', (e) => { take(e.payload); })
      .then((off) => { if (live) unlisten = off; else off(); })
      .catch(() => undefined);
    let ready = false;
    const read = () => {
      call<unknown>('startup_status')
        .then((raw) => { const s = take(raw); if (s?.ready) ready = true; if (live && !s) setUnavailable(true); })
        .catch(() => { if (live) setUnavailable(true); });
    };
    read();
    const timer = window.setInterval(() => { if (ready) window.clearInterval(timer); else read(); }, 600);
    return () => { live = false; window.clearTimeout(timeout); window.clearInterval(timer); unlisten?.(); };
  }, []);

  return { status, settled: !HOLD && startupSettled({ status, unavailable, waitedMs }) };
}
