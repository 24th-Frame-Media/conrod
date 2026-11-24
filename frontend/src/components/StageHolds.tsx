import { useState } from 'react';
import { Icon } from './basics';
import { skipConsequence } from '../lib/surface.mjs';
import { skipHeaderNote, skippedChip, stageName } from '../lib/stageState.mjs';
import type { SubjectWords } from '../lib/subjects.mjs';

type StageControl = (
  action: 'stage_pause' | 'stage_skip' | 'stage_rerun',
  stage: string,
  extra?: { paused?: boolean; skip?: boolean; full?: boolean },
) => void;

/**
 * Stages the photographer parked or skipped, each with its way back: a parked
 * stage resumes with the stage control (never the scan's Resume), and a skip,
 * which is stored on the album and applies to every later run, has an Undo.
 * Shown on the Ingest and the Identify page alike.
 */
export function StageHolds({ parked, skipped, onStageControl }: { parked: string[]; skipped: string[]; onStageControl?: StageControl }) {
  if (!parked.length && !skipped.length) return null;
  return (
    <div className="stage-holds" role="group" aria-label="Stages on hold">
      {parked.map((id) => (
        <span className="hold-chip paused" key={`parked:${id}`}>
          <span>{stageName(id)} is paused</span>
          {onStageControl && (
            <button type="button" onClick={() => onStageControl('stage_pause', id, { paused: false })}>Resume {stageName(id)}</button>
          )}
        </span>
      ))}
      {skipped.map((id) => {
        const chip = skippedChip(id);
        return (
          <span className="hold-chip skipped" key={`skipped:${id}`} title={chip.note}>
            <span>{chip.label}</span>
            <span className="hold-note">stays skipped on later runs</span>
            {onStageControl && (
              <button type="button" aria-label={chip.undo} onClick={() => onStageControl('stage_skip', id, { skip: false })}>
                <Icon name="undo" size={12} />Undo
              </button>
            )}
          </span>
        );
      })}
    </div>
  );
}

/**
 * The header's Skip button. It asks first and says what the click does: the
 * stage's own consequence, and that the skip persists for later runs of this
 * album until it is undone.
 */
export function SkipStageButton({ stage, label, remaining, subject, onSkip }: {
  stage: string; label: string; remaining: number; subject: SubjectWords; onSkip: () => void;
}) {
  const [asking, setAsking] = useState(false);
  const key = stage.replace(/^identify:/, '');
  const note = skipHeaderNote(stage, skipConsequence(key, { subject, remaining }));
  return (
    <span className="skip-stage">
      <button type="button" className="ghost" aria-expanded={asking} onClick={() => setAsking((v) => !v)}
        title={`Skip ${label} for this album. It stays skipped on later runs until you undo it.`}>
        Skip {label}
      </button>
      {asking && (
        <div className="skip-confirm" role="alertdialog" aria-label={`Skip ${label}?`}
          onKeyDown={(e) => { if (e.key === 'Escape') { e.stopPropagation(); setAsking(false); } }}>
          <p>{note}</p>
          <div className="skip-confirm-actions">
            <button type="button" className="ghost small" autoFocus onClick={() => setAsking(false)}>Cancel</button>
            <button type="button" className="primary small" onClick={() => { setAsking(false); onSkip(); }}>Skip {label}</button>
          </div>
        </div>
      )}
    </span>
  );
}
