import test from 'node:test';
import assert from 'node:assert/strict';
import { pluginBundleLine, pluginStatusLine } from '../src/lib/lightroomPlugin.mjs';

test('an installed plug-in says which version, and asks for a restart only while Lightroom is open', () => {
  assert.equal(pluginStatusLine(null), 'Checking…');
  assert.equal(pluginStatusLine({ installed: false }), 'Not installed');
  assert.equal(pluginStatusLine({ installed: true, version: '1.0.0' }), 'Installed v1.0.0');
  assert.equal(
    pluginStatusLine({ installed: true, version: '1.0.0', running: true }),
    'Installed v1.0.0 — restart Lightroom to load it',
  );
  assert.equal(
    pluginStatusLine({ installed: true, version: '1.0.0', running: true, needsUpdate: true }),
    'Lightroom is open: restart it to pick up the update',
  );
  assert.equal(
    pluginStatusLine({ installed: true, version: '0.9.0', running: false, needsUpdate: true }),
    'Installed v0.9.0',
    'a closed Lightroom picks the update up at its next start',
  );
  assert.equal(pluginStatusLine({ installed: true, version: null }), 'Installed');
});

test('an engine error is shown in its own words, and the bundle line names the version this build carries', () => {
  assert.equal(pluginStatusLine({ error: 'cannot write C:\\Modules' }), 'cannot write C:\\Modules');
  assert.equal(pluginBundleLine({ bundled: true, bundledVersion: '1.0.0-beta.8' }), 'This build carries v1.0.0-beta.8.');
  assert.equal(pluginBundleLine({ bundled: false }), 'This build does not carry the plug-in.');
  assert.equal(pluginBundleLine(null), null);
});
