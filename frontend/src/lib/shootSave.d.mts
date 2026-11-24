export type SaveStep = { label: string; run: () => Promise<unknown> | unknown };
export type SaveOutcome = { saved: string[]; failed: { label: string; message: string } | null; notRun: string[] };
export function featureLabel(key: string): string;
export function runSaveSteps(steps: readonly SaveStep[]): Promise<SaveOutcome>;
export function saveFailureMessage(outcome: SaveOutcome | null | undefined): string;
