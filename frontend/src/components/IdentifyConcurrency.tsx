import { concurrencyView } from '../lib/surface.mjs';
import type { IdentifyVram } from '../lib/types';
import { Icon } from './basics';

/** Settings → Identification: local-model worker concurrency and VRAM fit
 *  warning, derived from `concurrencyView` in surface.mjs using `identifyVram`
 *  and resolved settings. Worker seats appear only when both models run
 *  locally; when only one model fits in VRAM, a warning shows the measured
 *  sizes and explains serial fallback. */
type Props = {
  draft: Record<string, unknown>;
  identifyVram?: IdentifyVram | null;
  onChange: (key: string, value: number | boolean | string) => void;
  embedded?: boolean;
};

export function IdentifyConcurrency({ draft, identifyVram = null, onChange, embedded = false }: Props) {
  const view = concurrencyView(identifyVram ?? (draft as { identifyVram?: IdentifyVram | null }).identifyVram ?? null, draft);
  // Nothing to say when the two models do not both run on this PC and fit.
  if (!view.visible && !view.showWarning) return null;

  const numbers = view.numbers;
  return (
    <section
      className={embedded ? 'identify-concurrency embedded' : 'settings-section identify-concurrency'}
      aria-labelledby="identify-concurrency-head"
    >
      <h3 id="identify-concurrency-head"><Icon name="sliders" size={15} /> Two local models</h3>
      <p className="muted id-concurrency-lede">
        When the decision model and the vision model both run on this PC, you can set how many workers each stage uses.
      </p>

      {view.showWarning && (
        <div className="id-vram-warning" role="status">
          <div className="id-vram-warning-head"><Icon name="hourglass" size={15} /><b>{view.warning}</b></div>
          <span className="muted">{view.serialExplanation}</span>
          <span className="muted id-vram-numbers">
            Card {numbers.card ?? '?'} · Decision {numbers.decision ?? '?'} · Vision {numbers.vision ?? '?'} · together {numbers.sum ?? '?'}
          </span>
        </div>
      )}
      {!view.showWarning && view.present && (
        <p className="muted id-vram-numbers">
          Card {numbers.card ?? '?'} · Decision {numbers.decision ?? '?'} · Vision {numbers.vision ?? '?'} · both fit
        </p>
      )}

      {view.visible ? (
        <div className="id-seats" role="group" aria-label="Identify worker seats">
          <label className="id-seat">
            <b>Decision workers</b>
            <input type="number" min={1} max={16} step={1} value={view.seats.decision}
              onChange={(e) => onChange(view.seats.decisionKey, Math.max(1, Math.round(Number(e.target.value) || 1)))} />
            <small>Seats for the decision stage.</small>
          </label>
          <label className="id-seat">
            <b>Vision workers</b>
            <input type="number" min={1} max={16} step={1} value={view.seats.vision}
              onChange={(e) => onChange(view.seats.visionKey, Math.max(1, Math.round(Number(e.target.value) || 1)))} />
            <small>Seats for the vision stage.</small>
          </label>
        </div>
      ) : (
        <p className="muted">Worker seats appear when both models run locally.</p>
      )}
      {!view.showWarning && <p className="muted id-concurrency-mode">{view.explanation}</p>}
    </section>
  );
}
