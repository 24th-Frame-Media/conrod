import { useMemo } from 'react';
import { asset } from '../lib/api';
import { parseAttributes } from '../lib/review';
import {
  ATTRIBUTE_KEYS,
  type AttributeKey,
  type Detection,
  type Frame,
  type KnownVehicle,
} from '../lib/types';

export type CategoryFilter = 'all' | 'vehicles' | 'people' | 'landmarks';

export const LABELS: Record<string, string> = {
  plate: 'Plate',
  race_number: 'No.',
  make: 'Make',
  model: 'Model',
  colour: 'Colour',
  team: 'Team',
  driver: 'Driver',
  country: 'Country',
  plate_state: 'State',
  body_type: 'Body',
};

const EVIDENCE_FIELDS = ['plate', 'race_number', 'make', 'model', 'team', 'driver', 'country'] as const;

export function getCategory(d: Detection): 'vehicle' | 'person' | 'plate' | 'landmark' {
  const cls = (d.cls || '').toLowerCase();
  const reg = (d.region_type || '').toLowerCase();
  if (cls === 'eye' || reg === 'eye') return 'landmark';
  if (cls === 'plate' || reg === 'plate') return 'plate';
  if (cls === 'person' || cls === 'face' || reg === 'person' || reg === 'face') return 'person';
  return 'vehicle';
}

function categoryIcon(cat: 'vehicle' | 'person' | 'plate' | 'landmark'): string {
  switch (cat) {
    case 'vehicle':
      return '🚗';
    case 'person':
      return '👤';
    case 'plate':
      return '🔢';
    case 'landmark':
      return '👁';
  }
}

type Props = {
  detections: Detection[];
  selectedId: number | null;
  hoveredId?: number | null;
  categoryFilter: CategoryFilter;
  onSelect: (id: number | null) => void;
  onHover?: (id: number | null) => void;
  onFilterChange: (cat: CategoryFilter) => void;
  onEdit?: (id: number, updates: Record<string, string | string[] | null>) => unknown;
  allDetections?: Detection[];
  allFrames?: Frame[];
  known?: KnownVehicle[];
  frameBurstKey?: number | null;
  emptyMessage?: string;
};

export function DetectionsTable({
  detections,
  selectedId,
  hoveredId,
  categoryFilter,
  onSelect,
  onHover,
  onFilterChange,
  onEdit,
  allDetections,
  allFrames,
  known,
  frameBurstKey,
  emptyMessage,
}: Props) {
  const counts = useMemo(() => {
    let vehicles = 0;
    let people = 0;
    let landmarks = 0;
    let plates = 0;
    for (const d of detections) {
      const cat = getCategory(d);
      if (cat === 'vehicle') vehicles++;
      else if (cat === 'person') people++;
      else if (cat === 'plate') plates++;
      else landmarks++;
    }
    return { all: detections.length, vehicles, people, landmarks, plates };
  }, [detections]);

  const filtered = useMemo(() => {
    if (categoryFilter === 'all') return detections;
    if (categoryFilter === 'vehicles') {
      return detections.filter((d) => {
        const cat = getCategory(d);
        return cat === 'vehicle' || cat === 'plate';
      });
    }
    if (categoryFilter === 'people') {
      return detections.filter((d) => getCategory(d) === 'person');
    }
    if (categoryFilter === 'landmarks') {
      return detections.filter((d) => getCategory(d) === 'landmark');
    }
    return detections;
  }, [detections, categoryFilter]);

  // Precompute group evidence and seen-before matches for each detection
  const evidenceMap = useMemo(() => {
    const map = new Map<
      number,
      {
        peers: Detection[];
        suggestions: { key: (typeof EVIDENCE_FIELDS)[number]; values: [string, number][] }[];
        seen?: KnownVehicle & { similarity?: number };
      }
    >();

    if (!allDetections || allDetections.length === 0) return map;

    const burst = frameBurstKey;
    const burstImages = new Set(
      allFrames ? allFrames.filter((f) => burst != null && f.burst_key === burst).map((f) => f.id) : []
    );

    for (const d of detections) {
      const identity = parseAttributes(d);
      const sameIdentity = (other: Detection) => {
        const candidate = parseAttributes(other);
        return Boolean(
          (identity.plate && candidate.plate === identity.plate) ||
          (identity.race_number && candidate.race_number === identity.race_number) ||
          (identity.make && identity.model && candidate.make === identity.make && candidate.model === identity.model)
        );
      };

      const peers = allDetections.filter(
        (other) =>
          other.id === d.id ||
          (d.group_key != null
            ? other.group_key === d.group_key
            : burstImages.has(other.image_id) && sameIdentity(other))
      );

      const readings = peers.map(parseAttributes);
      const suggestions = EVIDENCE_FIELDS.map((key) => {
        const valMap = new Map<string, number>();
        readings
          .map((a) => a[key])
          .filter(Boolean)
          .forEach((value) => valMap.set(value!, (valMap.get(value!) ?? 0) + 1));
        return { key, values: [...valMap.entries()].sort((a, b) => b[1] - a[1]) };
      }).filter(
        (item) =>
          item.values.length > 0 &&
          (item.values.length > 1 || item.values[0][0] !== identity[item.key as AttributeKey])
      );

      const seen =
        d.known_match ??
        known?.find(
          (car) =>
            (identity.plate && car.plate.toUpperCase() === identity.plate.toUpperCase()) ||
            (identity.race_number &&
              car.race_number === identity.race_number &&
              (!identity.make || !car.make || car.make.toLowerCase() === identity.make.toLowerCase())) ||
            (identity.make &&
              identity.model &&
              car.make?.toLowerCase() === identity.make.toLowerCase() &&
              car.model?.toLowerCase() === identity.model.toLowerCase() &&
              (!identity.colour || !car.colour || car.colour.toLowerCase() === identity.colour.toLowerCase()))
        );

      map.set(d.id, { peers, suggestions, seen });
    }

    return map;
  }, [detections, allDetections, allFrames, known, frameBurstKey]);

  return (
    <div className="detections-panel">
      {/* Panel Header */}
      <div className="detections-header">
        <div className="detections-title">
          <span className="det-heading">Detections</span>
          <span className="det-total-badge">{detections.length}</span>
        </div>

        {/* Category Tabs */}
        <div className="det-tabs" role="tablist" aria-label="Detection categories">
          <button
            type="button"
            className={`det-tab${categoryFilter === 'all' ? ' active' : ''}`}
            onClick={() => onFilterChange('all')}
            title="Show all detections"
          >
            All <span className="det-count">{counts.all}</span>
          </button>
          {counts.vehicles > 0 && (
            <button
              type="button"
              className={`det-tab tab-vehicles${categoryFilter === 'vehicles' ? ' active' : ''}`}
              onClick={() => onFilterChange('vehicles')}
              title="Show vehicle detections"
            >
              Vehicles <span className="det-count">{counts.vehicles}</span>
            </button>
          )}
          {counts.people > 0 && (
            <button
              type="button"
              className={`det-tab tab-people${categoryFilter === 'people' ? ' active' : ''}`}
              onClick={() => onFilterChange('people')}
              title="Show person & face detections"
            >
              People <span className="det-count">{counts.people}</span>
            </button>
          )}
          {counts.landmarks > 0 && (
            <button
              type="button"
              className={`det-tab tab-landmarks${categoryFilter === 'landmarks' ? ' active' : ''}`}
              onClick={() => onFilterChange('landmarks')}
              title="Show eye landmarks"
            >
              Eyes <span className="det-count">{counts.landmarks}</span>
            </button>
          )}
        </div>
      </div>

      {/* Detections List */}
      <div className="detections-list">
        {filtered.length === 0 ? (
          <div className="empty-detections">
            {emptyMessage || (detections.length === 0 ? 'No detections in this frame' : 'No detections in this category')}
          </div>
        ) : (
          filtered.map((d, idx) => {
            const cat = getCategory(d);
            const a = parseAttributes(d);
            const isSelected = selectedId === d.id;
            const isHovered = hoveredId === d.id;
            const vehicleName = [a.make, a.model].filter(Boolean).join(' ');
            const personName = a.person_name;
            const primaryTitle =
              vehicleName ||
              personName ||
              (d.cls === 'face'
                ? `Face #${idx + 1}`
                : d.cls === 'person'
                ? `Person #${idx + 1}`
                : d.cls === 'eye'
                ? `Eye #${idx + 1}`
                : `${d.cls} #${idx + 1}`);

            const secondaryDetails = [
              a.body_type,
              a.colour,
              a.country,
              a.race_number ? `#${a.race_number}` : null,
            ]
              .filter(Boolean)
              .join(' · ');

            const sharpnessVal = typeof d.sharpness === 'number' && !isNaN(d.sharpness) ? d.sharpness : 0;
            const scorePct = Math.round(Math.min(1, Math.max(0, sharpnessVal)) * 100);
            const scoreTone = sharpnessVal >= 0.75 ? 'sharp' : sharpnessVal >= 0.5 ? 'fair' : 'soft';
            const widthPx = Math.round(Math.abs(d.x2 - d.x1));
            const heightPx = Math.round(Math.abs(d.y2 - d.y1));

            const evidence = evidenceMap.get(d.id);
            const isVehicle = cat === 'vehicle' || cat === 'plate';
            const isPerson = cat === 'person';
            const isLandmark = cat === 'landmark';

            return (
              <div
                key={d.id}
                className={`det-card det-card-${cat}${isSelected ? ' selected' : ''}${isHovered ? ' hovered' : ''}`}
                onClick={() => onSelect(isSelected ? null : d.id)}
                onMouseEnter={() => onHover?.(d.id)}
                onMouseLeave={() => onHover?.(null)}
              >
                <div className="det-card-main">
                  {/* Fixed-size Avatar Container */}
                  <div className={`det-avatar cat-${cat}`}>
                    {d.crop_path ? (
                      <img
                        src={asset(d.crop_path)}
                        alt={d.cls}
                        loading="lazy"
                      />
                    ) : (
                      <span className="det-avatar-icon" title={cat}>
                        {categoryIcon(cat)}
                      </span>
                    )}
                  </div>

                  {/* Subject Info */}
                  <div className="det-info">
                    <div className="det-title-row">
                      <span className={`det-pill pill-${cat}`}>{d.cls}</span>
                      {a.plate && <span className="det-chip-plate">{a.plate}</span>}
                      {d.burst_pick ? <span className="det-badge badge-keeper">★ Best</span> : null}
                      {d.panning ? <span className="det-badge badge-panned">panned</span> : null}
                      {d.cull_reason ? <span className="det-badge badge-culled">{d.cull_reason}</span> : null}
                    </div>

                    <div className="det-sub">
                      <span className="det-title">{primaryTitle}</span>
                      {secondaryDetails ? (
                        <span className="det-sub-muted">· {secondaryDetails}</span>
                      ) : (
                        <span className="det-sub-muted">· {widthPx}×{heightPx}px</span>
                      )}
                    </div>
                  </div>

                  {/* Metrics */}
                  <div className="det-metrics">
                    <span className={`det-focus-num ${scoreTone}`}>
                      {sharpnessVal.toFixed(3)}
                    </span>
                    <div className="det-meter-bg" title={`Focus score: ${sharpnessVal.toFixed(3)}`}>
                      <div className={`det-meter-fill ${scoreTone}`} style={{ width: `${scorePct}%` }} />
                    </div>
                  </div>
                </div>

                {/* Expandable Drawer for Selected Detection */}
                {isSelected && (
                  <div className="det-drawer" onClick={(e) => e.stopPropagation()}>
                    {/* Seen before banner for vehicle */}
                    {evidence?.seen && (
                      <div className="det-seen-box">
                        <div className="det-seen-text">
                          <span className="det-seen-badge">
                            Seen before{evidence.seen.similarity ? ` · ${Math.round(evidence.seen.similarity * 100)}% match` : ''}
                          </span>
                          <span className="det-seen-name">
                            {evidence.seen.plate} {[evidence.seen.make, evidence.seen.model].filter(Boolean).join(' ')}
                          </span>
                        </div>
                        {onEdit && (
                          <button
                            type="button"
                            className="det-apply-btn"
                            onClick={() =>
                              onEdit(d.id, {
                                plate: evidence.seen!.plate,
                                make: evidence.seen!.make ?? null,
                                model: evidence.seen!.model ?? null,
                                colour: evidence.seen!.colour ?? null,
                                team: evidence.seen!.team ?? null,
                                race_number: evidence.seen!.race_number ?? null,
                                driver: evidence.seen!.driver ?? null,
                                country: evidence.seen!.country ?? null,
                              })
                            }
                          >
                            Fill known
                          </button>
                        )}
                      </div>
                    )}

                    {/* Group evidence suggestions */}
                    {evidence && evidence.suggestions.length > 0 && (
                      <div className="det-evidence-box">
                        <div className="det-evidence-head">
                          <span>Group suggestions</span>
                          <span className="det-count">{evidence.peers.length} sightings</span>
                          {d.group_agreement != null && (
                            <span className="det-count">{Math.round(d.group_agreement * 100)}% agreement</span>
                          )}
                        </div>
                        <div className="det-evidence-rows">
                          {evidence.suggestions.map(({ key, values }) => (
                            <div className="det-evidence-row" key={key}>
                              <span className="det-evidence-lbl">{LABELS[key]}</span>
                              <div className="det-evidence-chips">
                                {values.map(([val, cnt], i) => (
                                  <button
                                    type="button"
                                    key={val}
                                    className={`det-choice-chip${i === 0 ? ' likely' : ''}`}
                                    title={`Seen in ${cnt} photo${cnt === 1 ? '' : 's'}`}
                                    onClick={() => onEdit?.(d.id, { [key]: val })}
                                  >
                                    {val} <small>{cnt}</small>
                                  </button>
                                ))}
                              </div>
                            </div>
                          ))}
                        </div>
                      </div>
                    )}

                    {/* Known person match banner */}
                    {isPerson && d.known_person_match && !a.person_name && (
                      <div className="det-seen-box">
                        <div className="det-seen-text">
                          <span className="det-seen-badge">Seen before?</span>
                          <span className="det-seen-name">
                            {d.known_person_match.name}
                            {d.known_person_match.country ? ` · ${d.known_person_match.country}` : ''}
                            {d.known_person_match.similarity
                              ? ` · ${Math.round(d.known_person_match.similarity * 100)}% match`
                              : ''}
                          </span>
                        </div>
                        {onEdit && (
                          <button
                            type="button"
                            className="det-apply-btn"
                            onClick={() =>
                              onEdit(d.id, {
                                person_name: d.known_person_match!.name,
                                country: d.known_person_match!.country ?? null,
                              })
                            }
                          >
                            Use name
                          </button>
                        )}
                      </div>
                    )}

                    {onEdit ? (
                      /* Editable Fields Form */
                      <div className="det-edit-section">
                        {isVehicle && (
                          <>
                            <label className="det-edit-field">
                              <span className="det-edit-lbl">Sponsors</span>
                              <input
                                type="text"
                                className="det-edit-input"
                                key={`${d.id}-sponsors-${(a.sponsors ?? []).join(',')}`}
                                defaultValue={(a.sponsors ?? []).join(', ')}
                                placeholder="Comma-separated names"
                                spellCheck={false}
                                onKeyDown={(e) => {
                                  if (e.key === 'Enter') e.currentTarget.blur();
                                }}
                                onBlur={(e) => {
                                  const sponsors = e.target.value.split(',').map((s) => s.trim()).filter(Boolean);
                                  if (JSON.stringify(sponsors) !== JSON.stringify(a.sponsors ?? [])) {
                                    onEdit(d.id, { sponsors });
                                  }
                                }}
                              />
                            </label>

                            <div className="det-edit-grid">
                              {ATTRIBUTE_KEYS.map((key) => {
                                const val = a[key] ?? '';
                                return (
                                  <label className="det-edit-field" key={`${d.id}-${key}-${val}`}>
                                    <span className="det-edit-lbl">{LABELS[key]}</span>
                                    <input
                                      type="text"
                                      className="det-edit-input"
                                      defaultValue={val}
                                      placeholder="—"
                                      autoComplete="off"
                                      spellCheck={false}
                                      onKeyDown={(e) => {
                                        if (e.key === 'Enter') e.currentTarget.blur();
                                      }}
                                      onBlur={(e) => {
                                        const newVal = e.target.value.trim();
                                        if (newVal !== val) {
                                          onEdit(d.id, { [key]: newVal || null });
                                        }
                                      }}
                                    />
                                  </label>
                                );
                              })}
                            </div>
                          </>
                        )}

                        {isPerson && (
                          <div className="det-edit-grid">
                            <label className="det-edit-field">
                              <span className="det-edit-lbl">Name</span>
                              <input
                                type="text"
                                className="det-edit-input"
                                defaultValue={a.person_name ?? ''}
                                placeholder="—"
                                autoComplete="off"
                                spellCheck={false}
                                onKeyDown={(e) => {
                                  if (e.key === 'Enter') e.currentTarget.blur();
                                }}
                                onBlur={(e) => {
                                  const newVal = e.target.value.trim();
                                  if (newVal !== (a.person_name ?? '')) {
                                    onEdit(d.id, { person_name: newVal || null });
                                  }
                                }}
                              />
                            </label>
                            <label className="det-edit-field">
                              <span className="det-edit-lbl">Country</span>
                              <input
                                type="text"
                                className="det-edit-input"
                                defaultValue={a.country ?? ''}
                                placeholder="—"
                                autoComplete="off"
                                spellCheck={false}
                                onKeyDown={(e) => {
                                  if (e.key === 'Enter') e.currentTarget.blur();
                                }}
                                onBlur={(e) => {
                                  const newVal = e.target.value.trim();
                                  if (newVal !== (a.country ?? '')) {
                                    onEdit(d.id, { country: newVal || null });
                                  }
                                }}
                              />
                            </label>
                          </div>
                        )}

                        {isLandmark && (
                          <div className="det-meta-grid">
                            <div className="det-meta-item">
                              <dt>Type</dt>
                              <dd>{d.cls}</dd>
                            </div>
                            <div className="det-meta-item">
                              <dt>Dimensions</dt>
                              <dd>{widthPx} × {heightPx} px</dd>
                            </div>
                            <div className="det-meta-item">
                              <dt>Sharpness</dt>
                              <dd>{sharpnessVal.toFixed(3)}</dd>
                            </div>
                          </div>
                        )}
                      </div>
                    ) : (
                      /* Read-Only Metadata Grid */
                      <div className="det-meta-grid">
                        {a.make && (
                          <div className="det-meta-item">
                            <dt>Make</dt>
                            <dd>{a.make}</dd>
                          </div>
                        )}
                        {a.model && (
                          <div className="det-meta-item">
                            <dt>Model</dt>
                            <dd>{a.model}</dd>
                          </div>
                        )}
                        {a.team && (
                          <div className="det-meta-item">
                            <dt>Team</dt>
                            <dd>{a.team}</dd>
                          </div>
                        )}
                        {a.driver && (
                          <div className="det-meta-item">
                            <dt>Driver</dt>
                            <dd>{a.driver}</dd>
                          </div>
                        )}
                        {a.person_name && (
                          <div className="det-meta-item">
                            <dt>Person</dt>
                            <dd>{a.person_name}</dd>
                          </div>
                        )}
                        {a.country && (
                          <div className="det-meta-item">
                            <dt>Country</dt>
                            <dd>{a.country}</dd>
                          </div>
                        )}
                        <div className="det-meta-item">
                          <dt>Dimensions</dt>
                          <dd>{widthPx} × {heightPx} px</dd>
                        </div>
                        <div className="det-meta-item">
                          <dt>Sharpness</dt>
                          <dd>{sharpnessVal.toFixed(3)}</dd>
                        </div>
                        {a.sponsors && a.sponsors.length > 0 && (
                          <div className="det-sponsors" style={{ gridColumn: '1 / -1' }}>
                            <span>Sponsors:</span>
                            {a.sponsors.map((s) => (
                              <span key={s} className="det-sponsor-tag">{s}</span>
                            ))}
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
