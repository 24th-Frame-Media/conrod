import { useEffect, useId, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { Portal, useBackdropClose } from './basics';
import { FOCUSABLE, createSheetStack, trapTarget } from '../lib/focusTrap.mjs';

const sheets = createSheetStack();

/**
 * The one shell every overlay sheet uses (Export, Shoot settings, Statistics, Identifications,
 * Lightroom import, What's new, Import). It is a real modal dialog:
 *  - `role="dialog"` and `aria-modal`, named by the sheet's own heading (`label` is the fallback);
 *  - Esc closes it, and only the sheet on top answers (a native `<dialog>` open above keeps its own Esc);
 *  - Tab stays inside it, and the first Tab from outside lands on its first control;
 *  - focus moves in when it opens and returns to whatever had it when it closes;
 *  - a click closes it only when it starts on the backdrop, so text can be selected without closing.
 * Mount it only while the sheet is open.
 */
export function Sheet({ label, onClose, className = '', style, children }: {
  label: string; onClose: () => void; className?: string; style?: CSSProperties; children: ReactNode;
}) {
  const backdrop = useBackdropClose(onClose);
  const ref = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  close.current = onClose;
  const fallbackId = useId();
  const [titleId, setTitleId] = useState<string | undefined>();

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return undefined;
    const token = {};
    sheets.push(token);
    const returnTo = document.activeElement as HTMLElement | null;

    // Name the dialog by its heading when it has one.
    const heading = dialog.querySelector<HTMLElement>('h1, h2, h3');
    if (heading) {
      if (!heading.id) heading.id = `${fallbackId}-title`;
      setTitleId(heading.id);
    }

    const focusables = () => Array.from(dialog.querySelectorAll<HTMLElement>(FOCUSABLE))
      // Only real Tab stops: shown (closed <details> content and hidden inputs are not) and not roving (tabindex -1).
      .filter((el) => el.tabIndex >= 0 && !el.closest('[hidden]')
        && (typeof el.checkVisibility === 'function' ? el.checkVisibility() : el.getClientRects().length > 0))
      // A radio group is one Tab stop: its checked button (or the first when none is checked).
      .filter((el) => {
        if (!(el instanceof HTMLInputElement) || el.type !== 'radio' || !el.name) return true;
        const group = Array.from(dialog.querySelectorAll<HTMLInputElement>(`input[type="radio"]`)).filter((r) => r.name === el.name && !r.disabled);
        return el === (group.find((r) => r.checked) ?? group[0]);
      });

    // Move focus in unless something inside (an autoFocus field) already has it. Skip the close button.
    if (!dialog.contains(document.activeElement)) {
      const first = focusables().find((el) => !el.classList.contains('import-close')) ?? dialog;
      first.focus({ preventScroll: true });
    }

    const onKey = (e: KeyboardEvent) => {
      if (!sheets.isTop(token)) return;
      if (e.key === 'Escape') {
        // A native dialog (Confirm, Compare) opened over the sheet owns Esc.
        if (document.querySelector('dialog[open]')) return;
        e.preventDefault();
        e.stopPropagation();
        close.current();
        return;
      }
      if (e.key !== 'Tab') return;
      const list = focusables();
      const at = list.indexOf(document.activeElement as HTMLElement);
      const to = trapTarget(list.length, at, e.shiftKey);
      if (list.length === 0) { e.preventDefault(); dialog.focus(); return; }
      if (to != null) { e.preventDefault(); list[to].focus(); }
    };
    // Capture, so the app's own Esc and Tab shortcuts never see a key the sheet used.
    document.addEventListener('keydown', onKey, true);
    return () => {
      document.removeEventListener('keydown', onKey, true);
      sheets.remove(token);
      if (returnTo && returnTo.isConnected) returnTo.focus({ preventScroll: true });
    };
  }, [fallbackId]);

  return (
    <Portal>
      <div className="import-overlay" {...backdrop}>
        <div ref={ref} className={`import-dialog ${className}`.trim()} style={style} role="dialog" aria-modal="true"
          aria-labelledby={titleId} aria-label={titleId ? undefined : label} tabIndex={-1}>
          {children}
        </div>
      </div>
    </Portal>
  );
}
