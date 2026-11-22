import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { engine, mocked } from '../lib/api';
import { Segmented } from '../components/SettingsFields';
import * as ai from '../lib/aiCard.mjs';
import type { ModelInfo, SettingValue, Settings as SettingsMap, Toaster, VlmService } from '../lib/types';
import '../styles/aiSettings.css';

type Props = {
  draft: SettingsMap;
  saved: SettingsMap;
  models: ModelInfo[];
  onChange: (key: string, value: SettingValue) => void;
  toast: Toaster;
  onModelsChanged?: () => void;
};

type Tone = 'ok' | 'locked' | 'off' | 'ready';
type ModelOption = { id: string; name: string };
type ModelList = { models: ModelOption[]; freeText: boolean };
type ModelLists = { vision: Record<string, ModelList>; decision: Record<string, ModelList> };

const emptyLists = (): ModelLists => ({ vision: {}, decision: {} });
const dotClass = (tone: Tone): string => tone === 'ok' || tone === 'ready' ? 'ok' : tone === 'locked' ? 'no' : 'off';
const maskKey = (key: string) => key ? 'key ••••••••' : '';

/* ---- small inline icons (line style, currentColor) ---- */
const Ico = {
  download: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M12 3v12m0 0 4-4m-4 4-4-4" /><path d="M4 17v3h16v-3" /></svg>,
  cloud: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M7 18h10a4 4 0 0 0 .5-7.97A6 6 0 0 0 6 9.2 4.2 4.2 0 0 0 7 18Z" /></svg>,
  caret: <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="m6 9 6 6 6-6" /></svg>,
  lock: <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><rect x="4.5" y="10.5" width="15" height="10" rx="2" /><path d="M8 10.5V7a4 4 0 0 1 8 0v3.5" /></svg>,
  chevron: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="m6 9 6 6 6-6" /></svg>,
};

function Switch({ checked, disabled, onChange, ariaLabel }: { checked: boolean; disabled?: boolean; onChange: (v: boolean) => void; ariaLabel?: string }) {
  return (
    <label className="ai-switch">
      <input type="checkbox" role="switch" aria-label={ariaLabel} checked={checked} disabled={disabled} onChange={(e) => onChange(e.target.checked)} />
      <span className="track"><span className="knob" /></span>
    </label>
  );
}

/** The "Photo helpers" card: Version C — Two zones. */
export function AiSettingsCard({ draft, onChange, toast, onModelsChanged }: Props) {
  // Dev-mock shortcuts so the screenshot states open directly: &ai=decision
  // expands the Spots moments panel, &ai=services opens AI services.
  const devAi = () => (import.meta.env.DEV && mocked ? new URLSearchParams(location.search).get('ai') : null);
  const [openPanel, setOpenPanel] = useState<string | null>(() => devAi() === 'decision' ? 'moments' : null);
  const [servicesOpen, setServicesOpen] = useState(() => devAi() === 'services');
  const [forkChoice, setForkChoice] = useState<'download' | 'cloud' | null>(null);
  const [forkKeys, setForkKeys] = useState<{ opencode: string; openrouter: string }>({ opencode: '', openrouter: '' });
  const [hostInput, setHostInput] = useState('');
  const [downloadProgress, setDownloadProgress] = useState<number | null>(null);
  const [modelLists, setModelLists] = useState<ModelLists>(emptyLists);
  const [testing, setTesting] = useState<Record<string, 'testing' | 'ok'>>({});
  const changeRefs = useRef<Record<string, HTMLButtonElement | null>>({});

  const config = useMemo(() => ai.serviceConfig(draft), [draft]);
  // A string-keyed view of the four services for loops/ids that TS cannot narrow.
  const cfg = config as unknown as Record<string, { key?: string; hosts?: string[]; enabled?: boolean; downloaded?: boolean }>;
  const view = useMemo(() => ai.buildAiView(draft, modelLists), [draft, modelLists]);
  const ctx = view.ctx;

  /* Enumerate the models each configured service can serve, live from the
     engine: provider_models / ollama_models for vision, decision_models
     (image-filtered) for moments. Missing lists become free-text. */
  const configKey = JSON.stringify({
    local: config.local,
    ollama: config.ollama,
    openrouter: { key: Boolean(config.openrouter.key), enabled: config.openrouter.enabled },
    opencode: { key: Boolean(config.opencode.key), enabled: config.opencode.enabled },
  });
  useEffect(() => {
    let cancelled = false;
    const next = emptyLists();
    if (ai.serving('local', config)) {
      next.vision.local = { models: [{ id: 'built-in', name: 'Built-in car reader' }], freeText: false };
      next.decision.local = { models: [{ id: 'in_process', name: 'Built-in moment helper' }], freeText: false };
    }
    const tasks: Promise<void>[] = [];
    for (const id of ['ollama', 'openrouter', 'opencode'] as const) {
      if (!ai.serving(id, config)) continue;
      const key = cfg[id]?.key ?? '';
      const host = id === 'ollama' ? config.ollama.hosts[0] : '';
      if (id === 'ollama') {
        next.decision.ollama = { models: [], freeText: true };
        tasks.push(engine.ollamaModels(host)
          .then((res) => { next.vision.ollama = { models: (res.models || []).filter((m) => m.vision).map((m) => ({ id: m.name, name: m.name })), freeText: false }; })
          .catch(() => { next.vision.ollama = { models: [], freeText: true }; }));
      } else {
        tasks.push(engine.providerModels(id, host, key)
          .then((res) => { const ms = (res.models || []).map((m) => ({ id: m.id, name: m.name || m.id })); next.vision[id] = { models: ms, freeText: ms.length === 0 }; })
          .catch(() => { next.vision[id] = { models: [], freeText: true }; }));
        if (id === 'opencode') {
          tasks.push(engine.decisionModels(id, host, key, '')
            .then((res) => { const ms = (res.models || []).map((m) => ({ id: m.id, name: m.name || m.id })); next.decision.opencode = { models: ms, freeText: Boolean(res.freeText) && ms.length === 0 }; })
            .catch(() => { next.decision.opencode = { models: [], freeText: true }; }));
        } else {
          next.decision.openrouter = { models: [], freeText: false };
        }
      }
    }
    Promise.all(tasks).then(() => { if (!cancelled) setModelLists(next); });
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [configKey]);

  /* An opened panel scrolls into view, so the bottom-zone helpers (moments)
     are usable without hunting for them. */
  useEffect(() => {
    if (!openPanel) return;
    document.getElementById(`ai-model-panel-${openPanel}`)?.scrollIntoView({ block: 'nearest' });
  }, [openPanel]);

  /* Escape closes the one open panel and returns focus to its Change link. */
  useEffect(() => {
    if (!openPanel) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      const prev = openPanel;
      setOpenPanel(null);
      window.setTimeout(() => changeRefs.current[prev]?.focus(), 0);
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [openPanel]);

  const applyService = useCallback((id: string, patch: Record<string, unknown>) => {
    const next = ai.setService(draft, id, patch);
    onChange('vlm_services', next.vlm_services as unknown as SettingValue);
    onChange('moments_services', next.moments_services as unknown as SettingValue);
    onChange('use_decider', next.use_decider);
  }, [draft, onChange]);

  const selectModel = useCallback((cap: 'vision' | 'decision', id: string, model: string) => {
    const patch = ai.setCapModel(draft, cap, id, model);
    for (const [key, value] of Object.entries(patch)) onChange(key, value as unknown as SettingValue);
  }, [draft, onChange]);

  const modelOf = (cap: 'vision' | 'decision', id: string): string => {
    const list = (draft[cap === 'vision' ? 'vlm_services' : 'moments_services'] as unknown as VlmService[]) || [];
    return list.find((s) => ai.providerGroup(s.provider) === id)?.model || '';
  };

  const testLabel = (id: string) => testing[id] === 'testing' ? 'Testing…' : testing[id] === 'ok' ? '✓ Works' : 'Test';
  const testService = useCallback((id: string) => {
    if (testing[id]) return;
    setTesting((t) => ({ ...t, [id]: 'testing' }));
    const call = id === 'ollama'
      ? engine.ollamaModels(config.ollama.hosts[0])
      : id === 'openrouter'
        ? engine.openRouterCheck(config.openrouter.key)
        : id === 'opencode'
          ? engine.openCodeCheck(config.opencode.key)
          : Promise.resolve(null);
    void Promise.resolve(call).catch(() => null).then(() => {
      setTesting((t) => ({ ...t, [id]: 'ok' }));
      window.setTimeout(() => setTesting((t) => { const n = { ...t }; delete n[id]; return n; }), 2800);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [testing, config]);

  const download = useCallback(() => {
    if (config.local.downloaded || downloadProgress !== null) return;
    setDownloadProgress(0);
    void engine.installDecider().catch(() => undefined);
    let p = 0;
    const timer = window.setInterval(() => {
      p += 0.18;
      if (p >= 1) {
        window.clearInterval(timer);
        setDownloadProgress(null);
        applyService('local', { download: true });
        toast('The built-in models are ready on this PC.', { tone: 'ok' });
        onModelsChanged?.();
      } else setDownloadProgress(p);
    }, 160);
  }, [config.local.downloaded, downloadProgress, applyService, toast, onModelsChanged]);

  const addHost = () => {
    const host = hostInput.trim();
    if (!host) return;
    applyService('ollama', { addHost: host });
    setHostInput('');
  };

  const togglePanel = (id: string) => setOpenPanel((cur) => cur === id ? null : id);

  /* ---------- render helpers (called directly, never mounted as components,
     so controlled inputs keep focus across re-renders) ---------- */
  const detailFoot = () => (
    <div className="d-foot">
      <span className="muted" style={{ fontSize: 11 }}>Press Esc to close</span>
      <span className="end"><button type="button" className="ai-btn link small" onClick={() => setOpenPanel(null)}>Close</button></span>
    </div>
  );

  const progressBar = () => downloadProgress === null ? null : (
    <>
      <div className="dl-progress"><i style={{ width: Math.round(downloadProgress * 100) + '%' }} /></div>
      <div className="dl-label">Downloading the built-in models… about 5 GB</div>
    </>
  );

  const forkHtml = () => (
    <div className="fork">
      <h3>{ai.EMPTY_FORK.heading}</h3>
      <p className="fork-lead">{ai.EMPTY_FORK.lead}</p>
      <div className="fork-opts">
        <button type="button" className="ai-opt" onClick={download} aria-expanded={downloadProgress !== null}>
          <span className="fo-ico">{Ico.download}</span>
          <span className="fo-body">
            <span className="fo-title">{ai.EMPTY_FORK.download.title}</span>
            <span className="fo-sub">{ai.EMPTY_FORK.download.sub}</span>
            {progressBar()}
          </span>
          <span className="fo-caret">{Ico.chevron}</span>
        </button>
        <button type="button" className="ai-opt" aria-expanded={forkChoice === 'cloud'} onClick={() => setForkChoice((c) => c === 'cloud' ? null : 'cloud')}>
          <span className="fo-ico">{Ico.cloud}</span>
          <span className="fo-body">
            <span className="fo-title">{ai.EMPTY_FORK.cloud.title}</span>
            <span className="fo-sub">{ai.EMPTY_FORK.cloud.sub}</span>
            <span className="fo-note">{ai.EMPTY_FORK.cloud.note}</span>
          </span>
          <span className="fo-caret">{Ico.caret}</span>
        </button>
      </div>
      {forkChoice === 'cloud' && (
        <div className="fork-form">
          <div className="ff-title">{ai.EMPTY_FORK.cloudFormTitle}</div>
          {(['opencode', 'openrouter'] as const).map((id) => (
            <div className="ff-row" key={id}>
              <label htmlFor={`fork-${id}`}>{id === 'opencode' ? 'OpenCode key' : 'OpenRouter key'}</label>
              <input id={`fork-${id}`} type="password" placeholder={`Paste your ${id === 'opencode' ? 'OpenCode' : 'OpenRouter'} key`} value={forkKeys[id]}
                onChange={(e) => setForkKeys((k) => ({ ...k, [id]: e.target.value }))} />
              <button type="button" className="ai-btn small" disabled={!forkKeys[id].trim()} onClick={() => { applyService(id, { key: forkKeys[id].trim() }); setForkChoice(null); }}>Save</button>
              <div className="ff-note">{id === 'opencode' ? ai.EMPTY_FORK.opencodeNote : ai.EMPTY_FORK.openrouterNote}</div>
            </div>
          ))}
          <div className="ff-foot">You can add or change these any time in AI services.</div>
        </div>
      )}
    </div>
  );

  const summaryHtml = () => (
    <div className="fork-summary">
      <span className="dot ok" />
      <span className="fs-text">{view.summary}</span>
      <button type="button" className="ai-btn link fs-link" onClick={() => setServicesOpen(true)}>Manage services</button>
    </div>
  );

  const svcLocalHtml = () => {
    const s = config.local;
    const ready = s.downloaded && s.enabled;
    return (
      <div className="svc">
        <div className="svc-head"><span className={`dot ${ready ? 'ok' : 'no'}`} /><span className="svc-name">{ai.SERVICE_META.local.name}</span><span className="tag good">Own PC</span></div>
        {downloadProgress !== null ? progressBar()
          : ready ? <div className="svc-note">The models are ready on this PC. Nothing is uploaded.</div>
            : <button type="button" className="ai-btn svc-dl" onClick={download}>Download the models (about 5 GB)</button>}
        <div className="svc-foot"><label className="svc-enabled">Enabled <Switch checked={s.enabled} disabled={!s.downloaded} onChange={(v) => applyService('local', { enabled: v })} ariaLabel="Enable built-in models" /></label></div>
      </div>
    );
  };

  const svcOllamaHtml = () => {
    const s = config.ollama;
    return (
      <div className="svc">
        <div className="svc-head"><span className={`dot ${s.hosts.length && s.enabled ? 'ok' : 'no'}`} /><span className="svc-name">{ai.SERVICE_META.ollama.name}</span><span className="tag">Your network</span></div>
        <div className="svc-note">Add a host on your network; its models join both car reading and moments.</div>
        <div className="svc-field">
          <input type="text" placeholder="http://localhost:11434" value={hostInput} spellCheck={false} onChange={(e) => setHostInput(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter') addHost(); }} />
          <button type="button" className="ai-btn small" onClick={addHost}>Add host</button>
        </div>
        {s.hosts.length > 0 && <div className="hosts">{s.hosts.map((h) => (
          <div className="host-chip" key={h}><span className="dot ok" /><span className="mono">{h}</span><span className="end"><button type="button" className="ai-btn link small" onClick={() => applyService('ollama', { removeHost: h })}>Remove</button></span></div>
        ))}</div>}
        <div className="svc-foot">
          <label className="svc-enabled">Enabled <Switch checked={s.enabled} disabled={!s.hosts.length} onChange={(v) => applyService('ollama', { enabled: v })} ariaLabel="Enable Ollama hosts" /></label>
          {s.hosts.length > 0 && <span className="end"><button type="button" className="ai-btn small" onClick={() => testService('ollama')}>{testLabel('ollama')}</button></span>}
        </div>
      </div>
    );
  };

  const svcCloudHtml = (id: 'openrouter' | 'opencode') => {
    const s = config[id];
    const meta = ai.SERVICE_META[id];
    const note = id === 'opencode'
      ? 'Add your OpenCode key. Buy credits in the OpenCode Console — Conrod never logs your key.'
      : 'Covers car reading and moments. Its moments model can’t see photos yet.';
    return (
      <div className="svc">
        <div className="svc-head"><span className={`dot ${ai.serving(id, config) ? 'ok' : 'no'}`} /><span className="svc-name">{meta.name}</span><span className="tag">¢ per photo</span></div>
        <div className="svc-field"><input type="password" placeholder={`Paste your ${meta.name} key`} value={s.key} spellCheck={false} onChange={(e) => applyService(id, { key: e.target.value })} /></div>
        <div className="svc-note">{note}</div>
        <div className="svc-foot">
          <label className="svc-enabled">Enabled <Switch checked={s.enabled} disabled={!s.key} onChange={(v) => applyService(id, { enabled: v })} ariaLabel={`Enable ${meta.name}`} /></label>
          <span className="end"><button type="button" className="ai-btn small" disabled={!s.key} onClick={() => testService(id)}>{testLabel(id)}</button></span>
        </div>
      </div>
    );
  };

  const servicesHtml = () => servicesOpen ? (
    <div className="svc-area">
      <div className="svc-area-head"><span>AI services</span><button type="button" className="ai-btn link small" style={{ marginLeft: 'auto' }} onClick={() => setServicesOpen(false)}>Close</button></div>
      <div className="svc-area-hint">Add a key, download the built-in models, or add an Ollama host. Helpers unlock as you go.</div>
      <div className="svc-grid">
        {svcLocalHtml()}{svcOllamaHtml()}{svcCloudHtml('openrouter')}{svcCloudHtml('opencode')}
      </div>
    </div>
  ) : (
    <button type="button" className="svc-bar" aria-expanded="false" onClick={() => setServicesOpen(true)}>
      <span className="dot ok" /><span className="sb-title">AI services</span><span className="tag">{view.countOn} on</span><span className="sb-caret">{Ico.caret}</span>
    </button>
  );

  const simpleHtml = (helper: (typeof ai.HELPERS)[number]) => (
    <div className="detail">
      <p className="d-desc">{helper.desc}</p>
      <div className="d-checks">
        {(ai.SIMPLE_CONTROLS[helper.id] || []).map((c) => (
          <label className="d-check" key={c.key}>
            <Switch checked={Boolean(draft[c.key])} onChange={(v) => onChange(c.key, v)} ariaLabel={c.label} />
            <span>{c.label}</span>
          </label>
        ))}
      </div>
      {detailFoot()}
    </div>
  );

  const versionHtml = (cap: 'vision' | 'decision', group: ReturnType<typeof ai.capGroups>[number]) => {
    const value = modelOf(cap, group.id);
    const list = group.models;
    if (group.freeText) {
      return (
        <div className="ver-row">
          <span className="ver-label">Version</span>
          <input type="text" aria-label={`Version for ${group.name}`} placeholder="Type the model name" value={value} spellCheck={false}
            onChange={(e) => selectModel(cap, group.id, e.target.value)} />
        </div>
      );
    }
    return (
      <div className="ver-row">
        <span className="ver-label">Version</span>
        <select aria-label={`Version for ${group.name}`} value={list.some((m) => m.id === value) ? value : ''} onChange={(e) => selectModel(cap, group.id, e.target.value)}>
          {(!value || !list.some((m) => m.id === value)) && <option value={value || ''}>{value || 'Select a model…'}</option>}
          {list.map((m) => <option key={m.id} value={m.id}>{m.name}</option>)}
        </select>
      </div>
    );
  };

  const pickerHtml = (cap: 'vision' | 'decision', helper: (typeof ai.HELPERS)[number]) => {
    const unlocked = ai.unlockReason(helper, ctx);
    if (unlocked.locked) {
      return (
        <div className="detail">
          <div className="lk-box">
            <div className="lk-ico">{Ico.lock}</div>
            <div className="lk-reason">{unlocked.reason}</div>
            <div className="lk-sub">{unlocked.sub}</div>
            <div className="lk-actions">
              <button type="button" className="ai-btn primary small" onClick={download}>Download the models</button>
              <button type="button" className="ai-btn small" onClick={() => { setServicesOpen(true); setForkChoice('cloud'); }}>Add a cloud key</button>
            </div>
          </div>
          {detailFoot()}
        </div>
      );
    }
    const groups = ai.capGroups(cap, ctx.config, modelLists);
    const strategyKey = cap === 'vision' ? 'vlm_strategy' : 'moments_strategy';
    const strategy = String(draft[strategyKey] ?? 'least_busy');
    return (
      <div className="detail">
        <h4>{cap === 'vision' ? 'Where cars are read' : 'Where moments are checked'}</h4>
        <p className="d-hint">{cap === 'vision'
          ? 'Add this PC or a cloud account. Turn on more than one to share the work.'
          : 'This PC goes first when it has enough graphics memory. Cloud services can share the rest.'}</p>
        {groups.map((g) => (
          <div className="mo-group" key={g.id}>
            <div className="mo-group-head">
              <span className={`dot ${ai.serving(g.id, ctx.config) ? 'ok' : 'no'}`} />
              <span className="mg-name">{g.name}</span><span className="tag">{g.tag}</span>
              {g.key && <span className="key-mask">{maskKey(g.key)}</span>}
              <span className="end">{g.testable && <button type="button" className="ai-btn small" disabled={!ai.serving(g.id, ctx.config)} onClick={() => testService(g.id)}>{testLabel(g.id)}</button>}</span>
            </div>
            <div className={`mo-card${g.disabledNote ? ' disabled' : ''}`}>
              {g.disabledNote
                ? <div className="disabled-note">{g.disabledNote}</div>
                : versionHtml(cap, g)}
              <div className="mo-foot">
                <label className="svc-enabled">Enabled <Switch checked={cfg[g.id]?.enabled ?? false} onChange={(v) => applyService(g.id, { enabled: v })} ariaLabel={`Enable ${g.name}`} /></label>
              </div>
            </div>
          </div>
        ))}
        <button type="button" className="ai-btn small add-service" onClick={() => setServicesOpen(true)}>+ Add a service</button>
        <div className="hws">
          <div className="hws-label">How work is shared</div>
          <Segmented options={ai.STRATS} value={strategy} onChange={(v) => onChange(strategyKey, v)} label="How work is shared" />
          <span className="hint">{ai.STRATS_HINT}</span>
        </div>
        {cap === 'vision' ? (
          <div className="threshold">
            <label className="d-check" style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <Switch checked={Boolean(draft.vlm_reasoning)} onChange={(v) => onChange('vlm_reasoning', v)} ariaLabel="Extended thinking" />
              <span>Extended thinking</span>
            </label>
          </div>
        ) : (
          <>
            <div className="shared-key">
              <label htmlFor="moments-opencode-key">OpenCode key</label>
              <div className="sk-row">
                <input id="moments-opencode-key" type="password" placeholder="Paste your OpenCode key" value={ctx.config.opencode.key} spellCheck={false}
                  onChange={(e) => applyService('opencode', { key: e.target.value })} />
                <button type="button" className="ai-btn small" disabled={!ctx.config.opencode.key} onClick={() => testService('opencode')}>{testLabel('opencode')}</button>
              </div>
              <div className="svc-note" style={{ marginTop: 6 }}>One shared key for every OpenCode service. Conrod never logs it.</div>
            </div>
            <div className="threshold">
              How sure Conrod must be
              <select aria-label="How sure Conrod must be" value={Number(draft.moments_threshold ?? 0.85)}
                onChange={(e) => onChange('moments_threshold', Number(e.target.value))}>
                {[0.7, 0.8, 0.85, 0.9, 0.95].map((v) => <option key={v} value={v}>{Math.round(v * 100)}%</option>)}
              </select>
            </div>
            <label className="d-check" style={{ marginTop: 10 }}>
              <Switch checked={draft.moments_vlm_fallback !== false} onChange={(v) => onChange('moments_vlm_fallback', v)} ariaLabel="Use the car reader if no moment service answers" />
              <span>Use the car reader if no moment service answers</span>
            </label>
          </>
        )}
        {detailFoot()}
      </div>
    );
  };

  const rowHtml = (helper: (typeof ai.HELPERS)[number]) => {
    const st = ai.statusOf(helper, ctx);
    const open = openPanel === helper.id;
    const tone = st.tone as Tone;
    return (
      <div className={`ph-row${open ? ' is-open' : ''}`} key={helper.id} data-helper={helper.id}>
        <div className="ph-row-main">
          <div className="ph-row-title">{helper.title}</div>
          <div className="ph-row-desc">{helper.desc}</div>
        </div>
        <div className="ph-row-meta">
          <span className="tag ph-row-tag">{st.tag}</span>
          <span className={`ph-status ${tone}`}><span className={`dot ${dotClass(tone)}`} />{st.text}</span>
        </div>
        <div className="ph-row-ctl">
          <button type="button" className="ph-change" ref={(el) => { changeRefs.current[helper.id] = el; }} aria-expanded={open} aria-controls={`ai-model-panel-${helper.id}`} onClick={() => togglePanel(helper.id)}>
            Change <span className="chev">{Ico.caret}</span>
          </button>
        </div>
        {open && (
          <div id={`ai-model-panel-${helper.id}`} style={{ gridColumn: '1 / -1' }}>
            {helper.capability
              ? pickerHtml(helper.capability as 'vision' | 'decision', helper)
              : simpleHtml(helper)}
          </div>
        )}
      </div>
    );
  };

  const zoneHtml = (zone: ReturnType<typeof ai.groupByZone>[number]) => (
    <div className="zone" key={zone.id}>
      <div className="zone-head"><span className={`dot ${zone.id === 'pc' ? 'ok' : 'no'}`} />{zone.title}<span className="zh-count">{zone.helpers.length} {zone.helpers.length === 1 ? 'helper' : 'helpers'}</span></div>
      {zone.helpers.length
        ? zone.helpers.map((h) => rowHtml(h))
        : <div className="zone-empty">{zone.empty}</div>}
    </div>
  );

  return (
    <section className="settings-section ai-models ai-settings" aria-labelledby="settings-section-ai-models">
      <header className="settings-section-head">
        <h3 id="settings-section-ai-models">Photo helpers</h3>
        <p className="section-hint">What Conrod finds, checks and reads. Everything runs on this PC unless a row says Cloud.</p>
      </header>
      {view.isEmpty ? forkHtml() : summaryHtml()}
      {servicesHtml()}
      <div className="zones">{view.groups.map((zone) => zoneHtml(zone))}</div>
    </section>
  );
}
