export type StackFrame = {
  id: number; path: string; rejected?: number; picked?: number; burst_key?: number | null; dupe_key?: number | null;
  manual_stars?: number | null; rating?: number | null;
};
export type StackFact = { cull?: string | null; pick?: boolean };
export type Facts = Map<number, StackFact>;
export type Decision = 'rejected' | 'picked' | 'undecided';
export type Stack<F extends StackFrame = StackFrame> = { key: string; frames: F[]; keepers: F[]; lead: F; total: number };

export function decisionOf(frame: StackFrame, fact: StackFact | undefined): Decision;
export function keepersOf<F extends StackFrame>(group: F[], facts: Facts): F[];
export function groupKeyOf(frame: StackFrame, dupes?: boolean): string;
export function keeperIds(frames: StackFrame[], facts: Facts): Set<number>;
export function leadOf<F extends StackFrame>(frames: F[], keepers: F[], facts: Facts): F;
export function makeStack<F extends StackFrame>(key: string, frames: F[], opts: { facts: Facts; keepers: Set<number>; total?: number }): Stack<F>;
export function buildStacks<F extends StackFrame>(frames: F[], opts: { facts: Facts; keepers: Set<number>; all?: F[]; dupes?: boolean }): Stack<F>[];

export function stackIndexOf(stacks: Stack[], id: number | null): number;
export function stepInStack(stacks: Stack[], id: number | null, delta: number): number | null;
export function stepStack(stacks: Stack[], id: number | null, delta: number): number | null;
export function stepFlat(frames: StackFrame[], id: number | null, delta: number): number | null;
export function stepRow(frames: StackFrame[], id: number | null, delta: number, cols: number): number | null;
export function reviewOrder<F extends StackFrame>(stack: Stack<F>): F[];
export function nextUndecided<F extends StackFrame>(seq: F[], id: number | null, undecided: (f: F) => boolean, opts?: { dir?: 1 | -1; wrap?: boolean }): number | null;
export function nearestVisible(prevIds: number[], visible: Set<number>, id: number): number | null;

export type StackLayout = {
  cols: number; rows: number; rowH: number; expandedRow: number; expandedTop: number; expandedH: number;
  rowTop: (row: number) => number; height: number;
};
export function stackLayout(opts: { count: number; cols: number; rowH: number; gap: number; pad: number; expandedIndex?: number; expandedH?: number }): StackLayout;
export function visibleRows(layout: StackLayout, scrollTop: number, viewH: number, overscan?: number): [number, number];
export function scrollTopFor(layout: StackLayout, index: number, scrollTop: number, viewH: number, pad: number): number | null;
