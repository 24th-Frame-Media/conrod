export type AiHelper = {
  id: string;
  title: string;
  desc: string;
  keys?: readonly string[];
  capability?: 'vision' | 'decision';
};
export type ServiceConfig = {
  local: { downloaded: boolean; enabled: boolean };
  ollama: { hosts: string[]; enabled: boolean };
  openrouter: { key: string; enabled: boolean };
  opencode: { key: string; enabled: boolean };
};
export type ModelOption = { id: string; name: string };
export type ModelList = { models: ModelOption[]; freeText: boolean };
export type ModelLists = { vision: Record<string, ModelList>; decision: Record<string, ModelList> };
export type AiCtx = { settings: Record<string, unknown>; config: ServiceConfig; modelLists: ModelLists };
export type AiStatus = { text: string; tone: 'ok' | 'locked' | 'off'; tag: string };
export type CapGroup = {
  id: string; name: string; tag: string; zone: string;
  key: string | null; models: ModelOption[];
  disabledNote: string | null; freeText: boolean; testable: boolean;
};
export type AiZone = { id: string; title: string; helpers: AiHelper[]; empty: string };
export type AiView = {
  config: ServiceConfig; ctx: AiCtx; groups: AiZone[];
  isEmpty: boolean; summary: string; countOn: number;
};
export type UnlockReason = { locked: false } | { locked: true; reason: string; sub: string };

export const ZONE_TITLES: { pc: string; cloud: string };
export const ZONE_EMPTY: { cloud: string };
export const NOT_SET_UP: string;
export const HELPERS: readonly AiHelper[];
export const SIMPLE_CONTROLS: Record<string, readonly { key: string; label: string }[]>;
export const SERVICE_IDS: readonly string[];
export const SERVICE_META: Record<string, { name: string; short: string; tag: string; zone: string }>;
export const STRATS: readonly { id: string; name: string }[];
export const STRATS_HINT: string;
export const EMPTY_FORK: {
  heading: string; lead: string;
  download: { title: string; sub: string };
  cloud: { title: string; sub: string; note: string };
  cloudFormTitle: string; opencodeNote: string; openrouterNote: string;
};
export const SUMMARY_LINES: { both: string; local: string; cloud: string };
export const LOCKED_REASON: { vision: string; decision: string };

export function providerGroup(provider: string): string;
export function serviceConfig(settings?: Record<string, unknown>): ServiceConfig;
export function serving(id: string, config: ServiceConfig): boolean;
export function anyServing(config: ServiceConfig): boolean;
export function countOn(config: ServiceConfig): number;
export function isEmpty(config: ServiceConfig): boolean;
export function capGroups(cap: string, config: ServiceConfig, modelLists: ModelLists): CapGroup[];
export function capUsable(cap: string, config: ServiceConfig): boolean;
export function capStatus(cap: string, config: ServiceConfig, modelLists: ModelLists): AiStatus;
export function statusOf(helper: AiHelper, ctx: AiCtx): AiStatus;
export function zoneOf(helper: AiHelper, ctx: AiCtx): 'pc' | 'cloud';
export function groupByZone(ctx: AiCtx): AiZone[];
export function unlockReason(helper: AiHelper, ctx: AiCtx): UnlockReason;
export function summaryLine(config: ServiceConfig): string;
export function buildAiView(settings: Record<string, unknown>, modelLists: ModelLists): AiView;
export function setService(settings: Record<string, unknown>, id: string, patch?: Record<string, unknown>): { vlm_services: unknown[]; moments_services: unknown[]; use_decider: boolean };
export function setCapModel(settings: Record<string, unknown>, cap: string, id: string, model: string): Record<string, unknown[]>;

/* ── services-plan rebuild ─────────────────────────────────────────────── */
export type ServiceModel = {
  id: string; role: string; img?: boolean; pM?: number; cM?: number;
  efforts?: readonly string[]; def?: string; variants?: readonly string[];
  local?: boolean; on?: boolean; dl?: string; group?: string; name?: string;
};
export type ServiceTypeDef = {
  name: string; tag: string; dot: string; kind: 'key' | 'host';
  decisions: boolean; thinking: boolean; keyPlaceholder?: string; note: string;
  models: readonly ServiceModel[];
};
export type ServiceCard = {
  key: string; type: string; apiKey: string; host: string;
  enabled: boolean; reasoning: boolean; visionSel: string; decisionSel: string;
};
export type SharingOption = { id: string; name: string };
export type ServingModel = { id: string; ready: boolean };
export type ModelLine = { head: string | null; rest: string[]; all: string[]; noModel: boolean };
export type HelperStatus = { text: string; cls: string; dot: string };
export type Opt = { id: string; name: string };

export const IN_TOK: number;
export const OUT_TOK: number;
export const SERVICE_TYPES: Record<string, ServiceTypeDef>;
export const ADD_ORDER: readonly string[];
export const ADD_DESC: Record<string, string>;
export const THINK_HINT: string;
export const BUILTIN: { name: string; tag: string; note: string; models: readonly (ServiceModel & { role: string; group: string; dl?: string; assets?: readonly string[] | null })[] };
/** The engine row id each built-in card row reads (stable, never a name search). */
export const BUILTIN_ROW_ID: Record<string, string>;
export type BuiltinRowState = { downloaded: boolean; on: boolean; available: boolean };
/** Built-in row states from the engine's live model rows + the use_decider setting. */
export function builtinRowStates(models?: readonly Record<string, unknown>[], useDecider?: boolean): Record<string, BuiltinRowState>;
/** Honest download progress: real byte totals, indeterminate only when unknown. */
export function downloadProgress(done: number, total: number): { known: boolean; pct: number | null; label: string };
export const SHARING: readonly SharingOption[];
export const SHARING_HINT: string;
export const PRICE_BY_ID: Record<string, { pM: number; cM: number }>;
export const VISION_HELPER: string;
export const DECISION_HELPER: string;
export const SERVICE_PROVIDER_OF: Record<string, string>;

export function sharingValue(settings?: Record<string, unknown>): string;
export function setSharing(value: string): { vlm_strategy: string; moments_strategy: string };
export function fmtUsd(v: number, deep?: boolean): string;
export function photoCost(id: string): { varies: true } | { free: true } | { usd: number } | null;
export function costLine(picked?: readonly { id: string; local: boolean }[]): string;
export function visionModels(type: string): readonly ServiceModel[];
export function decisionModels(type: string): readonly ServiceModel[];
export function makeCard(type: string, opts?: Record<string, unknown>): any;
export function defaultThinking(): boolean;
export function cardReady(card: Record<string, unknown>): boolean;
export function visionCandidates(type: string, live?: readonly Record<string, unknown>[]): Opt[];
export function decisionCandidates(type: string, live?: readonly Record<string, unknown>[], decisionLive?: readonly Record<string, unknown>[]): { models: Opt[]; freeText: boolean };
export function providerToType(provider: string): string | null;
export function cardKeyFor(entry: Record<string, unknown>): string | null;
export function cardsFromDraft(settings?: Record<string, unknown>): ServiceCard[];
export function addService(settings: Record<string, unknown>, type: string, opts?: Record<string, unknown>): { vlm_services: unknown[]; moments_services: unknown[] };
export function removeService(settings: Record<string, unknown>, key: string): { vlm_services: unknown[]; moments_services: unknown[] };
export function patchService(settings: Record<string, unknown>, key: string, patch?: Record<string, unknown>): { vlm_services: unknown[]; moments_services: unknown[] };
export function servingFor(helperId: string, ctx?: Record<string, unknown>): ServingModel[];
export function modelLine(list: readonly ServingModel[]): ModelLine;
export function helperStatus(on: boolean, list: readonly ServingModel[]): HelperStatus;
/** Who actually serves the moments (decisions) job: a cloud service, else the local path. */
export type MomentsStatus = {
  path: 'cloud' | 'local'; service: string | null; model: string | null;
  label: string; status: string; ready: boolean; others: string[];
};
export function momentsStatus(settings?: Record<string, unknown>, localInstalled?: boolean): MomentsStatus;
export function poolCount(ctx?: Record<string, unknown>): { n: number; svc: number };
export function poolLabel(p: { n: number; svc: number }): string;
export function summaryText(ctx?: Record<string, unknown>): { text: string; dot: string };
