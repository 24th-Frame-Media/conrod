import type { ShootStats, StatName, StatValue } from './shot.mjs';

/**
 * The fields the tile, row and segment helpers read. `shoot_stats` and `global_stats` both carry
 * them; the library's totals simply add albums and leave the per-shoot timestamps out.
 */
export type StatsLike = {
  frames?: number;
  cameras?: StatName[];
  lenses?: StatName[];
  most_used?: ShootStats['most_used'];
  f_numbers?: StatValue[];
  shutters?: StatValue[];
  isos?: StatValue[];
  focal_lengths?: StatValue[];
  pans?: number;
  bursts?: number;
  blurred?: number;
  soft?: number;
  sharp?: number;
  af?: ShootStats['af'];
};

/** One hero or exposure tile: a label above a big mono value, with an optional sub-line. */
export type StatsTile = { key: string; label: string; value: string; sub?: string | null };

/** One camera or lens: its share is 0..100 (null when the frame count is unknown). */
export type GearRow = {
  name: string;
  count: number;
  countLabel: string;
  share: number | null;
  mostUsed: boolean;
};

/** The visible lens rows plus how many smaller lenses the dialog hides. */
export type LensRows = { rows: GearRow[]; more: number };

/** One value bucket: the label for reading, its share of frames, and its bar against the top value. */
export type BucketRow = {
  value: number;
  label: string;
  count: number;
  countLabel: string;
  share: number | null;
  bar: number | null;
};

/** A tone is the colour the segment takes: green, amber or red. */
export type SegmentTone = 'good' | 'warn' | 'bad';

/** One stacked quality segment; sums to about 100 with its siblings. */
export type QualitySegment = {
  key: string;
  label: string;
  tone: SegmentTone;
  count: number;
  countLabel: string;
  percent: number | null;
};

/** The AF split: the total misses and only the non-zero causes. */
export type AfParts = {
  misses: number;
  missesLabel: string;
  items: { key: string; label: string; count: number; countLabel: string }[];
};

/** One small stat under the quality bar. */
export type MiniStat = { key: string; label: string; value: string };

/** A part of a whole as a whole percent, 0..100; null when the whole is unknown or empty. */
export function sharePercent(part: number | null | undefined, total: number | null | undefined): number | null;
/** The average frame size in bytes; null without a positive frame count. */
export function averageBytes(size: number | null | undefined, frames: number | null | undefined): number | null;
/** The biggest `limit` entries by count, plus how many were left out. */
export function topN<T extends { count: number }>(items: T[] | null | undefined, limit?: number): {
  rows: T[];
  more: number;
};
/** Positive parts of a whole as stacked segments, in the order given, with a share each. */
export function stackedSegments(parts: {
  key: string; label?: string; tone?: SegmentTone; count: number | null | undefined;
}[] | null | undefined): QualitySegment[];
/** A shoot's ends as one compact line: a date and clock range, or the full stamps across days. */
export function takenRange(from: number | null | undefined, to: number | null | undefined): string | null;
/** The four hero tiles: frames, size, duration and keepers, each with its one-line sub. */
export function shootTiles(stats: ShootStats | null | undefined): StatsTile[];
/** The camera rows, biggest first. */
export function cameraRows(stats: StatsLike | null | undefined): GearRow[];
/** The lens rows, biggest first, with the most-used flag. */
export function lensRows(stats: StatsLike | null | undefined, limit?: number): LensRows;
/** The most-used exposure values as tiles. */
export function exposureTiles(stats: StatsLike | null | undefined): StatsTile[];
/** The top values of one bucket as labelled rows with a share bar each. */
export function bucketRows(
  items: StatValue[] | null | undefined,
  format: (value: number | null | undefined) => string | null,
  total: number | null | undefined,
  limit?: number,
): BucketRow[];
/** "100–6,400" across a bucket list's values, or the one value they all share. */
export function rangeChip(
  items: StatValue[] | null | undefined,
  format: (value: number) => string | null,
): string | null;
/** The small chips under the exposure rows. */
export function exposureChips(stats: StatsLike | null | undefined): string[];
/** Sharp, soft and blurred subjects as stacked segments. */
export function qualityParts(stats: StatsLike | null | undefined): QualitySegment[];
/** The AF misses, split by cause; null when the album has no AF pass. */
export function afParts(stats: StatsLike | null | undefined): AfParts | null;
/** Pans, bursts and AF misses as small stats. */
export function shootMiniStats(stats: StatsLike | null | undefined): MiniStat[];
