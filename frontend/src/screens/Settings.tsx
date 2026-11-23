import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { KeyboardEvent as ReactKeyboardEvent } from 'react';
import { getVersion } from '@tauri-apps/api/app';
import { openPath, openUrl } from '@tauri-apps/plugin-opener';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { engine, inTauri } from '../lib/api';
import { plural } from '../lib/format';
import {
  FIELDS, SECTIONS, TABS, albumOverrideCounts, dirtyKeys, dirtyTabs, searchFields, tabById,
  type SettingsTabId,
} from '../lib/settings.mjs';
import { SettingsSection, SettingRow } from '../components/SettingsFields';
import { Maintenance } from '../components/Maintenance';
import { LightroomBridge } from '../components/LightroomBridge';
import { AiSettingsCard } from './AiSettingsCard';
import { IdentifyConcurrency } from '../components/IdentifyConcurrency';
import { Benchmark } from './Benchmark';
import { Known } from './Known';
import type {
  DevInfo,
  Job,
  ModelInfo,
  SettingValue,
  Settings as SettingsMap,
  Status,
  Toaster,
} from '../lib/types';
import type { Runner } from '../state/hooks';

type UpdateInfo = { version: string; notes: string | null; date: string | null };
type UpdateCardProps = {
  info: UpdateInfo | null;
  upToDate: boolean;
  error: string | null;
  checking: boolean;
  installing: boolean;
  progress: { done: number; total: number } | null;
  version: string | null;
  onCheck: () => void;
  onInstall: () => void;
  onOpenWhatsNew?: () => void;
};

function UpdateCard({ info, upToDate, error, checking, installing, progress, version, onCheck, onInstall, onOpenWhatsNew }: UpdateCardProps) {
  const badge = checking
    ? <span className="pill-badge checking">Checking…</span>
    : info
      ? <span className="pill-badge update-ready">Update available</span>
      : upToDate
        ? <span className="pill-badge up-to-date">Up to date</span>
        : error
          ? <span className="pill-badge error">Check failed</span>
          : null;

  return (
    <section className="sidebar-card" aria-labelledby="settings-updates">
      <h4 id="settings-updates">App updates</h4>
      <div className="sidebar-card-header">
        <div className="sidebar-brand">
          <span className="sidebar-brand-name">Conrod</span>
          {version && <span className="sidebar-brand-version">v{version}</span>}
        </div>
        {badge}
      </div>
      <div className="sidebar-update-body">
        {info ? (
          <div className="sidebar-update-alert">
            <div className="sidebar-update-lead">
              <strong>Version v{info.version} is ready</strong>
              {info.date ? <span className="muted small"> · {info.date.slice(0, 10)}</span> : null}
            </div>
            {info.notes && (
              <details className="sidebar-update-notes">
                <summary>Release highlights</summary>
                <div className="notes-text">{info.notes}</div>
              </details>
            )}
            <button type="button" className="primary small full-width" disabled={installing} onClick={onInstall}>
              {installing
                ? progress && progress.total > 0
                  ? `Downloading ${Math.round((progress.done / progress.total) * 100)}%…`
                  : 'Installing update…'
                : 'Install update and restart'}
            </button>
            <p className="sidebar-update-desc">Signed update, installed in place. Your library is untouched.</p>
          </div>
        ) : (
          <p className="sidebar-update-desc">
            {error ? `Could not check for updates: ${error}` : 'Checks the GitHub release for a signed update and installs it for you.'}
          </p>
        )}
        <div className="sidebar-btn-row">
          <button type="button" className="secondary small" disabled={checking || installing} onClick={onCheck}>
            {checking ? 'Checking…' : 'Check for updates'}
          </button>
          <span className="sidebar-link-group">
            <button type="button" className="sidebar-ext-link" onClick={onOpenWhatsNew}>What's new</button>
            <button type="button" className="sidebar-ext-link" onClick={() => void openUrl('https://github.com/24th-Frame-Media/conrod/releases')}>
              Releases ↗
            </button>
          </span>
        </div>
      </div>
    </section>
  );
}

type Props = {
  status?: Status;
  settings: SettingsMap;
  onSaved: (s: SettingsMap) => void;
  run: Runner;
  toast: Toaster;
  jobs?: Job[];
  models?: ModelInfo[];
  onModelsChanged?: () => void;
  onOpenWhatsNew?: () => void;
  onOpenWizard?: () => void;
  /** A tab another screen asked for (the Vision off card opens Identification); a new nonce asks again. */
  openTab?: { id: SettingsTabId; nonce: number };
  /** Dev instance info; the amber data-home banner only renders when present. */
  dev?: DevInfo | null;
};

/** Settings shared with the Python library; Save writes the whole block back. */
export function Settings({ settings, onSaved, run, toast, jobs = [], models = [], onModelsChanged, status, onOpenWhatsNew, onOpenWizard, openTab, dev }: Props) {
  const [draft, setDraft] = useState<SettingsMap>(settings);
  useEffect(() => setDraft(settings), [settings]);

  const [activeTab, setActiveTab] = useState<SettingsTabId>(openTab?.id ?? 'culling');
  useEffect(() => { if (openTab) setActiveTab(openTab.id); }, [openTab]);
  const [query, setQuery] = useState('');
  const [catalogOpen, setCatalogOpen] = useState(false);
  const [knownCount, setKnownCount] = useState<number | null>(null);

  const pending = useRef<Awaited<ReturnType<typeof check>> | null>(null);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [upToDate, setUpToDate] = useState(false);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [installingUpdate, setInstallingUpdate] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<{ done: number; total: number } | null>(null);
  const [appVersion, setAppVersion] = useState<string | null>(null);

  useEffect(() => {
    if (inTauri) void getVersion().then(setAppVersion);
  }, []);

  const refreshKnown = useCallback(() => {
    void engine.known().then((rows) => setKnownCount(rows.length)).catch(() => {});
  }, []);

  useEffect(() => { refreshKnown(); }, [catalogOpen, refreshKnown]);

  const checkForUpdates = useCallback(async (force = false) => {
    if (!inTauri) return;
    setCheckingUpdate(true);
    setUpdateError(null);
    try {
      const found = await check();
      pending.current = found;
      if (found) {
        setUpdateInfo({ version: found.version, notes: found.body ?? null, date: found.date ?? null });
        setUpToDate(false);
        if (force) toast(`Update available: v${found.version}`, { tone: 'ok' });
      } else {
        setUpdateInfo(null);
        setUpToDate(true);
        if (force) toast(`Conrod is up to date${appVersion ? ` (v${appVersion})` : ''}.`, { tone: 'ok' });
      }
    } catch (e) {
      setUpdateError(String(e));
      setUpToDate(false);
      if (force) toast(String(e), { tone: 'error' });
    } finally {
      setCheckingUpdate(false);
    }
  }, [toast, appVersion]);

  useEffect(() => { void checkForUpdates(false); }, [checkForUpdates]);

  /** Download the signed bundle, let Tauri install it in place, then restart. */
  const handleInstallUpdate = () => void run(async () => {
    const update = pending.current;
    if (!update) return;
    setInstallingUpdate(true);
    setUpdateProgress({ done: 0, total: 0 });
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === 'Started') setUpdateProgress({ done: 0, total: event.data.contentLength ?? 0 });
        else if (event.event === 'Progress') setUpdateProgress((p) => ({ done: (p?.done ?? 0) + event.data.chunkLength, total: p?.total ?? 0 }));
      });
      toast('Update installed. Restarting Conrod…', { tone: 'ok' });
      await relaunch();
    } catch (e) {
      setInstallingUpdate(false);
      setUpdateProgress(null);
      toast(String(e), { tone: 'error' });
    }
  });

  const changed = useMemo(() => dirtyKeys(draft, settings), [draft, settings]);
  const tabsDirty = useMemo(() => dirtyTabs(draft, settings), [draft, settings]);
  const changedCount = changed.length;
  const dirtyTabsNamed = TABS.filter((tab) => tabsDirty[tab.id] > 0).map((tab) => tab.label);

  const searching = query.trim().length > 0;
  const results = useMemo(
    () => (searching ? searchFields(query).filter((match) => draft[match.key] !== undefined) : []),
    [searching, query, draft],
  );
  const resultGroups = SECTIONS
    .map((section) => ({ section, matches: results.filter((match) => match.section === section.id) }))
    .filter((group) => group.matches.length > 0);

  const overrides = useMemo(() => albumOverrideCounts(jobs, settings), [jobs, settings]);
  const totalPhotos = jobs.reduce((sum, job) => sum + job.total, 0);

  const setValue = useCallback((key: string, value: SettingValue) => {
    setDraft((current) => ({ ...current, [key]: value }));
  }, []);

  const save = () => void run(async () => {
    onSaved(await engine.saveSettings(draft));
    toast('Settings saved.', { tone: 'ok' });
    onModelsChanged?.();
  });

  const discard = () => setDraft(settings);

  const pickTab = (id: SettingsTabId) => {
    setQuery('');
    setActiveTab(id);
  };

  const onTabKey = (e: ReactKeyboardEvent<HTMLElement>) => {
    const ids = TABS.map((tab) => tab.id);
    const at = ids.indexOf(activeTab);
    let next = -1;
    if (e.key === 'ArrowRight') next = (at + 1) % ids.length;
    else if (e.key === 'ArrowLeft') next = (at - 1 + ids.length) % ids.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = ids.length - 1;
    if (next < 0) return;
    e.preventDefault();
    pickTab(ids[next]);
    document.getElementById(`settings-tab-${ids[next]}`)?.focus();
  };

  const saveBar = (
    <footer className="settings-savebar">
      <span className={`save-state${changedCount ? ' is-dirty' : ''}`} role="status" aria-live="polite">
        <span className="save-dot" aria-hidden="true" />
        {changedCount
          ? <><b>{plural(changedCount, 'unsaved change', 'unsaved changes')}</b><span className="muted"> in {dirtyTabsNamed.join(', ')}</span></>
          : 'All changes saved'}
      </span>
      <span className="spacer" />
      <button type="button" className="ghost" disabled={!changedCount} onClick={discard}>Discard</button>
      <button type="button" className="primary" disabled={!changedCount} onClick={save}>Save changes</button>
    </footer>
  );

  if (catalogOpen) {
    return (
      <div className="screen settings-screen">
        <div className="settings-catalog-view">
          <div className="settings-catalog-bar">
            <button type="button" className="ghost small" onClick={() => setCatalogOpen(false)}>← Back to settings</button>
            <span className="muted small">{knownCount === null ? 'Known vehicles' : plural(knownCount, 'known vehicle')}</span>
          </div>
          <div className="settings-catalog-body">
            <Known run={run} toast={toast} />
          </div>
        </div>
        {saveBar}
      </div>
    );
  }

  return (
    <div className="screen settings-screen">
      <div className="settings-page">
        {dev && (
          <div className="dev-banner">
            <span className="dev-flag">DEV</span>
            <span>Development instance — albums, settings and cache live in <code className="mono">{dev.home}</code>. Your production library (<code className="mono">~/.conrod</code>) is untouched.</span>
            <button type="button" className="ghost small" onClick={() => { if (inTauri) void openPath(dev.path).catch(() => undefined); }}>Open data folder</button>
          </div>
        )}
        <header className="settings-head">
          <div className="settings-title">
            <h2>Settings</h2>
            <p className="lede">Global defaults for every new scan.</p>
          </div>
          <div className="settings-find">
            <input
              type="search"
              aria-label="Find a setting"
              placeholder="Find a setting…"
              value={query}
              spellCheck={false}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Escape') setQuery(''); }}
            />
            {query && (
              <button type="button" className="ghost small settings-find-clear" aria-label="Clear the search" onClick={() => setQuery('')}>×</button>
            )}
          </div>
        </header>

        <div className="settings-scope-note">
          <span className="settings-scope-icon" aria-hidden="true">◇</span>
          <p>
            <b>Every album can carry its own shoot type, blur strictness, burst gap, and whether it reads plates, race numbers and people.</b>{' '}
            Where it does, the album's value wins. Saving here changes the defaults new scans start from; an album you have already scanned keeps what it was scanned with.
          </p>
        </div>

        <div className="settings-main">
          <nav className="settings-tabs" role="tablist" aria-label="Settings sections" onKeyDown={onTabKey}>
            {TABS.map((tab) => {
              const dirtyCount = tabsDirty[tab.id];
              const selected = !searching && activeTab === tab.id;
              return (
                <button
                  key={tab.id}
                  type="button"
                  role="tab"
                  id={`settings-tab-${tab.id}`}
                  aria-selected={selected}
                  aria-controls={selected ? `settings-panel-${tab.id}` : undefined}
                  tabIndex={activeTab === tab.id ? 0 : -1}
                  className={`settings-tab-btn${selected ? ' active' : ''}`}
                  onClick={() => pickTab(tab.id)}
                >
                  <span>{tab.label}</span>
                  {dirtyCount > 0 && <span className="tab-dirty-dot" aria-hidden="true" title={`${dirtyCount} unsaved change${dirtyCount === 1 ? '' : 's'}`} />}
                  {dirtyCount > 0 && <span className="sr-only">{dirtyCount} unsaved change{dirtyCount === 1 ? '' : 's'}</span>}
                </button>
              );
            })}
          </nav>

          {searching ? (
            <div className="settings-search" role="region" aria-label={`Settings matching ${query.trim()}`}>
              <div className="settings-search-head">
                <h3>{results.length ? `${plural(results.length, 'setting')} matching “${query.trim()}”` : `No settings match “${query.trim()}”`}</h3>
                <button type="button" className="ghost small" onClick={() => setQuery('')}>Clear search</button>
              </div>
              {resultGroups.length === 0 ? (
                <p className="settings-search-empty">Try “burst”, “plate”, “strictness” or “tray”.</p>
              ) : resultGroups.map(({ section, matches }) => (
                <section className="settings-section settings-search-group" key={section.id} aria-labelledby={`settings-search-${section.id}`}>
                  <header className="settings-section-head">
                    <h3 id={`settings-search-${section.id}`}>{section.title}</h3>
                    <p className="section-hint">{tabById(section.tab)?.label}</p>
                  </header>
                  {matches.map((match) => (
                    <SettingRow
                      key={match.key}
                      field={FIELDS[match.key]}
                      value={draft[match.key]}
                      changed={draft[match.key] !== settings[match.key]}
                      onChange={(value) => setValue(match.key, value)}
                    />
                  ))}
                </section>
              ))}
            </div>
          ) : (
            <div
              className="settings-tab-content"
              role="tabpanel"
              id={`settings-panel-${activeTab}`}
              aria-labelledby={`settings-tab-${activeTab}`}
              tabIndex={0}
            >
              {activeTab === 'culling' && SECTIONS.filter((section) => section.tab === 'culling').map((section) => (
                <SettingsSection key={section.id} section={section} draft={draft} saved={settings} onChange={setValue} />
              ))}

              {activeTab === 'identification' && (
                <>
                  {SECTIONS.filter((section) => section.tab === 'identification').map((section) => (
                    section.id === 'ai-models' ? (
                      <AiSettingsCard
                        key={section.id}
                        draft={draft}
                        saved={settings}
                        models={models}
                        onChange={setValue}
                        toast={toast}
                        onModelsChanged={onModelsChanged}
                      />
                    ) : (
                      <SettingsSection key={section.id} section={section} draft={draft} saved={settings} onChange={setValue} />
                    )
                  ))}

                  <IdentifyConcurrency draft={draft} onChange={setValue} />

                  <details className="settings-section settings-advanced">
                    <summary>
                      <span className="summary-title" role="heading" aria-level={3}>Model benchmark</span>
                      <span className="summary-hint">Pick a sample, mark the truth and score vision models against it.</span>
                    </summary>
                    <div className="settings-advanced-body">
                      <Benchmark run={run} toast={toast} settings={draft} status={status} />
                    </div>
                  </details>
                </>
              )}

              {activeTab === 'metadata' && SECTIONS.filter((section) => section.tab === 'metadata').map((section) => (
                <SettingsSection key={section.id} section={section} draft={draft} saved={settings} onChange={setValue} />
              ))}

              {activeTab === 'lightroom' && SECTIONS.filter((section) => section.tab === 'lightroom').map((section) => (
                section.id === 'lightroom' ? (
                  <LightroomBridge
                    key={section.id}
                    value={draft.lightroom_bridge ?? false}
                    changed={draft.lightroom_bridge !== settings.lightroom_bridge}
                    onChange={(value) => setValue('lightroom_bridge', value)}
                    run={run}
                    toast={toast}
                  />
                ) : (
                  <SettingsSection key={section.id} section={section} draft={draft} saved={settings} onChange={setValue} />
                )
              ))}

              {activeTab === 'system' && (
                <section className="settings-section settings-system" aria-labelledby="settings-system-head">
                  <header className="settings-section-head">
                    <h3 id="settings-system-head">Maintenance</h3>
                    <p className="section-hint">Check the models this machine uses, clear cached previews, or reset the library. These actions never touch your original photographs.</p>
                  </header>
                  <Maintenance />
                </section>
              )}
            </div>
          )}
        </div>

        <aside className="settings-sidebar" aria-label="Status and library">
          <UpdateCard
            info={updateInfo}
            upToDate={upToDate}
            error={updateError}
            checking={checkingUpdate}
            installing={installingUpdate}
            progress={updateProgress}
            version={appVersion}
            onCheck={() => void checkForUpdates(true)}
            onInstall={handleInstallUpdate}
            onOpenWhatsNew={onOpenWhatsNew}
          />

          <section className="sidebar-card" aria-labelledby="settings-setup">
            <h4 id="settings-setup">Set up Conrod</h4>
            <p className="sidebar-card-note">Tell Conrod what you shoot and it tunes what it looks for, reads and how picky it is. Albums keep their own choices.</p>
            <button type="button" className="ghost small sidebar-card-action" onClick={onOpenWizard} disabled={!onOpenWizard}>
              Set up Conrod again
            </button>
          </section>

          <section className="sidebar-card" aria-labelledby="settings-library">
            <h4 id="settings-library">This library</h4>
            <dl className="sidebar-stat-grid">
              <div className="sidebar-stat-item">
                <dt className="stat-label">Albums</dt>
                <dd className="stat-value">{jobs.length}</dd>
              </div>
              <div className="sidebar-stat-item">
                <dt className="stat-label">Photos</dt>
                <dd className="stat-value">{totalPhotos.toLocaleString()}</dd>
              </div>
              <div className="sidebar-stat-item">
                <dt className="stat-label">Known vehicles</dt>
                <dd className="stat-value">{knownCount === null ? '—' : knownCount.toLocaleString()}</dd>
              </div>
            </dl>
            <button type="button" className="ghost small sidebar-card-action" onClick={() => setCatalogOpen(true)}>
              Open known vehicles →
            </button>
          </section>

          <section className="sidebar-card" aria-labelledby="settings-overrides">
            <h4 id="settings-overrides">Album overrides</h4>
            <p className="sidebar-card-note">Albums whose own value differs from these defaults.</p>
            <ul className="override-list">
              {overrides.map((override) => (
                <li className="override-row" key={override.key}>
                  <span>{override.label}</span>
                  <span className={`override-count${override.count ? '' : ' is-default'}`}>
                    {override.count ? plural(override.count, 'album') : 'defaults'}
                  </span>
                </li>
              ))}
            </ul>
          </section>
        </aside>
      </div>
      {saveBar}
    </div>
  );
}
