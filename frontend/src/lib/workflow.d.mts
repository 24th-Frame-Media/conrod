export type WorkflowMode = 'both' | 'cull' | 'identify';
export type WorkflowState = {
  /** `job.total > 0 && job.done >= job.total`, with no scan running. */
  scanned: boolean;
  /** Every subject is read: nothing eligible left and at least one detection in the payload. */
  identified: boolean;
  identifying: boolean;
  scanning: boolean;
};
export type PipelineStep = { key: 'scan' | 'cull' | 'identify'; label: string; done: boolean; active: boolean };
export type WorkflowOption = { id: WorkflowMode; label: string; hint: string };

export const WORKFLOW_MODES: readonly WorkflowMode[];
export const DEFAULT_WORKFLOW: WorkflowMode;
export const WORKFLOW_OPTIONS: readonly WorkflowOption[];

export function workflowModeOf(value: unknown): WorkflowMode;
export function workflowHint(value: unknown, options?: { vehicles?: boolean }): string;
export function workflowLabel(value: unknown): string;
export function workflowIncludesCull(value: unknown): boolean;
export function workflowIncludesIdentify(value: unknown): boolean;
export function pipelineSteps(mode: unknown, state?: Partial<WorkflowState>): PipelineStep[];
