// The Scan screen's whole state, decided from three sources: the live event
// stream, the `scan_status` read the screen makes on entry, and the album
// record. Pure, so node --test can pin the precedence without a browser.
//
// The rule: a live snapshot is trusted only for the album the engine calls
// active, and only when it wins the ordering guard over the hydration read
// (otherwise an older read would be rendered even though a newer event is in
// hand). The guard lives in snapshotOrder.mjs, shared by every run stream.
import { orderWins } from './snapshotOrder.mjs';

const TERMINAL = new Set(['done', 'done_errors', 'stopped', 'error']);

/** The scan state an album's stored status implies when nothing live answers. */
export function stateFromJob(job, progress = null) {
  if (progress?.state && progress.state !== 'running') return progress.state;
  if (!job) return 'idle';
  if (job.status === 'done_errors') return 'done_errors';
  if (job.status === 'error') return 'error';
  if (job.status === 'queued') return 'queued';
  if (job.status === 'done') return 'done';
  // `scanning` without the engine's active slot, `stopped`, `incomplete` and
  // an album only indexed are all work that stopped where it was.
  return 'stopped';
}

/**
 * `hydrate` is `{ status, at }` from `useScanStatus`; `live` is `scanLive[job.id]`.
 * Returns the state, the snapshot to draw (null when none), and the album's
 * counts for the summary card.
 */
export function scanView({ job = null, live = null, hydrate = null, activeJob = null } = {}) {
  // A hydration for a DIFFERENT album is ignored (audit P2-7: album switch must
  // not briefly draw the previous album's counts).
  const hydrateMine = hydrate && (hydrate.jobId == null || hydrate.jobId === job?.id) ? hydrate : null;
  const reported = hydrateMine?.status ?? null;
  const reportedAt = hydrateMine?.at ?? 0;
  const liveProgress = live?.progress ?? null;
  // Live event vs a slower `scan_status` read: the shared ordering guard. A live
  // event stamped in the SAME millisecond as the read now wins (was a `>` that
  // dropped it), and a slow poll can never outrank a newer pushed event.
  const liveStamp = liveProgress ? { at: live?.at ?? 0, source: 'live' } : null;
  const reportStamp = reported ? { at: reportedAt, source: 'hydrate' } : null;
  const liveWins = Boolean(liveProgress) && job != null && activeJob != null
    && job.id === activeJob && orderWins(reportStamp, liveStamp);
  const progress = liveWins ? liveProgress : reported?.progress ?? null;
  const source = liveWins ? 'live' : reported?.progress ? 'status' : 'job';
  let state;
  // A reported terminal state wins even while the engine still lingers the
  // active slot (audit P3-6): a reported error/done is never masked as running.
  if (reported?.state && TERMINAL.has(reported.state) && (activeJob === job?.id || activeJob == null)) state = reported.state;
  else if (job != null && activeJob === job.id) state = 'running';
  else if (reported?.state && reported.state !== 'running') state = reported.state;
  else state = stateFromJob(job, progress);
  // A `running` snapshot that is no longer the active album is a run that
  // ended (or a crash): the screen offers the resume, not a frozen bar.
  if (state === 'running' && (job == null || activeJob !== job.id)) state = 'stopped';
  return {
    source,
    state,
    progress,
    queuePosition: reported?.queue_position ?? null,
    errors: reported?.errors ?? job?.errors ?? 0,
    errorSamples: reported?.error_samples ?? job?.error_samples ?? [],
    resumeStage: reported?.resume_stage ?? null,
    paused: Boolean(reported?.paused),
    terminal: TERMINAL.has(state),
  };
}
