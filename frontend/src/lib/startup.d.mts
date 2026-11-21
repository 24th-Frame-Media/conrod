import type { StartupModel, StartupStatus } from './types';

export type StartupLine = { key: string; label: string; detail: string; tone: 'ok' | 'warn' };
export type ModelParts = { name: string; file: string; size: string };
export type SettledInput = { status: StartupStatus | null | undefined; unavailable?: boolean; waitedMs?: number; limitMs?: number };

export const STARTUP_MAX_VISIBLE: number;
export const STARTUP_LIMIT_MS: number;
export const MOCK_STEPS: readonly { step: string; label: string; detail: string; model?: StartupModel }[];

/** Whatever the engine sent, as a snapshot the splash can trust, or null when it is not one. */
export function normalizeStartup(raw: unknown): StartupStatus | null;
/** How full the bar is, 0 to 1, or null when the total is unknown. */
export function startupFraction(status: StartupStatus | null | undefined): number | null;
/** "2 / 5", or empty while the total is unknown. */
export function startupCount(status: StartupStatus | null | undefined): string;
export function startupHeadline(status: StartupStatus | null | undefined): string;
/** "38.4 MB"; empty when unknown. */
export function sizeText(bytes: number | null | undefined): string;
export function modelParts(model: StartupModel | null | undefined): ModelParts | null;
/** The newest finished steps, oldest first; an error makes the line amber. */
export function visibleSteps(status: StartupStatus | null | undefined, max?: number): StartupLine[];
/** Whether the splash may leave: ready, status unavailable, or too slow. */
export function startupSettled(input: SettledInput): boolean;
/** Dev mock of start-up, elapsedMs into a run. */
export function mockStartup(elapsedMs: number, durationMs?: number): StartupStatus;/** Of two snapshots, the one further along (an event and a command answer can arrive in either order). */
export function newerStartup(prev: StartupStatus | null | undefined, next: StartupStatus | null | undefined): StartupStatus | null;
