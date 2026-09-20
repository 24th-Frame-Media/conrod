import type { Frame } from './lib/types';
type Rated = Pick<Frame, 'manual_stars' | 'rating'>;
export function filename(path: string): string;
export function starsFor(rating: number | null | undefined): number;
export function frameStars(frame: Rated): number;
export function filterFrames<T extends Rated & Pick<Frame, 'path' | 'rejected'>>(
  frames: T[], search: string, minStars: number, rejected: string,
): T[];
