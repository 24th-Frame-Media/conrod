import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { getCurrentWindow, UserAttentionType } from '@tauri-apps/api/window';
import { Icon, KeysDialog, Splash } from './components/basics';
import { TitleBar, type StatusActions } from './components/TitleBar';
import { SetupWizard } from './components/SetupWizard';
import { WhatsNewSheet } from './components/WhatsNewSheet';
import { call, engine, inTauri, mocked } from './lib/api';
import { filename } from './lib/review';
import { queueLabel } from './lib/format';
import { healthStatus, modelGroups } from './lib/health.mjs';
import { openJobPage } from './lib/navigation.mjs';
import { classifyScanError, type ScanFailure } from './lib/scanError.mjs';
import { buildRunSnapshot } from './lib/runSnapshot.mjs';
import { subjectWords } from './lib/subjects.mjs';
import { applyOverrides, defaultSummary, isOn, overridesOf } from './lib/presetPolicy.mjs';
import { albumSetting } from './lib/album.mjs';
import { windowTitle } from './lib/devMode.mjs';
import { useDevInfo, useReloadState } from './lib/dev';
import { subjectExcluded } from './review.mjs';
import { decisionServiceLabel, vlmModelLabel } from './lib/vlm';
import type { DevStart } from './lib/mock';
import type { IdentifyVram, Job, Page, RunSnapshot, ScanArgs, Scope, Settings as SettingsMap, Status } from './lib/types';
import type { SettingsTabId } from './lib/settings.mjs';
import { albumVisionOn, type VisionAction } from './lib/vision.mjs';
import { Library } from './screens/Library';
import { noReviewVerbs, ReviewScreen, type ReviewVerbs } from './screens/Review';
import { ImportDialog, type ImportDefaults, type ScanDraft } from './screens/Import';
import { IdentifyScreen } from './screens/Identify';
import { ScanScreen } from './screens/Scan';
import { Settings } from './screens/Settings';
import { useAppVersion, useFolderDrop, useLocalFlag, useRun } from './state/hooks';
import { useToast } from './state/toast';
import { useEngine } from './state/useEngine';
import { useHotkeys } from './state/useHotkeys';
import { useIdentifyProgress } from './state/useIdentifyProgress';
import { useReview } from './state/useReview';
import { useScanProgress } from './state/useScanProgress';
import { useScanStatus } from './state/useScanStatus';
import { useStartup } from './state/useStartup';
import { useWhatsNew } from './state/useWhatsNew';
import { useWizard } from './state/useWizard';

/** Wires the engine hooks to the screens; every screen is a plain component that gets what it shows as props. */
export function App() {
  const toast = useToast();
  const run = useRun();
  const [page, setPage] = useState<Page>('Library');
  const [jobId, setJobId] = useState<number | null>(null);
  const [profile, setProfile] = useState('motorsport');
  const [draft, setDraft] = useState<ScanDraft>({ root: '', label: '' });
  const [starting, setStarting] = useState(false);
  const [showImport, setShowImport] = useState(false);
  /** A `scan` call that threw before an album existed; the scan screen keeps its plain reason in view. */
  const [scanError, setScanError] = useState<ScanFailure | null>(null);
  // A Settings tab another screen asked for; the nonce lets the same tab be asked for twice.
  const [settingsTab, setSettingsTab] = useState<{ id: SettingsTabId; nonce: number } | undefined>();
  // Best-effort mirror of Review's loupe/compare state, only for the Enter-key open-guard and dev-start below;
  // Review.tsx owns the real `view` state and App reaches it through `reviewVerbs` (see useHotkeys wiring).
  const [viewer, setViewer] = useState(false);
  const [help, setHelp] = useState(false);
  const [showBoxes, setShowBoxes] = useLocalFlag('conrod.boxes', true);
  const [dev, setDev] = useState<DevStart | null>(null);
  const reviewVerbs = useRef<ReviewVerbs>(noReviewVerbs);
  const whatsNew = useWhatsNew();

  const rv = useReview(jobId, run);
  const eng = useEngine(jobId, rv.refresh, run);
  // Who actually serves the moments job, in plain words (a cloud service, else the local path).
  const decisionService = decisionServiceLabel(eng.settings);
  const startup = useStartup();
  const appVersion = useAppVersion();
  const wizard = useWizard({ ready: eng.ready, settings: eng.settings, albums: eng.jobs.length, markSeen: whatsNew.markSeen });
  const { status } = eng;
  // Dev mode: the engine read `--dev` / `CONROD_DEV=1`. The DEV + live-reload
  // chips, the splash strip and the Settings banner all read this one object.
  const version = appVersion ?? (mocked ? '0.0.0-mock' : '');
  const devInfo = useDevInfo({ dev: eng.dev, devUrl: eng.devUrl, devHome: eng.devHome, version, mocked });
  const reloadState = useReloadState(devInfo);
  useEffect(() => { document.title = windowTitle('Conrod', eng.dev); }, [eng.dev]);
  const scanning = status.activeJob !== null;
  const scanLive = useScanProgress(status.activeJob);
  /** The selected album's scan, read on entry, so the Scan screen is whole on re-entry, restart or a queued album. */
  const scanStatus = useScanStatus(jobId, status.activeJob);
  const identifyLive = useIdentifyProgress();
  /** The album on screen, and its resolved settings — the single source of the
   *  flowchart's greyed steps, VRAM/concurrency and type suggestion. */
  const currentAlbum = eng.jobs.find((j) => j.id === jobId) ?? null;
  const resolvedSummary = useMemo(() => (currentAlbum
    ? applyOverrides(
      defaultSummary(String(currentAlbum.scan_profile || eng.settings.scan_profile || 'motorsport')),
      overridesOf(albumSetting(currentAlbum, 'overrides', null)),
      overridesOf(albumSetting(currentAlbum, 'user_defaults', null)),
    )
    : null), [currentAlbum, eng.settings.scan_profile]);
  const surfaceSubject = useMemo(() => subjectWords(resolvedSummary), [resolvedSummary]);
  const albumResolvedSettings = useMemo(() => {
    if (!currentAlbum) return eng.settings;
    return {
      ...eng.settings,
      scan_profile: String(currentAlbum.scan_profile || eng.settings.scan_profile || 'motorsport'),
      identify_ocr: (albumSetting(currentAlbum, 'identify_ocr', eng.settings) as boolean | undefined) ?? true,
      identify_decision: (albumSetting(currentAlbum, 'identify_decision', eng.settings) as boolean | undefined) ?? true,
      use_vlm: albumVisionOn(currentAlbum, eng.settings),
      read_plates: resolvedSummary ? isOn(resolvedSummary, 'plates') : Boolean(albumSetting(currentAlbum, 'read_plates', eng.settings)),
      read_numbers: resolvedSummary ? isOn(resolvedSummary, 'numbers') : Boolean(albumSetting(currentAlbum, 'read_numbers', eng.settings)),
      read_text: Boolean(albumSetting(currentAlbum, 'read_text', eng.settings)),
      moments_detect: resolvedSummary ? isOn(resolvedSummary, 'moments_detect') : eng.settings.moments_detect !== false,
    };
  }, [eng.settings, currentAlbum, resolvedSummary]);
  const identifyVram: IdentifyVram | null = eng.identifyVram
    ?? (eng.settings as unknown as { identifyVram?: IdentifyVram | null }).identifyVram
    ?? null;
  /**
   * The single run snapshot. One object, built from the engine `status` + live
   * events (hydrate-then-live) plus the album's resolved settings, drives the
   * TitleBar badge, the jobs dropdown, the Scan/Ingest flowchart, lane grid,
   * header counters and the run surface. Nothing downstream invents a stage, a
   * worker or a status.
   */
  const runSnapshot = useMemo<RunSnapshot>(() => buildRunSnapshot({
    status, jobs: eng.jobs, liveByJob: scanLive, hydrate: scanStatus, seed: scanError, jobId,
    settings: albumResolvedSettings, album: currentAlbum, identifyVram, decisionService, subject: surfaceSubject,
  }) as RunSnapshot, [status, eng.jobs, scanLive, scanStatus, scanError, jobId, albumResolvedSettings, currentAlbum, identifyVram, decisionService, surfaceSubject]);
  const rawStagePause = albumSetting(currentAlbum, 'native_stage_pause', null);
  const pausedIdentifyStages = Array.isArray(rawStagePause)
    ? rawStagePause.filter((s): s is string => typeof s === 'string' && s.startsWith('identify:'))
    : [];
  const identifyPaused = status.tasks.some((task) => task.state === 'paused' && /^Identifying\b/.test(task.label))
    || pausedIdentifyStages.length > 0;
  // Review stays open while its album scans: it shows the frames already measured, and the
  // pipeline actions (cull, identify, write) stay disabled by Review's own `busy` rule.
  const view: Page = page;
  const running = jobId != null && status.activeJob === jobId && (scanLive[jobId]?.progress.state ?? 'running') === 'running';
  const health = healthStatus({ tasks: status.tasks, models: eng.models });
  // The Library strip's "Setup needed" line: model readiness, computed here so the
  // screen itself no longer needs the models / health props the old rail used.
  const { local: libraryLocal } = modelGroups(eng.models);
  const librarySetup = {
    needed: libraryLocal.some((m) => !m.ready),
    ready: libraryLocal.filter((m) => m.ready).length,
    total: libraryLocal.length,
  };
  const goPage = (next: Page) => {
    if (next === 'Scan' && status.activeJob != null) setJobId(status.activeJob);
    if (next === 'Identify') {
      // The tab is live for whichever album is identifying: open that album, not the last one browsed.
      const key = status.operations.find((k) => k === `Identifying:${jobId}`) ?? status.operations.find((k) => k.startsWith('Identifying:'));
      const id = key ? Number(key.slice('Identifying:'.length)) : NaN;
      if (Number.isFinite(id)) setJobId(id);
    }
    setPage(next);
  };
  // The request is for one visit: leaving Settings forgets it, so the next visit opens on the first tab.
  useEffect(() => { if (page !== 'Settings') setSettingsTab(undefined); }, [page]);
  // The Vision off card's button: switch vision on for this album alone, or go and set a service up.
  const visionAction = (action: VisionAction['id']) => {
    if (action === 'service') {
      setSettingsTab({ id: 'identification', nonce: Date.now() });
      setPage('Settings');
      return;
    }
    if (jobId == null) return;
    void run(async () => {
      await engine.updateJobSettings(jobId, { use_vlm: true });
      await eng.refreshJobs();
      toast('Vision is on for this album. Run Identify again to use it.', { tone: 'ok' });
    });
  };

  /** A greyed step's shortcut: open this album's Manage shoot. */
  const [shootSettingsNonce, setShootSettingsNonce] = useState(0);
  const openShootSettings = () => {
    if (jobId == null) return;
    setPage('Review');
    setShootSettingsNonce((v) => v + 1);
  };
  /** Per-stage controls (pause, skip, rerun) sent to the engine. */
  const stageControl = (
    action: 'stage_pause' | 'stage_skip' | 'stage_rerun',
    stage: string,
    extra?: { paused?: boolean; skip?: boolean; full?: boolean },
  ) => void run(async () => {
    if (jobId == null) return;
    await engine.stageControl(action, jobId, stage, extra);
    eng.setStatus(await engine.status());
    await eng.refreshJobs();
  });
  /** Pause or resume all Identify sub-stages (OCR, Decision, Vision) cleanly at their next ask boundary. */
  const pauseIdentify = (paused: boolean) => void run(async () => {
    if (jobId == null) return;
    for (const stage of ['identify:ocr', 'identify:decision', 'identify:vision']) {
      await engine.stageControl('stage_pause', jobId, stage, { paused });
    }
    eng.setStatus(await engine.status());
    await eng.refreshJobs();
    toast(paused ? 'Identification paused. Anything in flight will finish, then hold.' : 'Resuming identification.');
  });
  /** Accept a shoot-type suggestion: one click, never silent; only un-run Vision prompts change. */
  const acceptTypeSuggestion = (type: string) => void run(async () => {
    if (jobId == null) return;
    const res = await engine.acceptTypeSuggestion(jobId, type);
    await eng.refreshJobs();
    if (res && res.accepted === false) {
      toast(res.note || 'The vision stage has already run; nothing left to change.');
    } else {
      toast('Shoot type accepted. Only the vision reads still to come will use it.', { tone: 'ok' });
    }
  });
  const handleSettingsSaved = (saved: SettingsMap) => {
    eng.setSettings(saved);
    void eng.refreshIdentifyVram().catch(() => undefined);
  };

  useEffect(() => {
    if (eng.ready && typeof eng.settings.scan_profile === 'string') setProfile(eng.settings.scan_profile);
  }, [eng.ready]); // eslint-disable-line react-hooks/exhaustive-deps -- only the first load seeds the chosen scan type

  useEffect(() => {
    if (!import.meta.env.DEV || !mocked) return;
    void import('./lib/mock').then((m) => {
      const d = m.devStart();
      setDev(d);
      if (d.page) setPage(d.page);
      if (d.jobId) setJobId(d.jobId);
      // QA only: `?page=settings&tab=identification` opens a tab directly for screenshots.
      if (d.page === 'Settings') {
        const tab = new URLSearchParams(location.search).get('tab');
        if (tab) setSettingsTab({ id: tab as SettingsTabId, nonce: 1 });
      }
    });
  }, []);
  useEffect(() => {
    if (dev?.viewer && rv.frames.length) { setViewer(true); reviewVerbs.current.setView('loupe'); setDev({ ...dev, viewer: false }); }
  }, [dev, rv.frames.length]);

  const dropping = useFolderDrop((folder) => {
    setDraft({ root: folder, label: filename(folder) });
    setShowImport(true);
    toast(`Ready to import ${filename(folder)}. Check the options and press Scan.`);
  });

  const startScan = async (args: ScanArgs, defaults?: ImportDefaults) => {
    setStarting(true);
    setScanError(null);
    // The sheet's choices belong to this album: they travel with the scan and
    // land in its job settings. Global defaults are only changed from Settings.
    const scan: ScanArgs = defaults
      ? {
          ...args, scanProfile: defaults.scan_profile, blurStrictness: defaults.blur_strictness, burstGap: defaults.burst_gap,
          groupingStacks: defaults.grouping_stacks, groupingSensitivity: defaults.grouping_sensitivity, groupingPause: defaults.grouping_pause,
          groupingKeepPans: defaults.grouping_keep_pans, groupingFocusAnchor: defaults.grouping_focus_anchor,
          groupingMerge: defaults.grouping_merge,
          workflowMode: defaults.workflow_mode, identifyScope: defaults.identify_scope,
        }
      : args;
    let id: number | null = null;
    let duplicates = 0, added = 0, started = true, queued = false, queuePosition = 0;
    try {
      // Narrow the try: ONLY a failure to START the scan is a scan failure. The
      // follow-up reads below run after acceptance — a failure there is its own
      // surfaced error, never a bogus "scan failed" banner.
      const r = await engine.scan(scan);
      id = r.jobId; duplicates = r.duplicates ?? 0; added = r.added ?? 0;
      started = r.scanning ?? true; queued = r.queued ?? false; queuePosition = r.queuePosition ?? 0;
    } catch (e) {
      // A scan that never started gets one plain reason on the Ingest screen, not a raw toast.
      // Clear jobId when a fresh import failed before creating an album so a
      // previously open album never hides the import error banner.
      const jid = 'jobId' in scan ? scan.jobId : null;
      const root = 'root' in scan ? scan.root : draft.root;
      const label = 'label' in scan ? scan.label : (jid != null ? eng.jobs.find((j) => j.id === jid)?.label ?? draft.label : draft.label);
      setScanError({ ...classifyScanError(String(e), { root, label }), jobId: jid ?? undefined, label, raw: String(e), args: scan });
      setJobId(jid ?? null);
      setShowImport(false);
      setPage('Scan');
      return;
    } finally {
      setStarting(false);
    }
    // The scan is running. A failed status/jobs read must NOT look like a failed scan.
    try {
      await eng.refreshJobs();
      eng.setStatus(await engine.status());
    } catch (e) {
      toast(`The scan started, but its status could not be refreshed: ${String(e)}`, { tone: 'error' });
    }
    if (id != null) setJobId(id);
    setShowImport(false);
    // A folder that is already an album gains only its new frames: say what
    // was skipped instead of silently scanning (or duplicating) again.
    if (!started && added === 0 && duplicates > 0) {
      setPage('Library');
      toast(`${duplicates.toLocaleString()} photos are already in this library. Nothing was imported.`, { tone: 'ok' });
      return;
    }
    // Another scan was running: the album is made and waits its turn; the
    // running one starts it when it ends.
    if (queued) {
      setPage('Library');
      toast(`Album queued. ${queueLabel(Math.max(0, queuePosition - 1))}. It starts when the running scan finishes.`, { tone: 'ok' });
      return;
    }
    if (duplicates > 0) {
      toast(`${duplicates.toLocaleString()} already-imported photos were skipped${added ? `; ${added} new ${added === 1 ? 'frame' : 'frames'} added` : ''}.`, { tone: 'ok' });
    }
    setPage('Scan');
    if (inTauri) void getCurrentWindow().requestUserAttention(UserAttentionType.Informational);
  };
  /** `scope` is the run's override; absent, the engine reads the album's own `identify_scope`. `onlyNew` reads just the subjects nobody has read yet. */
  const identify = (id: number, scope?: Scope, onlyNew = false) => void run(async () => {
    setJobId(id);
    setPage('Identify');
    // Clear any leftover identify stage pauses so a fresh/continued run starts moving right away.
    for (const stage of ['identify:ocr', 'identify:decision', 'identify:vision']) {
      await engine.stageControl('stage_pause', id, stage, { paused: false }).catch(() => undefined);
    }
    await engine.identify(id, scope, onlyNew);
    await eng.refreshJobs();
    toast('Identification started. The live trace is open.');
  });
  const write = (id: number, dryRun: boolean) => void run(async () => {
    await engine.write(id, dryRun);
    toast(dryRun ? 'Checking what would be written. The activity menu lists it and no file is touched.' : 'Writing started. Progress is in the activity menu.');
  });
  const openJob = (id: number) => {
    const job = eng.jobs.find((j) => j.id === id);
    // A stale null-job import failure must not follow the user into an album.
    setScanError(null);
    setJobId(id);
    // The primary entry point: hydrate from scan_status on jobId change, then go live.
    setPage(openJobPage(job, status.activeJob));
  };

  const actions: StatusActions = {
    pause: () => void run(async () => { await engine.pause(); toast('Paused. Anything mid-analysis will still finish.'); }),
    resume: () => void run(async () => { await engine.resumeScan(); toast('Resuming'); }),
    stop: () => void run(async () => {
      const s = await engine.stop();
      eng.setStatus(s as Status);
      toast('Stopping scan. Progress on current photos is saved.');
    }),
    cancel: (key) => void run(() => engine.cancelOperation(key)),
    unqueue: (j: Job) => void run(async () => {
      await engine.unqueue(j.id);
      eng.setStatus(await engine.status());
      await eng.refreshJobs();
      toast(`${j.label} is out of the queue. Start it from its card when you're ready.`);
    }),
  };
  // Albums waiting their turn, in the engine's order, named from the loaded job list.
  const queueJobs = (status.queue ?? []).map((id) => eng.jobs.find((j) => j.id === id)).filter((j): j is Job => Boolean(j));

  useHotkeys({
    page: view, viewerOpen: viewer, rejected: () => reviewVerbs.current.rejected(), picked: () => reviewVerbs.current.picked(),
    stepFrame: (d, repeat) => reviewVerbs.current.stepFrame(d, repeat),
    stepBurst: (d) => reviewVerbs.current.stepBurst(d),
    nextBurst: (d) => reviewVerbs.current.nextBurst(d),
    nextUndecided: () => reviewVerbs.current.nextUndecided(),
    enter: () => reviewVerbs.current.enter(),
    mark: (values) => reviewVerbs.current.mark(values),
    stackAction: (mode) => reviewVerbs.current.stackAction(mode),
    undo: () => reviewVerbs.current.undo(),
    redo: () => reviewVerbs.current.redo(),
    openViewer: () => { if (rv.frame) { setViewer(true); reviewVerbs.current.setView('loupe'); } },
    closeOverlays: () => {
      if (help || showImport) { setHelp(false); setShowImport(false); return; }
      setViewer(false);
      reviewVerbs.current.escape();
    },
    toggleHelp: () => setHelp((v) => !v),
    setView: (v) => { setViewer(v === 'loupe'); reviewVerbs.current.setView(v); },
  });

  return (
    <div className="app">
      <TitleBar page={view} setPage={goPage} jobs={eng.jobs} models={eng.models} status={status} health={health} actions={actions} popoverOpen={dev?.popover ?? false} dev={devInfo} reloadState={reloadState} runSnapshot={runSnapshot} onOpenJob={openJob} />
      <main className="stage" key={view}>
        {view === 'Library' && (
          <Library jobs={eng.jobs} scanning={scanning} live={scanLive} activeJob={status.activeJob} queue={status.queue ?? []}
            setup={librarySetup} onOpen={(j) => openJob(j.id)} onNewScan={() => setShowImport(true)}
            onResume={(j) => void startScan({ jobId: j.id })} onUnqueue={actions.unqueue}
            onOpenSettings={() => visionAction('service')}
            onDelete={(j) => void run(async () => {
              await engine.deleteJob(j.id);
              await eng.refreshJobs();
              if (jobId === j.id) setJobId(null);
              toast('Album deleted');
            })} />
        )}
        {view === 'Scan' && (
          <ScanScreen job={eng.jobs.find((j) => j.id === jobId)} live={jobId != null ? scanLive[jobId] : undefined} scanStatus={scanStatus} running={running}
            queue={queueJobs} onUnqueue={actions.unqueue} runSnapshot={runSnapshot}
            paused={status.tasks.some((t) => t.state === 'paused' && !/^Identifying\b/.test(t.label))} run={run} onSettingsSaved={handleSettingsSaved}
            identify={jobId != null ? identifyLive[jobId] : undefined} status={status} identifyPaused={identifyPaused}
            model={vlmModelLabel(eng.settings)} decisionService={decisionService} scanError={scanError}
            onResumeJob={() => { if (jobId != null) void startScan({ jobId }); }}
            onRetryScan={() => { if (scanError?.args) void startScan(scanError.args); else if (jobId != null) void startScan({ jobId }); else actions.resume(); }}
            onPause={actions.pause} onResume={actions.resume}
            onPauseIdentify={() => pauseIdentify(true)} onResumeIdentify={() => pauseIdentify(false)}
            onCancel={actions.stop} onCancelOperation={actions.cancel} onVisionAction={visionAction}
            surface={runSnapshot.surface} onStageControl={stageControl} onAcceptTypeSuggestion={acceptTypeSuggestion} onOpenShootSettings={openShootSettings}
            onReview={() => setPage('Review')} onLibrary={() => setPage('Library')} />
        )}
        {view === 'Identify' && (
          <IdentifyScreen job={eng.jobs.find((j) => j.id === jobId)} live={jobId != null ? identifyLive[jobId] : undefined}
            status={status} model={vlmModelLabel(eng.settings)} decisionService={decisionService}
            paused={identifyPaused} onPause={() => pauseIdentify(true)} onResume={() => pauseIdentify(false)}
            onStageControl={stageControl} onCancel={actions.cancel} onVisionAction={visionAction} onReview={() => setPage('Review')}
            onContinueIdentify={() => {
              if (jobId == null) return;
              const job = eng.jobs.find((j) => j.id === jobId);
              // A scan cut short resumes at the stage it recorded, which is the
              // identify it was in. A finished album whose identify ran on its
              // own reads only the subjects nobody has; both decide only the
              // frames nobody has judged, so nothing paid for runs twice.
              if (job && job.status !== 'done' && job.status !== 'done_errors') void startScan({ jobId });
              else identify(jobId, undefined, true);
            }} />
        )}
        {view === 'Review' && (
          <ReviewScreen rv={rv} jobs={eng.jobs} jobId={jobId} scanning={status.activeJob === jobId && jobId !== null} status={status} settings={eng.settings}
            showBoxes={showBoxes} onToggleBoxes={() => setShowBoxes((v) => !v)} verbs={reviewVerbs}
            onJobsChanged={eng.refreshJobs} onPickJob={setJobId} onCull={() => jobId != null && void startScan({ jobId })} onIdentify={(scope, onlyNew) => jobId != null && identify(jobId, scope, onlyNew)} onHelp={() => setHelp(true)} onOpenViewer={() => setViewer(true)}
              onLibrary={() => setPage('Library')} onWatchIdentify={() => jobId != null && setPage('Identify')} onWatchScan={() => setPage('Scan')}
              openShootSettings={shootSettingsNonce}
              onAcceptAll={() => void run(async () => {
                const ids = rv.review.detections.filter((d) => !d.reviewed && rv.review.frames.some((f) => f.id === d.image_id && !subjectExcluded(f, d))).map((d) => d.id);
                if (ids.length) await call('bulk_edit', { ids, reviewed: true });
                await call('seed_known', { jobId });
                await rv.refresh();
                toast(`${ids.length.toLocaleString()} detections confirmed. Write XMP when you're ready.`, { tone: 'ok' });
              })}
              onSkipIdentify={() => void run(async () => {
                if (jobId == null) return;
                await engine.cancelOperation(`Identifying:${jobId}`);
                eng.setStatus(await engine.status());
                const ids = rv.review.detections.filter((d) => !d.reviewed).map((d) => d.id);
                if (ids.length) await call('bulk_edit', { ids, reviewed: true });
                await rv.refresh();
                toast('Identification skipped. Write XMP when you\'re ready.', { tone: 'ok' });
              })}
              onSkipAlbumIdentify={() => void run(async () => {
                if (jobId == null) return;
                const r = await engine.skipIdentify(jobId);
                await rv.refresh();
                toast(`Skipped. ${r.skipped.toLocaleString()} subjects accepted as they stand.`, { tone: 'ok' });
              })}
              onWrite={(dryRun) => jobId != null && write(jobId, dryRun)}
              lightroomDev={dev?.lightroom}
              onEdit={(id, updates) => run(async () => { await engine.editDetection(id, updates); await rv.refresh(); })} />
        )}
        {view === 'Settings' && (
          <Settings settings={eng.settings} identifyVram={identifyVram} onSaved={handleSettingsSaved} run={run} toast={toast} jobs={eng.jobs} models={eng.models} onModelsChanged={eng.refreshModels} status={eng.status}
            onOpenWhatsNew={whatsNew.openManually} onOpenWizard={wizard.openManually} openTab={settingsTab} dev={devInfo} />
        )}
      </main>
      <ImportDialog draft={draft} setDraft={setDraft} profile={profile} setProfile={setProfile} settings={eng.settings} status={status} busy={starting}
        onStart={(args, defaults) => void startScan(args, defaults)} visible={showImport} onClose={() => setShowImport(false)} />
      {help && <KeysDialog onClose={() => setHelp(false)} />}
      {dropping && <div className="drop-overlay"><Icon name="drop" size={34} /><b>Drop a folder to scan it</b><span>Photos, or a folder of them</span></div>}
      <SetupWizard visible={wizard.open} settings={eng.settings} toast={toast} onClose={wizard.close}
        onSaved={(saved) => { handleSettingsSaved(saved); if (typeof saved.scan_profile === 'string') setProfile(saved.scan_profile); }}
        onOpenSettings={() => setPage('Settings')} />
      <WhatsNewSheet visible={whatsNew.open && wizard.decided && !wizard.open} fresh={whatsNew.fresh} earlier={whatsNew.earlier} onClose={whatsNew.close} />
      <Splash gone={eng.ready && startup.settled} status={startup.status} version={appVersion ?? (import.meta.env.DEV && mocked ? '0.0.0-mock' : null)} dev={devInfo} />
    </div>
  );
}
