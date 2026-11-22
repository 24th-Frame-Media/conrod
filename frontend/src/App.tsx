import { useCallback, useEffect, useRef, useState } from 'react';
import { getCurrentWindow, UserAttentionType } from '@tauri-apps/api/window';
import { Icon, KeysDialog, Splash } from './components/basics';
import { TitleBar, type StatusActions } from './components/TitleBar';
import { SetupWizard } from './components/SetupWizard';
import { WhatsNewSheet } from './components/WhatsNewSheet';
import { call, engine, inTauri, mocked } from './lib/api';
import { filename } from './lib/review';
import { queueLabel } from './lib/format';
import { healthStatus, modelGroups } from './lib/health.mjs';
import { classifyScanError, type ScanFailure } from './lib/scanError.mjs';
import { subjectExcluded } from './review.mjs';
import { vlmModelLabel } from './lib/vlm';
import type { DevStart } from './lib/mock';
import type { Job, Page, ScanArgs, Scope, Status } from './lib/types';
import type { SettingsTabId } from './lib/settings.mjs';
import type { VisionAction } from './lib/vision.mjs';
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
  const startup = useStartup();
  const appVersion = useAppVersion();
  const wizard = useWizard({ ready: eng.ready, settings: eng.settings, albums: eng.jobs.length, markSeen: whatsNew.markSeen });
  const { status } = eng;
  const scanning = status.activeJob !== null;
  const scanLive = useScanProgress();
  const identifyLive = useIdentifyProgress();
  const identifyPaused = status.tasks.some((task) => task.state === 'paused' && /^Identifying\b/.test(task.label));
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
    try {
      const { jobId: id, duplicates = 0, added = 0, scanning: started = true, queued = false, queuePosition = 0 } = await engine.scan(scan);
      await eng.refreshJobs();
      if (id != null) setJobId(id);
      eng.setStatus(await engine.status());
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
    } catch (e) {
      // A scan that never started gets one plain reason on the Ingest screen, not a raw toast.
      const jobId = 'jobId' in scan ? scan.jobId : null;
      const root = 'root' in scan ? scan.root : draft.root;
      const label = 'label' in scan ? scan.label : (jobId != null ? eng.jobs.find((j) => j.id === jobId)?.label ?? draft.label : draft.label);
      setScanError({ ...classifyScanError(String(e), { root, label }), jobId: jobId ?? undefined, label, raw: String(e), args: scan });
      if (jobId != null) setJobId(jobId);
      setPage('Scan');
    } finally {
      setStarting(false);
    }
  };
  /** `scope` is the run's override; absent, the engine reads the album's own `identify_scope`. `onlyNew` reads just the subjects nobody has read yet. */
  const identify = (id: number, scope?: Scope, onlyNew = false) => void run(async () => {
    setJobId(id);
    setPage('Identify');
    await engine.identify(id, scope, onlyNew);
    toast('Identification started. The live trace is open.');
  });
  const write = (id: number, dryRun: boolean) => void run(async () => {
    await engine.write(id, dryRun);
    toast(dryRun ? 'Checking what would be written. The activity menu lists it and no file is touched.' : 'Writing started. Progress is in the activity menu.');
  });
  const openJob = (id: number) => { setJobId(id); setPage(status.activeJob === id ? 'Scan' : 'Review'); };

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
      <TitleBar page={view} setPage={goPage} jobs={eng.jobs} models={eng.models} status={status} profile={profile} health={health} actions={actions} popoverOpen={dev?.popover ?? false} />
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
          <ScanScreen job={eng.jobs.find((j) => j.id === jobId)} live={jobId != null ? scanLive[jobId] : undefined} running={running}
            queue={queueJobs} onUnqueue={actions.unqueue}
            paused={status.tasks.some((t) => t.state === 'paused' && !/^Identifying\b/.test(t.label))} run={run} onSettingsSaved={eng.setSettings}
            identify={jobId != null ? identifyLive[jobId] : undefined} status={status} identifyPaused={identifyPaused}
            model={vlmModelLabel(eng.settings)} scanError={scanError}
            onRetryScan={() => { if (scanError?.args) void startScan(scanError.args); else if (jobId != null) void startScan({ jobId }); else actions.resume(); }}
            onPause={actions.pause} onResume={actions.resume} onCancel={actions.stop} onCancelOperation={actions.cancel} onVisionAction={visionAction}
            onReview={() => setPage('Review')} onLibrary={() => setPage('Library')} />
        )}
        {view === 'Identify' && (
          <IdentifyScreen job={eng.jobs.find((j) => j.id === jobId)} live={jobId != null ? identifyLive[jobId] : undefined}
            status={status} model={vlmModelLabel(eng.settings)}
            paused={identifyPaused} onCancel={actions.cancel} onVisionAction={visionAction} onReview={() => setPage('Review')} />
        )}
        {view === 'Review' && (
          <ReviewScreen rv={rv} jobs={eng.jobs} jobId={jobId} scanning={status.activeJob === jobId && jobId !== null} status={status} settings={eng.settings}
            showBoxes={showBoxes} onToggleBoxes={() => setShowBoxes((v) => !v)} verbs={reviewVerbs}
            onJobsChanged={eng.refreshJobs} onPickJob={setJobId} onCull={() => jobId != null && void startScan({ jobId })} onIdentify={(scope, onlyNew) => jobId != null && identify(jobId, scope, onlyNew)} onHelp={() => setHelp(true)} onOpenViewer={() => setViewer(true)}
              onLibrary={() => setPage('Library')} onWatchIdentify={() => jobId != null && setPage('Identify')} onWatchScan={() => setPage('Scan')}
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
          <Settings settings={eng.settings} onSaved={eng.setSettings} run={run} toast={toast} jobs={eng.jobs} models={eng.models} onModelsChanged={eng.refreshModels} status={eng.status}
            onOpenWhatsNew={whatsNew.openManually} onOpenWizard={wizard.openManually} openTab={settingsTab} />
        )}
      </main>
      <ImportDialog draft={draft} setDraft={setDraft} profile={profile} setProfile={setProfile} settings={eng.settings} status={status} busy={starting}
        onStart={(args, defaults) => void startScan(args, defaults)} visible={showImport} onClose={() => setShowImport(false)} />
      {help && <KeysDialog onClose={() => setHelp(false)} />}
      {dropping && <div className="drop-overlay"><Icon name="drop" size={34} /><b>Drop a folder to scan it</b><span>Photos, or a folder of them</span></div>}
      <SetupWizard visible={wizard.open} settings={eng.settings} toast={toast} onClose={wizard.close}
        onSaved={(saved) => { eng.setSettings(saved); if (typeof saved.scan_profile === 'string') setProfile(saved.scan_profile); }}
        onOpenSettings={() => setPage('Settings')} />
      <WhatsNewSheet visible={whatsNew.open && wizard.decided && !wizard.open} fresh={whatsNew.fresh} earlier={whatsNew.earlier} onClose={whatsNew.close} />
      <Splash gone={eng.ready && startup.settled} status={startup.status} version={appVersion ?? (import.meta.env.DEV && mocked ? '0.0.0-mock' : null)} />
    </div>
  );
}
