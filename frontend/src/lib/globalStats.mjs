// The library-wide statistics dialog's view model: the hero tiles, the completeness note and the
// gear shape the shoot dialog's row builders read. Pure, so the reading decisions (hit rate,
// keeper rate, which albums carry gear data) are testable without a DOM.

import { formatDuration } from './shot.mjs';
import { cameraRows, lensRows, sharePercent } from './shotStats.mjs';

const isNum = (v) => typeof v === 'number' && Number.isFinite(v);
const numOr = (v) => (isNum(v) ? v : null);
/** A count as the reader expects it: 23456 -> "23,456". */
const count = (v) => (numOr(v) == null ? null : Math.round(v).toLocaleString());
const albumsLabel = (n) => `${count(n)} ${n === 1 ? 'album' : 'albums'}`;

/** Subjects that got a sharpness verdict: sharp + soft + blurred across every album. */
export function measuredSubjects(stats) {
  return [stats?.sharp, stats?.soft, stats?.blurred]
    .reduce((sum, part) => sum + Math.max(0, numOr(part) ?? 0), 0);
}

/** The sharp subject rate as a whole percent; null when no subject was measured. */
export function sharpRate(stats) {
  return sharePercent(stats?.sharp, measuredSubjects(stats));
}

/** Effective keeper frames as a whole percent of every frame; null without frames. */
export function keeperRate(stats) {
  return sharePercent(stats?.keepers, stats?.frames);
}

/**
 * The four hero tiles: frames, time shooting, hit rate and keepers, each with its one-line sub.
 * The time tile is left out when no album has a first-to-last span to add up.
 */
export function globalTiles(stats) {
  if (!stats) return [];
  const frames = numOr(stats.frames);
  if (frames == null || frames <= 0) return [];
  const tiles = [];

  const albums = numOr(stats.albums);
  const hidden = numOr(stats.copies_hidden);
  const frameSub = [];
  if (albums != null && albums > 0) frameSub.push(albumsLabel(albums));
  if (hidden != null && hidden > 0) frameSub.push(`${count(hidden)} hidden`);
  tiles.push({ key: 'frames', label: 'Frames', value: count(frames), sub: frameSub.join(' · ') || null });

  const timed = numOr(stats.timed_albums);
  // The hero value is one line: "11 h 26 min" does not fit a 163 px tile, so
  // the total uses the short minute word. The average sub keeps the full one.
  const total = formatDuration(stats.shooting_seconds)?.replace(/ min$/, ' m') ?? null;
  if (timed != null && timed > 0 && total != null) {
    const average = formatDuration(stats.average_shoot_seconds);
    tiles.push({
      key: 'time',
      label: 'Time shooting',
      value: total,
      sub: average == null ? null : `avg ${average} per shoot`,
    });
  }

  const rate = sharpRate(stats);
  if (rate != null) {
    tiles.push({
      key: 'hit-rate',
      label: 'Hit rate',
      value: `${rate}%`,
      sub: `${count(stats.sharp)} of ${count(measuredSubjects(stats))} subjects sharp`,
    });
  }

  const keepers = numOr(stats.keepers);
  if (keepers != null) {
    const sub = [];
    const kept = keeperRate(stats);
    if (kept != null) sub.push(`${kept}% of frames kept`);
    const rejected = numOr(stats.rejected);
    if (rejected != null && rejected > 0) sub.push(`${count(rejected)} rejected`);
    tiles.push({ key: 'keepers', label: 'Keepers', value: count(keepers), sub: sub.join(' · ') || null });
  }

  return tiles;
}

/**
 * Why the gear and exposure figures cover fewer albums than the library holds. Null when every
 * album is complete, and when none is (the dialog replaces those sections instead).
 */
export function completeNote(stats) {
  const complete = numOr(stats?.complete_albums);
  const albums = numOr(stats?.albums);
  if (complete == null || albums == null) return null;
  if (complete <= 0 || complete >= albums) return null;
  return `Gear and exposure from ${count(complete)} of ${count(albums)} albums with complete shot data.`;
}

/**
 * The stats-shaped object `cameraRows`/`lensRows` read: gear shares are of the complete albums'
 * frames, so an incomplete album cannot shrink every bar.
 */
export function globalGear(stats) {
  return {
    frames: Math.max(0, numOr(stats?.complete_frames) ?? 0),
    cameras: stats?.cameras ?? [],
    lenses: stats?.lenses ?? [],
    most_used: { camera: stats?.most_used?.camera ?? null, lens: stats?.most_used?.lens ?? null },
  };
}

/** Camera rows for the gear list, biggest first, over the complete albums' frames. */
export function globalCameraRows(stats) {
  return cameraRows(globalGear(stats));
}

/** Lens rows, top five with the most-used flag, over the complete albums' frames. */
export function globalLensRows(stats, limit = 5) {
  return lensRows(globalGear(stats), limit);
}
