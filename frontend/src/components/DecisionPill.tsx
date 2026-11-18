import { applyDecision } from '../lib/decisions.mjs';
import type { Frame, MarkValues } from '../lib/types';

/**
 * Two mutually exclusive segments on a frame cell: ✓ keeps, ✗ culls. Both clicks stop at the pill, so they never
 * open the photo or change the focused frame; clicking the segment that is already on clears that flag.
 */
export function DecisionPill({ frame, onDecide, className }: {
  frame: Frame;
  onDecide: (values: MarkValues) => void;
  className?: string;
}) {
  const kept = Boolean(frame.picked);
  const culled = Boolean(frame.rejected);
  return (
    <span className={`decision-pill${className ? ` ${className}` : ''}`} role="group" aria-label="Keep or cull this photo"
      onClick={(e) => e.stopPropagation()} onDoubleClick={(e) => e.stopPropagation()}>
      <button type="button" className={`decision-seg keep${kept ? ' on' : ''}`} aria-pressed={kept}
        aria-label={kept ? 'Kept. Click to clear' : 'Keep this photo'}
        title={kept ? 'Kept (P). Click again to clear' : 'Keep this photo (P)'}
        onClick={(e) => { e.stopPropagation(); onDecide(kept ? { picked: false } : applyDecision(frame, 'keep')); }}>
        ✓
      </button>
      <button type="button" className={`decision-seg cull${culled ? ' on' : ''}`} aria-pressed={culled}
        aria-label={culled ? 'Culled. Click to clear' : 'Cull this photo'}
        title={culled ? 'Culled (X). Click again to clear' : 'Cull this photo (X)'}
        onClick={(e) => { e.stopPropagation(); onDecide(culled ? { rejected: false } : applyDecision(frame, 'cull')); }}>
        ✗
      </button>
    </span>
  );
}
