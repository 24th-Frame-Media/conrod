import { useEffect, useMemo, useState } from 'react';
import { exportLine, exportSummary } from '../lib/export.mjs';
import type { Frame } from '../lib/types';
import { Icon } from './basics';

type Props = {
  visible: boolean;
  albumLabel: string;
  frames: Frame[];
  keepers: Set<number>;
  /** Start with the dry run ticked (the first visit) or off. */
  dryRunFirst: boolean;
  onClose: () => void;
  onWrite: (dryRun: boolean) => void;
};

/**
 * The last step of the flow: what leaves Conrod. XMP sidecars next to the originals are the only
 * destination for now; the originals themselves are never moved or changed, and a dry run writes nothing.
 */
export function ExportSheet({ visible, albumLabel, frames, keepers, dryRunFirst, onClose, onWrite }: Props) {
  const [dryRun, setDryRun] = useState(dryRunFirst);
  const summary = useMemo(() => exportSummary(frames, keepers), [frames, keepers]);

  useEffect(() => {
    if (visible) setDryRun(dryRunFirst);
  }, [visible, dryRunFirst]);

  if (!visible) return null;
  return (
    <div className="import-overlay" onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="import-dialog export-sheet" style={{ maxWidth: 560 }}>
        <button className="ghost icon import-close" aria-label="Close" onClick={onClose}><Icon name="x" /></button>
        <h2>Export</h2>
        <p className="lede">Write what you decided about “{albumLabel}” next to the files, so Lightroom reads it.</p>

        <div className="export-counts">
          <span className="mono">{exportLine(summary)}</span>
        </div>

        <div className="export-dest">
          <span className="export-dest-icon"><Icon name="drop" size={16} /></span>
          <div>
            <b>XMP sidecars next to the originals</b>
            <p className="muted">
              Ratings, stars, keywords and vehicle details go into an .xmp file beside each photo.
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

        <div className="modal-actions" style={{ display: 'flex', justifyContent: 'flex-end', gap: 10, marginTop: 18 }}>
          <button type="button" className="ghost" onClick={onClose}>Cancel</button>
          <button type="button" className="primary" onClick={() => { onWrite(dryRun); onClose(); }}>
            {dryRun ? 'Check what would be written' : 'Write XMP'}
          </button>
        </div>
      </div>
    </div>
  );
}
