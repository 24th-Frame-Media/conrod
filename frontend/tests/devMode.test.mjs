import test from 'node:test';
import assert from 'node:assert/strict';
import { DEV_BADGE, DEV_TITLE, devServerNotice, mockSeed, windowTitle } from '../src/lib/devMode.mjs';

const params = (search) => new URLSearchParams(search);

test('the window title only carries the dev suffix in dev mode', () => {
  assert.equal(windowTitle('Conrod', false), 'Conrod');
  assert.equal(windowTitle('Conrod', true), `Conrod ${DEV_TITLE}`);
  assert.equal(DEV_TITLE, '[Dev]');
  assert.equal(DEV_BADGE, 'DEV');
});

test('the mock seed is ?mock=1 and the ?dev=1 dev seed', () => {
  assert.equal(mockSeed(params('')), false);
  assert.equal(mockSeed(params('?mock=1')), true);
  assert.equal(mockSeed(params('?dev=1')), true);
});

test('the dev server line only shows when dev mode fell back to the bundled UI', () => {
  assert.equal(devServerNotice(false, false, 'http://localhost:1420'), null);
  assert.equal(devServerNotice(true, true, 'http://localhost:1420'), null);
  assert.equal(
    devServerNotice(true, false, 'http://localhost:1420'),
    'Dev server not running at http://localhost:1420',
  );
});
