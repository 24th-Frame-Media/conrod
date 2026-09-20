import { useEffect, useRef, type ReactNode } from 'react';

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
} as const;
export type IconName = keyof typeof PATHS;

export function Icon({ name, size = 16 }: { name: IconName; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8"
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

export function Mark() {
  return (
    <svg className="mark" viewBox="0 0 26 72" fill="none" aria-hidden="true">
      <path d="M19 3 L12 45 L13.5 50 L7.5 54 L7.5 59 L4 69" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" />
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

export function Splash({ gone }: { gone: boolean }) {
  return (
    <div className={gone ? 'splash gone' : 'splash'} aria-hidden={gone}>
      <div className="splash-card">
        <div className="splash-mark"><Mark /></div>
        <div>
          <p className="splash-name"><b>con</b>rod</p>
          <p className="splash-ver">Rust edition</p>
        </div>
        <p className="splash-msg">Warming up the detectors</p>
        <div className="splash-bar"><i /></div>
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
  ['J  /  →  /  ↓', 'next photo'],
  ['K  /  ←  /  ↑', 'previous photo'],
  ['1 – 5', 'give it that many stars'],
  ['0', 'clear the stars, back to the measured rating'],
  ['X  /  Del', 'reject, or put a rejected one back'],
  ['U', 'undo your marks on this photo'],
  ['Enter', 'open the full frame'],
  ['B', 'show or hide detection outlines'],
  ['Esc', 'close the full frame'],
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
      <p className="muted small-print">On Train: <kbd>1</kbd>-<kbd>5</kbd> rate, <kbd>X</kbd> can&apos;t tell, <kbd>P</kbd> pan, <kbd>U</kbd> undo, <kbd>Z</kbd> 100%.</p>
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
