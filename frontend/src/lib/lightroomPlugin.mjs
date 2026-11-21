/*
 * The Lightroom plug-in card's words. Pure, so `node --test` imports them
 * without a DOM: the one-line status under the toggle.
 */

const version = (text) => (typeof text === 'string' && text.trim() !== '' ? text.trim() : null);

/**
 * "Installed v1.0.0 — restart Lightroom to load it", "Lightroom is open:
 * restart it to pick up the update", "Not installed", or the engine's own error.
 */
export function pluginStatusLine(status) {
  if (!status || typeof status !== 'object') return 'Checking…';
  if (status.error) return String(status.error);
  if (!status.installed) return 'Not installed';
  const tag = version(status.version);
  const installed = tag ? `Installed v${tag}` : 'Installed';
  if (status.running && status.needsUpdate) {
    return 'Lightroom is open: restart it to pick up the update';
  }
  if (status.running) return `${installed} — restart Lightroom to load it`;
  return installed;
}

/** The card's plain-language note about which version this build carries. */
export function pluginBundleLine(status) {
  if (!status || typeof status !== 'object') return null;
  if (!status.bundled) return 'This build does not carry the plug-in.';
  const tag = version(status.bundledVersion);
  return tag ? `This build carries v${tag}.` : null;
}
