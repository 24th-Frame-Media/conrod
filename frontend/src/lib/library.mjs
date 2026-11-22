// Pure helpers for the Library redesign (docs/plans/library-redesign.md).
// Self-contained on purpose: no imports, so `node --test` can load it without a
// bundler and the grouping / filtering / label logic stays easy to pin down in
// tests. The small formatters mirror frontend/src/lib/format.ts.

export const STATUS_OPTIONS = [
  { value: 'all', label: 'All statuses' },
  { value: 'ready', label: 'Ready to review' },
  { value: 'scanning', label: 'Scanning' },
  { value: 'queued', label: 'Queued' },
  { value: 'stopped', label: 'Stopped' },
  { value: 'retry', label: 'Needs a retry' },
];

export const SORT_OPTIONS = [
  { value: 'newest', label: 'Newest first' },
  { value: 'oldest', label: 'Oldest first' },
  { value: 'name', label: 'Name A–Z' },
  { value: 'photos', label: 'Most photos' },
];

// ── tiny formatters (mirrors of format.ts) ──
export const plural = (n, one, many = `${one}s`) => `${n.toLocaleString()} ${n === 1 ? one : many}`;
export const pct = (done, total) => (total > 0 ? Math.min(100, Math.round((done * 100) / total)) : 0);
export const shortDate = (unix) => new Date(unix * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });
export const hiddenCopies = (copies) => (copies ? ` · ${plural(copies, 'copy', 'copies')} hidden` : '');
export const queueLabel = (position) => {
  const n = position + 1;
  const s = n % 100 >= 11 && n % 100 <= 13 ? 'th' : (['th', 'st', 'nd', 'rd'][n % 10] ?? 'th');
  return `Queued · ${n}${s} in line`;
};
export const albumCount = (n) => plural(n, 'album');
export const groupTitle = (name, count) => `${name} · ${count}`;

// ── state classification ──
/** One bucket per album state. The engine's queue wins over the stored status. */
export function statusBucket(job, ctx = {}) {
  const queue = ctx.queue ?? [];
  if (job.total === 0) return 'empty';
  if (job.status === 'scanning') return 'scanning';
  if (queue.includes(job.id) || job.status === 'queued') return 'queued';
  if (job.status === 'error') return 'error';
  if (job.status === 'done_errors') return 'retry';
  if (job.status === 'incomplete' || job.status === 'stopped') return 'stopped';
  if (job.status === 'indexed') return 'indexed';
  return 'done';
}

/** Needs attention is everything that is not a finished / indexed album. */
export function isAttention(job, ctx) {
  const b = statusBucket(job, ctx);
  return b !== 'done' && b !== 'indexed';
}

export function matchesSearch(job, search) {
  const q = String(search ?? '').trim().toLowerCase();
  return !q || job.label.toLowerCase().includes(q);
}

/** The status select filters both groups by bucket. */
export function matchesStatus(job, status, ctx) {
  if (!status || status === 'all') return true;
  const b = statusBucket(job, ctx);
  if (status === 'ready') return b === 'done' || b === 'indexed';
  if (status === 'scanning') return b === 'scanning' || b === 'empty';
  if (status === 'queued') return b === 'queued';
  if (status === 'stopped') return b === 'stopped';
  if (status === 'retry') return b === 'retry' || b === 'error';
  return true;
}

export function visibleJobs(jobs, options) {
  const { search = '', status = 'all', activeJob = null, queue = [] } = options || {};
  const ctx = { activeJob, queue };
  return jobs.filter((j) => matchesSearch(j, search) && matchesStatus(j, status, ctx));
}

const sortCompare = (a, b, sort) => {
  if (sort === 'oldest') return a.created_at - b.created_at;
  if (sort === 'name') return a.label.localeCompare(b.label);
  if (sort === 'photos') return b.total - a.total;
  return b.created_at - a.created_at; // newest (default)
};

/**
 * Split into the two labelled rows. Needs attention keeps a fixed precedence
 * (active → empty → queue order → stopped → errors); the chosen sort breaks ties
 * inside each band and orders the Ready to review row.
 */
export function groupAlbums(jobs, options) {
  const { activeJob = null, queue = [], sort = 'newest' } = options || {};
  const ctx = { activeJob, queue };
  const attention = [], ready = [];
  for (const job of jobs) (isAttention(job, ctx) ? attention : ready).push(job);
  const rank = (job) => {
    const b = statusBucket(job, ctx);
    if (b === 'scanning') return job.id === activeJob ? 0 : 1;
    if (b === 'empty') return 2;
    if (b === 'queued') return 10 + (queue.indexOf(job.id) < 0 ? 999 : queue.indexOf(job.id));
    if (b === 'stopped') return 1000;
    return 2000; // retry / error
  };
  attention.sort((a, b) => rank(a) - rank(b) || sortCompare(a, b, sort));
  ready.sort((a, b) => sortCompare(a, b, sort));
  return { attention, ready };
}

// ── one card's whole anatomy ──
/**
 * Plain description of an album card: chip, primary action, meta line, badge,
 * placeholder and cover bar. `opts` carries the live scan numbers already
 * computed in the component (percent, stage, eta) plus queue / active context,
 * so this stays pure. Every returned object has the same keys.
 */
export function cardView(job, opts = {}) {
  const ctx = { activeJob: opts.activeJob ?? null, queue: opts.queue ?? [] };
  const bucket = statusBucket(job, ctx);
  const queuePosition = opts.queuePosition ?? ctx.queue.indexOf(job.id);
  const percent = opts.percent ?? pct(job.done, job.total);
  const left = Math.max(0, job.total - job.done);
  let chip = { text: '', tone: 'ready' };
  let primary = { label: '', action: 'open', hint: '' };
  let meta = shortDate(job.created_at) + hiddenCopies(job.copies);
  let badge = job.total === 0 ? null : plural(job.total, 'photo');
  let placeholder = false, bar = false;
  const scanningElsewhere = Boolean(opts.scanningElsewhere);

  switch (bucket) {
    case 'empty':
      chip = { text: 'No photos yet', tone: 'warn' };
      primary = { label: 'Watch scan', action: 'open', hint: '' };
      meta = 'Importing · no photos yet';
      placeholder = true;
      break;
    case 'scanning':
      chip = { text: `Scanning · ${percent}%` + (opts.stage ? ` · ${opts.stage}` : '') + (opts.eta ? ` · ${opts.eta}` : ''), tone: 'busy' };
      primary = { label: 'Watch scan', action: 'open', hint: '' };
      bar = true;
      break;
    case 'queued':
      chip = { text: queueLabel(queuePosition < 0 ? 0 : queuePosition), tone: 'warn' };
      primary = { label: 'Remove from queue', action: 'unqueue', hint: '' };
      meta = 'Starts when the current scan finishes';
      break;
    case 'stopped':
      chip = { text: `Stopped at ${percent}% · ${plural(left, 'frame')} left`, tone: 'warn' };
      primary = scanningElsewhere
        ? { label: 'Add to queue', action: 'resume', hint: 'Starts when the current scan finishes.' }
        : { label: 'Resume scan', action: 'resume', hint: '' };
      break;
    case 'retry':
      chip = { text: `${plural(job.errors ?? 0, 'photo')} could not be read`, tone: 'bad' };
      primary = { label: 'Retry failed photos', action: 'resume', hint: '' };
      break;
    case 'error':
      chip = { text: 'Folder unreachable', tone: 'bad' };
      primary = { label: 'Resume scan', action: 'resume', hint: '' };
      meta = 'Nothing already measured is lost.';
      break;
    case 'indexed':
      chip = { text: 'Indexed — ready to cull', tone: 'ready' };
      primary = { label: 'Suggest culls', action: 'resume', hint: '' };
      break;
    case 'done':
    default:
      chip = { text: 'Ready to review', tone: 'ready' };
      primary = { label: 'Open review', action: 'open', hint: '' };
      break;
  }
  return { bucket, chip, primary, meta, badge, placeholder, bar, percent };
}

// ── the sticky status strip ──
const ERROR_COPY = {
  folder_unreachable: (label) => `Conrod lost the folder for ${label}. Reconnect the drive, then press Resume scan. Nothing already measured is lost.`,
  disk_full: () => `The disk holding Conrod's cache is full. Clear cache in Settings → System, then Resume. Nothing already measured is lost.`,
  exiftool_missing: () => `ExifTool was not found, so focus points cannot be read. Cull will use the sharpest car instead.`,
  no_gpu: () => `No graphics card found. Detection runs on the CPU and will be slower.`,
};
export function errorBanner(label, kind = 'folder_unreachable') {
  return (ERROR_COPY[kind] ?? ERROR_COPY.folder_unreachable)(label);
}

/** Right-hand setup wording; null when every model is ready. */
export function setupLine(setup) {
  if (!setup || !setup.needed) return null;
  return `Setup needed: ${setup.ready} of ${setup.total} models ready`;
}

/**
 * The strip's left-hand work line. `scan` is the running album's live numbers
 * ({ label, percent, stage?, eta? }) or null; `queueLabels` are the waiting
 * albums in order; `error` is { label, kind } | null; `elsewhere` is true when a
 * scan runs and other unfinished albums can be queued.
 */
export function stripWork(opts = {}) {
  if (opts.error) return { text: errorBanner(opts.error.label, opts.error.kind), tone: 'bad', watch: false };
  const scan = opts.scan ?? null;
  const waiting = opts.queueLabels ?? [];
  if (scan && opts.elsewhere) {
    return { text: `A scan is running on ${scan.label}. Other albums can be added to the queue.`, tone: 'busy', watch: true };
  }
  if (scan) {
    const bits = [`Scanning ${scan.label}`, `${scan.percent}%`];
    if (scan.stage) bits.push(scan.stage);
    if (scan.eta) bits.push(scan.eta);
    return { text: bits.join(' · '), tone: 'busy', watch: true };
  }
  if (waiting.length) {
    return { text: `${plural(waiting.length, 'album')} waiting to scan · ${waiting[0]} starts next`, tone: 'warn', watch: false };
  }
  return { text: 'No scan running', tone: 'idle', watch: false };
}
