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

type Props = { settings: SettingsMap; onSaved: (s: SettingsMap) => void; run: Runner; toast: Toaster };

/** Settings shared with the Python library; Save writes the whole block back. */
export function Settings({ settings, onSaved, run, toast }: Props) {
  const [draft, setDraft] = useState<SettingsMap>(settings);
  useEffect(() => setDraft(settings), [settings]);
  const dirty = JSON.stringify(draft) !== JSON.stringify(settings);

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

  return (
    <div className="screen">
      <div className="pane wide">
        <h2>Settings</h2>
        <p className="lede">Shared with your existing Conrod library. Defaults are what measured best on real frames.</p>

        {/* Dedicated App Updates Hero Card */}
        <div className="update-hero-card">
          <div className="update-hero-main">
            <div className="update-brand">
              <span className="app-name">Conrod</span>
              <span className="app-version">v{updateInfo?.current ?? '1.0.0-beta.4'}</span>
              {updateInfo?.newer ? (
                <span className="pill-badge update-ready">Update Available: v{updateInfo.latest}</span>
              ) : updateInfo?.ok ? (
                <span className="pill-badge up-to-date">Up to date</span>
              ) : null}
            </div>
            <div className="update-controls">
              <button
                type="button"
                className="secondary"
                disabled={checkingUpdate || installingUpdate}
                onClick={() => void checkForUpdates(true)}
              >
                {checkingUpdate ? 'Checking…' : 'Check for updates'}
              </button>
            </div>
          </div>

          {updateInfo?.newer && (
            <div className="update-banner">
              <div className="update-banner-info">
                <strong>New Version {updateInfo.latest} is ready to install</strong>
                {updateInfo.size ? (
                  <span className="muted small"> · {(updateInfo.size / (1024 * 1024)).toFixed(1)} MB</span>
                ) : null}
                {updateInfo.notes && (
                  <div className="update-notes-box">
                    <pre className="update-notes-content">{updateInfo.notes}</pre>
                  </div>
                )}
              </div>
              <div className="update-banner-actions">
                {updateInfo.installable ? (
                  <button
                    type="button"
                    className="primary"
                    disabled={installingUpdate}
                    onClick={handleInstallUpdate}
                  >
                    {installingUpdate ? 'Installing…' : 'Update and restart'}
                  </button>
                ) : (
                  <a
                    className="button primary"
                    href={`https://github.com/kapsikkum/conrod/releases/tag/${updateInfo.tag ?? `v${updateInfo.latest}`}`}
                    target="_blank"
                    rel="noreferrer"
                  >
                    Download v{updateInfo.latest} from GitHub ↗
                  </a>
                )}
              </div>
            </div>
          )}

          {updateInfo?.error && (
            <div className="update-error-note">
              <span>{updateInfo.error}</span>
              <a
                href="https://github.com/kapsikkum/conrod/releases"
                target="_blank"
                rel="noreferrer"
                className="small"
              >
                GitHub Releases ↗
              </a>
            </div>
          )}
        </div>

        {GROUPS.map(([title, keys]) => (
          <section className="setting-group" key={title}>
            <h3>{title}</h3>
            {keys.filter((k) => draft[k] !== undefined).map((k) => {
              const value = draft[k];
              return (
                <label className="setting" key={k}>
                  <span><span className="label">{labelOf(k)}</span>{META[k]?.[1] && <span className="hint">{META[k]?.[1]}</span>}</span>
                  <Control
                    name={k}
                    value={value}
                    onChange={(v) => setDraft((d) => ({ ...d, [k]: v }))}
                    onProviderChange={(provider, defaultModel) => {
                      setDraft((d) => ({ ...d, vlm_provider: provider, vlm_model: defaultModel }));
                    }}
                  />
                </label>
              );
            })}
          </section>
        ))}

        <Maintenance />

        <div className="actions-row">
          <button className="primary" disabled={!dirty} onClick={save}>Save</button>
          <button className="ghost" disabled={!dirty} onClick={() => setDraft(settings)}>Discard changes</button>
          <button className="ghost" title="Fit Conrod's star ratings to the ones you have given by hand." onClick={learn}>Learn from my ratings</button>
          {dirty && <span className="muted">Unsaved changes</span>}
        </div>
      </div>

      <section className="settings-section">
        <h3>Known vehicles</h3>
        <p className="muted">Vehicles identified across albums. Used to auto-fill details when the same car appears in a new shoot.</p>
        <Known run={run} toast={toast} />
      </section>
    </div>
  );
}
