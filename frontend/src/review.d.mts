import type { Frame, Detection } from './lib/types';
export function subjectExcluded(frame: Frame, detection: Detection): boolean;
export function frameExcluded(frame: Frame, detections: Detection[]): boolean;
/** The frame's own fault (blurred, overexposed, underexposed) unless a hand star or a bracket excuses it. */
export function frameFlaw(frame: Pick<Frame, 'flaw' | 'manual_stars' | 'set_id'>): 'blurred' | 'overexposed' | 'underexposed' | null;
/** One plain sentence for a frame's own fault. */
export function flawWords(flaw: string | null | undefined): string | null;
/** The keeper floor (three stars) and whether the camera's focus area clears it. */
export const KEEPER_MIN_RATING: number;
export function focusSharp(frame: Pick<Frame, 'af_sharpness'>): boolean;
type Rated = Pick<Frame, 'manual_stars' | 'rating'>;
export function filename(path: string): string;
export function starsFor(rating: number | null | undefined): number;
export function frameStars(frame: Rated): number;
export function burstTopRanks<T extends Rated & Pick<Frame, 'id' | 'rejected' | 'rating'>>(
  group: T[], isPick: (frame: T) => boolean, top?: number,
): Map<number, number>;
export function filterFrames<T extends Rated & Pick<Frame, 'path' | 'rejected'>>(
  frames: T[], search: string, minStars: number, rejected: string,
): T[];
