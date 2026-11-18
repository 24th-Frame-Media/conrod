import type { Detection, Frame, Scope } from './types';

export type IdentifyScopeOption = { id: Scope; label: string; hint: string };
type ScopeFrame = Pick<Frame, 'id' | 'burst_pick' | 'rejected' | 'manual_stars'>;
type ScopeFacts = { dets: readonly Detection[] };

export const IDENTIFY_SCOPES: readonly Scope[];
export const DEFAULT_IDENTIFY_SCOPE: Scope;
export const IDENTIFY_SCOPE_OPTIONS: readonly IdentifyScopeOption[];

export function identifyScopeOf(value: unknown): Scope;
export function identifyScopeLabel(value: unknown): string;
export function identifyActionLabel(scope: unknown, n: number): string;
export function keeperCount(frames: readonly ScopeFrame[], facts: ReadonlyMap<number, ScopeFacts>, keepers: ReadonlySet<number>): number;
export function everythingCount(frames: readonly ScopeFrame[], facts: ReadonlyMap<number, ScopeFacts>): number;
