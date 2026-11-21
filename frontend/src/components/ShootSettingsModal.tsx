import { useState, useEffect } from 'react';
import { Icon, Portal, useBackdropClose } from './basics';
import { type Scope, type Settings } from '../lib/types';
import { ShootTypeSelect } from './ShootTypeSelect';
import { PresetPolicy } from './PresetPolicy';
import { overridesOf, type Overrides } from '../lib/presetPolicy.mjs';
import { thresholdOf } from '../lib/moments.mjs';
import { GroupingFields, groupingOf, type GroupingValues } from './GroupingFields';
import { IDENTIFY_SCOPE_OPTIONS, identifyScopeOf } from '../lib/identify.mjs';

type Props = {
  jobLabel: string;
  initialProfile: string;
  initialSettings: Settings;
  visible: boolean;
  onClose: () => void;
  onSave: (patch: {
    scan_profile: string;
    /** Detector and reader switches; a missing key follows the shoot type. Saved through `set_feature`, not the settings patch. */
    overrides: Overrides;
    read_action: boolean;
    group_vehicles: boolean;
    /** Which subjects identification reads; does not need a regroup. */
    identify_scope: Scope;
    /** How likely a moment must be before it counts (0.5 to 1). */
    moments_threshold: number;
  } & GroupingValues) => Promise<void>;
};

export function ShootSettingsModal({
  jobLabel,
  initialProfile,
  initialSettings,
  visible,
  onClose,
  onSave,
}: Props) {
  const [profile, setProfile] = useState(initialProfile || 'motorsport');
  const [overrides, setOverrides] = useState<Overrides>(() => overridesOf(initialSettings.overrides));
  const [readAction, setReadAction] = useState(initialSettings.read_action !== false);
  const [groupVehicles, setGroupVehicles] = useState(initialSettings.group_vehicles !== false);
  const [identifyScope, setIdentifyScope] = useState<Scope>(() => identifyScopeOf(initialSettings.identify_scope));
  const [momentsThreshold, setMomentsThreshold] = useState(() => String(thresholdOf(initialSettings.moments_threshold)));
  const [grouping, setGrouping] = useState<GroupingValues>(() => groupingOf(initialSettings));
  const [saving, setSaving] = useState(false);
  const backdrop = useBackdropClose(onClose);

  useEffect(() => {
    if (visible) {
      setProfile(initialProfile || 'motorsport');
      setOverrides(overridesOf(initialSettings.overrides));
      setReadAction(initialSettings.read_action !== false);
      setGroupVehicles(initialSettings.group_vehicles !== false);
      setIdentifyScope(identifyScopeOf(initialSettings.identify_scope));
      setMomentsThreshold(String(thresholdOf(initialSettings.moments_threshold)));
      setGrouping(groupingOf(initialSettings));
      setSaving(false);
    }
  }, [visible, initialProfile, initialSettings]);

  if (!visible) return null;

  const handleSave = async () => {
    setSaving(true);
    try {
      await onSave({
        scan_profile: profile,
        overrides,
        read_action: readAction,
        group_vehicles: groupVehicles,
        identify_scope: identifyScope,
        moments_threshold: thresholdOf(momentsThreshold),
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
          <div className="field-group">
            <label className="field-label" htmlFor="shoot-type">Type of shoot</label>
            <ShootTypeSelect id="shoot-type" value={profile} onChange={(next) => { setProfile(next); if (next !== profile) setOverrides({}); }} />
            <PresetPolicy preset={profile} overrides={overrides} onChange={setOverrides} disabled={saving}
              user={profile === initialProfile ? overridesOf(initialSettings.user_defaults) : undefined} />
          </div>

          <div className="field-group">
            <label className="field-label" htmlFor="shoot-identify-scope">Identify</label>
            <select
              id="shoot-identify-scope"
              value={identifyScope}
              disabled={saving}
              onChange={(e) => setIdentifyScope(identifyScopeOf(e.target.value))}
            >
              {IDENTIFY_SCOPE_OPTIONS.map((option) => <option key={option.id} value={option.id}>{option.label}</option>)}
            </select>
            <small className="muted">
              {(IDENTIFY_SCOPE_OPTIONS.find((option) => option.id === identifyScope) ?? IDENTIFY_SCOPE_OPTIONS[0]).hint}
            </small>
          </div>

          <div className="field-group">
            <label className="field-label" htmlFor="shoot-moments-threshold">Moment confidence</label>
            <input id="shoot-moments-threshold" type="number" min={0.5} max={1} step={0.05} value={momentsThreshold} disabled={saving} onChange={(e) => setMomentsThreshold(e.target.value)} />
            <small className="muted">A moment must be this likely before it is shown as sure or can lift a frame. Below it, it shows as maybe and never changes ranking.</small>
          </div>

          <div className="field-group toggles-group">
            <label className="field-label">Action and grouping</label>
            <label className="shoot-toggle-row">
              <input
                type="checkbox"
                checked={readAction}
                onChange={(e) => setReadAction(e.target.checked)}
              />
              <div className="toggle-text">
                <b>Read action and atmosphere</b>
                <span>Dust, tyre smoke, spray, airborne, flames, sparks, contact and wheel lift, only when clearly visible.</span>
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
