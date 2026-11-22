import test from 'node:test';
import assert from 'node:assert/strict';
import { clampPan, fitScaleAndSize, focusView, stageRatio, zoomToPoint } from '../src/lib/zoom.mjs';

/** Where a point of the photo (in photo pixels, from the photo's middle) sits in the stage. */
const onStage = (imgX, imgY, view) => ({
  x: view.x + imgX * view.scale,
  y: view.y + imgY * view.scale,
});

test('a photo is fitted inside the stage, whole, whatever its shape', () => {
  // A tall photo in a wide stage: contained by its height, so the whole frame is still visible.
  const tall = fitScaleAndSize(900, 600, 800, 1200);
  assert.equal(tall.imageH, 600);
  assert.equal(tall.imageW, 400);
  assert.equal(tall.scale1to1, 2);
  // A wide photo in a wide stage: contained by its width.
  const wide = fitScaleAndSize(900, 600, 1800, 1200);
  assert.equal(wide.imageW, 900);
  assert.equal(wide.imageH, 600);
  assert.equal(wide.scale1to1, 2);
  // Nothing measured yet: a sane placeholder, never a divide by zero.
  const none = fitScaleAndSize(0, 0, 0, 0);
  assert.equal(none.imageW, 800);
  assert.equal(none.imageH, 600);
});

test("the stage takes the photo's own shape, not the recorded one", () => {
  // A tall photo whose recorded size is the camera's: the stage follows the photo, or zooming it
  // would crop it to the wide box.
  assert.equal(stageRatio(6000, 4000, 800, 1200), 800 / 1200);
  // No file yet: the recorded shape, and 3:2 when there is not one either.
  assert.equal(stageRatio(4000, 6000, 0, 0), 4000 / 6000);
  assert.equal(stageRatio(0, 0, 0, 0), 1.5);
});

test('the photo stops at its own edges', () => {
  // Wider than the stage: it may move until its edge reaches the stage's edge.
  assert.equal(clampPan(999, 2, 900, 600), 600);
  assert.equal(clampPan(-999, 2, 900, 600), -600);
  // Narrower than the stage: it stays in the middle.
  assert.equal(clampPan(999, 2, 400, 900), 0);
  assert.ok(Math.abs(clampPan(-999, 2, 400, 900)) < 1e-9);
});

test('a click zooms to that point and puts it in the middle', () => {
  const view = { scale: 1, x: 0, y: 0 };
  const zoomed = zoomToPoint({
    offsetX: -100, offsetY: -150, view, scale: 2,
    imageW: 900, imageH: 600, containerW: 900, containerH: 600, centre: true,
  });
  // The point under the cursor ends up in the middle: the photo moves by half as much again.
  assert.equal(zoomed.x, 200);
  assert.equal(zoomed.y, 300);
  const after = onStage(-100, -150, zoomed);
  assert.equal(after.x, 0);
  assert.equal(after.y, 0);
});

test('a tall photo in a wide stage stays whole, and only its tall edges move', () => {
  const view = { scale: 1, x: 0, y: 0 };
  const zoomed = zoomToPoint({
    offsetX: -100, offsetY: -150, view, scale: 2,
    imageW: 400, imageH: 600, containerW: 900, containerH: 600, centre: true,
  });
  // The photo is narrower than the stage, so it cannot slide sideways: it stays in the middle.
  assert.equal(zoomed.x, 0);
  // It is taller than the stage, so it slides up to the point under the cursor.
  assert.equal(zoomed.y, 300);
});

test('a wheel keeps the point under the cursor where it is', () => {
  const view = { scale: 2, x: 0, y: 0 };
  const zoomed = zoomToPoint({
    offsetX: -100, offsetY: -150, view, scale: 4,
    imageW: 900, imageH: 600, containerW: 900, containerH: 600,
  });
  // The photo moves by the extra scale only: the point stays under the cursor.
  assert.equal(zoomed.x, 100);
  assert.equal(zoomed.y, 150);
  const after = onStage(-50, -75, zoomed);
  assert.equal(after.x, -100);
  assert.equal(after.y, -150);
});

test('the detection box lands in the middle of the stage', () => {
  // A car on the left of a tall photo: the photo slides right to bring the box to the middle.
  const view = focusView([0.1, 0.3, 0.3, 0.5], 533, 800, 533, 800, 1.5);
  assert.ok(view.scale >= 1.5);
  assert.ok(view.x > 0);
  assert.ok(view.y > 0);
  // The box's centre is the stage's middle.
  assert.ok(Math.abs(view.x + (0.2 - 0.5) * 533 * view.scale) < 0.001);
  assert.ok(Math.abs(view.y + (0.4 - 0.5) * 800 * view.scale) < 0.001);
});
