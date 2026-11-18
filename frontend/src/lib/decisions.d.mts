import type { Frame, MarkValues } from './types';

export type Decision = 'keep' | 'cull' | 'clear';
export type StackChange = { id: number; values: MarkValues };

export function applyDecision(frame: Frame | null | undefined, action: Decision): MarkValues;
export function stackDecision(frames: Frame[], keepId: number | 'cull'): StackChange[];
export function reviewedStacks(stacks: { frames: Frame[] }[], isUndecided: (frame: Frame) => boolean): { reviewed: number; total: number };
export function lowStarBadge(stars: number | null | undefined): string | null;
