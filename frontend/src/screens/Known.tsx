import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import { Confirm, Empty, FilePick } from '../components/basics';
import { call, engine } from '../lib/api';
import { words } from '../lib/format';
import type { KnownVehicle, Toaster } from '../lib/types';
import type { Runner } from '../state/hooks';

const FIELDS = ['make', 'model', 'colour', 'team', 'race_number', 'driver', 'country'] as const;

/** A text cell that saves when it loses focus; Enter commits, Escape puts the old value back. */
function Cell({ value, onSave }: { value: string; onSave: (v: string) => void }) {
  const [text, setText] = useState(value);
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => setText(value), [value]);
  return (
    <input ref={ref} value={text} spellCheck={false} aria-label="value"
      onChange={(e) => setText(e.target.value)}
      onKeyDown={(e) => { if (e.key === 'Enter') ref.current?.blur(); if (e.key === 'Escape') { setText(value); ref.current?.blur(); } }}
      onBlur={() => { if (text !== value) onSave(text); }} />
  );
}

/** What each plate turned out to be, kept across every album. It only ever fills blanks. */
export function Known({ run, toast }: { run: Runner; toast: Toaster }) {
  const [rows, setRows] = useState<KnownVehicle[]>([]);
  const [search, setSearch] = useState('');
  const [draft, setDraft] = useState<KnownVehicle>({ plate: '' });
  const [confirmRemoveAll, setConfirmRemoveAll] = useState(false);
  const load = useCallback(() => run(async () => setRows(await engine.known())), [run]);
  useEffect(() => { void load(); }, [load]);

  const save = (v: KnownVehicle) => run(async () => { await engine.saveKnown(v); setRows(await engine.known()); });
  const remove = (plate: string) => run(async () => { await engine.deleteKnown(plate); setRows(await engine.known()); toast(`${plate} removed`); });
  const add = async (e: FormEvent) => {
    e.preventDefault();
    await save(draft);
    toast(`${draft.plate} saved`, { tone: 'ok' });
    setDraft({ plate: '' });
  };
  const needle = search.trim().toLowerCase();
  const shown = useMemo(() => rows.filter((r) => !needle || Object.values(r).join(' ').toLowerCase().includes(needle)), [rows, needle]);

  return (
    <div className="screen">
      <div className="pane wide known-page">
        <div className="known-page-head">
          <div><h2>Known vehicles</h2><p className="lede">Confirmed vehicle details reused across albums. A fresh reading always takes priority.</p></div>
          <div className="actions-row known-actions"><FilePick label="Import CSV" accept=".csv,text/csv" onFile={file => void run(async () => { const result = await call<{written: number}>('import_known', {csv: await file.text()}); await load(); toast(`Imported ${result.written} vehicles`); })} />
            <button onClick={() => void run(async () => { const csv = await call<string>('export_known'); const url = URL.createObjectURL(new Blob([csv], {type: 'text/csv'})); const a = document.createElement('a'); a.href = url; a.download = 'known-vehicles.csv'; a.click(); setTimeout(() => URL.revokeObjectURL(url), 1000); })}>Export</button>
            <button onClick={() => void run(async () => { const result = await call<{written: number}>('seed_known'); await load(); toast(`Built registry from ${result.written} cars`); })}>Rebuild from albums</button>
            <button className="ghost danger" disabled={!rows.length} onClick={() => setConfirmRemoveAll(true)}>Remove all</button>
          </div>
        </div>
        <div className="actions-row known-tools">
          <input type="search" aria-label="Search vehicles" placeholder="plate, make, model or team" spellCheck={false} value={search} onChange={(e) => setSearch(e.target.value)} />
          <span className="muted"><b>{shown.length.toLocaleString()}</b> of {rows.length.toLocaleString()} vehicles</span>
          <details className="known-add"><summary>+ Add vehicle</summary>
            <form className="known-zone" onSubmit={add}>
              <div className="known-row">
                <input aria-label="Plate" required placeholder="Plate" value={draft.plate} onChange={(e) => setDraft({ ...draft, plate: e.target.value })} />
                {FIELDS.map((k) => <input key={k} aria-label={words(k)} placeholder={words(k)} value={draft[k] ?? ''} onChange={(e) => setDraft({ ...draft, [k]: e.target.value })} />)}
                <button className="primary" disabled={!draft.plate.trim()}>Add</button>
              </div>
            </form>
          </details>
        </div>
        {rows.length ? (
          <div className="known-table-wrap">
            <div className="known-table" role="table">
              <div className="known-row known-head" role="row"><span>Plate</span>{FIELDS.map((k) => <span key={k}>{words(k)}</span>)}<span /></div>
              {shown.map((v) => (
                <div className="known-row" role="row" key={v.plate}>
                  <span className="known-plate">{v.plate}</span>
                  {FIELDS.map((k) => <Cell key={k} value={v[k] ?? ''} onSave={(text) => void save({ ...v, [k]: text })} />)}
                  <button className="ghost small danger" onClick={() => void remove(v.plate)}>Remove</button>
                </div>
              ))}
            </div>
          </div>
        ) : (
          <Empty icon="folder" title="No known vehicles yet."><p>Add a plate above and it will be remembered on every shoot.</p></Empty>
        )}
      </div>
      {confirmRemoveAll && <Confirm title="Remove all known vehicles?" action={`Remove ${rows.length.toLocaleString()} vehicles`} onCancel={() => setConfirmRemoveAll(false)} onConfirm={() => { setConfirmRemoveAll(false); void run(async () => { const result = await engine.deleteAllKnown(); setRows([]); toast(`${result.removed.toLocaleString()} known vehicles removed`); }); }}><p>This clears the reusable vehicle registry. Albums, photographs, culling results and manually written metadata are not changed.</p><p><b>This cannot be undone.</b> You can rebuild the registry from your albums later.</p></Confirm>}
    </div>
  );
}
