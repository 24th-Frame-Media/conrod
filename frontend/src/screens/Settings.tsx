import { Maintenance } from '../components/Maintenance';
import { Benchmark } from './Benchmark';
import { Known } from './Known';
import { useEffect, useState, useCallback } from 'react';
import { getVersion } from '@tauri-apps/api/app';
import { openUrl } from '@tauri-apps/plugin-opener';
import { engine, inTauri } from '../lib/api';
import { words } from '../lib/format';

export const DEFAULT_OLLAMA_HOST = 'http://127.0.0.1:11434';
import type {
  Status,
  Settings as SettingsMap,
  SettingValue,
  Toaster,
  UpdateInfo,
  OllamaModel,
  Job,
  ModelInfo,
  VlmService,
  VlmProvider,
  VlmStrategy,
} from '../lib/types';
import type { Runner } from '../state/hooks';

const GROUPS: [string, string[]][] = [
  ['Detection & focus', ['detect_conf', 'min_box_fraction', 'max_vehicles_per_frame', 'include_cars', 'include_bikes', 'include_trucks', 'sharp_at', 'blurred_below', 'auto_reject_below_stars', 'cull_blurred', 'burst_gap']],
  ['Identification & AI', ['read_plates', 'read_numbers', 'read_text', 'use_vlm', 'vlm_timeout', 'vlm_max_retries', 'vlm_input_edge', 'use_known_vehicles', 'normalise_names']],
  ['Metadata & desktop', ['write_rating', 'write_label', 'overwrite_rating', 'overwrite_label', 'write_caption', 'overwrite_caption', 'write_plate_keyword', 'write_sidecar_for_raw', 'keyword_prefix', 'close_to_tray']],
];

/** [label, hint]; a key without an entry falls back to its own name. */
const META: Record<string, [string, string?]> = {
  detect_conf: ['Detection confidence', 'Lower finds more vehicles, and more false ones.'],
  min_box_fraction: ['Smallest subject', 'As a fraction of the frame.'],
  max_vehicles_per_frame: ['Vehicles per frame'],
  sharp_at: ['Counts as sharp at', 'Subject sharpness at or above this.'],
  blurred_below: ['Counts as blurred below'],
  auto_reject_below_stars: ['Auto-reject below (stars)', '0 never rejects on its own.'],
  cull_blurred: ['Cull blurred frames', 'A red rating, never a deletion.'],
  burst_gap: ['Burst gap (seconds)', 'Frames closer than this belong to one pass.'],
  use_vlm: ['Use vision model', 'Identify make, model, colour and team.'],
  vlm_timeout: ['Timeout (seconds)'],
  vlm_max_retries: ['Max retries'],
  vlm_input_edge: ['Input image edge (px)', 'Images are resized so their longest edge fits this before upload.'],
  use_known_vehicles: ['Use known vehicles', 'Fill blanks from cars you have met.'],
  normalise_names: ['Tidy makes and models'],
  write_sidecar_for_raw: ['Write sidecars next to RAW files'],
  keyword_prefix: ['Keyword prefix'],
  close_to_tray: ['Keep running in the tray when closed'],
};
const labelOf = (key: string) => META[key]?.[0] ?? words(key).replace(/^./, (c) => c.toUpperCase());

const VLM_PROVIDERS: { id: VlmProvider; name: string; defaultModel: string }[] = [
  { id: 'ollama', name: 'Ollama (Local / Self-hosted)', defaultModel: 'qwen2.5vl:7b' },
  { id: 'openai', name: 'OpenAI (GPT-4o, etc.)', defaultModel: 'gpt-4o' },
  { id: 'anthropic', name: 'Anthropic (Claude 3.5 / 3.7)', defaultModel: 'claude-3-7-sonnet-20250219' },
  { id: 'gemini', name: 'Google Gemini', defaultModel: 'gemini-1.5-flash' },
];

const VLM_STRATEGIES: { id: VlmStrategy; name: string }[] = [
  { id: 'least_busy', name: 'Least busy' },
  { id: 'round_robin', name: 'Round robin' },
  { id: 'random', name: 'Random' },
];

const KEY_KINDS = [
  { id: 'auto', name: 'Auto-detect' },
  { id: 'api-key', name: 'API key' },
  { id: 'claude-code', name: 'Claude Code token' },
];

function getVlmServices(draft: SettingsMap): VlmService[] {
  const v = draft.vlm_services;
  return Array.isArray(v) ? (v as unknown as VlmService[]) : [];
}

/** "Ollama · host:port" for a server card title, or just the provider name for cloud services. */
function serverTitle(s: VlmService): string {
  if (s.provider !== 'ollama') return VLM_PROVIDERS.find((p) => p.id === s.provider)?.name.split(' (')[0] || s.provider;
  const host = (s.host || '').replace(/^https?:\/\//, '');
  return `Ollama · ${host}`;
}

/** A cloud model text field that only commits (and thus saves) on blur/Enter, not per keystroke. */
function CloudModelInput({ model, onCommit }: { model: string; onCommit: (model: string) => void }) {
  const [value, setValue] = useState(model);
  useEffect(() => setValue(model), [model]);
  return (
    <input
      type="text"
      aria-label="Model"
      value={value}
      spellCheck={false}
      onChange={(e) => setValue(e.target.value)}
      onBlur={() => { if (value.trim() && value !== model) onCommit(value.trim()); }}
      onKeyDown={(e) => { if (e.key === 'Enter') (e.target as HTMLInputElement).blur(); }}
    />
  );
}

/** One VLM server: status dot, model picker (select for Ollama, text for cloud), enabled toggle, remove. */
function VlmServerCard({ service, status, onChangeModel, onToggleEnabled, onRemove }: {
  service: VlmService;
  status?: ModelInfo;
  onChangeModel: (model: string) => void;
  onToggleEnabled: () => void;
  onRemove: () => void;
}) {
  const [models, setModels] = useState<OllamaModel[]>([]);
  const [loading, setLoading] = useState(false);

  const load = useCallback(() => {
    if (service.provider !== 'ollama') return;
    setLoading(true);
    void engine.ollamaModels(service.host)
      .then((res) => setModels(res.models.filter((m) => m.vision)))
      .catch(() => setModels([]))
      .finally(() => setLoading(false));
  }, [service.provider, service.host]);

  useEffect(() => { load(); }, [load]);

  return (
    <div className="vlm-server-card">
      <div className="vlm-server-head">
        <span className={`dot ${status ? (status.ready ? 'ok' : 'no') : ''}`} title={status?.detail} />
        <span className="vlm-server-title">{serverTitle(service)}</span>
      </div>
      {service.provider === 'ollama' ? (
        <div className="model-input-row">
          <select aria-label="Model" value={service.model} onChange={(e) => onChangeModel(e.target.value)}>
            {!models.some((m) => m.name === service.model) && service.model && (
              <option value={service.model}>{service.model}</option>
            )}
            {models.map((m) => <option key={m.name} value={m.name}>{m.name}</option>)}
          </select>
          <button type="button" className="ghost small refresh-models-btn" disabled={loading} onClick={load} title="Refresh models">
            {loading ? '…' : '↻'}
          </button>
        </div>
      ) : (
        <CloudModelInput model={service.model} onCommit={onChangeModel} />
      )}
      {status?.detail && <p className="muted vlm-server-detail">{status.detail}</p>}
      <div className="vlm-server-foot">
        <label className="vlm-service-enabled">
          <input type="checkbox" checked={service.enabled} onChange={onToggleEnabled} />
          Enabled
        </label>
        <button type="button" className="ghost small" onClick={onRemove}>Remove</button>
      </div>
    </div>
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

type SettingsTab = 'detection' | 'vlm' | 'metadata' | 'maintenance' | 'known';

const SETTINGS_TABS: { id: SettingsTab; label: string }[] = [
  { id: 'detection', label: 'Detection & Culling' },
  { id: 'vlm', label: 'Vision & AI' },
  { id: 'metadata', label: 'Metadata & Desktop' },
  { id: 'maintenance', label: 'System & Maintenance' },
  { id: 'known', label: 'Known Vehicles' },
];

/** Settings shared with the Python library; Save writes the whole block back. */
export function Settings({ settings, onSaved, run, toast, jobs = [], models = [], onModelsChanged, status }: Props) {
  const [draft, setDraft] = useState<SettingsMap>(settings);
  useEffect(() => setDraft(settings), [settings]);
  const dirty = JSON.stringify(draft) !== JSON.stringify(settings);
  const [activeTab, setActiveTab] = useState<SettingsTab>('detection');
  const [knownCount, setKnownCount] = useState<number | null>(null);

  useEffect(() => {
    let active = true;
    void engine.known().then((rows) => {
      if (active) setKnownCount(rows.length);
    }).catch(() => {});
    return () => { active = false; };
  }, [activeTab]);

  const isTabDirty = useCallback((tabId: SettingsTab): boolean => {
    let keys: string[] = [];
    if (tabId === 'detection') keys = GROUPS[0][1];
    else if (tabId === 'vlm') keys = GROUPS[1][1];
    else if (tabId === 'metadata') keys = GROUPS[2][1];
    return keys.some((k) => draft[k] !== settings[k]);
  }, [draft, settings]);

  // App Update state
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [installingUpdate, setInstallingUpdate] = useState(false);
  const [appVersion, setAppVersion] = useState<string | null>(null);
  useEffect(() => {
    if (inTauri) void getVersion().then(setAppVersion);
  }, []);

  // Ollama Models state, used by the "add VLM provider" form below.
  const [ollamaModels, setOllamaModels] = useState<OllamaModel[]>([]);
  const [ollamaOnline, setOllamaOnline] = useState<boolean | null>(null);
  const [ollamaLoading, setOllamaLoading] = useState(false);

  const checkForUpdates = useCallback(async (force = false) => {
    setCheckingUpdate(true);
    try {
      const res = await engine.checkUpdate(force);
      setUpdateInfo(res);
      if (force) {
        if (res.newer) {
          toast(`Update available: v${res.latest}`, { tone: 'ok' });
        } else if (res.ok) {
          toast(`Conrod is up to date (v${res.current}).`, { tone: 'ok' });
        } else if (res.error) {
          toast(res.error, { tone: 'error' });
        }
      }
    } catch (e) {
      if (force) toast(String(e), { tone: 'error' });
    } finally {
      setCheckingUpdate(false);
    }
  }, [toast]);

  useEffect(() => {
    void checkForUpdates(false);
  }, [checkForUpdates]);

  const handleInstallUpdate = () => void run(async () => {
    setInstallingUpdate(true);
    try {
      await engine.installUpdate();
      toast('Installing update and restarting Conrod...', { tone: 'ok' });
    } catch (e) {
      setInstallingUpdate(false);
      toast(String(e), { tone: 'error' });
    }
  });

  const fetchOllamaModels = useCallback(async (host?: string) => {
    setOllamaLoading(true);
    try {
      const res = await engine.ollamaModels(host);
      setOllamaOnline(res.online);
      setOllamaModels(res.models);
    } catch {
      setOllamaOnline(false);
      setOllamaModels([]);
    } finally {
      setOllamaLoading(false);
    }
  }, []);

  const save = () => void run(async () => {
    onSaved(await engine.saveSettings(draft));
    toast('Settings saved.', { tone: 'ok' });
    onModelsChanged?.();
  });

  // Add-VLM-provider form state.
  const [showAddVlm, setShowAddVlm] = useState(false);
  const [newProvider, setNewProvider] = useState<VlmProvider>('ollama');
  const [newHost, setNewHost] = useState(DEFAULT_OLLAMA_HOST);
  const [newModel, setNewModel] = useState('');
  const [newApiKey, setNewApiKey] = useState('');
  const [newKeyKind, setNewKeyKind] = useState('auto');

  useEffect(() => {
    if (showAddVlm && newProvider === 'ollama') void fetchOllamaModels(newHost);
  }, [showAddVlm, newProvider, newHost, fetchOllamaModels]);

  const addVlmService = () => {
    if (!newModel.trim()) return;
    const service: VlmService = {
      id: `vlm-${Date.now()}`,
      provider: newProvider,
      model: newModel.trim(),
      enabled: true,
      ...(newProvider === 'ollama' ? { host: newHost } : { api_key: newApiKey }),
      ...(newProvider === 'anthropic' ? { key_kind: newKeyKind } : {}),
    };
    setDraft((d) => ({ ...d, vlm_services: [...getVlmServices(d), service] as unknown as SettingValue }));
    setShowAddVlm(false);
    setNewModel('');
    setNewApiKey('');
    setNewKeyKind('auto');
  };

  const removeVlmService = (id: string) => {
    setDraft((d) => ({ ...d, vlm_services: getVlmServices(d).filter((s) => s.id !== id) as unknown as SettingValue }));
  };

  /** Patches one VLM service and, when `autosave`, persists immediately (used for the model picker). */
  const setVlmService = (id: string, patch: Partial<VlmService>, autosave = false) => {
    const next = getVlmServices(draft).map((s) => (s.id === id ? { ...s, ...patch } : s));
    const nextDraft = { ...draft, vlm_services: next as unknown as SettingValue };
    setDraft(nextDraft);
    if (autosave) void run(async () => { onSaved(await engine.saveSettings(nextDraft)); toast('Settings saved.', { tone: 'ok' }); onModelsChanged?.(); });
  };

  const toggleVlmServiceEnabled = (id: string) => {
    setDraft((d) => ({
      ...d,
      vlm_services: getVlmServices(d).map((s) => (s.id === id ? { ...s, enabled: !s.enabled } : s)) as unknown as SettingValue,
    }));
  };

  const learn = () => void run(async () => {
    const out = await engine.trainTaste();
    toast(typeof out.n === 'number' ? `Learned from ${out.n} of your ratings.` : 'Learned from your ratings.', { tone: 'ok' });
  });

  function Control({
    name,
    value,
    onChange,
  }: {
    name: string;
    value: SettingValue;
    onChange: (v: SettingValue) => void;
  }) {
    if (typeof value === 'boolean') return <input type="checkbox" aria-label={labelOf(name)} checked={value} onChange={(e) => onChange(e.target.checked)} />;
    if (typeof value === 'number') return <input type="number" step="any" aria-label={labelOf(name)} value={value} onChange={(e) => onChange(Number(e.target.value))} />;
    return <input type={name.includes('api_key') ? 'password' : 'text'} aria-label={labelOf(name)} value={value} spellCheck={false} onChange={(e) => onChange(e.target.value)} />;
  }

  const totalPhotos = jobs.reduce((n, j) => n + j.total, 0);

  return (
    <div className="screen settings-screen">
      <div className={`settings-layout ${activeTab === 'known' ? 'known-layout' : ''}`}>
        <div className={`pane settings-main ${activeTab === 'known' ? 'known-pane' : ''}`}>
          <h2>Settings</h2>
          <p className="lede">Shared with your existing Conrod library. Defaults are what measured best on real frames.</p>

          {/* Settings Navigation Tabs */}
          <nav className="settings-tabs" role="tablist" aria-label="Settings categories">
            {SETTINGS_TABS.map((t) => (
              <button
                key={t.id}
                role="tab"
                aria-selected={activeTab === t.id}
                className={`settings-tab-btn ${activeTab === t.id ? 'active' : ''}`}
                onClick={() => setActiveTab(t.id)}
              >
                <span>{t.label}</span>
                {isTabDirty(t.id) && <span className="tab-dirty-dot" title="Unsaved changes in this tab" />}
              </button>
            ))}
          </nav>

          {/* Tab 1: Detection & Culling */}
          {activeTab === 'detection' && (
            <div className="settings-tab-content">
              <section className="setting-group">
                <h3>Detection & Culling</h3>
                {GROUPS[0][1].filter((k) => draft[k] !== undefined).map((k) => (
                  <label className="setting" key={k}>
                    <span><span className="label">{labelOf(k)}</span>{META[k]?.[1] && <span className="hint">{META[k]?.[1]}</span>}</span>
                    <Control
                      name={k}
                      value={draft[k]}
                      onChange={(v) => setDraft((d) => ({ ...d, [k]: v }))}
                    />
                  </label>
                ))}
              </section>
              <div className="actions-row">
                <button className="primary" disabled={!dirty} onClick={save}>Save</button>
                <button className="ghost" disabled={!dirty} onClick={() => setDraft(settings)}>Discard changes</button>
                <button className="ghost" title="Fit Conrod's star ratings to the ones you have given by hand." onClick={learn}>Learn from my ratings</button>
                {dirty && <span className="muted">Unsaved changes</span>}
              </div>
            </div>
          )}

          {/* Tab 2: Vision & AI */}
          {activeTab === 'vlm' && (
            <div className="settings-tab-content">
              <section className="setting-group">
                <div className="vision-section-head">
                  <h3>Vision providers</h3>
                  <div className="segmented" role="radiogroup" aria-label="Strategy">
                    {VLM_STRATEGIES.map((s) => (
                      <button
                        key={s.id}
                        type="button"
                        role="radio"
                        aria-checked={String(draft.vlm_strategy || 'least_busy') === s.id}
                        className={`segmented-btn ${String(draft.vlm_strategy || 'least_busy') === s.id ? 'active' : ''}`}
                        onClick={() => setDraft((d) => ({ ...d, vlm_strategy: s.id }))}
                      >
                        {s.name}
                      </button>
                    ))}
                  </div>
                </div>

                <div className="vlm-server-grid">
                  {getVlmServices(draft).map((s) => {
                    const status = models.find((m) => m.file === 'vlm' && m.id === s.id);
                    return (
                      <VlmServerCard
                        key={s.id}
                        service={s}
                        status={status}
                        onChangeModel={(model) => setVlmService(s.id, { model }, true)}
                        onToggleEnabled={() => toggleVlmServiceEnabled(s.id)}
                        onRemove={() => removeVlmService(s.id)}
                      />
                    );
                  })}
                  {getVlmServices(draft).length === 0 && <p className="muted">No vision providers configured.</p>}
                </div>

                {!showAddVlm ? (
                  <button type="button" className="ghost small" onClick={() => setShowAddVlm(true)}>+ Add server</button>
                ) : (
                  <div className="vlm-add-form">
                    <label className="setting">
                      <span className="label">Type</span>
                      <select
                        aria-label="Provider type"
                        value={newProvider}
                        onChange={(e) => {
                          const p = e.target.value as VlmProvider;
                          setNewProvider(p);
                          setNewModel(VLM_PROVIDERS.find((item) => item.id === p)?.defaultModel || '');
                        }}
                      >
                        {VLM_PROVIDERS.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
                      </select>
                    </label>

                    {newProvider === 'ollama' ? (
                      <>
                        <label className="setting">
                          <span className="label">Host</span>
                          <input type="text" value={newHost} spellCheck={false} onChange={(e) => setNewHost(e.target.value)} />
                        </label>
                        <label className="setting">
                          <span className="label">Model</span>
                          <div className="model-input-row">
                            <select aria-label="Model" value={newModel} onChange={(e) => setNewModel(e.target.value)}>
                              <option value="">Select a model…</option>
                              {ollamaModels.filter((m) => m.vision).map((m) => (
                                <option key={m.name} value={m.name}>{m.name}</option>
                              ))}
                            </select>
                            <button type="button" className="ghost small refresh-models-btn" disabled={ollamaLoading} onClick={() => void fetchOllamaModels(newHost)}>
                              {ollamaLoading ? '…' : '↻'} Refresh
                            </button>
                          </div>
                          {ollamaOnline === false && <span className="status-badge offline">○ Ollama offline at {newHost}</span>}
                        </label>
                      </>
                    ) : (
                      <>
                        <label className="setting">
                          <span className="label">API key</span>
                          <input type="password" value={newApiKey} spellCheck={false} onChange={(e) => setNewApiKey(e.target.value)} />
                        </label>
                        {newProvider === 'anthropic' && (
                          <label className="setting">
                            <span className="label">Key kind</span>
                            <select value={newKeyKind} onChange={(e) => setNewKeyKind(e.target.value)}>
                              {KEY_KINDS.map((k) => <option key={k.id} value={k.id}>{k.name}</option>)}
                            </select>
                          </label>
                        )}
                        <label className="setting">
                          <span className="label">Model</span>
                          <input type="text" value={newModel} spellCheck={false} placeholder={VLM_PROVIDERS.find((p) => p.id === newProvider)?.defaultModel} onChange={(e) => setNewModel(e.target.value)} />
                        </label>
                      </>
                    )}

                    <div className="actions-row">
                      <button type="button" className="primary small" disabled={!newModel.trim()} onClick={addVlmService}>Add</button>
                      <button type="button" className="ghost small" onClick={() => setShowAddVlm(false)}>Cancel</button>
                    </div>
                  </div>
                )}
              </section>

              <section className="setting-group">
                {draft.use_vlm !== undefined && (
                  <label className="setting">
                    <span><span className="label">{labelOf('use_vlm')}</span>{META.use_vlm?.[1] && <span className="hint">{META.use_vlm[1]}</span>}</span>
                    <Control name="use_vlm" value={draft.use_vlm} onChange={(v) => setDraft((d) => ({ ...d, use_vlm: v }))} />
                  </label>
                )}
                <details className="advanced-vlm-settings">
                  <summary>Advanced</summary>
                  {GROUPS[1][1].filter((k) => k !== 'use_vlm' && draft[k] !== undefined).map((k) => (
                    <label className="setting" key={k}>
                      <span><span className="label">{labelOf(k)}</span>{META[k]?.[1] && <span className="hint">{META[k]?.[1]}</span>}</span>
                      <Control
                        name={k}
                        value={draft[k]}
                        onChange={(v) => setDraft((d) => ({ ...d, [k]: v }))}
                      />
                    </label>
                  ))}
                </details>
              </section>
              <div className="actions-row">
                <button className="primary" disabled={!dirty} onClick={save}>Save</button>
                <button className="ghost" disabled={!dirty} onClick={() => setDraft(settings)}>Discard changes</button>
                {dirty && <span className="muted">Unsaved changes</span>}
              </div>

              <section className="setting-group" style={{ marginTop: 24 }}>
                <h3>Benchmark</h3>
                <Benchmark run={run} toast={toast} settings={draft} status={status} />
              </section>
            </div>
          )}

          {/* Tab 3: Metadata & Desktop */}
          {activeTab === 'metadata' && (
            <div className="settings-tab-content">
              <section className="setting-group">
                <h3>Metadata & Desktop</h3>
                {GROUPS[2][1].filter((k) => draft[k] !== undefined).map((k) => (
                  <label className="setting" key={k}>
                    <span><span className="label">{labelOf(k)}</span>{META[k]?.[1] && <span className="hint">{META[k]?.[1]}</span>}</span>
                    <Control
                      name={k}
                      value={draft[k]}
                      onChange={(v) => setDraft((d) => ({ ...d, [k]: v }))}
                    />
                  </label>
                ))}
              </section>
              <div className="actions-row">
                <button className="primary" disabled={!dirty} onClick={save}>Save</button>
                <button className="ghost" disabled={!dirty} onClick={() => setDraft(settings)}>Discard changes</button>
                {dirty && <span className="muted">Unsaved changes</span>}
              </div>
            </div>
          )}

          {/* Tab 4: System & Maintenance */}
          {activeTab === 'maintenance' && (
            <div className="settings-tab-content">
              <Maintenance />
              {dirty && (
                <div className="unsaved-floating-bar">
                  <span>You have unsaved changes in your settings.</span>
                  <div className="actions-row" style={{ marginTop: 0 }}>
                    <button className="primary small" onClick={save}>Save</button>
                    <button className="ghost small" onClick={() => setDraft(settings)}>Discard</button>
                  </div>
                </div>
              )}
            </div>
          )}

          {/* Tab 5: Known Vehicles */}
          {activeTab === 'known' && (
            <div className="settings-tab-content settings-known-tab">
              <div className="known-tab-intro">
                <h3>Known vehicles</h3>
                <p className="muted">Vehicles identified across albums. Used to auto-fill details when the same car appears in a new shoot.</p>
              </div>
              <Known run={run} toast={toast} />
              {dirty && (
                <div className="unsaved-floating-bar">
                  <span>You have unsaved changes in your settings.</span>
                  <div className="actions-row" style={{ marginTop: 0 }}>
                    <button className="primary small" onClick={save}>Save</button>
                    <button className="ghost small" onClick={() => setDraft(settings)}>Discard</button>
                  </div>
                </div>
              )}
            </div>
          )}
        </div>

        {/* Right Stats & Info Sidebar */}
        <aside className="settings-sidebar">
          {/* Software & Updates Card */}
          <div className="sidebar-card update-card">
            <div className="sidebar-card-header">
              <div className="sidebar-brand">
                <span className="sidebar-brand-name">Conrod</span>
                {(updateInfo?.current ?? appVersion) && (
                  <span className="sidebar-brand-version">v{updateInfo?.current ?? appVersion}</span>
                )}
              </div>
              {checkingUpdate ? (
                <span className="pill-badge checking">Checking…</span>
              ) : updateInfo?.newer ? (
                <span className="pill-badge update-ready">Update available</span>
              ) : updateInfo?.ok ? (
                <span className="pill-badge up-to-date">Up to date</span>
              ) : updateInfo?.error ? (
                <span className="pill-badge error">Check failed</span>
              ) : null}
            </div>

            <div className="sidebar-update-body">
              {updateInfo?.newer ? (
                <div className="sidebar-update-alert">
                  <div className="sidebar-update-lead">
                    <strong>Version v{updateInfo.latest} is ready</strong>
                    {updateInfo.size ? <span className="muted small"> · {(updateInfo.size / (1024 * 1024)).toFixed(1)} MB</span> : null}
                  </div>
                  {updateInfo.notes && (
                    <details className="sidebar-update-notes">
                      <summary>Release highlights</summary>
                      <div className="notes-text">{updateInfo.notes}</div>
                    </details>
                  )}
                  {updateInfo.installable ? (
                    <button
                      type="button"
                      className="primary small full-width"
                      disabled={installingUpdate}
                      onClick={handleInstallUpdate}
                    >
                      {installingUpdate ? 'Installing update…' : 'Install update & restart'}
                    </button>
                  ) : (
                    <button
                      type="button"
                      className="button primary small full-width"
                      onClick={() => void openUrl(`https://github.com/kapsikkum/conrod/releases/tag/${updateInfo.tag ?? `v${updateInfo.latest}`}`)}
                    >
                      Download from GitHub ↗
                    </button>
                  )}
                </div>
              ) : (
                <p className="sidebar-update-desc">
                  Checks GitHub for the latest release. Updates are downloaded and installed automatically.
                </p>
              )}

              <div className="sidebar-btn-row">
                <button
                  type="button"
                  className="secondary small"
                  disabled={checkingUpdate || installingUpdate}
                  onClick={() => void checkForUpdates(true)}
                >
                  {checkingUpdate ? 'Checking…' : 'Check for updates'}
                </button>
                <button
                  type="button"
                  className="sidebar-ext-link"
                  onClick={() => void openUrl('https://github.com/kapsikkum/conrod/releases')}
                >
                  Releases ↗
                </button>
              </div>
            </div>
          </div>

          {/* Library Overview Card */}
          <div className="sidebar-card">
            <h4>Library Overview</h4>
            <div className="sidebar-stat-grid">
              <div className="sidebar-stat-item">
                <span className="stat-label">Albums</span>
                <span className="stat-value">{jobs.length}</span>
              </div>
              <div className="sidebar-stat-item">
                <span className="stat-label">Photos</span>
                <span className="stat-value">{totalPhotos.toLocaleString()}</span>
              </div>
              <div className="sidebar-stat-item">
                <span className="stat-label">Known Cars</span>
                <span className="stat-value">{knownCount !== null ? knownCount.toLocaleString() : '—'}</span>
              </div>
            </div>
            {activeTab !== 'known' && (
              <button
                type="button"
                className="ghost small sidebar-card-action"
                onClick={() => setActiveTab('known')}
              >
                View Known Vehicles Catalog →
              </button>
            )}
          </div>

          {/* Vision & AI Pipeline Card */}
          <div className="sidebar-card">
            <h4>Vision & AI Pipeline</h4>
            <div className="sidebar-meta-list">
              <div className="sidebar-meta-row">
                <span className="meta-key">Providers</span>
                <span className="meta-val">
                  {getVlmServices(draft).filter((s) => s.enabled).length} / {getVlmServices(draft).length} enabled
                </span>
              </div>
              <div className="sidebar-meta-row">
                <span className="meta-key">Strategy</span>
                <span className="meta-val">
                  {VLM_STRATEGIES.find((s) => s.id === draft.vlm_strategy)?.name ?? 'Least busy'}
                </span>
              </div>
            </div>
          </div>

          {/* Local ML Models Card */}
          {models.length > 0 && (
            <div className="sidebar-card">
              <h4>Local ML Models</h4>
              <div className="sidebar-meta-list">
                {models.map((m) => (
                  <div className="sidebar-meta-row" key={m.name}>
                    <span className="meta-key truncate" title={m.name}>
                      <span className={`dot ${m.ready ? 'ok' : 'no'}`} />
                      {m.name}
                    </span>
                    <span className="meta-val small">{m.ready ? 'Ready' : 'Unavailable'}</span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}
