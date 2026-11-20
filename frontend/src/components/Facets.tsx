import { useEffect, useState } from 'react';
import { asset } from '../lib/api';
import { facetsFor, tabsWithData, validTab, type FacetKind } from '../lib/facets.mjs';
import type { FacetItem } from '../lib/review';
import type { Facet } from '../lib/types';

type Props = { preset: string; overrides?: Record<string, boolean>; facets: Record<FacetKind, FacetItem[]>; facet: Facet | null; onPick: (facet: Facet | null) => void };

const TABS: Record<FacetKind, string> = { number: 'Numbers', plate: 'Plates', make: 'Makes', team: 'Teams', person: 'People', subject: 'Subjects', moment: 'Moments' };
const EMPTY: Record<FacetKind, string> = {
  number: 'No race numbers read yet', plate: 'No plates read yet', make: 'No makes read yet', team: 'No teams read yet',
  person: 'No people found yet. Turn on people for this album', subject: 'No subjects found yet', moment: 'No moments spotted yet',
};
const sentence = (value: string) => value.charAt(0).toUpperCase() + value.slice(1);

/** The review sidebar: what this kind of shoot can be browsed by, narrowed to what the album has. Pick one to narrow the grid to it. */
export function Facets({ preset, overrides = {}, facets, facet, onPick }: Props) {
  const [wanted, setWanted] = useState<FacetKind | null>(null);
  const allowed = facetsFor(preset, overrides);
  const tabs = tabsWithData(preset, facets, overrides);
  const kind = validTab(wanted, tabs);
  const active = facet && allowed.includes(facet.kind) ? facet : null;
  // A filter that this preset no longer offers would hide frames with no way to clear it.
  useEffect(() => { if (facet && !allowed.includes(facet.kind)) onPick(null); });

  if (!allowed.length) return null;
  const items = kind ? facets[kind] : [];
  // A person is shown by name once someone is named, else by their place in the list.
  const personLabel = (value: string) => {
    const at = facets.person.findIndex((p) => p.value === value);
    return facets.person[at]?.who || `Person ${at + 1}`;
  };
  const label = (f: Facet) => (f.kind === 'person' ? personLabel(f.value) : f.kind === 'number' ? `#${f.value}` : f.kind === 'subject' || f.kind === 'moment' ? sentence(f.value.replace(/_/g, ' ')) : f.value);
  return (
    <aside className="facets" aria-label={allowed.map((k) => TABS[k]).join(', ')}>
      {tabs.length > 1 && (
        <div className="facet-tabs" role="tablist">
          {tabs.map((k) => (
            <button key={k} role="tab" aria-selected={kind === k} className={kind === k ? 'active' : ''} onClick={() => setWanted(k)}>
              {TABS[k]}
            </button>
          ))}
        </div>
      )}
      {tabs.length === 1 && <h3 className="facet-title">{TABS[tabs[0]]}</h3>}
      {active && <button className="ghost small facet-clear" onClick={() => onPick(null)}>Clear {label(active)}</button>}
      <ul className="facet-list">
        {items.map((item, i) => {
          const on = active?.kind === kind && active.value === item.value;
          return (
            <li key={item.value} className={on ? 'active' : ''}>
              <button aria-pressed={on} onClick={() => kind && onPick(on ? null : { kind, value: item.value })}>
                {kind === 'person' ? (
                  <>
                    {item.thumb && <img className="facet-face" src={asset(item.thumb)} alt="" />}
                    <span className="n">{item.who || `Person ${i + 1}`}</span>
                  </>
                ) : (
                  <>
                    <span className="n">{kind === 'number' ? '#' : ''}{kind === 'subject' ? sentence(item.value) : kind === 'moment' ? sentence(item.value.replace(/_/g, ' ')) : item.value}</span>
                    <span className="who">{item.who}</span>
                  </>
                )}
                <span className="c">{item.count}</span>
              </button>
            </li>
          );
        })}
      </ul>
      {!tabs.length && <p className="muted facet-empty">{EMPTY[allowed[0]]}</p>}
    </aside>
  );
}
