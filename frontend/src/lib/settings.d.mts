export type SettingsTabId = 'culling' | 'identification' | 'workflow' | 'metadata' | 'lightroom' | 'system';
export type SettingsFieldKind = 'toggle' | 'number' | 'text' | 'segmented' | 'profile' | 'workflow';
export type IdentifyStageKey = 'ocr' | 'decision' | 'vision';
export const DEFAULT_IDENTIFY_ORDER: readonly IdentifyStageKey[];
export const PARALLEL_NOTE: string;
export function normalizeIdentifyOrder(order: readonly string[] | null | undefined): IdentifyStageKey[];
export function moveIdentifyStage(order: readonly string[] | null | undefined, stage: string, delta: -1 | 1): IdentifyStageKey[];
export type SettingsFieldOption = { id: string; name: string };
export type SettingsField = {
  key: string;
  label: string;
  hint?: string;
  kind: SettingsFieldKind;
  album?: boolean;
  unit?: string;
  options?: readonly SettingsFieldOption[];
};
export type SettingsTab = { id: SettingsTabId; label: string; hint: string };
export type SettingsSection = {
  id: string;
  tab: SettingsTabId;
  title: string;
  hint?: string;
  advanced?: boolean;
  keys: readonly string[];
};
export type SettingsMatch = {
  key: string;
  label: string;
  tab: SettingsTabId;
  section: string;
  sectionTitle: string;
};
export type AlbumOverrideCount = { key: string; label: string; count: number };

export const TABS: readonly SettingsTab[];
export const FIELDS: Record<string, SettingsField>;
export const SECTIONS: readonly SettingsSection[];
export const OVERRIDABLE: readonly { key: string; label: string }[];

export function tabById(id: string): SettingsTab | null;
export function keyTab(key: string): SettingsTabId | null;
export function fieldsOf(sectionId: string): SettingsField[];
export function dirtyKeys(
  draft: Record<string, unknown> | null | undefined,
  saved: Record<string, unknown> | null | undefined,
): string[];
export function dirtyTabs(
  draft: Record<string, unknown> | null | undefined,
  saved: Record<string, unknown> | null | undefined,
): Record<SettingsTabId, number>;
export function searchFields(query: string): SettingsMatch[];
export function albumOverrideCounts(
  jobs: readonly unknown[],
  settings: Record<string, unknown> | null | undefined,
): AlbumOverrideCount[];
