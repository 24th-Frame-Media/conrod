import test from 'node:test';
import assert from 'node:assert/strict';
import {
  DEV_HOME, DEFAULT_URL, RELOAD, DEV_CHIP_TOOLTIP, FALLBACK_TOAST, RECONNECT_TOAST,
  SPLASH_STRIP, SPLASH_VERSION, SETTINGS_BANNER, POPOVER_NOTE, BUILD_LABEL,
  RELOAD_UI_LABEL, FOLDER_LABEL, displayHome, makeDevInfo, resolveDevInfo,
  forcedReloadState, initialReloadState, bootToast, transitionToast, showDevChrome,
} from '../src/lib/devInfo.mjs';

const URL = DEFAULT_URL;
const params = (search) => new URLSearchParams(search);

// ---- dev info: normal mode stays absent (pixel identity), dev carries the home ----

test('displayHome shows the friendly ~/.conrod-dev and keeps custom homes', () => {
  assert.equal(displayHome('C:/Users/kapsikkum/.conrod-dev'), DEV_HOME);
  assert.equal(displayHome('C:\\Users\\kapsikkum\\.conrod-dev'), DEV_HOME);
  assert.equal(displayHome('D:/work/my-dev-home'), 'D:/work/my-dev-home');
  assert.equal(displayHome(''), DEV_HOME);
  assert.equal(displayHome(undefined), DEV_HOME);
});

test('makeDevInfo separates the display home from the real openPath target', () => {
  const d = makeDevInfo({ path: 'C:/Users/kapsikkum/.conrod-dev', url: URL, version: '1.0.0', commit: 'abc1234' });
  assert.deepEqual(d, {
    home: '~/.conrod-dev', path: 'C:/Users/kapsikkum/.conrod-dev', url: URL, version: '1.0.0', commit: 'abc1234',
  });
});

test('resolveDevInfo is null in normal mode (no badge, no hairline)', () => {
  assert.equal(resolveDevInfo({ bootstrap: { dev: false }, params: params(''), mocked: false }), null);
  assert.equal(resolveDevInfo({ bootstrap: {}, params: params('mock=1'), mocked: true }), null);
  assert.equal(showDevChrome(null), false);
});

test('resolveDevInfo reads the engine Bootstrap in the real dev app', () => {
  const d = resolveDevInfo({ bootstrap: { dev: true, devUrl: URL, devHome: 'C:/Users/k/.conrod-dev' }, params: params(''), version: '1.0.0', mocked: false });
  assert.equal(d.home, '~/.conrod-dev');
  assert.equal(d.path, 'C:/Users/k/.conrod-dev');
  assert.equal(d.url, URL);
  assert.equal(d.version, '1.0.0');
  assert.equal(showDevChrome(d), true);
});

test('resolveDevInfo synthesises the browser mock from ?dev=1 (and the init script wins)', () => {
  const mock = resolveDevInfo({ bootstrap: {}, params: params('mock=1&dev=1'), version: '0.0.0-mock', mocked: true });
  assert.equal(mock.home, '~/.conrod-dev');
  assert.equal(mock.url, URL);
  assert.equal(mock.commit, 'a1b2c3d');
  const injected = resolveDevInfo({ injected: { home: 'D:/dev/.conrod-dev', url: 'http://localhost:5173', version: '9', commit: 'fffffff' }, bootstrap: { dev: false }, params: params(''), mocked: false });
  assert.equal(injected.path, 'D:/dev/.conrod-dev');
  assert.equal(injected.url, 'http://localhost:5173');
});

// ---- chip states: exact classes, labels, tooltips and popover source lines ----

test('chip state "ok" = green steady Live reload', () => {
  assert.equal(RELOAD.ok.cls, 'ok');
  assert.equal(RELOAD.ok.label, 'Live reload');
  assert.equal(RELOAD.ok.state, 'Live reload');
  assert.equal(RELOAD.ok.stateCls, 'done');
  assert.equal(RELOAD.ok.tooltip(URL), 'Vite dev server connected at http://localhost:1420 — edits reload automatically.');
  assert.equal(RELOAD.ok.ui(URL), 'Live reload · http://localhost:1420');
});

test('chip state "busy" = teal breathing Reconnecting…', () => {
  assert.equal(RELOAD.busy.cls, 'busy');
  assert.equal(RELOAD.busy.label, 'Reconnecting…');
  assert.equal(RELOAD.busy.stateCls, 'running');
  assert.equal(RELOAD.busy.tooltip(URL), 'Lost the dev server at http://localhost:1420 — retrying. The UI keeps working meanwhile.');
  assert.equal(RELOAD.busy.ui(URL), 'Reconnecting · http://localhost:1420');
});

test('chip state "warn" = amber steady Bundled UI', () => {
  assert.equal(RELOAD.warn.cls, 'warn');
  assert.equal(RELOAD.warn.label, 'Bundled UI');
  assert.equal(RELOAD.warn.stateCls, 'paused');
  assert.equal(RELOAD.warn.tooltip(URL), 'Dev server not running at http://localhost:1420 — showing the bundled UI. Start it: npm --prefix frontend run dev');
  assert.equal(RELOAD.warn.ui(), 'Bundled UI (frontend/dist)');
});

// ---- popover / splash / banner strings ----

test('the Development popover reads Build, UI source, Data home and its note', () => {
  assert.equal(BUILD_LABEL('1.0.0-beta.8', 'a1b2c3d'), 'v1.0.0-beta.8-dev+a1b2c3d');
  assert.equal(BUILD_LABEL('1.0.0-beta.8', ''), 'v1.0.0-beta.8-dev');
  assert.equal(POPOVER_NOTE, 'Started with --dev. Production Conrod keeps its library in ~/.conrod; this instance never reads or writes it.');
  assert.equal(RELOAD_UI_LABEL, 'Reload UI');
  assert.equal(FOLDER_LABEL, 'Open data folder');
  assert.equal(DEV_CHIP_TOOLTIP('~/.conrod-dev'), 'Development build (--dev). Data home: ~/.conrod-dev — separate from production.');
});

test('the splash gains · dev and the isolated data-home strip only in dev', () => {
  assert.equal(SPLASH_VERSION('1.0.0-beta.8', true), 'v1.0.0-beta.8 · dev');
  assert.equal(SPLASH_VERSION('1.0.0-beta.8', false), 'v1.0.0-beta.8');
  assert.equal(SPLASH_STRIP('~/.conrod-dev'), 'Isolated instance — data stays in ~/.conrod-dev');
});

test('the Settings banner names the dev home and reassures about production', () => {
  assert.equal(
    SETTINGS_BANNER('~/.conrod-dev'),
    'Development instance — albums, settings and cache live in ~/.conrod-dev. Your production library (~/.conrod) is untouched.',
  );
});

// ---- mock seeds + state machine + transition toasts ----

test('?devserver=up|down|reconnecting forces the three chip states', () => {
  assert.equal(forcedReloadState(params('dev=1&devserver=up')), 'ok');
  assert.equal(forcedReloadState(params('dev=1&devserver=down')), 'warn');
  assert.equal(forcedReloadState(params('dev=1&devserver=reconnecting')), 'busy');
  assert.equal(forcedReloadState(params('dev=1')), null);
  assert.equal(forcedReloadState(null), null);
});

test('boot state is ok under Vite and Bundled UI when serving frontend/dist', () => {
  assert.equal(initialReloadState({ forced: null, hotPresent: true }), 'ok');
  assert.equal(initialReloadState({ forced: null, hotPresent: false }), 'warn');
  assert.equal(initialReloadState({ forced: 'busy', hotPresent: true }), 'busy');
});

test('one plain 6500 ms fallback toast at boot and one ok toast on reconnect', () => {
  assert.deepEqual(bootToast('warn', URL), { message: 'Dev server not running at http://localhost:1420 — showing the bundled UI.', ms: 6500 });
  assert.equal(bootToast('ok', URL), null);
  assert.deepEqual(transitionToast('busy', 'ok', URL), { message: 'Live reload reconnected to http://localhost:1420.', tone: 'ok' });
  assert.equal(transitionToast('ok', 'busy', URL), null);
  assert.equal(FALLBACK_TOAST(URL), 'Dev server not running at http://localhost:1420 — showing the bundled UI.');
  assert.equal(RECONNECT_TOAST(URL), 'Live reload reconnected to http://localhost:1420.');
});

// ---- acceptance: normal mode renders none of the extra chrome ----

test('normal mode is pixel-identical: no dev info, no suffix, no badge', () => {
  const dev = resolveDevInfo({ bootstrap: { dev: false, devUrl: URL }, params: params(''), version: '1.0.0', mocked: false });
  assert.equal(dev, null);
  assert.equal(showDevChrome(dev), false);
  assert.equal(SPLASH_VERSION('1.0.0', false), 'v1.0.0');
});
