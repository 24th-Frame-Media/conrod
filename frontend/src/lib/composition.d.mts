export type CompTone = 'good' | 'ok' | 'warn';
export type CompMode = 'centre' | 'thirds' | 'phi';
export type CompLabel = { id: string; text: string; tone: CompTone };

/** The composition fields the review payload carries per frame; all optional and null until `composition` runs. */
export type CompositionFrame = {
  id: number;
  comp_placement?: number | null;
  comp_mode?: CompMode | null;
  comp_balance_x?: number | null;
  comp_balance_y?: number | null;
  comp_symmetry?: number | null;
  comp_clutter?: number | null;
  comp_lines?: number | null;
  comp_subject_share?: number | null;
};

export function compositionLabels(frame: CompositionFrame | null | undefined): CompLabel[];
/** The stack's median measured clutter and the frames at least a tenth (and 0.05) below it. */
export type StackClutter = { median: number | null; cleanestIds: number[] };
export function stackClutter(frames: readonly CompositionFrame[] | null | undefined): StackClutter;
export function relativeCleaner(median: number | null | undefined, clutter: number | null | undefined): boolean;
export function panComparison(frames: readonly CompositionFrame[] | null | undefined): { id: number; clutter: number } | null;
