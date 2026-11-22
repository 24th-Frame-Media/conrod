import type { Job, Page } from './types';
import type { Stack } from './stacks.mjs';

/** The page a Library card opens on: the worker screen while its scan is live, waiting, stopped or failed; Review otherwise. */
export function openJobPage(job: Job | null | undefined, activeJob?: number | null): Page;

/** Where one arrow press went: a frame inside the stack, or the neighbouring stack to enter. */
export type StackStep = { id: number; expanded: true } | { stackIndex: number };

/** The next non-empty stack after `from` in `direction` (+1 forward, -1 back), or null at the end. */
export function nextStackIndex(stacks: Stack[], from: number, direction: number): number | null;

/** Shooting-order movement inside a stack, continuing into the neighbouring stack when enabled. */
export function stepInStack(
  stacks: Stack[],
  from: number,
  id: number | null,
  direction: 'forward' | 'back',
  opts?: { crossStacks?: boolean },
): StackStep | null;

/** The first frame of a stack when moving forward, its last when moving back. */
export function edgeFrameId(stack: Stack, direction: 'forward' | 'back'): number | null;
