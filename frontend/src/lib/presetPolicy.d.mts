import type { FacetKind } from './facets.mjs';

export type PolicyRow = {
  key: string; label: string; on: boolean; default: boolean; overridden: boolean;
  requires: string | null; blocked: boolean;
};
export type PolicySummary = {
  preset: string; label: string; profile: string; detects: PolicyRow[]; reads: PolicyRow[];
  moments: { detect: PolicyRow; rank: PolicyRow; vocabulary: { key: string; label: string; weight: number }[] };
  panForgiven: boolean; blur: string; levelCheck: boolean; darkOk: boolean; deliverMost: boolean;
};
export type Overrides = Record<string, boolean>;
export const DETECT_KEYS: readonly string[];
export const READ_KEYS: readonly string[];
export const MOMENT_KEYS: readonly string[];
export const FACET_ORDER: readonly FacetKind[];
export function defaultSummary(preset: string | null | undefined): PolicySummary;
export function overridesOf(stored: unknown): Overrides;
export function applyOverrides(summary: PolicySummary, overrides?: Overrides): PolicySummary;
export function nextOverrides(overrides: Overrides, row: PolicyRow, on: boolean): Overrides;
export function isOn(summary: PolicySummary, key: string): boolean;
export function noteList(summary: PolicySummary): string[];
export function needsLabel(summary: PolicySummary, row: PolicyRow): string;
export function facetsFromSummary(summary: PolicySummary): FacetKind[];
