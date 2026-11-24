export const STAGE_NAMES: Readonly<Record<string, string>>;
export const IDENTIFY_STAGE_IDS: readonly string[];
export function stageName(id: string | null | undefined): string;
export function parkedStageIds(input?: { stages?: ReadonlyArray<{ id?: string; status?: string; state?: string; parked?: boolean }> | null; album?: { settings_json?: string | null } | null; active?: boolean }): string[];
export function scanPausedFrom(input?: { tasks?: ReadonlyArray<{ state?: string; label?: string; parked?: boolean; stage_parked?: boolean }> | null; parked?: readonly string[] }): boolean;
export function parkedSentence(parked: readonly string[] | null | undefined): string;
export function skippedStageIds(input?: { album?: { settings_json?: string | null } | null }): string[];
export function skippedChip(id: string): { id: string; name: string; label: string; note: string; undo: string };
export function skipHeaderNote(id: string, consequence?: string): string;
