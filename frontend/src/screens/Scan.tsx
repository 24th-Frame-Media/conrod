import type { ReactNode } from 'react';
import type { IdentifyLive, Job, RunSnapshot, Settings, Status } from '../lib/types';
import type { VisionAction } from '../lib/vision.mjs';
import { scanFailureFor, type ScanFailure } from '../lib/scanError.mjs';
import { scanSeedFor } from '../lib/runSnapshot.mjs';
import { scanView } from '../lib/scanStatus.mjs';
import { Icon } from '../components/basics';
import type { Runner } from '../state/hooks';
import type { ScanHydration } from '../state/useScanStatus';
import type { ScanLive } from '../state/useScanProgress';
import { IngestScreen, ScanFailureBanner } from './Ingest';

type Props = {
  job: Job | undefined; live: ScanLive | undefined; running: boolean; paused: boolean;
  /** The album's `scan_status` read, so a queued, stopped or finished album still has a page. */
  scanStatus?: ScanHydration | null;
  /** Albums waiting their turn, first to start first. */
  queue?: Job[];
  identify?: IdentifyLive; status: Status; model?: string; identifyPaused: boolean;
  identifyOrder?: readonly string[];
  /** The cloud decision service actually serving moments (e.g. "jev-1.13 via OpenCode"), or null. */
  decisionService?: string | null;
  run: Runner; onSettingsSaved: (s: Settings) => void;
  onPause: () => void; onResume: () => void; onCancel: () => void; onCancelOperation: (key: string) => void;
  onPauseIdentify?: () => void; onResumeIdentify?: () => void;
  onReview: () => void; onLibrary: () => void;
  onVisionAction?: (id: VisionAction['id']) => void;
  onUnqueue?: (job: Job) => void;
  /** Resume a stopped album from the stage it reached (`scan` with the album's id). */
  onResumeJob?: () => void;
  /** A scan call that threw before the album existed, shown by the same banner as a failed run. */
  scanError?: ScanFailure | null;
  /** Normalized run snapshot passed through to the live Ingest screen. */
  runSnapshot?: RunSnapshot;
  /** Retry the scan after the failure's cause is fixed. */
  onRetryScan?: () => void;
  /** Derived surface model from the run snapshot (flowchart sub-nodes,
   *  disabled steps, controls, VRAM warning, and type suggestion). */
  surface?: any;
  /** Per-stage controls: pause, resume, skip, and run again. */
  onStageControl?: (
    action: 'stage_pause' | 'stage_skip' | 'stage_rerun',
    stage: string,
    extra?: { paused?: boolean; skip?: boolean; full?: boolean },
  ) => void;
  /** Accept the engine's shoot-type suggestion (explicit user action). */
  onAcceptTypeSuggestion?: (type: string) => void;
  /** A greyed node's shortcut: open this album's Manage shoot. */
  onOpenShootSettings?: () => void;
};

/** A card for a scan that is not running right now: waiting its turn, stopped, failed or done. */
function ScanNotice({ tone, title, note, children }: { tone: 'queued' | 'stopped' | 'done'; title: string; note: string; children?: ReactNode }) {
  return (
    <div className="screen">
      <div className="identify-page ingest-page">
        <section className={`scan-summary ${tone}`} role="status" aria-label="Scan status">
          <div className="ss-mark"><Icon name={tone === 'done' ? 'check' : 'hourglass'} size={20} /></div>
          <div className="ss-text"><b>{title}</b><span className="muted">{note}</span></div>
          <div className="ss-actions">{children}</div>
        </section>
      </div>
    </div>
  );
}

function photos(count: number) {
  return `${count.toLocaleString()} ${count === 1 ? 'photo' : 'photos'}`;
}

/**
 * An album's worker screen at any moment. A running scan is the live Ingest page, fed by the snapshot when no event is
 * in hand; a queued, stopped, failed or finished album gets the same screen's summary with the one action that helps.
 */
export function ScanScreen({ job, live, scanStatus, status, paused, queue, onUnqueue, onResumeJob, onReview, onLibrary, scanError, runSnapshot, onRetryScan, ...rest }: Props) {
  const view = scanView({ job, live, hydrate: scanStatus, activeJob: status.activeJob });
  // Bind import errors strictly to this album so a null-job failure never
  // appears on an unrelated album.
  const seed = scanSeedFor(scanError ?? null, job?.id ?? null);

  if (job && seed) {
    return (
      <div className="screen"><div className="identify-page ingest-page">
        <ScanFailureBanner failure={seed} onRetry={onRetryScan ?? onResumeJob} onLibrary={onLibrary} />
      </div></div>
    );
  }
  if (job) {
    const done = view.progress?.done ?? job.done ?? 0;
    const total = view.progress?.total ?? job.total ?? 0;
    const left = Math.max(0, total - done);
    if (view.state === 'queued') {
      const place = view.queuePosition;
      return (
        <ScanNotice tone="queued" title={`${job.label} is waiting its turn`}
          note={place ? `Place ${place} in line. It starts as soon as the current scan finishes.` : 'It starts as soon as the current scan finishes.'}>
          {onUnqueue && <button className="ghost" onClick={() => onUnqueue(job)}>Remove from queue</button>}
          {onLibrary && <button className="ghost" onClick={onLibrary}>Library</button>}
        </ScanNotice>
      );
    }
    if (view.state === 'error') {
      const failure = scanFailureFor({ job, progress: view.progress, status });
      if (failure) return <div className="screen"><div className="identify-page ingest-page"><ScanFailureBanner failure={failure} onRetry={onRetryScan ?? onResumeJob} onLibrary={onLibrary} /></div></div>;
    }
    if (view.state === 'stopped') {
      const indexed = job.status === 'indexed';
      return (
        <ScanNotice tone="stopped" title={indexed ? `${job.label} is indexed` : `${job.label} stopped at ${total > 0 ? Math.round((done * 100) / total) : 0}%`}
          note={indexed ? 'Anything already measured is kept. Resume to suggest culls for this album.'
            : `${photos(left)} left to read. Anything already measured is kept; a resume starts where it stopped.`}>
          {(onResumeJob || onRetryScan) && <button className="primary" onClick={onResumeJob ?? onRetryScan}>Resume scan</button>}
          {onReview && <button className="ghost" onClick={onReview}>Review what is ready</button>}
          {onLibrary && <button className="ghost" onClick={onLibrary}>Library</button>}
        </ScanNotice>
      );
    }
    if (view.state === 'done' || view.state === 'done_errors') {
      const errors = view.errors;
      const note = errors > 0
        ? `${view.errorSamples[0] ?? 'Some files could not be read'}. The rest are ready to review; retry the failed frames from Review.`
        : 'Keepers are suggestions. Nothing is rejected or written until you say so.';
      return (
        <ScanNotice tone="done" title={`${job.label} — ${photos(total)} read${errors ? ` · ${photos(errors)} could not be read` : ''}`} note={note}>
          {errors > 0 && (onRetryScan ?? onResumeJob) && <button className="ghost" onClick={onRetryScan ?? onResumeJob}>Retry failed photos</button>}
          {onReview && <button className="primary" autoFocus onClick={onReview}>Review</button>}
          {onLibrary && <button className="ghost" onClick={onLibrary}>Library</button>}
        </ScanNotice>
      );
    }
  }

  // Running, or still asking: the live page. A snapshot read on entry draws the same screen as an event would.
  const snapshot = view.progress;
  const scanLive = view.source === 'live' && live ? live : snapshot ? { progress: snapshot, stable: false, history: [], at: scanStatus?.at ?? 0 } : undefined;
  return <IngestScreen {...rest} mode="ingest" scan={scanLive} job={job} status={status} paused={paused} running={view.state === 'running'}
    queue={queue} onUnqueue={onUnqueue} onReview={onReview} onLibrary={onLibrary} onRetryScan={onRetryScan} scanError={scanError ?? undefined} runSnapshot={runSnapshot} />;
}
