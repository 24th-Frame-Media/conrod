import { useEffect } from 'react';
import { usePresetPolicy } from '../state/usePresetPolicy';
import { needsLabel, nextOverrides, noteList, type Overrides, type PolicyRow, type PolicySummary } from '../lib/presetPolicy.mjs';

type Props = {
  preset: string;
  /** The album's own switches; a missing key follows the shoot type. */
  overrides: Overrides;
  onChange: (next: Overrides) => void;
  /** An album's own copy of the photographer's defaults for this type; without it the saved profile applies. */
  user?: Overrides;
  disabled?: boolean;
  /** Called with each new summary, for callers that react to it (the entry list shows when numbers are read). */
  onSummary?: (summary: PolicySummary) => void;
};

/** "What Conrod will do" for the picked shoot type: what it looks for and reads, each switchable, plus the settings that are not switches. */
export function PresetPolicy({ preset, overrides, onChange, user, disabled, onSummary }: Props) {
  const summary = usePresetPolicy(preset, overrides, user);
  useEffect(() => { onSummary?.(summary); }, [summary]); // eslint-disable-line react-hooks/exhaustive-deps -- report each new answer
  const list = (title: string, rows: PolicyRow[]) => (
    <>
      <h4>{title}</h4>
      <ul className="policy-list">
        {rows.map((row) => (
          <li key={row.key} className={`policy-row${row.on ? ' on' : ''}`}>
            <label>
              <input type="checkbox" checked={row.on} disabled={disabled || row.blocked} onChange={(e) => onChange(nextOverrides(overrides, row, e.target.checked))} />
              <span>{row.label}</span>
            </label>
            {row.blocked && <small className="muted">{needsLabel(summary, row)}</small>}
            {row.overridden && !row.blocked && <small className="policy-tag">{row.default ? 'off for this album' : 'on for this album'}</small>}
          </li>
        ))}
      </ul>
    </>
  );
  return (
    <section className="policy" aria-label="What Conrod will do for this shoot type">
      <p className="policy-title">For {summary.label.toLowerCase()}, Conrod will</p>
      {list('Look for', summary.detects)}
      {list('Read', summary.reads)}
      {summary.moments.vocabulary.length > 0 && (
        <>
          <h4>Moments</h4>
          <ul className="policy-list">
            {[summary.moments.detect, summary.moments.rank].map((row) => (
              <li key={row.key} className={`policy-row${row.on ? ' on' : ''}`}>
                <label>
                  <input type="checkbox" checked={row.on} disabled={disabled || row.blocked} onChange={(e) => onChange(nextOverrides(overrides, row, e.target.checked))} />
                  <span>{row.label}</span>
                </label>
                {row.blocked && <small className="muted">{needsLabel(summary, row)}</small>}
                {row.overridden && !row.blocked && <small className="policy-tag">{row.default ? 'off for this album' : 'on for this album'}</small>}
              </li>
            ))}
          </ul>
          <p className="policy-moments muted">{summary.moments.vocabulary.map((m) => m.label).join(' \u00b7 ')}</p>
        </>
      )}
      <p className="policy-notes muted">{noteList(summary).join(' \u00b7 ')}</p>
    </section>
  );
}
