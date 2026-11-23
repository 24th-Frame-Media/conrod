/**
 * Dev mode's UI strings, kept in one plain module so the badge, the window
 * title and the "dev server not running" line are testable without a browser.
 * The flag itself comes from the engine's Bootstrap (`dev`), which the app sets
 * with `--dev` or `CONROD_DEV=1`.
 */

/** The window title suffix and badge word. */
export const DEV_TITLE = '[Dev]';
export const DEV_BADGE = 'DEV';

/** `?mock=1` runs the browser mock; `?dev=1` also does, as a dev-mode seed. */
export function mockSeed(params) {
  return params.has('mock') || params.has('dev');
}

/** The document title: `Conrod` in a release, `Conrod [Dev]` in dev mode. */
export function windowTitle(base, dev) {
  return dev ? `${base} ${DEV_TITLE}` : base;
}

/** What the UI says when dev mode is on but the Vite server is not up. */
export function devServerNotice(dev, reachable, url) {
  return dev && !reachable ? `Dev server not running at ${url}` : null;
}
