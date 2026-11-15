import { useEffect, useRef } from 'react';
import type { MarkValues, Page } from '../lib/types';

export type Hotkeys = {
  page: Page;
  viewerOpen: boolean;
  /** Read at key time, so they describe the frame that is selected now, not the one at the last render. */
  rejected: () => boolean;
  picked: () => boolean;
  /** Left/right (and j/k): move within the current burst, clamped at its edges (flat view: one frame). */
  stepFrame: (delta: number) => void;
  /** Up/down: jump to the previous/next burst, landing on its lead (flat view: one row). */
  stepBurst: (delta: number) => void;
  /** Tab / Shift+Tab: done with this burst, go to the next (previous) one. */
  nextBurst: (delta: number) => void;
  /** N: the next frame nobody has decided on yet. */
  nextUndecided: () => void;
  /** Enter: expand the stack in place, or open the loupe. */
  enter: () => void;
  mark: (values: MarkValues) => void;
  openViewer: () => void;
  closeOverlays: () => void;
  toggleHelp: () => void;
  toggleBoxes: () => void;
  /** 1.3: G = grid, E = loupe (Viewer), C = compare. */
  setView: (view: 'grid' | 'loupe' | 'compare') => void;
  /** Train-page verbs; `rate(0)` is "can't tell". */
  train: { rate: (stars: number) => void; pan: () => void; undo: () => void; zoom: () => void };
};

/** One window-level key handler; it always sees the latest props and never fires from a text field. */
export function useHotkeys(keys: Hotkeys) {
  const latest = useRef(keys);
  useEffect(() => { latest.current = keys; });
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest?.('input,textarea,select') || e.ctrlKey || e.metaKey || e.altKey) return;
      const h = latest.current;
      const k = e.key.toLowerCase();
      // Enter and Space still press a focused toolbar or dialog button.
      const inChrome = Boolean(target?.closest?.('.toolbar, dialog')) && Boolean(target?.closest?.('button, a, summary'));
      if (k === 'escape') { h.closeOverlays(); return; }
      if (k === '?') { e.preventDefault(); h.toggleHelp(); return; }
      if (h.page !== 'Review' && h.page !== 'Train') return;
      if (k === 'tab' && h.page === 'Review' && !target?.closest?.('.toolbar')) { e.preventDefault(); h.nextBurst(e.shiftKey ? -1 : 1); return; }
      if (k === 'arrowright' || k === 'j') { e.preventDefault(); h.stepFrame(1); }
      else if (k === 'arrowleft' || k === 'k') { e.preventDefault(); h.stepFrame(-1); }
      else if (k === 'arrowdown') { e.preventDefault(); h.stepBurst(1); }
      else if (k === 'arrowup') { e.preventDefault(); h.stepBurst(-1); }
      else if (k === 'b') h.toggleBoxes();
      else if (h.page === 'Review') {
        if (/^[0-5]$/.test(k)) { e.preventDefault(); h.mark({ stars: Number(k) }); }
        else if (k === 'x' || k === 'delete') h.mark(h.rejected() ? { rejected: false } : { rejected: true });
        else if (k === 'p') h.mark(h.picked() ? { picked: false } : { picked: true, rejected: false });
        else if (k === 'u') h.mark({ stars: null, rejected: false, picked: false });
        else if (k === 'n') h.nextUndecided();
        else if (k === 'enter' && !inChrome) { e.preventDefault(); h.enter(); }
        else if (k === ' ' && !document.querySelector('dialog[open]') && !inChrome) { e.preventDefault(); h.openViewer(); }
        else if (k === 'g') h.setView('grid');
        else if (k === 'e') h.setView('loupe');
        else if (k === 'c') h.setView('compare');
      } else if (/^[1-5]$/.test(k)) h.train.rate(Number(k));
      else if (k === 'x') h.train.rate(0);
      else if (k === 'p') h.train.pan();
      else if (k === 'u') h.train.undo();
      else if (k === 'z' || k === ' ') { e.preventDefault(); h.train.zoom(); }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
}
