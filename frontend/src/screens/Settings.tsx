import { Maintenance } from '../components/Maintenance';
import { Known } from './Known';
import { useEffect, useState, useCallback } from 'react';
import { engine } from '../lib/api';
import { words } from '../lib/format';
import type {
  Settings as SettingsMap,
  SettingValue,
  Toaster,
  UpdateInfo,
  OllamaModel,
  Job,
  ModelInfo,
} from '../lib/types';
import type { Runner } from '../state/hooks';

const GROUPS: [string, string[]][] = [
  ['Detection & focus', ['detect_conf', 'min_box_fraction', 'max_vehicles_per_frame', 'include_cars', 'include_bikes', 'include_trucks', 'sharp_at', 'blurred_below', 'auto_reject_below_stars', 'cull_blurred', 'burst_gap']],
  ['Identification & AI', ['read_plates', 'read_numbers', 'read_text', 'use_vlm', 'vlm_provider', 'vlm_model', 'vlm_host', 'vlm_extra_hosts', 'vlm_api_key', 'vlm_timeout', 'use_known_vehicles', 'normalise_names']],
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
  vlm_provider: ['Vision provider', 'Ollama (local), or OpenAI, Anthropic, Gemini'],
  vlm_model: ['Vision model', 'e.g. qwen2.5vl:7b, gpt-4o, claude-3-7-sonnet'],
  vlm_host: ['Vision host', 'For Ollama or local server (default http://127.0.0.1:11434)'],
  vlm_extra_hosts: ['Extra hosts'],
  vlm_api_key: ['API key', 'Required for cloud providers (OpenAI, Anthropic, Gemini). Stored locally.'],
  vlm_timeout: ['Timeout (seconds)'],
  use_known_vehicles: ['Use known vehicles', 'Fill blanks from cars you have met.'],
  normalise_names: ['Tidy makes and models'],
  write_sidecar_for_raw: ['Write sidecars next to RAW files'],
  keyword_prefix: ['Keyword prefix'],
  close_to_tray: ['Keep running in the tray when closed'],
};
const labelOf = (key: string) => META[key]?.[0] ?? words(key).replace(/^./, (c) => c.toUpperCase());

const VLM_PROVIDERS = [
  { id: 'ollama', name: 'Ollama (Local / Self-hosted)', defaultModel: 'qwen2.5vl:7b' },
  { id: 'openai', name: 'OpenAI (GPT-4o, etc.)', defaultModel: 'gpt-4o' },
  { id: 'anthropic', name: 'Anthropic (Claude 3.5 / 3.7)', defaultModel: 'claude-3-7-sonnet-20250219' },
  { id: 'gemini', name: 'Google Gemini', defaultModel: 'gemini-1.5-flash' },
];

type Props = {
  settings: SettingsMap;
  onSaved: (s: SettingsMap) => void;
  run: Runner;
  toast: Toaster;
  jobs?: Job[];
  models?: ModelInfo[];
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
export function Settings({ settings, onSaved, run, toast, jobs = [], models = [] }: Props) {
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

  // Ollama Models state
  const [ollamaModels, setOllamaModels] = useState<OllamaModel[]>([]);
  const [ollamaOnline, setOllamaOnline] = useState<boolean | null>(null);
  const [ollamaLoading, setOllamaLoading] = useState(false);
  const [customModelInput, setCustomModelInput] = useState(false);

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

  useEffect(() => {
    if (draft.vlm_provider === 'ollama') {
      void fetchOllamaModels(String(draft.vlm_host || ''));
    }
  }, [draft.vlm_provider, draft.vlm_host, fetchOllamaModels]);

  const save = () => void run(async () => {
    onSaved(await engine.saveSettings(draft));
    toast('Settings saved.', { tone: 'ok' });
  });

  const learn = () => void run(async () => {
    const out = await engine.trainTaste();
    toast(typeof out.n === 'number' ? `Learned from ${out.n} of your ratings.` : 'Learned from your ratings.', { tone: 'ok' });
  });

  function Control({
    name,
    value,
    onChange,
    onProviderChange,
  }: {
    name: string;
    value: SettingValue;
    onChange: (v: SettingValue) => void;
    onProviderChange?: (provider: string, defaultModel: string) => void;
  }) {
    if (name === 'vlm_provider') {
      return (
        <select
          aria-label={labelOf(name)}
          value={String(value).toLowerCase()}
          onChange={(e) => {
            const p = e.target.value;
            onChange(p);
            const found = VLM_PROVIDERS.find((item) => item.id === p);
            if (found && onProviderChange) {
              onProviderChange(p, found.defaultModel);
            }
          }}
        >
          {VLM_PROVIDERS.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      );
    }

    if (name === 'vlm_model' && draft.vlm_provider === 'ollama') {
      const currentModel = String(value);
      const isKnown = ollamaModels.some((m) => m.name === currentModel);
      const isCustom = customModelInput || (!isKnown && currentModel !== '' && ollamaModels.length > 0);
      const visionModels = ollamaModels.filter((m) => m.vision);
      const otherModels = ollamaModels.filter((m) => !m.vision);

      return (
        <div className="model-selector-wrap">
          <div className="model-input-row">
            {!isCustom && ollamaModels.length > 0 ? (
              <select
                aria-label={labelOf(name)}
                value={currentModel}
                onChange={(e) => {
                  if (e.target.value === '__custom__') {
                    setCustomModelInput(true);
                  } else {
                    onChange(e.target.value);
                  }
                }}
              >
                {visionModels.length > 0 && (
                  <optgroup label="Vision Models (Recommended)">
                    {visionModels.map((m) => (
                      <option key={m.name} value={m.name}>
                        {m.name} {m.size ? `· ${(m.size / (1024 * 1024 * 1024)).toFixed(1)} GB` : ''}
                      </option>
                    ))}
                  </optgroup>
                )}
                {otherModels.length > 0 && (
                  <optgroup label="Other Local Models">
                    {otherModels.map((m) => (
                      <option key={m.name} value={m.name}>
                        {m.name}
                      </option>
                    ))}
                  </optgroup>
                )}
                <option value="__custom__">Custom model name…</option>
              </select>
            ) : (
              <div className="custom-model-input">
                <input
                  type="text"
                  aria-label={labelOf(name)}
                  value={value as string}
                  placeholder="e.g. qwen2.5vl:7b"
                  spellCheck={false}
                  onChange={(e) => onChange(e.target.value)}
                />
                {ollamaModels.length > 0 && (
                  <button
                    type="button"
                    className="ghost small"
                    onClick={() => {
                      setCustomModelInput(false);
                      if (visionModels[0]) onChange(visionModels[0].name);
                    }}
                  >
                    List
                  </button>
                )}
              </div>
            )}
            <button
              type="button"
              className="ghost small refresh-models-btn"
              title="Refresh models from Ollama"
              disabled={ollamaLoading}
              onClick={() => void fetchOllamaModels(String(draft.vlm_host || ''))}
            >
              {ollamaLoading ? '…' : '↻'}
            </button>
          </div>
          <div className="ollama-status-line">
            {ollamaOnline === true ? (
              <span className="status-badge online">
                ● Ollama connected ({ollamaModels.length} models, {visionModels.length} vision)
              </span>
            ) : ollamaOnline === false ? (
              <span className="status-badge offline">
                ○ Ollama offline at {String(draft.vlm_host || 'http://127.0.0.1:11434')}
              </span>
            ) : null}
          </div>
        </div>
      );
    }

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
                <h3>Vision & AI</h3>
                {GROUPS[1][1].filter((k) => draft[k] !== undefined).map((k) => (
                  <label className="setting" key={k}>
                    <span><span className="label">{labelOf(k)}</span>{META[k]?.[1] && <span className="hint">{META[k]?.[1]}</span>}</span>
                    <Control
                      name={k}
                      value={draft[k]}
                      onChange={(v) => setDraft((d) => ({ ...d, [k]: v }))}
                      onProviderChange={(provider, defaultModel) => {
                        setDraft((d) => ({ ...d, vlm_provider: provider, vlm_model: defaultModel }));
                      }}
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
                <span className="sidebar-brand-version">v{updateInfo?.current ?? '1.0.0-beta.5'}</span>
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
                      <pre>{updateInfo.notes}</pre>
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
                    <a
                      className="button primary small full-width"
                      href={`https://github.com/kapsikkum/conrod/releases/tag/${updateInfo.tag ?? `v${updateInfo.latest}`}`}
                      target="_blank"
                      rel="noreferrer"
                    >
                      Download from GitHub ↗
                    </a>
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
                <a
                  href="https://github.com/kapsikkum/conrod/releases"
                  target="_blank"
                  rel="noreferrer"
                  className="sidebar-ext-link"
                >
                  Releases ↗
                </a>
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
                <span className="meta-key">Provider</span>
                <span className="meta-val">
                  {VLM_PROVIDERS.find((p) => p.id === draft.vlm_provider)?.name.split(' ')[0] ?? String(draft.vlm_provider || 'Ollama')}
                </span>
              </div>
              <div className="sidebar-meta-row">
                <span className="meta-key">Active Model</span>
                <span className="meta-val mono small truncate" title={String(draft.vlm_model || 'None')}>
                  {String(draft.vlm_model || 'None')}
                </span>
              </div>
              <div className="sidebar-meta-row">
                <span className="meta-key">Status</span>
                <span className="meta-val">
                  {draft.vlm_provider === 'ollama' ? (
                    ollamaOnline === true ? (
                      <span className="status-badge online">● Online ({ollamaModels.length} models)</span>
                    ) : ollamaOnline === false ? (
                      <span className="status-badge offline">○ Offline</span>
                    ) : (
                      <span className="muted small">Checking…</span>
                    )
                  ) : draft.vlm_api_key ? (
                    <span className="status-badge online">● Key set</span>
                  ) : (
                    <span className="status-badge offline">○ No key</span>
                  )}
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
                  <div className="sidebar-meta-row" key={m.file}>
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

          {/* System & Environment Card */}
          <div className="sidebar-card">
            <h4>System & Runtime</h4>
            <div className="sidebar-meta-list">
              <div className="sidebar-meta-row">
                <span className="meta-key">Platform</span>
                <span className="meta-val">Windows x64</span>
              </div>
              <div className="sidebar-meta-row">
                <span className="meta-key">Architecture</span>
                <span className="meta-val">Native Rust + Tauri 2</span>
              </div>
              <div className="sidebar-meta-row">
                <span className="meta-key">Database</span>
                <span className="meta-val">SQLite (WAL mode)</span>
              </div>
            </div>
          </div>
        </aside>
      </div>
    </div>
  );
}
