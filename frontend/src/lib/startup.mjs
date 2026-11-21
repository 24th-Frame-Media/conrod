/** What the splash shows while the engine starts: the running step, the finished ones, a bar.
 *  The engine sends one snapshot shape both as the `startup-progress` event and from the
 *  `startup_status` command, so the splash can always replace what it holds. */

/** Finished steps listed under the bar. */
export const STARTUP_MAX_VISIBLE = 5;
/** The splash never outlasts this on a stuck start-up step; the app opens anyway. */
export const STARTUP_LIMIT_MS = 5000;

const count = (n) => (Number.isFinite(n) && n > 0 ? Math.floor(n) : 0);
const text = (v) => (typeof v === 'string' && v.trim() ? v.trim() : null);

function stepOf(raw) {
  if (!raw || typeof raw !== 'object') return null;
  const label = text(raw.label);
  if (!label) return null;
  return {
    step: text(raw.step) ?? label,
    label,
    detail: text(raw.detail),
    error: text(raw.error),
    model: raw.model && typeof raw.model === 'object' && text(raw.model.file) ? raw.model : null,
  };
}

/** Whatever the engine sent, as a snapshot the splash can trust, or null when it is not one. */
export function normalizeStartup(raw) {
  const now = stepOf(raw);
  if (!now) return null;
  const completed = Array.isArray(raw.completed) ? raw.completed.map(stepOf).filter(Boolean) : [];
  return { ...now, done: count(raw.done), total: count(raw.total), ready: raw.ready === true, completed };
}

/** How full the bar is, 0 to 1, or null when the engine has not said how many steps there are. */
export function startupFraction(status) {
  if (!status || !(status.total > 0)) return null;
  return Math.min(1, Math.max(0, status.done / status.total));
}

/** `2 / 5` beside the headline, or nothing while the total is unknown. */
export function startupCount(status) {
  if (!status || !(status.total > 0)) return '';
  return `${Math.min(status.done, status.total)} / ${status.total}`;
}

/** The line the splash says right now. */
export function startupHeadline(status) {
  return status?.label || 'Starting';
}

/** `1.4 MB`, `38 MB`, `6.5 GB`; empty when the size is unknown. */
export function sizeText(bytes) {
  if (!Number.isFinite(bytes) || bytes <= 0) return '';
  const units = ['B', 'KB', 'MB', 'GB'];
  let n = bytes;
  let i = 0;
  while (n >= 1000 && i < units.length - 1) { n /= 1000; i += 1; }
  return `${n >= 100 || i === 0 ? Math.round(n) : Number(n.toFixed(1))} ${units[i]}`;
}

/** The model a step is about: its plain name, its file (shown in mono) and its size. */
export function modelParts(model) {
  if (!model || !text(model.file)) return null;
  return { name: text(model.name) ?? '', file: model.file, size: sizeText(model.bytes) };
}

/** The newest `max` finished steps, oldest first, each marked `ok` or `warn` (an error shows amber). */
export function visibleSteps(status, max = STARTUP_MAX_VISIBLE) {
  const done = status?.completed ?? [];
  return done.slice(Math.max(0, done.length - max)).map((s) => ({
    key: s.step,
    label: s.label,
    detail: s.error ?? s.detail ?? '',
    tone: s.error ? 'warn' : 'ok',
  }));
}

/** Whether the splash may leave: start-up finished, or its status is unavailable, or it took too long. */
export function startupSettled({ status, unavailable = false, waitedMs = 0, limitMs = STARTUP_LIMIT_MS }) {
  return unavailable || status?.ready === true || waitedMs >= limitMs;
}

const MOCK_FILE = { name: 'Car and person detector', file: 'yolo11s-960.onnx', bytes: 38_400_000 };
/** The steps the mock plays; mirrors what the engine really runs at start. */
export const MOCK_STEPS = [
  { step: 'library', label: 'Opening your library', detail: '3 albums' },
  { step: 'albums', label: 'Checking for albums a crash left open', detail: 'None found' },
  { step: 'models', label: 'Checking models', detail: 'All 11 models are on disk', model: MOCK_FILE },
  { step: 'exiftool', label: 'Looking for ExifTool', detail: 'Bundled with Conrod' },
  { step: 'gpu', label: 'Checking the graphics card', detail: 'NVIDIA GeForce RTX 4070, 12 GB' },
];

/** Dev mock: what start-up looks like `elapsedMs` into a run of `durationMs` (even steps). */
export function mockStartup(elapsedMs, durationMs = 2000) {
  const each = durationMs / MOCK_STEPS.length;
  const index = Math.min(MOCK_STEPS.length, Math.max(0, Math.floor(elapsedMs / each)));
  const completed = MOCK_STEPS.slice(0, index).map(({ model: _model, ...step }) => step);
  const total = MOCK_STEPS.length;
  if (index >= total) {
    return { step: 'ready', label: 'Ready', detail: null, model: null, error: null, done: total, total, ready: true, completed };
  }
  const now = MOCK_STEPS[index];
  return { step: now.step, label: now.label, detail: null, model: now.model ?? null, error: null, done: index, total, ready: false, completed };
}

/** Of two snapshots, the one further along: an event and a startup_status answer can arrive in either order. */
export function newerStartup(prev, next) {
  if (!next) return prev ?? null;
  if (!prev) return next;
  if (prev.ready && !next.ready) return prev;
  return next.ready || next.completed.length >= prev.completed.length ? next : prev;
}
