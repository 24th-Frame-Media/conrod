import type { IdentifyItem, IdentifyLive, IdentifyStage } from './types';

export type FlowState = 'done' | 'active' | 'pending';
export type FlowNode = { number: string; label: string; sub: string; stage: IdentifyStage; state: FlowState };
export type Rate = { perMin: number; secondsPerCar: number };

export const IDENTIFY_STAGES: readonly IdentifyStage[];
export const FLOW_NODES: readonly Omit<FlowNode, 'state'>[];
export function activeStage(stage: string | null | undefined): number;
export function flowNodes(live: IdentifyLive | null | undefined, model?: string | null, stage?: IdentifyStage | null): FlowNode[];
export function displayPath(item: IdentifyItem | null | undefined): string;
export function fileName(item: IdentifyItem | null | undefined): string;
export function displayItem(live: IdentifyLive | null | undefined, selectedId?: number | null): IdentifyItem | null;
/** The newest finished car while its result is still fresh; `withinMs` defaults to 1800. */
export function justFinished(live: IdentifyLive | null | undefined, now: number, withinMs?: number): IdentifyItem | null;
export function confidencePercent(conf: number | null | undefined): number | null;
export function confidenceTone(conf: number | null | undefined): 'good' | 'warn' | 'bad' | 'none';
export function progressPercent(done: number | null | undefined, total: number | null | undefined): number;
export function elapsed(ms: number | null | undefined): string;
export function rate(done: number | null | undefined, elapsedMs: number | null | undefined): Rate | null;
export function rateLabel(done: number | null | undefined, elapsedMs: number | null | undefined): string;
export function prettyReply(reply: unknown): string;
export function sourceLabel(source: string | null | undefined): string;
