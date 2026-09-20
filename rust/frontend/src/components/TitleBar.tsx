import { useEffect, useRef, useState } from 'react';
import { getCurrentWindow, type Window } from '@tauri-apps/api/window';
import { inTauri, mocked } from '../lib/api';
import { etaText, pct, plural } from '../lib/format';
import type { Job, Page, Status } from '../lib/types';
import { PROFILES } from '../lib/types';
import { Icon, Mark } from './basics';

const TABS: [Page, string, string][] = [
  ['Library', 'Library', 'Your albums'],
  ['Scan', 'Scan', 'Add a shoot'],
  ['Review', 'Review', 'Cull and label'],
  ['Train', 'Train', 'Teach it your eye'],
  ['Known vehicles', 'Vehicles', 'Known vehicles'],
  ['Settings', 'Settings', 'Settings'],
];

export type StatusActions = { pause: () => void; resume: () => void; stop: () => void; cancel: (key: string) => void };

/** Minimise / maximise / close. Only drawn when the window has no OS frame of its own. */
function WindowControls() {
  const [decorated, setDecorated] = useState(true);
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    if (!inTauri) return;
    const win = getCurrentWindow();
    const sync = () => void win.isMaximized().then(setMaximized);
    let off: (() => void) | undefined;
    let dead = false;
    void win.isDecorated().then(setDecorated);
    sync();
    void win.onResized(sync).then((u) => { if (dead) u(); else off = u; });
    return () => { dead = true; off?.(); };
  }, []);
  if (inTauri ? decorated : !mocked) return null;
  const act = (f: (w: Window) => Promise<void>) => () => { if (inTauri) void f(getCurrentWindow()); };
  return (
    <div className="winctl" role="group" aria-label="Window controls">
      <button aria-label="Minimise" onClick={act((w) => w.minimize())}><Icon name="min" /></button>
      <button aria-label={maximized ? 'Restore' : 'Maximise'} onClick={act((w) => w.toggleMaximize())}><Icon name={maximized ? 'restore' : 'max'} /></button>
      <button className="close" aria-label="Close" onClick={act((w) => w.close())}><Icon name="x" /></button>
    </div>
  );
}

/** The health light of the Python header, grown into a popover: tasks, pause / stop, cancel, event log. */
function StatusPill({ status, actions, defaultOpen }: { status: Status; actions: StatusActions; defaultOpen: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => { if (defaultOpen) setOpen(true); }, [defaultOpen]);
  const active = status.tasks.find((t) => t.state === 'running' || t.state === 'paused');
  const failed = status.tasks.find((t) => t.state === 'failed' && t.error !== 'stopped');
  const tone = active ? (active.state === 'paused' ? 'warn' : 'busy') : failed ? 'error' : 'ok';
  const label = active ? (active.state === 'paused' ? 'Paused' : active.label) : failed ? 'Needs attention' : 'All caught up';
  const tasks = [...status.tasks].sort((a, b) => b.id - a.id).slice(0, 6);

  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => { if (!box.current?.contains(e.target as Node)) setOpen(false); };
    const esc = (e: KeyboardEvent) => { if (e.key === 'Escape') setOpen(false); };
    window.addEventListener('pointerdown', away);
    window.addEventListener('keydown', esc);
    return () => { window.removeEventListener('pointerdown', away); window.removeEventListener('keydown', esc); };
  }, [open]);

  return (
    <div className="status" ref={box}>
      <button className={`health ${tone}`} aria-expanded={open} aria-haspopup="dialog" title="Activity" onClick={() => setOpen((v) => !v)}>
        <span className="light" />
        <span className="health-text">{label}</span>
        {active && active.total > 0 && <small>{active.done.toLocaleString()}/{active.total.toLocaleString()}</small>}
        <Icon name="chevron" size={13} />
      </button>
      {open && (
        <div className="popover" role="dialog" aria-label="Activity">
          <div className="popover-head"><h3>Activity</h3></div>
          {tasks.length === 0 && <p className="muted">No background tasks.</p>}
          {tasks.map((t) => (
            <div className="task" key={t.id}>
              <div className="task-row"><b>{t.label}</b><span className={`state ${t.state}`}>{t.state}</span></div>
              {t.total > 0 && <div className="bar"><div className="fill" style={{ width: `${pct(t.done, t.total)}%` }} /></div>}
              <small className="muted">{t.error || t.detail || (t.total > 0 ? `${t.done.toLocaleString()} of ${t.total.toLocaleString()}` : '')}{t.eta != null && ` · ${etaText(t.eta)}`}</small>
            </div>
          ))}
          {(status.activeJob != null || status.operations.length > 0) && (
            <div className="popover-actions">
              {status.activeJob != null && (
                <>
                  <button className="ghost small" onClick={active?.state === 'paused' ? actions.resume : actions.pause}>{active?.state === 'paused' ? 'Resume' : 'Pause'}</button>
                  <button className="ghost small danger" onClick={actions.stop}>Stop scan</button>
                </>
              )}
              {status.operations.map((key) => <button key={key} className="ghost small" onClick={() => actions.cancel(key)}>Cancel {key}</button>)}
            </div>
          )}
          <details><summary>Event log</summary><pre className="log mono">{status.log.join('\n') || 'Nothing yet.'}</pre></details>
        </div>
      )}
    </div>
  );
}

type Props = {
  page: Page; setPage: (page: Page) => void; jobs: Job[]; status: Status; profile: string;
  actions: StatusActions; popoverOpen: boolean;
};

/** Topbar and title bar in one: brand, tabs, stats, scan type, status pill, window controls. Drag it to move the window. */
export function TitleBar({ page, setPage, jobs, status, profile, actions, popoverOpen }: Props) {
  const active = status.tasks.find((t) => t.state === 'running' || t.state === 'paused');
  const photos = jobs.reduce((n, j) => n + j.total, 0);
  return (
    <header className="topbar" data-tauri-drag-region>
      <div className="brand" data-tauri-drag-region>
        <div className="brand-mark"><Mark /></div>
        <h1>Conrod</h1>
      </div>
      <nav className="tabs" aria-label="Screens">
        {TABS.map(([id, label, title]) => (
          <button key={id} className={page === id ? 'active' : ''} title={title} aria-current={page === id ? 'page' : undefined} onClick={() => setPage(id)}>{label}</button>
        ))}
      </nav>
      <div className="spacer" data-tauri-drag-region />
      <div className="stats" data-tauri-drag-region>
        <span><b>{plural(jobs.length, 'album')}</b></span><span><b>{photos.toLocaleString()}</b> photos</span>
      </div>
      <button className="pill scan-type" title="Scan type. Click to change it" onClick={() => setPage('Scan')}>
        {(PROFILES.find((p) => p.id === profile)?.title ?? profile).toUpperCase()}
      </button>
      <StatusPill status={status} actions={actions} defaultOpen={popoverOpen} />
      <WindowControls />
      {active && active.total > 0 && <i className="topbar-progress" style={{ width: `${pct(active.done, active.total)}%` }} />}
    </header>
  );
}
