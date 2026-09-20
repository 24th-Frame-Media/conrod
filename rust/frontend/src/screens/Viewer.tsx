import type { CSSProperties } from 'react';
import { Modal } from '../components/basics';
import { StarControl } from '../components/Stars';
import { asset } from '../lib/api';
import { boxOf, filename, frameStars, parseAttributes } from '../lib/review';
import type { ReviewModel } from '../state/useReview';
import { usePreview, type Runner } from '../state/hooks';

type Props = { rv: ReviewModel; run: Runner; showBoxes: boolean; onToggleBoxes: () => void; onClose: () => void };

/** The full frame, with what was read off it down the side. Arrows walk the album without closing. */
export function Viewer({ rv, run, showBoxes, onToggleBoxes, onClose }: Props) {
  const { frame, frames, facts, step, mark } = rv;
  const preview = usePreview(frame?.id ?? null, run);
  if (!frame) return null;
  const dets = facts.get(frame.id)?.dets ?? [];
  const index = frames.findIndex((f) => f.id === frame.id);
  const ratio = frame.width && frame.height ? frame.width / frame.height : 3 / 2;
  const stars = frameStars(frame);
  return (
    <Modal label="Photo viewer" className="viewer" onClose={onClose}>
      <div className="frame-view">
        <div className="frame-stage-big">
          <div className={`stage-inner${preview ? '' : ' loading'}`} style={{ '--ar': ratio } as CSSProperties}>
            <img src={asset(preview) ?? asset(frame.thumb_path)} alt={filename(frame.path)} />
            {showBoxes && (
              <div className="overlay">
                {dets.map((d) => {
                  const b = boxOf(d, frame);
                  if (!b) return null;
                  return (
                    <div key={d.id} className={`box${d.panning ? ' pan' : ''}`}
                      style={{ left: `${b[0] * 100}%`, top: `${b[1] * 100}%`, width: `${(b[2] - b[0]) * 100}%`, height: `${(b[3] - b[1]) * 100}%` }}>
                      <span className="box-label">{d.cls} {d.sharpness.toFixed(2)}</span>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
          <button className="frame-step prev" title="Previous frame (←)" aria-label="Previous frame" disabled={index <= 0} onClick={() => step(-1)}>‹</button>
          <button className="frame-step next" title="Next frame (→)" aria-label="Next frame" disabled={index < 0 || index >= frames.length - 1} onClick={() => step(1)}>›</button>
        </div>
        <aside className="frame-side">
          <div className="frame-side-head">
            <h3 title={frame.path}>{filename(frame.path)}</h3>
            <button className="ghost small" autoFocus onClick={onClose}>Close</button>
          </div>
          <div className="readout">
            <div className="read"><span className="lbl">Size</span><span className="val">{frame.width ? `${frame.width} × ${frame.height}` : '—'}</span></div>
            <div className="read"><span className="lbl">Focus</span><span className="val">{frame.rating != null ? frame.rating.toFixed(3) : '—'}</span></div>
            {dets.map((d) => {
              const a = parseAttributes(d);
              const vehicle = [a.make, a.model].filter(Boolean).join(' ');
              return (
                <div className="viewer-subject" key={d.id}>
                  <div className="read"><span className="lbl">{d.cls}</span><span className="val">focus {d.sharpness.toFixed(3)}{d.panning ? ' · panned' : ''}{d.burst_pick ? ' · keeper' : ''}</span></div>
                  {a.plate && <div className="read"><span className="lbl">Plate</span><span className="val strong">{a.plate}</span></div>}
                  {a.race_number && <div className="read"><span className="lbl">No.</span><span className="val strong">{a.race_number}</span></div>}
                  {vehicle && <div className="read"><span className="lbl">Vehicle</span><span className="val">{vehicle}</span></div>}
                  {a.colour && <div className="read"><span className="lbl">Colour</span><span className="val">{a.colour}</span></div>}
                  {a.team && <div className="read"><span className="lbl">Team</span><span className="val">{a.team}</span></div>}
                  {d.cull_reason && <div className="read"><span className="lbl">Cull</span><span className="val unverified">{d.cull_reason}</span></div>}
                </div>
              );
            })}
          </div>
          <p id="frame-caption" className="muted mono">{frame.path}</p>
          <div className="frame-side-foot">
            <StarControl value={stars} onChange={(n) => void mark({ stars: n })} />
            <button className="ghost small" title="Back to the measured rating (U)" onClick={() => void mark({ stars: null, rejected: false })}>Clear</button>
            <div className="spacer" />
            <button className={`ghost danger cut${frame.rejected ? ' on' : ''}`} onClick={() => void mark({ rejected: !frame.rejected })}>{frame.rejected ? 'Put back' : 'Reject'}</button>
          </div>
          <label className="check-line"><input type="checkbox" checked={showBoxes} onChange={onToggleBoxes} /> Show detection outlines <kbd>B</kbd></label>
          <p className="muted frame-pos">{index >= 0 ? `${(index + 1).toLocaleString()} / ${frames.length.toLocaleString()}` : ''}</p>
        </aside>
      </div>
    </Modal>
  );
}
