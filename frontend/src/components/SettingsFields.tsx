import type { KeyboardEvent, ReactNode } from 'react';
import type { SettingsField, SettingsFieldOption, SettingsSection } from '../lib/settings.mjs';
import { fieldsOf, moveIdentifyStage, normalizeIdentifyOrder } from '../lib/settings.mjs';
import type { SettingValue } from '../lib/types';
import { ShootTypeSelect } from './ShootTypeSelect';

type SegmentedProps = {
  options: readonly SettingsFieldOption[];
  value: string;
  onChange: (next: string) => void;
  labelId?: string;
  label?: string;
  describedBy?: string;
};

/** Arrow-key aware radio group, used for strictness and the provider strategy. */
export function Segmented({ options, value, onChange, labelId, label, describedBy }: SegmentedProps) {
  const index = options.findIndex((option) => option.id === value);
  const known = index >= 0;

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const step = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
    if (!step || options.length === 0) return;
    e.preventDefault();
    const next = (Math.max(index, 0) + step + options.length) % options.length;
    onChange(options[next].id);
    e.currentTarget.querySelectorAll<HTMLButtonElement>('[role="radio"]')[next]?.focus();
  };

  return (
    <div
      className="segmented"
      role="radiogroup"
      aria-labelledby={labelId}
      aria-label={labelId ? undefined : label}
      aria-describedby={describedBy}
      onKeyDown={onKeyDown}
    >
      {options.map((option, i) => {
        const active = option.id === value;
        return (
          <button
            key={option.id}
            type="button"
            role="radio"
            aria-checked={active}
            tabIndex={active || (!known && i === 0) ? 0 : -1}
            className={`segmented-btn${active ? ' active' : ''}`}
            onClick={() => onChange(option.id)}
          >
            {option.name}
          </button>
        );
      })}
    </div>
  );
}

type SettingRowProps = {
  field: SettingsField;
  value: SettingValue;
  changed: boolean;
  onChange: (value: SettingValue) => void;
};

/** One labelled control. Everything gets an id, a label and a hint association. */
export function SettingRow({ field, value, changed, onChange }: SettingRowProps) {
  const id = `setting-${field.key}`;
  const labelId = `${id}-label`;
  const hintId = field.hint ? `${id}-hint` : undefined;
  const segmented = field.kind === 'segmented';

  const copy = (
    <>
      {field.label}
      {field.album && (
        <span className="setting-scope" title="Each album can override this">per album</span>
      )}
      {changed && <span className="sr-only">, unsaved change</span>}
    </>
  );

  let control: ReactNode;
  if (field.kind === 'toggle') {
    control = (
      <input
        id={id}
        type="checkbox"
        role="switch"
        checked={Boolean(value)}
        aria-describedby={hintId}
        onChange={(e) => onChange(e.target.checked)}
      />
    );
  } else if (field.kind === 'number') {
    control = (
      <span className={`setting-input${field.unit ? ' has-unit' : ''}`}>
        <input
          id={id}
          type="number"
          step="any"
          value={typeof value === 'number' ? value : ''}
          aria-describedby={hintId}
          onChange={(e) => onChange(e.target.value === '' ? 0 : Number(e.target.value))}
        />
        {field.unit && <span className="setting-unit" aria-hidden="true">{field.unit}</span>}
      </span>
    );
  } else if (field.kind === 'segmented') {
    control = (
      <Segmented
        options={field.options ?? []}
        value={String(value ?? '')}
        onChange={onChange}
        labelId={labelId}
        describedBy={hintId}
      />
    );
  } else if (field.kind === 'profile') {
    control = (
      <ShootTypeSelect
        id={id}
        label={field.label}
        value={String(value ?? '')}
        onChange={onChange}
      />
    );
  } else if (field.kind === 'workflow') {
    const order = normalizeIdentifyOrder(Array.isArray(value) ? value : null);
    const names: Record<string, string> = { ocr: 'OCR', decision: 'Decider', vision: 'Vision' };
    control = (
      <div className="wf-inline-order" role="group" aria-labelledby={labelId}>
        {order.map((stage, i) => (
          <button
            key={stage}
            type="button"
            className="ghost small mono"
            title={i > 0 ? `Move ${names[stage]} earlier` : `${names[stage]} runs first`}
            disabled={i === 0}
            onClick={() => onChange(moveIdentifyStage(order, stage, -1))}
          >
            {i + 1}. {names[stage] ?? stage}{i > 0 ? ' ↑' : ''}
          </button>
        ))}
      </div>
    );
  } else {
    control = (
      <input
        id={id}
        type="text"
        value={String(value ?? '')}
        spellCheck={false}
        aria-describedby={hintId}
        onChange={(e) => onChange(e.target.value)}
      />
    );
  }

  return (
    <div className={`setting${changed ? ' is-dirty' : ''}`} data-setting={field.key}>
      <div className="setting-copy">
        {segmented ? (
          <span className="label" id={labelId}>{copy}</span>
        ) : (
          <label className="label" id={labelId} htmlFor={id}>{copy}</label>
        )}
        {field.hint && <p className="hint" id={hintId}>{field.hint}</p>}
      </div>
      <div className="setting-control">{control}</div>
    </div>
  );
}

type SettingsSectionProps = {
  section: SettingsSection;
  draft: Record<string, SettingValue>;
  saved: Record<string, SettingValue>;
  onChange: (key: string, value: SettingValue) => void;
  children?: ReactNode;
};

/** A card of related settings; advanced cards are a fold. */
export function SettingsSection({ section, draft, saved, onChange, children }: SettingsSectionProps) {
  const fields = fieldsOf(section.id).filter((field) => draft[field.key] !== undefined);
  if (fields.length === 0 && !children) return null;

  const headingId = `settings-section-${section.id}`;
  const hintId = section.hint ? `${headingId}-hint` : undefined;
  const changed = fields.filter((field) => draft[field.key] !== saved[field.key]).length;
  const rows = fields.map((field) => (
    <SettingRow
      key={field.key}
      field={field}
      value={draft[field.key]}
      changed={draft[field.key] !== saved[field.key]}
      onChange={(value) => onChange(field.key, value)}
    />
  ));

  if (section.advanced) {
    return (
      <details className="settings-section settings-advanced" aria-labelledby={headingId} aria-describedby={hintId}>
        <summary>
          <span className="summary-title" role="heading" aria-level={3} id={headingId}>{section.title}</span>
          {section.hint && <span className="summary-hint" id={hintId}>{section.hint}</span>}
          {changed > 0 && <span className="tab-dirty-dot" aria-label={`${changed} unsaved change${changed === 1 ? '' : 's'}`} />}
        </summary>
        <div className="settings-advanced-body">
          {rows}
          {children}
        </div>
      </details>
    );
  }

  return (
    <section className="settings-section" aria-labelledby={headingId} aria-describedby={hintId}>
      <header className="settings-section-head">
        <h3 id={headingId}>{section.title}</h3>
        {section.hint && <p className="section-hint" id={hintId}>{section.hint}</p>}
      </header>
      {rows}
      {children}
    </section>
  );
}
