import { useCallback, useEffect, useState } from 'react';
import { call } from '../lib/api';
import { FIELDS } from '../lib/settings.mjs';
import { pluginBundleLine, pluginStatusLine } from '../lib/lightroomPlugin.mjs';
import { SettingRow } from './SettingsFields';
import type { LightroomPluginAction, LightroomPluginStatus, SettingValue, Toaster } from '../lib/types';
import type { Runner } from '../state/hooks';

type Props = {
  value: SettingValue;
  changed: boolean;
  onChange: (value: SettingValue) => void;
  run: Runner;
  toast: Toaster;
};

/** The Lightroom Classic tab: the bridge switch, the plug-in's state, and its two buttons. */
export function LightroomBridge({ value, changed, onChange, run, toast }: Props) {
  const [status, setStatus] = useState<LightroomPluginStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const next = await call<LightroomPluginStatus>('lightroom_plugin_status');
      setStatus(next && typeof next === 'object' ? next : null);
      setError(null);
    } catch (e) {
      setStatus(null);
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => { void refresh(); }, [refresh]);

  const install = () => void run(async () => {
    setBusy(true);
    try {
      const result = await call<LightroomPluginAction>('install_lightroom_plugin');
      toast(result?.updated ? 'Plug-in installed in Lightroom Classic.' : 'The plug-in is already up to date.', { tone: 'ok' });
      await refresh();
    } finally {
      setBusy(false);
    }
  });

  const remove = () => {
    if (!window.confirm('Remove the Conrod Bridge from Lightroom Classic? Your photos and catalog are not touched.')) return;
    void run(async () => {
      setBusy(true);
      try {
        await call<LightroomPluginAction>('remove_lightroom_plugin');
        toast('Plug-in removed from Lightroom Classic.', { tone: 'ok' });
        await refresh();
      } finally {
        setBusy(false);
      }
    });
  };

  const turnOff = () => {
    if (status?.installed && !window.confirm('Turning this off removes the Conrod Bridge from Lightroom Classic. Continue?')) return;
    onChange(false);
  };

  const line = pluginStatusLine(error ? { error } : status);
  const bundle = pluginBundleLine(status);

  return (
    <section className="settings-section" aria-labelledby="settings-section-lightroom">
      <header className="settings-section-head">
        <h3 id="settings-section-lightroom">Lightroom Classic</h3>
        <p className="section-hint">
          The Conrod Bridge plug-in carries your stars and picks both ways. Conrod installs it and keeps it current;
          restart Lightroom after an update so it loads.
        </p>
      </header>
      <SettingRow
        field={FIELDS.lightroom_bridge}
        value={value}
        changed={changed}
        onChange={(next) => (next ? onChange(true) : turnOff())}
      />
      <div className="lightroom-plugin-row">
        <span className="lightroom-plugin-status" role="status" aria-live="polite">{line}</span>
        <span className="spacer" />
        <button type="button" className="secondary small" disabled={busy || status?.bundled === false} onClick={install}>
          {status?.installed ? 'Reinstall plug-in' : 'Install plug-in'}
        </button>
        <button type="button" className="ghost small" disabled={busy || !status?.installed} onClick={remove}>
          Remove
        </button>
      </div>
      {bundle && <p className="muted small lightroom-plugin-note">{bundle}</p>}
    </section>
  );
}
