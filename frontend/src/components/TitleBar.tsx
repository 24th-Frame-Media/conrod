import { useEffect, useRef, useState } from 'react';
import { getCurrentWindow, type Window } from '@tauri-apps/api/window';
import { inTauri, mocked } from '../lib/api';
import { etaText, pct, plural } from '../lib/format';
import type { Job, ModelInfo, Page, Status } from '../lib/types';
import { profileLabel } from '../lib/types';
import { Icon, Mark } from './basics';

const TABS: [Page, string, string][] = [
  ['Library', 'Library', 'Your albums'],
  ['Review', 'Review', 'Cull, identify and export'],
  ['Train', 'Train', 'Teach it your eye'],
  ['Settings', 'Settings', 'Settings and vehicles'],
];

export type StatusActions = { pause: () => void; resume: () => void; stop: () => void; cancel: (key: string) => void };

function workLabel(label: string): string {
  if (label.startsWith('Identifying')) return 'Identifying';
  if (label.startsWith('Grouping')) return 'Grouping';
  if (label.startsWith('Culling')) return 'Culling';
  if (label.startsWith('Loading')) return 'Loading';
  return label.replace(/\s*[·-]\s*album\s+\d+$/i, '');
}

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
function StatusPill({ status, jobs, models, actions, defaultOpen }: { status: Status; jobs: Job[]; models: ModelInfo[]; actions: StatusActions; defaultOpen: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  const [stopping, setStopping] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => { if (defaultOpen) setOpen(true); }, [defaultOpen]);
  useEffect(() => { if (status.activeJob == null) setStopping(false); }, [status.activeJob]);
  const active = status.tasks.find((t) => t.state === 'running' || t.state === 'paused');
  const failed = status.tasks.find((t) => t.state === 'failed' && t.error !== 'stopped');
  const tone = active ? (active.state === 'paused' ? 'warn' : 'busy') : failed ? 'error' : 'ok';
  const label = active ? (active.state === 'paused' ? 'Paused' : workLabel(active.label)) : failed ? 'Error' : 'Ready';
  const activeTasks = status.tasks.filter((task) => task.state === 'running' || task.state === 'paused').sort((a, b) => b.id - a.id);
  const finishedTasks = status.tasks.filter((task) => task.state !== 'running' && task.state !== 'paused').sort((a, b) => b.id - a.id);
  const activeJob = jobs.find((job) => job.id === status.activeJob);
  const readyModels = models.filter((model) => model.ready).length;

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
      <button className={`health ${tone}`} aria-expanded={open} aria-haspopup="dialog" title="Notifications and activity" onClick={() => setOpen((v) => !v)}>
        <span className="light" />
        <span className="health-text">{label}</span>
        {active && active.total > 0 && <small>{active.done.toLocaleString()}/{active.total.toLocaleString()}</small>}
        <Icon name="chevron" size={13} />
      </button>
      {open && (
        <div className="popover activity-dashboard" role="dialog" aria-label="Notifications and activity">
          <div className="popover-head"><h3>Background work</h3><span className={`state ${tone === 'busy' ? 'running' : tone === 'error' ? 'failed' : 'done'}`}>{active ? (active.state === 'paused' ? 'Paused' : 'Running') : tone === 'error' ? 'Error' : 'Ready'}</span></div>
          <div className="activity-summary">
            <div><b>{activeJob && active?.total ? `${active.done.toLocaleString()} / ${active.total.toLocaleString()}` : '—'}</b><small>{activeJob ? activeJob.label : 'No active scan'}</small></div>
            <div><b>{readyModels} / {models.length}</b><small>Models ready</small></div>
            <div><b>{status.tasks.filter((task) => task.state === 'running' || task.state === 'paused').length}</b><small>Active tasks</small></div>
          </div>
          {activeJob && active?.total ? <div className="bar scan-overall" title="Overall scan progress"><div className="fill" style={{ width: `${pct(active.done, active.total)}%` }} /></div> : null}
          <div className="activity-section"><span>Running now</span><span>{activeTasks.length}</span></div>
          {activeTasks.length === 0 && <p className="muted activity-empty">No background tasks.</p>}
          {activeTasks.map((t) => (
            <div className="task" key={t.id}>
              <div className="task-row"><b>{workLabel(t.label)}</b><span className={`state ${t.state}`}>{t.state}</span></div>
              {t.total > 0 && <div className="bar"><div className="fill" style={{ width: `${pct(t.done, t.total)}%` }} /></div>}
              <small className="muted">{t.error || (t.detail ? `Now: ${t.detail}` : t.total > 0 ? `${t.done.toLocaleString()} of ${t.total.toLocaleString()}` : '')}{t.eta != null && ` · ${etaText(t.eta)}`}</small>
            </div>
          ))}
          {(status.activeJob != null || status.operations.length > 0) && (
            <div className="popover-actions">
              {status.activeJob != null && (
                <>
                  <button className="ghost small" onClick={active?.state === 'paused' ? actions.resume : actions.pause}>{active?.state === 'paused' ? 'Resume' : 'Pause'}</button>
                  <button className="ghost small danger" disabled={stopping} onClick={() => { setStopping(true); actions.stop(); }}>{stopping ? 'Stopping…' : 'Stop scan'}</button>
                </>
              )}
              {status.operations.map((key) => <button key={key} className="ghost small" onClick={() => actions.cancel(key)}>Cancel {key}</button>)}
            </div>
          )}
          <div className="activity-section"><span>Detection models</span><span>{readyModels}/{models.length}</span></div>
          <ul className="activity-models">
            {models.map((model) => <li key={model.file}><span className={`dot ${model.ready ? 'ok' : 'no'}`} /><span>{model.name}</span><small>{model.ready ? 'Ready' : 'Unavailable'}</small></li>)}
            {!models.length && <li className="muted">Checking model availability…</li>}
          </ul>
          <details className="activity-history">
            <summary>Recent activity ({finishedTasks.length})</summary>
            {finishedTasks.map((t) => (
              <div className="task" key={t.id}>
                <div className="task-row"><b>{t.label}</b><span className={`state ${t.state}`}>{t.state}</span></div>
                <small className="muted">{t.error || t.detail || (t.total > 0 ? `${t.done.toLocaleString()} of ${t.total.toLocaleString()}` : '')}</small>
              </div>
            ))}
            {!finishedTasks.length && <p className="muted activity-empty">Nothing finished yet.</p>}
            <details><summary>Event log</summary><pre className="log mono">{status.log.join('\n') || 'Nothing yet.'}</pre></details>
          </details>
        </div>
      )}
    </div>
  );
}

type Props = {
  page: Page; setPage: (page: Page) => void; jobs: Job[]; models: ModelInfo[]; status: Status; profile: string;
  actions: StatusActions; popoverOpen: boolean;
};

/** Topbar and title bar in one: brand, tabs, stats, scan type, status pill, window controls. Drag it to move the window. */
export function TitleBar({ page, setPage, jobs, models, status, profile, actions, popoverOpen }: Props) {
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
      <StatusPill status={status} jobs={jobs} models={models} actions={actions} defaultOpen={popoverOpen} />
      <WindowControls />
      {active && active.total > 0 && <i className="topbar-progress" style={{ width: `${pct(active.done, active.total)}%` }} />}
    </header>
  );
}
