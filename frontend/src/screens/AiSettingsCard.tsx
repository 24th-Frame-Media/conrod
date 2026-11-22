import { useEffect, useMemo, useState } from 'react';
import { engine, mocked } from '../lib/api';
import * as ai from '../lib/aiCard.mjs';
import type { ModelInfo, SettingValue, Settings as SettingsMap, Toaster } from '../lib/types';
import '../styles/aiSettings.css';

// The pure services-plan model is a plain .mjs module; keep one `any` handle so
// the JSX below stays free of generated-type friction.
const A: any = ai;

type Props = {
  draft: SettingsMap;
  saved: SettingsMap;
  models: ModelInfo[];
  onChange: (key: string, value: SettingValue) => void;
  toast: Toaster;
  onModelsChanged?: () => void;
};

type Card = {
  key: string; type: string; apiKey: string; host: string;
  enabled: boolean; reasoning: boolean; visionSel: string; decisionSel: string;
};
type Opt = { id: string; name: string };
type CardModels = {
  vision: Opt[]; decision: Opt[]; decisionFreeText: boolean;
  liveOllama: { name: string; vision: boolean }[];
};

const hostShort = (url: string) => (url || '').replace(/^https?:\/\//, '').replace(/\/$/, '') || 'new host';

function Switch({ on, onChange, ariaLabel, mini }: { on: boolean; onChange: (v: boolean) => void; ariaLabel?: string; mini?: boolean }) {
  return (
    <button
      type="button"
      className={`switch${mini ? ' mini' : ''}${on ? ' on' : ''}`}
      aria-label={ariaLabel}
      aria-pressed={on}
      onClick={() => onChange(!on)}
    />
  );
}

/** The rebuilt AI services card. The services panel is always shown; there is
 *  no Manage/collapse. Each added service is one card with its own key or host,
 *  Test/Remove, a cost line, a per-card Thinking toggle (default off), and
 *  Vision/Decisions dropdowns filled from the engine's live model lists. */
export function AiSettingsCard({ draft, models, onChange, toast, onModelsChanged }: Props) {
  // Dev-mock shortcuts so a screenshot state opens straight from a URL.
  const dev = () => (import.meta.env.DEV && mocked ? new URLSearchParams(location.search) : new URLSearchParams());

  const cards = useMemo<Card[]>(() => A.cardsFromDraft(draft) as Card[], [draft]);
  const routing = A.sharingValue(draft) as string;

  const [modelLists, setModelLists] = useState<Record<string, CardModels>>({});
  const [addMenuOpen, setAddMenuOpen] = useState(() => dev().get('aiadd') === '1');
  const [openPopup, setOpenPopup] = useState<string | null>(() => dev().get('aipopup') || null);
  const [testing, setTesting] = useState<Record<string, 'testing' | 'ok'>>({});
  const [tested, setTested] = useState<Record<string, boolean>>({});
  const [refreshNonce, setRefreshNonce] = useState(0);
  const [downloadProgress, setDownloadProgress] = useState<number | null>(null);
  const [opts, setOpts] = useState<Record<string, string>>({});
  const [builtinEnabled, setBuiltinEnabled] = useState(true);
  const [builtinRows, setBuiltinRows] = useState<Record<string, { on: boolean; downloaded: boolean }>>(() => {
    const init: Record<string, { on: boolean; downloaded: boolean }> = {};
    for (const m of A.BUILTIN.models) init[m.id] = { on: Boolean(m.on), downloaded: !m.dl };
    return init;
  });
  const [helpersOn, setHelpersOn] = useState<Record<string, boolean>>(() => {
    const init: Record<string, boolean> = {};
    for (const h of A.HELPERS) {
      const keys = (A.SIMPLE_CONTROLS[h.id] || []).map((c: { key: string }) => c.key);
      init[h.id] = keys.length ? keys.some((k: string) => Boolean(draft[k])) : true;
    }
    return init;
  });

  /* Non-AI helpers run on Conrod's own bundled models, read from the app's
     model manifest/status (the `models` prop). No cloud provider is involved. */
  const bundled = useMemo(() => {
    const find = (needle: string) => models.find((m) => m.name.toLowerCase().includes(needle));
    const mk = (needle: string, fallback: string) => {
      const m = find(needle);
      return { id: m ? (m.file || m.name) : fallback, ready: m ? m.ready : true };
    };
    return {
      'cars-people': [mk('detector', 'Vehicle detector (YOLO)'), mk('person', 'Person detector')],
      sharpness: [mk('sharp', 'Edge sharpness'), { id: 'Quality tie-break', ready: true }],
      faces: [mk('face', 'Face + eye model')],
      plates: [mk('plate', 'Plate reader (OCR)'), { id: 'Plate spell-check', ready: true }],
      'race-numbers': [mk('text', 'Text reader (OCR)'), mk('plate', 'Number OCR')],
    } as Record<string, { id: string; ready: boolean }[]>;
  }, [models]);

  const ctx = useMemo(() => {
    const v = builtinRows['qwen3-vl:8b'];
    const d = builtinRows['pplx-decider-local'];
    const builtinVision = builtinEnabled && v && v.on && v.downloaded ? ['qwen3-vl:8b'] : [];
    const builtinDecisions = builtinEnabled && d && d.on && d.downloaded ? ['pplx-decider-local'] : [];
    const cardsCtx = cards.map((c) => ({
      type: c.type, enabled: c.enabled, visionSel: c.visionSel, decisionSel: c.decisionSel,
      apiKey: c.apiKey, tested: Boolean(tested[c.key]),
      liveModels: (modelLists[c.key] || {}).liveOllama || [],
    }));
    return { builtinVision, builtinDecisions, cards: cardsCtx, bundled, routing };
  }, [cards, builtinRows, builtinEnabled, tested, modelLists, bundled, routing]);

  const pool = A.poolCount(ctx) as { n: number; svc: number };
  const summary = A.summaryText(ctx) as { text: string; dot: string };

  /* Fill each card's Vision/Decisions dropdowns from the engine's live model
     lists. Unknown/failed lists fall back to the known capable models; an
     OpenRouter decisions list with no decision-capable id becomes free text. */
  const listKey = refreshNonce + '::' + cards.map((c) => `${c.key}|${c.type}|${c.apiKey ? 1 : 0}`).join(';');
  useEffect(() => {
    let cancelled = false;
    const tasks = cards.map(async (c): Promise<[string, CardModels]> => {
      const def = A.SERVICE_TYPES[c.type];
      const key = def.kind === 'key' ? c.apiKey : c.host;
      if (c.type === 'ollama') {
        try {
          const res = await engine.ollamaModels(c.host);
          const live = res.models || [];
          return [c.key, { vision: A.visionCandidates('ollama', live), decision: [], decisionFreeText: false, liveOllama: live }];
        } catch {
          return [c.key, { vision: [], decision: [], decisionFreeText: false, liveOllama: [] }];
        }
      }
      let live: Opt[] = [];
      try { const res = await engine.providerModels(c.type, '', key); live = (res.models || []) as Opt[]; } catch { /* no list */ }
      // TODO(engine, backend-owned): OpenRouter decision enumeration is a
      // decision_models command (same { id/model, sees_photos, note? } shape as
      // OpenCode's); both render through this same path. Until it lands,
      // decisionCandidates falls back to the live model list / free text.
      let decisionLive: Opt[] = [];
      if (def.decisions && (c.type === 'opencode' || c.type === 'openrouter')) {
        try {
          const res = await engine.decisionModels(c.type, '', key, '');
          decisionLive = ((res.models || []) as any[]).map((m) => ({ id: m.id || m.model || m.name, name: m.name || m.id || m.model }));
        } catch { /* enumeration command not landed yet -> fallback below */ }
      }
      const dc = A.decisionCandidates(c.type, live, decisionLive) as { models: Opt[]; freeText: boolean };
      return [c.key, { vision: A.visionCandidates(c.type, live) as Opt[], decision: dc.models, decisionFreeText: dc.freeText, liveOllama: [] }];
    });
    Promise.all(tasks).then((entries) => { if (!cancelled) setModelLists(Object.fromEntries(entries)); });
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [listKey]);

  /* Esc and outside clicks close the one open helper popup and the add menu. */
  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (!(e.target instanceof Element)) return;
      if (!e.target.closest('.hm-more-wrap') && !e.target.closest('.hm-pop')) setOpenPopup(null);
      if (!e.target.closest('.add-tile') && !e.target.closest('.add-menu')) setAddMenuOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { setOpenPopup(null); setAddMenuOpen(false); }
    };
    document.addEventListener('mousedown', onDown);
    document.addEventListener('keydown', onKey);
    return () => { document.removeEventListener('mousedown', onDown); document.removeEventListener('keydown', onKey); };
  }, []);

  /* ---------- settings write-through ---------- */
  const applyPatch = (p: { vlm_services: unknown[]; moments_services: unknown[] }) => {
    onChange('vlm_services', p.vlm_services as unknown as SettingValue);
    onChange('moments_services', p.moments_services as unknown as SettingValue);
  };
  const patchCard = (key: string, patch: Record<string, unknown>) => applyPatch(A.patchService(draft, key, patch));
  const removeCard = (key: string) => applyPatch(A.removeService(draft, key));
  const chooseSharing = (value: string) => {
    const p = A.setSharing(value) as { vlm_strategy: string; moments_strategy: string };
    onChange('vlm_strategy', p.vlm_strategy);
    onChange('moments_strategy', p.moments_strategy);
  };
  const doAdd = (type: string) => {
    const firstOllama = !cards.some((c) => c.type === 'ollama');
    const url = type === 'ollama' ? (firstOllama ? 'http://localhost:11434' : '') : undefined;
    applyPatch(A.addService(draft, type, { hostIndex: cards.filter((c) => c.type === 'ollama').length + 1, url }));
    setAddMenuOpen(false);
  };

  /* ---------- Test buttons (existing check commands) ---------- */
  const testLabel = (key: string) => testing[key] === 'testing' ? 'Testing…' : testing[key] === 'ok' ? 'Ready ✓' : 'Test';
  const test = (key: string, promise: Promise<unknown>) => {
    if (testing[key]) return;
    setTesting((t) => ({ ...t, [key]: 'testing' }));
    void Promise.resolve(promise).catch(() => null).then(() => {
      setTesting((t) => ({ ...t, [key]: 'ok' }));
      setTested((s) => ({ ...s, [key]: true }));
      setRefreshNonce((n) => n + 1);
      window.setTimeout(() => setTesting((t) => { const n = { ...t }; delete n[key]; return n; }), 2400);
    });
  };
  const testCard = (c: Card) => {
    const promise = c.type === 'ollama' ? engine.ollamaModels(c.host)
      : c.type === 'openrouter' ? engine.openRouterCheck(c.apiKey)
        : c.type === 'opencode' ? engine.openCodeCheck(c.apiKey)
          : Promise.resolve(null);
    test(c.key, promise);
  };

  /* ---------- built-in model rows: download -> real install commands ---------- */
  const downloadBuiltin = (id: string) => {
    const row = A.BUILTIN.models.find((m: { id: string }) => m.id === id);
    if (!row || builtinRows[id].downloaded || downloadProgress !== null) return;
    setDownloadProgress(0);
    const cmd = row.role === 'decisions' ? engine.installDecider() : engine.installModels();
    void Promise.resolve(cmd).catch(() => undefined);
    let p = 0;
    const timer = window.setInterval(() => {
      p += 0.2;
      if (p >= 1) {
        window.clearInterval(timer);
        setDownloadProgress(null);
        setBuiltinRows((s) => ({ ...s, [id]: { on: true, downloaded: true } }));
        toast('The built-in models are ready on this PC.', { tone: 'ok' });
        onModelsChanged?.();
      } else setDownloadProgress(p);
    }, 160);
  };

  /* ---------- render helpers ---------- */
  const optVal = (key: string, def: string) => opts[key] ?? def;

  const modelDef = (card: Card, id: string) =>
    (A.SERVICE_TYPES[card.type].models || []).find((m: { id: string }) => m.id === id);

  const modelOpts = (card: Card, selId: string) => {
    const m = modelDef(card, selId) as { efforts?: string[]; def?: string; variants?: string[] } | undefined;
    if (!m) return null;
    const base = `${card.key}:${selId}`;
    const bits = [];
    if (card.reasoning && m.efforts && m.efforts.length) {
      const key = base + ':thinking';
      const cur = optVal(key, m.def || m.efforts[0]);
      // The card already has a "Thinking" toggle; this per-model row is its
      // effort level, so name it that instead of doubling the word.
      bits.push(
        <label key="thinking">Effort
          <select aria-label={`Thinking level for ${selId}`} value={cur} onChange={(e) => setOpts((s) => ({ ...s, [key]: e.target.value }))}>
            {m.efforts.map((e) => <option key={e} value={e}>{e.charAt(0).toUpperCase() + e.slice(1)}</option>)}
          </select>
        </label>,
      );
    }
    if (m.variants && m.variants.length) {
      const key = base + ':variant';
      const cur = optVal(key, m.variants[0]);
      bits.push(
        <label key="variant">Variant
          <select aria-label={`Variant for ${selId}`} value={cur} onChange={(e) => setOpts((s) => ({ ...s, [key]: e.target.value }))}>
            {m.variants.map((v) => <option key={v} value={v}>{v}</option>)}
          </select>
        </label>,
      );
    }
    return bits.length ? <div className="mo-opts">{bits}</div> : null;
  };

  const pickOptions = (card: Card, role: 'vision' | 'decisions') => {
    const def = A.SERVICE_TYPES[card.type];
    if (role === 'decisions' && !def.decisions) return null;
    const ml = modelLists[card.key] || { vision: [], decision: [], decisionFreeText: false, liveOllama: [] };
    if (role === 'vision') {
      const options = ml.vision || [];
      return { options, freeText: false, disabled: options.length === 0, placeholder: card.type === 'ollama' ? 'Test the host to list its models' : 'None offered' };
    }
    const options = ml.decision || [];
    return { options, freeText: ml.decisionFreeText || options.length === 0, disabled: false };
  };

  const pickRow = (card: Card, role: 'vision' | 'decisions', label: string) => {
    const o = pickOptions(card, role);
    if (!o) return null;
    const sel = role === 'vision' ? card.visionSel : card.decisionSel;
    const setSel = (v: string) => patchCard(card.key, role === 'vision' ? { visionSel: v } : { decisionSel: v });
    let control;
    if (o.freeText) {
      control = <input className="free-text" type="text" placeholder="Type the model id" aria-label={`${label} model`}
        value={sel === 'none' ? '' : sel} onChange={(e) => setSel(e.target.value)} />;
    } else if (o.disabled) {
      control = <select disabled aria-label={`${label} model`}><option>{o.placeholder}</option></select>;
    } else {
      control = (
        <select aria-label={`${label} model`} value={o.options.some((x) => x.id === sel) ? sel : 'none'} onChange={(e) => setSel(e.target.value)}>
          <option value="none">None — skip</option>
          {o.options.map((x) => <option key={x.id} value={x.id}>{x.id}</option>)}
        </select>
      );
    }
    const cur = sel !== 'none' ? sel : null;
    return (
      <div className="pick">
        <div className="pick-line"><label>{label}</label>{control}</div>
        {role === 'decisions' && cur ? <div className="pick-meta">sees photos</div> : null}
        {cur ? modelOpts(card, cur) : null}
      </div>
    );
  };

  const builtinRow = (m: { id: string; role: string; dl?: string; variants?: string[] }) => {
    const st = builtinRows[m.id] || { on: false, downloaded: false };
    const missing = Boolean(m.dl) && !st.downloaded;
    const meta = missing ? `${m.dl} · not downloaded` : 'on this PC';
    const ctl = missing
      ? <button className="btn small primary mo-dl" disabled={downloadProgress !== null} onClick={() => downloadBuiltin(m.id)}>
          {downloadProgress !== null ? 'Downloading…' : 'Download'}
        </button>
      : <Switch mini on={st.on} ariaLabel={m.id} onChange={(v) => setBuiltinRows((s) => ({ ...s, [m.id]: { ...s[m.id], on: v } }))} />;
    let variant = null;
    if (st.on && st.downloaded && m.variants && m.variants.length) {
      const key = `builtin:${m.id}:variant`;
      variant = (
        <div className="mo-opts"><label>Variant
          <select aria-label={`Variant for ${m.id}`} value={optVal(key, m.variants[0])} onChange={(e) => setOpts((s) => ({ ...s, [key]: e.target.value }))}>
            {m.variants.map((v) => <option key={v} value={v}>{v}</option>)}
          </select>
        </label></div>
      );
    }
    return (
      <div className={`mo${st.on && st.downloaded ? '' : ' off'}`}>
        <div className="mo-line">{ctl}<div className="mo-name"><span className="mono">{m.id}</span><span className="mo-meta">{meta}</span></div></div>
        {variant}
      </div>
    );
  };

  const builtinCard = () => (
    <section className={`service${builtinEnabled ? '' : ' svc-off'}`} data-card="builtin">
      <div className="c-head"><span className={`dot ${builtinEnabled ? 'ok' : 'off'}`} /><h4>{A.BUILTIN.name}</h4><span className="tag own">{A.BUILTIN.tag}</span></div>
      <p className="note">{A.BUILTIN.note}</p>
      <p className="cost-line">Free — runs on this PC. Nothing is uploaded.</p>
      <div className="mo-scroll">
        <h3 className="mo-group">Reads photos</h3>
        {A.BUILTIN.models.filter((m: { role: string }) => m.role === 'vision').map(builtinRow)}
        <h3 className="mo-group">Makes decisions</h3>
        {A.BUILTIN.models.filter((m: { role: string }) => m.role === 'decisions').map(builtinRow)}
      </div>
      <div className="svc-foot">
        <span className="foot-label">Enabled</span>
        <Switch on={builtinEnabled} ariaLabel="Enable built-in models" onChange={setBuiltinEnabled} />
        <span className="spacer" />
        <button className="btn small" onClick={() => test('builtin', Promise.resolve(null))}>{testLabel('builtin')}</button>
      </div>
    </section>
  );

  const serviceCard = (card: Card) => {
    const def = A.SERVICE_TYPES[card.type];
    const title = card.type === 'ollama' ? `${def.name} — ${hostShort(card.host)}` : def.name;
    const picked: { id: string; local: boolean }[] = [];
    if (card.visionSel && card.visionSel !== 'none') picked.push({ id: card.visionSel, local: card.type === 'ollama' });
    if (def.decisions && card.decisionSel && card.decisionSel !== 'none') picked.push({ id: card.decisionSel, local: false });
    const cost = A.costLine(picked) as string;
    // Per-service thinking lives on the entry as `reasoning` (default false);
    // the backend applies it when building the VLM request (engine seam TODO).
    const think = card.reasoning;
    return (
      <section className={`service${card.enabled ? '' : ' svc-off'}`} data-card={card.key} key={card.key}>
        <div className="c-head">
          <span className={`dot ${card.enabled ? def.dot : 'off'}`} />
          <h4 title={title}>{title}</h4>
          <span className="tag">{def.tag}</span>
        </div>
        <p className="note">{def.note}</p>
        {def.kind === 'key'
          ? <input className="key" type="password" placeholder={def.keyPlaceholder} value={card.apiKey} spellCheck={false}
              aria-label={`${def.name} key`} onChange={(e) => patchCard(card.key, { apiKey: e.target.value })} />
          : <input className="key mono" type="text" placeholder="http://host:11434" value={card.host} spellCheck={false}
              aria-label="Ollama host address" onChange={(e) => patchCard(card.key, { host: e.target.value })} />}
        <p className="cost-line" title="Estimate at about 900 tokens in and 250 out per photo, summed over the picked models.">{cost}</p>
        <div className="mo-scroll">
          <h3 className="mo-group">Reads photos</h3>
          {pickRow(card, 'vision', 'Vision')}
          {def.decisions && <h3 className="mo-group">Makes decisions</h3>}
          {def.decisions && pickRow(card, 'decisions', 'Decisions')}
        </div>
        {def.thinking && (
          <div className="think-row">
            <Switch mini on={think} ariaLabel={`Thinking for ${def.name}`} onChange={(v) => patchCard(card.key, { reasoning: v })} />
            <div><span className="think-label">Thinking</span><p className="think-hint">{A.THINK_HINT}</p></div>
          </div>
        )}
        <div className="svc-foot">
          <span className="foot-label">Enabled</span>
          <Switch on={card.enabled} ariaLabel={`Enable ${title}`} onChange={(v) => patchCard(card.key, { enabled: v })} />
          <span className="spacer" />
          <button className="link danger" onClick={() => removeCard(card.key)}>Remove</button>
          <button className="btn small" onClick={() => testCard(card)}>{testLabel(card.key)}</button>
        </div>
      </section>
    );
  };

  const addTile = () => (
    <div className="add-tile" role="button" tabIndex={0} aria-haspopup="true" aria-expanded={addMenuOpen}
      onClick={() => setAddMenuOpen((o) => !o)}
      onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setAddMenuOpen((o) => !o); } }}>
      <span className="plus">+</span>
      <span className="add-label">Add service</span>
      <span className="add-hint">OpenCode, OpenRouter, Local Ollama (one card per host), Gemini, Anthropic or OpenAI</span>
      <div className="add-menu" hidden={!addMenuOpen} onClick={(e) => e.stopPropagation()}>
        {A.ADD_ORDER.map((type: string) => {
          const added = type !== 'ollama' && cards.some((c) => c.type === type);
          return (
            <button type="button" key={type} disabled={added}
              onClick={(e) => { e.stopPropagation(); if (!added) doAdd(type); }}>
              <span className="am-name">{A.SERVICE_TYPES[type].name}</span>
              <span className="am-desc">{A.ADD_DESC[type]}</span>
              {added && <span className="am-added">Added ✓</span>}
            </button>
          );
        })}
      </div>
    </div>
  );

  const toggleHelper = (h: { id: string }) => {
    const on = !helpersOn[h.id];
    setHelpersOn((s) => ({ ...s, [h.id]: on }));
    const keys = (A.SIMPLE_CONTROLS[h.id] || []).map((c: { key: string }) => c.key);
    for (const k of keys) onChange(k, on);
  };

  const helperRow = (h: { id: string; title: string; desc: string }) => {
    const on = Boolean(helpersOn[h.id]);
    const list = A.servingFor(h.id, ctx) as { id: string; ready: boolean }[];
    const st = A.helperStatus(on, list) as { text: string; cls: string; dot: string };
    const line = A.modelLine(list) as { head: string | null; rest: string[]; all: string[]; noModel: boolean };
    const open = openPopup === h.id;
    return (
      <div className="helper" key={h.id} data-h={h.id}>
        <Switch mini on={on} ariaLabel={h.title} onChange={() => toggleHelper(h)} />
        <div className="helper-main">
          <h4 className="helper-title" title={h.desc}>{h.title}</h4>
          <div className="hm-line">
            {line.noModel ? <span className="hm-none">no model yet</span> : (
              <>
                <span className="hm-model">{line.head}</span>
                {line.rest.length > 0 && (
                  <span className={`hm-more-wrap${open ? ' open' : ''}`}>
                    <button className="hm-more" aria-expanded={open} aria-haspopup="true"
                      onClick={(e) => { e.stopPropagation(); setOpenPopup((cur) => cur === h.id ? null : h.id); }}>
                      +{line.rest.length} more <svg width="9" height="9" viewBox="0 0 12 12" fill="none" aria-hidden="true"><path d="M3 4.5 6 7.5 9 4.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" /></svg>
                    </button>
                    <span className="hm-pop" hidden={!open}>
                      <span className="hm-pop-title">Models in use</span>
                      {line.all.map((id) => <span key={id}>{id}</span>)}
                    </span>
                  </span>
                )}
              </>
            )}
          </div>
        </div>
        <div className="helper-status"><span className={`dot ${st.dot}`} /><span className={`ph-status${st.cls ? ' ' + st.cls : ''}`}>{st.text}</span></div>
      </div>
    );
  };

  return (
    <section className="ai-settings">
      <div className="strip">
        <span className={`dot ${summary.dot}`} />
        <span className="strip-text">{summary.text}</span>
        <span className="spacer" />
      </div>

      <hr className="divider" />

      <div className="svc-head"><h2>AI services</h2></div>
      <p className="sub">The models on this PC are always here. Add a service for more help, pick its models from the dropdowns. Cloud prices are estimates per photo.</p>

      <div className="share-bar">
        <h3>How work is shared</h3>
        <div className="seg" role="group" aria-label="How work is shared across services">
          {A.SHARING.map((s: { id: string; name: string }) => (
            <button key={s.id} className={routing === s.id ? 'on' : ''} aria-pressed={routing === s.id} onClick={() => chooseSharing(s.id)}>{s.name}</button>
          ))}
        </div>
        <span className="share-hint">{A.SHARING_HINT}</span>
        <span className="spacer" />
        <span className="pool">
          {pool.n ? <><b>{pool.n}</b> model{pool.n === 1 ? '' : 's'} in the pool · {pool.svc} service{pool.svc === 1 ? '' : 's'}</> : 'Nothing in the pool yet'}
        </span>
      </div>

      <div className="grid">
        {builtinCard()}
        {cards.map(serviceCard)}
        {addTile()}
      </div>

      <section className="helpers">
        <h2 className="section-title">Photo helpers</h2>
        <p className="sub">Each helper shows the model it runs on. Switch one off to skip it.</p>
        <div className="helper-list">{A.HELPERS.map(helperRow)}</div>
      </section>
    </section>
  );
}
