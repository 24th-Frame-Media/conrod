export type LightroomPluginState = {
  installed?: boolean;
  version?: string | null;
  bundled?: boolean;
  bundledVersion?: string | null;
  needsUpdate?: boolean;
  modules?: string;
  path?: string;
  running?: boolean;
  enabled?: boolean;
  error?: string;
};

/** The one-line status under the Lightroom Classic toggle. */
export function pluginStatusLine(status: LightroomPluginState | null | undefined): string;

/** "This build carries v1.0.0.", or null when it carries nothing. */
export function pluginBundleLine(status: LightroomPluginState | null | undefined): string | null;
