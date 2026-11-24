// Saving Shoot Settings is several engine calls (the settings patch, one per changed
// detector or reader, then a regroup). There is no single engine command for the lot,
// so it must never fail half-way in silence: run the steps in order, stop at the first
// failure, and say exactly what was saved and what was not. Pure, so node --test can
// pin it.

/** Plain label for a detector or reader switch key. */
export function featureLabel(key) {
  const words = String(key ?? '').replace(/[_-]+/g, ' ').trim();
  return words ? words.charAt(0).toUpperCase() + words.slice(1) : 'Setting';
}

const reasonOf = (error) => String((error && error.message) || error || 'Unknown error').replace(/^Error:\s*/, '').trim();

/**
 * Run `steps` ([{ label, run }]) in order and stop at the first failure.
 * Returns `{ saved, failed, notRun }`: labels of the steps that completed, the one
 * that threw (`{ label, message }`, or null), and the labels never started.
 */
export async function runSaveSteps(steps) {
  const list = Array.isArray(steps) ? steps : [];
  const saved = [];
  for (let i = 0; i < list.length; i += 1) {
    try {
      await list[i].run();
      saved.push(list[i].label);
    } catch (error) {
      return { saved, failed: { label: list[i].label, message: reasonOf(error) }, notRun: list.slice(i + 1).map((s) => s.label) };
    }
  }
  return { saved, failed: null, notRun: [] };
}

/** The one visible sentence for a save that did not finish; '' when everything saved. */
export function saveFailureMessage(outcome) {
  if (!outcome || !outcome.failed) return '';
  const { saved, failed, notRun } = outcome;
  const sentence = (text) => (/[.!?]$/.test(text) ? text : `${text}.`);
  const parts = [sentence(`${failed.label} could not be saved: ${failed.message}`)];
  parts.push(saved.length ? `Already saved: ${saved.join(', ')}.` : 'Nothing was changed.');
  if (notRun.length) parts.push(`Not applied: ${notRun.join(', ')}.`);
  return parts.join(' ');
}
