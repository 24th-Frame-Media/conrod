import { useCallback, useEffect, useRef, useState } from 'react';
import { Icon, KeysDialog, Splash } from './components/basics';
import { TitleBar, type StatusActions } from './components/TitleBar';
import { engine, mocked } from './lib/api';
import { filename } from './lib/review';
import type { DevStart } from './lib/mock';
import type { Page, ScanArgs } from './lib/types';
import { Known } from './screens/Known';
import { Library } from './screens/Library';
import { ReviewScreen } from './screens/Review';
import { Scan, type ScanDraft } from './screens/Scan';
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
    setPage('Scan');
    toast(`Ready to scan ${filename(folder)}. Choose a scan type and press Start.`);
  });

  const startScan = async (args: ScanArgs) => {
    setStarting(true);
    await run(async () => {
      const { jobId: id } = await engine.scan(args);
      await eng.refreshJobs();
      setJobId(id);
      eng.setStatus(await engine.status());
      setPage('Review');
      toast('Scan started. Photographs appear as they are measured.', { tone: 'ok' });
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
    stop: () => void run(async () => { await engine.stop(); toast('Stopping after the current frame. You can resume it later.'); }),
    cancel: (key) => void run(() => engine.cancelOperation(key)),
  };

  useHotkeys({
    page, viewerOpen: viewer, rejected: Boolean(rv.frame?.rejected),
    step: (d) => (page === 'Train' ? trainVerbs.current.step(d) : rv.step(d)),
    mark: (values) => void rv.mark(values),
    openViewer: () => { if (rv.frame) setViewer(true); },
    closeOverlays: () => { setViewer(false); setHelp(false); },
    toggleHelp: () => setHelp((v) => !v),
    toggleBoxes: () => setShowBoxes((v) => !v),
    train: { rate: (n) => trainVerbs.current.rate(n), pan: () => trainVerbs.current.pan(), undo: () => trainVerbs.current.undo(), zoom: () => trainVerbs.current.zoom() },
  });

  return (
    <div className="app">
      <TitleBar page={page} setPage={setPage} jobs={eng.jobs} status={status} profile={profile} actions={actions} popoverOpen={dev?.popover ?? false} />
      <main className="stage" key={page}>
        {page === 'Library' && (
          <Library jobs={eng.jobs} models={eng.models} scanning={scanning} onOpen={(j) => openJob(j.id)} onNewScan={() => setPage('Scan')}
            onIdentify={(j) => identify(j.id)} onResume={(j) => void startScan({ jobId: j.id })}
            onDelete={(j) => void run(async () => {
              await engine.deleteJob(j.id);
              await eng.refreshJobs();
              if (jobId === j.id) setJobId(null);
              toast('Album deleted');
            })} />
        )}
        {page === 'Scan' && (
          <Scan draft={draft} setDraft={setDraft} profile={profile} setProfile={setProfile} models={eng.models} status={status} busy={starting}
            actions={actions} onStart={(args) => void startScan(args)} onReviewActive={() => { if (status.activeJob != null) openJob(status.activeJob); }} />
        )}
        {page === 'Review' && (
          <ReviewScreen rv={rv} jobs={eng.jobs} jobId={jobId} scanning={status.activeJob === jobId && jobId !== null} scanRunning={scanning}
            onPickJob={setJobId} onIdentify={() => jobId != null && identify(jobId)} onWrite={(dryRun) => jobId != null && write(jobId, dryRun)}
            overwrites={(['overwrite_rating', 'overwrite_label'] as const).filter((k) => eng.settings[k] === true).map((k) => (k === 'overwrite_rating' ? 'ratings' : 'colour labels'))}
            onResume={() => jobId != null && void startScan({ jobId })} onHelp={() => setHelp(true)} onOpenViewer={() => setViewer(true)}
            onLibrary={() => setPage('Library')}
            onEdit={(id, updates) => run(async () => { await engine.editDetection(id, updates); await rv.refresh(); })} />
        )}
        {page === 'Train' && (
          <Train rv={rv} jobs={eng.jobs} jobId={jobId} run={run} toast={toast} showBoxes={showBoxes} verbs={trainVerbs} onPickJob={setJobId}
            onLibrary={() => setPage('Library')} />
        )}
        {page === 'Known vehicles' && <Known run={run} toast={toast} />}
        {page === 'Settings' && <Settings settings={eng.settings} onSaved={eng.setSettings} run={run} toast={toast} />}
      </main>
      {viewer && page === 'Review' && <Viewer rv={rv} run={run} showBoxes={showBoxes} onToggleBoxes={() => setShowBoxes((v) => !v)} onClose={() => setViewer(false)} />}
      {help && <KeysDialog onClose={() => setHelp(false)} />}
      {dropping && <div className="drop-overlay"><Icon name="drop" size={34} /><b>Drop a folder to scan it</b><span>Photos, or a folder of them</span></div>}
      <Splash gone={eng.ready} />
    </div>
  );
}
