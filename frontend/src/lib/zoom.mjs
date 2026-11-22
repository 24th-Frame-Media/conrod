// Zoom geometry for the photo viewer: how a photo fits the stage, how far it may pan, where a click
// lands when the view zooms, and what shape the stage takes. Pure, so node tests reach the same numbers
// the UI shows.
//
// Everything here is about the PHOTO, not the frame: a tall photo keeps its own shape, so zooming it
// never crops it to a wide box.

/**
 * The photo's size inside the stage (fit-contained, so the whole frame stays visible) and the scale that
 * shows it at its own pixels (1:1).
 */
export function fitScaleAndSize(containerW, containerH, naturalW, naturalH) {
  if (!containerW || !containerH || !naturalW || !naturalH) {
    return { scale1to1: 2.5, imageW: containerW || 800, imageH: containerH || 600 };
  }
  const imgAspect = naturalW / naturalH;
  const contAspect = containerW / containerH;
  let imageW = containerW;
  let imageH = containerH;
  if (imgAspect > contAspect) {
    imageW = containerW;
    imageH = containerW / imgAspect;
  } else {
    imageH = containerH;
    imageW = containerH * imgAspect;
  }
  const scale1to1 = Math.max(1, naturalW / imageW);
  return { scale1to1, imageW, imageH };
}

/**
 * The shape the stage takes: the photo's own shape once the file is known, else the frame's recorded one.
 * A tall photo in a wide stage would be cropped to the wide box as soon as it is zoomed.
 */
export function stageRatio(frameW, frameH, naturalW, naturalH) {
  if (naturalW > 0 && naturalH > 0) return naturalW / naturalH;
  if (frameW > 0 && frameH > 0) return frameW / frameH;
  return 3 / 2;
}

/** How far the photo may move from the middle: its own edges stop at the stage's edges. */
export function clampPan(val, scale, imageDimension, containerDimension) {
  const max = Math.max(0, (imageDimension * scale - containerDimension) / 2);
  return Math.max(-max, Math.min(max, val));
}

/**
 * The view after a zoom: the photo keeps the point under the cursor (or the cursor's point moves to the
 * middle on a click), and the photo never slides past its own edges.
 *
 * `offsetX`/`offsetY` are the pointer's distance from the middle of the stage, in stage pixels; `view` is
 * the current `{ scale, x, y }`; `imageW`/`imageH` are the photo's size at 1:1 with the stage.
 */
export function zoomToPoint({ offsetX, offsetY, view, scale, imageW, imageH, containerW, containerH, centre }) {
  let targetX = 0;
  let targetY = 0;
  if (offsetX != null && offsetY != null) {
    // The point under the cursor, in the photo's own pixels.
    const imgX = (offsetX - view.x) / view.scale;
    const imgY = (offsetY - view.y) / view.scale;
    if (centre || view.scale <= 1.01) {
      // A click zooms to that point and puts it in the middle.
      targetX = -imgX * scale;
      targetY = -imgY * scale;
    } else {
      // A wheel keeps the point under the cursor where it is.
      targetX = offsetX - imgX * scale;
      targetY = offsetY - imgY * scale;
    }
  } else {
    targetX = (view.x / view.scale) * scale;
    targetY = (view.y / view.scale) * scale;
  }
  return {
    scale,
    x: clampPan(targetX, scale, imageW, containerW),
    y: clampPan(targetY, scale, imageH, containerH),
  };
}

/** Scale and pan that put the middle of `box` (a fraction of the photo) in the middle of the stage. */
export function focusView(box, containerW, containerH, imageW, imageH, scale1to1) {
  const [x1, y1, x2, y2] = box;
  const cx = (x1 + x2) / 2;
  const cy = (y1 + y2) / 2;
  const bw = Math.max(0.01, x2 - x1);
  const bh = Math.max(0.01, y2 - y1);
  const fitSubjectScale = 0.75 / Math.max(bw, bh);
  const scale = Math.max(scale1to1, Math.min(scale1to1 * 2, fitSubjectScale));
  const rawX = -(cx - 0.5) * imageW * scale;
  const rawY = -(cy - 0.5) * imageH * scale;
  return { scale, x: clampPan(rawX, scale, imageW, containerW), y: clampPan(rawY, scale, imageH, containerH) };
}
