import { useEffect, useRef, useState } from 'react';
import type { Settings } from '../lib/types';

/** The album grouping choices, in the engine's own (snake_case) settings keys. */
export type GroupingValues = {
  grouping_stacks: boolean;
  grouping_sensitivity: GroupingSensitivity;
  grouping_pause: number;
  grouping_keep_pans: boolean;
  grouping_merge?: boolean;
  grouping_focus_anchor: boolean;
};
export type GroupingSensitivity = 'off' | 'gentle' | 'normal' | 'eager';

export const GROUPING_DEFAULTS: GroupingValues = {
  grouping_stacks: true,
  grouping_sensitivity: 'normal',
  grouping_pause: 0.8,
  grouping_keep_pans: true,
  grouping_merge: true,
  grouping_focus_anchor: true,
};

const SENSITIVITIES: { id: GroupingSensitivity; name: string; hint: string }[] = [
  { id: 'off', name: 'Off', hint: 'One stack per burst: a burst of shooting is never cut. Stacking stays on, so this is not the same as Stack photos off.' },
  { id: 'gentle', name: 'Gentle', hint: 'Only a big change of subject starts a new stack: fewer, longer stacks.' },
  { id: 'normal', name: 'Normal', hint: 'The measured balance between a change of subject and a new stack.' },
  { id: 'eager', name: 'Eager', hint: 'The smallest change of subject starts a new stack: more, shorter stacks.' },
];

const PAUSE_HINT = 'Frames further apart than this start a new stack, if the picture also changed. 0 never splits on a pause.';

/** The five grouping choices from a settings blob (an album's, the defaults, or an older payload), always complete. */
export function groupingOf(settings: Settings | Record<string, unknown> | null | undefined): GroupingValues {
  const raw = settings ?? {};
  const sensitivity = SENSITIVITIES.some((option) => option.id === raw.grouping_sensitivity)
    ? raw.grouping_sensitivity as GroupingSensitivity
    : GROUPING_DEFAULTS.grouping_sensitivity;
  const pause = Number(raw.grouping_pause);
  return {
    grouping_stacks: raw.grouping_stacks !== false,
    grouping_sensitivity: sensitivity,
    grouping_pause: Number.isFinite(pause) && pause >= 0 ? Math.min(10, pause) : GROUPING_DEFAULTS.grouping_pause,
    grouping_keep_pans: raw.grouping_keep_pans !== false,
    grouping_merge: raw.grouping_merge !== false,
    grouping_focus_anchor: raw.grouping_focus_anchor !== false,
  };
}

/** The same stacking controls in the import sheet and an album's shoot settings. */
export function GroupingFields({ value, onChange, disabled = false, idPrefix = 'grouping' }: {
  value: GroupingValues;
  onChange: (patch: Partial<GroupingValues>) => void;
  /** A whole form in flight: every control goes quiet, the Stack photos switch included. */
  disabled?: boolean;
  idPrefix?: string;
}) {
  const off = disabled || !value.grouping_stacks;
  const sensitivity = SENSITIVITIES.find((option) => option.id === value.grouping_sensitivity) ?? SENSITIVITIES[2];

  return (
    <section className="grouping-fields" aria-label="Stacking">
      <h3 className="grouping-title">Stacking</h3>
      <label className="import-check">
        <input id={`${idPrefix}-stacks`} type="checkbox" checked={value.grouping_stacks} disabled={disabled}
          onChange={(e) => onChange({ grouping_stacks: e.target.checked })} />
        <span><b>Stack photos</b><small>Frames of one burst become one stack with a suggested keeper.</small></span>
      </label>
      {!value.grouping_stacks && (
        <p className="grouping-note">Every photo will stand alone and no keepers will be suggested.</p>
      )}
      <div className="grouping-body" aria-disabled={off || undefined}>
        <div className="import-gap">
          <label htmlFor={`${idPrefix}-sensitivity`}><b>Sensitivity</b><small>{sensitivity.hint}</small></label>
          <select id={`${idPrefix}-sensitivity`} value={value.grouping_sensitivity} disabled={off}
            onChange={(e) => onChange({ grouping_sensitivity: e.target.value as GroupingSensitivity })}>
            {SENSITIVITIES.map((option) => <option key={option.id} value={option.id}>{option.name}</option>)}
          </select>
        </div>
        <div className="import-gap">
          <label htmlFor={`${idPrefix}-pause`}><b>Pause</b><small>{PAUSE_HINT}</small></label>
          <span className="import-gap-input">
            <PauseInput id={`${idPrefix}-pause`} value={value.grouping_pause} disabled={off}
              onChange={(pause) => onChange({ grouping_pause: pause })} />
            <span className="muted">s</span>
          </span>
        </div>
        <label className="import-check">
          <input type="checkbox" checked={value.grouping_keep_pans} disabled={off}
            onChange={(e) => onChange({ grouping_keep_pans: e.target.checked })} />
          <span><b>Keep pans together</b><small>A pan is one action however far the car travels, so its frames stay one stack.</small></span>
        </label>
        <label className="import-check">
          <input type="checkbox" checked={value.grouping_merge !== false} disabled={off}
            onChange={(e) => onChange({ grouping_merge: e.target.checked })} />
          <span><b>Join look-alike bursts</b><small>Stacks shots of the same subject taken a few seconds apart.</small></span>
        </label>
        <label className="import-check">
          <input type="checkbox" checked={value.grouping_focus_anchor} disabled={off}
            onChange={(e) => onChange({ grouping_focus_anchor: e.target.checked })} />
          <span><b>Anchor on the focus area</b><small>A frame the detector missed attaches to the car the camera focused on.</small></span>
        </label>
      </div>
    </section>
  );
}

/** Seconds 0..10, kept as text while it has focus so a half-typed decimal is not rewritten under the caret. */
function PauseInput({ id, value, disabled, onChange }: { id: string; value: number; disabled: boolean; onChange: (seconds: number) => void }) {
  const [text, setText] = useState(String(value));
  const editing = useRef(false);
  useEffect(() => { if (!editing.current) setText(String(value)); }, [value]);
  return (
    <input id={id} type="number" min={0} max={10} step={0.1} value={text} disabled={disabled}
      onFocus={() => { editing.current = true; }}
      onBlur={() => { editing.current = false; setText(String(value)); }}
      onChange={(e) => {
        const next = e.target.value;
        setText(next);
        if (next.trim() === '') { onChange(0); return; }
        const seconds = Number(next);
        if (Number.isFinite(seconds)) onChange(Math.min(10, Math.max(0, seconds)));
      }} />
  );
}
