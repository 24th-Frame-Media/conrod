import { useEffect, useRef, useState } from 'react';
import { call } from '../lib/api';
import { clock, orderWins } from '../lib/snapshotOrder.mjs';
import type { ScanStatus } from '../lib/types';

/** How often a running or queued album re-reads its scan status. */
export const SCAN_STATUS_POLL_MS = 1500;

/**
 * A read of `scan_status`, stamped so a live event can outrank it. `at` is the
 * REQUEST time (from the shared monotonic clock), never the resolve time — a
 * slow read must not beat a newer pushed event (audit 4-fe-state P2-6). `jobId`
 * keys the answer so a switch of album never briefly draws the old one (P2-7).
 */
export type ScanHydration = { status: ScanStatus; at: number; jobId?: number | null };

/**
 * The album's `scan_status` read on entry and job change, so the Scan screen can draw a running scan from its
 * snapshot, a queued one from its place in line, and a finished one from the album record. Polls every 1.5 s only
 * while the engine calls the album active or it is queued; a terminal answer stops the timer.
 *
 * The last hydration is KEPT while only `activeJob` changes (no flash of "stopped / Resume"), and every
 * answer is ordering-guarded and keyed by job id.
 */
export function useScanStatus(jobId: number | null, activeJob: number | null): ScanHydration | null {
  const [hydrated, setHydrated] = useState<ScanHydration | null>(null);
  const appliedRef = useRef<{ at: number; source: string; jobId: number | null } | null>(null);
  useEffect(() => {
    if (jobId == null) {
      setHydrated(null);
      appliedRef.current = null;
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
      // Request time, BEFORE the await. A newer live event that arrives during
      // this flight has a later stamp and wins; a backward clock step cannot
      // freeze the answer because the clock is monotonic.
      const at = clock.now();
      let next: ScanStatus | null = null;
      try {
        next = await call<ScanStatus>('scan_status', { jobId });
      } catch {
        // Keep the last answer; the album record still carries a summary.
      }
      if (!alive) return;
      if (next) {
        const stamp = { at, source: 'hydrate', jobId };
        if (orderWins(appliedRef.current, stamp)) {
          appliedRef.current = stamp;
          setHydrated({ status: next, at, jobId });
        }
        queued = next.state === 'queued';
        active = next.state === 'running';
      }
      schedule();
    };
    // Do NOT clear `hydrated` here: when only `activeJob` changed we keep the last
    // whole snapshot while re-reading, so a finished album never flashes "stopped".
    void read();
    return () => {
      alive = false;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [jobId, activeJob]);
  return hydrated;
}
