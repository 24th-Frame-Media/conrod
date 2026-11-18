import { useEffect, useRef } from 'react';
import { applyDecision } from '../lib/decisions.mjs';
import type { MarkValues, Page } from '../lib/types';

export type Hotkeys = {
  page: Page;
  viewerOpen: boolean;
  /** Read at key time, so they describe the frame that is selected now, not the one at the last render. */
  rejected: () => boolean;
  picked: () => boolean;
  /** Left/right (and j/k): move within the current burst, clamped at its edges (flat view: one frame). */
  stepFrame: (delta: number, repeat?: boolean) => void;
  /** Up/down: jump to the previous/next burst, landing on its lead (flat view: one row). */
  stepBurst: (delta: number) => void;
  /** Tab / Shift+Tab: done with this burst, go to the next (previous) one. */
  nextBurst: (delta: number) => void;
  /** N: the next frame nobody has decided on yet. */
  nextUndecided: () => void;
  /** Enter: expand the stack in place, or open the loupe. */
  enter: () => void;
  mark: (values: MarkValues) => void;
  /** B / Shift+B while culling: decide the whole stack around the focused frame. */
  stackAction: (mode: 'keep' | 'cull') => void;
  /** Ctrl+Z / Ctrl+Shift+Z while culling: undo (redo) the last decision. */
  undo: () => void;
  redo: () => void;
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
      if (target?.closest?.('input,textarea,select') || e.altKey) return;
      const h = latest.current;
      // Undo and redo are the only shortcuts with a modifier.
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z') {
        if (h.page !== 'Review') return;
        e.preventDefault();
        if (e.shiftKey) h.redo(); else h.undo();
        return;
      }
      if (e.ctrlKey || e.metaKey) return;
      const k = e.key.toLowerCase();
      // Enter and Space still press a focused toolbar or dialog button.
      const inChrome = Boolean(target?.closest?.('.toolbar, dialog')) && Boolean(target?.closest?.('button, a, summary'));
      if (k === 'escape') { h.closeOverlays(); return; }
      if (k === '?') { e.preventDefault(); h.toggleHelp(); return; }
      if (h.page !== 'Review' && h.page !== 'Train') return;
      if (k === 'tab' && h.page === 'Review' && !target?.closest?.('.toolbar')) { e.preventDefault(); h.nextBurst(e.shiftKey ? -1 : 1); return; }
      if (k === 'arrowright' || k === 'j') { e.preventDefault(); h.stepFrame(1, e.repeat); }
      else if (k === 'arrowleft' || k === 'k') { e.preventDefault(); h.stepFrame(-1, e.repeat); }
      else if (k === 'arrowdown') { e.preventDefault(); h.stepBurst(1); }
      else if (k === 'arrowup') { e.preventDefault(); h.stepBurst(-1); }
      else if (k === 'b' && h.page !== 'Review') h.toggleBoxes();
      else if (h.page === 'Review') {
        // A modal owns its own keys; the pills and the toolbar are the way to decide while one is open.
        if (document.querySelector('dialog[open]')) return;
        if (/^[0-5]$/.test(k)) { e.preventDefault(); h.mark({ stars: Number(k) }); }
        else if (k === 'x' || k === 'delete') h.mark(applyDecision(null, 'cull'));
        else if (k === 'p') h.mark(applyDecision(null, 'keep'));
        else if (k === 'u') h.mark({ ...applyDecision(null, 'clear'), stars: null });
        else if (k === 'b') { e.preventDefault(); h.stackAction(e.shiftKey ? 'cull' : 'keep'); }
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
