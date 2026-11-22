import { useEffect, useState } from 'react';
import { call } from '../lib/api';
import type { ScanStatus } from '../lib/types';

/** How often a running or queued album re-reads its scan status. */
export const SCAN_STATUS_POLL_MS = 1500;

/** A read of `scan_status`, stamped so a live event can outrank it. */
export type ScanHydration = { status: ScanStatus; at: number };

/**
 * The album's `scan_status` read on entry and job change, so the Scan screen can draw a running scan from its
 * snapshot, a queued one from its place in line, and a finished one from the album record. Polls every 1.5 s only
 * while the engine calls the album active or it is queued; a terminal answer stops the timer.
 */
export function useScanStatus(jobId: number | null, activeJob: number | null): ScanHydration | null {
  const [hydrated, setHydrated] = useState<ScanHydration | null>(null);
  useEffect(() => {
    if (jobId == null) {
      setHydrated(null);
      return;
    }
    let alive = true;
    let timer: number | undefined;
    let queued = false;
    let active = activeJob === jobId;
    const schedule = () => {
      if (alive && (active || queued)) timer = window.setTimeout(read, SCAN_STATUS_POLL_MS);
    };
    const read = async () => {
      timer = undefined;
      let next: ScanStatus | null = null;
      try {
        next = await call<ScanStatus>('scan_status', { jobId });
      } catch {
        // Keep the last answer; the album record still carries a summary.
      }
      if (!alive) return;
      if (next) {
        setHydrated({ status: next, at: Date.now() });
        queued = next.state === 'queued';
        active = next.state === 'running';
      }
      schedule();
    };
    setHydrated(null);
    void read();
    return () => {
      alive = false;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [jobId, activeJob]);
  return hydrated;
}
