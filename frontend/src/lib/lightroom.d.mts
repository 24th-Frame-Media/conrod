/** One package waiting in the bridge: its file name, album label, photo count and age in seconds. */
export type LightroomPendingRow = { name: string; album: string | null; photos: number | null; age: number | null };

/** The `lightroom_export` result line, and the two import lines. */
export function percentLabel(percent: number | null | undefined): string | null;
export function lightroomExportLine(result: { package?: string | null; photos?: number | null } | null | undefined): string;
export function lightroomImportLine(result: { matched?: number | null; unmatched?: number | null; missing?: number | null } | null | undefined): string;
export function lightroomImportPercent(result: { percent?: number | null } | null | undefined): string | null;

/** "just now", "7 min ago"; null without a usable age. */
export function ageLabel(seconds: number | null | undefined): string | null;

/** Accepts the command's array or `{ packages: [...] }` and canonicalises the rows the UI shows. */
export function pendingPackages(
  payload: Array<Record<string, unknown>> | { packages?: Array<Record<string, unknown>> } | null | undefined,
): LightroomPendingRow[];

/** "Bathurst 1000 · 64 photos · 7 min ago". */
export function packageLine(pkg: Partial<LightroomPendingRow> | null | undefined): string;
/** "2 packages waiting", "No packages waiting". */
export function pendingLine(list: Array<unknown> | null | undefined): string;

/** A key-redacted one-line error for inline display. */
export function lightroomError(error: unknown): string;
