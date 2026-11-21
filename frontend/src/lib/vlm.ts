import type { Settings, VlmService } from './types';

/** Every vision service the album has configured, enabled or not. */
export function vlmServices(settings: Settings): VlmService[] {
  const value = settings.vlm_services;
  return Array.isArray(value) ? (value as unknown as VlmService[]) : [];
}

/**
 * The service that will actually answer: the first enabled one, as the engine
 * picks it. `undefined` when every service is off, so the UI shows nothing
 * rather than a legacy model name the user disabled.
 */
export function primaryVlmService(settings: Settings): VlmService | undefined {
  return vlmServices(settings).find((service) => service.enabled);
}

/** The enabled service's model, for a label; `undefined` when none is on. */
export function vlmModelLabel(settings: Settings): string | undefined {
  return primaryVlmService(settings)?.model;
}
