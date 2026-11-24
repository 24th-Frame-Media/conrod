import { useEffect, useRef, useState } from 'react';
import { call } from '../lib/api';
import {
  lightroomError, lightroomImportLine, lightroomImportPercent, packageLine, pendingLine, pendingPackages,
} from '../lib/lightroom.mjs';
import type { LightroomPendingRow } from '../lib/lightroom.mjs';
import { Icon } from './basics';
import { Sheet } from './Sheet';
import type { LightroomImport as LightroomImportResult, LightroomPending } from '../lib/types';

type Props = {
  visible: boolean;
  jobId: number;
  albumLabel: string;
  /** Dev start only (`?mock=1&lightroom=imported`): import the waiting packages as the dialog opens. */
  autoRun?: boolean;
  onClose: () => void;
  /** The album's catalog columns changed; Review reloads so the stats tile can follow. */
  onImported?: () => void;
};

/**
 * `lightroom_import`: reads what the photographer decided in Lightroom, either from the packages
 * the Conrod Bridge plugin wrote into `bridge\in` or from their `.lrcat` directly. Nothing here
 * overwrites Conrod's own stars or flags; the catalog is a second opinion.
 */
export function LightroomImport({ visible, jobId, albumLabel, autoRun = false, onClose, onImported }: Props) {
  const [catalog, setCatalog] = useState('');
  const [packages, setPackages] = useState<LightroomPendingRow[] | null>(null);
  const [pendingError, setPendingError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<LightroomImportResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const autoRan = useRef(false);

  useEffect(() => {
    if (!visible) return;
    let live = true;
    setPackages(null);
    setPendingError(null);
    setResult(null);
    setError(null);
    call<LightroomPending>('lightroom_pending', { dir: 'in' })
      .then((payload) => {
        if (live) setPackages(pendingPackages(payload));
      })
      .catch((e) => {
        if (!live) return;
        setPackages([]);
        setPendingError(lightroomError(e));
      });
    return () => { live = false; };
  }, [visible, jobId]);

  const runImport = async (withCatalog: boolean) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const args = withCatalog ? { jobId, catalog: catalog.trim() } : { jobId };
      setResult(await call<LightroomImportResult>('lightroom_import', args));
      onImported?.();
    } catch (e) {
      setError(lightroomError(e));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (!visible) {
      autoRan.current = false;
      return;
    }
    if (!autoRun || autoRan.current || packages == null) return;
    autoRan.current = true;
    void runImport(false);
    // Dev start only: the guard makes it one import per open.
  }, [visible, autoRun, packages]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!visible) return null;
  const waiting = packages ?? [];
  const percent = result == null ? null : lightroomImportPercent(result);

  return (
    <Sheet label="Import from Lightroom" className="lightroom-dialog" style={{ maxWidth: 560 }} onClose={onClose}>
        <button className="ghost icon import-close" aria-label="Close" onClick={onClose}><Icon name="x" /></button>
        <h2>Import from Lightroom</h2>
        <p className="lede">
          Read the flags, stars and colour labels you gave “{albumLabel}” in Lightroom Classic. Conrod’s own
          decisions stay as they are; the catalog is a second opinion.
        </p>

        <div className="field-group lr-block">
          <span className="field-label">Waiting in the bridge</span>
          {packages == null ? (
            <p className="muted export-hint">Looking for packages…</p>
          ) : waiting.length === 0 ? (
            <p className="muted export-hint">
              {pendingLine(waiting)}. In Lightroom: <b>Library → Plug-in Extras → Send flags to Conrod</b>.
            </p>
          ) : (
            <>
              <div className="lr-packages">
                {waiting.map((pkg) => (
                  <div className="lr-package" key={pkg.name}>
                    <b className="mono">{pkg.name}</b>
                    <span className="muted">{packageLine(pkg)}</span>
                  </div>
                ))}
              </div>
              <p className="muted export-hint">{pendingLine(waiting)}; Conrod moves each one to <b>done</b> after it reads it.</p>
            </>
          )}
          {pendingError != null && <p className="error-text" title={pendingError}>{pendingError}</p>}
        </div>

        <div className="field-group lr-block">
          <label className="field-label" htmlFor="lr-catalog">Read my Lightroom catalog (.lrcat)</label>
          <input
            id="lr-catalog"
            value={catalog}
            spellCheck={false}
            placeholder="C:\Users\you\Pictures\Lightroom\Lightroom Catalog.lrcat"
            onChange={(e) => setCatalog(e.target.value)}
          />
          <small className="muted">
            Optional. Close Lightroom Classic first: it locks the catalog while it is open. The plugin’s
            packages are the usual way and need no path.
          </small>
          <div className="lr-block-actions">
            <button type="button" className="ghost small" disabled={busy || catalog.trim() === ''} onClick={() => void runImport(true)}>
              Read catalog
            </button>
          </div>
        </div>

        {result != null && (
          <div className="export-counts lr-summary">
            <span className="mono">{lightroomImportLine(result)}</span>
            {percent != null && <b className="lr-summary-pct">{percent}</b>}
          </div>
        )}
        {error != null && <p className="error-text" title={error}>{error}</p>}

        <div className="modal-actions" style={{ display: 'flex', justifyContent: 'flex-end', gap: 10, marginTop: 18 }}>
          <button type="button" className="ghost" onClick={onClose}>{result == null ? 'Cancel' : 'Done'}</button>
          <button type="button" className="primary" disabled={busy || waiting.length === 0} onClick={() => void runImport(false)}>
            {busy ? 'Reading…' : `Import ${waiting.length === 1 ? 'package' : 'packages'}`}
          </button>
        </div>
      </Sheet>
  );
}
