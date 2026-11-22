/* Running one pipeline stage again on a finished album: the plain labels, the busy/running state read from the engine's operation keys, and the done toast. Pure, so node --test can import it. */

/** The stages a rerun can name, in the order the scan runs them. */
export const RERUN_STAGES = ['detect', 'rate', 'identify', 'moments', 'pick'];

/** The operation-name prefix the engine gives every stage rerun (`Rerunning detect:5`). */
export const RERUN_KIND = 'Rerunning';

const LABELS = {
  detect: { vehicles: 'Find cars again', subjects: 'Find subjects again' },
  rate: { vehicles: 'Check sharpness again', subjects: 'Check sharpness again' },
  identify: { vehicles: 'Read cars again', subjects: 'Read subjects again' },
  moments: { vehicles: 'Judge moments again', subjects: 'Judge moments again' },
  pick: { vehicles: 'Pick best of bursts again', subjects: 'Pick best of bursts again' },
};

const HINTS = {
  detect: 'Look for subjects in every frame again; your reads, stars and marks stay',
  rate: 'Re-measure sharpness from the pixels; a hand star is never lowered',
  identify: 'Read numbers, plates and details on the new subjects only',
  moments: 'Ask the judge about each keeper frame again, then choose the keepers',
  pick: 'Choose the best frame of each burst again; a hand pick always wins',
};

/** Whatever was stored (or nothing) becomes a stage the rest of the UI can switch on, or null. */
export function rerunStageOf(value) {
  return RERUN_STAGES.includes(value) ? value : null;
}

/** The plain button label for a stage. A shoot with no vehicles reads "subjects", not "cars". */
export function rerunLabel(stage, { vehicles = true } = {}) {
  const entry = LABELS[rerunStageOf(stage)];
  if (!entry) return 'Run again';
  return vehicles ? entry.vehicles : entry.subjects;
}

/** The menu hint for a stage; an unknown stage has none. */
export function rerunHint(stage) {
  return HINTS[rerunStageOf(stage)] ?? '';
}

/** The engine operation key a stage rerun runs under. */
export function rerunKey(stage, jobId) {
  return `${RERUN_KIND} ${stage}:${jobId}`;
}

/** The stage rerun running on this album right now, or null. */
export function runningRerun(operations, jobId) {
  const suffix = `:${jobId}`;
  for (const key of operations ?? []) {
    if (typeof key === 'string' && key.startsWith(`${RERUN_KIND} `) && key.endsWith(suffix)) {
      const stage = key.slice(RERUN_KIND.length + 1, key.length - suffix.length);
      if (rerunStageOf(stage)) return stage;
    }
  }
  return null;
}

/** True when any stage rerun is running on any album: only one rerun at a time. */
export function anyRerunRunning(operations) {
  return (operations ?? []).some((key) => typeof key === 'string' && key.startsWith(`${RERUN_KIND} `));
}

/** The toast once a rerun finishes. The engine writes a plain count summary for every stage but identify; identify gets a short done line. */
export function rerunDoneText(stage, detail) {
  const label = rerunLabel(stage);
  const text = typeof detail === 'string' ? detail.trim() : '';
  if (text && rerunStageOf(stage) !== 'identify') return text;
  return `${label} — done.`;
}

/** The toast when a rerun starts (the work runs in the background). */
export function rerunStartedText(stage) {
  return `${rerunLabel(stage)} started. Progress in Activity.`;
}
