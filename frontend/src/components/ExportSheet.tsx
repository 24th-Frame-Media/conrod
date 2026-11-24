import { useEffect, useMemo, useRef, useState } from 'react';
import { call } from '../lib/api';
import { exportLine, exportRule, exportSummary } from '../lib/export.mjs';
import { lightroomError, lightroomExportLine } from '../lib/lightroom.mjs';
import type { Frame, LightroomExport } from '../lib/types';
import { Icon } from './basics';
import { Sheet } from './Sheet';

type Props = {
  visible: boolean;
  jobId: number;
  albumLabel: string;
  frames: Frame[];
  keepers: Set<number>;
  /** Start with the dry run ticked (the first visit) or off. */
  dryRunFirst: boolean;
  /** Dev start only (`?mock=1&lightroom=sent`): send the verdicts as soon as the sheet opens. */
  sendOnOpen?: boolean;
  onClose: () => void;
  onWrite: (dryRun: boolean) => void;
};

/**
 * The last step of the flow: what leaves Conrod. XMP sidecars next to the originals are the only
 * destination for now; the originals themselves are never moved or changed, and a dry run writes nothing.
 */
export function ExportSheet({ visible, jobId, albumLabel, frames, keepers, dryRunFirst, sendOnOpen = false, onClose, onWrite }: Props) {
  const [dryRun, setDryRun] = useState(dryRunFirst);
  const [sending, setSending] = useState(false);
  const [sent, setSent] = useState<LightroomExport | null>(null);
  const [sendError, setSendError] = useState<string | null>(null);
  const summary = useMemo(() => exportSummary(frames, keepers), [frames, keepers]);
  const autoRan = useRef(false);

  useEffect(() => {
    if (visible) setDryRun(dryRunFirst);
  }, [visible, dryRunFirst]);

  // The Lightroom send reports inline, never as a toast: the sheet already toasts once through the
  // Write XMP path, and using both must not leave two messages behind.
  const send = async () => {
    if (sending) return;
    setSending(true);
    setSendError(null);
    try {
      setSent(await call<LightroomExport>('lightroom_export', { jobId }));
    } catch (e) {
      setSendError(lightroomError(e));
    } finally {
      setSending(false);
    }
  };

  useEffect(() => {
    if (!visible) {
      autoRan.current = false;
      setSent(null);
      setSendError(null);
      return;
    }
    if (!sendOnOpen || autoRan.current) return;
    autoRan.current = true;
    void send();
    // `send` reads the current jobId only; the guard above keeps the dev start to one send.
  }, [visible, sendOnOpen]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!visible) return null;
  return (
    <Sheet label="Export" className="export-sheet" style={{ maxWidth: 560 }} onClose={onClose}>
        <button className="ghost icon import-close" aria-label="Close" onClick={onClose}><Icon name="x" /></button>
        <h2>Export</h2>
        <p className="lede">Write what you decided about “{albumLabel}” next to the files, so Lightroom reads it.</p>

        <div className="export-counts">
          <span className="mono">{exportLine(summary)}</span>
        </div>
        <p className="muted export-rule">{exportRule()}</p>

        <div className="export-dest">
          <span className="export-dest-icon"><Icon name="drop" size={16} /></span>
          <div>
            <b>XMP sidecars next to the originals</b>
            <p className="muted">
              Ratings, stars, keywords and subject details go into an .xmp file beside each photo.
              The originals are never moved, renamed or changed.
            </p>
          </div>
        </div>

        <label className="import-check">
          <input type="checkbox" checked={dryRun} onChange={(e) => setDryRun(e.target.checked)} />
          <span>
            <b>Dry run</b>
            <small>List what would be written and touch nothing on disk.</small>
          </span>
        </label>

        <p className="muted export-hint">
          Lightroom Classic: <b>Import → Add</b> on the folder for new photos, or <b>Metadata → Read Metadata from Files</b> for ones already imported.
        </p>

        <h3 className="export-subhead">Lightroom</h3>
        <p className="muted export-hint">
          Stars, pick and reject flags, colour labels and Conrod keywords, written for the Conrod Bridge plugin.
        </p>

        <div className="export-lr">
          <button type="button" className="ghost" disabled={sending} onClick={() => void send()}>
            {sending ? 'Sending…' : sent ? 'Send again' : 'Send verdicts to Lightroom'}
          </button>
          {sent != null && (
            <p className="export-result" title={sent.path}>
              <Icon name="check" size={14} />
              <span className="mono">{lightroomExportLine(sent)}</span>
            </p>
          )}
          {sendError != null && (
            <p className="error-text" title={sendError}>{sendError}</p>
          )}
          <p className="muted export-hint">
            The Conrod Bridge plugin applies them the next time Lightroom is open: <b>Library → Plug-in Extras → Apply verdicts from Conrod</b>.
          </p>
        </div>

        <div className="modal-actions" style={{ display: 'flex', justifyContent: 'flex-end', gap: 10, marginTop: 18 }}>
          <button type="button" className="ghost" onClick={onClose}>Cancel</button>
          <button type="button" className="primary" onClick={() => { onWrite(dryRun); onClose(); }}>
            {dryRun ? 'Check what would be written' : 'Export'}
          </button>
        </div>
      </Sheet>
  );
}
