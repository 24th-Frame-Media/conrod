import type { DeciderLive, DeciderState, DeciderStatus } from './types';

export type DeciderNode = {
  id: 'decider'; number: string; label: 'DECIDER'; sub: string; meta: string; title: string;
  state: 'done' | 'active' | 'pending'; off: boolean; filter: 'decider';
};
export type StatusLine = { tone: 'ok' | 'warn' | 'bad' | 'idle'; title: string; detail: string };
export function deciderWord(state: DeciderState | string): string;
export function deciderActive(live: DeciderLive | null | undefined): boolean;
export function msWords(ms: number | null | undefined): string;
export function deciderNode(live: DeciderLive | null | undefined, serving?: string | null): DeciderNode;
export function percent(p: number): string;
export function barTone(p: number): 'good' | 'warn' | 'none';
export function sizeWords(bytes: number): string;
export function statusLine(status: DeciderStatus | null | undefined): StatusLine;
export function canInstall(status: DeciderStatus | null | undefined): boolean;