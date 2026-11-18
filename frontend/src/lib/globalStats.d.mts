import type { GlobalStats } from './types';
import type { GearRow, LensRows, StatsTile } from './shotStats.mjs';

/** Subjects that got a sharpness verdict: sharp + soft + blurred across every album. */
export function measuredSubjects(stats: GlobalStats | null | undefined): number;
/** The sharp subject rate as a whole percent; null when no subject was measured. */
export function sharpRate(stats: GlobalStats | null | undefined): number | null;
/** Effective keeper frames as a whole percent of every frame; null without frames. */
export function keeperRate(stats: GlobalStats | null | undefined): number | null;
/** The four hero tiles: frames, time shooting, hit rate and keepers. */
export function globalTiles(stats: GlobalStats | null | undefined): StatsTile[];
/** Why gear and exposure cover fewer albums than the library holds; null when not needed. */
export function completeNote(stats: GlobalStats | null | undefined): string | null;
/** The stats-shaped object the gear row builders read; shares are of the complete frames. */
export type GlobalGear = {
  frames: number;
  cameras: GlobalStats['cameras'];
  lenses: GlobalStats['lenses'];
  most_used: GlobalStats['most_used'];
};
export function globalGear(stats: GlobalStats | null | undefined): GlobalGear;
/** Camera rows for the gear list, biggest first. */
export function globalCameraRows(stats: GlobalStats | null | undefined): GearRow[];
/** Lens rows, top five with the most-used flag. */
export function globalLensRows(stats: GlobalStats | null | undefined, limit?: number): LensRows;
