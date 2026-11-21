/* The album's workflow choice (cull, identify or both) and what the stepper and actions make of it. Pure, so node --test can import it. */

export const WORKFLOW_MODES = ['both', 'cull', 'identify'];
export const DEFAULT_WORKFLOW = 'both';

/** The three choices, in the order the pickers list them: the same words in the review toolbar and the import sheet. */
export const WORKFLOW_OPTIONS = [
  { id: 'both', label: 'Identify + cull', hint: 'Rate and reject soft frames, then read numbers, plates and vehicle data on the keepers.' },
  { id: 'cull', label: 'Cull only', hint: 'Rate and reject soft frames. Nothing is identified.' },
  { id: 'identify', label: 'Identify only', hint: 'Keep every photo and identify all subjects. No automatic culls.' },
];

/**
 * A mode's hint for this shoot. A wedding or a bird shoot has no numbers or plates to read, so the words say
 * what Conrod can actually see instead of car data.
 */
export function workflowHint(value, { vehicles = true } = {}) {
  const mode = workflowModeOf(value);
  if (mode === 'both' && !vehicles) {
    return 'Rate and reject soft frames, then read the names, numbers and details Conrod can see on the keepers.';
  }
  return (WORKFLOW_OPTIONS.find((option) => option.id === mode) ?? WORKFLOW_OPTIONS[0]).hint;
}

/** Whatever was stored (or nothing) becomes a mode the rest of the UI can switch on. */
export function workflowModeOf(value) {
  return WORKFLOW_MODES.includes(value) ? value : DEFAULT_WORKFLOW;
}

/** The picker's words for a mode; an unknown value reads as the default. */
export function workflowLabel(value) {
  const mode = workflowModeOf(value);
  return WORKFLOW_OPTIONS.find((option) => option.id === mode).label;
}

export function workflowIncludesCull(value) {
  return workflowModeOf(value) !== 'identify';
}

export function workflowIncludesIdentify(value) {
  return workflowModeOf(value) !== 'cull';
}

/**
 * The steps the album actually runs, already filtered by its mode and numbered from 1 by position.
 * `scanned` is the engine's own count (`total > 0`, every frame done, no scan running); culling is part of
 * that scan, so both early steps settle together. `identified` is the payload's own verdict, not `job.status`,
 * which can stay `scanning` on a long-finished album.
 */
export function pipelineSteps(mode, state) {
  const now = state ?? {};
  const scanned = Boolean(now.scanned) && !now.scanning;
  const steps = [{ key: 'scan', label: 'Scanned', done: scanned, active: Boolean(now.scanning) }];
  if (workflowIncludesCull(mode)) steps.push({ key: 'cull', label: 'Culled', done: scanned, active: false });
  if (workflowIncludesIdentify(mode)) {
    steps.push({ key: 'identify', label: 'Identified', done: Boolean(now.identified) && !now.identifying, active: Boolean(now.identifying) });
  }
  return steps;
}
