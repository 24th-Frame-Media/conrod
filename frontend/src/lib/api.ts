import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import type {
  Bootstrap, GlobalStats, Job, MarkValues, Review, ScanArgs, Settings, Status, KnownVehicle,
} from './types';

/** The engine's answer to `scan`: the album it started (or found), and what a
 *  re-import did with the frames that were already there. */
export type ScanStarted = {
  jobId: number | null;
  /** Frames the import found already in the library; they are never imported twice. */
  duplicates?: number;
  /** Frames added by this import. */
  added?: number;
  /** True when the folder was already an album and this import joined it. */
  existing?: boolean;
  /** False when nothing was added, so no scan started. */
  scanning?: boolean;
  /** True when another scan was running: the album is created and waits its turn. */
  queued?: boolean;
  /** Its place in line, counting from 1 (1 = starts next). */
  queuePosition?: number;
};

/** `?mock=1` under `vite dev` only. The whole branch is dead code in production builds. */
export const mocked: boolean = import.meta.env.DEV && new URLSearchParams(location.search).has('mock');
export const inTauri = '__TAURI_INTERNALS__' in window;

export const call = async <T = unknown>(action: string, args: unknown = {}): Promise<T> => {
  if (import.meta.env.DEV && mocked) return (await import('./mock')).mockCall<T>(action, args);
  return invoke<T>('command', { action, args });
};

export const chooseFolder = async (): Promise<string | null> => {
  if (import.meta.env.DEV && mocked) return 'D:\\Photos\\2026\\09\\Bathurst 1000';
  return invoke<string | null>('choose_folder');
};

/** Local file path (or a data: URI from the mock) to something an <img> can load. */
export const asset = (path: string | null | undefined): string | undefined =>
  !path ? undefined : path.startsWith('data:') ? path : convertFileSrc(path);

export const engine = {
  bootstrap: () => call<Bootstrap>('bootstrap'),
  status: () => call<Status>('status'),
  jobs: () => call<Job[]>('jobs'),
  globalStats: () => call<GlobalStats>('global_stats'),
  review: (jobId: number) => call<Review>('review', { jobId }),
  scan: (args: ScanArgs) => call<ScanStarted>('scan', args),
  pause: () => call('pause'),
  resumeScan: () => call('resume_scan'),
  stop: () => call('stop'),
  /** Take an album out of the wait-for-its-turn line; it can be started by hand. */
  unqueue: (jobId: number) => call('unqueue', { jobId }),
  mark: (imageId: number, values: MarkValues) => call('mark', { imageId, ...values }),
  editDetection: (detectionId: number, updates: Record<string, string | string[] | null>) => call('edit_detection', { detectionId, ...updates }),
  preview: (imageId: number) => call<string>('preview', { imageId }),
  identify: (jobId: number, scope?: import('./types').Scope, onlyNew = false) =>
    call('identify', { jobId, ...(scope ? { scope } : {}), ...(onlyNew ? { onlyNew: true } : {}) }),
  /** Accept the album's readings as they stand, so Review opens without a model run. */
  skipIdentify: (jobId: number) => call<{ ok: boolean; skipped: number }>('skip_identify', { jobId }),
  identified: (jobId: number) => call<import('./types').Identified>('identified', { jobId }),
  acceptIdentifications: (ids: number[]) => call('bulk_edit', { ids, reviewed: true }),
  write: (jobId: number, dryRun = false) => call('write', { jobId, dryRun }),
  cancelOperation: (key: string) => call('cancel_operation', { key }),
  deciderStatus: () => call<import('./types').DeciderStatus>('decider_status'),
  installDecider: () => call('install_decider'),
  saveSettings: (settings: Settings) => call<Settings>('save_settings', settings),
  workerLimits: () => call<import('./types').WorkerLimits>('worker_limits'),
  known: () => call<KnownVehicle[]>('known'),
  saveKnown: (vehicle: KnownVehicle) => call('save_known', vehicle),
  deleteKnown: (plate: string) => call('delete_known', { plate }),
  deleteAllKnown: () => call<{ removed: number }>('delete_all_known'),
  deleteJob: (jobId: number) => call('delete_job', { jobId }),
  setIncludePeople: (jobId: number, value: boolean) => call<{ ok: boolean; jobId: number; settings: Settings; operation: string | null }>('set_include_people', { jobId, value }),
  /** `userDefaults`: an album's own copy of the photographer's defaults; absent, the saved profile's apply (the import sheet). */
  presetPolicy: (preset: string, overrides: Record<string, boolean> = {}, userDefaults?: Record<string, boolean>) =>
    call<import('./presetPolicy.mjs').PolicySummary>('preset_policy', { preset, overrides, ...(userDefaults ? { userDefaults } : {}) }),
  setFeature: (jobId: number, feature: string, value: boolean | null) =>
    call<{ ok: boolean; jobId: number; settings: Settings; operation: string | null; needsRescan: boolean; lanes: import('./presetPolicy.mjs').PolicySummary }>('set_feature', { jobId, feature, value }),
  updateJobSettings: (jobId: number, patch: Record<string, unknown>) => call('update_job_settings', { jobId, patch }),
  ollamaModels: (host?: string) => call<import('./types').OllamaModelsResponse>('ollama_models', { host }),
  providerModels: (provider: string, host?: string, apiKey?: string) =>
    call<import('./types').ProviderModelsResponse>('provider_models', { provider, host, apiKey }),
  openRouterCheck: (key: string) => call<import('./types').OpenRouterCheck>('openrouter_check', { key }),
};
