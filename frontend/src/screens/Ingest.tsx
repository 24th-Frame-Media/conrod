import { useEffect, useRef, useState } from 'react';
import { IdentifyDetail, IdentifyLane, hash } from '../components/IdentifyParts';
import { FrameDetail, ScanLane } from '../components/IngestLane';
import { WorkerPanel } from '../components/WorkerPanel';
import { DeciderLane } from '../components/DeciderLane';
import { deciderActive } from '../lib/decider.mjs';
import { useDeciderProgress } from '../state/useDeciderProgress';
import { Empty, Icon } from '../components/basics';
import { asset } from '../lib/api';
import { confidencePercent, confidenceTone, displayItem, elapsed, progressPercent, rateLabel, workerLanes } from '../lib/identifyFlow.mjs';
import {
  frameRate, identifyEta, identifyNodes, ingestFraction, ingestIdentify, ingestNodes, ingestPhase, laneKey, laneState, leftLabel,
  phaseRunning, phaseTone, phaseWord, selectLanes, starsText, stripItems, verdictText, visionExpected,
} from '../lib/ingest.mjs';
import { visionHeader, visionOffReason, type VisionAction } from '../lib/vision.mjs';
import { etaLabel, etaRemaining, formatRate, orderStages, overallEta, overallFraction, summaryLine } from '../lib/scan.mjs';
import type { IdentifyLive, Job, ScanFound, ScanRecentFrame, Settings, Status } from '../lib/types';
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

/**
 * One live page for an album's whole ingest: one bar, one flowchart (read to pick, then vision), a grid of every worker that is
 * doing something (scan lanes with boxes and autofocus, vision lanes with the model's question and reply), and one strip of what
 * just finished. The same screen shows a standalone identification run with only the vision stages.
 */
export function IngestScreen({
  mode, job, scan, identify, status, model, running = false, paused, identifyPaused, run, onSettingsSaved, onPause, onResume, onCancel,
  onCancelOperation, onVisionAction, onReview, onLibrary, queue = [], onUnqueue,
}: IngestProps) {
  const [picked, setPicked] = useState<Pick | null>(null);
  const [filter, setFilter] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);
  const decider = useDeciderProgress();
  // Prompts only ride on the cars being read and on the snapshot's active list, not on the
  // finished strip; keep the last one per car so a finished car can still show what was asked.
  const prompts = useRef<Record<number, string>>({});
  const p = mode === 'ingest' ? scan?.progress : undefined;
  const operation = job ? status.operations.find((key) => key.startsWith('Identifying:') && key.endsWith(`:${job.id}`)) : undefined;
  const stages = p ? orderStages(p.pipeline.stages) : [];
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

  const scanState = !p ? (running ? 'starting' : job?.status === 'done' || job?.status === 'done_errors' ? 'done' : 'idle')
    : p.state === 'running' && !running && job?.status !== 'scanning' ? 'stopped' : p.state === 'done_errors' ? 'done' : p.state;
  const phase = ingestPhase({ mode, scan: mode === 'ingest' ? { state: scanState, stage: p?.stage } : null, identify: idLive, identifying: Boolean(operation) });
  if (!job || phase === 'idle') {
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
  const vision = mode === 'identify' || visionExpected(stages, idLive);
  const frozen = live && phase !== 'starting' && (phase === 'vision' ? identifyPaused : paused);
  const head = stages.find((s) => s.id === p?.stage);
  const scanFrac = p ? (scanState === 'done' ? 1 : overallFraction(stages, p.overall)) : 0;
  const frac = mode === 'identify' ? progressPercent(idLive?.done, idLive?.total) / 100
    : phase === 'done' ? 1 : ingestFraction({ scanFraction: scanFrac, identify: idLive, vision });
  const pct = Math.round(frac * 100);
  const nodes = mode === 'identify' ? identifyNodes(idLive, idLive?.model || model, deciderLive) : ingestNodes({ stages, identify: idLive, model, finished: phase === 'done', decider: deciderLive });

  const visionLanes = workerLanes(idLive);
  const scanFilter = filter && filter !== 'vision' && filter !== 'decider' ? filter : null;
  const picks = selectLanes(p?.pipeline.workers, { filter: scanFilter, cap: 8, all: showAll, paused, identify: !idLive });
  const allScan = selectLanes(p?.pipeline.workers, { cap: 1000, paused, identify: !idLive }).lanes;
  const awake = allScan.filter((w) => laneState(w, paused) !== 'idle').length;
  // Scan lanes that have nothing left to do give way to the barrier note (group, pick) and to the vision workers.
  const barrier = mode === 'ingest' && (phase === 'group' || phase === 'pick') && filter == null;
  const hideScan = filter == null && awake === 0 && (barrier || visionLanes.length > 0);
  const showScan = mode === 'ingest' && Boolean(p) && filter !== 'vision' && filter !== 'decider' && !hideScan;
  const showVision = visionLanes.length > 0 && (filter == null || filter === 'vision');
  // The decision model's page: when picked in the flowchart, or while it has something to show.
  // Identify runs it beside vision, so the page is not limited to the scan screen.
  const showDecider = deciderLive != null && (filter === 'decider' || (filter == null && deciderActive(deciderLive)));

  const total = stages[0]?.total || p?.total || job.total;
  const done = idLive?.done ?? 0;
  const idTotal = idLive?.total ?? 0;
  const smoothed = scan ? etaRemaining(scan.history) : null;
  const eta = phase === 'vision' ? etaLabel(identifyEta(idLive), done >= 3) : etaLabel(smoothed ?? (p ? overallEta(stages, p.overall) : null), Boolean(scan?.stable));
  const rate = phase === 'vision' ? (idLive ? rateLabel(idLive.done, idLive.elapsed_ms) : '') : frameRate(formatRate(p?.rate_per_s));
  const sub = phase === 'starting' ? (mode === 'identify' ? 'Waiting for the first car…' : 'Reading the folder…')
    : phase === 'vision' ? (visionOff ? visionHeader(visionOff) : `Reading cars with ${idLive?.model || model || 'the vision model'}`)
      : phase === 'scan' || phase === 'group' || phase === 'pick' ? `Stage ${(p?.stage_index ?? 0) + 1} of ${p?.stage_count ?? nodes.length} · ${head?.label ?? p?.stage}`
        : phase === 'done' ? (mode === 'identify' ? 'Identification complete' : 'Scan complete') : '';

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
            </span>
          </div>
          {live && phase !== 'starting' && (
            <dl className="id-stats">
              {phase === 'vision'
                ? <div><dt>Cars</dt><dd className="mono">{idLive ? `${done.toLocaleString()} / ${idTotal.toLocaleString()}` : '–'}</dd></div>
                : <div><dt>{head?.label ?? p?.stage ?? 'Frames'}</dt><dd className="mono">{p ? `${p.done.toLocaleString()} / ${p.total.toLocaleString()}` : '–'}</dd></div>}
              <div><dt>Rate</dt><dd className="mono">{rate || '–'}</dd></div>
              <div><dt>Time left</dt><dd className="mono ing-eta">{leftLabel(eta)}</dd></div>
              <div><dt>Workers</dt><dd className="mono">{phase === 'vision' ? visionLanes.length || '–' : allScan.length ? `${awake} / ${allScan.length}` : '–'}</dd></div>
            </dl>
          )}
          {live && (mode === 'identify' || phase === 'vision') && (
            <div className="id-actions">
              <button className="ghost danger" onClick={() => onCancelOperation(operation ?? `Identifying:${job.id}`)}>Stop identification</button>
            </div>
          )}
          {live && mode === 'ingest' && phase !== 'vision' && (
            <div className="id-actions">
              <button className="ghost" onClick={paused ? onResume : onPause}>{paused ? 'Resume' : 'Pause'}</button>
              <button className="ghost danger" onClick={onCancel}>Cancel scan</button>
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
              {phase === 'starting' ? (mode === 'identify' ? 'Waiting for the first car…' : 'Starting…')
                : mode === 'identify' ? `${pct}% · ${done.toLocaleString()} / ${idTotal.toLocaleString()}` : `${pct}%`}
            </span>
          </div>
        )}
        {mode === 'ingest' && (phase === 'starting' || phase === 'scan' || phase === 'group' || phase === 'pick') && (
          <p className="muted ing-lock"><Icon name="eye" size={13} /> Review opens when the scan finishes. Other albums stay open from the Library.</p>
        )}

        <ol className={`id-flow ing-flow${live ? '' : ' still'}`} aria-label={mode === 'identify' ? 'Identification stages' : 'Pipeline stages'}>
          {nodes.map((node, i) => {
            const inner = (
              <>
                <span className="id-node-top">
                  <span className="id-node-num mono">{node.number}</span>
                  {node.state === 'done' && <Icon name="check" size={12} />}
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
          })}
        </ol>

        {(phase === 'done' || phase === 'stopped') && mode === 'ingest' && p && (
          <section className="scan-summary" aria-label="Scan summary">
            <div className="ss-mark"><Icon name={phase === 'done' ? 'check' : 'x'} size={20} /></div>
            <div className="ss-text">
              <b>{phase === 'done'
                ? summaryLine(total, p.found) + (idLive?.done ? ` · ${idLive.done.toLocaleString()} ${idLive.done === 1 ? 'car' : 'cars'} read` : '') + (job.errors ? ` · ${job.errors.toLocaleString()} could not be read` : '')
                : `Stopped at ${pct}% · ${p.found.subjects.toLocaleString()} subjects found so far`}</b>
              <span className="muted">{phase !== 'done' ? 'Frames that were read can be reviewed now. Resume the scan from the Library to finish grouping and picking.'
                : job.errors ? `${job.error_samples?.[0] ?? 'Some files could not be read'}. The rest are ready to review; retry the failed frames from the Library.`
                  : 'Keepers are suggestions. Nothing is rejected or written until you say so.'}</span>
            </div>
            <button className="primary" autoFocus={phase === 'done'} onClick={onReview}>Review</button>
          </section>
        )}
        {phase === 'error' && mode === 'ingest' && (
          <div className="scan-summary"><div className="ss-text"><b>The scan did not finish.</b>
            <span className="muted">{idLive?.errors?.[0] ?? 'Check the activity log in the top bar.'}</span></div><button onClick={onLibrary}>Back to library</button></div>
        )}
        {mode === 'identify' && (phase === 'done' || phase === 'stopped' || phase === 'error') && (
          <section className={`scan-summary id-summary ${phase}`} aria-label="Identification summary">
            <div className="ss-mark"><Icon name={phase === 'done' ? 'check' : 'x'} size={20} /></div>
            <div className="ss-text">
              <b>{phase === 'done'
                ? `${done.toLocaleString()} ${done === 1 ? 'car' : 'cars'} read in ${elapsed(idLive?.elapsed_ms) || 'no time at all'}`
                : phase === 'stopped' ? `Stopped at ${pct}% · ${done.toLocaleString()} cars read` : 'Identification did not finish.'}</b>
              <span className="muted">{phase === 'error'
                ? (idLive?.errors?.[0] ?? 'Check the activity log in the top bar.')
                : 'Identifications are suggestions until you accept them. Nothing is written until you say so.'}</span>
            </div>
            <button className="primary" onClick={onReview}>Back to review</button>
          </section>
        )}

        <div className={`ing-body${mode === 'ingest' && p ? '' : ' solo'}`}>
          <div className="ing-main">
            <section aria-label="Workers" className="ing-workers">
              {!(detailCar || detailFrame) && (
                <div className="ing-workers-head">
                  <div className="section-label"><span>Workers</span><span>{live && !filter ? 'live' : ''}</span></div>
                  {filter && (
                    <span className="ing-chip">
                      SHOWING {nodes.find((n) => n.filter === filter)?.label ?? filter.toUpperCase()}
                      <button type="button" aria-label="Show all workers" onClick={() => { setFilter(null); setShowAll(false); }}>✕ clear</button>
                    </span>
                  )}
                </div>
              )}
              {detailCar ? (
                <IdentifyDetail detail={detailCar} prompt={detailCar.prompt ?? prompts.current[detailCar.detection_id]} live={detailCar.stage !== 'done' && phase === 'vision'} visionOff={visionOff} onVisionAction={onVisionAction} />
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
                  <div className="id-lanes ing-lanes">
              {showDecider && deciderLive && <DeciderLane live={deciderLive} />}
                    {showScan && picks.lanes.map((worker) => <ScanLane key={laneKey(worker)} worker={worker} paused={paused} recent={p?.recent ?? []} />)}
                    {showVision && visionLanes.map((lane) => (
                      <IdentifyLane key={`vision:${lane.worker}`} name="VISION" worker={lane.worker} item={lane.item} live={lane.live} visionOff={visionOff} onVisionAction={onVisionAction}
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
                      : mode === 'identify' ? 'Waiting for the workers to pick up the first cars…' : 'Waiting for the workers to pick up the first frames…'}
                </p>
              )}
            </section>

            <section className="id-recent ing-strip" aria-label={mode === 'identify' ? 'Cars already read' : 'Frames and cars as they finish'}>
              <div className="ing-strip-head">
                <span className="section-label">{mode === 'identify' ? 'Recent cars' : 'Just finished'}<span>{strip.length ? 'newest first' : ''}</span></span>
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
                        <button type="button" className={`id-card ing-card car${picked?.kind === 'car' && picked.id === car.detection_id ? ' sel' : ''}`} key={item.key}
                          title={[car.make, car.model, car.team].filter(Boolean).join(' · ') || 'Car read'} onClick={() => pickCar(car.detection_id)}>
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
              ) : <p className="muted id-recent-empty">{mode === 'identify' ? 'Finished cars appear here, with the number the engine read.' : 'Frames and cars appear here as they come through.'}</p>}
            </section>
          </div>

          {mode === 'ingest' && p && (
            <aside className="scan-rail">
              <Found found={p.found} identify={vision} />
              {run && onSettingsSaved && <WorkerPanel run={run} onSaved={onSettingsSaved} scanning={phase === 'scan' || phase === 'starting' || phase === 'group' || phase === 'pick'} />}
            </aside>
          )}
        </div>
      </div>
    </div>
  );
}
