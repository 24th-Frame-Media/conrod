import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow, UserAttentionType } from '@tauri-apps/api/window';
import { useCallback, useEffect, useRef, useState } from 'react';
import { call, engine, inTauri } from '../lib/api';
import type { Job, ModelInfo, Settings, Status } from '../lib/types';
import type { Runner } from './hooks';
import { useToast } from './toast';

const emptyStatus: Status = { tasks: [], log: [], activeJob: null, operations: [] };

/**
 * Bootstrap plus the engine's pushed `status` events. Jobs are re-read while anything runs; the open
 * album's review is re-read on every change when only an operation runs, and every 4 s while a scan writes into it.
 * Also raises the toast when a scan ends.
 */
export function useEngine(jobId: number | null, refreshReview: () => Promise<void>, run: Runner) {
  const toast = useToast();
  const [ready, setReady] = useState(false);
  const [jobs, setJobs] = useState<Job[]>([]);
  const [settings, setSettings] = useState<Settings>({});
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [status, setStatus] = useState<Status>(emptyStatus);
  const latest = useRef({ jobId, refreshReview, jobs });
  useEffect(() => { latest.current = { jobId, refreshReview, jobs }; });

  const refreshJobs = useCallback(async () => setJobs(await engine.jobs()), []);
  // `health` = bootstrap's local models plus one live row per VLM server (with its id).
  const refreshModels = useCallback(async () => setModels(await call<ModelInfo[]>('health')), []);

  useEffect(() => {
    let live = true;
    void run(async () => {
      const b = await engine.bootstrap();
      if (!live) return;
      setJobs(b.jobs); setSettings(b.settings); setModels(b.models); setStatus(b.status);
      void refreshModels().catch(() => undefined);
    }).finally(() => { if (live) setReady(true); });
    return () => { live = false; };
  }, [run, refreshModels]);

  useEffect(() => {
    let live = true;
    let lastActive: number | null = null;
    let lastReview = 0;
    let revision: number | undefined;
    let lastError = '';
    const apply = async (s: Status) => {
      if (!live) return;
      setStatus(s);
      lastError = '';
      const changed = s.revision !== undefined && s.revision !== revision;
      revision = s.revision;
      if (s.activeJob || s.operations.length || lastActive || changed) {
        setJobs(await engine.jobs());
        if (latest.current.jobId && (!s.activeJob || Date.now() - lastReview > 4000)) {
          lastReview = Date.now();
          await latest.current.refreshReview();
        }
      }
      if (!s.activeJob && !s.operations.length && lastActive) { const m = await call<ModelInfo[]>('health'); if(live) setModels(m); }
      lastActive = s.activeJob ?? (s.operations.length ? -1 : null);
    };
    const update = async (pushed?: Status) => {
      try {
        await apply(pushed ?? (await engine.status()));
      } catch (e) {
        if (live && String(e) !== lastError) { lastError = String(e); toast(lastError, { tone: 'error' }); }
      }
    };
    // The shell pushes `status` whenever a task changes; the slow poll is only a safety net
    // (and how the browser mock, which has no shell, keeps working).
    let unlisten: (() => void) | undefined;
    listen<Status>('status', (e) => void update(e.payload))
      .then((off) => { if (live) unlisten = off; else off(); })
      .catch(() => undefined);
    const timer = window.setInterval(() => void update(), 5000);
    return () => { live = false; clearInterval(timer); unlisten?.(); };
  }, [toast]);

  // A scan that stops being the active job is finished, failed or stopped: say which.
  const wasActive = useRef<number | null>(null);
  useEffect(() => {
    const was = wasActive.current;
    wasActive.current = status.activeJob;
    if (was == null || status.activeJob != null) return;
    const name = latest.current.jobs.find((j) => j.id === was)?.label;
    const last = status.tasks.reduce<Status['tasks'][number] | null>((a, t) => (!a || t.id > a.id ? t : a), null);
    if (last?.state === 'failed' && last.error === 'stopped') toast('Scan stopped. You can resume it later.');
    else if (last?.state === 'failed') toast(`Scan failed: ${last.error ?? 'unknown error'}`, { tone: 'error' });
    else toast(name ? `Scan complete: ${name}` : 'Scan complete', { tone: 'ok' });
    if (inTauri) void getCurrentWindow().requestUserAttention(last?.state === 'failed' ? UserAttentionType.Critical : UserAttentionType.Informational);
  }, [status, toast]);

  return { ready, jobs, setJobs, settings, setSettings, models, status, setStatus, refreshJobs, refreshModels };
}
export type EngineModel = ReturnType<typeof useEngine>;
