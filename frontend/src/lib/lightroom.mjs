/*
 * The Lightroom bridge's words. Pure, so `node --test` imports them without a DOM: the export
 * result line, the import summary, the pending-package rows and the error line.
 *
 * Folder layout and package shapes: docs/plans/lightroom-bridge.md.
 */

const isNum = (v) => typeof v === 'number' && Number.isFinite(v);
const num = (v) => (isNum(v) ? v : null);
const text = (v) => (typeof v === 'string' && v.trim() !== '' ? v.trim() : null);
const count = (v) => (num(v) == null ? '0' : Math.round(v).toLocaleString('en-US'));
const photos = (n) => `${count(n)} ${num(n) === 1 ? 'photo' : 'photos'}`;

/** `61%` from a 0..100 percent; a 0..1 share is read as one too. Null stays null. */
export function percentLabel(percent) {
  const p = num(percent);
  if (p == null) return null;
  const whole = p > 0 && p < 1 ? p * 100 : p;
  return `${Math.max(0, Math.min(100, Math.round(whole)))}%`;
}

/** "58 frames for Lightroom · package bathurst-1000-1763600000, waiting for the plugin". */
export function lightroomExportLine(result) {
  const name = text(result?.package);
  return `${count(result?.photos)} frames for Lightroom${name == null ? '' : ` · package ${name}`}, waiting for the plugin`;
}

/** The import's counts: "58 matched · 6 unmatched · 11 frames with no catalog entry". */
export function lightroomImportLine(result) {
  const missing = num(result?.missing) ?? 0;
  const frames = `${count(missing)} ${missing === 1 ? 'frame' : 'frames'} with no catalog entry`;
  return `${count(result?.matched)} matched · ${count(result?.unmatched)} unmatched · ${frames}`;
}

/** "72% matched your picks", or null when the import could not compare anything. */
export function lightroomImportPercent(result) {
  const percent = percentLabel(result?.percent);
  return percent == null ? null : `${percent} matched your picks`;
}

/** Seconds into "just now", "7 min ago", "2 h ago", "3 d ago"; null when the age is unknown. */
export function ageLabel(seconds) {
  const s = num(seconds);
  if (s == null || s < 0) return null;
  if (s < 60) return 'just now';
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  return `${Math.floor(s / 86400)} d ago`;
}

/**
 * One row per waiting package. The Rust reader owns the payload; this accepts an array or
 * `{ packages: [...] }` and the field names both a cool and a hurried writer could pick.
 */
export function pendingPackages(payload) {
  const list = Array.isArray(payload) ? payload : Array.isArray(payload?.packages) ? payload.packages : [];
  return list
    .filter((item) => item != null && typeof item === 'object')
    .map((item) => ({
      name: text(item.name) ?? text(item.package) ?? text(item.file) ?? 'package',
      album: text(item.album) ?? text(item.label),
      photos: num(item.photos) ?? num(item.count),
      age: num(item.age_s) ?? num(item.age_seconds) ?? num(item.age),
    }));
}

/** "Bathurst 1000 · 64 photos · 7 min ago"; the album and age drop out when unknown. */
export function packageLine(pkg) {
  const parts = [];
  if (text(pkg?.album) != null) parts.push(text(pkg.album));
  if (num(pkg?.photos) != null) parts.push(photos(pkg.photos));
  const age = ageLabel(pkg?.age);
  if (age != null) parts.push(age);
  return parts.join(' · ');
}

/** "2 packages waiting" / "1 package waiting" / "No packages waiting". */
export function pendingLine(list) {
  const n = Array.isArray(list) ? list.length : 0;
  if (n === 0) return 'No packages waiting';
  return `${n} ${n === 1 ? 'package' : 'packages'} waiting`;
}

const SECRET = /\bsk-[A-Za-z0-9_-]{8,}\b/g;
const KEYED = /\b(api[_-]?key|token|secret|password)\b(\s*[=:]\s*)\S+/gi;

/**
 * An error line safe for the DOM. The bridge commands carry no credentials, but an engine error
 * can echo whatever it was given, so the app's rule applies here too: a key never reaches the UI.
 */
export function lightroomError(error) {
  const raw = error instanceof Error ? error.message : String(error ?? '');
  const line = raw.trim() === '' ? 'The Lightroom bridge could not be read.' : raw.trim();
  return line.replace(SECRET, 'sk-***').replace(KEYED, (_m, name, sep) => `${name}${sep}***`);
}
