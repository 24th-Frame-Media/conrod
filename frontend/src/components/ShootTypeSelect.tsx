import { PROFILE_GROUPS, profileParent } from '../lib/types';

const KNOWN = new Set(PROFILE_GROUPS.flatMap((g) => [g.id, ...g.children.map((c) => c.id)]));

type Props = { value: string; onChange: (profile: string) => void; id?: string; label?: string };

/** The one shoot-type control: every preset in one list, grouped. Used by the import sheet and the album's shoot settings. */
export function ShootTypeSelect({ value, onChange, id, label = 'Shoot type' }: Props) {
  const current = KNOWN.has(value) ? value : profileParent(value);
  const group = PROFILE_GROUPS.find((g) => g.id === profileParent(current));
  return (
    <div className="shoot-type">
      <select id={id} aria-label={label} value={current} onChange={(e) => onChange(e.target.value)}>
        {PROFILE_GROUPS.map((g) => (
          <optgroup key={g.id} label={g.title}>
            <option value={g.id}>{g.title} · general</option>
            {g.children.map((child) => <option key={child.id} value={child.id}>{g.title} · {child.title}</option>)}
          </optgroup>
        ))}
      </select>
      {group && <small className="muted">{group.hint}</small>}
    </div>
  );
}
