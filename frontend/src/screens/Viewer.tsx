import { useState, useEffect, useMemo, useRef, type CSSProperties } from 'react';
import { Modal } from '../components/basics';
import { StarControl } from '../components/Stars';
import { asset } from '../lib/api';
import { boxOf, filename, frameStars, frameExcluded } from '../lib/review';
import type { ReviewModel } from '../state/useReview';
import { usePreview, type Runner } from '../state/hooks';
import { ZoomPanImage, type ZoomPanRef } from '../components/Media';
import { DetectionsTable, getCategory, type CategoryFilter } from '../components/DetectionsTable';

type Props = {
  rv: ReviewModel;
  run: Runner;
  showBoxes: boolean;
  onToggleBoxes: () => void;
  onClose: () => void;
};

/** The full frame, with interactive detections table and color-coded outline overlay. */
export function Viewer({ rv, run, showBoxes, onToggleBoxes, onClose }: Props) {
  const { frame, frames, facts, step, mark } = rv;
  const preview = usePreview(frame?.id ?? null, run);
  const zoomRef = useRef<ZoomPanRef>(null);
  const [selectedDetId, setSelectedDetId] = useState<number | null>(null);
  const [hoveredDetId, setHoveredDetId] = useState<number | null>(null);
  const [categoryFilter, setCategoryFilter] = useState<CategoryFilter>('all');
  const [showLabels, setShowLabels] = useState(true);

  // Keyboard shortcut: 'L' toggles box labels, Space/Z toggles 1:1 zoom, +/- steps zoom
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA') return;
      if (e.key === 'l' || e.key === 'L') {
        e.preventDefault();
        setShowLabels((v) => !v);
      } else if (e.key === 'z' || e.key === 'Z' || e.key === ' ') {
        e.preventDefault();
        zoomRef.current?.toggleZoom();
      } else if (e.key === '+' || e.key === '=') {
        e.preventDefault();
        zoomRef.current?.zoomIn();
      } else if (e.key === '-' || e.key === '_') {
        e.preventDefault();
        zoomRef.current?.zoomOut();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  if (!frame) return null;
  const dets = facts.get(frame.id)?.dets ?? [];
  const index = frames.findIndex((f) => f.id === frame.id);
  const ratio = frame.width && frame.height ? frame.width / frame.height : 3 / 2;
  const stars = frameStars(frame);

  // Filter which bounding boxes are rendered on the photo stage
  const visibleBoxes = useMemo(() => {
    return dets.filter((d) => {
      const cat = getCategory(d);
      if (categoryFilter === 'vehicles') return cat === 'vehicle' || cat === 'plate';
      if (categoryFilter === 'people') return cat === 'person';
      if (categoryFilter === 'landmarks') return cat === 'landmark';
      // 'all': show vehicles, people, and plates; landmarks (eyes) are hidden by default
      // to avoid obscuring faces with dozens of overlapping badges unless selected or in landmark tab
      if (cat === 'landmark') return selectedDetId === d.id || hoveredDetId === d.id;
      return true;
    });
  }, [dets, categoryFilter, selectedDetId, hoveredDetId]);

  return (
    <Modal label="Photo viewer" className="viewer" onClose={onClose}>
      <div className="frame-view">
        <div className="frame-stage-big">
          <div className="stage-inner" style={{ '--ar': ratio } as CSSProperties}>
            <ZoomPanImage
              ref={zoomRef}
              src={asset(preview) ?? asset(frame.thumb_path)}
              alt={filename(frame.path)}
            >
              {showBoxes && (
                <div className="overlay">
                  {visibleBoxes.map((d) => {
                    const b = boxOf(d, frame);
                    if (!b) return null;
                    const cat = getCategory(d);
                    const isSelected = selectedDetId === d.id;
                    const isHovered = hoveredDetId === d.id;
                    const shouldShowLabel = showLabels || isSelected || isHovered;

                    return (
                      <div
                        key={d.id}
                        className={`box box-${cat}${d.panning ? ' pan' : ''}${isSelected ? ' active-box' : ''}${isHovered ? ' hovered' : ''}`}
                        style={{
                          left: `${b[0] * 100}%`,
                          top: `${b[1] * 100}%`,
                          width: `${(b[2] - b[0]) * 100}%`,
                          height: `${(b[3] - b[1]) * 100}%`,
                          pointerEvents: 'auto',
                        }}
                        onClick={(e) => {
                          e.stopPropagation();
                          setSelectedDetId(isSelected ? null : d.id);
                        }}
                        onMouseEnter={() => setHoveredDetId(d.id)}
                        onMouseLeave={() => setHoveredDetId(null)}
                      >
                        {shouldShowLabel && (
                          <span className={`box-label label-${cat}`}>
                            {d.cls} {d.sharpness.toFixed(2)}
                          </span>
                        )}
                      </div>
                    );
                  })}
                </div>
              )}
            </ZoomPanImage>
          </div>

          {/* Quick viewer controls floating over bottom of stage */}
          <div className="viewer-stage-controls">
            <button
              type="button"
              className={`stage-pill-btn${showBoxes ? ' on' : ''}`}
              onClick={onToggleBoxes}
              title="Toggle detection boxes (B)"
            >
              Outlines <kbd>B</kbd>
            </button>
            {showBoxes && (
              <button
                type="button"
                className={`stage-pill-btn${showLabels ? ' on' : ''}`}
                onClick={() => setShowLabels((v) => !v)}
                title="Toggle box text labels (L)"
              >
                Labels <kbd>L</kbd>
              </button>
            )}
          </div>

          <div className="frame-nav previous" role="group" aria-label="Previous photos">
            <button
              title="First frame"
              aria-label="First frame"
              disabled={index <= 0}
              onClick={() => {
                setSelectedDetId(null);
                step(Number.NEGATIVE_INFINITY);
              }}
            >
              First
            </button>
            <button
              className="frame-step"
              title="Previous frame (←)"
              aria-label="Previous frame"
              disabled={index <= 0}
              onClick={() => {
                setSelectedDetId(null);
                step(-1);
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
              disabled={index < 0 || index >= frames.length - 1}
              onClick={() => {
                setSelectedDetId(null);
                step(1);
              }}
            >
              ›
            </button>
            <button
              title="Last frame"
              aria-label="Last frame"
              disabled={index < 0 || index >= frames.length - 1}
              onClick={() => {
                setSelectedDetId(null);
                step(Number.POSITIVE_INFINITY);
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
              {index >= 0 ? `${(index + 1).toLocaleString()} / ${frames.length.toLocaleString()}` : ''}
            </p>
          </div>
        </aside>
      </div>
    </Modal>
  );
}
