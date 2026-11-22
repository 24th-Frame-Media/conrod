export type RerunStage = 'detect' | 'rate' | 'identify' | 'moments' | 'pick';

export const RERUN_STAGES: readonly RerunStage[];
export const RERUN_KIND: string;

export function rerunStageOf(value: unknown): RerunStage | null;
export function rerunLabel(stage: unknown, opts?: { vehicles?: boolean }): string;
export function rerunHint(stage: unknown): string;
export function rerunKey(stage: RerunStage, jobId: number): string;
export function runningRerun(operations: readonly string[] | undefined | null, jobId: number): RerunStage | null;
export function anyRerunRunning(operations: readonly string[] | undefined | null): boolean;
export function rerunDoneText(stage: unknown, detail: string | null | undefined): string;
export function rerunStartedText(stage: unknown): string;
