import type { PipelineStage, PipelineWorker, ScanFound, WorkerLimit } from './types';

export type StageStatus = 'off' | 'done' | 'active' | 'pending';
export type EtaSample = { finish: number; now: number };
export type WorkerPool = { id: 'preview' | 'cull' | 'detect' | 'identify'; key: string; label: string; hint: string };

export const STAGE_ORDER: readonly string[];
export const WORKER_POOLS: readonly WorkerPool[];
export function orderStages(stages: PipelineStage[] | null | undefined): PipelineStage[];
export function stageStatus(stage: PipelineStage | null | undefined): StageStatus;
export function readyQueue(ordered: PipelineStage[], index: number): number;
export function bottleneck(stages: PipelineStage[] | null | undefined, workers?: PipelineWorker[] | null, minQueue?: number): string | null;
export function overallFraction(stages: PipelineStage[] | null | undefined): number;
export function overallEta(stages: PipelineStage[] | null | undefined): number | null;
export function waitingText(worker: PipelineWorker | null | undefined): string;
export function workersOf(workers: PipelineWorker[] | null | undefined, stageId: string): PipelineWorker[];
export function workerCounts(stage: PipelineStage, workers: PipelineWorker[] | null | undefined): { busy: number; waiting: number; idle: number };
export function formatDuration(seconds: number | null | undefined): string;
export function formatRate(rate: number | null | undefined): string;
export function pushEtaSample(history: EtaSample[] | null | undefined, nowMs: number, etaS: number | null | undefined, done: number, total: number): EtaSample[];
export function etaStable(history: EtaSample[] | null | undefined): boolean;
export function etaLabel(etaS: number | null | undefined, stable: boolean): string;
export function percent(done: number, total: number): number;
export function summaryLine(total: number, found: Partial<ScanFound> | null | undefined): string;
export function clampWorkers(value: number, limit: WorkerLimit): number;
