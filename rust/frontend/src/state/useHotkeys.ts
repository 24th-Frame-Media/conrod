import { useEffect, useRef } from 'react';
import type { MarkValues, Page } from '../lib/types';

export type Hotkeys = {
  page: Page;
  viewerOpen: boolean;
  rejected: boolean;
  step: (delta: number) => void;
  mark: (values: MarkValues) => void;
  openViewer: () => void;
  closeOverlays: () => void;
  toggleHelp: () => void;
  toggleBoxes: () => void;
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
      if (k === 'escape') { h.closeOverlays(); return; }
      if (k === '?') { e.preventDefault(); h.toggleHelp(); return; }
      if (h.page !== 'Review' && h.page !== 'Train') return;
      if (['arrowright', 'arrowdown', 'j'].includes(k)) { e.preventDefault(); h.step(1); }
      else if (['arrowleft', 'arrowup', 'k'].includes(k)) { e.preventDefault(); h.step(-1); }
      else if (k === 'b') h.toggleBoxes();
      else if (h.page === 'Review') {
        if (/^[0-5]$/.test(k)) { e.preventDefault(); h.mark({ stars: Number(k) }); }
        else if (k === 'x' || k === 'delete') h.mark({ rejected: !h.rejected });
        else if (k === 'u') h.mark({ stars: null, rejected: false });
        else if (k === 'enter' && !h.viewerOpen) { e.preventDefault(); h.openViewer(); }
      } else if (/^[1-5]$/.test(k)) h.train.rate(Number(k));
      else if (k === 'x') h.train.rate(0);
      else if (k === 'p') h.train.pan();
      else if (k === 'u') h.train.undo();
      else if (k === 'z') h.train.zoom();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
}
