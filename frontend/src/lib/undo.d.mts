import type { Frame, MarkValues } from './types';

/** One frame's part of an undo step; a single mark has exactly one, a bulk stack action has one per frame. */
export type UndoChange = { id: number; restore: MarkValues; after: MarkValues };
/** One undo step. `id` is the frame the view goes back to (the first change). */
export type UndoEntry = { id: number; changes: UndoChange[] };

export function capture(frame: Frame | null | undefined, values: MarkValues): MarkValues;
export function record(past: UndoEntry[], entry: UndoEntry, limit?: number): UndoEntry[];
export function undo(past: UndoEntry[], future: UndoEntry[]): { entry: UndoEntry; past: UndoEntry[]; future: UndoEntry[] } | null;
export function redo(past: UndoEntry[], future: UndoEntry[]): { entry: UndoEntry; past: UndoEntry[]; future: UndoEntry[] } | null;
