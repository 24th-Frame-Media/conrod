// Viewer helpers: the camera's autofocus area as a fractional box and the label the inspector shows.
// Pure, so node tests reach the same rounding the UI shows. The payload stores fractions of the upright photo.

/** The frame's AF box as 0..1 of the photo, or null when the payload carries none. */
export function afBox(frame) {
  if (!frame) return null;
  const { af_x1: x1, af_y1: y1, af_x2: x2, af_y2: y2 } = frame;
  if (x1 == null || y1 == null || x2 == null || y2 == null) return null;
  if (![x1, y1, x2, y2].every((v) => Number.isFinite(v))) return null;
  const c = (v) => Math.max(0, Math.min(1, v));
  const box = [c(x1), c(y1), c(x2), c(y2)];
  // An inverted or empty box is not an area worth drawing.
  return box[2] > box[0] && box[3] > box[1] ? box : null;
}

/** Where the camera focused, as the AF box's centre in rounded percent: "56% × 58%", or null. */
export function afLabel(frame) {
  const box = afBox(frame);
  if (!box) return null;
  const x = Math.round(((box[0] + box[2]) / 2) * 100);
  const y = Math.round(((box[1] + box[3]) / 2) * 100);
  return `${x}% × ${y}%`;
}
