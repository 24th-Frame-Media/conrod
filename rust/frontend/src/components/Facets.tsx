import { useState } from 'react';
import type { FacetItem } from '../lib/review';
import type { Facet } from '../lib/types';

type Props = { facets: Record<Facet['kind'], FacetItem[]>; facet: Facet | null; onPick: (facet: Facet | null) => void };

/** The review sidebar: numbers or plates found in the album; pick one to narrow the grid to it. */
export function Facets({ facets, facet, onPick }: Props) {
  const [kind, setKind] = useState<Facet['kind']>('number');
  const items = facets[kind];
  return (
    <aside className="facets" aria-label="Numbers and plates">
      <div className="facet-tabs" role="tablist">
        {(['number', 'plate'] as const).map((k) => (
          <button key={k} role="tab" aria-selected={kind === k} className={kind === k ? 'active' : ''} onClick={() => setKind(k)}>
            {k === 'number' ? 'Numbers' : 'Plates'}
          </button>
        ))}
      </div>
      {facet && <button className="ghost small facet-clear" onClick={() => onPick(null)}>Clear {facet.kind === 'plate' ? facet.value : `#${facet.value}`}</button>}
      <ul className="facet-list">
        {items.map((item) => {
          const active = facet?.kind === kind && facet.value === item.value;
          return (
            <li key={item.value} className={active ? 'active' : ''}>
              <button aria-pressed={active} onClick={() => onPick(active ? null : { kind, value: item.value })}>
                <span className="n">{kind === 'number' ? '#' : ''}{item.value}</span>
                <span className="who">{item.who}</span>
                <span className="c">{item.count}</span>
              </button>
            </li>
          );
        })}
      </ul>
      {!items.length && <p className="muted facet-empty">{kind === 'number' ? 'No race numbers read yet.' : 'No plates read yet.'}</p>}
    </aside>
  );
}
