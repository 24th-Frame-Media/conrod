import { useEffect, useRef, useState, type CSSProperties } from 'react';
import { asset } from '../lib/api';
import {
  afLabel, afTagSpot, boxLegend, boxRole, boxStyle, laneFrame, laneNumber, laneState, laneText, mainVehicle,
  roleLabel, starsText, stepProgress, tagFits, verdictText,
} from '../lib/ingest.mjs';
import type { AfKind, LaneBox, PipelineWorker, ScanRecentFrame } from '../lib/types';
import '../styles/ingest.css';

type Af = [number, number, number, number] | null | undefined;

/**
 * A frame with its detection boxes and autofocus area drawn over it. Boxes are fractions of the upright frame, so the stage sizes
 * itself to the picture's own aspect ratio (contained in the viewport) and the boxes are plain percentages of it. By default the boxes
 * are bare outlines (main solid, other cars thin grey, the AF area dashed amber): a worker card shows only the photo, the scan and
 * the boxes. With `labels` (the full-page detail) each box carries its tag, and the AF tag moves to whichever corner of its box
 * (or just outside it) is free of detection labels.
 */
export function FrameStage({ src, boxes, af, afKind, labels = false }: { src: string; boxes: LaneBox[]; af?: Af; afKind?: AfKind | null; labels?: boolean }) {
  const [shape, setShape] = useState<{ src: string; ar: number } | null>(null);
  const [width, setWidth] = useState(320);
  const stage = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = stage.current;
    if (!el || typeof ResizeObserver === 'undefined') return undefined;
    const watch = new ResizeObserver(() => setWidth(el.clientWidth));
    watch.observe(el);
    setWidth(el.clientWidth);
    return () => watch.disconnect();
  }, []);
  const ar = shape?.src === src ? shape.ar : 1.5;
  const afStyle = boxStyle(af);
  const afText = afLabel(afKind);
  const afSpot = labels && afStyle ? afTagSpot(af, boxes, { width, height: width / ar }, afText) : null;
  const main = mainVehicle(boxes, af);
  return (
    <div className="ing-frame">
      <div className="ing-frame-box" ref={stage} style={{ '--ar': ar } as CSSProperties}>
        <img src={src} alt="" draggable={false} loading="eager"
          onLoad={(e) => { const { naturalWidth: w, naturalHeight: h } = e.currentTarget; if (w > 0 && h > 0) setShape({ src, ar: w / h }); }} />
        {boxes.map((b, i) => {
          const style = boxStyle(b.box);
          const role = boxRole(b, i, boxes, af);
          const label = roleLabel(b, role, main.byAf);
          return style && (
            <span className={`ing-box${role ? ` ${role}` : ''}`} style={style} key={`${b.cls}:${b.box.join(',')}:${i}`}>
              {labels && tagFits(b.box, width, label) && <span className="ing-box-tag mono">{label}</span>}
            </span>
          );
        })}
        {afStyle && (
          <span className={`ing-af${afKind === 'grid' ? ' grid' : ''}`} style={afStyle} key={`af:${af?.join(',')}`}>
            {afSpot && <span className={`ing-box-tag ing-af-tag at-${afSpot} mono`}>{afText}</span>}
          </span>
        )}
      </div>
    </div>
  );
}

/** A segmented bar over the scan steps with the current one named once, in full ("Sharpness · 4 of 6"); each segment names its step on hover. */
function Steps({ step, stage }: { step: string | null; stage: string }) {
  const progress = stepProgress(step, stage);
  if (!progress) return null;
  return (
    <div className="ing-steps-wrap">
      <ol className="ing-steps" role="img" aria-label={progress.label}>
        {progress.steps.map((s) => <li key={s.id} className={s.state === 'todo' ? '' : s.state} title={s.name} />)}
      </ol>
      <span className="ing-steps-label mono">{progress.label}</span>
    </div>
  );
}

/** The key to the boxes on a frame: the picture carries no text, so which outline is which is said here. */
function Legend({ boxes, af }: { boxes: LaneBox[]; af?: Af }) {
  const items = boxLegend(boxes, af);
  if (!items.length) return null;
  return (
    <span className="ing-legend">
      {items.map((it) => <span key={it.kind} className="ing-legend-item"><i className={`ing-swatch ${it.kind}`} aria-hidden="true" />{it.label}</span>)}
    </span>
  );
}

/**
 * One scan worker: the frame it holds with the boxes and autofocus area found so far, the step it is in, and the rating once it
 * has one. A paused or waiting lane says so instead of looking busy; an idle one dims. Everything but the file name is optional.
 */
export function ScanLane({ worker, paused, recent, layout = 'card' }: { worker: PipelineWorker; paused: boolean; recent: ScanRecentFrame[]; layout?: 'card' | 'tile' }) {
  const state = laneState(worker, paused);
  const text = laneText(worker, paused);
  const frame = laneFrame(worker, recent);
  const image = frame.thumb_url ? asset(frame.thumb_url) : '';
  const number = String(laneNumber(worker)).padStart(2, '0');
  const stars = starsText(frame.stars);
  const verdict = verdictText(frame.verdict, frame.panning);
  const holding = state !== 'idle' && Boolean(frame.file);
  // One short line, centred in the empty frame: the step is named once, in the stage bar below, not here.
  const empty = state === 'idle' ? 'No frame yet' : state === 'paused' ? 'Paused' : state === 'waiting' ? text : frame.file ? 'Preparing the frame…' : 'Waiting for a frame…';
  const viewfinder = (
    <div className="id-view id-view-lane">
      {image && <span className="id-view-fill" style={{ backgroundImage: `url("${image}")` }} aria-hidden="true" />}
      {image
        ? <FrameStage src={image} boxes={frame.boxes} af={frame.af} afKind={frame.af_kind} />
        : <span className="id-view-empty ing-empty"><i className="ing-ph" aria-hidden="true" />{empty}</span>}
      <span className="id-corner tl" aria-hidden="true" /><span className="id-corner tr" aria-hidden="true" />
      <span className="id-corner bl" aria-hidden="true" /><span className="id-corner br" aria-hidden="true" />
      {state === 'busy' && <span className="id-scanline" aria-hidden="true" />}
    </div>
  );
  const meta = (
    <>
      {holding && <span className="ing-file mono muted" title={frame.file}>{frame.file}</span>}
      <Steps step={frame.step} stage={worker.stage} />
      <div className="id-lane-result ing-result" key={frame.file || 'empty'}>
        <span className={`ing-stars mono${stars ? '' : ' none'}`} aria-label={stars ? `${frame.stars} stars` : 'Not rated yet'}>{stars || '☆☆☆☆☆'}</span>
        <div className="id-lane-facts">
          <b>{verdict || 'Not measured yet'}</b>
          <span className="muted">{frame.boxes.length ? `${frame.boxes.length} ${frame.boxes.length === 1 ? 'detection' : 'detections'}` : 'no detections yet'}</span>
          <Legend boxes={frame.boxes} af={frame.af} />
        </div>
      </div>
    </>
  );
  return (
    <section className={`id-lane ing-lane ${state}${layout === 'tile' ? ' tile' : ''}${state === 'busy' ? ' live' : ''}`}
      aria-label={`Worker ${laneNumber(worker)}, ${worker.stage}, ${text}${frame.file ? `, ${frame.file}` : ''}`}>
      <header className="id-lane-head">
        <span className="id-lane-name mono">Worker {number}</span>
        <span className={`id-lane-stage ing-stage ${state}`}>{worker.stage}</span>
        <span className={`ing-lane-text ${state}`}>{state === 'busy' ? '' : text}</span>
        {state === 'busy' && <i className="id-lane-dot" aria-hidden="true" />}
      </header>
      {layout === 'tile' ? (
        <div className="id-lane-split">
          <div className="id-lane-split-left">{viewfinder}</div>
          <div className="id-lane-split-right ing-scan-split-right">{meta}</div>
        </div>
      ) : (
        <>
          {viewfinder}
          {meta}
        </>
      )}
    </section>
  );
}

/** The full page for one culled frame: the picture with its boxes and autofocus area, and the rating facts. */
export function FrameDetail({ frame }: { frame: ScanRecentFrame }) {
  const image = frame.thumb_url ? asset(frame.thumb_url) : '';
  const stars = starsText(frame.stars);
  const verdict = verdictText(frame.verdict, frame.panning);
  const boxes = frame.boxes ?? [];
  const main = mainVehicle(boxes, frame.af);
  const facts: [string, string][] = [
    ['File', frame.file ?? '—'], ['Verdict', verdict || '—'], ['Stars', stars ? `${stars} (${frame.stars})` : '—'],
    ['Panning', frame.panning ? 'Yes' : 'No'], ['Subject', frame.subject || '—'], ['Reads as', frame.label || 'Not identified'],
  ];
  return (
    <div className="id-main">
      <section className="id-left" aria-label="Frame">
        <div className="id-view">
          {image && <span className="id-view-fill" style={{ backgroundImage: `url("${image}")` }} aria-hidden="true" />}
          {image ? <FrameStage src={image} boxes={boxes} af={frame.af} afKind={frame.af_kind} labels /> : <span className="id-view-empty">No picture for this frame</span>}
          <span className="id-corner tl" aria-hidden="true" /><span className="id-corner tr" aria-hidden="true" />
          <span className="id-corner bl" aria-hidden="true" /><span className="id-corner br" aria-hidden="true" />
          <span className="id-badge">Done</span>
          {frame.file && <span className="id-file" title={frame.file}>{frame.file}</span>}
        </div>
        <div className="id-result ing-frame-result" key={frame.image_id}>
          <div className={`ing-stars-big mono${stars ? '' : ' none'}`}>{stars || '☆☆☆☆☆'}</div>
          <div className="id-facts">
            <b>{verdict || 'Not measured'}</b>
            <span className="muted">{frame.label ? `Reads as ${frame.label}` : 'Not identified'}</span>
          </div>
          <span className={`ing-verdict ${frame.verdict ?? 'none'}`}>{frame.verdict ?? 'unrated'}</span>
        </div>
      </section>
      <aside className="id-right" aria-label="What the scan found">
        <section className="id-panel">
          <h4>Facts</h4>
          <dl className="ing-facts">
            {facts.map(([k, v]) => <div key={k}><dt>{k}</dt><dd className="mono">{v}</dd></div>)}
          </dl>
        </section>
        <section className="id-panel">
          <h4>Detections</h4>
          {boxes.length ? (
            <ul className="ing-det-list">
              {boxes.map((b, i) => {
                const role = boxRole(b, i, boxes, frame.af);
                return (
                  <li key={`${b.cls}:${i}`}>
                    <span>{b.cls}{role && <b className={`ing-role ${role}`}> · {roleLabel(b, role, main.byAf)}</b>}</span>
                    <span className="mono">{Math.round(b.conf * 100)}%</span>
                  </li>
                );
              })}
            </ul>
          ) : <p className="muted id-panel-empty">No boxes were kept for this frame.</p>}
        </section>
        <section className="id-panel">
          <h4>Autofocus</h4>
          <p className="muted id-panel-empty">{frame.af ? (frame.af_kind === 'grid' ? 'The dashed amber box is the union of the focus cells the camera reported in focus.' : 'The dashed amber box on the picture is where the camera focused.') : 'No autofocus area was read for this frame.'}</p>
        </section>
      </aside>
    </div>
  );
}
