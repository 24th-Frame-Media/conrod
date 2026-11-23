// A scan that did not finish, in the words a photographer can act on. The engine sends one raw
// error string; this turns it into a kind, a plain reason and a Resume prompt, so the scan screen
// can keep it in view instead of a six-second toast.

const KIND_TITLES = {
  disk_full: 'The disk is full',
  folder_missing: 'Conrod lost the folder',
  nas_unreachable: 'The network drive is offline',
  model_failed: 'A model could not be loaded',
  unknown: 'The scan did not finish',
};

/** The last path segment, so a long root reads as "Bathurst 1000", not the whole path. */
function folderName(root = '') {
  const parts = String(root).split(/[\\/]/).filter(Boolean);
  return parts.at(-1) ?? '';
}

const textOf = (raw, root) => `${root ?? ''} ${raw ?? ''}`;

function isNetworkPath(root = '') {
  return /^(?:\\\\|\/\/)/.test(String(root));
}

/** Which plain-language reason the engine's error falls into. */
export function scanErrorKind(raw = '', root = '') {
  const text = textOf(raw, root);
  if (/(?:no space|disk full|not enough (?:disk )?space|enospc|os error 112|quota)/i.test(text)) return 'disk_full';
  if (isNetworkPath(root) || /(?:nas|network|smb|unreachable|no route|timed out|timeout|offline|connection (?:refused|reset)|host is down)/i.test(text)) {
    return 'nas_unreachable';
  }
  if (/(?:no such file|not found|does not exist|cannot find|os error 2|folder.*(?:missing|gone)|path.*(?:missing|invalid)|drive.*(?:missing|not ready))/i.test(text)) {
    return 'folder_missing';
  }
  if (/(?:model|onnx|detector|face detector|install the models|load the detector|could not load|cannot load)/i.test(text)) return 'model_failed';
  return 'unknown';
}

/**
 * `{ kind, title, message, detail, resumable }`. `detail` is the raw engine text, for a title or
 * the activity log, never the main line. Every kind can be resumed once the cause is fixed.
 */
export function classifyScanError(raw = '', { root = '', label = '' } = {}) {
  const kind = scanErrorKind(raw, root);
  const folder = folderName(root) || 'the folder';
  const title = KIND_TITLES[kind] ?? KIND_TITLES.unknown;
  const message = {
    disk_full: 'Conrod ran out of room for its cache, so it stopped safely. Clear some space or use Settings → System → Clear cache, then press Resume. Nothing already measured is lost.',
    folder_missing: `The folder ${label ? `“${label}”` : `“${folder}”`} is not there any more. Reconnect the drive and press Resume. Nothing already measured is lost.`,
    nas_unreachable: `Conrod cannot reach ${label ? `“${label}”` : `“${folder}”`}. Check the NAS or the network, then press Resume. Nothing already measured is lost.`,
    model_failed: 'Conrod could not load what it needs to read frames. Check the models in Library → Setup, then press Resume. Nothing already measured is lost.',
    unknown: 'Conrod stopped safely. Check the activity log in the top bar, then press Resume. Nothing already measured is lost.',
  }[kind];
  return { kind, title, message, detail: String(raw ?? ''), resumable: true };
}

/**
 * The failure the scan screen should show, if any. A seed is a `scan` call that threw before the
 * engine made an album. Otherwise only an album the engine marked `error` counts; unreadable files
 * on a finished album are `done_errors` and keep their own summary.
 */
export function scanFailureFor({ job = null, progress = null, status = null, seed = null } = {}) {
  if (seed) return seed;
  if (!job || (job.status !== 'error' && progress?.state !== 'error')) return null;
  const failed = (status?.tasks ?? [])
    .filter((task) => task.state === 'failed')
    .sort((a, b) => b.id - a.id)[0];
  const raw = failed?.error || job.error_samples?.[0] || '';
  return {
    ...classifyScanError(raw, { root: job.root, label: job.label }),
    jobId: job.id,
    label: job.label,
    raw,
  };
}
