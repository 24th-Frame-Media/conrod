// One placement rule for every anchored popup in the app (AI card menus,
// Review/Identify action menus, the TitleBar activity popover). Static
// `top: calc(100% + …)` / `left|right: 0` rules only ever opened a menu down
// and to one side, so a menu near a window edge was clipped. Measure the
// anchor and the menu against the window, prefer below, flip above when they
// would cross the bottom edge, flip/right-align when they would cross the
// right edge, then clamp. Fixed positioning is required: the Settings pane
// and the inspector scroll in their own containers, so absolute coordinates
// would drift with them.

export const MENU_GAP = 5;
export const MENU_MARGIN = 8;

/** `align`: how the menu lines up with its anchor when there is room.
 *  'left' = left edges, 'center' = centred, 'right' = right edges. */
export function placeAnchoredMenu(anchor, menu, align = 'left') {
  const a = anchor.getBoundingClientRect();
  // Switch to fixed *before* measuring: an absolute + `right: 0` menu is
  // shrink-to-fit against its anchor's (narrow) containing block, so it
  // would otherwise be measured at its min-width and stay too narrow. Reset
  // any previous clamp so each placement uses the natural size.
  menu.style.width = '';
  menu.style.position = 'fixed';
  menu.style.top = '0px';
  menu.style.left = '0px';
  menu.style.right = 'auto';
  menu.style.maxHeight = 'none';
  menu.style.overflowY = 'visible';
  const m = menu.getBoundingClientRect();
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const maxH = Math.max(48, vh - 2 * MENU_MARGIN);
  const w = m.width;
  const h = Math.min(m.height, maxH);

  const below = a.bottom + MENU_GAP;
  const above = a.top - MENU_GAP - h;
  let top = below + h <= vh - MENU_MARGIN ? below : (above >= MENU_MARGIN ? above : below);

  let left = align === 'center' ? a.left + a.width / 2 - w / 2
    : align === 'right' ? a.right - w
      : a.left;
  if (left + w > vw - MENU_MARGIN) {
    const flipped = a.right - w; // right-align to the anchor instead
    if (flipped >= MENU_MARGIN) left = flipped;
  }

  top = Math.max(MENU_MARGIN, Math.min(top, vh - MENU_MARGIN - h));
  left = Math.max(MENU_MARGIN, Math.min(left, Math.max(MENU_MARGIN, vw - MENU_MARGIN - w)));

  menu.style.position = 'fixed';
  menu.style.top = `${Math.round(top)}px`;
  menu.style.left = `${Math.round(left)}px`;
  menu.style.right = 'auto';
  menu.style.maxHeight = `${Math.round(maxH)}px`;
  menu.style.overflowY = m.height > maxH ? 'auto' : 'visible';
  menu.style.visibility = 'visible';
}
