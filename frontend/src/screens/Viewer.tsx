import { useState, useEffect, useMemo, useRef } from 'react';
import { Modal } from '../components/basics';
import { StarControl } from '../components/Stars';
import { asset } from '../lib/api';
import { filename, frameStars, frameExcluded } from '../lib/review';
import type { Stack } from '../lib/stacks.mjs';
import type { Frame } from '../lib/types';
import type { ReviewModel } from '../state/useReview';
import { usePreview, type Runner } from '../state/hooks';
import { ZoomImage, type ZoomImageHandle } from '../components/ZoomImage';
import { DetectionsTable, type CategoryFilter } from '../components/DetectionsTable';
import { BurstFilmstrip } from '../components/BurstFilmstrip';

type Props = {
  rv: ReviewModel;
  run: Runner;
  showBoxes: boolean;
  onToggleBoxes: () => void;
  onClose: () => void;
  /** The current burst, the album's keepers, a step along the navigation Review owns, and where the frame sits in it. */
  stack: Stack<Frame> | null;
  keeperIds: ReadonlySet<number>;
  onStep: (delta: number) => void;
  position: string;
};

/** The full frame, with interactive detections table and color-coded outline overlay. */
export function Viewer({ rv, run, showBoxes, onToggleBoxes, onClose, stack, keeperIds, onStep, position }: Props) {
  const { frame, facts, mark } = rv;
  const preview = usePreview(frame?.id ?? null, run);
  const zoomImgRef = useRef<ZoomImageHandle>(null);
  const [selectedDetId, setSelectedDetId] = useState<number | null>(null);
  const [hoveredDetId, setHoveredDetId] = useState<number | null>(null);
  const [categoryFilter, setCategoryFilter] = useState<CategoryFilter>('all');

  // Keyboard shortcuts: L toggles box labels, Space/Z toggles 1:1 zoom, +/- steps zoom, F zooms to the detected box.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA') return;
      if (e.key === 'l' || e.key === 'L') {
        e.preventDefault();
        zoomImgRef.current?.toggleLabels();
      } else if (e.key === 'z' || e.key === 'Z' || e.key === ' ') {
        e.preventDefault();
        zoomImgRef.current?.toggleZoom();
      } else if (e.key === '+' || e.key === '=') {
        e.preventDefault();
        zoomImgRef.current?.zoomIn();
      } else if (e.key === '-' || e.key === '_') {
        e.preventDefault();
        zoomImgRef.current?.zoomOut();
      } else if (e.key === 'f' || e.key === 'F') {
        e.preventDefault();
        zoomImgRef.current?.cycleBoxFocus();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  if (!frame) return null;
  const dets = facts.get(frame.id)?.dets ?? [];
  const stars = frameStars(frame);

  return (
    <Modal label="Photo viewer" className="viewer" onClose={onClose}>
      <div className="frame-view">
        <div className="frame-stage-big">
          <ZoomImage
            ref={zoomImgRef}
            frame={frame}
            dets={dets}
            src={asset(preview) ?? asset(frame.thumb_path)}
            alt={filename(frame.path)}
            showBoxes={showBoxes}
            onToggleBoxes={onToggleBoxes}
            selectedDetId={selectedDetId}
            hoveredDetId={hoveredDetId}
            onSelectDet={setSelectedDetId}
            onHoverDet={setHoveredDetId}
            categoryFilter={categoryFilter}
          />

          <div className="frame-nav previous" role="group" aria-label="Previous photos">
            <button
              title="First frame of the burst"
              aria-label="First frame"
              onClick={() => {
                setSelectedDetId(null);
                onStep(Number.NEGATIVE_INFINITY);
              }}
            >
              First
            </button>
            <button
              className="frame-step"
              title="Previous frame (←)"
              aria-label="Previous frame"
              onClick={() => {
                setSelectedDetId(null);
                onStep(-1);
              }}
            >
              ‹
            </button>
          </div>
          <div className="frame-nav following" role="group" aria-label="Next photos">
            <button
              className="frame-step"
              title="Next frame (→)"
              aria-label="Next frame"
              onClick={() => {
                setSelectedDetId(null);
                onStep(1);
              }}
            >
              ›
            </button>
            <button
              title="Last frame of the burst"
              aria-label="Last frame"
              onClick={() => {
                setSelectedDetId(null);
                onStep(Number.POSITIVE_INFINITY);
              }}
            >
              Last
            </button>
          </div>
        </div>

        <aside className="frame-side">
          <div className="frame-side-head">
            <div className="frame-title-wrap">
              <h3 title={frame.path}>{filename(frame.path)}</h3>
              <p className="frame-meta-sub">
                {frame.width ? `${frame.width} × ${frame.height}` : '—'}
                {frame.rating != null ? ` · Focus ${frame.rating.toFixed(3)}` : ''}
                {frame.burst_key != null ? ` · Burst #${frame.burst_key}` : ''}
              </p>
            </div>
            <button className="ghost small" autoFocus onClick={onClose}>
              Close
            </button>
          </div>

          {stack && stack.frames.length > 1 && (
            <BurstFilmstrip
              frames={stack.frames}
              keeperIds={keeperIds}
              selectedId={frame.id}
              culled={(f) => frameExcluded(f, facts.get(f.id)?.dets ?? [])}
              onSelect={(id) => rv.setSelected(id)}
              onDecide={(id, values) => void rv.mark(values, id)}
            />
          )}

          <DetectionsTable
            detections={dets}
            selectedId={selectedDetId}
            hoveredId={hoveredDetId}
            categoryFilter={categoryFilter}
            onSelect={setSelectedDetId}
            onHover={setHoveredDetId}
            onFilterChange={setCategoryFilter}
          />

          <p id="frame-caption" className="muted mono" title={frame.path}>
            {frame.path}
          </p>

          <div className="frame-side-foot">
            <StarControl value={stars} onChange={(n) => void mark({ stars: n })} />
            <button
              className="ghost small"
              title="Back to the measured rating (U)"
              onClick={() => void mark({ stars: null, rejected: false })}
            >
              Clear
            </button>
            <div className="spacer" />
            <button
              className={`ghost small${frame.picked ? ' on' : ''}`}
              aria-pressed={Boolean(frame.picked)}
              title="Keeper (P)"
              onClick={() => void mark(frame.picked ? { picked: false } : { picked: true, rejected: false })}
            >
              {frame.picked ? 'Keeper ✓' : 'Keeper'}
            </button>
            <button
              className={`ghost danger cut${frameExcluded(frame, dets) ? ' on' : ''}`}
              onClick={() => void mark({ rejected: !frameExcluded(frame, dets) })}
            >
              {frameExcluded(frame, dets) ? 'Restore for ML' : 'Reject'}
            </button>
          </div>

          <div className="frame-side-bottom-info">
            <label className="check-line">
              <input type="checkbox" checked={showBoxes} onChange={onToggleBoxes} />
              Show outlines <kbd>B</kbd>
            </label>
            <p className="muted frame-pos">
              {position}
            </p>
          </div>
        </aside>
      </div>
    </Modal>
  );
}
