// Keyboard rules for a modal sheet, kept pure so node --test can pin them.

/** Elements a Tab press can land on inside a dialog. */
export const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), summary, [tabindex]:not([tabindex="-1"])';

/**
 * Where Tab should go inside a dialog with `count` focusable controls, given the
 * index that has focus now (-1 when focus is outside or on the dialog itself).
 * Returns the index to focus, or null to let the browser move focus normally
 * (it stays inside the dialog by itself in the middle of the list).
 */
export function trapTarget(count, current, shift) {
  if (count <= 0) return null;
  if (current < 0) return shift ? count - 1 : 0;
  if (shift && current === 0) return count - 1;
  if (!shift && current === count - 1) return 0;
  return null;
}

/**
 * The open sheets, newest last: only the one on top answers Esc and Tab, so a sheet
 * opened over another closes alone. Each entry is an opaque token.
 */
export function createSheetStack() {
  const open = [];
  return {
    push(token) { open.push(token); },
    remove(token) { const i = open.indexOf(token); if (i >= 0) open.splice(i, 1); },
    isTop(token) { return open.length > 0 && open[open.length - 1] === token; },
    get size() { return open.length; },
  };
}
