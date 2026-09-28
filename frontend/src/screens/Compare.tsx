import { useEffect, useMemo, useRef, type Ref } from 'react';
import { Modal } from '../components/basics';
import { StarControl } from '../components/Stars';
import { asset } from '../lib/api';
import { burstGroups, burstPickOf, filename, frameStars, frameExcluded } from '../lib/review';
import type { ReviewModel } from '../state/useReview';
import { usePreview, type Runner } from '../state/hooks';
import { ZoomImage, type ZoomImageHandle } from '../components/ZoomImage';
import { BurstFilmstrip } from '../components/BurstFilmstrip';
import type { Frame } from '../lib/types';

type Props = {
  rv: ReviewModel; run: Runner; showBoxes: boolean; onToggleBoxes: () => void;
  right: Frame | null; activePane: 'left' | 'right'; onSetActivePane: (pane: 'left' | 'right') => void;
  /** Filmstrip click: sets the right pane, matching what the stepFrame/stepBurst hotkeys move (see Review.tsx). */
  onPickFilmstrip: (id: number) => void;
  onClose: () => void;
};

function Pane({ frame, run, rv, showBoxes, onToggleBoxes, active, onActivate, zoomRef }: {
  frame: Frame | null; run: Runner; rv: ReviewModel; showBoxes: boolean; onToggleBoxes: () => void;
  active: boolean; onActivate: () => void; zoomRef: Ref<ZoomImageHandle>;
}) {
  const preview = usePreview(frame?.id ?? null, run);
  const dets = frame ? rv.facts.get(frame.id)?.dets ?? [] : [];
  const stars = frame ? frameStars(frame) : 0;
  return (
    <div className={`compare-pane${active ? ' active' : ''}`} onClick={onActivate}>
      {frame ? (
        <>
          <div className="frame-stage-big">
            <ZoomImage ref={zoomRef} frame={frame} dets={dets} src={asset(preview) ?? asset(frame.thumb_path)} alt={filename(frame.path)} showBoxes={showBoxes} onToggleBoxes={onToggleBoxes} />
          </div>
          <div className="compare-pane-foot">
            <span className="mono muted" title={frame.path}>{filename(frame.path)}</span>
            <StarControl value={stars} onChange={(n) => void rv.mark({ stars: n }, frame.id)} />
            <button className={`ghost danger cut${frameExcluded(frame, dets) ? ' on' : ''}`} onClick={(e) => { e.stopPropagation(); void rv.mark({ rejected: !frameExcluded(frame, dets) }, frame.id); }}>
              {frameExcluded(frame, dets) ? 'Restore for ML' : 'Reject'}
            </button>
          </div>
        </>
      ) : (
        <div className="empty muted">No next frame or burst pick to compare against.</div>
      )}
    </div>
  );
}

/** 1.3: the selected frame vs. the next frame or its burst pick, side by side. */
export function Compare({ rv, run, showBoxes, onToggleBoxes, right, activePane, onSetActivePane, onPickFilmstrip, onClose }: Props) {
  const left = rv.frame ?? null;
  const leftZoom = useRef<ZoomImageHandle>(null);
  const rightZoom = useRef<ZoomImageHandle>(null);

  // 1.4: the filmstrip tracks whichever frame the stepFrame/stepBurst hotkeys move — that's always the right
  // pane (see Review.tsx's `verbs.current`), falling back to the left frame when there's no right pane yet.
  const filmBase = right ?? left;
  const burstFrames = useMemo(
    () => (filmBase ? burstGroups(rv.frames).find((g) => g.some((f) => f.id === filmBase.id)) ?? [filmBase] : []),
    [rv.frames, filmBase],
  );
  const burstPick = useMemo(() => (filmBase ? burstPickOf(burstFrames, rv.facts) : null), [burstFrames, filmBase, rv.facts]);

  // Zoom, labels and box-focus are replayed on both panes so they stay in sync; arrows/stars/reject
  // route through Review's `verbs.step`/`verbs.mark`, which already know this is the compare view.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA') return;
      const k = e.key.toLowerCase();
      if (k === 'z' || k === ' ') { e.preventDefault(); leftZoom.current?.toggleZoom(); rightZoom.current?.toggleZoom(); }
      else if (k === '+' || k === '=') { e.preventDefault(); leftZoom.current?.zoomIn(); rightZoom.current?.zoomIn(); }
      else if (k === '-' || k === '_') { e.preventDefault(); leftZoom.current?.zoomOut(); rightZoom.current?.zoomOut(); }
      else if (k === 'l') { e.preventDefault(); leftZoom.current?.toggleLabels(); rightZoom.current?.toggleLabels(); }
      else if (k === 'f') { e.preventDefault(); leftZoom.current?.cycleBoxFocus(); rightZoom.current?.cycleBoxFocus(); }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  if (!left) return null;
  return (
    <Modal label="Compare photos" className="viewer compare" onClose={onClose}>
      <div className="compare-view">
        <Pane frame={left} run={run} rv={rv} showBoxes={showBoxes} onToggleBoxes={onToggleBoxes} active={activePane === 'left'} onActivate={() => onSetActivePane('left')} zoomRef={leftZoom} />
        <Pane frame={right} run={run} rv={rv} showBoxes={showBoxes} onToggleBoxes={onToggleBoxes} active={activePane === 'right'} onActivate={() => onSetActivePane('right')} zoomRef={rightZoom} />
      </div>
      {burstFrames.length > 1 && burstPick && (
        <BurstFilmstrip
          frames={burstFrames}
          pickId={burstPick.id}
          selectedId={filmBase?.id ?? null}
          culled={(f) => frameExcluded(f, rv.facts.get(f.id)?.dets ?? [])}
          onSelect={onPickFilmstrip}
        />
      )}
      <button className="ghost small compare-close" autoFocus onClick={onClose}>Close</button>
    </Modal>
  );
}
