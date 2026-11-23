import type { PolicySummary } from './presetPolicy.mjs';

export type SubjectWords = { one: string; many: string; title: string; rate: string; each: string };
export type SubjectKind = 'vehicles' | 'animals' | 'people' | 'subjects';

export const SUBJECT_WORDS: Record<SubjectKind, SubjectWords>;
export function subjectKind(summary: PolicySummary | null | undefined): SubjectKind;
export function subjectWords(summary: PolicySummary | null | undefined): SubjectWords;
/** Whether the album has a reader to run over a subject; unknown keeps them on. */
export function readsSubjects(summary: PolicySummary | null | undefined): boolean;
/** A detector's purpose line for the flow node. */
export function detectorPurpose(words?: SubjectWords | null): string;
/** The live identification banner's title for this album. */
export function identifyTitle(summary: PolicySummary | null | undefined): string;
