import type { Stack } from './stacks.mjs';

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
