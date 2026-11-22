/** The photo's fitted size in the stage and the scale that shows it at its own pixels (1:1). */
export function fitScaleAndSize(
  containerW: number,
  containerH: number,
  naturalW: number,
  naturalH: number,
): { scale1to1: number; imageW: number; imageH: number };

/** The shape the stage takes: the photo's own shape once known, else the frame's recorded one. */
export function stageRatio(frameW: number, frameH: number, naturalW: number, naturalH: number): number;

/** How far the photo may move from the middle: its own edges stop at the stage's edges. */
export function clampPan(val: number, scale: number, imageDimension: number, containerDimension: number): number;

/** The view after a zoom: the point under the cursor stays put, and the photo never slides off its edges. */
export function zoomToPoint(args: {
  offsetX?: number | null;
  offsetY?: number | null;
  view: { scale: number; x: number; y: number };
  scale: number;
  imageW: number;
  imageH: number;
  containerW: number;
  containerH: number;
  centre?: boolean;
}): { scale: number; x: number; y: number };

/** Scale and pan that put the middle of `box` (a fraction of the photo) in the middle of the stage. */
export function focusView(
  box: [number, number, number, number],
  containerW: number,
  containerH: number,
  imageW: number,
  imageH: number,
  scale1to1: number,
): { scale: number; x: number; y: number };
