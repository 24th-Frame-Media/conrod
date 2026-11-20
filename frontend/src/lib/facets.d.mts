export type FacetKind = 'number' | 'plate' | 'make' | 'team' | 'person' | 'subject';
export const FACET_KINDS: readonly FacetKind[];
export function facetsFor(preset: string | null | undefined, overrides?: Record<string, boolean>): FacetKind[];
export function tabsWithData(preset: string | null | undefined, facets: Partial<Record<FacetKind, readonly unknown[]>> | null | undefined, overrides?: Record<string, boolean>): FacetKind[];
export function validTab(wanted: FacetKind | null | undefined, tabs: readonly FacetKind[]): FacetKind | null;
