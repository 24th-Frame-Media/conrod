import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { KeyboardEvent as ReactKeyboardEvent } from 'react';
import { getVersion } from '@tauri-apps/api/app';
import { openUrl } from '@tauri-apps/plugin-opener';
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
import { Benchmark } from './Benchmark';
import { Known } from './Known';
import type {
  Job,
  ModelInfo,
  OllamaModel,
  SettingValue,
  Settings as SettingsMap,
  Status,
  Toaster,
  VlmProvider,
  VlmService,
} from '../lib/types';
import type { Runner } from '../state/hooks';

export const DEFAULT_OLLAMA_HOST = 'http://127.0.0.1:11434';

const VLM_PROVIDERS: { id: VlmProvider; name: string; defaultModel: string }[] = [
  { id: 'ollama', name: 'Ollama (local or self-hosted)', defaultModel: 'qwen2.5vl:7b' },
  { id: 'openai', name: 'OpenAI (GPT-4o and newer)', defaultModel: 'gpt-4o' },
  { id: 'anthropic', name: 'Anthropic (Claude 3.5 / 3.7)', defaultModel: 'claude-3-7-sonnet-20250219' },
  { id: 'gemini', name: 'Google Gemini', defaultModel: 'gemini-1.5-flash' },
];

const KEY_KINDS = [
  { id: 'auto', name: 'Auto-detect' },
  { id: 'api-key', name: 'API key' },
  { id: 'claude-code', name: 'Claude Code token' },
];

function getVlmServices(draft: SettingsMap): VlmService[] {
  const value = draft.vlm_services;
  return Array.isArray(value) ? (value as unknown as VlmService[]) : [];
}

/** "Ollama · host:port" for a server card title, or just the provider name for cloud services. */
function serverTitle(service: VlmService): string {
  if (service.provider !== 'ollama') return VLM_PROVIDERS.find((p) => p.id === service.provider)?.name.split(' (')[0] || service.provider;
  const host = (service.host || '').replace(/^https?:\/\//, '');
  return `Ollama · ${host}`;
}

/** A cloud model text field that only commits on blur or Enter, not on every keystroke. */
function CloudModelInput({ model, title, onCommit }: { model: string; title: string; onCommit: (model: string) => void }) {
  const [value, setValue] = useState(model);
  useEffect(() => setValue(model), [model]);
  return (
    <input
      type="text"
      aria-label={`Model for ${title}`}
      value={value}
      spellCheck={false}
      onChange={(e) => setValue(e.target.value)}
      onBlur={() => { if (value.trim() && value !== model) onCommit(value.trim()); }}
      onKeyDown={(e) => { if (e.key === 'Enter') (e.target as HTMLInputElement).blur(); }}
    />
  );
}

type VlmServerCardProps = {
  service: VlmService;
  status?: ModelInfo;
  onChangeModel: (model: string) => void;
  onToggleEnabled: () => void;
  onRemove: () => void;
};

/** One vision server: status dot, model picker, enabled toggle, remove. */
function VlmServerCard({ service, status, onChangeModel, onToggleEnabled, onRemove }: VlmServerCardProps) {
  const [models, setModels] = useState<OllamaModel[]>([]);
  const [loading, setLoading] = useState(false);
  const title = serverTitle(service);

  const load = useCallback(() => {
    if (service.provider !== 'ollama') return;
    setLoading(true);
    void engine.ollamaModels(service.host)
      .then((res) => setModels(res.models.filter((model) => model.vision)))
      .catch(() => setModels([]))
      .finally(() => setLoading(false));
  }, [service.provider, service.host]);

  useEffect(() => { load(); }, [load]);

  return (
    <div className="vlm-server-card">
      <div className="vlm-server-head">
        <span className={`dot ${status ? (status.ready ? 'ok' : 'no') : ''}`} aria-hidden="true" />
        <span className="vlm-server-title">{title}</span>
        <span className="sr-only">{status ? (status.ready ? 'Ready' : 'Unavailable') : 'Not checked yet'}</span>
      </div>
      {service.provider === 'ollama' ? (
        <div className="model-input-row">
          <select aria-label={`Model for ${title}`} value={service.model} onChange={(e) => onChangeModel(e.target.value)}>
            {!models.some((model) => model.name === service.model) && service.model && (
              <option value={service.model}>{service.model}</option>
            )}
            {models.map((model) => <option key={model.name} value={model.name}>{model.name}</option>)}
          </select>
          <button type="button" className="ghost small refresh-models-btn" disabled={loading} onClick={load} aria-label={`Refresh models for ${title}`} title="Refresh models">
            {loading ? '…' : '↻'}
          </button>
        </div>
      ) : (
        <CloudModelInput model={service.model} title={title} onCommit={onChangeModel} />
      )}
      {status?.detail && <p className="muted vlm-server-detail">{status.detail}</p>}
      <div className="vlm-server-foot">
        <label className="vlm-service-enabled">
          <input type="checkbox" checked={service.enabled} onChange={onToggleEnabled} />
          Enabled
        </label>
        <button type="button" className="ghost small" aria-label={`Remove ${title}`} onClick={onRemove}>Remove</button>
      </div>
    </div>
  );
}

/** The add-a-provider form; local state so typing here never disturbs the saved settings. */
function VlmAddForm({ onAdd }: { onAdd: (service: VlmService) => void }) {
  const [open, setOpen] = useState(false);
  const [provider, setProvider] = useState<VlmProvider>('ollama');
  const [host, setHost] = useState(DEFAULT_OLLAMA_HOST);
  const [model, setModel] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [keyKind, setKeyKind] = useState('auto');
  const [models, setModels] = useState<OllamaModel[]>([]);
  const [online, setOnline] = useState<boolean | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async (targetHost?: string) => {
    setLoading(true);
    try {
      const res = await engine.ollamaModels(targetHost);
      setOnline(res.online);
      setModels(res.models);
    } catch {
      setOnline(false);
      setModels([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (open && provider === 'ollama') void load(host);
  }, [open, provider, host, load]);

  const add = () => {
    const trimmed = model.trim();
    if (!trimmed) return;
    onAdd({
      id: `vlm-${Date.now()}`,
      provider,
      model: trimmed,
      enabled: true,
      ...(provider === 'ollama' ? { host } : { api_key: apiKey }),
      ...(provider === 'anthropic' ? { key_kind: keyKind } : {}),
    });
    setOpen(false);
    setModel('');
    setApiKey('');
    setKeyKind('auto');
  };

  if (!open) {
    return <button type="button" className="ghost small settings-add" onClick={() => setOpen(true)}>+ Add server</button>;
  }

  return (
    <div className="vlm-add-form">
      <p className="vlm-add-title">Add a vision provider</p>
      <label className="setting">
        <span className="label">Type</span>
        <select
          aria-label="Provider type"
          value={provider}
          onChange={(e) => {
            const next = e.target.value as VlmProvider;
            setProvider(next);
            setModel(VLM_PROVIDERS.find((item) => item.id === next)?.defaultModel || '');
          }}
        >
          {VLM_PROVIDERS.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
        </select>
      </label>

      {provider === 'ollama' ? (
        <>
          <label className="setting">
            <span className="label">Host</span>
            <input type="text" value={host} spellCheck={false} onChange={(e) => setHost(e.target.value)} />
          </label>
          <label className="setting">
            <span className="label">Model</span>
            <div className="model-input-row">
              <select aria-label="Model for the new server" value={model} onChange={(e) => setModel(e.target.value)}>
                <option value="">Select a model…</option>
                {models.filter((m) => m.vision).map((m) => (
                  <option key={m.name} value={m.name}>{m.name}</option>
                ))}
              </select>
              <button type="button" className="ghost small refresh-models-btn" disabled={loading} onClick={() => void load(host)} aria-label="Refresh models for the new server">
                {loading ? '…' : '↻'}
              </button>
            </div>
            {online === false && <span className="status-badge offline">○ Ollama offline at {host}</span>}
          </label>
        </>
      ) : (
        <>
          <label className="setting">
            <span className="label">API key</span>
            <input type="password" value={apiKey} spellCheck={false} onChange={(e) => setApiKey(e.target.value)} />
          </label>
          {provider === 'anthropic' && (
            <label className="setting">
              <span className="label">Key kind</span>
              <select value={keyKind} onChange={(e) => setKeyKind(e.target.value)}>
                {KEY_KINDS.map((kind) => <option key={kind.id} value={kind.id}>{kind.name}</option>)}
              </select>
            </label>
          )}
          <label className="setting">
            <span className="label">Model</span>
            <input type="text" value={model} spellCheck={false} placeholder={VLM_PROVIDERS.find((p) => p.id === provider)?.defaultModel} onChange={(e) => setModel(e.target.value)} />
          </label>
        </>
      )}

      <div className="actions-row">
        <button type="button" className="primary small" disabled={!model.trim()} onClick={add}>Add</button>
        <button type="button" className="ghost small" onClick={() => setOpen(false)}>Cancel</button>
      </div>
    </div>
  );
}

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
};

function UpdateCard({ info, upToDate, error, checking, installing, progress, version, onCheck, onInstall }: UpdateCardProps) {
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
          <button type="button" className="sidebar-ext-link" onClick={() => void openUrl('https://github.com/kapsikkum/conrod/releases')}>
            Releases ↗
          </button>
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
};

/** Settings shared with the Python library; Save writes the whole block back. */
export function Settings({ settings, onSaved, run, toast, jobs = [], models = [], onModelsChanged, status }: Props) {
  const [draft, setDraft] = useState<SettingsMap>(settings);
  useEffect(() => setDraft(settings), [settings]);

  const [activeTab, setActiveTab] = useState<SettingsTabId>('culling');
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

  const services = getVlmServices(draft);
  const enabledServices = services.filter((service) => service.enabled).length;
  const strategyName = FIELDS.vlm_strategy.options?.find((option) => option.id === String(draft.vlm_strategy ?? 'least_busy'))?.name ?? 'Least busy';
  const overrides = useMemo(() => albumOverrideCounts(jobs, settings), [jobs, settings]);
  const totalPhotos = jobs.reduce((sum, job) => sum + job.total, 0);

  const setValue = useCallback((key: string, value: SettingValue) => {
    setDraft((current) => ({ ...current, [key]: value }));
  }, []);

  const setServices = (next: VlmService[]) => {
    setDraft((current) => ({ ...current, vlm_services: next as unknown as SettingValue }));
  };

  const save = () => void run(async () => {
    onSaved(await engine.saveSettings(draft));
    toast('Settings saved.', { tone: 'ok' });
    onModelsChanged?.();
  });

  const discard = () => setDraft(settings);

  const learn = () => void run(async () => {
    const out = await engine.trainTaste();
    toast(typeof out.n === 'number' ? `Learned from ${out.n} of your ratings.` : 'Learned from your ratings.', { tone: 'ok' });
  });

  const openTab = (id: SettingsTabId) => {
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
    openTab(ids[next]);
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
                  onClick={() => openTab(tab.id)}
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
                <SettingsSection key={section.id} section={section} draft={draft} saved={settings} onChange={setValue}>
                  {section.id === 'culling' && (
                    <div className="settings-action-row">
                      <div>
                        <b>Learn from my ratings</b>
                        <p className="hint">Fit Conrod's star ratings to the ones you have given by hand.</p>
                      </div>
                      <button type="button" className="ghost small" onClick={learn}>Learn now</button>
                    </div>
                  )}
                </SettingsSection>
              ))}

              {activeTab === 'identification' && (
                <>
                  {SECTIONS.filter((section) => section.tab === 'identification').map((section) => (
                    section.id === 'vision-providers' ? (
                      <section key={section.id} className="settings-section" aria-labelledby="settings-section-vision-providers">
                        <header className="settings-section-head">
                          <h3 id="settings-section-vision-providers">Vision providers</h3>
                          <p className="section-hint">One or more servers that read make, model, colour and team. Work is spread across the ones that are on.</p>
                        </header>
                        <SettingRow
                          field={FIELDS.vlm_strategy}
                          value={draft.vlm_strategy ?? 'least_busy'}
                          changed={draft.vlm_strategy !== settings.vlm_strategy}
                          onChange={(value) => setValue('vlm_strategy', value)}
                        />
                        <div className="vlm-server-grid">
                          {services.map((service) => {
                            const serviceStatus = models.find((model) => model.file === 'vlm' && model.id === service.id);
                            return (
                              <VlmServerCard
                                key={service.id}
                                service={service}
                                status={serviceStatus}
                                onChangeModel={(model) => setServices(services.map((item) => (item.id === service.id ? { ...item, model } : item)))}
                                onToggleEnabled={() => setServices(services.map((item) => (item.id === service.id ? { ...item, enabled: !item.enabled } : item)))}
                                onRemove={() => setServices(services.filter((item) => item.id !== service.id))}
                              />
                            );
                          })}
                          {services.length === 0 && <p className="vlm-empty muted">No vision providers yet. Add a local Ollama server or a cloud key below.</p>}
                        </div>
                        <VlmAddForm onAdd={(service) => setServices([...services, service])} />
                      </section>
                    ) : (
                      <SettingsSection key={section.id} section={section} draft={draft} saved={settings} onChange={setValue} />
                    )
                  ))}

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
          />

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

          <section className="sidebar-card" aria-labelledby="settings-models">
            <h4 id="settings-models">Models</h4>
            <div className="sidebar-meta-list">
              <div className="sidebar-meta-row">
                <span className="meta-key">Vision providers</span>
                <span className="meta-val">{enabledServices} of {services.length} on</span>
              </div>
              <div className="sidebar-meta-row">
                <span className="meta-key">Load balancing</span>
                <span className="meta-val">{strategyName}</span>
              </div>
            </div>
            {models.length > 0 && (
              <>
                <h5 className="sidebar-subhead">On this machine</h5>
                <div className="sidebar-meta-list">
                  {models.map((model) => (
                    <div className="sidebar-meta-row" key={model.name}>
                      <span className="meta-key truncate" title={model.name}>
                        <span className={`dot ${model.ready ? 'ok' : 'no'}`} aria-hidden="true" />
                        {model.name}
                      </span>
                      <span className="meta-val small">{model.ready ? 'Ready' : 'Unavailable'}</span>
                    </div>
                  ))}
                </div>
              </>
            )}
          </section>
        </aside>
      </div>
      {saveBar}
    </div>
  );
}
