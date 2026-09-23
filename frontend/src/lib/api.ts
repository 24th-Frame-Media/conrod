import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import type {
  Bootstrap, Job, MarkValues, Region, Review, ScanArgs, Settings, Status, KnownVehicle, Training,
} from './types';

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
  review: (jobId: number) => call<Review>('review', { jobId }),
  scan: (args: ScanArgs) => call<{ jobId: number }>('scan', args),
  pause: () => call('pause'),
  resumeScan: () => call('resume_scan'),
  stop: () => call('stop'),
  mark: (imageId: number, values: MarkValues) => call('mark', { imageId, ...values }),
  editDetection: (detectionId: number, updates: Record<string, string | string[] | null>) => call('edit_detection', { detectionId, ...updates }),
  preview: (imageId: number) => call<string>('preview', { imageId }),
  identify: (jobId: number) => call('identify', { jobId }),
  write: (jobId: number, dryRun = false) => call('write', { jobId, dryRun }),
  cancelOperation: (key: string) => call('cancel_operation', { key }),
  saveSettings: (settings: Settings) => call<Settings>('save_settings', settings),
  known: () => call<KnownVehicle[]>('known'),
  saveKnown: (vehicle: KnownVehicle) => call('save_known', vehicle),
  deleteKnown: (plate: string) => call('delete_known', { plate }),
  deleteAllKnown: () => call<{ removed: number }>('delete_all_known'),
  trainingStatus: () => call<Training>('training_status'),
  trainLabel: (detectionId: number, stars: number, pan: boolean) => call('train_label', { detectionId, stars, pan }),
  undoLabel: () => call('undo_label'),
  trainModel: (region: Region) => call<Training>('train_model', { region }),
  forgetModel: (region: Region) => call<Training>('forget_model', { region }),
  trainTaste: () => call<Record<string, unknown>>('train_taste'),
  deleteJob: (jobId: number) => call('delete_job', { jobId }),
  updateJobSettings: (jobId: number, patch: Record<string, unknown>) => call('update_job_settings', { jobId, patch }),
};
