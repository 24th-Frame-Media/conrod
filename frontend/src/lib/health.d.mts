import type { ModelInfo, Task } from './types';

export type ModelGroups = {
  /** Conrod's own required models: the only rows Library's Install button may fetch. */
  local: ModelInfo[];
  /** Opt-in models (quality, decision): shown, never installed by "Install missing". */
  optional: ModelInfo[];
  /** Vision services (Ollama, OpenRouter, …), configured in Settings. */
  vision: ModelInfo[];
};
export type HealthTone = 'ok' | 'busy' | 'warn' | 'error';
export type HealthStatus = { tone: HealthTone; label: string; detail: string };

export function modelGroups(models?: ModelInfo[] | null): ModelGroups;
export function installableModels(models?: ModelInfo[] | null): ModelInfo[];
export function modelWord(model: ModelInfo): string;
export function workLabel(label?: string): string;
export function healthStatus(input?: { tasks?: Task[] | null; models?: ModelInfo[] | null }): HealthStatus;
