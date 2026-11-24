// What the run screens say about a stage the photographer has parked or skipped,
// kept apart from the scan itself.
//
// A parked stage (stage_pause) and a paused scan (pause / resume_scan) are two
// different holds with two different Resume buttons. The engine marks the parent
// task paused when a stage parks (a parked Decision stage pauses "Judging
// moments"), so the task list alone cannot tell them apart; the stage controls
// can. Pure, so node --test can pin it.
import { albumSetting } from './album.mjs';

/** Plain names for the wire stage ids, for chips and buttons. */
export const STAGE_NAMES = Object.freeze({
  read: 'Read', detect: 'Detect', sharpness: 'Sharpness', faces: 'Faces', group: 'Group', pick: 'Pick',
  'identify:ocr': 'OCR', 'identify:decision': 'Decision', 'identify:vision': 'Vision',
});

/** The three identify sub-stages. */
export const IDENTIFY_STAGE_IDS = Object.freeze(['identify:ocr', 'identify:decision', 'identify:vision']);

/** The cull stages: parking one holds the scan itself (engine `CULL_STAGES`). */
const CULL_STAGE_IDS = Object.freeze(['read', 'detect', 'sharpness', 'faces']);

export const stageName = (id) => STAGE_NAMES[id] ?? String(id ?? '');

const idList = (value) => (Array.isArray(value) ? value.filter((v) => typeof v === 'string') : []);
const uniq = (list) => [...new Set(list)];

/** The task a parked stage marks paused: its label, so the screen can tell it from a scan pause. */
const STAGE_TASK = Object.freeze({ 'identify:decision': /^Judging\b/i });

/**
 * Stage ids currently parked by `stage_pause`. Sources, in order of trust:
 *  - a stage in the run snapshot that says so: an explicit `parked: true`
 *    (assumed field name; the engine reports a distinct per-stage flag) or the
 *    status word `paused` that `Progress::pause_stage` already sends;
 *  - the album's stored stage controls (`native_stage_pause`), which the app
 *    re-reads after every stage command.
 * Only meaningful while a run is active; the caller passes `active`.
 */
export function parkedStageIds({ stages = [], album = null, active = true } = {}) {
  if (!active) return [];
  const fromStages = (Array.isArray(stages) ? stages : [])
    .filter((s) => s && typeof s.id === 'string'
      && (s.parked === true || String(s.status ?? s.state ?? '').toLowerCase() === 'paused'))
    .map((s) => s.id);
  return uniq([...fromStages, ...idList(albumSetting(album, 'native_stage_pause', null))]);
}

/**
 * Is the scan itself paused (the Pause button), as opposed to a stage that
 * parked? A paused task counts unless it belongs to a parked stage: a parked
 * Decision pauses "Judging moments", a parked cull stage holds the scan, and a
 * task the engine flags as stage-parked is never the scan's. `Identifying…`
 * tasks are the identify run's own pause, not the scan's.
 */
export function scanPausedFrom({ tasks = [], parked = [] } = {}) {
  if (parked.some((id) => CULL_STAGE_IDS.includes(id))) return false;
  const owned = parked.map((id) => STAGE_TASK[id]).filter(Boolean);
  return (Array.isArray(tasks) ? tasks : []).some((t) => {
    if (!t || t.state !== 'paused') return false;
    const label = String(t.label ?? '');
    if (/^Identifying\b/.test(label)) return false;
    if (t.parked === true || t.stage_parked === true) return false;
    return !owned.some((re) => re.test(label));
  });
}

/** Parked stages in a plain sentence: "Decision is paused", "OCR and Vision are paused". */
export function parkedSentence(parked) {
  const names = (Array.isArray(parked) ? parked : []).map(stageName);
  if (!names.length) return '';
  const list = names.length > 1 ? `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}` : names[0];
  return `${list} ${names.length > 1 ? 'are' : 'is'} paused`;
}

/**
 * Stages the photographer skipped that still apply to later runs. The skip is
 * stored on the album (`native_stage_skip`) and read by every later identify,
 * so a header Skip is not a one-run action.
 */
export function skippedStageIds({ album = null } = {}) {
  return uniq(idList(albumSetting(album, 'native_stage_skip', null)));
}

/** The chip a skipped stage shows, with the consequence that it persists. */
export function skippedChip(id) {
  const name = stageName(id);
  return {
    id, name,
    label: `${name} skipped`,
    note: `Skipped for this album: later runs skip ${name} too until you undo it.`,
    undo: `Undo skip ${name}`,
  };
}

/** What the header Skip button promises: it persists. */
export function skipHeaderNote(id, consequence = '') {
  const name = stageName(id);
  const tail = 'It stays skipped on later runs of this album until you undo it.';
  return [consequence, `${name} will be skipped. ${tail}`].filter(Boolean).join(' ');
}
