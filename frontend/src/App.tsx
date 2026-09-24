import { useCallback, useEffect, useRef, useState } from 'react';
import { getCurrentWindow, UserAttentionType } from '@tauri-apps/api/window';
import { Icon, KeysDialog, Splash } from './components/basics';
import { TitleBar, type StatusActions } from './components/TitleBar';
import { call, engine, inTauri, mocked } from './lib/api';
import { filename } from './lib/review';
import { subjectExcluded } from './review.mjs';
import type { DevStart } from './lib/mock';
import type { Page, ScanArgs, Status } from './lib/types';
import { Library } from './screens/Library';
import { ReviewScreen } from './screens/Review';
import { ImportDialog, type ScanDraft } from './screens/Scan';
import { Settings } from './screens/Settings';
import { noTrainVerbs, Train, type TrainVerbs } from './screens/Train';
import { Viewer } from './screens/Viewer';
import { useFolderDrop, useLocalFlag, useRun } from './state/hooks';
import { useToast } from './state/toast';
import { useEngine } from './state/useEngine';
import { useHotkeys } from './state/useHotkeys';
import { useReview } from './state/useReview';

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
  const [viewer, setViewer] = useState(false);
  const [help, setHelp] = useState(false);
  const [showBoxes, setShowBoxes] = useLocalFlag('conrod.boxes', true);
  const [dev, setDev] = useState<DevStart | null>(null);
  const trainVerbs = useRef<TrainVerbs>(noTrainVerbs);

  const rv = useReview(jobId, run);
  const eng = useEngine(jobId, rv.refresh, run);
  const { status } = eng;
  const scanning = status.activeJob !== null;

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
    if (dev?.viewer && rv.frames.length) { setViewer(true); setDev({ ...dev, viewer: false }); }
  }, [dev, rv.frames.length]);

  const dropping = useFolderDrop((folder) => {
    setDraft({ root: folder, label: filename(folder) });
    setShowImport(true);
    toast(`Ready to import ${filename(folder)}. Choose options and press Import.`);
  });

  const startScan = async (args: ScanArgs) => {
    setStarting(true);
    await run(async () => {
      const { jobId: id } = await engine.scan(args);
      await eng.refreshJobs();
      setJobId(id);
      eng.setStatus(await engine.status());
      setShowImport(false);
      setPage('Review');
      toast('stage' in args && args.stage === 'index' ? 'Imported. Review your photos and reject unwanted frames before running ML.' : 'Subject rating started. Review suggested culls before identification.', { tone: 'ok' });
      if (inTauri) void getCurrentWindow().requestUserAttention(UserAttentionType.Informational);
    });
    setStarting(false);
  };
  const identify = (id: number) => void run(async () => { await engine.identify(id); toast('Identification started. Progress is in the activity menu.'); });
  const write = (id: number, dryRun: boolean) => void run(async () => {
    await engine.write(id, dryRun);
    toast(dryRun ? 'Checking what would be written. The activity menu lists it and no file is touched.' : 'Writing started. Progress is in the activity menu.');
  });
  const openJob = (id: number) => { setJobId(id); setPage('Review'); };

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
    page, viewerOpen: viewer, rejected: Boolean(rv.frame && rv.facts.get(rv.frame.id)?.cull),
    step: (d) => (page === 'Train' ? trainVerbs.current.step(d) : rv.step(d)),
    mark: (values) => void rv.mark(values),
    openViewer: () => { if (rv.frame) setViewer(true); },
    closeOverlays: () => { setViewer(false); setHelp(false); setShowImport(false); },
    toggleHelp: () => setHelp((v) => !v),
    toggleBoxes: () => setShowBoxes((v) => !v),
    train: { rate: (n) => trainVerbs.current.rate(n), pan: () => trainVerbs.current.pan(), undo: () => trainVerbs.current.undo(), zoom: () => trainVerbs.current.zoom() },
  });

  return (
    <div className="app">
      <TitleBar page={page} setPage={setPage} jobs={eng.jobs} models={eng.models} status={status} profile={profile} actions={actions} popoverOpen={dev?.popover ?? false} />
      <main className="stage" key={page}>
        {page === 'Library' && (
          <Library jobs={eng.jobs} models={eng.models} scanning={scanning} onOpen={(j) => openJob(j.id)} onNewScan={() => setShowImport(true)}
            onIdentify={(j) => identify(j.id)} onResume={(j) => void startScan({ jobId: j.id })}
            onDelete={(j) => void run(async () => {
              await engine.deleteJob(j.id);
              await eng.refreshJobs();
              if (jobId === j.id) setJobId(null);
              toast('Album deleted');
            })} />
        )}
        {page === 'Review' && (
          <ReviewScreen rv={rv} jobs={eng.jobs} jobId={jobId} scanning={status.activeJob === jobId && jobId !== null} status={status} settings={eng.settings}
            onPickJob={setJobId} onCull={() => jobId != null && void startScan({ jobId })} onIdentify={() => jobId != null && identify(jobId)} onHelp={() => setHelp(true)} onOpenViewer={() => setViewer(true)}
              onLibrary={() => setPage('Library')}
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
        {page === 'Train' && (
          <Train rv={rv} jobs={eng.jobs} jobId={jobId} run={run} toast={toast} showBoxes={showBoxes} verbs={trainVerbs} onPickJob={setJobId}
            onLibrary={() => setPage('Library')} />
        )}
        {page === 'Settings' && <Settings settings={eng.settings} onSaved={eng.setSettings} run={run} toast={toast} jobs={eng.jobs} models={eng.models} />}
      </main>
      <ImportDialog draft={draft} setDraft={setDraft} profile={profile} setProfile={setProfile} models={eng.models} settings={eng.settings} status={status} busy={starting}
        actions={actions} onStart={(args) => void startScan(args)} onReviewActive={() => { if (status.activeJob != null) openJob(status.activeJob); }}
        visible={showImport} onClose={() => setShowImport(false)} />
      {viewer && page === 'Review' && <Viewer rv={rv} run={run} showBoxes={showBoxes} onToggleBoxes={() => setShowBoxes((v) => !v)} onClose={() => setViewer(false)} />}
      {help && <KeysDialog onClose={() => setHelp(false)} />}
      {dropping && <div className="drop-overlay"><Icon name="drop" size={34} /><b>Drop a folder to scan it</b><span>Photos, or a folder of them</span></div>}
      <Splash gone={eng.ready} />
    </div>
  );
}
