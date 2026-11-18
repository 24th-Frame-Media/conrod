// Shot data: EXIF metadata per frame and the album's `shoot_stats` counts, formatted once so the
// settings card and the full-frame viewer read the same. Pure, so node tests reach the same
// rounding the UI shows. Every value is optional: a file with no EXIF yields null, never "null".

const isNum = (v) => typeof v === 'number' && Number.isFinite(v);
const numOr = (v) => (isNum(v) ? v : null);
/** 5.60 -> "5.6", 4.00 -> "4": one decimal at most, no padded zero. */
const trim1 = (v) => String(Number(v.toFixed(1)));
const text = (v) => (typeof v === 'string' && v.trim() !== '' ? v.trim() : null);
/** A count as the reader expects it: 6912 -> "6,912". */
const count = (v) => (numOr(v) == null ? null : Math.round(v).toLocaleString());

/** The shutter as a fraction below half a second: "1/1000", else "0.5 s". */
export function shutterLabel(seconds) {
  const s = numOr(seconds);
  if (s == null || s <= 0) return null;
  return s < 0.5 ? `1/${Math.round(1 / s)}` : `${trim1(s)} s`;
}

/** "1/1000 s", "0.5 s", "1.3 s"; null when the file records no usable exposure time. */
export function formatShutter(seconds) {
  const label = shutterLabel(seconds);
  if (label == null) return null;
  return label.startsWith('1/') ? `${label} s` : label;
}

/** "ƒ/5.6", "ƒ/4"; null without a usable f-number. */
export function formatAperture(f) {
  const v = numOr(f);
  return v == null || v <= 0 ? null : `ƒ/${trim1(v)}`;
}

/** "400 mm", "24 mm"; null without a focal length. */
export function formatFocal(mm) {
  const v = numOr(mm);
  return v == null || v <= 0 ? null : `${trim1(v)} mm`;
}

const BYTE_UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];

/** "24.3 MB", "12.4 GB"; whole units from 100 up, so a card line stays short. */
export function formatBytes(bytes) {
  const v = numOr(bytes);
  if (v == null || v < 0) return null;
  let unit = 0;
  let n = v;
  while (n >= 1024 && unit < BYTE_UNITS.length - 1) {
    n /= 1024;
    unit += 1;
  }
  if (unit === 0) return `${Math.round(n).toLocaleString()} B`;
  return `${n < 100 ? trim1(n) : Math.round(n).toLocaleString()} ${BYTE_UNITS[unit]}`;
}

/** "+0.3 EV", "0.0 EV", "-0.7 EV"; the sign is always explicit except at zero. */
export function formatEv(ev) {
  const v = numOr(ev);
  if (v == null) return null;
  return `${v > 0 ? '+' : ''}${v.toFixed(1)} EV`;
}

/** "12.5 m", "3 m", "1.2 km" past a kilometre; null without a focus distance. */
export function formatDistance(m) {
  const v = numOr(m);
  if (v == null || v < 0) return null;
  return v >= 1000 ? `${trim1(v / 1000)} km` : `${trim1(v)} m`;
}

/** A short local date and time; null without a usable epoch. */
export function formatTaken(epoch) {
  const v = numOr(epoch);
  if (v == null || v <= 0) return null;
  const date = new Date(v * 1000);
  if (!Number.isFinite(date.getTime())) return null;
  // EXIF has no time zone: `taken_at` is the camera's wall clock encoded as UTC. Reading it back
  // in UTC shows the time the photographer saw; the viewer's own zone would shift it by hours.
  return date.toLocaleString(undefined, {
    day: 'numeric', month: 'short', year: 'numeric', hour: '2-digit', minute: '2-digit',
    timeZone: 'UTC',
  });
}

/** "45 min", "2 h 14 min", "3 d 2 h"; null for no duration. */
export function formatDuration(seconds) {
  const v = numOr(seconds);
  if (v == null || v <= 0) return null;
  const minutes = Math.round(v / 60);
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    const rest = minutes % 60;
    return rest > 0 ? `${hours} h ${rest} min` : `${hours} h`;
  }
  return `${Math.floor(hours / 24)} d ${hours % 24} h`;
}

/** The frame's shot details, present values only, in the viewer's order. */
export function shotRows(frame) {
  if (!frame) return [];
  const rows = [];
  const push = (label, value) => {
    if (value != null && value !== '') rows.push({ label, value });
  };
  push('Camera', text(frame.camera));
  push('Lens', text(frame.lens));
  push('Focal length', formatFocal(frame.focal_length));
  push('Aperture', formatAperture(frame.f_number));
  push('Shutter', formatShutter(frame.exposure_time));
  push('ISO', count(frame.iso));
  push('Exposure bias', formatEv(frame.exposure_bias));
  push('Focus distance', formatDistance(frame.focus_distance));
  push('File size', formatBytes(frame.file_size));
  push('Taken', formatTaken(frame.taken_at));
  return rows;
}
