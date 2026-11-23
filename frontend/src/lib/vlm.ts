import { momentsStatus } from './aiCard.mjs';
import type { Settings, VlmService } from './types';

/** Every vision service the album has configured, enabled or not. */
function vlmServices(settings: Settings): VlmService[] {
  const value = settings.vlm_services;
  return Array.isArray(value) ? (value as unknown as VlmService[]) : [];
}

/**
 * The service that will actually answer: the first enabled one, as the engine
 * picks it. `undefined` when every service is off, so the UI shows nothing
 * rather than a legacy model name the user disabled.
 */
function primaryVlmService(settings: Settings): VlmService | undefined {
  return vlmServices(settings).find((service) => service.enabled);
}

/** The enabled service's model, for a label; `undefined` when none is on. */
export function vlmModelLabel(settings: Settings): string | undefined {
  return primaryVlmService(settings)?.model;
}

/**
 * The cloud decision service actually serving the moments job, in plain words
 * (e.g. "jev-1.13 via OpenCode"), or null when the local built-in model is the
 * only path. The live screens pass this to the DECIDER node so it names the
 * service that is doing the work instead of the local model's install state.
 */
export function decisionServiceLabel(settings: Settings): string | null {
  const ms = momentsStatus(settings) as { path: string; label: string };
  return ms.path === 'cloud' ? ms.label : null;
}
