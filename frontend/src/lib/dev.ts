import { useEffect, useMemo, useRef, useState } from 'react';
import { useToast } from '../state/toast';
import {
  resolveDevInfo, forcedReloadState, initialReloadState, bootToast, transitionToast,
} from './devInfo.mjs';
import type { DevInfo, ReloadState } from './types';

type DevOpts = { dev: boolean; devUrl: string; devHome: string; version: string; mocked: boolean };

/** The dev instance's info, or null in normal mode. Stable across renders. */
export function useDevInfo(opts: DevOpts): DevInfo | null {
  const { dev, devUrl, devHome, version, mocked } = opts;
  return useMemo(() => {
    const injected = typeof window !== 'undefined'
      ? (window as unknown as { __CONROD_DEV__?: { home: string; url: string; version: string; commit: string } }).__CONROD_DEV__
      : undefined;
    return resolveDevInfo({
      injected,
      bootstrap: { dev, devUrl, devHome },
      params: typeof location !== 'undefined' ? new URLSearchParams(location.search) : new URLSearchParams(),
      version,
      mocked,
    });
  }, [dev, devUrl, devHome, version, mocked]);
}

/**
 * Live-reload state: green steady (Vite connected), teal breathing (Vite
 * retrying), amber steady (fell back to the bundled UI). The Vite client owns
 * reconnection; we only listen to its websocket events and raise one toast per
 * transition. The browser mock can force a state with `?devserver=…`.
 */
export function useReloadState(dev: DevInfo | null): ReloadState {
  const toast = useToast();
  const params = typeof location !== 'undefined' ? new URLSearchParams(location.search) : new URLSearchParams();
  const forced = forcedReloadState(params);
  const hot = import.meta.hot;
  const booted = useRef(false);
  const [state, setState] = useState<ReloadState>('ok');

  // Boot state + one fallback toast, fired once when dev info appears (Bootstrap
  // loads asynchronously, so dev can be null on the very first render).
  useEffect(() => {
    if (!dev || booted.current) return;
    booted.current = true;
    const init = initialReloadState({ forced, hotPresent: Boolean(hot) });
    setState(init);
    const boot = bootToast(init, dev.url);
    if (boot) toast(boot.message, { ms: boot.ms });
  }, [dev, forced, hot, toast]);

  // While Vite serves the page, mirror its websocket connection state.
  useEffect(() => {
    if (!dev || forced || !hot) return;
    const onDisconnect = () => setState('busy');
    const onConnect = () => setState((prev) => {
      const t = transitionToast(prev, 'ok', dev.url);
      if (t) toast(t.message, { tone: 'ok' });
      return 'ok';
    });
    hot.on('vite:ws:disconnect', onDisconnect);
    hot.on('vite:ws:connect', onConnect);
    return () => {
      hot.off('vite:ws:disconnect', onDisconnect);
      hot.off('vite:ws:connect', onConnect);
    };
  }, [dev, forced, hot, toast]);

  return state;
}
