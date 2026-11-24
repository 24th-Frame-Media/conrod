import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { getCurrentWindow, type Window } from '@tauri-apps/api/window';
import { inTauri, mocked } from '../lib/api';
import { etaText, pct, plural } from '../lib/format';
import { applyTheme, storedTheme } from '../lib/theme.mjs';
import { modelGroups, modelWord, workLabel, type HealthStatus } from '../lib/health.mjs';
import { placeAnchoredMenu } from '../lib/anchoredMenu.mjs';
import type { DevInfo, DropdownJob, Job, ModelInfo, Page, ReloadState, RunSnapshot, Status, Task } from '../lib/types';
import { DevCluster } from './DevStatus';
import { stageName } from '../lib/stageState.mjs';
import { Icon, Mark } from './basics';

/** Engine health rows carry wire-format provider ids as `name` and use
 *  `optional` as a pseudo-status. Render a friendly name and a readiness word
 *  so the popover never leaks a token like `in_process` or shows "Optional"
 *  where a state belongs (the row still has its readiness dot and the section
 *  still has its ready/total count). */
const HEALTH_NAMES: Record<string, string> = {
  in_process: 'Local (this PC)',
  'opencode-go': 'OpenCode (pay-as-you-go credits)',
  opencode: 'OpenCode (pay-as-you-go credits)',
  openrouter: 'OpenRouter',
  ollama: 'Ollama',
  gemini: 'Gemini',
  anthropic: 'Anthropic',
  openai: 'OpenAI',
};
const healthName = (m: ModelInfo) => HEALTH_NAMES[m.name] ?? m.name;
const healthState = (m: ModelInfo) => {
  const word = modelWord(m);
  return word === 'Optional' ? 'Not installed' : word;
};

const TABS: [Page, string, string][] = [
  ['Library', 'Library', 'Your albums'],
  ['Review', 'Review', 'Cull, identify and export'],
  ['Settings', 'Settings', 'Settings, models and known entries'],
];

/** One tab for a whole ingest: the scan and the identification that follows it (or a standalone identify run) share it. The Identify page key stays for callers. */
const INGEST_TAB: [Page, string, string] = ['Scan', 'Ingest', 'Watch the scan and the identification'];

export type StatusActions = { pause: () => void; resume: () => void; /** Resume one parked stage: the stage control, not the scan's. */ resumeStage: (stage: string) => void; stop: () => void; cancel: (key: string) => void; unqueue: (job: Job) => void };

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

/** Strip internal `:<jobId>` suffixes from operation keys before showing them in the UI. */
const cleanOpLabel = (key: string) => key.replace(/:\d+$/, '');

/** Compact two-tab activity popover: Activity (running ops, live scans, history) and Models (readiness). */
function StatusPill({
  status, jobs, models, health, actions, defaultOpen, scanRows, activeJobId, onOpenJob, scanPaused, parkedStages = [],
}: {
  status: Status; jobs: Job[]; models: ModelInfo[]; health: HealthStatus; actions: StatusActions; defaultOpen: boolean;
  scanRows?: DropdownJob[]; activeJobId?: number | null; onOpenJob?: (id: number) => void;
  /** The scan itself is paused (Pause / Resume scan). A parked stage never sets this. */
  scanPaused?: boolean;
  /** Stages parked by a stage pause, resumed with the stage control. */
  parkedStages?: string[];
}) {
  const [open, setOpen] = useState(defaultOpen);
  const [tab, setTab] = useState<'activity' | 'models'>('activity');
  const [stopping, setStopping] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const popover = useRef<HTMLDivElement>(null);

  const active = status.tasks.find((t) => t.state === 'running' || t.state === 'paused');
  const { local, optional, vision } = modelGroups(models);
  const readyModels = local.filter((model) => model.ready).length;
  const visionReady = vision.filter((model) => model.ready).length;
  const modelsAttention = (local.length > 0 && readyModels < local.length) || vision.some((m) => !m.ready);
  const activeTasks = status.tasks.filter((task) => task.state === 'running' || task.state === 'paused').sort((a, b) => b.id - a.id);
  const finishedTasks = status.tasks.filter((task) => task.state !== 'running' && task.state !== 'paused').sort((a, b) => b.id - a.id);
  const activeJob = jobs.find((job) => job.id === status.activeJob);
  const waiting = (status.queue ?? []).map((id) => jobs.find((job) => job.id === id)).filter((job): job is Job => Boolean(job));
  const tone = health.tone;
  const hasScansList = Boolean(scanRows && onOpenJob);
  const activeInScans = Boolean(hasScansList && activeJobId != null && scanRows?.some((row) => row.id === activeJobId));
  const isScanTask = (task: Task): boolean => {
    if (!activeJob) return false;
    const label = task.label ?? '';
    if (/^(Scanning|Culling|Grouping|Loading|Indexing)\b/i.test(label)) return true;
    if (activeJob.label && label.includes(activeJob.label)) return true;
    const m = label.match(/\balbum\s+(\d+)\b/i);
    return Boolean(m && Number(m[1]) === activeJob.id);
  };
  const activeScanTask = activeJob
    ? (activeTasks.find(isScanTask) ?? activeTasks.find((t) => !/^(Writing|Rerunning|Exporting|Importing|Installing|Filling)\b/i.test(t.label ?? '')))
    : undefined;
  const summaryTask = activeScanTask ?? active;
  const shownActiveTasks = activeInScans && activeJob != null && activeScanTask
    ? activeTasks.filter((t) => t.id !== activeScanTask.id)
    : activeTasks;

  // Match each running background task to its cancellable operation key so Cancel lives inline without leaking `:jobId`.
  const matchedOpFor = (t: Task): string | undefined => {
    const cleanTask = workLabel(t.label).toLowerCase();
    const rawTask = (t.label ?? '').toLowerCase();
    return status.operations.find((key) => {
      const base = cleanOpLabel(key).toLowerCase();
      return base === cleanTask || rawTask.startsWith(base);
    });
  };
  const matchedOps = new Set(shownActiveTasks.map(matchedOpFor).filter((k): k is string => Boolean(k)));
  const unmatchedOps = status.operations.filter((key) => !matchedOps.has(key));
  const showRunningNow = shownActiveTasks.length > 0 || unmatchedOps.length > 0 || (!activeInScans && status.activeJob != null);

  const activeMeta = (() => {
    if (!activeScanTask) return '';
    if (activeScanTask.error) return activeScanTask.error;
    const parts: string[] = [];
    if (activeScanTask.total > 0) parts.push(`${activeScanTask.done.toLocaleString()} / ${activeScanTask.total.toLocaleString()}`);
    else if (activeScanTask.detail) parts.push(activeScanTask.detail);
    if (activeScanTask.eta != null) parts.push(etaText(activeScanTask.eta));
    return parts.join(' · ');
  })();

  const taskMeta = (t: Task) => {
    if (t.error) return t.error;
    const parts: string[] = [];
    if (t.total > 0) parts.push(`${t.done.toLocaleString()} / ${t.total.toLocaleString()}`);
    else if (t.detail) parts.push(t.detail);
    if (t.eta != null) parts.push(etaText(t.eta));
    return parts.join(' · ');
  };

  useLayoutEffect(() => {
    if (!open || !box.current || !popover.current) return;
    const place = () => placeAnchoredMenu(box.current!, popover.current!, 'right');
    place();
    window.addEventListener('resize', place);
    document.addEventListener('scroll', place, true);
    return () => {
      window.removeEventListener('resize', place);
      document.removeEventListener('scroll', place, true);
    };
  }, [open, tab, shownActiveTasks.length, scanRows?.length]);

  useEffect(() => { if (defaultOpen) setOpen(true); }, [defaultOpen]);
  useEffect(() => { if (!open) setTab('activity'); }, [open]);
  useEffect(() => { if (status.activeJob == null) setStopping(false); }, [status.activeJob]);

  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => { if (!box.current?.contains(e.target as Node)) setOpen(false); };
    const esc = (e: KeyboardEvent) => { if (e.key === 'Escape') setOpen(false); };
    window.addEventListener('pointerdown', away);
    window.addEventListener('keydown', esc);
    return () => { window.removeEventListener('pointerdown', away); window.removeEventListener('keydown', esc); };
  }, [open]);

  const busyNow = activeTasks.length > 0 || status.activeJob != null || status.operations.length > 0;

  return (
    <div className="status" ref={box}>
      <button className={`health ${tone}`} aria-expanded={open} aria-haspopup="dialog" title={health.detail || 'Activity and models'} onClick={() => setOpen((v) => !v)}>
        <span className="light" />
        <span className="health-text">{health.label}</span>
        {active && active.total > 0 && <small>{active.done.toLocaleString()}/{active.total.toLocaleString()}</small>}
        <Icon name="chevron" size={13} />
      </button>
      {open && (
        <div className="popover activity-dashboard" role="dialog" aria-label="Activity and models" ref={popover}>
          <div className="act-head">
            <div className="act-tabs" role="tablist" aria-label="Activity sections">
              <button
                type="button"
                role="tab"
                aria-selected={tab === 'activity'}
                className={`act-tab${tab === 'activity' ? ' on' : ''}`}
                onClick={() => setTab('activity')}
              >
                {busyNow && <span className="act-tab-dot busy" aria-hidden="true" />}
                Activity
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={tab === 'models'}
                className={`act-tab${tab === 'models' ? ' on' : ''}`}
                onClick={() => setTab('models')}
              >
                <span className={`act-tab-dot ${modelsAttention ? 'warn' : 'ok'}`} aria-hidden="true" />
                Models
                {local.length > 0 && <span className="act-tab-count">{readyModels}/{local.length}</span>}
              </button>
            </div>
            <span className={`act-pill state ${tone === 'busy' ? 'running' : tone === 'error' ? 'failed' : tone === 'warn' ? 'paused' : 'done'}`}>
              {health.label}
            </span>
          </div>

          {tab === 'activity' ? (
            <div className="act-body" role="tabpanel">
              {showRunningNow && (
                <div className="act-group">
                  <div className="act-label"><span>Running</span><span>{shownActiveTasks.length || unmatchedOps.length}</span></div>
                  {shownActiveTasks.map((t) => {
                    const opKey = matchedOpFor(t);
                    const meta = taskMeta(t);
                    return (
                      <div className="act-op" key={t.id}>
                        <div className="act-op-top">
                          <b>{workLabel(t.label)}</b>
                          {opKey ? (
                            <button type="button" className="act-inline-btn danger" onClick={() => actions.cancel(opKey)}>
                              Cancel
                            </button>
                          ) : (
                            <span className={`state ${t.state}`}>{t.state}</span>
                          )}
                        </div>
                        {t.total > 0 && (
                          <div className="jobs-bar">
                            <span className={`jobs-bar-fill ${t.state === 'paused' ? 'paused' : 'running'}`} style={{ width: `${pct(t.done, t.total)}%` }} />
                          </div>
                        )}
                        {meta && <small className="act-meta muted">{meta}</small>}
                      </div>
                    );
                  })}
                  {(!activeInScans && status.activeJob != null) && (
                    <div className="act-op-actions">
                      <button type="button" className="ghost small" onClick={scanPaused ? actions.resume : actions.pause}>
                        {scanPaused ? 'Resume' : 'Pause'}
                      </button>
                      {parkedStages.map((id) => (
                        <button type="button" key={id} className="ghost small" onClick={() => actions.resumeStage(id)}>Resume {stageName(id)}</button>
                      ))}
                      <button type="button" className="ghost small danger" disabled={stopping} onClick={() => { setStopping(true); actions.stop(); }}>
                        {stopping ? 'Stopping…' : 'Stop scan'}
                      </button>
                    </div>
                  )}
                  {unmatchedOps.length > 0 && (
                    <div className="act-op-actions">
                      {unmatchedOps.map((key) => (
                        <button type="button" key={key} className="ghost small danger" onClick={() => actions.cancel(key)}>
                          Cancel {cleanOpLabel(key)}
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              )}

              {!hasScansList && waiting.length > 0 && (
                <div className="act-group">
                  <div className="act-label"><span>Queued</span><span>{waiting.length}</span></div>
                  {waiting.map((job) => (
                    <div className="jobs-row jobs-row-card queued" key={job.id}>
                      <span className="jobs-row-top">
                        <b className="jobs-row-label">{job.label}</b>
                        <button type="button" className="act-inline-btn" onClick={() => actions.unqueue(job)}>Remove</button>
                      </span>
                    </div>
                  ))}
                </div>
              )}

              {scanRows && onOpenJob && (
                <div className="act-group">
                  <div className="act-label"><span>Scans</span><span>{scanRows.length}</span></div>
                  {scanRows.length === 0 ? (
                    <p className="muted activity-empty">No albums yet. Import a folder to start one.</p>
                  ) : (
                    <div className="activity-scans" role="list" aria-label="Scans">
                      {scanRows.map((row) => {
                        const isActive = row.id === activeJobId;
                        const progressPct = row.chip.progress != null ? Math.max(0, Math.min(100, Math.round(row.chip.progress * 100))) : null;
                        if (isActive) {
                          // The scan's own pause only: a parked stage marks its task paused too, and has its own Resume below.
                          const isPaused = row.chip.state === 'paused';
                          return (
                            <div key={row.id} role="listitem" className="jobs-row jobs-row-card active">
                              <button
                                type="button"
                                className="jobs-row-main"
                                onClick={() => { onOpenJob(row.id); setOpen(false); }}
                              >
                                <span className="jobs-row-top">
                                  <b className="jobs-row-label">{row.label}</b>
                                  <span className={`jobs-chip ${row.chip.tone}`}>{row.chip.word}</span>
                                </span>
                                {progressPct != null && (
                                  <span className="jobs-bar" aria-label={`${progressPct} percent`}>
                                    <span className={`jobs-bar-fill ${row.chip.tone}`} style={{ width: `${progressPct}%` }} />
                                  </span>
                                )}
                                {activeMeta && (
                                  <span className="jobs-row-meta muted" title={activeScanTask?.detail || undefined}>{activeMeta}</span>
                                )}
                              </button>
                              {status.activeJob != null && (
                                <div className="jobs-row-actions">
                                  <button type="button" className="act-inline-btn" onClick={isPaused ? actions.resume : actions.pause}>
                                    {isPaused ? 'Resume' : 'Pause'}
                                  </button>
                                  {parkedStages.map((id) => (
                                    <button key={id} type="button" className="act-inline-btn" onClick={() => actions.resumeStage(id)}>
                                      Resume {stageName(id)}
                                    </button>
                                  ))}
                                  <button
                                    type="button"
                                    className="act-inline-btn danger"
                                    disabled={stopping}
                                    onClick={() => { setStopping(true); actions.stop(); }}
                                  >
                                    {stopping ? 'Stopping…' : 'Stop'}
                                  </button>
                                </div>
                              )}
                            </div>
                          );
                        }
                        const queuedJob = row.chip.state === 'queued' ? jobs.find((j) => j.id === row.id) : undefined;
                        if (queuedJob) {
                          return (
                            <div key={row.id} role="listitem" className="jobs-row jobs-row-card queued">
                              <button
                                type="button"
                                className="jobs-row-main"
                                onClick={() => { onOpenJob(row.id); setOpen(false); }}
                              >
                                <span className="jobs-row-top">
                                  <b className="jobs-row-label">{row.label}</b>
                                  <span className={`jobs-chip ${row.chip.tone}`}>{row.chip.word}</span>
                                </span>
                              </button>
                              <button
                                type="button"
                                className="act-inline-btn"
                                aria-label={`Remove ${queuedJob.label} from queue`}
                                onClick={() => actions.unqueue(queuedJob)}
                              >
                                Remove
                              </button>
                            </div>
                          );
                        }
                        const isDone = row.chip.state === 'done' || row.chip.state === 'done_errors';
                        const showPartialBar = !isDone && progressPct != null && progressPct > 0 && progressPct < 100;
                        return (
                          <button
                            type="button"
                            key={row.id}
                            role="listitem"
                            className={`jobs-row${isDone ? ' is-done' : ''}`}
                            onClick={() => { onOpenJob(row.id); setOpen(false); }}
                          >
                            <span className="jobs-row-top">
                              <b className="jobs-row-label">{row.label}</b>
                              {showPartialBar && <span className="jobs-row-pct">{progressPct}%</span>}
                              <span className={`jobs-chip ${row.chip.tone}`}>{row.chip.word}</span>
                            </span>
                            {showPartialBar && (
                              <span className="jobs-bar thin" aria-label={`${progressPct} percent`}>
                                <span className={`jobs-bar-fill ${row.chip.tone}`} style={{ width: `${progressPct}%` }} />
                              </span>
                            )}
                          </button>
                        );
                      })}
                    </div>
                  )}
                </div>
              )}

              <details className="activity-history">
                <summary>Recent activity ({finishedTasks.length})</summary>
                <div className="activity-history-list">
                  {finishedTasks.map((t) => (
                    <div className="task" key={t.id}>
                      <div className="task-row"><b>{t.label}</b><span className={`state ${t.state}`}>{t.state}</span></div>
                      {(t.error || t.detail || t.total > 0) && (
                        <small className="muted">{t.error || t.detail || `${t.done.toLocaleString()} of ${t.total.toLocaleString()}`}</small>
                      )}
                    </div>
                  ))}
                  {!finishedTasks.length && <p className="muted activity-empty">Nothing finished yet</p>}
                  <details><summary>Event log</summary><pre className="log mono">{status.log.join('\n') || 'Nothing yet.'}</pre></details>
                </div>
              </details>
            </div>
          ) : (
            <div className="act-body" role="tabpanel">
              <div className="act-group">
                <div className="act-label"><span>Setup models</span><span>{readyModels}/{local.length}</span></div>
                <ul className="activity-models">
                  {local.map((model) => <li key={model.file ?? model.name}><span className={`dot ${model.ready ? 'ok' : 'no'}`} /><span className="activity-model-name" title={model.name}>{healthName(model)}</span><small>{healthState(model)}</small></li>)}
                  {!local.length && <li className="muted">Checking model availability…</li>}
                </ul>
              </div>
              {vision.length > 0 && (
                <div className="act-group">
                  <div className="act-label"><span>Vision services</span><span>{visionReady}/{vision.length}</span></div>
                  <ul className="activity-models">
                    {vision.map((model) => <li key={model.id ?? model.name}><span className={`dot ${model.ready ? 'ok' : 'no'}`} /><span className="activity-model-name" title={model.name}>{healthName(model)}</span><small title={model.detail}>{healthState(model)}</small></li>)}
                  </ul>
                </div>
              )}
              {optional.length > 0 && (
                <div className="act-group">
                  <div className="act-label"><span>Optional models</span><span>{optional.filter((model) => model.ready).length}/{optional.length}</span></div>
                  <ul className="activity-models">
                    {optional.map((model) => <li key={model.file ?? model.name}><span className={`dot ${model.ready ? 'ok' : 'no'}`} /><span className="activity-model-name" title={model.name}>{healthName(model)}</span><small>{healthState(model)}</small></li>)}
                  </ul>
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

type Props = {
  page: Page; setPage: (page: Page) => void; jobs: Job[]; models: ModelInfo[]; status: Status;
  health: HealthStatus; actions: StatusActions; popoverOpen: boolean; dev: DevInfo | null; reloadState: ReloadState;
  /** The single run snapshot: live scan rows in the activity popover read from it. */
  runSnapshot?: RunSnapshot;
  /** Open a job's run view (hydrate-then-live); one-click entry from the Scans section. */
  onOpenJob?: (id: number) => void;
};

/** Topbar and title bar in one: brand, tabs, stats, status pill, window controls. Drag it to move the window. */
export function TitleBar({ page, setPage, jobs, models, status, health, actions, popoverOpen, dev, reloadState, runSnapshot, onOpenJob }: Props) {
  const active = status.tasks.find((t) => t.state === 'running' || t.state === 'paused');
  const photos = jobs.reduce((n, j) => n + j.total, 0);
  const [theme, setTheme] = useState(storedTheme);
  // The Ingest tab exists only while a scan or identification is live, or while either screen is open (to read the summary).
  const scanning = status.activeJob != null;
  const identifying = status.operations.some((key) => key.startsWith('Identifying:'));
  const shown = (id: Page) => page === id || (id === 'Scan' && page === 'Identify');
  const tabs = [
    TABS[0],
    ...(scanning || identifying || page === 'Scan' || page === 'Identify' ? [INGEST_TAB] : []),
    ...TABS.slice(1),
  ];
  return (
    <header className="topbar" data-dev={dev ? '' : undefined} data-tauri-drag-region>
      <div className="brand" data-tauri-drag-region>
        <div className="brand-mark"><Mark /></div>
        <h1>Conrod</h1>
      </div>
      {dev && <DevCluster dev={dev} state={reloadState} />}
      <nav className="tabs" aria-label="Screens">
        {tabs.map(([id, label, title]) => (
          <button key={id} className={shown(id) ? 'active' : ''} title={id === 'Review' && scanning ? 'Cull the frames already measured while the scan finishes' : title} aria-current={shown(id) ? 'page' : undefined} onClick={() => setPage(id === 'Scan' && identifying && !scanning ? 'Identify' : id)}>
            {label}
            {id === 'Scan' && (scanning || identifying) && <i className="tab-live" aria-label={identifying ? 'identifying' : 'running'} />}
          </button>
        ))}
      </nav>
      <div className="spacer" data-tauri-drag-region />
      <div className="stats" data-tauri-drag-region>
        <span><b>{plural(jobs.length, 'album')}</b></span><span><b>{photos.toLocaleString()}</b> photos</span>
      </div>
      <button
        type="button"
        className="theme-toggle"
        title={theme === 'dark' ? 'Switch to the light theme' : 'Switch to the dark theme'}
        aria-label={theme === 'dark' ? 'Switch to the light theme' : 'Switch to the dark theme'}
        onClick={() => setTheme(applyTheme(theme === 'dark' ? 'light' : 'dark'))}
      >
        <Icon name={theme === 'dark' ? 'sun' : 'moon'} />
      </button>
      <StatusPill
        status={status}
        jobs={jobs}
        models={models}
        health={health}
        actions={actions}
        defaultOpen={popoverOpen}
        scanRows={runSnapshot?.jobs}
        activeJobId={runSnapshot?.activeJob ?? status.activeJob}
        onOpenJob={onOpenJob}
        scanPaused={runSnapshot?.paused}
        parkedStages={runSnapshot?.parkedStages}
      />
      <WindowControls />
      {active && active.total > 0 && <i className="topbar-progress" style={{ width: `${pct(active.done, active.total)}%` }} />}
    </header>
  );
}
