export type FacetKind = 'number' | 'plate' | 'make' | 'team' | 'person' | 'subject';
export const FACET_KINDS: readonly FacetKind[];
export function facetsFor(preset: string | null | undefined): FacetKind[];
export function tabsWithData(preset: string | null | undefined, facets: Partial<Record<FacetKind, readonly unknown[]>> | null | undefined): FacetKind[];
export function validTab(wanted: FacetKind | null | undefined, tabs: readonly FacetKind[]): FacetKind | null;
