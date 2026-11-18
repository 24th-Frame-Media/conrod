import { useCallback, useEffect, useMemo, useState } from 'react';
import { asset, engine } from '../lib/api';
import { countsOf, readOf, summaryOf } from '../lib/identified.mjs';
import type { Identified } from '../lib/types';
import { Icon, Portal, useBackdropClose } from './basics';

type Props = {
  visible: boolean;
  jobId: number | null;
  albumLabel: string;
  /** True while the identify pass runs: the list refreshes itself. */
  identifying: boolean;
  onClose: () => void;
  onOpenFrame: (imageId: number) => void;
  onAccepted: () => void;
  toast: (message: string, options?: { tone?: 'error' | 'ok'; ms?: number }) => void;
};

/**
 * Everything the identify pass has read in this album: the race number, plate and
 * vision-model fields per detection, unreviewed first. It is the answer to "what is
 * it finding?" while the pass runs and "what did it read?" afterwards.
 */
export function Identifications({ visible, jobId, albumLabel, identifying, onClose, onOpenFrame, onAccepted, toast }: Props) {
  const [data, setData] = useState<Identified | null>(null);
  const [onlyNew, setOnlyNew] = useState(true);
  const [busy, setBusy] = useState(false);
  const [picked, setPicked] = useState<Set<number>>(new Set());
  const backdrop = useBackdropClose(onClose);

  const load = useCallback(async () => {
    if (jobId == null) return;
    try { setData(await engine.identified(jobId)); } catch { /* the next tick retries */ }
  }, [jobId]);

  useEffect(() => {
    if (!visible) return;
    setPicked(new Set());
    void load();
    if (!identifying) return;
    const timer = setInterval(() => void load(), 4000);
    return () => clearInterval(timer);
  }, [visible, load, identifying]);

  const rows = useMemo(() => {
    const all = data?.rows ?? [];
    return onlyNew ? all.filter((r) => Number(r.reviewed) !== 1) : all;
  }, [data, onlyNew]);

  const counts = countsOf(data?.rows ?? []);
  if (!visible) return null;

  const accept = async (ids: number[]) => {
    if (!ids.length) return;
    setBusy(true);
    try {
      await engine.acceptIdentifications(ids);
      setPicked(new Set());
      await load();
      onAccepted();
      toast(`${ids.length} identification${ids.length === 1 ? '' : 's'} accepted.`, { tone: 'ok' });
    } catch (e) {
      toast(String(e), { tone: 'error' });
    } finally { setBusy(false); }
  };

  const toggle = (id: number) => setPicked((s) => {
    const next = new Set(s);
    if (next.has(id)) next.delete(id); else next.add(id);
    return next;
  });

  return (
    <Portal>
    <div className="import-overlay" {...backdrop}>
      <div className="import-dialog identified-sheet">
        <button className="ghost icon import-close" aria-label="Close" onClick={onClose}><Icon name="x" /></button>
        <h2>Identifications</h2>
        <p className="lede">
          What Conrod read on “{albumLabel}”{identifying ? ' · still identifying, this list updates itself' : ''}.
        </p>

        <div className="identified-bar">
          <div className="identified-counts mono">
            {data ? `${(counts.unreviewed).toLocaleString()} to review · ${counts.reviewed.toLocaleString()} accepted · ${(data.total).toLocaleString()} read` : 'Loading…'}
            {data && data.total > (data.shown ?? data.rows.length) ? ` · showing ${data.shown.toLocaleString()}` : ''}
          </div>
          <span className="spacer" />
          <label className="identified-filter"><input type="checkbox" checked={onlyNew} onChange={(e) => setOnlyNew(e.target.checked)} /> Needs a look</label>
          <button type="button" className="ghost small" onClick={() => void load()} disabled={busy}>Refresh</button>
          <button type="button" className="primary small" disabled={busy || !picked.size} onClick={() => void accept([...picked])}>
            Accept {picked.size ? `${picked.size} ` : ''}selected
          </button>
        </div>

        <div className="identified-list">
          {rows.length === 0 ? (
            <p className="muted identified-empty">
              {data == null ? 'Loading what was read…' : onlyNew ? 'Nothing is waiting for a look.' : 'Nothing has been read yet. Run identification first.'}
            </p>
          ) : rows.map((r) => {
            const read = readOf(r);
            return (
              <div key={r.id} className={`identified-row${read.reviewed ? ' done' : ''}${picked.has(r.id) ? ' picked' : ''}`}>
                <input type="checkbox" checked={picked.has(r.id)} onChange={() => toggle(r.id)} aria-label={`Select ${r.frame_path?.split(/[\\/]/).pop() ?? 'detection'}`} />
                <button type="button" className="identified-thumb" onClick={() => onOpenFrame(r.image_id)} title="Open the photo">
                  {asset(r.crop_path ?? r.frame_thumb)
                    ? <img src={asset(r.crop_path ?? r.frame_thumb)} alt="" loading="lazy" />
                    : <span className="identified-noimg"><Icon name="images" size={16} /></span>}
                </button>
                <div className="identified-main">
                  <b>{r.frame_path?.split(/[\\/]/).pop() ?? `Photo ${r.image_id}`}</b>
                  <span className="muted">{summaryOf(r)}</span>
                </div>
                <span className="identified-conf mono">{read.confidence ? `${Math.round(read.confidence * 100)}%` : '—'}</span>
                <span className={`identified-state ${read.reviewed ? 'done' : 'new'}`}>{read.reviewed ? 'Accepted' : 'New'}</span>
                <button type="button" className="ghost small" disabled={busy || read.reviewed} onClick={() => void accept([r.id])}>Accept</button>
              </div>
            );
          })}
        </div>
      </div>
    </div>
    </Portal>
  );
}
