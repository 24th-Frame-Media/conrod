import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { engine, mocked } from '../lib/api';
import {
  ADD_DESC, ADD_ORDER, BUILTIN, BUILTIN_ROW_ID, DECIDER_QUANTS, DECISION_HELPER,
  HELPERS, SERVICE_TYPES, SHARING, SHARING_HINT, SIMPLE_CONTROLS, THINK_HINT,
  addService, builtinRowStates, builtinVisionEnabled, cardsFromDraft, checkOutcome, costLine,
  decisionCandidates, decisionFlag, decisionMeta, downloadProgress, helperStatus, isPicked, modelLine,
  momentsStatus, patchService, poolCount, removeService, servingFor, setBuiltinVision, setSharing,
  sharingValue, summaryText, visionCandidates,
  type AiHelper, type BuiltinModel, type Opt, type ServiceCard,
} from '../lib/aiCard.mjs';
import type { IdentifyVram, ModelInfo, ModelsStatus, SettingValue, Settings as SettingsMap, Toaster } from '../lib/types';
import { placeAnchoredMenu } from '../lib/anchoredMenu.mjs';
import { IdentifyConcurrency } from '../components/IdentifyConcurrency';
import '../styles/aiSettings.css';

type Props = {
  draft: SettingsMap;
  saved: SettingsMap;
  models: ModelInfo[];
  identifyVram?: IdentifyVram | null;
  onChange: (key: string, value: SettingValue) => void;
  toast: Toaster;
  onModelsChanged?: () => void;
};

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

/** The AI services card. The services panel is always shown; each added service
 *  is one card with its own key or host, Test/Remove, a cost line, a per-card
 *  Thinking toggle (default off), and Vision/Decisions dropdowns filled from
 *  the engine's live model lists. */
export function AiSettingsCard({ draft, models, identifyVram, onChange, toast, onModelsChanged }: Props) {
  // Dev-mock shortcuts so a screenshot state opens straight from a URL.
  const dev = () => (import.meta.env.DEV && mocked ? new URLSearchParams(location.search) : new URLSearchParams());

  const cards = useMemo<ServiceCard[]>(() => cardsFromDraft(draft), [draft]);
  const routing = sharingValue(draft);

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

  // Built-in rows read the engine's model manifest + the real use_decider /
  // built-in vision service settings directly, with no local overrides.
  const rowSource = liveRows ?? models;
  const builtinVisionOn = builtinVisionEnabled(draft);
  const builtinRows = useMemo(
    () => builtinRowStates(rowSource, Boolean(draft.use_decider), builtinVisionOn),
    [rowSource, draft.use_decider, builtinVisionOn],
  );

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
    const builtinVision = v && v.on && v.downloaded ? ['qwen3-vl:8b'] : [];
    const d = builtinRows['pplx-decider-local'];
    const builtinDecisions = d && d.on && d.downloaded ? ['pplx-decider-local'] : [];
    const cardsCtx = cards.map((c) => ({
      type: c.type, enabled: c.enabled, visionSel: c.visionSel, decisionSel: c.decisionSel,
      apiKey: c.apiKey, tested: Boolean(tested[c.key]),
      liveModels: (modelLists[c.key] || {}).liveOllama || [],
    }));
    return { builtinVision, builtinDecisions, cards: cardsCtx, bundled, routing };
  }, [cards, builtinRows, tested, modelLists, bundled, routing]);

  const pool = poolCount(ctx);
  const summary = summaryText(ctx);
  // One source of truth for "who is judging moments". `draft` is the live settings
  // wire; `localInstalled` is the built-in local decider's own install state.
  const momentSt = momentsStatus(draft, Boolean(builtinRows['pplx-decider-local']?.downloaded));

  /* Fill each card's Vision/Decisions dropdowns from the engine's live model
     lists. Unknown/failed lists fall back to the known capable models; an
     OpenRouter decisions list with no decision-capable id becomes free text. */
  const listKey = refreshNonce + '::' + cards.map((c) => `${c.key}|${c.type}|${c.apiKey ? 1 : 0}`).join(';');
  useEffect(() => {
    let cancelled = false;
    const tasks = cards.map(async (c): Promise<[string, CardModels]> => {
      const def = SERVICE_TYPES[c.type];
      const key = def.kind === 'key' ? c.apiKey : c.host;
      if (c.type === 'ollama') {
        try {
          const res = await engine.ollamaModels(c.host);
          const live = res.models || [];
          return [c.key, { vision: visionCandidates('ollama', live), decision: [], decisionFreeText: false, liveOllama: live }];
        } catch {
          return [c.key, { vision: [], decision: [], decisionFreeText: false, liveOllama: [] }];
        }
      }
      let live: Opt[] = [];
      try { const res = await engine.providerModels(c.type, '', key); live = res.models || []; } catch { /* no list */ }
      // TODO(engine, backend-owned): OpenRouter decision enumeration is a
      // decision_models command (same { id/model, sees_photos, note? } shape as
      // OpenCode's); both render through this same path. Until it lands,
      // decisionCandidates falls back to the live model list / free text.
      let decisionLive: Opt[] = [];
      let engineFreeText = false;
      if (def.decisions && (c.type === 'opencode' || c.type === 'openrouter')) {
        try {
          const res = await engine.decisionModels(c.type, '', key, '');
          // Keep what the engine said about each model (sees photos, unverified) and its free-text answer.
          decisionLive = (res.models || []).map((m) => ({ id: m.id || m.name, name: m.name || m.id, image: m.image, unverified: m.unverified }));
          engineFreeText = Boolean(res.freeText);
        } catch { /* enumeration command not landed yet -> fallback below */ }
      }
      const dc = decisionCandidates(c.type, live, decisionLive, { freeText: engineFreeText });
      return [c.key, { vision: visionCandidates(c.type, live), decision: dc.models, decisionFreeText: dc.freeText, liveOllama: [] }];
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
    const engineId = BUILTIN_ROW_ID[downloading];
    const st = builtinRowStates(liveRows ?? models, Boolean(draft.use_decider))[downloading];
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
    const entry = Object.entries(BUILTIN_ROW_ID).find(([, eid]) => eid === installInfo.id);
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
  const patchCard = (key: string, patch: Record<string, unknown>) => applyPatch(patchService(draft, key, patch));
  const removeCard = (key: string) => applyPatch(removeService(draft, key));
  const chooseSharing = (value: string) => {
    const p = setSharing(value);
    onChange('vlm_strategy', p.vlm_strategy);
    onChange('moments_strategy', p.moments_strategy);
  };
  const doAdd = (type: string) => {
    const firstOllama = !cards.some((c) => c.type === 'ollama');
    const url = type === 'ollama' ? (firstOllama ? 'http://localhost:11434' : '') : undefined;
    applyPatch(addService(draft, type, { hostIndex: cards.filter((c) => c.type === 'ollama').length + 1, url }));
    setAddMenuOpen(false);
  };

  /* ---------- Test buttons: the REAL engine check commands ---------- */
  const testLabel = (key: string) => testing[key] === 'testing' ? 'Testing…'
    : testing[key] === 'ok' ? 'Ready ✓'
      : testing[key] === 'fail' ? 'Retry' : 'Test';
  const finishTest = (key: string, res: unknown, err?: unknown) => {
    if (!aliveRef.current) return;
    const outcome = checkOutcome(res, err);
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
  const testCard = (c: ServiceCard) => {
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
    const row = BUILTIN.models.find((m) => m.id === id);
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
    const cmd = row.role === 'decisions'
      ? engine.installDecider()
      : engine.installModels(row.assets ? [...row.assets] : []);
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
          const { reason } = checkOutcome(null, err);
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
    const first = BUILTIN.models.find((m) => !builtinRows[m.id]?.downloaded);
    if (first) downloadBuiltin(first.id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /* ---------- render helpers ---------- */
  const pickOptions = (card: ServiceCard, role: 'vision' | 'decisions') => {
    const def = SERVICE_TYPES[card.type];
    if (role === 'decisions' && !def.decisions) return null;
    const ml = modelLists[card.key] || { vision: [], decision: [], decisionFreeText: false, liveOllama: [] };
    if (role === 'vision') {
      const options = ml.vision || [];
      return { options, freeText: false, disabled: options.length === 0, placeholder: card.type === 'ollama' ? 'Test the host to list its models' : 'None offered' };
    }
    const options = ml.decision || [];
    return { options, freeText: ml.decisionFreeText || options.length === 0, disabled: false };
  };

  const pickRow = (card: ServiceCard, role: 'vision' | 'decisions', label: string) => {
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
        <select aria-label={`${label} model`} value={isPicked(sel) ? sel : 'none'} onChange={(e) => setSel(e.target.value)}>
          <option value="none">None — skip</option>
          {/* A saved model the list does not carry stays visible and in use, never shown as None. */}
          {isPicked(sel) && !o.options.some((x) => x.id === sel) && <option value={sel}>{sel} (not in the list)</option>}
          {o.options.map((x) => <option key={x.id} value={x.id}>{x.id}{role === 'decisions' ? decisionFlag(x) : ''}</option>)}
        </select>
      );
    }
    const cur = isPicked(sel) ? sel : null;
    return (
      <div className="pick">
        <div className="pick-line"><label>{label}</label>{control}</div>
        {role === 'decisions' && cur ? <div className="pick-meta">{decisionMeta(o.options.find((x) => x.id === cur))}</div> : null}
      </div>
    );
  };

  const builtinRow = (m: BuiltinModel) => {
    const st = builtinRows[m.id] || { on: false, downloaded: false, available: true };
    const missing = !st.downloaded;
    const engineId = BUILTIN_ROW_ID[m.id];
    // Running is the engine's own install state for this row (or a command we
    // just sent whose tracker entry has not landed yet), never a local guess.
    const running = downloading === m.id || Boolean(installInfo && installInfo.id === engineId && !installInfo.error);
    const anyRunning = downloading !== null || Boolean(installInfo && !installInfo.error);
    const failed = !running && missing && dlError[m.id];
    const prog = running && installInfo && installInfo.id === engineId
      ? downloadProgress(installInfo.done, installInfo.total)
      : null;
    const meta = st.available === false ? 'not available yet'
      : running ? (prog ? prog.label : 'Downloading…')
        : missing ? (m.dl ? `${m.dl} · not downloaded` : 'not downloaded')
          : m.role === 'vision' ? 'on this PC · built-in'
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
    } else if (m.role === 'vision') {
      ctl = <Switch mini on={st.on} ariaLabel={m.id} onChange={(v) => onChange('vlm_services', setBuiltinVision(draft, v) as unknown as SettingValue)} />;
    } else {
      ctl = <Switch mini on={st.on} ariaLabel={m.id} onChange={(v) => onChange('use_decider', v)} />;
    }
    let quant = null;
    if (m.role === 'decisions' && st.on && st.downloaded) {
      const curQuant = String(draft.decider_quant || 'auto');
      quant = (
        <div className="mo-opts"><label>Precision
          <select
            aria-label={`Precision for ${m.id}`}
            value={DECIDER_QUANTS.some((q) => q.id === curQuant) ? curQuant : 'auto'}
            onChange={(e) => onChange('decider_quant', e.target.value)}
          >
            {DECIDER_QUANTS.map((q) => <option key={q.id} value={q.id}>{q.name}</option>)}
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
        {quant}
      </div>
    );
  };

  const builtinCard = () => (
    <section className="service" data-card="builtin">
      <div className="c-head"><span className="dot ok" /><h4>{BUILTIN.name}</h4><span className="tag own">{BUILTIN.tag}</span></div>
      <p className="note">{BUILTIN.note}</p>
      <p className="cost-line">Free — runs on this PC. Nothing is uploaded.</p>
      <div className="mo-scroll">
        <h3 className="mo-group">Reads photos</h3>
        {BUILTIN.models.filter((m) => m.role === 'vision').map(builtinRow)}
        <h3 className="mo-group">Makes decisions</h3>
        {momentSt.path === 'cloud' && (
          <p className="mo-served">Moments are judged by {momentSt.label}. The download below is only for this PC's local model.</p>
        )}
        {BUILTIN.models.filter((m) => m.role === 'decisions').map(builtinRow)}
      </div>
      <div className="svc-foot">
        <span className="foot-label">Always available on this PC</span>
      </div>
    </section>
  );

  const serviceCard = (card: ServiceCard) => {
    const def = SERVICE_TYPES[card.type];
    const title = card.type === 'ollama' ? `${def.name} — ${hostShort(card.host)}` : def.name;
    const picked: { id: string; local: boolean }[] = [];
    if (isPicked(card.visionSel)) picked.push({ id: card.visionSel, local: card.type === 'ollama' });
    if (def.decisions && isPicked(card.decisionSel)) picked.push({ id: card.decisionSel, local: false });
    const cost = costLine(picked);
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
            <div><span className="think-label">Thinking</span><p className="think-hint">{THINK_HINT}</p></div>
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
        {ADD_ORDER.map((type) => {
          const added = type !== 'ollama' && cards.some((c) => c.type === type);
          return (
            <button type="button" key={type} disabled={added}
              onClick={(e) => { e.stopPropagation(); if (!added) doAdd(type); }}>
              <span className="am-name">{SERVICE_TYPES[type].name}</span>
              <span className="am-desc">{ADD_DESC[type]}</span>
              {added && <span className="am-added">Added ✓</span>}
            </button>
          );
        })}
      </div>}
    </div>
  );

  const helperKeys = (h: AiHelper) => (SIMPLE_CONTROLS[h.id] || []).map((c) => c.key);
  const isHelperOn = (h: AiHelper) => {
    const keys = helperKeys(h);
    return keys.length
      ? keys.some((k) => (k in draft ? Boolean(draft[k]) : k === 'use_vlm' || k === 'identify_decision'))
      : true;
  };
  const toggleHelper = (h: AiHelper) => {
    const next = !isHelperOn(h);
    for (const k of helperKeys(h)) onChange(k, next);
  };

  const helperRow = (h: AiHelper) => {
    const on = isHelperOn(h);
    // "Spots moments" names the service that is actually judging, not one bare
    // model id: a cloud service (JEV via OpenCode) needs nothing installed locally.
    if (h.id === DECISION_HELPER) {
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
    const list = servingFor(h.id, ctx);
    const st = helperStatus(on, list);
    const line = modelLine(list);
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
          {SHARING.map((s) => (
            <button key={s.id} className={routing === s.id ? 'on' : ''} aria-pressed={routing === s.id} onClick={() => chooseSharing(s.id)}>{s.name}</button>
          ))}
        </div>
        <span className="share-hint">{SHARING_HINT}</span>
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

      <IdentifyConcurrency draft={draft} identifyVram={identifyVram} onChange={onChange} embedded />

      <section className="helpers">
        <h2 className="section-title">Photo helpers</h2>
        <p className="sub">Each helper shows the model it runs on. Switch one off to skip it.</p>
        <div className="helper-list">{HELPERS.map(helperRow)}</div>
      </section>
    </section>
  );
}
