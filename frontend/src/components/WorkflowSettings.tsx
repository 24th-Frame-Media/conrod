import {
  DEFAULT_IDENTIFY_ORDER,
  PARALLEL_NOTE,
  moveIdentifyStage,
  normalizeIdentifyOrder,
  type IdentifyStageKey,
} from '../lib/settings.mjs';
import { Icon } from './basics';
import type { IdentifyVram, SettingValue, Settings as SettingsMap } from '../lib/types';
import '../styles/workflow.css';

const STAGE_META: Record<IdentifyStageKey, { label: string; badge: string; hint: string }> = {
  ocr: {
    label: 'OCR',
    badge: 'CPU',
    hint: 'Reads numbers, plates, and any visible text locally on CPU.',
  },
  decision: {
    label: 'Decider',
    badge: 'GPU / Cloud',
    hint: 'Judges action moments, suggests shoot type, and picks each burst peak.',
  },
  vision: {
    label: 'Vision',
    badge: 'GPU / LAN / Cloud',
    hint: 'Reads make, model, colour, team, and action tags with the vision model.',
  },
};

const PRESETS: { id: string; label: string; order: readonly IdentifyStageKey[] }[] = [
  { id: 'default', label: 'OCR → Decider → Vision', order: DEFAULT_IDENTIFY_ORDER },
  { id: 'vision-first', label: 'Vision → OCR → Decider', order: ['vision', 'ocr', 'decision'] },
  { id: 'decider-first', label: 'Decider → Vision → OCR', order: ['decision', 'vision', 'ocr'] },
];

type Props = {
  draft: SettingsMap;
  saved: SettingsMap;
  identifyVram?: IdentifyVram | null;
  onChange: (key: string, value: SettingValue) => void;
};

/** Configurable Identify workflow: run stages simultaneously and/or choose their execution order. */
export function WorkflowSettings({ draft, saved, identifyVram = null, onChange }: Props) {
  const parallel = Boolean(draft.identify_parallel);
  const parallelDirty = Boolean(draft.identify_parallel) !== Boolean(saved.identify_parallel);
  const order = normalizeIdentifyOrder(Array.isArray(draft.identify_order) ? draft.identify_order : null);
  const savedOrder = normalizeIdentifyOrder(Array.isArray(saved.identify_order) ? saved.identify_order : null);
  const orderDirty = JSON.stringify(order) !== JSON.stringify(savedOrder);
  const serialCard = identifyVram?.mode === 'serial';
  const gpuOrder = order.filter((k): k is 'decision' | 'vision' => k === 'decision' || k === 'vision');

  return (
    <section className="settings-section wf-section" aria-labelledby="settings-section-identify-workflow">
      <header className="settings-section-head">
        <h3 id="settings-section-identify-workflow">Identify workflow</h3>
        <p className="section-hint">
          Choose the order OCR, Decider, and Vision run in, or allow them to run at the same time to cut total wait time.
        </p>
      </header>

      <div className={`setting${parallelDirty ? ' is-dirty' : ''}`} data-setting="identify_parallel">
        <div className="setting-copy">
          <label className="label" htmlFor="setting-identify_parallel">
            Run OCR, Decider and Vision at the same time
            {parallelDirty && <span className="sr-only">, unsaved change</span>}
          </label>
          <p className="hint" id="setting-identify_parallel-hint">
            Runs CPU OCR, the decision model, and the vision model concurrently when your hardware allows it.
          </p>
          <p className="hint wf-parallel-note" id="setting-identify_parallel-note">{PARALLEL_NOTE}</p>
        </div>
        <div className="setting-control">
          <input
            id="setting-identify_parallel"
            type="checkbox"
            role="switch"
            checked={parallel}
            aria-describedby="setting-identify_parallel-hint setting-identify_parallel-note"
            onChange={(e) => onChange('identify_parallel', e.target.checked)}
          />
        </div>
      </div>

      <div className={`wf-hw-note ${serialCard ? 'warn' : 'ok'}`} role="status">
        <span className="wf-hw-dot" aria-hidden="true" />
        <span>
          {serialCard
            ? 'This graphics card fits one local model at a time. With parallel on, CPU OCR runs alongside Decider and Vision while the two local GPU models take turns in your order below.'
            : parallel
              ? 'Full parallel active: OCR, Decider, and Vision will run at the same time on the next Identify run.'
              : 'Your setup can run OCR, Decider, and Vision simultaneously (e.g. CPU OCR + local GPU Decider + LAN/cloud Vision or ample VRAM).'}
        </span>
      </div>

      <div className={`wf-order-block${orderDirty ? ' is-dirty' : ''}`} data-setting="identify_order">
        <div className="wf-order-head">
          <div>
            <span className="label">Stage order</span>
            <p className="hint">
              {parallel
                ? serialCard
                  ? 'Sets the handoff order between your local GPU stages and the flowchart layout.'
                  : 'Sets the flowchart layout and the fallback order when parallel mode is turned off.'
                : 'Stages run one after another in this exact order.'}
            </p>
          </div>
          <div className="wf-presets" role="group" aria-label="Stage order presets">
            {PRESETS.map((preset) => {
              const active = JSON.stringify(order) === JSON.stringify(preset.order);
              return (
                <button
                  key={preset.id}
                  type="button"
                  className={`ghost small wf-preset-btn${active ? ' on' : ''}`}
                  aria-pressed={active}
                  onClick={() => onChange('identify_order', [...preset.order])}
                >
                  {preset.label}
                </button>
              );
            })}
          </div>
        </div>

        <ol className="wf-stage-list" aria-label="Identify stage order">
          {order.map((key, index) => {
            const meta = STAGE_META[key];
            return (
              <li className="wf-stage-row" key={key}>
                <span className="wf-stage-num mono">{String(index + 1).padStart(2, '0')}</span>
                <div className="wf-stage-info">
                  <div className="wf-stage-title">
                    <b>{meta.label}</b>
                    <span className="wf-stage-badge mono">{meta.badge}</span>
                  </div>
                  <span className="muted wf-stage-hint">{meta.hint}</span>
                </div>
                <div className="wf-stage-move" role="group" aria-label={`Reorder ${meta.label}`}>
                  <button
                    type="button"
                    className="ghost small mono"
                    disabled={index === 0}
                    aria-label={`Move ${meta.label} earlier`}
                    onClick={() => onChange('identify_order', moveIdentifyStage(order, key, -1))}
                  >
                    <Icon name="arrowUp" size={14} />
                  </button>
                  <button
                    type="button"
                    className="ghost small mono"
                    disabled={index === order.length - 1}
                    aria-label={`Move ${meta.label} later`}
                    onClick={() => onChange('identify_order', moveIdentifyStage(order, key, 1))}
                  >
                    <Icon name="arrowDown" size={14} />
                  </button>
                </div>
              </li>
            );
          })}
        </ol>

        <div className="wf-preview" aria-label="Pipeline execution preview">
          <span className="wf-preview-label mono">{parallel ? (serialCard ? 'Hybrid parallel' : 'Parallel') : 'Sequential'}</span>
          {parallel ? (
            serialCard ? (
              <div className="wf-preview-parallel">
                <span className="wf-pill mono">OCR (CPU)</span>
                <span className="wf-parallel-plus mono">+</span>
                <span className="wf-pill mono">{STAGE_META[gpuOrder[0]].label} → {STAGE_META[gpuOrder[1]].label} (GPU)</span>
              </div>
            ) : (
              <div className="wf-preview-parallel">
                {order.map((k, i) => (
                  <span className="wf-preview-seg" key={k}>
                    {i > 0 && <span className="wf-parallel-plus mono">+</span>}
                    <span className="wf-pill mono">{STAGE_META[k].label}</span>
                  </span>
                ))}
              </div>
            )
          ) : (
            <div className="wf-preview-seq">
              {order.map((k, i) => (
                <span className="wf-preview-seg" key={k}>
                  {i > 0 && <span className="wf-arrow mono">→</span>}
                  <span className="wf-pill mono">{String(i + 1).padStart(2, '0')} · {STAGE_META[k].label}</span>
                </span>
              ))}
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
