// Who owns a key press: the window's shortcuts or the control that has focus.
// Pure (it only asks the target `matches` / `closest`), so node --test can pin it.

/** Controls that press themselves on Enter and Space. */
const PRESSABLE = 'button, a[href], summary, [role="button"], [role="tab"], [role="switch"], [role="menuitem"], [role="checkbox"], [role="radio"]';
/** Controls that take typing or letters for themselves. */
const TYPING = 'input, textarea, select, [contenteditable=""], [contenteditable="true"]';
/** Chrome around the grid whose buttons must keep Enter and Space: the toolbar, dialogs,
 *  the Inspector and the viewer side panel, zoom tools and the stage pills. A button inside
 *  the photo grid itself is not chrome: Enter there still opens the burst. */
const CHROME = '.toolbar, dialog, .inspector, .frame-side, .zoom-tools, .stage-pill-btn, .facets';

const closest = (target, selector) => Boolean(target && typeof target.closest === 'function' && target.closest(selector));

/** Enter and Space belong to a focused button in the toolbar, a dialog, the Inspector, the viewer panel or the zoom tools. */
export function ownsActivationKeys(target) {
  return closest(target, CHROME) && closest(target, PRESSABLE);
}

/**
 * Whether Viewer / Compare should leave this key to the focused control.
 * Typing fields and selects keep every key. A pressable control (button, link,
 * summary, switch) keeps Space and Enter so they press it instead of zooming.
 */
export function zoomKeyIgnored(target, key) {
  if (closest(target, TYPING)) return true;
  return (key === ' ' || key === 'Enter') && closest(target, PRESSABLE);
}
