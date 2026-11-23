/**
 * Dev-mode chrome: the solid DEV chip, the live-reload chip and its Development
 * popover, the splash and Settings banners, and the live-reload state machine.
 * Pure data and pure helpers only — no React, no DOM reads — so every string
 * and every state is checkable with `node --test`. The React pieces
 * (DevStatus.tsx, TitleBar, Splash, Settings, App) consume these.
 *
 * Ground truth: conrod-notes/mocks/dev-mode/SPEC.md (exact strings, §8).
 */

export const DEV_HOME = '~/.conrod-dev';
export const PROD_HOME = '~/.conrod';
export const DEFAULT_URL = 'http://localhost:1420';

export const RELOAD_UI_LABEL = 'Reload UI';
export const FOLDER_LABEL = 'Open data folder';

/** The three live-reload states: chip class/label, popover state word, tooltip, UI-source line. */
export const RELOAD = {
  ok: {
    cls: 'ok', label: 'Live reload', state: 'Live reload', stateCls: 'done',
    tooltip: (url) => `Vite dev server connected at ${url} — edits reload automatically.`,
    ui: (url) => `Live reload · ${url}`,
  },
  busy: {
    cls: 'busy', label: 'Reconnecting…', state: 'Reconnecting…', stateCls: 'running',
    tooltip: (url) => `Lost the dev server at ${url} — retrying. The UI keeps working meanwhile.`,
    ui: (url) => `Reconnecting · ${url}`,
  },
  warn: {
    cls: 'warn', label: 'Bundled UI', state: 'Bundled UI', stateCls: 'paused',
    tooltip: (url) => `Dev server not running at ${url} — showing the bundled UI. Start it: npm --prefix frontend run dev`,
    ui: () => 'Bundled UI (frontend/dist)',
  },
};

export const DEV_CHIP_TOOLTIP = (home) => `Development build (--dev). Data home: ${home} — separate from production.`;
export const FALLBACK_TOAST = (url) => `Dev server not running at ${url} — showing the bundled UI.`;
export const RECONNECT_TOAST = (url) => `Live reload reconnected to ${url}.`;
export const SPLASH_STRIP = (home) => `Isolated instance — data stays in ${home}`;
export const SPLASH_VERSION = (version, dev) => `v${version}${dev ? ' · dev' : ''}`;
export const SETTINGS_BANNER = (home) => `Development instance — albums, settings and cache live in ${home}. Your production library (~/.conrod) is untouched.`;
export const POPOVER_NOTE = 'Started with --dev. Production Conrod keeps its library in ~/.conrod; this instance never reads or writes it.';
export const BUILD_LABEL = (version, commit) => `v${version}-dev${commit ? `+${commit}` : ''}`;

/** A real path ending in `.conrod-dev` displays as the friendly `~/.conrod-dev`. */
export function displayHome(path) {
  if (!path) return DEV_HOME;
  const norm = String(path).replace(/\\/g, '/').replace(/\/$/, '');
  return /(^|\/)\.conrod-dev$/.test(norm) ? DEV_HOME : norm;
}

/** `home` is the display string, `path` the real folder (for openPath). */
export function makeDevInfo({ home, path, url = DEFAULT_URL, version = '', commit = '' } = {}) {
  const real = path ?? home ?? DEV_HOME;
  return { home: home ?? displayHome(real), path: real, url, version, commit };
}

/**
 * The single source of truth for "is this a dev instance?". Returns null in
 * normal mode, so none of the dev chrome renders (pixel identity). Order:
 * an injected `window.__CONROD_DEV__`, the engine Bootstrap, then the browser
 * mock's `?dev=1` seed.
 * @param {{ injected?: { home: string; url: string; version: string; commit: string } | null,
 *           bootstrap?: { dev?: boolean; devUrl?: string; devHome?: string },
 *           params?: URLSearchParams, version?: string, mocked?: boolean }} [opts]
 */
export function resolveDevInfo({ injected, bootstrap = {}, params = new URLSearchParams(), version = '', mocked = false } = {}) {
  if (injected) return makeDevInfo({ path: injected.home, url: injected.url, version: injected.version, commit: injected.commit });
  const dev = Boolean(bootstrap.dev) || (mocked && params.has('dev'));
  if (!dev) return null;
  if (mocked) return makeDevInfo({ url: bootstrap.devUrl || DEFAULT_URL, version: version || '0.0.0-mock', commit: 'a1b2c3d' });
  return makeDevInfo({ path: bootstrap.devHome, url: bootstrap.devUrl || DEFAULT_URL, version, commit: '' });
}

/** The browser mock forces a state with `?devserver=up|down|reconnecting`. */
export function forcedReloadState(params) {
  if (!params) return null;
  switch (params.get('devserver')) {
    case 'up': return 'ok';
    case 'reconnecting': return 'busy';
    case 'down': return 'warn';
    default: return null;
  }
}

/** Vite served this page (import.meta.hot present) => ok; bundled dist => bundled-UI fallback. */
export function initialReloadState({ forced, hotPresent }) {
  if (forced) return forced;
  return hotPresent ? 'ok' : 'warn';
}

/** One plain toast at boot when there was never a dev server (bundled UI). */
export function bootToast(state, url) {
  return state === 'warn' ? { message: FALLBACK_TOAST(url), ms: 6500 } : null;
}

/** A reconnect (busy -> ok) raises one ok-tone toast; other transitions are silent. */
export function transitionToast(from, to, url) {
  return from === 'busy' && to === 'ok' ? { message: RECONNECT_TOAST(url), tone: 'ok' } : null;
}

/**
 * Normal mode renders none of the dev chrome. Both the DEV chip / hairline in
 * TitleBar and the splash strip are gated on this; with no dev info the DOM is
 * exactly today's, byte for byte.
 */
export const showDevChrome = (dev) => Boolean(dev);
