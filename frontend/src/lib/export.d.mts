export type ExportSummary = { total: number; keepers: number; rejected: number; undecided: number };
export function exportSummary(frames: Array<{ id: number; rejected?: number | null }>, keeperIds: Set<number> | Iterable<number> | null | undefined): ExportSummary;
export function exportLine(summary: ExportSummary): string;
export function exportRule(): string;
