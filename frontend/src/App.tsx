import { useCallback, useEffect, useRef, useState } from 'react';
import { getCurrentWindow, UserAttentionType } from '@tauri-apps/api/window';
import { Icon, KeysDialog, Splash } from './components/basics';
import { TitleBar, type StatusActions } from './components/TitleBar';
import { call, engine, inTauri, mocked } from './lib/api';
import { filename } from './lib/review';
import { subjectExcluded } from './review.mjs';
import type { DevStart } from './lib/mock';
import type { Page, ScanArgs, Scope, Status } from './lib/types';
import { Library } from './screens/Library';
import { noReviewVerbs, ReviewScreen, type ReviewVerbs } from './screens/Review';
import { ImportDialog, type ImportDefaults, type ScanDraft } from './screens/Import';
import { IdentifyScreen } from './screens/Identify';
import { ScanScreen } from './screens/Scan';
import { Settings } from './screens/Settings';
import { noTrainVerbs, Train, type TrainVerbs } from './screens/Train';
import { useFolderDrop, useLocalFlag, useRun } from './state/hooks';
import { useToast } from './state/toast';
import { useEngine } from './state/useEngine';
import { useHotkeys } from './state/useHotkeys';
import { useIdentifyProgress } from './state/useIdentifyProgress';
import { useReview } from './state/useReview';
import { useScanProgress } from './state/useScanProgress';

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
  // Best-effort mirror of Review's loupe/compare state, only for the Enter-key open-guard and dev-start below;
  // Review.tsx owns the real `view` state and App reaches it through `reviewVerbs` (see useHotkeys wiring).
  const [viewer, setViewer] = useState(false);
  const [help, setHelp] = useState(false);
  const [showBoxes, setShowBoxes] = useLocalFlag('conrod.boxes', true);
  const [dev, setDev] = useState<DevStart | null>(null);
  const trainVerbs = useRef<TrainVerbs>(noTrainVerbs);
  const reviewVerbs = useRef<ReviewVerbs>(noReviewVerbs);

  const rv = useReview(jobId, run);
  const eng = useEngine(jobId, rv.refresh, run);
  const { status } = eng;
  const scanning = status.activeJob !== null;
  const scanLive = useScanProgress();
  const identifyLive = useIdentifyProgress();
  const identifyPaused = status.tasks.some((task) => task.state === 'paused' && /^Identifying\b/.test(task.label));
  // Review is locked for an album until its scan finishes: asking for it shows the scan instead.
  // The scan's own final snapshot unlocks it a beat before the pushed status catches up.
  const locked = jobId != null && status.activeJob === jobId && (scanLive[jobId]?.progress.state ?? 'running') === 'running';
  const view: Page = page === 'Review' && locked ? 'Scan' : page;
  const goPage = (next: Page) => {
    if (next === 'Scan' && status.activeJob != null) setJobId(status.activeJob);
    if (next === 'Identify') {
      // The tab is live for whichever album is identifying: open that album, not the last one browsed.
      const key = status.operations.find((k) => k === `Identifying:${jobId}`) ?? status.operations.find((k) => k.startsWith('Identifying:'));
      const id = key ? Number(key.slice('Identifying:'.length)) : NaN;
      if (Number.isFinite(id)) setJobId(id);
    }
    setPage(next === 'Review' && locked ? 'Scan' : next);
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
    await run(async () => {
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
      const { jobId: id } = await engine.scan(scan);
      await eng.refreshJobs();
      setJobId(id);
      eng.setStatus(await engine.status());
      setShowImport(false);
      setPage('Scan');
      if (inTauri) void getCurrentWindow().requestUserAttention(UserAttentionType.Informational);
    });
    setStarting(false);
  };
  /** `scope` is the run's override; absent, the engine reads the album's own `identify_scope`. */
  const identify = (id: number, scope?: Scope) => void run(async () => {
    setJobId(id);
    setPage('Identify');
    await engine.identify(id, scope);
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
  };

  useHotkeys({
    page: view, viewerOpen: viewer, rejected: () => reviewVerbs.current.rejected(), picked: () => reviewVerbs.current.picked(),
    stepFrame: (d, repeat) => (page === 'Train' ? trainVerbs.current.step(d) : reviewVerbs.current.stepFrame(d, repeat)),
    stepBurst: (d) => (page === 'Train' ? trainVerbs.current.step(d) : reviewVerbs.current.stepBurst(d)),
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
    toggleBoxes: () => setShowBoxes((v) => !v),
    setView: (v) => { setViewer(v === 'loupe'); reviewVerbs.current.setView(v); },
    train: { rate: (n) => trainVerbs.current.rate(n), pan: () => trainVerbs.current.pan(), undo: () => trainVerbs.current.undo(), zoom: () => trainVerbs.current.zoom() },
  });

  return (
    <div className="app">
      <TitleBar page={view} setPage={goPage} jobs={eng.jobs} models={eng.models} status={status} profile={profile} actions={actions} popoverOpen={dev?.popover ?? false} />
      <main className="stage" key={view}>
        {view === 'Library' && (
          <Library jobs={eng.jobs} models={eng.models} scanning={scanning} live={scanLive} activeJob={status.activeJob} onOpen={(j) => openJob(j.id)} onNewScan={() => setShowImport(true)}
            onIdentify={(j) => identify(j.id)} onResume={(j) => void startScan({ jobId: j.id })}
            onDelete={(j) => void run(async () => {
              await engine.deleteJob(j.id);
              await eng.refreshJobs();
              if (jobId === j.id) setJobId(null);
              toast('Album deleted');
            })}
            onToggleIncludePeople={(j) => void run(async () => {
              const result = await engine.setIncludePeople(j.id, !j.include_people);
              await eng.refreshJobs();
              toast(result.ok ? 'Album settings updated' : 'Could not update album settings', { tone: result.ok ? 'ok' : 'error' });
            })} />
        )}
        {view === 'Scan' && (
          <ScanScreen job={eng.jobs.find((j) => j.id === jobId)} live={jobId != null ? scanLive[jobId] : undefined} running={locked}
            paused={status.tasks.some((t) => t.state === 'paused' && !/^Identifying\b/.test(t.label))} run={run} onSettingsSaved={eng.setSettings}
            identify={jobId != null ? identifyLive[jobId] : undefined} status={status} identifyPaused={identifyPaused}
            model={typeof eng.settings.vlm_model === 'string' ? eng.settings.vlm_model : undefined}
            onPause={actions.pause} onResume={actions.resume} onCancel={actions.stop} onCancelOperation={actions.cancel}
            onReview={() => setPage('Review')} onLibrary={() => setPage('Library')} />
        )}
        {view === 'Identify' && (
          <IdentifyScreen job={eng.jobs.find((j) => j.id === jobId)} live={jobId != null ? identifyLive[jobId] : undefined}
            status={status} model={typeof eng.settings.vlm_model === 'string' ? eng.settings.vlm_model : undefined}
            paused={identifyPaused} onCancel={actions.cancel} onReview={() => setPage('Review')} />
        )}
        {view === 'Review' && (
          <ReviewScreen rv={rv} jobs={eng.jobs} jobId={jobId} scanning={status.activeJob === jobId && jobId !== null} status={status} settings={eng.settings}
            showBoxes={showBoxes} onToggleBoxes={() => setShowBoxes((v) => !v)} verbs={reviewVerbs}
            onJobsChanged={eng.refreshJobs} onPickJob={setJobId} onCull={() => jobId != null && void startScan({ jobId })} onIdentify={(scope) => jobId != null && identify(jobId, scope)} onHelp={() => setHelp(true)} onOpenViewer={() => setViewer(true)}
              onLibrary={() => setPage('Library')} onWatchIdentify={() => jobId != null && setPage('Identify')}
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
              onWrite={(dryRun) => jobId != null && write(jobId, dryRun)}
              onEdit={(id, updates) => run(async () => { await engine.editDetection(id, updates); await rv.refresh(); })} />
        )}
        {view === 'Train' && (
          <Train rv={rv} jobs={eng.jobs} jobId={jobId} run={run} toast={toast} showBoxes={showBoxes} verbs={trainVerbs} onPickJob={setJobId}
            onLibrary={() => setPage('Library')} />
        )}
        {view === 'Settings' && <Settings settings={eng.settings} onSaved={eng.setSettings} run={run} toast={toast} jobs={eng.jobs} models={eng.models} onModelsChanged={eng.refreshModels} status={eng.status} />}
      </main>
      <ImportDialog draft={draft} setDraft={setDraft} profile={profile} setProfile={setProfile} settings={eng.settings} status={status} busy={starting}
        onStart={(args, defaults) => void startScan(args, defaults)} visible={showImport} onClose={() => setShowImport(false)} />
      {help && <KeysDialog onClose={() => setHelp(false)} />}
      {dropping && <div className="drop-overlay"><Icon name="drop" size={34} /><b>Drop a folder to scan it</b><span>Photos, or a folder of them</span></div>}
      <Splash gone={eng.ready} />
    </div>
  );
}
