import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { KeyboardEvent as ReactKeyboardEvent } from 'react';
import { getVersion } from '@tauri-apps/api/app';
import { openUrl } from '@tauri-apps/plugin-opener';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { engine, inTauri } from '../lib/api';
import { plural } from '../lib/format';
import {
  AI_MODELS, FIELDS, SECTIONS, TABS, aiModelStatus, aiModelStatusLabel, aiModelWhere,
  albumOverrideCounts, decisionOpenCodeKey, dirtyKeys, dirtyTabs, searchFields, tabById,
  type SettingsTabId,
} from '../lib/settings.mjs';
import { SettingsSection, SettingRow } from '../components/SettingsFields';
import { Maintenance } from '../components/Maintenance';
import { DeciderCard } from '../components/DeciderCard';
import { LightroomBridge } from '../components/LightroomBridge';
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

/** Every OpenCode request is billed from the Console credit balance, never the Go coding subscription. */
const OPENCODE_CREDITS_HINT =
  'OpenCode is charged per use from your OpenCode Console credit balance. Do not paste an OpenCode Go subscription key: Go is sold for coding assistants, not for Conrod. Add credits in the Console and use that key for vision and JEV.';

const VLM_PROVIDERS: { id: VlmProvider; name: string; defaultModel: string }[] = [
  { id: 'ollama', name: 'Ollama (local or self-hosted)', defaultModel: 'qwen3-vl:8b-instruct-q4_K_M' },
  { id: 'openrouter', name: 'OpenRouter', defaultModel: 'qwen/qwen2.5-vl-72b-instruct' },
  { id: 'openai', name: 'OpenAI (GPT-4o and newer)', defaultModel: 'gpt-4o' },
  { id: 'anthropic', name: 'Anthropic (Claude 3.5 / 3.7)', defaultModel: 'claude-3-7-sonnet-20250219' },
  { id: 'gemini', name: 'Google Gemini', defaultModel: 'gemini-1.5-flash' },
  { id: 'opencode', name: 'OpenCode (pay-as-you-go credits)', defaultModel: 'glm-5.3-flash' },
];

/** Names for provider ids that older settings can still contain. */
const LEGACY_PROVIDER_NAMES: Partial<Record<VlmProvider, string>> = {
  'opencode-go': 'OpenCode (pay-as-you-go credits)',
};

/** The provider's shown name, including ids that are no longer offered. */
function providerName(provider: VlmProvider): string {
  return VLM_PROVIDERS.find((p) => p.id === provider)?.name ?? LEGACY_PROVIDER_NAMES[provider] ?? provider;
}

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
  if (service.provider !== 'ollama') return providerName(service.provider);
  const host = (service.host || '').replace(/^https?:\/\//, '');
  return `Ollama · ${host}`;
}

/** The cloud providers whose key can list its models. The others only take a typed name. */
const LISTABLE_PROVIDERS: VlmProvider[] = ['openai', 'openrouter', 'opencode', 'opencode-go'];
/** Sentinel for the "Other…" choice in a model select; never a real model id. */
const CUSTOM_MODEL = '__other__';

/** An error line safe for the DOM: the API key can never be echoed back into it. */
function safeError(error: unknown, apiKey: string): string {
  const text = error instanceof Error ? error.message : String(error);
  const key = apiKey.trim();
  return key ? text.split(key).join('***') : text;
}

type CloudModelControlProps = {
  provider: VlmProvider;
  model: string;
  title: string;
  host?: string;
  apiKey?: string;
  /** Fetch as soon as the control appears (a saved card), not only on refresh. */
  autoLoad?: boolean;
  /** Commit every keystroke; the add form's Add button needs a live value. */
  live?: boolean;
  onChange: (model: string) => void;
};

/**
 * The model field for a cloud provider. OpenAI and OpenCode can list their models, so they
 * get a native picker with "Other…" for an id the list does not know. Anthropic and Gemini
 * have no list endpoint here, so they keep a clearly labelled text field.
 */
function CloudModelControl({ provider, model, title, host, apiKey, autoLoad = false, live = false, onChange }: CloudModelControlProps) {
  const listable = LISTABLE_PROVIDERS.includes(provider);
  const [models, setModels] = useState<{ id: string; name: string }[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [custom, setCustom] = useState(false);
  const [draft, setDraft] = useState(model);
  const customRef = useRef<HTMLInputElement>(null);

  useEffect(() => { setDraft(model); }, [model]);
  // The add form reuses this control when the provider changes; a stale list would lie.
  useEffect(() => { setModels([]); setError(null); setCustom(false); }, [provider]);
  useEffect(() => { if (custom) customRef.current?.focus(); }, [custom]);

  const load = useCallback(() => {
    const key = (apiKey ?? '').trim();
    if (!listable || !key) return;
    setLoading(true);
    setError(null);
    void engine.providerModels(provider, host, key)
      .then((res) => {
        setModels(res.models);
        if (res.error) setError(safeError(res.error, key));
        else if (!res.models.length) setError('The provider returned no models; type the model name.');
      })
      .catch((e) => { setModels([]); setError(safeError(e, key)); })
      .finally(() => setLoading(false));
  }, [provider, host, apiKey, listable]);

  useEffect(() => { if (autoLoad) load(); }, [autoLoad, load]);

  const commit = () => {
    const next = draft.trim();
    if (next && next !== model) onChange(next);
  };

  const pick = (next: string) => {
    if (next === CUSTOM_MODEL) {
      setDraft(model);
      setCustom(true);
      return;
    }
    setCustom(false);
    if (next) onChange(next);
  };

  const textField = (
    <input
      ref={customRef}
      type="text"
      aria-label={custom ? `Custom model for ${title}` : `Model for ${title}`}
      value={draft}
      spellCheck={false}
      placeholder="Type the model name"
      onChange={(e) => { setDraft(e.target.value); if (live) onChange(e.target.value); }}
      onBlur={commit}
      onKeyDown={(e) => { if (e.key === 'Enter') (e.target as HTMLInputElement).blur(); }}
    />
  );
  const note = loading ? 'Loading models…' : error;

  return (
    <div className="cloud-model">
      {listable ? (
        <>
          <div className="model-input-row">
            <select aria-label={`Model for ${title}`} value={custom ? CUSTOM_MODEL : model} onChange={(e) => pick(e.target.value)}>
              {!model && !custom && <option value="">Select a model…</option>}
              {model && !custom && !models.some((m) => m.id === model) && <option value={model}>{model}</option>}
              {models.map((m) => <option key={m.id} value={m.id}>{m.name}</option>)}
              <option value={CUSTOM_MODEL}>Other…</option>
            </select>
            <button
              type="button"
              className="ghost small refresh-models-btn"
              disabled={loading || !(apiKey ?? '').trim()}
              onClick={load}
              aria-label={`Refresh models for ${title}`}
              title="Refresh models"
            >
              {loading ? '…' : '↻'}
            </button>
          </div>
          {custom && textField}
        </>
      ) : (
        <>
          {textField}
          <span className="muted vlm-server-detail">Type the model name; this provider does not list models.</span>
        </>
      )}
      {note && <span className="muted vlm-server-detail">{note}</span>}
    </div>
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
        <CloudModelControl
          provider={service.provider}
          model={service.model}
          title={title}
          host={service.host}
          apiKey={service.api_key}
          autoLoad
          onChange={onChangeModel}
        />
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

type VlmAddFormProps = {
  onAdd: (service: VlmService) => void;
  /** The decision model's OpenRouter key, offered for reuse here. */
  sharedKey?: string;
};

/** The add-a-provider form; local state so typing here never disturbs the saved settings. */
function VlmAddForm({ onAdd, sharedKey = '' }: VlmAddFormProps) {
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
      ...(provider === 'opencode' && host.trim() ? { host: host.trim() } : {}),
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
            setHost(next === 'ollama' ? DEFAULT_OLLAMA_HOST : '');
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
          {provider === 'openrouter' && sharedKey.trim() && apiKey !== sharedKey && (
            <button type="button" className="ghost small" onClick={() => setApiKey(sharedKey)}>
              Use the decision model's OpenRouter key
            </button>
          )}
          {provider === 'anthropic' && (
            <label className="setting">
              <span className="label">Key kind</span>
              <select value={keyKind} onChange={(e) => setKeyKind(e.target.value)}>
                {KEY_KINDS.map((kind) => <option key={kind.id} value={kind.id}>{kind.name}</option>)}
              </select>
            </label>
          )}
          {provider === 'opencode' && (
            <>
              <label className="setting">
                <span className="label">Endpoint (optional)</span>
                <input type="text" value={host} spellCheck={false} placeholder="https://opencode.ai/inference/openai/v1" onChange={(e) => setHost(e.target.value)} />
              </label>
              <p className="section-hint" role="note">{OPENCODE_CREDITS_HINT}</p>
            </>
          )}
          <label className="setting">
            <span className="label">Model</span>
            <CloudModelControl
              provider={provider}
              model={model}
              title="the new server"
              host={host}
              apiKey={apiKey}
              live
              onChange={setModel}
            />
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

type ModelSwitchProps = {
  fieldKey: string;
  value: SettingValue;
  onChange: (value: SettingValue) => void;
};

/** A compact on/off control inside an AI model row; the label is the field's own. */
function ModelSwitch({ fieldKey, value, onChange }: ModelSwitchProps) {
  const field = FIELDS[fieldKey];
  if (!field) return null;
  return (
    <label className="ai-model-switch">
      <input
        type="checkbox"
        role="switch"
        checked={Boolean(value)}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span>{field.label}</span>
    </label>
  );
}

type AiModelsCardProps = {
  draft: SettingsMap;
  saved: SettingsMap;
  models: ModelInfo[];
  services: VlmService[];
  onChange: (key: string, value: SettingValue) => void;
  setServices: (next: VlmService[]) => void;
  toast: Toaster;
  onModelsChanged?: () => void;
};

/**
 * The one AI models card: every model Conrod can use, what it does, where it
 * runs, whether it is ready, and its switch. The vision servers and the
 * decision model's transport live here rather than in their own sections.
 */
function AiModelsCard({ draft, saved, models, services, onChange, setServices, toast, onModelsChanged }: AiModelsCardProps) {
  const [testing, setTesting] = useState(false);
  const [testingOpenCode, setTestingOpenCode] = useState(false);
  const openRouterKey = String(draft.moments_openrouter_key ?? '');
  const openCodeKey = decisionOpenCodeKey(draft);
  // JEV is offered once there is an OpenCode key to call it with; a saved
  // choice stays visible even after the key is removed, so it can be changed.
  const showOpenCode = Boolean(openCodeKey.trim()) || String(draft.moments_transport ?? '') === 'opencode';

  const testOpenRouter = async () => {
    if (testing) return;
    setTesting(true);
    try {
      const res = await engine.openRouterCheck(openRouterKey);
      if (res.ok) toast(`OpenRouter key works${res.label ? ` (${res.label})` : ''}.`, { tone: 'ok' });
      else toast(`OpenRouter: ${res.error ?? 'the key was refused'}`, { tone: 'error' });
    } catch (e) {
      toast(String(e), { tone: 'error' });
    } finally {
      setTesting(false);
    }
  };

  const testOpenCode = async () => {
    if (testingOpenCode) return;
    setTestingOpenCode(true);
    try {
      const res = await engine.openCodeCheck(openCodeKey);
      if (res.ok) toast(`OpenCode key works${res.label ? ` (${res.label})` : ''}.`, { tone: 'ok' });
      else toast(`OpenCode: ${res.error ?? 'the key was refused'}`, { tone: 'error' });
    } catch (e) {
      toast(String(e), { tone: 'error' });
    } finally {
      setTestingOpenCode(false);
    }
  };

  return (
    <section className="settings-section ai-models" aria-labelledby="settings-section-ai-models">
      <header className="settings-section-head">
        <h3 id="settings-section-ai-models">AI models</h3>
        <p className="section-hint">Every model Conrod can use, what it does, where it runs and whether it is ready.</p>
      </header>

      <div className="ai-model-list">
        {AI_MODELS.map((model) => {
          const state = aiModelStatus(model, { settings: draft, models });
          return (
            <div className={`ai-model-row is-${state.replace(' ', '-')}`} key={model.id} data-model={model.id}>
              <div className="ai-model-copy">
                <strong>{model.title}</strong>
                <p>{model.does}</p>
                <p className="muted small">
                  Runs on {aiModelWhere(model, draft)} ·{' '}
                  <span className={`ai-model-status is-${state.replace(' ', '-')}`}>
                    <span className={`dot ${state === 'ready' ? 'ok' : state === 'off' ? '' : 'no'}`} aria-hidden="true" />
                    {' '}{aiModelStatusLabel(state)}
                  </span>
                </p>
              </div>
              <div className="ai-model-controls">
                {model.switches.map((key) => (
                  <ModelSwitch
                    key={key}
                    fieldKey={key}
                    value={draft[key]}
                    onChange={(value) => onChange(key, value)}
                  />
                ))}
              </div>
            </div>
          );
        })}
      </div>

      <div className="ai-model-detail">
        <h4>Vision servers</h4>
        <p className="section-hint">One or more servers that read make, model, colour and team. Work is spread across the ones that are on.</p>
        <p className="section-hint" role="note">{OPENCODE_CREDITS_HINT}</p>
        <SettingRow
          field={FIELDS.vlm_strategy}
          value={draft.vlm_strategy ?? 'least_busy'}
          changed={draft.vlm_strategy !== saved.vlm_strategy}
          onChange={(value) => onChange('vlm_strategy', value)}
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
          {services.length === 0 && <p className="vlm-empty muted">No vision servers yet. Add a local Ollama server or a cloud key below.</p>}
        </div>
        <VlmAddForm sharedKey={openRouterKey} onAdd={(service) => setServices([...services, service])} />
      </div>

      <div className="ai-model-detail">
        <h4>Decision model</h4>
        <DeciderCard toast={toast} onChanged={onModelsChanged} />
        <SettingRow
          field={FIELDS.decider_quant}
          value={draft.decider_quant ?? 'auto'}
          changed={draft.decider_quant !== saved.decider_quant}
          onChange={(value) => onChange('decider_quant', value)}
        />
        <SettingRow
          field={showOpenCode ? FIELDS.moments_transport : {
            ...FIELDS.moments_transport,
            options: (FIELDS.moments_transport.options ?? []).filter((option) => option.id !== 'opencode'),
          }}
          value={draft.moments_transport ?? 'auto'}
          changed={draft.moments_transport !== saved.moments_transport}
          onChange={(value) => onChange('moments_transport', value)}
        />
        <p className="section-hint">
          {String(draft.moments_transport ?? 'auto') === 'openrouter'
            ? 'Questions go to OpenRouter. Add a key below; the vision model can share it.'
            : String(draft.moments_transport ?? 'auto') === 'opencode'
              ? 'Questions go to OpenCode’s decision model JEV. Add an OpenCode key below, or switch on an OpenCode vision service to share its key. JEV is charged per use to your Console credits ($0.042 per million input tokens; output free).'
              : "Local runs on this PC's graphics card. OpenCode (JEV) and OpenRouter send the questions to the cloud."}
        </p>
        <SettingRow
          field={FIELDS.moments_opencode_model}
          value={draft.moments_opencode_model ?? 'jev-1.13'}
          changed={draft.moments_opencode_model !== saved.moments_opencode_model}
          onChange={(value) => onChange('moments_opencode_model', value)}
        />
        <SettingRow
          field={FIELDS.moments_opencode_key}
          value={draft.moments_opencode_key ?? ''}
          changed={draft.moments_opencode_key !== saved.moments_opencode_key}
          onChange={(value) => onChange('moments_opencode_key', value)}
        />
        <p className="section-hint" role="note">{OPENCODE_CREDITS_HINT}</p>
        <div className="actions-row">
          <button type="button" className="ghost small" disabled={testingOpenCode || !openCodeKey.trim()} onClick={() => void testOpenCode()}>
            {testingOpenCode ? 'Testing…' : 'Test OpenCode key'}
          </button>
          <span className="muted small">Checks the key against OpenCode’s model list and says whether JEV is on it, without spending a model call.</span>
        </div>
        <SettingRow
          field={FIELDS.moments_openrouter_model}
          value={draft.moments_openrouter_model ?? ''}
          changed={draft.moments_openrouter_model !== saved.moments_openrouter_model}
          onChange={(value) => onChange('moments_openrouter_model', value)}
        />
        <SettingRow
          field={FIELDS.moments_openrouter_key}
          value={draft.moments_openrouter_key ?? ''}
          changed={draft.moments_openrouter_key !== saved.moments_openrouter_key}
          onChange={(value) => onChange('moments_openrouter_key', value)}
        />
        <div className="actions-row">
          <button type="button" className="ghost small" disabled={testing || !openRouterKey.trim()} onClick={() => void testOpenRouter()}>
            {testing ? 'Testing…' : 'Test OpenRouter key'}
          </button>
          <span className="muted small">Checks the key with OpenRouter without spending a model call.</span>
        </div>
      </div>
    </section>
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
            <button type="button" className="sidebar-ext-link" onClick={() => void openUrl('https://github.com/kapsikkum/conrod/releases')}>
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
};

/** Settings shared with the Python library; Save writes the whole block back. */
export function Settings({ settings, onSaved, run, toast, jobs = [], models = [], onModelsChanged, status, onOpenWhatsNew, onOpenWizard, openTab }: Props) {
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

  const services = getVlmServices(draft);
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
                      <AiModelsCard
                        key={section.id}
                        draft={draft}
                        saved={settings}
                        models={models}
                        services={services}
                        onChange={setValue}
                        setServices={setServices}
                        toast={toast}
                        onModelsChanged={onModelsChanged}
                      />
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
