import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow, UserAttentionType } from '@tauri-apps/api/window';
import { useCallback, useEffect, useRef, useState } from 'react';
import { call, engine, inTauri } from '../lib/api';
import { clock, applySnapshot } from '../lib/snapshotOrder.mjs';
import { classifyScanError } from '../lib/scanError.mjs';
import type { IdentifyVram, Job, ModelInfo, Settings, Status } from '../lib/types';
import type { Runner } from './hooks';
import { useToast } from './toast';

const emptyStatus: Status = { tasks: [], log: [], activeJob: null, operations: [] };

type Stamp = { at: number; source: string };

/**
 * Bootstrap plus the engine's pushed `status` events. Jobs are re-read while anything runs; the open
 * album's review is re-read on every change when only an operation runs, and every 4 s while a scan writes into it.
 * Also raises the toast when a scan ends.
 *
 * Every `status` / `jobs` write goes through the ordering guard (snapshotOrder.mjs): a slow poll
 * that resolves after a newer pushed event can NEVER overwrite it (the false "Scan complete" toast
 * and the reordered-jobs bug). All streams share one monotonic clock, so `at` is request/emit time
 * and a backward wall-clock step can never freeze the state.
 */
export function useEngine(jobId: number | null, refreshReview: () => Promise<void>, run: Runner) {
  const toast = useToast();
  const [ready, setReady] = useState(false);
  const [jobs, setJobs] = useState<Job[]>([]);
  const [settings, setSettings] = useState<Settings>({});
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [identifyVram, setIdentifyVram] = useState<IdentifyVram | null>(null);
  const [status, setStatusRaw] = useState<Status>(emptyStatus);
  const [dev, setDev] = useState(false);
  const [devUrl, setDevUrl] = useState('');
  const [devServer, setDevServer] = useState(false);
  const [devHome, setDevHome] = useState('');
  // Last applied status/jobs stamps, shared by bootstrap, the poll, pushes and command follow-ups.
  const appliedRef = useRef<Stamp | null>(null);
  const jobsAppliedRef = useRef<Stamp | null>(null);
  const latest = useRef({ jobId, refreshReview, jobs });
  useEffect(() => { latest.current = { jobId, refreshReview, jobs }; });

  /** The guarded status writer. `applySnapshot` returns the incoming stamp only when it wins. */
  const commit = useCallback((s: Status, stamp: Stamp) => {
    if (applySnapshot(appliedRef.current, stamp) !== stamp) return false;
    appliedRef.current = stamp;
    setStatusRaw(s);
    return true;
  }, []);

  // The setter App uses after a command (start/stop/unqueue): a fresh read, still guarded so a
  // push that arrived during the command can never be clobbered.
  const setStatus = useCallback((s: Status) => { commit(s, { at: clock.now(), source: 'live' }); }, [commit]);

  const refreshJobs = useCallback(async () => {
    const stamp: Stamp = { at: clock.now(), source: 'live' };
    const next = await engine.jobs();
    if (applySnapshot(jobsAppliedRef.current, stamp) === stamp) {
      jobsAppliedRef.current = stamp;
      setJobs(next);
    }
  }, []);
  // `health` = bootstrap's local models plus one live row per VLM server (with its id).
  const refreshModels = useCallback(async () => setModels(await call<ModelInfo[]>('health')), []);
  const refreshIdentifyVram = useCallback(async () => {
    const ms = await call<{ identifyVram?: IdentifyVram | null }>('models_status');
    if (ms && 'identifyVram' in ms) setIdentifyVram(ms.identifyVram ?? null);
  }, []);

  useEffect(() => {
    let live = true;
    void run(async () => {
      const b = await engine.bootstrap();
      if (!live) return;
      const stamp: Stamp = { at: clock.now(), source: 'live' };
      jobsAppliedRef.current = stamp;
      setJobs(b.jobs); setSettings(b.settings); setModels(b.models); commit(b.status, stamp);
      setIdentifyVram(b.identifyVram ?? (b.settings as unknown as { identifyVram?: IdentifyVram | null })?.identifyVram ?? null);
      setDev(Boolean(b.dev)); setDevUrl(b.devUrl ?? ''); setDevServer(Boolean(b.devServer)); setDevHome(b.devHome ?? '');
      void refreshModels().catch(() => undefined);
    }).finally(() => { if (live) setReady(true); });
    return () => { live = false; };
  }, [run, refreshModels, commit]);

  useEffect(() => {
    let alive = true;
    let lastActive: number | null = null;
    let lastReview = 0;
    let revision: number | undefined;
    let lastError = '';
    // Jobs / review re-reads, done ONLY for a snapshot that won the ordering guard.
    const sideEffects = async (s: Status) => {
      const changed = s.revision !== undefined && s.revision !== revision;
      revision = s.revision;
      if (s.activeJob || s.operations.length || lastActive || changed) {
        const jStamp: Stamp = { at: clock.now(), source: 'poll' };
        const nextJobs = await engine.jobs();
        if (applySnapshot(jobsAppliedRef.current, jStamp) === jStamp && alive) {
          jobsAppliedRef.current = jStamp;
          setJobs(nextJobs);
        }
        if (alive && latest.current.jobId && (!s.activeJob || Date.now() - lastReview > 4000)) {
          lastReview = Date.now();
          await latest.current.refreshReview();
        }
      }
      if (!s.activeJob && !s.operations.length && lastActive) { const m = await call<ModelInfo[]>('health'); if (alive) setModels(m); }
      lastActive = s.activeJob ?? (s.operations.length ? -1 : null);
    };
    const update = async (pushed?: Status) => {
      // Stamp at REQUEST/EMIT time, before the await: a poll that is in flight while a
      // newer push arrives keeps its older stamp and is rejected when it resolves.
      const stamp: Stamp = { at: clock.now(), source: pushed ? 'live' : 'poll' };
      let s: Status;
      try {
        s = pushed ?? (await engine.status());
      } catch (e) {
        if (alive && String(e) !== lastError) { lastError = String(e); toast(String(e), { tone: 'error' }); }
        return;
      }
      if (!commit(s, stamp)) return; // a stale poll must not overwrite a newer pushed event
      lastError = '';
      await sideEffects(s);
    };
    // The shell pushes `status` whenever a task changes; the slow poll is only a safety net
    // (and how the browser mock, which has no shell, keeps working).
    let unlisten: (() => void) | undefined;
    listen<Status>('status', (e) => void update(e.payload))
      .then((off) => { if (alive) unlisten = off; else off(); })
      .catch(() => undefined);
    const timer = window.setInterval(() => void update(), 5000);
    return () => { alive = false; clearInterval(timer); unlisten?.(); };
  }, [toast, commit]);

  // A scan that stops being the active job is finished, failed or stopped: say which.
  const wasActive = useRef<number | null>(null);
  useEffect(() => {
    const was = wasActive.current;
    wasActive.current = status.activeJob;
    if (was == null || status.activeJob != null) return;
    const name = latest.current.jobs.find((j) => j.id === was)?.label;
    const last = status.tasks.reduce<Status['tasks'][number] | null>((a, t) => (!a || t.id > a.id ? t : a), null);
    if (last?.state === 'cancelled' || (last?.state === 'failed' && last.error === 'stopped')) toast('Scan stopped. You can resume it later.');
    else if (last?.state === 'failed') {
      const words = classifyScanError(last.error ?? '', { label: name ?? '' });
      toast(`${words.title}. Open Ingest for the reason and a way to resume.`, { tone: 'error' });
    } else if (status.scan?.job === was && status.scan.errors > 0) {
      const n = status.scan.errors;
      toast(`${name ?? 'Scan'} finished with ${n.toLocaleString()} ${n === 1 ? 'photo' : 'photos'} that could not be read. ${status.scan.error_samples[0] ?? ''}`.trim(), { tone: 'error' });
    } else toast(name ? `Scan complete: ${name}` : 'Scan complete', { tone: 'ok' });
    if (inTauri) void getCurrentWindow().requestUserAttention(last?.state === 'failed' ? UserAttentionType.Critical : UserAttentionType.Informational);
  }, [status, toast]);

  return { ready, jobs, setJobs, settings, setSettings, models, identifyVram, refreshIdentifyVram, status, setStatus, refreshJobs, refreshModels, dev, devUrl, devServer, devHome };
}
export type EngineModel = ReturnType<typeof useEngine>;
