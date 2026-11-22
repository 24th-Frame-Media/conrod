import type { DeciderLive, IdentifyItem, IdentifyLive, LaneBox, LaneStep, PipelineStage, PipelineWorker, ScanRecentFrame } from './types';

export type NodeState = 'done' | 'active' | 'pending';
export type IngestNode = {
  /** Stage id (`read`…`pick`), `vision`, `done`; the identify-only chart uses the lower-cased label. */
  id: string;
  number: string;
  label: string;
  /** The real count, or a status word. */
  sub: string;
  /** Busy workers and the model, for the second line. */
  meta: string;
  title: string;
  state: NodeState;
  /** A stage this shoot does not run. */
  off: boolean;
  /** What clicking the node filters the worker grid to; null when the node is not a filter. */
  filter: string | null;
};
export type IngestPhase = 'idle' | 'starting' | 'scan' | 'group' | 'pick' | 'vision' | 'done' | 'stopped' | 'error';
export type LaneUiState = 'busy' | 'waiting' | 'paused' | 'idle';
export type LaneFrameView = {
  file: string;
  step: LaneStep | string | null;
  thumb_url: string;
  boxes: LaneBox[];
  af: [number, number, number, number] | null;
  af_kind: 'point' | 'grid' | null;
  stars: number | null;
  verdict: 'sharp' | 'soft' | 'blurred' | null;
  panning: boolean;
};
export type StripItem =
  | { kind: 'car'; key: string; id: number; car: IdentifyItem }
  | { kind: 'frame'; key: string; id: number; frame: ScanRecentFrame };

export const SCAN_STEPS: readonly LaneStep[];
export const STEP_STAGE: Readonly<Record<string, string>>;
export function stageOfStep(step: string | null | undefined, fallback?: string): string;
export function stepIndex(step: string | null | undefined): number;
export function visionExpected(stages: PipelineStage[] | null | undefined, identify?: IdentifyLive | null): boolean;
export function ingestNodes(input?: { stages?: PipelineStage[] | null; identify?: IdentifyLive | null; model?: string; finished?: boolean; decider?: import('./types').DeciderLive | null; serving?: string | null }): IngestNode[];
export function identifyNodes(live: IdentifyLive | null | undefined, model?: string | null, decider?: DeciderLive | null, serving?: string | null): IngestNode[];
export function ingestPhase(input?: {
  mode?: 'ingest' | 'identify';
  scan?: { state: string; stage?: string } | null;
  identify?: IdentifyLive | null;
  identifying?: boolean;
}): IngestPhase;
export function phaseRunning(phase: IngestPhase): boolean;
export function phaseWord(phase: IngestPhase, paused?: boolean): string;
export function phaseTone(phase: IngestPhase, paused?: boolean): 'running' | 'done' | 'stopped' | 'error';
export function ingestFraction(input?: { scanFraction?: number; identify?: IdentifyLive | null; vision?: boolean }): number;
export function identifyEta(identify: IdentifyLive | null | undefined): number | null;
export function leftLabel(label: string): string;
export function frameRate(text: string): string;
export function laneKey(worker: PipelineWorker): string;
export function laneNumber(worker: PipelineWorker): number;
export function laneState(worker: PipelineWorker | null | undefined, paused?: boolean): LaneUiState;
export function laneText(worker: PipelineWorker, paused?: boolean): string;
export function laneBadge(state: LaneUiState): string;
export function selectLanes(
  workers: PipelineWorker[] | null | undefined,
  options?: { filter?: string | null; cap?: number; all?: boolean; paused?: boolean; identify?: boolean },
): { lanes: PipelineWorker[]; hidden: number };
export function laneFrame(worker: PipelineWorker, recent: ScanRecentFrame[] | null | undefined): LaneFrameView;
export function boxStyle(box: readonly number[] | null | undefined): { left: string; top: string; width: string; height: string } | null;
export function starsText(stars: number | null | undefined): string;
export function verdictText(verdict: string | null | undefined, panning?: boolean): string;
export function tagWidth(text: string | null | undefined): number;
export function tagFits(box: readonly number[] | null | undefined, frameWidth: number, text: string | null | undefined): boolean;
export function boxLabel(box: { cls?: string; conf?: number } | null | undefined): string;
export type BoxRole = 'main' | 'background';
/** The judged car: the vehicle under the focus area, else the biggest; `byAf` says which rule chose it. */
export function mainVehicle(boxes: readonly LaneBox[] | null | undefined, af?: readonly number[] | null): { index: number; byAf: boolean };
export function boxRole(box: { cls?: string } | null | undefined, index: number, boxes: readonly LaneBox[] | null | undefined, af?: readonly number[] | null): BoxRole | null;
export function roleLabel(box: { cls?: string; conf?: number } | null | undefined, role: BoxRole | null, byAf?: boolean): string;
export function etaLines(input?: { vision?: boolean; scanDone?: boolean; scanEta?: number | null; scanStable?: boolean; identify?: IdentifyLive | null }): string[];
export const TAG_HEIGHT: number;
export type AfTagSpot = 'bl' | 'br' | 'tr' | 'tl' | 'below' | 'above';
export function afLabel(kind: 'point' | 'grid' | null | undefined): string;
export function afTagSpot(
  af: readonly number[] | null | undefined,
  boxes: readonly { cls?: string; conf?: number; box: readonly number[] }[] | null | undefined,
  frame: { width: number; height: number },
  text?: string,
): AfTagSpot | null;
export function failedWords(errors: number | null | undefined): string;
export function ingestIdentify(identify: IdentifyLive | null | undefined, options?: { planned?: boolean; identifying?: boolean }): IdentifyLive | undefined;
export function stripItems(input?: { cars?: IdentifyItem[]; frames?: ScanRecentFrame[]; limit?: number }): StripItem[];
