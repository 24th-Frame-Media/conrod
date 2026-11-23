import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { engine, mocked } from '../lib/api';
import * as ai from '../lib/aiCard.mjs';
import type { ModelInfo, ModelsStatus, SettingValue, Settings as SettingsMap, Toaster } from '../lib/types';
import { placeAnchoredMenu } from '../lib/anchoredMenu.mjs';
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
  const [testing, setTesting] = useState<Record<string, 'testing' | 'ok' | 'fail'>>({});
  const [testErrors, setTestErrors] = useState<Record<string, string>>({});
  const [tested, setTested] = useState<Record<string, boolean>>({});
  const [refreshNonce, setRefreshNonce] = useState(0);
  // Which built-in model is installing right now (null = none): a real running
  // state that ends only when the manifest shows the model appeared.
  const [downloading, setDownloading] = useState<string | null>(null);
  // Live disk-truth rows from `models_status` (never the health list, which
  // carries cloud service rows). Null until the first re-check lands; the
  // Bootstrap `models` prop is only the fallback before then.
  const [liveRows, setLiveRows] = useState<ModelInfo[] | null>(null);
  const [installInfo, setInstallInfo] = useState<ModelsStatus['install']>(null);
  const [dlError, setDlError] = useState<Record<string, string>>({});
  const pollRef = useRef<number | null>(null);
  const sawInstallRef = useRef(false);
  const startedAtRef = useRef(0);
  // One ref for every pending timer so unmount never leaves one behind.
  const timeoutsRef = useRef<Set<number>>(new Set());
  const aliveRef = useRef(true);
  const [opts, setOpts] = useState<Record<string, string>>({});
  const [builtinEnabled, setBuiltinEnabled] = useState(true);
  // Built-in rows read the engine's model manifest + the real use_decider
  // setting, never hardcoded flags. Only an explicit switch toggle is local.
  const [switchOverrides, setSwitchOverrides] = useState<Record<string, boolean>>({});
  const rowSource = liveRows ?? models;
  const derivedRows = useMemo(
    () => A.builtinRowStates(rowSource, Boolean(draft.use_decider)) as Record<string, { on: boolean; downloaded: boolean; available: boolean }>,
    [rowSource, draft.use_decider],
  );
  const builtinRows = useMemo<Record<string, { on: boolean; downloaded: boolean; available: boolean }>>(() => {
    const out: Record<string, { on: boolean; downloaded: boolean; available: boolean }> = {};
    for (const id of Object.keys(derivedRows)) {
      const d = derivedRows[id];
      out[id] = { downloaded: d.downloaded, available: d.available, on: d.downloaded ? (switchOverrides[id] ?? d.on) : false };
    }
    return out;
  }, [derivedRows, switchOverrides]);
  const [helpersOn, setHelpersOn] = useState<Record<string, boolean>>(() => {
    const init: Record<string, boolean> = {};
    for (const h of A.HELPERS) {
      const keys = (A.SIMPLE_CONTROLS[h.id] || []).map((c: { key: string }) => c.key);
      init[h.id] = keys.length ? keys.some((k: string) => Boolean(draft[k])) : true;
    }
    return init;
  });

  // Anchors for the two viewport-clamped menus (see placeAnchoredMenu).
  const addAnchorRef = useRef<HTMLSpanElement | null>(null);
  const addMenuRef = useRef<HTMLDivElement | null>(null);
  const helperPopRef = useRef<HTMLSpanElement | null>(null);

  /* Re-place every open anchored menu when one opens, on window resize, and on
     any scroll (the Settings pane scrolls in its own container, so capture). */
  useLayoutEffect(() => {
    if (!openPopup && !addMenuOpen) return;
    const helperAnchor = openPopup
      ? document.querySelector<HTMLElement>(`.ai-settings .helper[data-h="${openPopup}"] .hm-more-wrap`)
      : null;
    const place = () => {
      if (openPopup && helperAnchor && helperPopRef.current) placeAnchoredMenu(helperAnchor, helperPopRef.current, 'left');
      if (addMenuOpen && addAnchorRef.current && addMenuRef.current) placeAnchoredMenu(addAnchorRef.current, addMenuRef.current, 'center');
    };
    place();
    window.addEventListener('resize', place);
    document.addEventListener('scroll', place, true);
    return () => {
      window.removeEventListener('resize', place);
      document.removeEventListener('scroll', place, true);
    };
  }, [openPopup, addMenuOpen]);

  /* Non-AI helpers run on Conrod's own bundled models, read from the app's
     live model rows. No cloud provider is involved, and a missing row is
     never assumed ready. */
  const bundled = useMemo(() => {
    const find = (needle: string) => rowSource.find((m) => m.name.toLowerCase().includes(needle));
    const mk = (needle: string, fallback: string) => {
      const m = find(needle);
      return { id: m ? (m.file || m.name) : fallback, ready: m ? Boolean(m.downloaded ?? m.ready) : false };
    };
    const quality = rowSource.find((m) => m.id === 'quality');
    return {
      'cars-people': [mk('detector', 'Vehicle detector (YOLO)'), mk('person', 'Person detector')],
      sharpness: [{ id: 'Edge sharpness', ready: true }, {
        // Edge sharpness is Conrod's own measure, not a downloadable model:
        // it is always here. The quality tie-break IS a model (opt-in).
        id: quality && quality.available === false ? 'Quality tie-break (not available yet)' : 'Quality tie-break',
        ready: quality ? Boolean(quality.downloaded ?? quality.ready) : false,
      }],
      faces: [mk('face', 'Face + eye model')],
      plates: [mk('plate', 'Plate reader (OCR)'), { id: 'Plate spell-check', ready: true }],
      'race-numbers': [mk('text', 'Text reader (OCR)'), mk('plate', 'Number OCR')],
    } as Record<string, { id: string; ready: boolean }[]>;
  }, [rowSource]);

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
  // One source of truth for "who is judging moments". `draft` is the live settings
  // wire; `localInstalled` is the built-in local decider's own install state.
  const momentSt = A.momentsStatus(draft, Boolean(builtinRows['pplx-decider-local']?.downloaded)) as {
    path: string; service: string | null; model: string | null; label: string; status: string; ready: boolean; others: string[];
  };

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

  const stopPolling = () => {
    if (pollRef.current != null) { window.clearInterval(pollRef.current); pollRef.current = null; }
  };
  /* Clear every pending test/install timer and the install poll on unmount. */
  useEffect(() => {
    aliveRef.current = true;
    return () => {
      aliveRef.current = false;
      stopPolling();
      for (const t of timeoutsRef.current) window.clearTimeout(t);
      timeoutsRef.current.clear();
    };
  }, []);

  /* Truth on open: re-read every local model row from disk the moment the
     card mounts, instead of trusting the Bootstrap snapshot (which can be
     stale, and whose health refresh carries cloud rows). */
  const refreshLive = async () => {
    try {
      const res = await engine.modelsStatus();
      if (!aliveRef.current) return;
      setLiveRows(res.rows || []);
      setInstallInfo(res.install ?? null);
      if (res.install && res.install.id) sawInstallRef.current = true;
      // An install already in flight (started here, or before the card opened)
      // keeps its live progress polling; a finished or failed one stops it.
      if (res.install && !res.install.error) {
        if (pollRef.current == null) pollRef.current = window.setInterval(() => { void refreshLive(); }, 1000);
      } else if (downloading == null) {
        stopPolling();
      }
    } catch { /* the engine will answer on the next poll */ }
  };
  useEffect(() => {
    void refreshLive();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /* A Download is finished only when the real manifest shows the model's files
     on disk (the engine flips `downloaded` only after hash verification). A
     failure is the engine's own plain reason, never a timer pretending success. */
  useEffect(() => {
    if (!downloading) return;
    const engineId = A.BUILTIN_ROW_ID[downloading] as string;
    const st = A.builtinRowStates(liveRows ?? models, Boolean(draft.use_decider))[downloading] as { downloaded: boolean } | undefined;
    if (st?.downloaded) {
      stopPolling();
      setDownloading(null);
      setDlError((s) => { const n = { ...s }; delete n[downloading]; return n; });
      toast('The built-in model is ready on this PC.', { tone: 'ok' });
      onModelsChanged?.();
      return;
    }
    const fail = (reason: string) => {
      stopPolling();
      setDownloading(null);
      setDlError((s) => ({ ...s, [downloading]: reason }));
      toast(`Install failed — ${reason}`, { tone: 'error' });
    };
    if (installInfo && installInfo.id === engineId && installInfo.error) {
      fail(String(installInfo.error));
      return;
    }
    // The install ended (tracker cleared) without the model appearing: it was
    // stopped or died. Only conclude this once we actually saw it running.
    if (!installInfo && sawInstallRef.current) {
      fail('The install stopped before the model was verified. Check disk space and try again.');
      return;
    }
    if (!installInfo && !sawInstallRef.current && Date.now() - startedAtRef.current > 15000) {
      fail('The install did not start. Check the activity list and try again.');
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [liveRows, installInfo, downloading, draft.use_decider]);

  /* A failed install the engine still remembers shows on its row when the card
     opens: the plain reason and a Retry, not a silent Download button. */
  useEffect(() => {
    if (!installInfo?.error || downloading) return;
    const entry = Object.entries(A.BUILTIN_ROW_ID as Record<string, string>).find(([, eid]) => eid === installInfo.id);
    if (!entry) return;
    const [cardId] = entry;
    const reason = String(installInfo.error);
    setDlError((s) => (s[cardId] === reason ? s : { ...s, [cardId]: reason }));
  }, [installInfo, downloading]);

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

  /* ---------- Test buttons: the REAL engine check commands ---------- */
  const testLabel = (key: string) => testing[key] === 'testing' ? 'Testing…'
    : testing[key] === 'ok' ? 'Ready ✓'
      : testing[key] === 'fail' ? 'Retry' : 'Test';
  const finishTest = (key: string, res: unknown, err?: unknown) => {
    if (!aliveRef.current) return;
    const outcome = A.checkOutcome(res, err) as { ready: boolean; reason: string };
    setTesting((t) => ({ ...t, [key]: outcome.ready ? 'ok' : 'fail' }));
    if (outcome.ready) {
      setTestErrors((s) => { const n = { ...s }; delete n[key]; return n; });
      setTested((s) => ({ ...s, [key]: true }));
      setRefreshNonce((n) => n + 1);
    } else {
      // A bad key / offline host is never "tested": name the engine's plain reason.
      setTested((s) => { const n = { ...s }; delete n[key]; return n; });
      setTestErrors((s) => ({ ...s, [key]: outcome.reason }));
      toast(`Not ready — ${outcome.reason}`, { tone: 'error' });
    }
    const t = window.setTimeout(() => {
      timeoutsRef.current.delete(t);
      if (aliveRef.current) setTesting((prev) => { const n = { ...prev }; delete n[key]; return n; });
    }, 3200);
    timeoutsRef.current.add(t);
  };
  const test = (key: string, promise: Promise<unknown>) => {
    if (testing[key] === 'testing') return;
    setTesting((t) => ({ ...t, [key]: 'testing' }));
    setTestErrors((s) => { const n = { ...s }; delete n[key]; return n; });
    void Promise.resolve(promise).then(
      (res) => finishTest(key, res),
      (err) => finishTest(key, undefined, err),
    );
  };
  const testCard = (c: Card) => {
    const promise: Promise<unknown> = c.type === 'ollama' ? engine.ollamaModels(c.host)
      : c.type === 'openrouter' ? engine.openRouterCheck(c.apiKey)
        : c.type === 'opencode' ? engine.openCodeCheck(c.apiKey)
          : engine.providerModels(c.type, '', c.apiKey);
    test(c.key, promise);
  };
  // Dev QA only: `&aitest=1` runs each card's real check once on open, so a
  // failing-key state can be screenshotted without an interactive click.
  const autoTested = useRef(false);
  useEffect(() => {
    if (autoTested.current || !dev().get('aitest') || !cards.length) return;
    autoTested.current = true;
    for (const c of cards) testCard(c);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cards]);

  /* ---------- built-in model rows: download -> real install commands ---------- */
  const downloadBuiltin = (id: string) => {
    const row = A.BUILTIN.models.find((m: { id: string }) => m.id === id);
    if (!row || builtinRows[id]?.downloaded || downloading) return;
    if (installInfo && !installInfo.error) return; // one install at a time on the card
    if (builtinRows[id]?.available === false) return; // nothing pinned to fetch
    setDownloading(id);
    setDlError((s) => { const n = { ...s }; delete n[id]; return n; });
    setInstallInfo(null);
    sawInstallRef.current = false;
    startedAtRef.current = Date.now();
    // Exactly the clicked model's assets (vision), or the decider's own
    // installer (weights + projector + runtime, VRAM-gated quant).
    const cmd = row.role === 'decisions' ? engine.installDecider() : engine.installModels(row.assets as string[]);
    void Promise.resolve(cmd).then(
      () => {
        if (!aliveRef.current) return;
        // Poll the live models status until the row's files are verified on
        // disk, or the engine reports a plain failure. No timer pretends an
        // install finished, and a multi-GB download is never cut off by a
        // short try-count.
        stopPolling();
        pollRef.current = window.setInterval(() => { void refreshLive(); }, 1000);
        void refreshLive();
      },
      (err) => {
        stopPolling();
        if (aliveRef.current) {
          setDownloading(null);
          const reason = (A.checkOutcome(null, err) as { reason: string }).reason;
          setDlError((s) => ({ ...s, [id]: reason }));
          toast(`Install failed — ${reason}`, { tone: 'error' });
        }
      },
    );
  };
  // Dev QA only: `&aidownload=1` starts one real built-in install on open, so
  // the running -> installed transition can be screenshotted.
  const autoDownloaded = useRef(false);
  useEffect(() => {
    if (autoDownloaded.current || !dev().get('aidownload')) return;
    autoDownloaded.current = true;
    const first = A.BUILTIN.models.find((m: { id: string }) => !builtinRows[m.id]?.downloaded);
    if (first) downloadBuiltin(first.id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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
        value={sel === 'none' ? '' : sel} onChange={(e) => setSel(e.target.value.trim() ? e.target.value : 'none')} />;
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
    const cur = A.isPicked(sel) ? sel : null;
    return (
      <div className="pick">
        <div className="pick-line"><label>{label}</label>{control}</div>
        {role === 'decisions' && cur ? <div className="pick-meta">sees photos</div> : null}
        {cur ? modelOpts(card, cur) : null}
      </div>
    );
  };

  const builtinRow = (m: { id: string; role: string; dl?: string; variants?: string[] }) => {
    const st = builtinRows[m.id] || { on: false, downloaded: false, available: true };
    const missing = !st.downloaded;
    const engineId = A.BUILTIN_ROW_ID[m.id] as string;
    // Running is the engine's own install state for this row (or a command we
    // just sent whose tracker entry has not landed yet), never a local guess.
    const running = downloading === m.id || Boolean(installInfo && installInfo.id === engineId && !installInfo.error);
    const anyRunning = downloading !== null || Boolean(installInfo && !installInfo.error);
    const failed = !running && missing && dlError[m.id];
    const prog = running && installInfo && installInfo.id === engineId
      ? A.downloadProgress(installInfo.done, installInfo.total) as { known: boolean; pct: number | null; label: string }
      : null;
    const meta = st.available === false ? 'not available yet'
      : running ? (prog ? prog.label : 'Downloading…')
        : missing ? (m.dl ? `${m.dl} · not downloaded` : 'not downloaded')
          : 'on this PC';
    let ctl;
    if (st.available === false) {
      // No pinned source exists: never offer a Download that cannot work.
      ctl = <span className="mo-na">Not available yet</span>;
    } else if (running) {
      ctl = <button className="btn small primary mo-dl running" disabled>
        <span className="mo-spin" aria-hidden="true" />Installing…
      </button>;
    } else if (missing) {
      ctl = <button className={`btn small primary mo-dl${failed ? ' retry' : ''}`} disabled={anyRunning} onClick={() => downloadBuiltin(m.id)}>
        {failed ? 'Retry' : 'Download'}
      </button>;
    } else {
      ctl = <Switch mini on={st.on} ariaLabel={m.id} onChange={(v) => {
        // The decision row's switch is the real use_decider setting (the
        // moments lane reads it); the vision row's stays a local preference
        // until the in-process runner ships.
        if (m.role === 'decisions') onChange('use_decider', v);
        else setSwitchOverrides((s) => ({ ...s, [m.id]: v }));
      }} />;
    }
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
      <div className={`mo${st.on && st.downloaded ? '' : ' off'}`} key={m.id}>
        <div className="mo-line">{ctl}<div className="mo-name"><span className="mono">{m.id}</span><span className="mo-meta">{meta}</span></div></div>
        {running && prog?.known && (
          <div className="mo-progress" role="progressbar" aria-valuenow={prog.pct ?? 0} aria-valuemin={0} aria-valuemax={100} aria-label={`Downloading ${m.id}`}>
            <span style={{ width: `${prog.pct}%` }} />
          </div>
        )}
        {failed && <p className="mo-fail" role="status">{dlError[m.id]}</p>}
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
        {momentSt.path === 'cloud' && (
          <p className="mo-served">Moments are judged by {momentSt.label}. The download below is only for this PC's local model.</p>
        )}
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
    if (A.isPicked(card.visionSel)) picked.push({ id: card.visionSel, local: card.type === 'ollama' });
    if (def.decisions && A.isPicked(card.decisionSel)) picked.push({ id: card.decisionSel, local: false });
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
        {testErrors[card.key]
          ? <p className="test-reason" role="status">Not ready — {testErrors[card.key]}</p>
          : null}
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
      <span className="add-label" ref={addAnchorRef}>Add service</span>
      <span className="add-hint">OpenCode, OpenRouter, Local Ollama (one card per host), Gemini, Anthropic or OpenAI</span>
      {addMenuOpen && <div className="add-menu" ref={addMenuRef} onClick={(e) => e.stopPropagation()}>
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
      </div>}
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
    // "Spots moments" names the service that is actually judging, not one bare
    // model id: a cloud service (JEV via OpenCode) needs nothing installed locally.
    if (h.id === A.DECISION_HELPER) {
      const st = !on
        ? { text: 'Off', cls: 'off', dot: 'off' }
        : { text: momentSt.status, cls: momentSt.ready ? '' : 'locked', dot: momentSt.ready ? 'ok' : 'amber' };
      return (
        <div className="helper" key={h.id} data-h={h.id}>
          <Switch mini on={on} ariaLabel={h.title} onChange={() => toggleHelper(h)} />
          <div className="helper-main">
            <h4 className="helper-title" title={h.desc}>{h.title}</h4>
            <div className="hm-line">
              {momentSt.path === 'cloud'
                ? <span className="hm-model" title={momentSt.label}>{momentSt.label}{momentSt.others.length ? ` +${momentSt.others.length} more` : ''}</span>
                : momentSt.ready
                  ? <span className="hm-model">pplx-decider-local</span>
                  : <span className="hm-none">local decision model</span>}
            </div>
          </div>
          <div className="helper-status"><span className={`dot ${st.dot}`} /><span className={`ph-status${st.cls ? ' ' + st.cls : ''}`}>{st.text}</span></div>
        </div>
      );
    }
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
                    {open && <span className="hm-pop" ref={helperPopRef}>
                      <span className="hm-pop-title">Models in use</span>
                      {line.all.map((id) => <span key={id}>{id}</span>)}
                    </span>}
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
