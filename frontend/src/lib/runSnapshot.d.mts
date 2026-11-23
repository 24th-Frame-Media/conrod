import type { DropdownJob, JobChip, PipelineWorker, RunSnapshot, StageFailure } from './types';

/** Generous last-report window before a worker is shown as stalled (epoch ms). */
export const STALL_MS: number;
export type WorkerLifecycle = 'working' | 'paused' | 'done' | 'failed' | 'stalled';
export function workerLifecycle(worker: any, now?: number, stallMs?: number): any;
/** Only active workers for the lane grid; done/failed leave, stalled is honest. */
export function activeWorkers(workers: any, options?: {
  now?: number; stallMs?: number; filter?: string | null; cap?: number; all?: boolean;
  paused?: boolean; identify?: boolean; runActive?: boolean;
}): { lanes: PipelineWorker[]; hidden: number; stalled: number };
export const RUN_WORDS: Readonly<Record<string, string>>;
export function runStateWord(state?: string | null): string;
export function toneFor(state?: string | null): string;
export function jobChip(job: any, context?: any): JobChip;
export function dropdownRows(jobs: any, context?: any): DropdownJob[];
export function scanSeedFor(seed: any, jobId?: number | null): any;
export function stageFailureReason(stage: any): string;
export function stageFailed(stage: any): boolean;
export function failedStages(stages: any): StageFailure[];
/** The ONE run snapshot, built from engine status + live events (hydrate-then-live). */
export function buildRunSnapshot(options?: any): RunSnapshot;
export { orderWins, makeClock, applySnapshot } from './snapshotOrder.mjs';
export { stageStatus } from './scan.mjs';
