import { useEffect, useState, useRef } from 'react';
import { asset } from '../lib/api';
import { filename, frameStars, frameExcluded } from '../lib/review';
import type { Detection, Frame, KnownVehicle, MarkValues } from '../lib/types';
import { StarControl } from './Stars';
import { LoadableImage } from './Media';
import { DetectionsTable, type CategoryFilter } from './DetectionsTable';

type Props = {
  frame: Frame | null;
  detections: Detection[];
  scanning: boolean;
  allDetections?: Detection[];
  allFrames?: Frame[];
  known?: KnownVehicle[];
  onMark: (values: MarkValues) => void;
  onOpen: () => void;
  onEdit: (id: number, updates: Record<string, string | string[] | null>) => unknown;
};

const DEFAULT_WIDTH = 360;
const MIN_WIDTH = 280;
const MAX_WIDTH = 800;

function getStoredWidth(): number {
  try {
    const val = localStorage.getItem('conrod:inspector-width');
    if (val) {
      const n = Number(val);
      if (Number.isFinite(n) && n >= MIN_WIDTH && n <= MAX_WIDTH) return n;
    }
  } catch { /* ignore */ }
  return DEFAULT_WIDTH;
}

/** The selected photograph: thumbnail, stars, reject, measurements, and expandable detections table. */
export function Inspector({
  frame,
  detections,
  scanning,
  allDetections = [],
  allFrames = [],
  known = [],
  onMark,
  onOpen,
  onEdit,
}: Props) {
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [hoveredId, setHoveredId] = useState<number | null>(null);
  const [categoryFilter, setCategoryFilter] = useState<CategoryFilter>('all');
  const [width, setWidth] = useState<number>(getStoredWidth);
  const [resizing, setResizing] = useState(false);
  const widthRef = useRef(width);
  widthRef.current = width;

  useEffect(() => {
    try {
      localStorage.setItem('conrod:inspector-width', String(width));
    } catch { /* ignore */ }
  }, [width]);

  const handleMouseDown = (e: React.MouseEvent) => {
    e.preventDefault();
    setResizing(true);
    const startX = e.clientX;
    const startWidth = widthRef.current;

    const onMouseMove = (ev: MouseEvent) => {
      const delta = startX - ev.clientX;
      const maxAllowed = Math.min(MAX_WIDTH, Math.round(window.innerWidth * 0.65));
      const newWidth = Math.max(MIN_WIDTH, Math.min(maxAllowed, startWidth + delta));
      setWidth(newWidth);
    };

    const onMouseUp = () => {
      setResizing(false);
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onMouseUp);
    };

    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', onMouseUp);
  };

  const handleResetWidth = () => {
    setWidth(DEFAULT_WIDTH);
  };

  if (!frame) {
    return (
      <aside className={`inspector${resizing ? ' resizing' : ''}`} style={{ width: `${width}px`, flex: `0 0 ${width}px` }}>
        <div
          className="inspector-resizer"
          onMouseDown={handleMouseDown}
          onDoubleClick={handleResetWidth}
          title="Drag to resize inspector · Double-click to reset"
        />
        <p className="muted">{scanning ? 'Results appear here while the scan runs.' : 'Select a photograph.'}</p>
      </aside>
    );
  }

  const stars = frameStars(frame);

  return (
    <aside
      className={`inspector${resizing ? ' resizing' : ''}`}
      style={{ width: `${width}px`, flex: `0 0 ${width}px` }}
      aria-label="Selected photograph"
    >
      <div
        className="inspector-resizer"
        onMouseDown={handleMouseDown}
        onDoubleClick={handleResetWidth}
        title="Drag to resize inspector · Double-click to reset"
      />
      <div className="frame-side-head">
        <h3 title={frame.path}>{filename(frame.path)}</h3>
        {frame.status !== 'done' && <span className="tag">{frame.status}</span>}
      </div>

      <button className="inspect-image" onClick={onOpen} title="Open the full frame (Enter)">
        <LoadableImage src={asset(frame.thumb_path)} alt={filename(frame.path)} />
      </button>

      <div className="frame-side-foot">
        <StarControl value={stars} onChange={(n) => onMark({ stars: n })} />
        <button
          className="ghost small"
          title="Back to the measured rating (U)"
          onClick={() => onMark({ stars: null, rejected: false })}
        >
          Clear
        </button>
        <div className="spacer" />
        <button
          className={`ghost danger cut${frameExcluded(frame, detections) ? ' on' : ''}`}
          onClick={() => onMark({ rejected: !frameExcluded(frame, detections) })}
        >
          {frameExcluded(frame, detections) ? 'Restore for ML' : 'Reject'}
        </button>
      </div>

      <dl className="meta">
        <dt>Size</dt>
        <dd>{frame.width ? `${frame.width} × ${frame.height}` : '—'}</dd>
        <dt>Focus</dt>
        <dd>{frame.rating != null ? frame.rating.toFixed(3) : '—'}</dd>
        <dt>Burst</dt>
        <dd>{frame.burst_key ?? '—'}</dd>
      </dl>

      {frame.error && <p className="error-text">{frame.error}</p>}

      <DetectionsTable
        detections={detections}
        selectedId={selectedId}
        hoveredId={hoveredId}
        categoryFilter={categoryFilter}
        onSelect={setSelectedId}
        onHover={setHoveredId}
        onFilterChange={setCategoryFilter}
        onEdit={onEdit}
        allDetections={allDetections}
        allFrames={allFrames}
        known={known}
        frameBurstKey={frame.burst_key}
        emptyMessage={frame.status === 'done' ? 'No subjects found in this frame' : undefined}
      />
    </aside>
  );
}
