import { useCallback, useEffect, useState } from 'react';
import { engine } from '../lib/api';
import { canInstall, sizeWords, statusLine } from '../lib/decider.mjs';
import type { DeciderStatus } from '../lib/types';
import '../styles/ingest.css';

type Props = {
  /** Called after an install starts so the page can refresh the model list. */
  onChanged?: () => void;
  toast: (message: string, options?: { tone?: 'ok' | 'error' }) => void;
};

const gb = (bytes: number) => (bytes / 2 ** 30).toFixed(1) + ' GB';

/**
 * The Settings card for the decision model: whether it can run here (video memory against what it needs), how big the
 * download is, the install button with progress, and the live state. The on/off switch and the precision override are the
 * ordinary rows beneath it.
 */
export function DeciderCard({ onChanged, toast }: Props) {
  const [status, setStatus] = useState<DeciderStatus | null>(null);
  const refresh = useCallback(() => {
    void engine.deciderStatus().then(setStatus).catch(() => undefined);
  }, []);
  useEffect(() => { refresh(); }, [refresh]);
  // Poll while an install runs; otherwise a slow tick keeps the free-memory figure honest.
  const installing = status?.state === 'installing';
  useEffect(() => {
    const id = window.setInterval(refresh, installing ? 1000 : 5000);
    return () => window.clearInterval(id);
  }, [installing, refresh]);

  const line = statusLine(status);
  const gpu = status?.gpu ?? null;
  const pct = status?.install && status.install.total > 0 ? Math.round((status.install.done / status.install.total) * 100) : 0;
  const install = async () => {
    try {
      await engine.installDecider();
      toast('Installing the decision model. It keeps going if you leave this page.', { tone: 'ok' });
      refresh();
      onChanged?.();
    } catch (e) {
      toast(String(e), { tone: 'error' });
    }
  };
  return (
    <div className={'decider-card ' + line.tone} role="group" aria-label="Decision model status">
      <div className="decider-card-head">
        <span className={line.tone === 'ok' ? 'dot ok' : 'dot no'} aria-hidden="true" />
        <b>{line.title}</b>
        {line.detail && <span className="muted">{line.detail}</span>}
      </div>
      <p className="muted decider-card-gpu">
        {gpu && status
          ? gpu.name + ': ' + gb(gpu.free) + ' free of ' + gb(gpu.total) + '. int8 needs ' + gb(status.needs.int8) + ', int4 needs ' + gb(status.needs.int4) + ' free.'
          : 'No graphics card found.'}
      </p>
      {installing && (
        <div className="bar" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100} aria-label="Decision model download">
          <div className="fill" style={{ width: pct + '%' }} />
        </div>
      )}
      {status && canInstall(status) && (
        <div className="decider-card-actions">
          <button type="button" className="primary small" onClick={() => void install()}>
            {status.installed && status.download_bytes === 0 ? 'Prepare the decision model' : 'Install the decision model (' + sizeWords(status.download_bytes) + ')'}
          </button>
          <span className="muted small">Downloaded once, checked against a pinned hash, and resumed if interrupted. It is not bundled with Conrod.</span>
        </div>
      )}
      {status?.state === 'ready' && (
        <p className="muted small">While it works it holds about {status.quant === 'int4' ? '4' : '6'} GB of video memory, and gives it back after a minute of quiet.</p>
      )}
    </div>
  );
}