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
