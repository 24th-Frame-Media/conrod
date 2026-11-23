import { useEffect, useId, useRef, type MouseEvent as ReactMouseEvent, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { modelParts, startupCount, startupFraction, startupHeadline, visibleSteps } from '../lib/startup.mjs';
import { SPLASH_VERSION } from '../lib/devInfo.mjs';
import type { DevInfo, StartupStatus } from '../lib/types';

/**
 * Sheets live on `document.body`, not inside the screen: a transformed or scrolled
 * ancestor turns `position: fixed` into "relative to that ancestor", which is what
 * made the import sheet drift off screen.
 */
export function Portal({ children }: { children: ReactNode }) {
  return createPortal(children, document.body);
}

/**
 * Backdrop click that does not fire when a text selection started inside the sheet
 * and ended on the backdrop: only a press that began on the backdrop closes.
 */
export function useBackdropClose(onClose: () => void) {
  const down = useRef(false);
  return {
    onMouseDown: (e: ReactMouseEvent) => { down.current = e.target === e.currentTarget; },
    onClick: (e: ReactMouseEvent) => {
      const close = down.current && e.target === e.currentTarget;
      down.current = false;
      if (close) onClose();
    },
  };
}

const PATHS = {
  x: 'M6 6l12 12M18 6L6 18',
  min: 'M5 12h14',
  max: 'M5.5 5.5h13v13h-13z',
  restore: 'M9 9h9.5v9.5H9z M6 15V5.5h9.5',
  chevron: 'M6 9l6 6 6-6',
  keyboard: 'M3 7h18v10H3z M7 11h.01 M11 11h.01 M15 11h.01 M8 14.2h8',
  folder: 'M3 6.5h6l2 2h10V18H3z',
  images: 'M4 5h16v14H4z M4 16l5-5 4 4 3-3 4 4',
  plus: 'M12 5v14M5 12h14',
  check: 'M5 12.5l4.5 4.5L19 7.5',
  drop: 'M12 4v11 M7.5 10.5L12 15l4.5-4.5 M5 19h14',
  car: 'M4 16v-4l2-5h12l2 5v4z M4 12h16 M7.5 14.2h.01 M16.5 14.2h.01',
  paw: 'M9 11.5a1.7 2 0 100-4 1.7 2 0 000 4z M15 11.5a1.7 2 0 100-4 1.7 2 0 000 4z M5.8 15a1.5 1.8 0 100-3.6 1.5 1.8 0 000 3.6z M18.2 15a1.5 1.8 0 100-3.6 1.5 1.8 0 000 3.6z M12 20.2c-2.8 0-5-1.5-5-3.5 0-1.7 2.2-3.4 5-3.4s5 1.7 5 3.4c0 2-2.2 3.5-5 3.5z',
  person: 'M12 11a3.5 3.5 0 100-7 3.5 3.5 0 000 7z M5 20c0-3.5 3-6 7-6s7 2.5 7 6',
  plate: 'M3 7.5h18v9H3z M7 12h10',
  eye: 'M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z M12 14.5a2.5 2.5 0 100-5 2.5 2.5 0 000 5z',
  sliders: 'M4 7h9 M17 7h3 M4 17h3 M11 17h9 M15 4.5v5 M9 14.5v5',
  undo: 'M9 14L4 9l5-5 M4 9h10.5a5.5 5.5 0 010 11H11',
  hourglass: 'M7 4h10 M7 20h10 M8 4v1c0 3.5 4 4 4 7s-4 3.5-4 7v1 M16 4v1c0 3.5-4 4-4 7s4 3.5 4 7v1',
  sun: 'M12 16.5a4.5 4.5 0 100-9 4.5 4.5 0 000 9z M12 2v2 M12 20v2 M4.9 4.9l1.4 1.4 M17.7 17.7l1.4 1.4 M2 12h2 M20 12h2 M4.9 19.1l1.4-1.4 M17.7 6.3l1.4-1.4',
  moon: 'M20 14.5A8 8 0 019.5 4a8 8 0 1010.5 10.5z',
} as const;
export type IconName = keyof typeof PATHS;

/** The one icon set: 24px grid, 1.5 stroke, takes the colour of its text. */
export function Icon({ name, size = 16 }: { name: IconName; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5"
      strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d={PATHS[name]} />
    </svg>
  );
}

/** The Python brand mark: a connecting rod drawn as one stroke. */
/** A button that opens the system file dialog, in place of the browser's own "Choose File" control. */
export function FilePick({ label, accept, onFile }: { label: string; accept: string; onFile: (file: File) => void }) {
  const input = useRef<HTMLInputElement>(null);
  return (
    <>
      <button type="button" onClick={() => input.current?.click()}>{label}</button>
      <input ref={input} type="file" accept={accept} hidden onChange={(e) => { const file = e.target.files?.[0]; e.target.value = ''; if (file) onFile(file); }} />
    </>
  );
}

/**
 * The app mark (crates/conrod-app/icons/conrod-mark.svg): one forged con-rod
 * at 35 degrees, unequal eyes, a split big end, and an I-beam slot with the
 * single low kink of The Chase. The mask keeps the bores and slot transparent
 * on light and dark.
 */
export function Mark() {
  const mask = useId().replace(/:/g, '');
  return (
    <svg className="mark" viewBox="0 0 256 256" aria-hidden="true">
      <mask id={mask}>
        <rect width="256" height="256" fill="#fff" />
        <circle cx="128" cy="56" r="11" fill="#000" />
        <circle cx="128" cy="200" r="18" fill="#000" />
        <path d="M128 88 L128 124 L135 131 L135 157" stroke="#000" strokeWidth="10" strokeLinecap="round" strokeLinejoin="round" fill="none" />
        <path d="M86 200 H110 M146 200 H170" stroke="#000" strokeWidth="1.6" />
      </mask>
      <g transform="translate(128 128) rotate(35) scale(0.98) translate(-128 -136)">
        <path
          mask={`url(#${mask})`}
          fill="currentColor"
          d="M150.5 69 A26 26 0 1 0 105.5 69 C109 77 112 84 112 94 C112 122 110 148 104 162 C102 167 100.5 170.5 98.3 173.2 A40 40 0 1 0 157.7 173.2 C155.5 170.5 154 167 152 162 C146 148 144 122 144 94 C144 84 147 77 150.5 69 Z"
        />
      </g>
    </svg>
  );
}

export function Empty({ icon = 'images', title, children }: { icon?: IconName; title: string; children?: ReactNode }) {
  return (
    <div className="empty">
      <div className="empty-icon"><Icon name={icon} size={26} /></div>
      <h3>{title}</h3>
      {children}
    </div>
  );
}

/**
 * Start-up screen: what the engine is doing right now (with the model it is about, if any), a bar
 * that fills as steps finish, and the last few finished steps ticked off. A step that went wrong
 * shows amber and never blocks. Without a status yet it is the logo and an indeterminate bar.
 */
export function Splash({ gone, status, version, dev }: { gone: boolean; status?: StartupStatus | null; version?: string | null; dev?: DevInfo | null }) {
  const fraction = startupFraction(status);
  const model = modelParts(status?.model);
  const count = startupCount(status);
  const steps = visibleSteps(status);
  return (
    <div className={gone ? 'splash gone' : 'splash'} data-dev={dev ? '' : undefined} aria-hidden={gone}>
      <div className="splash-card">
        <div className="splash-mark"><Mark /></div>
        <div>
          <p className="splash-name"><b>con</b>rod</p>
          {version && <p className="splash-ver">{SPLASH_VERSION(version, Boolean(dev))}</p>}
        </div>
        {dev && (
          <div className="splash-dev">
            <span className="dev-flag">DEV</span>
            <span>Isolated instance — data stays in <code className="mono">{dev.home}</code></span>
          </div>
        )}
        <div className="splash-now" role="status">
          <p className={status?.error ? 'splash-msg warn' : 'splash-msg'}>
            <span>{startupHeadline(status)}</span>
            {count && !status?.ready && <span className="splash-count mono">{count}</span>}
          </p>
          {model && (
            <p className="splash-model">
              {model.name && <span>{model.name}</span>}
              <code className="mono">{model.file}</code>
              {model.size && <span className="splash-size">{model.size}</span>}
            </p>
          )}
        </div>
        <div className={fraction == null ? 'splash-bar' : 'splash-bar determinate'}>
          <i style={fraction == null ? undefined : { width: `${Math.round(fraction * 100)}%` }} />
        </div>
        <ul className="splash-steps" aria-label="Finished start-up steps">
          {steps.map((s) => (
            <li key={s.key} className={s.tone === 'warn' ? 'warn' : undefined} title={s.detail ? `${s.label}: ${s.detail}` : s.label}>
              {s.tone === 'warn' ? <b aria-label="Problem">!</b> : <Icon name="check" size={12} />}
              <span className="splash-step-label">{s.label}</span>
              {s.detail && <span className="splash-step-detail">{s.detail}</span>}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

/** A native <dialog>: focus trap, Esc and the backdrop come for free. Click outside closes it. */
export function Modal({ label, className, onClose, children }: { label: string; className?: string; onClose: () => void; children: ReactNode }) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => { const d = ref.current; if (d && !d.open) d.showModal(); }, []);
  return (
    <dialog ref={ref} className={className} aria-label={label} onClose={onClose}
      onMouseDown={(e) => { if (e.target === ref.current) onClose(); }}>
      {children}
    </dialog>
  );
}

const KEYS: [string, string][] = [
  ['←  /  →', 'previous / next photo in the burst, and on into the next stack (flat view: previous / next photo)'],
  ['↑  /  ↓', 'previous / next burst (flat view: one row up / down)'],
  ['Tab  /  Shift+Tab', 'done with this burst: next / previous burst'],
  ['N', 'next undecided burst or photo'],
  ['Enter', 'open the burst in place; Enter again opens the full frame'],
  ['Esc', 'close the full frame, then collapse the burst'],
  ['P', 'keep this photo (click the ✓ segment again to clear it)'],
  ['X  /  Del', 'cull this photo'],
  ['U', 'clear stars, reject and keeper on this photo'],
  ['B  /  Shift+B', 'keep this photo and cull its stack / cull the whole stack'],
  ['1 – 5', 'give it that many stars'],
  ['0', 'clear the stars, back to the measured rating'],
  ['Ctrl+Z  /  Ctrl+Shift+Z', 'undo / redo the last decision (a stack action is one step)'],
  ['G  /  E  /  C', 'grid, full frame, compare the burst side by side'],
  ['?', 'this list'],
];

export function KeysDialog({ onClose }: { onClose: () => void }) {
  return (
    <Modal label="Keyboard shortcuts" className="small-dialog" onClose={onClose}>
      <div className="dialog-head"><h3>Keyboard shortcuts</h3><button className="ghost small" onClick={onClose}>Close</button></div>
      <dl className="keys">
        {KEYS.map(([k, what]) => (
          <div key={k}><dt>{k.split(/\s{2}\/\s{2}/).map((part) => <kbd key={part}>{part}</kbd>)}</dt><dd>{what}</dd></div>
        ))}
      </dl>
      <p className="muted small-print">With auto-advance on (Manage shoot), P and X move on to the next burst or frame, and with cross-stack arrows on, ← and → carry on into the stack beside them.</p>
    </Modal>
  );
}

export function Confirm({ title, children, action, onConfirm, onCancel }: {
  title: string; children: ReactNode; action: string; onConfirm: () => void; onCancel: () => void;
}) {
  return (
    <Modal label={title} className="small-dialog" onClose={onCancel}>
      <div className="dialog-head"><h3>{title}</h3></div>
      <div className="dialog-body">{children}</div>
      <div className="actions-row end">
        <button className="ghost" onClick={onCancel}>Cancel</button>
        <button className="danger" autoFocus onClick={onConfirm}>{action}</button>
      </div>
    </Modal>
  );
}
