export type AiHelper = {
  id: string;
  title: string;
  desc: string;
  keys?: readonly string[];
  capability?: 'vision' | 'decision';
};
export type ServiceModel = {
  id: string;
  role: string;
  img?: boolean;
  pM?: number;
  cM?: number;
  local?: boolean;
  on?: boolean;
};
export type ServiceTypeDef = {
  name: string;
  tag: string;
  dot: string;
  kind: 'key' | 'host';
  decisions: boolean;
  thinking: boolean;
  keyPlaceholder?: string;
  note: string;
  models: readonly ServiceModel[];
};
export type BuiltinModel = {
  id: string;
  role: 'vision' | 'decisions';
  group: string;
  dl?: string;
  on: boolean;
  assets: readonly string[] | null;
};
export type ServiceCard = {
  key: string;
  type: string;
  apiKey: string;
  host: string;
  enabled: boolean;
  reasoning: boolean;
  visionSel: string;
  decisionSel: string;
};
export type SharingOption = { id: string; name: string };
export type ServingModel = { id: string; ready: boolean };
export type ModelLine = { head: string | null; rest: string[]; all: string[]; noModel: boolean };
export type HelperStatus = { text: string; cls: string; dot: string };
export type Opt = { id: string; name: string };
export type LiveModel = { id?: string; model?: string; name?: string; vision?: boolean };
export type ManifestModelRow = {
  id?: string;
  name?: string;
  file?: string;
  ready?: boolean;
  downloaded?: boolean;
  available?: boolean;
};
export type BuiltinRowState = { downloaded: boolean; on: boolean; available: boolean };
export type AiPoolCard = {
  type?: string;
  enabled: boolean;
  visionSel: string;
  decisionSel: string;
  apiKey?: string;
  tested?: boolean;
  liveModels?: readonly { name: string; vision: boolean }[];
};
export type AiPoolCtx = {
  builtinVision?: readonly string[];
  builtinDecisions?: readonly string[];
  cards?: readonly AiPoolCard[];
  bundled?: Record<string, readonly ServingModel[]>;
  routing?: string;
};
export type MomentsStatus = {
  path: 'cloud' | 'local';
  service: string | null;
  model: string | null;
  label: string;
  status: string;
  ready: boolean;
  others: string[];
};
export type CheckOutcome = { ready: boolean; reason: string };

export const HELPERS: readonly AiHelper[];
export const SIMPLE_CONTROLS: Record<string, readonly { key: string; label: string }[]>;
export const IN_TOK: number;
export const OUT_TOK: number;
export const SERVICE_TYPES: Record<string, ServiceTypeDef>;
export const ADD_ORDER: readonly string[];
export const ADD_DESC: Record<string, string>;
export const THINK_HINT: string;
export const BUILTIN: { name: string; tag: string; note: string; models: readonly BuiltinModel[] };
export const DECIDER_QUANTS: readonly SharingOption[];
export const BUILTIN_ROW_ID: Record<string, string>;
export const SHARING: readonly SharingOption[];
export const SHARING_HINT: string;
export const DECISION_HELPER: string;

export function sharingValue(settings?: Record<string, unknown>): string;
export function setSharing(value: string): { vlm_strategy: string; moments_strategy: string };
export function fmtUsd(v: number, deep?: boolean): string;
export function photoCost(id: string): { varies: true } | { free: true } | { usd: number };
export function isPicked(sel: unknown): boolean;
export function costLine(picked?: readonly { id: string; local: boolean }[]): string;
export function decisionModels(type: string): readonly ServiceModel[];
export function makeCard(type: string, opts?: Record<string, unknown>): {
  key: string;
  type: string;
  enabled: boolean;
  thinking: boolean;
  reasoning: boolean;
  sel: { vision: string; decisions: string };
  opts: Record<string, unknown>;
  apiKey: string;
  url: string;
  tested: boolean;
};
export function defaultThinking(): boolean;
export function visionCandidates(type: string, live?: readonly LiveModel[]): Opt[];
export function decisionCandidates(
  type: string,
  live?: readonly LiveModel[],
  decisionLive?: readonly LiveModel[],
): { models: Opt[]; freeText: boolean };
export function providerToType(provider: string): string | null;
export function cardsFromDraft(settings?: Record<string, unknown>): ServiceCard[];
export function addService(
  settings: Record<string, unknown>,
  type: string,
  opts?: Record<string, unknown>,
): { vlm_services: unknown[]; moments_services: unknown[] };
export function removeService(
  settings: Record<string, unknown>,
  key: string,
): { vlm_services: unknown[]; moments_services: unknown[] };
export function patchService(
  settings: Record<string, unknown>,
  key: string,
  patch?: Record<string, unknown>,
): { vlm_services: unknown[]; moments_services: unknown[] };
export function servingFor(helperId: string, ctx?: AiPoolCtx): ServingModel[];
export function modelLine(list: readonly ServingModel[]): ModelLine;
export function helperStatus(on: boolean, list: readonly ServingModel[]): HelperStatus;
export function momentsStatus(settings?: Record<string, unknown>, localInstalled?: boolean): MomentsStatus;
export function poolCount(ctx?: AiPoolCtx): { n: number; svc: number };
export function summaryText(ctx?: AiPoolCtx): { text: string; dot: string };
export function checkOutcome(res: unknown, err?: unknown): CheckOutcome;
export function builtinRowStates(
  models?: readonly ManifestModelRow[],
  useDecider?: boolean,
): Record<string, BuiltinRowState>;
export function downloadProgress(done: number, total: number): { known: boolean; pct: number | null; label: string };
