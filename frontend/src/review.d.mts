import type { Frame, Detection } from './lib/types';
export function subjectExcluded(frame: Frame, detection: Detection): boolean;
export function frameExcluded(frame: Frame, detections: Detection[]): boolean;
type Rated = Pick<Frame, 'manual_stars' | 'rating'>;
export function filename(path: string): string;
export function starsFor(rating: number | null | undefined): number;
export function frameStars(frame: Rated): number;
export function filterFrames<T extends Rated & Pick<Frame, 'path' | 'rejected'>>(
  frames: T[], search: string, minStars: number, rejected: string,
): T[];
