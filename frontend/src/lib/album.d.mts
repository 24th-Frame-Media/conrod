export type Grouping = 'bursts' | 'vehicles' | 'dupes' | 'flat';
export type ReviewPrefs = { grouping: Grouping; sort: 'review' | 'best' | 'worst' | 'frame' | 'pick' | 'taste'; view: 'all' | 'keepers' | 'undecided' | 'rejected' | 'top3' | 'review' | 'needsLook'; minStars: number };
export const defaultPrefs: ReviewPrefs;
export function prefsKey(jobId: number): string;
export function normalisePrefs(raw: unknown): ReviewPrefs;
export function parsePrefs(text: string | null | undefined): ReviewPrefs;
export function albumSetting(job: { settings_json?: string | null } | null | undefined, key: string, globals: Record<string, unknown> | null | undefined): unknown;
