import type { DeciderLive, IdentifyItem, IdentifyLive, IdentifyStage } from './types';

export type FlowState = 'done' | 'active' | 'pending';
export type FlowNode = { number: string; label: string; sub: string; stage: IdentifyStage; state: FlowState; /** The VISION node while the vision model is off. */ off?: boolean };
export type Rate = { perMin: number; secondsPerCar: number };

export const IDENTIFY_STAGES: readonly IdentifyStage[];
export const FLOW_NODES: readonly Omit<FlowNode, 'state'>[];
export function activeStage(stage: string | null | undefined): number;
export function flowNodes(live: IdentifyLive | null | undefined, model?: string | null, stage?: IdentifyStage | null): FlowNode[];
export type IdentifyPhase = {
  name: 'moments' | 'peaks' | string;
  done: number;
  total: number;
  state: 'running' | 'done' | 'stopped';
};

/** A run-level phase as the header and the DECIDER node say it; empty for none. */
export function phaseLabel(phase: IdentifyPhase | null | undefined): string;
export type DeciderNode = { id: string; number: string; label: string; sub: string; meta: string; title: string; state: FlowState; off: boolean; filter: null };
/** The decision model's node for the identify flow, or null before it reports anything.
 *  `serving` names the cloud decision service actually doing moments (e.g. "jev-1.13 via OpenCode");
 *  its tick still waits for `phase` to finish. */
export function deciderStepNode(live: DeciderLive | null | undefined, serving?: string | null, phase?: IdentifyPhase | null): DeciderNode | null;
export function displayPath(item: IdentifyItem | null | undefined): string;
export function fileName(item: IdentifyItem | null | undefined): string;
export function displayItem(live: IdentifyLive | null | undefined, selectedId?: number | null): IdentifyItem | null;
/** One entry per worker: the car it is reading, else the last it finished, else null. */
export function workerLanes(live: IdentifyLive | null | undefined): { worker: number; item: IdentifyItem | null; live: boolean }[];
export const ACTION_LABELS: Readonly<Record<string, string>>;
export function actionLabel(id: string | null | undefined): string;
export function confidencePercent(conf: number | null | undefined): number | null;
export function confidenceTone(conf: number | null | undefined): 'good' | 'warn' | 'bad' | 'none';
export function progressPercent(done: number | null | undefined, total: number | null | undefined): number;
export function elapsed(ms: number | null | undefined): string;
export function rate(done: number | null | undefined, elapsedMs: number | null | undefined): Rate | null;
export function rateLabel(done: number | null | undefined, elapsedMs: number | null | undefined): string;
export function prettyReply(reply: unknown): string;
export function sourceLabel(source: string | null | undefined): string;
