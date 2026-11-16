import type { Frame, MarkValues } from './types';

export type UndoEntry = { id: number; restore: MarkValues; after: MarkValues };
export function capture(frame: Frame | null | undefined, values: MarkValues): MarkValues;
export function record(past: UndoEntry[], entry: UndoEntry, limit?: number): UndoEntry[];
export function undo(past: UndoEntry[], future: UndoEntry[]): { entry: UndoEntry; past: UndoEntry[]; future: UndoEntry[] } | null;
export function redo(past: UndoEntry[], future: UndoEntry[]): { entry: UndoEntry; past: UndoEntry[]; future: UndoEntry[] } | null;
