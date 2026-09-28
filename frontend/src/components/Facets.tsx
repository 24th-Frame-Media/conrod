import { useState } from 'react';
import { asset } from '../lib/api';
import type { FacetItem } from '../lib/review';
import type { Facet } from '../lib/types';

type Props = { facets: Record<Facet['kind'], FacetItem[]>; facet: Facet | null; onPick: (facet: Facet | null) => void };

const TABS = { number: 'Numbers', plate: 'Plates', person: 'People' } as const;
const EMPTY = { number: 'No race numbers read yet.', plate: 'No plates read yet.', person: 'No people found yet. Turn on people for this album.' } as const;

/** The review sidebar: numbers, plates or people found in the album; pick one to narrow the grid to it. */
export function Facets({ facets, facet, onPick }: Props) {
  const [kind, setKind] = useState<Facet['kind']>('number');
  const items = facets[kind];
  // A person is shown by name once someone is named, else by their place in the list.
  const personLabel = (value: string) => {
    const at = facets.person.findIndex((p) => p.value === value);
    return facets.person[at]?.who || `Person ${at + 1}`;
  };
  const label = (f: Facet) => (f.kind === 'plate' ? f.value : f.kind === 'person' ? personLabel(f.value) : `#${f.value}`);
  return (
    <aside className="facets" aria-label="Numbers, plates and people">
      <div className="facet-tabs" role="tablist">
        {(['number', 'plate', 'person'] as const).map((k) => (
          <button key={k} role="tab" aria-selected={kind === k} className={kind === k ? 'active' : ''} onClick={() => setKind(k)}>
            {TABS[k]}
          </button>
        ))}
      </div>
      {facet && <button className="ghost small facet-clear" onClick={() => onPick(null)}>Clear {label(facet)}</button>}
      <ul className="facet-list">
        {items.map((item, i) => {
          const active = facet?.kind === kind && facet.value === item.value;
          return (
            <li key={item.value} className={active ? 'active' : ''}>
              <button aria-pressed={active} onClick={() => onPick(active ? null : { kind, value: item.value })}>
                {kind === 'person' ? (
                  <>
                    {item.thumb && <img className="facet-face" src={asset(item.thumb)} alt="" />}
                    <span className="n">{item.who || `Person ${i + 1}`}</span>
                  </>
                ) : (
                  <>
                    <span className="n">{kind === 'number' ? '#' : ''}{item.value}</span>
                    <span className="who">{item.who}</span>
                  </>
                )}
                <span className="c">{item.count}</span>
              </button>
            </li>
          );
        })}
      </ul>
      {!items.length && <p className="muted facet-empty">{EMPTY[kind]}</p>}
    </aside>
  );
}
