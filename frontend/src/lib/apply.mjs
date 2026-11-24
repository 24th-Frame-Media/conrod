/*
 * What Conrod applies to an album without being asked, in Review's words.
 * Pure, so `node --test` imports it without a DOM.
 */

const colour = (label) => {
  const name = typeof label === 'string' ? label.trim() : '';
  return name === '' || name.toLowerCase() === 'none' ? null : name;
};

/** "Stars and Blue picks are applied when the scan finishes." and its two siblings. */
export function applyLine({ autoApply, status, pickLabel } = {}) {
  if (autoApply === false) {
    return 'Stars and picks are not applied automatically';
  }
  const name = colour(pickLabel);
  const what = name ? `Stars and ${name} picks` : 'Stars';
  if (status === 'done') return `${what} applied to the files`;
  if (status === 'done_errors') return `${what} are applied when the frames that failed are read`;
  return `${what} are applied when the scan finishes`;
}
