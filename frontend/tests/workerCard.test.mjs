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
// Layout regressions: the Decider card once took the review grid's global `.card` (position: absolute) and sat on top of Worker 02.
test('the decider card does not borrow the review grid\'s global .card class', () => {
  const src = read('components/DeciderLane.tsx');
  assert.ok(!/decider-lane \$\{layout\}/.test(src), 'the layout word must not be put on the lane as a bare class');
  assert.ok(/layout === 'card' \? 'dec-card'/.test(src));
  const screens = read('styles/screens.css');
  assert.ok(/^\.card \{ position: absolute/m.test(screens), 'the premise: .card is absolutely positioned');
});

test('a worker card never grows past its grid cell, and "+N more" never sits over a card', () => {
  const css = read('styles/ingest.css');
  assert.ok(/^\.id-lane \{ grid-template-columns: minmax\(0, 1fr\); \}/m.test(css), 'a lane\'s own column is capped at its width');
  const pill = css.match(/\.ing-overflow-pill \{[^}]*\}/)[0];
  assert.ok(!/position:\s*absolute/.test(pill), 'the pill sits in its own row under the list');
  assert.ok(!/ing-overflow-fade/.test(css) && !/ing-overflow-fade/.test(read('screens/Ingest.tsx')), 'no fade veil over the cards');
  assert.ok(/\.ing-main, \.scan-rail \{ min-height: auto; \}/.test(css), 'stacked blocks keep their height instead of sliding under the next');
});
