import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

// Worker cards (scan lanes, identify lanes, the decider) are quiet: the picture shows only the photo, the scan line and the box
// outlines. No "Live" pill, no file-name chip and no text on the image; the file name sits in the card's info area.
const SRC = join(dirname(fileURLToPath(import.meta.url)), '..', 'src');
const read = (rel) => readFileSync(join(SRC, rel), 'utf8');
const CARDS = ['components/IngestLane.tsx', 'components/IdentifyParts.tsx', 'components/DeciderLane.tsx'];

test('no worker card carries a "Live" pill', () => {
  for (const file of CARDS) {
    const src = read(file);
    assert.ok(!/>\s*Live\s*</.test(src) && !/['"`]Live['"`]/.test(src), `${file} still has a Live pill`);
    assert.ok(!/className=\{?[`"'][^`"']*\bid-badge\b[^`"']*\blive\b/.test(src), `${file} still styles a live badge`);
  }
  assert.ok(!/\.id-badge\.live/.test(read('styles/screens.css')), 'the live badge style is gone');
});

test('the file name is not chipped onto a worker card picture', () => {
  for (const file of CARDS) {
    const src = read(file);
    // The full-page detail views (IdentifyDetail, FrameDetail) are not worker cards.
    const lane = src.split(/export function (?:IdentifyDetail|FrameDetail)/)[0];
    assert.ok(!/className="id-file"/.test(lane), `${file} puts the file name on the picture`);
    assert.ok(/className="ing-file /.test(lane), `${file} shows the file name in the info area`);
  }
});

test('box outlines on a worker card carry no text unless the detail page asks for labels', () => {
  const src = read('components/IngestLane.tsx');
  assert.ok(/labels = false/.test(src), 'labels are off by default');
  assert.ok(/labels && tagFits/.test(src), 'a box tag needs labels');
  assert.ok(/labels && afStyle/.test(src), 'the AF tag needs labels');
  assert.equal((src.match(/<FrameStage /g) ?? []).length, 2);
  assert.ok(/<FrameStage [^>]*labels \/>/.test(src), 'the full-page detail keeps its labels');
  assert.ok(!/<FrameStage [^>]*frame\.boxes[^>]*labels/.test(src), 'the lane does not');
});

test('the stage strip never prints a truncated word', () => {
  const src = read('components/IngestLane.tsx');
  assert.ok(!/SCAN_STEPS\.map/.test(src), 'the lane does not print each step name in a cramped cell');
  assert.ok(/ing-steps-label/.test(src));
});
