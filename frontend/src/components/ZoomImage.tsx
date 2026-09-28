import { forwardRef, useImperativeHandle, useMemo, useRef, useState, type CSSProperties } from 'react';
import { boxOf, plateBoxOf } from '../lib/review';
import { ZoomPanImage, type ZoomPanRef } from './Media';
import { getCategory, type CategoryFilter } from './DetectionsTable';
import type { Detection, Frame } from '../lib/types';

export type ZoomImageHandle = {
  toggleZoom: () => void;
  zoomIn: () => void;
  zoomOut: () => void;
  toggleLabels: () => void;
  /** Zoom to the detected subject box; repeat calls cycle vehicle -> plate -> face -> fit. */
  cycleBoxFocus: () => void;
};

type Props = {
  frame: Frame;
  dets: Detection[];
  src?: string;
  alt: string;
  showBoxes: boolean;
  onToggleBoxes: () => void;
  selectedDetId?: number | null;
  hoveredDetId?: number | null;
  onSelectDet?: (id: number | null) => void;
  onHoverDet?: (id: number | null) => void;
  categoryFilter?: CategoryFilter;
};

/** One zoomable/pannable photo stage with the detection-box overlay; shared by Viewer and Compare. */
export const ZoomImage = forwardRef<ZoomImageHandle, Props>(function ZoomImage(
  { frame, dets, src, alt, showBoxes, onToggleBoxes, selectedDetId = null, hoveredDetId = null, onSelectDet, onHoverDet, categoryFilter = 'all' },
  ref,
) {
  const zoomRef = useRef<ZoomPanRef>(null);
  const [showLabels, setShowLabels] = useState(true);
  const [focusIndex, setFocusIndex] = useState<number | null>(null);

  const visibleBoxes = useMemo(() => dets.filter((d) => {
    const cat = getCategory(d);
    if (categoryFilter === 'vehicles') return cat === 'vehicle' || cat === 'plate';
    if (categoryFilter === 'people') return cat === 'person';
    if (categoryFilter === 'landmarks') return cat === 'landmark';
    // 'all': show vehicles, people, and plates; landmarks (eyes) are hidden unless selected/hovered.
    if (cat === 'landmark') return selectedDetId === d.id || hoveredDetId === d.id;
    return true;
  }), [dets, categoryFilter, selectedDetId, hoveredDetId]);

  // 1.5: vehicle -> plate (from attributes, no separate detection row) -> face, in that order.
  const focusBoxes = useMemo(() => {
    const vehicle = dets.find((d) => getCategory(d) === 'vehicle');
    const face = dets.find((d) => (d.region_type ?? '') === 'face');
    const boxes: [number, number, number, number][] = [];
    if (vehicle) { const b = boxOf(vehicle, frame); if (b) boxes.push(b); }
    if (vehicle) { const b = plateBoxOf(vehicle, frame); if (b) boxes.push(b); }
    if (face) { const b = boxOf(face, frame); if (b) boxes.push(b); }
    return boxes;
  }, [dets, frame]);

  useImperativeHandle(ref, () => ({
    toggleZoom: () => { setFocusIndex(null); zoomRef.current?.toggleZoom(); },
    zoomIn: () => zoomRef.current?.zoomIn(),
    zoomOut: () => zoomRef.current?.zoomOut(),
    toggleLabels: () => setShowLabels((v) => !v),
    cycleBoxFocus: () => {
      if (!focusBoxes.length) return;
      const next = focusIndex == null ? 0 : focusIndex + 1 >= focusBoxes.length ? null : focusIndex + 1;
      setFocusIndex(next);
      if (next == null) zoomRef.current?.reset();
      else zoomRef.current?.zoomToBox(focusBoxes[next]);
    },
  }), [focusBoxes, focusIndex]);

  const ratio = frame.width && frame.height ? frame.width / frame.height : 3 / 2;

  return (
    <div className="stage-inner" style={{ '--ar': ratio } as CSSProperties}>
      <ZoomPanImage ref={zoomRef} src={src} alt={alt}>
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
                  onClick={(e) => { e.stopPropagation(); onSelectDet?.(isSelected ? null : d.id); }}
                  onMouseEnter={() => onHoverDet?.(d.id)}
                  onMouseLeave={() => onHoverDet?.(null)}
                >
                  {shouldShowLabel && <span className={`box-label label-${cat}`}>{d.cls} {d.sharpness.toFixed(2)}</span>}
                </div>
              );
            })}
          </div>
        )}
      </ZoomPanImage>

      <div className="viewer-stage-controls">
        <button type="button" className={`stage-pill-btn${showBoxes ? ' on' : ''}`} onClick={onToggleBoxes} title="Toggle detection boxes (B)">
          Outlines <kbd>B</kbd>
        </button>
        {showBoxes && (
          <button type="button" className={`stage-pill-btn${showLabels ? ' on' : ''}`} onClick={() => setShowLabels((v) => !v)} title="Toggle box text labels (L)">
            Labels <kbd>L</kbd>
          </button>
        )}
      </div>
    </div>
  );
});
