// The shoot statistics dialog's view model: hero tiles, gear lists, exposure buckets and quality
// segments. Pure, so the reading decisions (shares, top-N tails, stacked percentages) are testable
// without a DOM, and the dialog only lays the rows out.

import {
  formatAperture, formatBytes, formatDuration, formatFocal, formatShutter, formatTaken,
} from './shot.mjs';

const isNum = (v) => typeof v === 'number' && Number.isFinite(v);
const numOr = (v) => (isNum(v) ? v : null);
const text = (v) => (typeof v === 'string' && v.trim() !== '' ? v.trim() : null);
/** A count as the reader expects it: 6912 -> "6,912". */
const count = (v) => (numOr(v) == null ? null : Math.round(v).toLocaleString());
/** 5.60 -> "5.6", 4.00 -> "4": one decimal at most, no padded zero. */
const trim1 = (v) => String(Number(v.toFixed(1)));

/** A part of a whole as a whole percent, 0..100; null when the whole is unknown or empty. */
export function sharePercent(part, total) {
  const p = numOr(part);
  const t = numOr(total);
  if (p == null || t == null || t <= 0) return null;
  return Math.max(0, Math.min(100, Math.round((Math.max(0, p) / t) * 100)));
}

/** The average frame size in bytes; null without a positive frame count. */
export function averageBytes(size, frames) {
  const s = numOr(size);
  const f = numOr(frames);
  if (s == null || s < 0 || f == null || f <= 0) return null;
  return Math.round(s / f);
}

/** The biggest `limit` entries by count, plus how many were left out. */
export function topN(items, limit = 5) {
  const list = (items ?? [])
    .filter((item) => numOr(item?.count) > 0)
    .slice()
    .sort((a, b) => numOr(b.count) - numOr(a.count));
  const size = Math.max(0, Math.floor(numOr(limit) ?? 0));
  const rows = list.slice(0, size);
  return { rows, more: list.length - rows.length };
}

/** Positive parts of a whole as stacked segments, in the order given, with a share each. */
export function stackedSegments(parts) {
  const list = (parts ?? []).filter((part) => numOr(part?.count) > 0);
  const total = list.reduce((sum, part) => sum + numOr(part.count), 0);
  return list.map((part) => ({
    key: part.key,
    label: text(part.label) ?? part.key,
    tone: part.tone ?? 'good',
    count: numOr(part.count),
    countLabel: count(part.count),
    percent: sharePercent(part.count, total),
  }));
}

/**
 * A shoot's ends as one compact line: "8 Oct 2026 · 09:12–15:51" on one
 * day, "8–9 Oct 2026 · 23:00–01:00" across midnight. The date is written
 * once so the tile's sub stays two lines instead of three.
 */
export function takenRange(from, to) {
  const a = numOr(from);
  const b = numOr(to);
  if (a == null || b == null || a <= 0 || b <= a) return null;
  const start = new Date(a * 1000);
  const end = new Date(b * 1000);
  if (!Number.isFinite(start.getTime()) || !Number.isFinite(end.getTime())) return null;
  // `taken_at` is the camera's wall clock encoded as UTC (EXIF carries no zone), so every
  // component here is read in UTC; the machine's own zone would shift the shoot by hours.
  const sameDay = start.getUTCFullYear() === end.getUTCFullYear()
    && start.getUTCMonth() === end.getUTCMonth()
    && start.getUTCDate() === end.getUTCDate();
  const sameYear = start.getUTCFullYear() === end.getUTCFullYear();
  const sameMonth = sameDay || (sameYear && start.getUTCMonth() === end.getUTCMonth());
  // Fixed English month names and 24-hour clocks instead of the machine locale:
  // the same shoot must read the same way on an en-AU laptop and a CI runner.
  const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  const day = (value) => String(value.getUTCDate());
  const month = (value) => MONTHS[value.getUTCMonth()];
  const pad = (value) => String(value).padStart(2, '0');
  let date;
  if (sameDay) {
    date = `${day(start)} ${month(start)} ${start.getUTCFullYear()}`;
  } else if (sameMonth) {
    date = `${day(start)}–${day(end)} ${month(start)} ${start.getUTCFullYear()}`;
  } else if (sameYear) {
    date = `${day(start)} ${month(start)}–${day(end)} ${month(end)} ${start.getUTCFullYear()}`;
  } else {
    const s = formatTaken(a);
    const e = formatTaken(b);
    return s != null && e != null ? `${s} → ${e}` : null;
  }
  const clock = (value) => `${pad(value.getUTCHours())}:${pad(value.getUTCMinutes())}`;
  return `${date} · ${clock(start)}–${clock(end)}`;
}

/** The four hero tiles: frames, size, duration and keepers, each with its one-line sub. */
export function shootTiles(stats) {
  if (!stats) return [];
  const tiles = [];
  const frames = numOr(stats.frames);

  if (frames != null && frames > 0) {
    const hidden = numOr(stats.copies_hidden);
    tiles.push({
      key: 'frames',
      label: 'Frames',
      value: count(frames),
      sub: hidden != null && hidden > 0 ? `${count(hidden)} hidden` : null,
    });
  }

  const size = formatBytes(stats.size_bytes);
  if (size != null) {
    const average = averageBytes(stats.size_bytes, frames);
    tiles.push({
      key: 'size',
      label: 'Size',
      value: size,
      sub: average == null ? null : `${formatBytes(average)} average`,
    });
  }

  const from = numOr(stats.taken_from);
  const to = numOr(stats.taken_to);
  if (from != null && to != null && from > 0 && to > from) {
    const range = takenRange(from, to);
    tiles.push({
      key: 'duration',
      label: 'Duration',
      value: formatDuration(to - from),
      sub: range,
    });
  }

  const keepers = numOr(stats.keepers);
  if (keepers != null) {
    const sub = [];
    const rejected = numOr(stats.rejected);
    if (rejected != null) sub.push(`${count(rejected)} rejected`);
    const bursts = numOr(stats.bursts);
    if (bursts != null && bursts > 0) sub.push(`${count(bursts)} bursts`);
    tiles.push({ key: 'keepers', label: 'Keepers', value: count(keepers), sub: sub.join(' · ') || null });
  }

  return tiles;
}

/** One camera or lens row with its share of the frames; `more` is the tail the layout hides. */
function gearRows(items, mostUsed, total, limit) {
  const { rows, more } = topN(items, limit);
  const most = text(mostUsed);
  return {
    rows: rows
      .map((item) => {
        const name = text(item.name);
        return {
          name,
          count: numOr(item.count),
          countLabel: count(item.count),
          share: sharePercent(item.count, total),
          mostUsed: most != null && name === most,
        };
      })
      .filter((row) => row.name != null),
    more,
  };
}

/** The camera rows, biggest first; usually one, but a shoot can span two bodies. */
export function cameraRows(stats) {
  const items = stats?.cameras ?? [];
  return gearRows(items, stats?.most_used?.camera, numOr(stats?.frames), items.length).rows;
}

/** The lens rows, biggest first, each with its share of the frames and the most-used flag. */
export function lensRows(stats, limit = 5) {
  return gearRows(stats?.lenses, stats?.most_used?.lens, numOr(stats?.frames), limit);
}

/** The most-used exposure values as tiles: aperture, shutter, ISO and focal length. */
export function exposureTiles(stats) {
  const used = stats?.most_used ?? {};
  const tiles = [];
  const push = (key, label, value) => {
    if (value != null) tiles.push({ key, label, value });
  };
  push('aperture', 'Aperture', formatAperture(used.f_number));
  push('shutter', 'Shutter', formatShutter(used.shutter));
  push('iso', 'ISO', count(used.iso));
  push('focal', 'Focal length', formatFocal(used.focal_length));
  return tiles;
}

/** The top values of one bucket as labelled rows with a share bar each. */
export function bucketRows(items, format, total, limit = 3) {
  const { rows } = topN(items, limit);
  // Bars compare the top values with each other, so the most-used f-stop fills
  // its track; `share` keeps the honest share of all frames for tooltips/tests.
  const top = numOr(rows[0]?.count);
  return rows
    .map((item) => ({
      value: numOr(item.value),
      label: format(numOr(item.value)),
      count: numOr(item.count),
      countLabel: count(item.count),
      share: sharePercent(item.count, total),
      bar: sharePercent(item.count, top),
    }))
    .filter((row) => row.value != null && row.label != null);
}

/** "100–6,400" across a bucket list's values, or the one value they all share. */
export function rangeChip(items, format) {
  const values = [...new Set((items ?? [])
    .map((item) => numOr(item?.value))
    .filter((v) => v != null && v > 0))]
    .sort((a, b) => a - b);
  if (values.length === 0) return null;
  return values.length === 1
    ? format(values[0])
    : `${format(values[0])}–${format(values[values.length - 1])}`;
}

/** The small muted chips under the exposure rows: "ISO 100–6,400", "100–400 mm". */
export function exposureChips(stats) {
  const chips = [];
  const iso = rangeChip(stats?.isos, count);
  if (iso != null) chips.push(`ISO ${iso}`);
  const focal = rangeChip(stats?.focal_lengths, trim1);
  if (focal != null) chips.push(`${focal} mm`);
  return chips;
}

/** Sharp, soft and blurred subjects as stacked segments, biggest story first. */
export function qualityParts(stats) {
  return stackedSegments([
    { key: 'sharp', label: 'Sharp', tone: 'good', count: stats?.sharp },
    { key: 'soft', label: 'Soft', tone: 'warn', count: stats?.soft },
    { key: 'blurred', label: 'Blurred', tone: 'bad', count: stats?.blurred },
  ]);
}

/** The AF misses, split the way the detector records them; null when the album has no AF pass. */
export function afParts(stats) {
  const af = stats?.af;
  if (!af || !(numOr(af.frames) > 0)) return null;
  const items = [
    { key: 'off', label: 'off subject', count: af.off_subject },
    { key: 'no-subject', label: 'no subject', count: af.no_subject },
    { key: 'missing', label: 'no AF', count: af.missing },
    { key: 'soft', label: 'soft', count: af.soft },
  ]
    .filter((item) => numOr(item.count) > 0)
    .map((item) => ({ ...item, count: numOr(item.count), countLabel: count(item.count) }));
  const misses = (numOr(af.missing) ?? 0) + (numOr(af.off_subject) ?? 0) + (numOr(af.no_subject) ?? 0);
  return { misses, missesLabel: count(misses), items };
}

/** Pans, bursts and AF misses as small stats under the quality bar. */
export function shootMiniStats(stats) {
  const items = [];
  const pans = numOr(stats?.pans);
  if (pans != null && pans > 0) items.push({ key: 'pans', label: 'Pans', value: count(pans) });
  const bursts = numOr(stats?.bursts);
  if (bursts != null && bursts > 0) items.push({ key: 'bursts', label: 'Bursts', value: count(bursts) });
  const af = afParts(stats);
  if (af != null && af.misses > 0) items.push({ key: 'af-misses', label: 'AF misses', value: af.missesLabel });
  return items;
}
