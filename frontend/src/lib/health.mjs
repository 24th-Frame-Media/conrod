// One readiness model for the whole app: the health badge, its popover and Library's Setup card
// all read these helpers. A model row's `file` is "vlm" when it is a vision service (Ollama,
// OpenRouter, …), not a file Library's "Install missing" can fetch.

const VISION = 'vlm';

/** Group the engine's model rows by what can be done about them. */
export function modelGroups(models = []) {
  const local = [];
  const optional = [];
  const vision = [];
  for (const model of models ?? []) {
    if (model?.file === VISION) vision.push(model);
    else if (model?.optional) optional.push(model);
    else local.push(model);
  }
  return { local, optional, vision };
}

/** The rows Library's "Install missing" may fetch: Conrod's own required models, never a vision service. */
export function installableModels(models = []) {
  return modelGroups(models).local.filter((model) => !model.ready);
}

/** A row's short state word, in the same words everywhere. */
export function modelWord(model) {
  if (model?.file === VISION && /disabled in settings/i.test(model.detail ?? '')) return 'Off in Settings';
  if (model?.ready) return 'Ready';
  if (model?.optional) return 'Optional';
  if (model?.file === VISION) return 'Unavailable';
  return 'Not installed';
}

/** The task label without its album suffix, as the title bar has always shown it. */
export function workLabel(label = '') {
  if (label.startsWith('Identifying')) return 'Identifying';
  if (label.startsWith('Grouping')) return 'Grouping';
  if (label.startsWith('Culling')) return 'Culling';
  if (label.startsWith('Loading')) return 'Loading';
  // Tolerate a stray U+00C2 before the middle dot: a double-encoded separator
  // (\u00c2\u00b7) once shipped in task labels and must not leak into the badge.
  return label.replace(/\s*(?:\u00c2?\u00b7|\u2013|-)\s*album\s+\d+$/i, '');
}

const visionDetail = (rows) => {
  const off = rows.find((row) => /disabled in settings/i.test(row.detail ?? ''));
  if (off) return 'Vision is switched off in Settings; Conrod reads plates and numbers only.';
  const missing = rows.find((row) => !row.ready);
  return missing?.detail ? `Vision is unavailable: ${missing.detail}` : 'No vision service is available.';
};

/**
 * `{ tone, label, detail }`: task state first (Running, Paused, Error), then setup. Missing
 * required models are "Setup needed"; a vision service that is off or down is the amber
 * "Ready · vision off". Optional models never change the badge.
 */
export function healthStatus({ tasks = [], models = [] } = {}) {
  const active = (tasks ?? []).find((task) => task.state === 'running' || task.state === 'paused');
  if (active) {
    return {
      tone: active.state === 'paused' ? 'warn' : 'busy',
      label: active.state === 'paused' ? 'Paused' : workLabel(active.label),
      detail: active.detail || active.error || '',
    };
  }
  const failed = (tasks ?? []).find((task) => task.state === 'failed');
  if (failed) return { tone: 'error', label: 'Error', detail: failed.error || failed.label || '' };

  const groups = modelGroups(models);
  const missing = groups.local.filter((model) => !model.ready);
  if (missing.length) {
    return {
      tone: 'warn',
      label: 'Setup needed',
      detail: `${missing.length} of ${groups.local.length} models are not installed. Install them from Library → Setup.`,
    };
  }
  const visionOff = groups.vision.length > 0 && !groups.vision.some((model) => model.ready && !/disabled in settings/i.test(model.detail ?? ''));
  if (visionOff) return { tone: 'warn', label: 'Ready · vision off', detail: visionDetail(groups.vision) };
  return { tone: 'ok', label: 'Ready', detail: '' };
}
