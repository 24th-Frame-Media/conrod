import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import { Confirm, Empty, FilePick } from '../components/basics';
import { call, engine } from '../lib/api';
import { words } from '../lib/format';
import type { KnownVehicle, Toaster } from '../lib/types';
import type { Runner } from '../state/hooks';

const FIELDS = ['make', 'model', 'colour', 'team', 'race_number', 'driver', 'country'] as const;
const ALL_FIELDS = ['plate', ...FIELDS] as const;

/** A text cell that saves when it loses focus; Enter commits, Escape puts the old value back. */
function Cell({
  value,
  onSave,
  className,
}: {
  value: string;
  onSave: (v: string) => void;
  className?: string;
}) {
  const [text, setText] = useState(value);
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => setText(value), [value]);

  return (
    <input
      ref={ref}
      className={className}
      value={text}
      spellCheck={false}
      aria-label="value"
      onChange={(e) => setText(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === 'Enter') ref.current?.blur();
        if (e.key === 'Escape') {
          setText(value);
          ref.current?.blur();
        }
      }}
      onBlur={() => {
        if (text !== value) onSave(text);
      }}
    />
  );
}

/** Interactive modal to resolve conflicting or duplicate vehicle records. */
function MergeDialog({
  carA,
  carB,
  onCancel,
  onMerge,
}: {
  carA: KnownVehicle;
  carB: KnownVehicle;
  onCancel: () => void;
  onMerge: (merged: KnownVehicle) => Promise<void>;
}) {
  // Initialize merged with best values from both cars
  const [merged, setMerged] = useState<KnownVehicle>(() => {
    const init: KnownVehicle = { plate: carA.plate || carB.plate };
    for (const key of FIELDS) {
      init[key] = carA[key] || carB[key] || '';
    }
    return init;
  });

  const pickField = (key: keyof KnownVehicle, val: string) => {
    setMerged((prev) => ({ ...prev, [key]: val }));
  };

  const [submitting, setSubmitting] = useState(false);

  const handleConfirm = async (e: FormEvent) => {
    e.preventDefault();
    setSubmitting(true);
    try {
      await onMerge(merged);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <dialog open className="small-dialog merge-modal" onClick={(e) => e.stopPropagation()}>
      <div className="dialog-head">
        <div>
          <h3>Merge vehicles</h3>
          <p className="dialog-sub">
            Merging <b>{carA.plate}</b> and <b>{carB.plate}</b>. Pick which values to keep or edit them directly.
          </p>
        </div>
        <button type="button" className="ghost small" onClick={onCancel}>✕</button>
      </div>

      <form onSubmit={handleConfirm} className="merge-form">
        <div className="merge-fields-list">
          {ALL_FIELDS.map((key) => {
            const valA = (carA[key] ?? '').trim();
            const valB = (carB[key] ?? '').trim();
            const current = (merged[key] ?? '').trim();

            return (
              <div className="merge-field-row" key={key}>
                <span className="merge-field-label">{words(key)}</span>
                <div className="merge-choices">
                  <button
                    type="button"
                    className={`merge-choice-btn${current === valA && valA ? ' active' : ''}`}
                    disabled={!valA}
                    onClick={() => pickField(key, valA)}
                    title={valA ? `Keep ${carA.plate}'s value` : 'No value'}
                  >
                    <span className="choice-source">A</span>
                    <span className="choice-text">{valA || '—'}</span>
                  </button>

                  <button
                    type="button"
                    className={`merge-choice-btn${current === valB && valB && valB !== valA ? ' active' : ''}`}
                    disabled={!valB}
                    onClick={() => pickField(key, valB)}
                    title={valB ? `Keep ${carB.plate}'s value` : 'No value'}
                  >
                    <span className="choice-source">B</span>
                    <span className="choice-text">{valB || '—'}</span>
                  </button>
                </div>

                <input
                  type="text"
                  className="merge-input"
                  value={merged[key] ?? ''}
                  placeholder={`Merged ${words(key).toLowerCase()}`}
                  onChange={(e) => pickField(key, e.target.value)}
                />
              </div>
            );
          })}
        </div>

        <div className="actions-row merge-actions">
          <button type="button" className="ghost" onClick={onCancel} disabled={submitting}>
            Cancel
          </button>
          <button type="submit" className="primary" disabled={submitting || !merged.plate.trim()}>
            {submitting ? 'Merging…' : `Merge into ${merged.plate.trim().toUpperCase()}`}
          </button>
        </div>
      </form>
    </dialog>
  );
}

/** What each plate turned out to be, kept across every album. It only ever fills blanks. */
export function Known({ run, toast }: { run: Runner; toast: Toaster }) {
  const [rows, setRows] = useState<KnownVehicle[]>([]);
  const [search, setSearch] = useState('');
  const [draft, setDraft] = useState<KnownVehicle>({ plate: '' });
  const [confirmRemoveAll, setConfirmRemoveAll] = useState(false);

  // Merge state
  const [selectedPlates, setSelectedPlates] = useState<Set<string>>(new Set());
  const [mergingFrom, setMergingFrom] = useState<KnownVehicle | null>(null);
  const [mergePair, setMergePair] = useState<{ a: KnownVehicle; b: KnownVehicle } | null>(null);

  const load = useCallback(() => run(async () => setRows(await engine.known())), [run]);
  useEffect(() => {
    void load();
  }, [load]);

  const save = (v: KnownVehicle) =>
    run(async () => {
      await engine.saveKnown(v);
      setRows(await engine.known());
    });

  const remove = (plate: string) =>
    run(async () => {
      await engine.deleteKnown(plate);
      setRows(await engine.known());
      toast(`${plate} removed`);
    });

  const add = async (e: FormEvent) => {
    e.preventDefault();
    const cleanPlate = draft.plate.trim().toUpperCase();
    if (!cleanPlate) return;

    const existing = rows.find((r) => r.plate.toUpperCase() === cleanPlate);
    if (existing) {
      setMergePair({ a: existing, b: { ...draft, plate: cleanPlate } });
      setDraft({ plate: '' });
      return;
    }

    await save({ ...draft, plate: cleanPlate });
    toast(`${cleanPlate} saved`, { tone: 'ok' });
    setDraft({ plate: '' });
  };

  const handleEditPlate = async (vehicle: KnownVehicle, newPlateRaw: string) => {
    const newPlate = newPlateRaw.trim().toUpperCase();
    if (!newPlate || newPlate === vehicle.plate.toUpperCase()) return;

    const conflict = rows.find(
      (r) => r.plate.toUpperCase() === newPlate && r.plate.toUpperCase() !== vehicle.plate.toUpperCase()
    );

    if (conflict) {
      setMergePair({ a: vehicle, b: conflict });
      return;
    }

    await save({ ...vehicle, plate: newPlate, old_plate: vehicle.plate });
    toast(`Updated plate to ${newPlate}`, { tone: 'ok' });
  };

  const toggleSelect = (plate: string) => {
    setSelectedPlates((prev) => {
      const next = new Set(prev);
      if (next.has(plate)) next.delete(plate);
      else next.add(plate);
      return next;
    });
  };

  const handleMergeSelected = () => {
    const plates = [...selectedPlates];
    if (plates.length !== 2) return;
    const a = rows.find((r) => r.plate === plates[0]);
    const b = rows.find((r) => r.plate === plates[1]);
    if (a && b) {
      setMergePair({ a, b });
      setSelectedPlates(new Set());
    }
  };

  const executeMerge = async (merged: KnownVehicle) => {
    if (!mergePair) return;
    const { a, b } = mergePair;
    const targetPlate = merged.plate.trim().toUpperCase();

    await run(async () => {
      // 1. Save merged record
      await engine.saveKnown({ ...merged, plate: targetPlate });

      // 2. Clean up old plates if different from the target merged plate
      if (a.plate.toUpperCase() !== targetPlate) {
        await engine.deleteKnown(a.plate);
      }
      if (b.plate.toUpperCase() !== targetPlate) {
        await engine.deleteKnown(b.plate);
      }

      setRows(await engine.known());
      toast(`Merged ${a.plate} and ${b.plate} into ${targetPlate}`, { tone: 'ok' });
      setMergePair(null);
      setMergingFrom(null);
    });
  };

  const needle = search.trim().toLowerCase();
  const shown = useMemo(
    () => rows.filter((r) => !needle || Object.values(r).join(' ').toLowerCase().includes(needle)),
    [rows, needle]
  );

  return (
    <div className="known-section">
      <div className="pane wide known-page">
        <div className="known-page-head">
          <div>
            <h2>Known vehicles</h2>
            <p className="lede">
              Confirmed vehicle details reused across albums. A fresh reading always takes priority.
            </p>
          </div>
          <div className="actions-row known-actions">
            <FilePick
              label="Import CSV"
              accept=".csv,text/csv"
              onFile={(file) =>
                void run(async () => {
                  const result = await call<{ written: number }>('import_known', { csv: await file.text() });
                  await load();
                  toast(`Imported ${result.written} vehicles`);
                })
              }
            />
            <button
              type="button"
              onClick={() =>
                void run(async () => {
                  const csv = await call<string>('export_known');
                  const url = URL.createObjectURL(new Blob([csv], { type: 'text/csv' }));
                  const a = document.createElement('a');
                  a.href = url;
                  a.download = 'known-vehicles.csv';
                  a.click();
                  setTimeout(() => URL.revokeObjectURL(url), 1000);
                })
              }
            >
              Export
            </button>
            <button
              type="button"
              onClick={() =>
                void run(async () => {
                  const result = await call<{ written: number }>('seed_known');
                  await load();
                  toast(`Built registry from ${result.written} cars`);
                })
              }
            >
              Rebuild from albums
            </button>
            <button
              type="button"
              className="ghost danger"
              disabled={!rows.length}
              onClick={() => setConfirmRemoveAll(true)}
            >
              Remove all
            </button>
          </div>
        </div>

        {/* Merge Helper Banner if picking 2nd car */}
        {mergingFrom && (
          <div className="known-merge-banner">
            <span>
              Merging from <b>{mergingFrom.plate}</b>. Click <b>"Merge with this"</b> on another vehicle to combine.
            </span>
            <button type="button" className="ghost small" onClick={() => setMergingFrom(null)}>
              Cancel
            </button>
          </div>
        )}

        <div className="actions-row known-tools">
          <input
            type="search"
            aria-label="Search vehicles"
            placeholder="plate, make, model or team"
            spellCheck={false}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <span className="muted">
            <b>{shown.length.toLocaleString()}</b> of {rows.length.toLocaleString()} vehicles
          </span>

          {selectedPlates.size === 2 && (
            <button type="button" className="primary small" onClick={handleMergeSelected}>
              Merge 2 selected vehicles…
            </button>
          )}

          {selectedPlates.size > 0 && selectedPlates.size !== 2 && (
            <button type="button" className="ghost small" onClick={() => setSelectedPlates(new Set())}>
              Clear selection ({selectedPlates.size})
            </button>
          )}

          <details className="known-add">
            <summary>+ Add vehicle</summary>
            <form className="known-zone" onSubmit={add}>
              <div className="known-row">
                <span />
                <input
                  aria-label="Plate"
                  required
                  placeholder="Plate"
                  value={draft.plate}
                  onChange={(e) => setDraft({ ...draft, plate: e.target.value })}
                />
                {FIELDS.map((k) => (
                  <input
                    key={k}
                    aria-label={words(k)}
                    placeholder={words(k)}
                    value={draft[k] ?? ''}
                    onChange={(e) => setDraft({ ...draft, [k]: e.target.value })}
                  />
                ))}
                <button type="submit" className="primary" disabled={!draft.plate.trim()}>
                  Add
                </button>
              </div>
            </form>
          </details>
        </div>

        {rows.length ? (
          <div className="known-table-wrap">
            <div className="known-table" role="table">
              <div className="known-row known-head" role="row">
                <span className="known-col-select" />
                <span>Plate</span>
                {FIELDS.map((k) => (
                  <span key={k}>{words(k)}</span>
                ))}
                <span className="known-col-actions">Actions</span>
              </div>
              {shown.map((v) => {
                const isSelected = selectedPlates.has(v.plate);
                const isMergingSource = mergingFrom?.plate === v.plate;

                return (
                  <div
                    className={`known-row${isSelected ? ' selected' : ''}${isMergingSource ? ' merging-source' : ''}`}
                    role="row"
                    key={v.plate}
                  >
                    <input
                      type="checkbox"
                      className="known-select-chk"
                      aria-label={`Select ${v.plate}`}
                      checked={isSelected}
                      onChange={() => toggleSelect(v.plate)}
                    />
                    <Cell
                      className="known-plate-input"
                      value={v.plate}
                      onSave={(text) => void handleEditPlate(v, text)}
                    />
                    {FIELDS.map((k) => (
                      <Cell
                        key={k}
                        value={v[k] ?? ''}
                        onSave={(text) => void save({ ...v, [k]: text })}
                      />
                    ))}
                    <div className="known-row-actions">
                      {mergingFrom ? (
                        mergingFrom.plate !== v.plate ? (
                          <button
                            type="button"
                            className="primary small"
                            onClick={() => {
                              setMergePair({ a: mergingFrom, b: v });
                            }}
                          >
                            Merge with this
                          </button>
                        ) : (
                          <span className="merging-tag">Source</span>
                        )
                      ) : (
                        <button
                          type="button"
                          className="ghost small"
                          title="Merge with another vehicle"
                          onClick={() => setMergingFrom(v)}
                        >
                          Merge
                        </button>
                      )}
                      <button
                        type="button"
                        className="ghost small danger"
                        onClick={() => void remove(v.plate)}
                      >
                        ✕
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        ) : (
          <Empty icon="folder" title="No known vehicles yet.">
            <p>Add a plate above and it will be remembered on every shoot.</p>
          </Empty>
        )}
      </div>

      {/* Merge Modal Dialog */}
      {mergePair && (
        <MergeDialog
          carA={mergePair.a}
          carB={mergePair.b}
          onCancel={() => {
            setMergePair(null);
            setMergingFrom(null);
          }}
          onMerge={executeMerge}
        />
      )}

      {/* Remove All Confirm */}
      {confirmRemoveAll && (
        <Confirm
          title="Remove all known vehicles?"
          action={`Remove ${rows.length.toLocaleString()} vehicles`}
          onCancel={() => setConfirmRemoveAll(false)}
          onConfirm={() => {
            setConfirmRemoveAll(false);
            void run(async () => {
              const result = await engine.deleteAllKnown();
              setRows([]);
              toast(`${result.removed.toLocaleString()} known vehicles removed`);
            });
          }}
        >
          <p>
            This clears the reusable vehicle registry. Albums, photographs, culling results and manually written metadata
            are not changed.
          </p>
          <p>
            <b>This cannot be undone.</b> You can rebuild the registry from your albums later.
          </p>
        </Confirm>
      )}
    </div>
  );
}
