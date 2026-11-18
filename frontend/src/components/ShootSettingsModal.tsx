import { Fragment, useRef, useState, useEffect } from 'react';
import { Icon, Portal, useBackdropClose } from './basics';
import { call } from '../lib/api';
import { statSections, type ShootStats } from '../lib/shot.mjs';
import { type Settings } from '../lib/types';
import { ShootTypeSelect } from './ShootTypeSelect';
import { GroupingFields, groupingOf, type GroupingValues } from './GroupingFields';

type Props = {
  jobId: number;
  jobLabel: string;
  initialProfile: string;
  initialSettings: Settings;
  visible: boolean;
  onClose: () => void;
  /** The statistics arrived; the first call may have backfilled EXIF into the album's frames. */
  onStatsLoaded?: () => void;
  onSave: (patch: {
    scan_profile: string;
    read_numbers: boolean;
    read_plates: boolean;
    group_vehicles: boolean;
  } & GroupingValues) => Promise<void>;
};

export function ShootSettingsModal({
  jobId,
  jobLabel,
  initialProfile,
  initialSettings,
  visible,
  onClose,
  onStatsLoaded,
  onSave,
}: Props) {
  const [profile, setProfile] = useState(initialProfile || 'motorsport');
  const [readNumbers, setReadNumbers] = useState(initialSettings.read_numbers !== false);
  const [readPlates, setReadPlates] = useState(initialSettings.read_plates !== false);
  const [groupVehicles, setGroupVehicles] = useState(initialSettings.group_vehicles !== false);
  const [grouping, setGrouping] = useState<GroupingValues>(() => groupingOf(initialSettings));
  const [saving, setSaving] = useState(false);
  const [stats, setStats] = useState<ShootStats | null>(null);
  const [statsLoading, setStatsLoading] = useState(false);
  const [statsError, setStatsError] = useState<string | null>(null);
  const backdrop = useBackdropClose(onClose);
  // The parent passes a fresh callback each render; keeping it in a ref means the fetch below runs
  // when the dialog opens (or the album changes), not on every parent render.
  const statsLoaded = useRef(onStatsLoaded);
  statsLoaded.current = onStatsLoaded;

  useEffect(() => {
    if (visible) {
      setProfile(initialProfile || 'motorsport');
      setReadNumbers(initialSettings.read_numbers !== false);
      setReadPlates(initialSettings.read_plates !== false);
      setGroupVehicles(initialSettings.group_vehicles !== false);
      setGrouping(groupingOf(initialSettings));
      setSaving(false);
    }
  }, [visible, initialProfile, initialSettings]);

  // Shot statistics are read only while the dialog is open; a closed or reopened dialog drops the
  // stale response instead of painting it over the next album's numbers.
  useEffect(() => {
    if (!visible) return;
    let live = true;
    setStats(null);
    setStatsError(null);
    setStatsLoading(true);
    call<ShootStats>('shoot_stats', { jobId })
      .then((next) => {
        if (!live) return;
        setStats(next);
        // Only a first read (which backfills lens/ISO/size) changes the frames
        // payload; a cached read must not make Review reload 15 MB for nothing.
        if (next.backfilled > 0) statsLoaded.current?.();
      })
      .catch((e) => {
        if (live) setStatsError(String(e));
      })
      .finally(() => {
        if (live) setStatsLoading(false);
      });
    return () => {
      live = false;
    };
  }, [visible, jobId]);

  if (!visible) return null;

  const sections = stats ? statSections(stats) : [];

  const handleSave = async () => {
    setSaving(true);
    try {
      await onSave({
        scan_profile: profile,
        read_numbers: readNumbers,
        read_plates: readPlates,
        group_vehicles: groupVehicles,
        ...grouping,
      });
      onClose();
    } finally {
      setSaving(false);
    }
  };

  return (
    <Portal>
    <div className="import-overlay" {...backdrop}>
      <div className="import-dialog shoot-settings-modal" style={{ maxWidth: 540 }}>
        <button className="ghost icon import-close" aria-label="Close" onClick={onClose}>
          <Icon name="x" />
        </button>
        <h2>Shoot Settings</h2>
        <p className="lede">Customize processing for “{jobLabel}”</p>

        <div className="shoot-settings-body">
          <div className="field-group shot-stats">
            <span className="field-label">Shoot statistics</span>
            {statsLoading && <p className="muted shot-stats-note">Reading shot data…</p>}
            {statsError != null && (
              <p className="error-text shot-stats-error" title={statsError}>
                Shot statistics could not be read.
              </p>
            )}
            {sections.length > 0 ? (
              <div className="shot-stats-grid">
                {sections.map((section) => (
                  <section key={section.key} className={`shot-stats-group ${section.key}`}>
                    <h4>{section.title}</h4>
                    <dl className="meta">
                      {section.rows.map((row) => (
                        <Fragment key={row.label}>
                          <dt>{row.label}</dt>
                          <dd title={row.value}>{row.value}</dd>
                        </Fragment>
                      ))}
                    </dl>
                    {(section.lines ?? []).map((line) => (
                      <p key={line.label} className="shot-stats-line">
                        <span>{line.label}</span> {line.value}
                      </p>
                    ))}
                  </section>
                ))}
              </div>
            ) : !statsLoading && statsError == null ? (
              <p className="muted shot-stats-note">No shot data in this shoot yet.</p>
            ) : null}
            {stats != null && stats.backfilled > 0 && (
              <p className="muted shot-stats-note">Read shot data for {stats.backfilled.toLocaleString()} frames.</p>
            )}
          </div>

          <div className="field-group">
            <label className="field-label" htmlFor="shoot-type">Type of shoot</label>
            <ShootTypeSelect id="shoot-type" value={profile} onChange={setProfile} />
          </div>

          <div className="field-group toggles-group">
            <label className="field-label">Vehicle Detection & Identification</label>
            <label className="shoot-toggle-row">
              <input
                type="checkbox"
                checked={readNumbers}
                onChange={(e) => setReadNumbers(e.target.checked)}
              />
              <div className="toggle-text">
                <b>Read race & competition numbers</b>
                <span>
                  OCR vehicle competition numbers (#0–999). Turn off if vehicles have no numbers to prevent misread or hallucinated numbers.
                </span>
              </div>
            </label>

            <label className="shoot-toggle-row">
              <input
                type="checkbox"
                checked={readPlates}
                onChange={(e) => setReadPlates(e.target.checked)}
              />
              <div className="toggle-text">
                <b>Read licence plates</b>
                <span>OCR vehicle registration and licence plates.</span>
              </div>
            </label>

            <label className="shoot-toggle-row">
              <input
                type="checkbox"
                checked={groupVehicles}
                onChange={(e) => setGroupVehicles(e.target.checked)}
              />
              <div className="toggle-text">
                <b>Group vehicles by visual likeness</b>
                <span>Link identical vehicles across passes and burst stacks.</span>
              </div>
            </label>
          </div>

          <div className="field-group">
            <GroupingFields value={grouping} onChange={(patch) => setGrouping((current) => ({ ...current, ...patch }))} disabled={saving} idPrefix="album-grouping" />
          </div>
        </div>

        <div className="modal-actions" style={{ display: 'flex', justifyContent: 'flex-end', gap: 10, marginTop: 20 }}>
          <button type="button" className="ghost" onClick={onClose} disabled={saving}>
            Cancel
          </button>
          <button type="button" className="primary" onClick={handleSave} disabled={saving}>
            {saving ? 'Saving…' : 'Save Shoot Settings'}
          </button>
        </div>
      </div>
    </div>
    </Portal>
  );
}
