import { useEffect, useState, useRef } from 'react';
import { asset, call } from '../lib/api';
import { filename, frameStars, frameExcluded } from '../lib/review';
import type { Detection, Frame, KnownVehicle, MarkValues } from '../lib/types';
import { StarControl } from './Stars';
import { LoadableImage } from './Media';
import { DetectionsTable, getCategory, type CategoryFilter } from './DetectionsTable';

export type ReviewKind = 'portrait' | 'motorsport' | 'motorsport+people' | 'mixed';

type Props = {
  frame: Frame | null;
  detections: Detection[];
  scanning: boolean;
  allDetections?: Detection[];
  allFrames?: Frame[];
  known?: KnownVehicle[];
  kind?: ReviewKind;
  onMark: (values: MarkValues) => void;
  onOpen: () => void;
  onEdit: (id: number, updates: Record<string, string | string[] | null>) => unknown;
};

/** Portrait albums only ever care about people/face/eye rows; car-only albums only ever care about the vehicle. */
function filterByKind(list: Detection[], kind: ReviewKind): Detection[] {
  if (kind === 'portrait') return list.filter((d) => getCategory(d) === 'person' || getCategory(d) === 'landmark');
  if (kind === 'motorsport') return list.filter((d) => getCategory(d) === 'vehicle' || getCategory(d) === 'plate');
  return list;
}

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
  kind = 'mixed',
  onMark,
  onOpen,
  onEdit,
}: Props) {
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [hoveredId, setHoveredId] = useState<number | null>(null);
  const [categoryFilter, setCategoryFilter] = useState<CategoryFilter>(kind === 'portrait' ? 'people' : 'all');
  const shownDetections = filterByKind(detections, kind);
  const shownAllDetections = filterByKind(allDetections, kind);
  const [width, setWidth] = useState<number>(getStoredWidth);
  const [resizing, setResizing] = useState(false);
  const [guess, setGuess] = useState<string | null>(null);
  useEffect(() => setGuess(null), [frame?.path]);
  // Asks the Settings model about this one frame; shown only, never stored.
  const identify = async (path: string) => {
    setGuess('Asking…');
    try {
      const v = await call<Record<string, string | null>>('describe_image', { path });
      setGuess([v.make, v.model, v.colour, v.number && `#${v.number}`].filter(Boolean).join(' · ') || 'Nothing recognised');
    } catch (e) {
      setGuess(`Failed: ${e}`);
    }
  };
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

      {kind !== 'portrait' && (
        <div className="frame-side-foot">
          <button className="ghost small" disabled={guess === 'Asking…'} onClick={() => void identify(frame.path)}
            title="Ask the vision model in Settings about this frame. Nothing is saved.">Identify</button>
          {guess && <span className="mono" style={{ fontSize: 12 }}>{guess}</span>}
        </div>
      )}

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
        detections={shownDetections}
        selectedId={selectedId}
        hoveredId={hoveredId}
        categoryFilter={categoryFilter}
        onSelect={setSelectedId}
        onHover={setHoveredId}
        onFilterChange={setCategoryFilter}
        onEdit={onEdit}
        allDetections={shownAllDetections}
        allFrames={allFrames}
        known={known}
        frameBurstKey={frame.burst_key}
        emptyMessage={frame.status === 'done' ? 'No subjects found in this frame' : undefined}
      />
    </aside>
  );
}
