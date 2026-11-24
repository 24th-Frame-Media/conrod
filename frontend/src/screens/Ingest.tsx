import { useEffect, useRef, useState } from 'react';
import { IdentifyDetail, IdentifyLane, hash } from '../components/IdentifyParts';
import { FrameDetail, ScanLane } from '../components/IngestLane';
import { FlowNode } from '../components/FlowNode';
import { WorkerPanel } from '../components/WorkerPanel';
import { DeciderLane } from '../components/DeciderLane';
import { deciderActive } from '../lib/decider.mjs';
import { useDeciderProgress } from '../state/useDeciderProgress';
import { Empty, Icon } from '../components/basics';
import { asset } from '../lib/api';
import { albumSetting } from '../lib/album.mjs';
import { applyOverrides, defaultSummary, overridesOf } from '../lib/presetPolicy.mjs';
import { readsSubjects, subjectWords } from '../lib/subjects.mjs';
import {
  confidencePercent, confidenceTone, displayItem, elapsed, phaseLabel, progressPercent, rateLabel, visibleWorkerLanes, workerGridMode, workerLanes,
  type WorkerDensity,
} from '../lib/identifyFlow.mjs';
import {
  etaLines, frameRate, identifyEta, identifyNodes, ingestFraction, ingestIdentify, ingestNodes, ingestPhase, laneKey, laneState, leftLabel,
  phaseRunning, phaseTone, phaseWord, stagePlace, starsText, stripItems, verdictText, visionExpected,
} from '../lib/ingest.mjs';
import { activeWorkers, scanSeedFor } from '../lib/runSnapshot.mjs';
import { visionHeader, visionModelWords, visionOffReason, type VisionAction } from '../lib/vision.mjs';
import { etaLabel, etaRemaining, formatRate, orderStages, overallEta, overallFraction, summaryLine } from '../lib/scan.mjs';
import type { IdentifyLive, Job, RunSnapshot, ScanFound, ScanRecentFrame, Settings, Status } from '../lib/types';
import { scanFailureFor, type ScanFailure } from '../lib/scanError.mjs';
import type { Runner } from '../state/hooks';
import type { ScanLive } from '../state/useScanProgress';

export type IngestProps = {
  /** `ingest` is a scan and everything after it; `identify` is a standalone identification run (only the vision stages). */
  mode: 'ingest' | 'identify';
  job: Job | undefined;
  scan?: ScanLive;
  identify?: IdentifyLive;
  status: Status;
  /** The vision model this album would call, for the VISION node's subline. */
  model?: string;
  /** The cloud decision service actually serving moments (e.g. "jev-1.13 via OpenCode"), or null
   *  for the local path. Names the serving service on the DECIDER node and lane. */
  decisionService?: string | null;
  /** The Vision off card's button: turn vision on for this album, or open Settings to set up a service. */
  onVisionAction?: (id: VisionAction['id']) => void;
  /** The album's scan is running right now (the engine says so; the last snapshot can lag). */
  running?: boolean;
  /** Albums waiting their turn, first to start first. */
  queue?: Job[];
  paused: boolean;
  identifyPaused: boolean;
  run?: Runner;
  onSettingsSaved?: (s: Settings) => void;
  onPause?: () => void;
  onResume?: () => void;
  onCancel?: () => void;
  onCancelOperation: (key: string) => void;
  onReview: () => void;
  onLibrary: () => void;
  /** Take a waiting album out of the queue; nothing already imported is lost. */
  onUnqueue?: (job: Job) => void;
  /** A scan that failed before or during the run; shown as a persistent banner with Resume. */
  scanError?: ScanFailure | null;
  /** Normalized run snapshot: stages, workers, counters, and failed stages. */
  runSnapshot?: RunSnapshot;
  /** Retry the failed scan; the banner's Resume. */
  onRetryScan?: () => void;
  /** A stopped or failed identification: continue it where it stopped, asking only
   *  the frames nobody decided and the subjects nobody read. Never fired by itself. */
  onContinueIdentify?: () => void;
  /** Derived surface model from the run snapshot (flowchart sub-nodes,
   *  disabled steps, controls, VRAM warning, and type suggestion). */
  surface?: any;
  /** Per-stage controls matching the engine wire command shape. */
  onStageControl?: (
    action: 'stage_pause' | 'stage_skip' | 'stage_rerun',
    stage: string,
    extra?: { paused?: boolean; skip?: boolean; full?: boolean },
  ) => void;
  /** Accept the engine's shoot-type suggestion (explicit user action). */
  onAcceptTypeSuggestion?: (type: string) => void;
  /** Shortcut from a disabled node to open this album's shoot settings. */
  onOpenShootSettings?: () => void;
};

type Pick = { kind: 'car' | 'frame'; id: number };

const FOUND: [keyof ScanFound, string][] = [
  ['subjects', 'Subjects'], ['soft', 'Soft'], ['closed_eyes', 'Closed eyes'], ['numbers', 'Numbers'], ['plates', 'Plates'], ['bursts', 'Bursts'], ['keepers', 'Keepers'],
];

function Found({ found, identify }: { found: ScanFound; identify: boolean }) {
  return (
    <div className="rail-card found-card">
      <h4>Found so far</h4>
      <dl>
        {FOUND.filter(([k]) => identify || (k !== 'numbers' && k !== 'plates')).map(([k, label]) => (
          <div key={k} className={found[k] ? '' : 'zero'}><dt>{label}</dt><dd className="mono">{(found[k] ?? 0).toLocaleString()}</dd></div>
        ))}
      </dl>
    </div>
  );
}

export function ScanFailureBanner({ failure, onRetry, onLibrary }: { failure: ScanFailure; onRetry?: () => void; onLibrary?: () => void }) {
  return (
    <section className="scan-summary scan-failure" role="alert">
      <div className="ss-mark"><Icon name="x" size={20} /></div>
      <div className="ss-text">
        <b>{failure.title}{failure.label ? ` — ${failure.label}` : ''}</b>
        <span className="muted" title={failure.detail || undefined}>{failure.message}</span>
      </div>
      <div className="ss-actions">
        {onRetry && <button className="primary" onClick={onRetry}>Resume</button>}
        {onLibrary && <button className="ghost" onClick={onLibrary}>Library</button>}
      </div>
    </section>
  );
}

/**
 * One live page for an album's whole ingest: one bar, one flowchart (read to pick, then vision), a grid of every worker that is
 * doing something (scan lanes with boxes and autofocus, vision lanes with the model's question and reply), and one strip of what
 * just finished. The same screen shows a standalone identification run with only the vision stages.
 */
export function IngestScreen({
  mode, job, scan, identify, status, model, decisionService = null, running = false, paused, identifyPaused, run, onSettingsSaved, onPause, onResume, onCancel,
  onCancelOperation, onVisionAction, onReview, onLibrary, queue = [], onUnqueue, scanError, runSnapshot, onRetryScan, onContinueIdentify,
  surface, onStageControl, onAcceptTypeSuggestion, onOpenShootSettings,
}: IngestProps) {
  const [picked, setPicked] = useState<Pick | null>(null);
  const [filter, setFilter] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);
  const [density, setDensity] = useState<WorkerDensity>('auto');
  const [overflow, setOverflow] = useState({ hasMore: false, hiddenBelow: 0 });
  const workersRef = useRef<HTMLElement | null>(null);
  const decider = useDeciderProgress();
  // Prompts only ride on the cars being read and on the snapshot's active list, not on the
  // finished strip; keep the last one per car so a finished car can still show what was asked.
  const prompts = useRef<Record<number, string>>({});
  const p = mode === 'ingest' ? scan?.progress : undefined;
  const operation = job ? status.operations.find((key) => key.startsWith('Identifying:') && key.endsWith(`:${job.id}`)) : undefined;
  // Normalized snapshot for this run: stages, workers, counters, and failed
  // stages come from it stage-agnostically.
  const snap = runSnapshot && runSnapshot.activeJob === job?.id ? runSnapshot : null;
  const stages = snap ? snap.stages : (p ? orderStages(p.pipeline.stages) : []);
  const failedMap: Record<string, string> = Object.fromEntries((snap?.failedStages ?? []).map((f) => [f.id, f.reason]));
  const idLive = mode === 'identify' ? identify : ingestIdentify(identify, { planned: visionExpected(stages, null), identifying: Boolean(operation) });
  // The decision model's newest feed: the dedicated event, else the copy the identify snapshot
  // carries so the identify page shows it even before the first `decider-progress` arrives.
  const deciderLive = decider ?? idLive?.decider ?? null;
  // Vision off is a reason, never a model name: the screen says why and offers the way to change it.
  const visionOff = visionOffReason(idLive);
  useEffect(() => {
    for (const car of idLive?.active ?? []) {
      if (car.prompt) prompts.current[car.detection_id] = car.prompt;
    }
    if (idLive?.current?.prompt) prompts.current[idLive.current.detection_id] = idLive.current.prompt;
  }, [idLive]);

  const measureOverflow = () => {
    const el = workersRef.current;
    if (!el) return;
    const remaining = el.scrollHeight - el.clientHeight - el.scrollTop;
    if (remaining <= 18) {
      setOverflow((prev) => (prev.hasMore || prev.hiddenBelow ? { hasMore: false, hiddenBelow: 0 } : prev));
      return;
    }
    const bottom = el.getBoundingClientRect().bottom;
    const cards = Array.from(el.querySelectorAll<HTMLElement>('.ing-lanes > .id-lane'));
    const hiddenBelow = cards.filter((card) => card.getBoundingClientRect().top > bottom - 36).length;
    setOverflow((prev) => (prev.hasMore && prev.hiddenBelow === hiddenBelow ? prev : { hasMore: true, hiddenBelow }));
  };

  useEffect(() => {
    const el = workersRef.current;
    if (!el) return undefined;
    const raf = requestAnimationFrame(measureOverflow);
    const onScroll = () => measureOverflow();
    el.addEventListener('scroll', onScroll, { passive: true });
    let ro: ResizeObserver | undefined;
    if (typeof ResizeObserver !== 'undefined') {
      ro = new ResizeObserver(measureOverflow);
      ro.observe(el);
    }
    return () => {
      cancelAnimationFrame(raf);
      el.removeEventListener('scroll', onScroll);
      ro?.disconnect();
    };
  }, [density, filter, showAll, picked, idLive, deciderLive, p]);

  const scanState = !p ? (running ? 'starting' : job?.status === 'error' ? 'error' : job?.status === 'done' || job?.status === 'done_errors' ? 'done' : 'idle')
    : p.state === 'running' && !running && job?.status !== 'scanning' ? 'stopped' : p.state === 'done_errors' ? 'done' : p.state;
  // Bind import errors strictly to this album: a null-job failure shows only
  // on the empty Scan screen, never on another album.
  const seed = scanSeedFor(scanError ?? null, job?.id ?? null);
  const failure = mode === 'ingest' ? scanFailureFor({ job, progress: p, status, seed }) : null;
  const phase = ingestPhase({ mode, scan: mode === 'ingest' ? { state: scanState, stage: p?.stage } : null, identify: idLive, identifying: Boolean(operation) });
  if (!job || phase === 'idle') {
    if (mode === 'ingest' && failure) {
      return (
        <div className="screen">
          <div className="identify-page ingest-page">
            <ScanFailureBanner failure={failure} onRetry={onRetryScan ?? onResume} onLibrary={onLibrary} />
          </div>
        </div>
      );
    }
    return mode === 'identify' ? (
      <div className="screen">
        <Empty icon="eye" title="No identification is running">
          <p>Start one from an album with <b>Identify</b>, or watch a finished one from Review.</p>
          <button className="primary" onClick={onReview}>Back to review</button>
        </Empty>
      </div>
    ) : (
      <div className="screen"><Empty icon="images" title="No scan is running.">
        <p>Start one from the Library with <b>Import photos</b>, or open an album to review it.</p>
        <button className="primary" onClick={onLibrary}>Back to library</button>
      </Empty></div>
    );
  }

  const live = phaseRunning(phase);
  // What this album looks for, in its own words: a rodeo reads animals and riders, a motorsport
  // album vehicles. The header, the flowchart and the rate all ask this instead of saying "cars".
  const albumSummary = applyOverrides(
    defaultSummary(String(job.scan_profile || 'motorsport')),
    overridesOf(albumSetting(job, 'overrides', null)),
    overridesOf(albumSetting(job, 'user_defaults', null)),
  );
  const subject = subjectWords(albumSummary);
  const reads = readsSubjects(albumSummary);
  // Both the subject reads and the bursts compared after them are the identify run: the header,
  // stats and stop button belong to it, and the count they show is its own.
  const identifying = phase === 'vision' || phase === 'peaks';
  const vision = mode === 'identify' || visionExpected(stages, idLive);
  const frozen = live && phase !== 'starting' && (identifying ? identifyPaused : paused);
  const head = stages.find((s) => s.id === p?.stage);
  const done = idLive?.done ?? 0;
  const idTotal = idLive?.total ?? 0;
  const activePhase = idLive?.phase?.state === 'running' ? idLive.phase : null;
  const ocrPhase = activePhase?.name === 'ocr';
  const phaseCounts = Boolean(activePhase && (activePhase.total ?? 0) > 0 && (done === 0 || activePhase.name === 'peaks' || !reads));
  const shownDone = phaseCounts ? (activePhase?.done ?? 0) : done;
  const shownTotal = phaseCounts ? (activePhase?.total ?? 0) : idTotal;
  const scanFrac = p ? (scanState === 'done' ? 1 : overallFraction(stages, p.overall)) : 0;
  const frac = mode === 'identify' ? progressPercent(shownDone, shownTotal) / 100
    : phase === 'done' ? 1 : ingestFraction({ scanFraction: scanFrac, identify: idLive, vision });
  const pct = Math.round(frac * 100);
  // When the run snapshot is active, render its derived surface nodes (identify
  // sub-nodes, disabled steps, controls, status words); otherwise fall back to
  // legacy node builders for identify-only runs or older engines.
  const surfaceNodes = mode === 'ingest' && snap && surface && Array.isArray(surface.nodes) ? surface.nodes : null;
  const nodes: any[] = surfaceNodes ?? (mode === 'identify' ? identifyNodes(idLive, idLive?.model || model, deciderLive, decisionService, subject)
    : ingestNodes({ stages, identify: idLive, model, finished: phase === 'done', decider: deciderLive, serving: decisionService, subject, reads, failed: failedMap }));

  const visionLanes = workerLanes(idLive);
  const shownVisionLanes = visibleWorkerLanes(visionLanes, idLive?.phase);
  const scanFilter = filter && filter !== 'vision' && filter !== 'decider' ? filter : null;
  // Render only active workers on their current step: done/failed workers leave
  // the grid (results move to the recent feed), workers past the report window
  // show as stalled, and run end / stage transitions clear stale lanes.
  const workerOpts = { now: Date.now(), stallMs: snap?.stallMs ?? 45000, paused, identify: !idLive, runActive: snap ? snap.runActive : live };
  const snapWorkers = snap ? snap.workers : p?.pipeline.workers;
  const picks = activeWorkers(snapWorkers, { ...workerOpts, filter: scanFilter, cap: 8, all: showAll });
  const allScan = activeWorkers(snapWorkers, { ...workerOpts, cap: 1000 }).lanes;
  const awake = allScan.filter((w) => laneState(w, paused) !== 'idle').length;
  // Scan lanes that have nothing left to do give way to the barrier note (group, pick) and to the vision workers.
  const barrier = mode === 'ingest' && (phase === 'group' || phase === 'pick') && filter == null;
  const hideScan = filter == null && awake === 0 && (barrier || shownVisionLanes.length > 0);
  const showScan = mode === 'ingest' && Boolean(p) && filter !== 'vision' && filter !== 'decider' && !hideScan;
  const showVision = shownVisionLanes.length > 0 && (filter == null || filter === 'vision');
  // The decision model's page: when picked in the flowchart, or while it has something to show.
  // Identify runs it beside vision, so the page is not limited to the scan screen.
  const showDecider = deciderLive != null && (filter === 'decider' || (filter == null && deciderActive(deciderLive)));
  const visibleWorkerCount = (showDecider ? 1 : 0) + (showScan ? picks.lanes.length : 0) + (showVision ? shownVisionLanes.length : 0);
  const liveWorkerCount = (showDecider && deciderLive && (deciderLive.busy > 0 || deciderLive.state === 'loading' || deciderLive.state === 'running') ? 1 : 0)
    + (showScan ? picks.lanes.filter((w) => laneState(w, paused) === 'busy').length : 0)
    + (showVision ? shownVisionLanes.filter((l) => l.live).length : 0);
  const gridMode = workerGridMode(visibleWorkerCount, { soloDecider: showDecider && visibleWorkerCount === 1, density });
  const laneLayout: 'card' | 'tile' = gridMode === 'tiles' ? 'tile' : 'card';
  const deciderLayout: 'hero' | 'card' | 'tile' = gridMode === 'hero' ? 'hero' : laneLayout;

  const total = stages[0]?.total || p?.total || job.total;
  const smoothed = scan ? etaRemaining(scan.history) : null;
  const idProgressLive = idLive && phaseCounts ? { ...idLive, done: shownDone, total: shownTotal } : idLive;
  // The bar already spans scan + identify; the time left must say both, not only the stage that happens to be running.
  const timeLeft = mode === 'identify'
    ? [leftLabel(etaLabel(identifyEta(idProgressLive), shownDone >= 3))]
    : etaLines({ vision, scanDone: identifying || phase === 'done', scanEta: smoothed ?? (p ? overallEta(stages, p.overall) : null), scanStable: Boolean(scan?.stable), identify: idLive, subject });
  const rate = identifying ? (idLive ? rateLabel(shownDone, idLive.elapsed_ms, subject) : '') : frameRate(formatRate(p?.rate_per_s));
  // Disabled steps do not count toward progress; compute "Stage N of M" over
  // active stages only so the index never exceeds the total.
  const place = stagePlace(nodes, head?.id ?? p?.stage);
  const stageTotal = surfaceNodes ? place.total : nodes.filter((node) => node.id !== 'done').length;
  const stageAt = surfaceNodes ? place.at : nodes.findIndex((node) => node.id === (head?.id ?? p?.stage)) + 1;
  const sub = phase === 'starting' ? (mode === 'identify' ? (reads ? `Waiting for the first ${subject.one}…` : 'Starting the judge…') : 'Reading the folder…')
    : phase === 'peaks' ? (phaseLabel(idLive?.phase) || 'Comparing bursts')
      : phase === 'vision' ? (activePhase ? (phaseLabel(activePhase) || (ocrPhase ? 'Reading plates and numbers' : 'Judging moments'))
        : !reads ? (phaseLabel(idLive?.phase) || 'Judging moments')
        : visionOff ? visionHeader(visionOff)
        : `Reading ${subject.many} with ${visionModelWords(idLive?.model || model) || 'the vision model'}`)
        : phase === 'scan' || phase === 'group' || phase === 'pick' ? `Stage ${Math.max(1, stageAt)} of ${stageTotal} · ${head?.label ?? p?.stage}`
          : phase === 'done' ? (mode === 'identify' ? 'Identification complete' : 'Scan complete') : '';
  const statLabel = activePhase?.name === 'ocr' ? 'OCR'
    : activePhase?.name === 'peaks' ? 'Bursts'
      : activePhase?.name === 'moments' || !reads ? 'Moments'
        : subject.title;

  const detailCar = picked?.kind === 'car' ? displayItem(idLive, picked.id) : null;
  const detailFrame = picked?.kind === 'frame' ? p?.recent.find((f) => f.image_id === picked.id) ?? null : null;
  const strip = stripItems({ cars: idLive?.recent ?? [], frames: mode === 'ingest' ? p?.recent ?? [] : [], limit: 40 });
  const pickCar = (id: number) => { setPicked({ kind: 'car', id }); };
  const pickFrame = (id: number) => { setPicked({ kind: 'frame', id }); };

  return (
    <div className="screen identify-screen ingest-screen">
      <div className="identify-page ingest-page">
        <header className={`id-head ing-head ${phase}`}>
          <div className="id-head-main">
            <span className="id-eyebrow mono">{mode === 'identify' ? 'IDENTIFY / LIVE' : 'INGEST / LIVE'}</span>
            <h2>{job.label}</h2>
            <span className="ing-statusline">
              <span className={`id-state ${phaseTone(phase, frozen)}`}><i />{phaseWord(phase, frozen)}</span>
              {sub && <span className="muted ing-sub-text">{sub}</span>}
              {mode === 'ingest' && (phase === 'starting' || phase === 'scan' || phase === 'group' || phase === 'pick') && (
                <span className="muted ing-lock-note"><Icon name="eye" size={13} /> Review opens when the scan finishes. Other albums stay open from the Library.</span>
              )}
            </span>
          </div>
          {live && phase !== 'starting' && (
            <dl className="id-stats">
              {identifying
                ? <div><dt>{statLabel}</dt><dd className="mono">{shownTotal > 0 || idLive
                  ? `${shownDone.toLocaleString()} / ${shownTotal.toLocaleString()}`
                  : '–'}</dd></div>
                : <div><dt>{head?.label ?? p?.stage ?? 'Frames'}</dt><dd className="mono">{p ? `${p.done.toLocaleString()} / ${p.total.toLocaleString()}` : '–'}</dd></div>}
              <div><dt>Rate</dt><dd className="mono">{rate || '–'}</dd></div>
              <div><dt>Time left</dt><dd className="mono ing-eta">{timeLeft.map((line, i) => <span key={`${i}:${line}`} className={i > 0 ? 'ing-eta-next' : ''}>{line}</span>)}</dd></div>
              <div><dt>Workers</dt><dd className="mono">{identifying ? visionLanes.length || '–' : allScan.length ? `${awake} / ${allScan.length}` : '–'}</dd></div>
            </dl>
          )}
          {live && (mode === 'identify' || identifying) && (
            <div className="id-actions">
              <button className="ghost danger" onClick={() => onCancelOperation(operation ?? `Identifying:${job.id}`)}>Stop identification</button>
            </div>
          )}
          {live && mode === 'ingest' && !identifying && (
            <div className="id-actions">
              <button className="ghost" onClick={paused ? onResume : onPause}>{paused ? 'Resume' : 'Pause'}</button>
              {/* Run-level Stop: stops the run while preserving completed work for resume. */}
              <button className="ghost danger" onClick={onCancel} title="Stop this run. Anything already read is kept and can be resumed.">Stop</button>
            </div>
          )}
        </header>

        {mode === 'ingest' && queue.length > 0 && (
          <div className="ing-lock ing-queue" role="status">
            <Icon name="hourglass" size={13} />
            <span>Next: {queue[0].label}, {queue.length} waiting</span>
            {onUnqueue && queue.map((q) => (
              <button key={q.id} type="button" className="ghost small" title={`Take ${q.label} out of the line. Nothing imported is lost.`}
                onClick={() => onUnqueue(q)}>Remove {q.label}</button>
            ))}
          </div>
        )}

        {(live || mode === 'identify') && (
          <div className="id-progress">
            <div className="bar id-bar" role="progressbar" aria-valuenow={phase === 'starting' ? undefined : pct} aria-valuemin={0} aria-valuemax={100}
              aria-label={mode === 'identify' ? 'Identification progress' : 'Scan progress'}>
              {phase === 'starting' ? <div className="fill indeterminate" /> : <div className="fill" style={{ width: `${pct}%` }} />}
            </div>
            <span className="mono id-progress-count">
              {phase === 'starting' ? (mode === 'identify' ? (reads ? `Waiting for the first ${subject.one}…` : 'Starting the judge…') : 'Starting…')
                : mode === 'identify' ? `${pct}% · ${shownDone.toLocaleString()} / ${shownTotal.toLocaleString()}` : `${pct}%`}
            </span>
          </div>
        )}

        <ol className={`id-flow ing-flow${live ? '' : ' still'}`} aria-label={mode === 'identify' ? 'Identification stages' : 'Pipeline stages'}>
          {surfaceNodes ? (
            nodes.map((node, i) => (
              <FlowNode key={node.id} node={node} first={i === 0} filter={filter}
                onFilter={(f) => { setFilter(f); setShowAll(false); setPicked(null); }}
                onStageControl={onStageControl} onAcceptTypeSuggestion={onAcceptTypeSuggestion}
                onOpenShootSettings={onOpenShootSettings} />
            ))
          ) : (
            nodes.map((node, i) => {
              const inner = (
                <>
                  <span className="id-node-top">
                    <span className="id-node-num mono">{node.number}</span>
                    {node.state === 'done' && <Icon name="check" size={12} />}
                    {node.state === 'failed' && <Icon name="x" size={12} />}
                    {node.state === 'active' && <i className="id-node-dot" aria-hidden="true" />}
                  </span>
                  <b>{node.label}</b>
                  <small className="ing-sub" title={node.title || node.sub}>{node.sub}</small>
                  {node.meta && <small className="ing-meta" title={node.meta}>{node.meta}</small>}
                </>
              );
              const cls = `id-node ${node.state}${node.off ? ' off' : ''}`;
              return (
                <li className="id-flow-cell" key={node.id}>
                  {i > 0 && <span className={`id-link${node.state === 'active' ? ' flow' : node.state === 'done' ? ' done' : ''}`} aria-hidden="true" />}
                  {node.filter ? (
                    <button type="button" className={`${cls} ing-node${filter === node.filter ? ' sel' : ''}`} aria-pressed={filter === node.filter}
                      title={filter === node.filter ? `Showing ${node.label} workers. Click to show all.` : `Show only the ${node.label} workers`}
                      onClick={() => { setFilter((f) => (f === node.filter ? null : node.filter)); setShowAll(false); setPicked(null); }}>
                      {inner}
                    </button>
                  ) : <div className={cls}>{inner}</div>}
                </li>
              );
            })
          )}
        </ol>

        {/* Surface every failed stage with its plain reason and a retry action,
            regardless of stage id. */}
        {snap && snap.failedStages.length > 0 && (
          <section className="scan-summary scan-failure ing-stage-failure" role="alert" aria-label="Failed stages">
            <div className="ss-mark"><Icon name="x" size={20} /></div>
            <div className="ss-text">
              <b>These stages need another try</b>
              <span className="muted">{snap.failedStages.map((f) => `${f.label}: ${f.reason}`).join(' · ')}</span>
            </div>
            <div className="ss-actions">
              {onRetryScan && <button className="primary" onClick={onRetryScan}>Retry the failed stage</button>}
            </div>
          </section>
        )}

        {(phase === 'done' || phase === 'stopped') && mode === 'ingest' && p && (
          <section className="scan-summary" aria-label="Scan summary">
            <div className="ss-mark"><Icon name={phase === 'done' ? 'check' : 'x'} size={20} /></div>
            <div className="ss-text">
              <b>{phase === 'done'
                ? summaryLine(total, p.found) + (idLive?.done ? ` · ${idLive.done.toLocaleString()} ${idLive.done === 1 ? subject.one : subject.many} read` : '') + (job.errors ? ` · ${job.errors.toLocaleString()} could not be read` : '')
                : `Stopped at ${pct}% · ${p.found.subjects.toLocaleString()} subjects found so far`}</b>
              <span className="muted">{phase !== 'done' ? 'Frames that were read can be reviewed now. Resume the scan from the Library to finish grouping and picking.'
                : job.errors ? `${job.error_samples?.[0] ?? 'Some files could not be read'}. The rest are ready to review; retry the failed frames from the Library.`
                  : 'Keepers are suggestions. Nothing is rejected or written until you say so.'}</span>
            </div>
            <button className="primary" autoFocus={phase === 'done'} onClick={onReview}>Review</button>
          </section>
        )}
        {phase === 'error' && mode === 'ingest' && (
          failure
            ? <ScanFailureBanner failure={failure} onRetry={onRetryScan ?? onResume} onLibrary={onLibrary} />
            : <div className="scan-summary"><div className="ss-text"><b>The scan did not finish.</b>
              <span className="muted">Check the activity log in the top bar, then try again.</span></div><button onClick={onLibrary}>Back to library</button></div>
        )}
        {mode === 'identify' && (phase === 'done' || phase === 'stopped' || phase === 'error') && (
          <section className={`scan-summary id-summary ${phase}`} aria-label="Identification summary">
            <div className="ss-mark"><Icon name={phase === 'done' ? 'check' : 'x'} size={20} /></div>
            <div className="ss-text">
              <b>{phase === 'done'
                ? `${done.toLocaleString()} ${done === 1 ? subject.one : subject.many} read in ${elapsed(idLive?.elapsed_ms) || 'no time at all'}`
                : phase === 'stopped' ? `Stopped at ${pct}% · ${done.toLocaleString()} ${subject.many} read` : 'Identification did not finish.'}</b>
              <span className="muted">{phase === 'error'
                ? (idLive?.errors?.[0] ?? 'Check the activity log in the top bar.')
                : 'Identifications are suggestions until you accept them. Nothing is written until you say so.'}</span>
            </div>
            <div className="ss-actions">
              {(phase === 'stopped' || phase === 'error') && onContinueIdentify
                && <button className="primary" onClick={onContinueIdentify}>Continue identification</button>}
              <button className="ghost" onClick={onReview}>Back to review</button>
            </div>
          </section>
        )}

        <div className={`ing-body${mode === 'ingest' && p ? '' : ' solo'}`}>
          <div className="ing-main">
            <div className="ing-workers-shell">
              <section aria-label="Workers" className="ing-workers" ref={workersRef}>
                {!(detailCar || detailFrame) && (
                  <div className="ing-workers-head">
                    <div className="section-label">
                      <span>Workers</span>
                      {visibleWorkerCount > 0 && (
                        <span className="ing-count-badge mono">
                          {liveWorkerCount > 0 ? `${liveWorkerCount} live · ` : ''}{visibleWorkerCount} shown
                        </span>
                      )}
                    </div>
                    {filter && (
                      <span className="ing-chip">
                        SHOWING {nodes.find((n) => n.filter === filter)?.label ?? filter.toUpperCase()}
                        <button type="button" aria-label="Show all workers" onClick={() => { setFilter(null); setShowAll(false); }}>✕ clear</button>
                      </span>
                    )}
                    {visibleWorkerCount > 0 && (
                      <div className="ing-density" role="group" aria-label="Worker card layout">
                        {(['auto', 'cards', 'tiles'] as const).map((opt) => (
                          <button
                            key={opt}
                            type="button"
                            className={`mono${density === opt ? ' on' : ''}`}
                            aria-pressed={density === opt}
                            onClick={() => setDensity(opt)}
                          >
                            {opt === 'auto' ? 'Auto' : opt === 'cards' ? 'Cards' : 'Tiles'}
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                )}
                {detailCar ? (
                  <IdentifyDetail detail={detailCar} prompt={detailCar.prompt ?? prompts.current[detailCar.detection_id]} live={detailCar.stage !== 'done' && identifying} visionOff={visionOff} onVisionAction={onVisionAction} subject={subject} />
                ) : detailFrame ? (
                  <FrameDetail frame={detailFrame} />
                ) : (showScan && picks.lanes.length) || showVision || showDecider || barrier ? (
                  <>
                    {barrier && (
                      <div className="ing-barrier" role="status">
                        <i className="id-node-dot" aria-hidden="true" />
                        <b>{phase === 'group' ? 'Grouping' : 'Picking'}</b>
                        <span className="muted">{phase === 'group' ? `Matching ${total.toLocaleString()} frames into bursts. This step has no workers to show.` : 'Ranking each burst for a suggested keeper. This step has no workers to show.'}</span>
                      </div>
                    )}
                    <div className={`id-lanes ing-lanes ing-lanes-${gridMode}${visibleWorkerCount === 1 && gridMode !== 'hero' ? ' single' : ''}`}>
                      {showDecider && deciderLive && <DeciderLane live={deciderLive} serving={decisionService} layout={deciderLayout} />}
                      {showScan && picks.lanes.map((worker) => <ScanLane key={laneKey(worker)} worker={worker} paused={paused} recent={p?.recent ?? []} layout={laneLayout} />)}
                      {showVision && shownVisionLanes.map((lane) => (
                        <IdentifyLane key={`vision:${lane.worker}`} name={ocrPhase ? 'OCR' : 'VISION'} ocrOnly={ocrPhase} layout={laneLayout} worker={lane.worker} item={lane.item} live={lane.live} visionOff={visionOff} onVisionAction={onVisionAction} subject={subject}
                          prompt={lane.item ? lane.item.prompt ?? prompts.current[lane.item.detection_id] : undefined}
                          onOpen={() => lane.item && pickCar(lane.item.detection_id)} />
                      ))}
                    </div>
                    {showScan && (picks.hidden > 0 || showAll) && (
                      <button type="button" className="ghost small ing-more" onClick={() => setShowAll((v) => !v)}>
                        {showAll ? 'Show the busiest only' : `Show all ${picks.lanes.length + picks.hidden} scan workers`}
                      </button>
                    )}
                  </>
                ) : (
                  <p className="muted ing-empty-note">
                    {filter ? 'No workers on this stage right now.'
                      : phase === 'starting' ? 'Loading the models and listing the files. The workers appear in a moment.'
                        : mode === 'identify' ? `Waiting for the workers to pick up the first ${subject.many}…` : 'Waiting for the workers to pick up the first frames…'}
                  </p>
                )}
              </section>
              {!(detailCar || detailFrame) && overflow.hasMore && (
                <>
                  <div className="ing-overflow-fade" aria-hidden="true" />
                  <button
                    type="button"
                    className="ing-overflow-pill mono"
                    onClick={() => workersRef.current?.scrollBy({ top: 240, behavior: 'smooth' })}
                  >
                    {overflow.hiddenBelow > 0
                      ? `+${overflow.hiddenBelow} more ${overflow.hiddenBelow === 1 ? 'worker' : 'workers'} below ↓`
                      : 'More below ↓'}
                  </button>
                </>
              )}
            </div>

            <section className="id-recent ing-strip" aria-label={mode === 'identify' ? `${subject.title} already read` : `Frames and ${subject.many} as they finish`}>
              <div className="ing-strip-head">
                <span className="section-label">{mode === 'identify' ? `Recent ${subject.many}` : 'Just finished'}<span>{strip.length ? 'newest first' : ''}</span></span>
                <button type="button" className={`id-live-return${picked == null ? ' on' : ''}`} disabled={picked == null}
                  title="Back to the workers" onClick={() => setPicked(null)}>
                  <i aria-hidden="true" />WORKERS
                </button>
              </div>
              {strip.length ? (
                <div className="ing-strip-row">
                  {strip.map((item) => {
                    if (item.kind === 'car') {
                      const car = item.car;
                      const conf = car.number ? confidencePercent(car.number_conf) : null;
                      const tone = confidenceTone(car.number_conf);
                      const thumb = asset(car.thumb_path ?? car.preview_path ?? car.path);
                      return (
                        <button type="button" className={`id-card ing-card subject${picked?.kind === 'car' && picked.id === car.detection_id ? ' sel' : ''}`} key={item.key}
                          title={[car.make, car.model, car.team].filter(Boolean).join(' · ') || 'Subject read'} onClick={() => pickCar(car.detection_id)}>
                          {thumb ? <img src={thumb} alt="" loading="lazy" draggable={false} /> : <span className="id-card-noimg" aria-hidden="true" />}
                          <span className="id-card-num">{hash(car.number)}</span>
                          <span className="id-card-make">{[car.make, car.model].filter(Boolean).join(' ') || car.plate || (visionOff ? 'Vision off' : 'Make not read')}</span>
                          <span className={`id-card-conf ${car.number ? tone : 'none'}`}>{conf != null ? `${conf}%` : car.number ? 'no confidence' : 'no number'}</span>
                        </button>
                      );
                    }
                    const f: ScanRecentFrame = item.frame;
                    return (
                      <button type="button" className={`id-card ing-card frame ${f.verdict}${picked?.kind === 'frame' && picked.id === f.image_id ? ' sel' : ''}`} key={item.key}
                        title={[f.file, verdictText(f.verdict, f.panning)].filter(Boolean).join(' · ') || 'Frame'} onClick={() => pickFrame(f.image_id)}>
                        {f.thumb_url ? <img src={asset(f.thumb_url)} alt="" loading="lazy" draggable={false} /> : <span className="id-card-noimg" aria-hidden="true" />}
                        <span className="id-card-num">{verdictText(f.verdict, f.panning) || 'Frame'}</span>
                        <span className="id-card-make">{f.label || 'Not identified'}</span>
                        <span className="id-card-conf">{starsText(f.stars) || ' '}</span>
                      </button>
                    );
                  })}
                </div>
              ) : <p className="muted id-recent-empty">{mode === 'identify' ? `Finished ${subject.many} appear here, with the number the engine read.` : `Frames and ${subject.many} appear here as they come through.`}</p>}
            </section>
          </div>

          {mode === 'ingest' && p && (
            <aside className="scan-rail">
              <Found found={p.found} identify={vision} />
              {run && onSettingsSaved && <WorkerPanel run={run} onSaved={onSettingsSaved} scanning={phase === 'scan' || phase === 'starting' || phase === 'group' || phase === 'pick'} concurrency={surface?.concurrency} />}
            </aside>
          )}
        </div>
      </div>
    </div>
  );
}
