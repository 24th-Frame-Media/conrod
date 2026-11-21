// Pure words for when the vision model is not asked: the identify screen's header, the VISION node and the worker cards,
// plus the album's own vision switch. The engine sends `model: null` and a `vision_off_reason` instead of a model name.

/** Why vision is off: the album chose it, Settings has it off, no service is switched on, or this shoot type has nothing to ask. */
export const VISION_OFF_REASONS = ['album', 'settings', 'no_service', 'lanes'];

/** The reason an identify snapshot carries, or null when vision is on (or the snapshot predates the field). */
export function visionOffReason(live) {
  const reason = live?.vision_off_reason;
  return VISION_OFF_REASONS.includes(reason) ? reason : null;
}

/**
 * A vision model as a photographer reads it: a friendly name (`Qwen2.5-VL`), not the file or tag (`qwen2.5vl:7b`),
 * and where the photos go: `this PC` for Ollama, `cloud` for the hosted providers. Unknown names are kept as sent.
 */
export function visionModelWords(model) {
  const raw = String(model ?? '').trim();
  if (!raw) return '';
  const base = raw.replace(/\s+via\s+.+$/i, '').trim();
  const name = /qwen2\.5vl/i.test(base) ? 'Qwen2.5-VL' : /qwen/i.test(base) ? 'Qwen' : base.replace(/:[^/]+$/, '') || base;
  const where = /ollama|localhost|127\.0\.0\.1|this pc/i.test(raw) ? 'this PC'
    : /openrouter|openai|anthropic|gemini|cloud/i.test(raw) ? 'cloud'
      : /qwen/i.test(base) || /:[a-z0-9._-]+$/i.test(base) ? 'this PC' : '';
  return [name, where].filter(Boolean).join(' · ');
}

const HEADER = {
  album: 'vision model off for this album',
  settings: 'vision model off in Settings',
  no_service: 'no vision service set up',
  lanes: 'this type of shoot does not ask the vision model',
};

/** The header's line while a run goes on without vision. */
export function visionHeader(reason) {
  return `Reading plates and numbers \u2014 ${HEADER[reason] ?? HEADER.album}`;
}

const NODE = { album: 'off for this album', settings: 'off in Settings', no_service: 'no service set up', lanes: 'not used here' };

/** The VISION node's subtitle when vision is off. */
export function visionNodeSub(reason) {
  return NODE[reason] ?? NODE.album;
}

const WHY = {
  album: 'You turned vision off for this album, so Conrod reads plates and numbers only. Make, colour and team stay blank.',
  settings: 'Vision is switched off in Settings, so Conrod reads plates and numbers only. Make, colour and team stay blank.',
  no_service: 'No vision service is switched on in Settings, so Conrod reads plates and numbers only. Make, colour and team stay blank.',
  lanes: 'This type of shoot has nothing to ask the vision model.',
};

/** What a vision worker's card says in place of "Make not read" and the prompt: `{ title, why, action }`, with the button, if any. */
export function visionLaneWords(reason) {
  const action = reason === 'album' || reason === 'settings'
    ? { id: 'album', label: 'Turn vision on for this album' }
    : reason === 'no_service' ? { id: 'service', label: 'Set up a vision service' } : null;
  return { title: 'Vision off', why: WHY[reason] ?? WHY.album, action };
}

/**
 * The album's own vision choice from its stored settings: true or false when it set one, null while it follows the
 * Settings switch. (`albumSetting` cannot tell: it would answer with the global one.)
 */
export function albumVisionChoice(job) {
  let stored;
  try { stored = job?.settings_json ? JSON.parse(job.settings_json) : null; } catch { stored = null; }
  if (!stored || typeof stored !== 'object' || stored.vlm_inherit == null) return null;
  return typeof stored.use_vlm === 'boolean' ? stored.use_vlm : null;
}

/** Whether vision runs for an album: its own choice, else the Settings switch. */
export function albumVisionOn(job, globals) {
  const own = albumVisionChoice(job);
  return own ?? globals?.use_vlm !== false;
}

/** The album switch's caption for its own choice (true, false, or null to follow Settings): what it does now, and where that comes from. */
export function albumVisionHint(own, globals) {
  const model = String(globals?.vlm_services?.find?.((s) => s.enabled)?.model || globals?.vlm_model || '').trim();
  if (own === true) return globals?.use_vlm === false ? 'On for this album, although it is off in Settings.' : 'On for this album.';
  if (own === false) return 'Off for this album. Conrod reads plates and numbers only.';
  return globals?.use_vlm === false ? 'Off in Settings, so off here. Switch it on to use it for this album only.' : model ? `Follows Settings: on, using ${model}.` : 'Follows Settings: on.';
}

/** The choice a ticked or cleared switch means: the same as Settings is no choice (null), anything else is the album's own. */
export function nextAlbumVision(checked, globalOn) {
  return checked === globalOn ? null : checked;
}
