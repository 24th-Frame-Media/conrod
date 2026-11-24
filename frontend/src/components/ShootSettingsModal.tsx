import { useState, useEffect } from 'react';
import { Icon } from './basics';
import { Sheet } from './Sheet';
import { type Scope, type Settings } from '../lib/types';
import { ShootTypeSelect } from './ShootTypeSelect';
import { PresetPolicy } from './PresetPolicy';
import { defaultSummary, isOn, overridesOf, type Overrides, type PolicySummary } from '../lib/presetPolicy.mjs';
import { thresholdOf } from '../lib/moments.mjs';
import { subjectWords } from '../lib/subjects.mjs';
import { GroupingFields, groupingOf, type GroupingValues } from './GroupingFields';
import { IDENTIFY_SCOPE_OPTIONS, identifyScopeOf } from '../lib/identify.mjs';
import { albumVisionHint, nextAlbumVision } from '../lib/vision.mjs';

function seedOverrides(profile: string, rawOverrides: Overrides | undefined, settings: Settings): Overrides {
  const base = overridesOf(rawOverrides ?? settings.overrides);
  if (
    settings.identify_decision === false &&
    base.moments_detect === undefined &&
    defaultSummary(profile).moments.vocabulary.length > 0
  ) {
    return { ...base, moments_detect: false };
  }
  return base;
}

type Props = {
  jobLabel: string;
  initialProfile: string;
  initialSettings: Settings;
  /** The album's own detector, reader and moment switches. */
  initialOverrides?: Overrides;
  /** The album's snapshot of the photographer's defaults for this shoot type. */
  initialUserDefaults?: Overrides;
  /** The album's own vision choice; null while it follows the Settings switch. */
  initialVision: boolean | null;
  /** The global Settings, for the vision switch an album follows. */
  globals: Settings;
  visible: boolean;
  onClose: () => void;
  onSave: (patch: {
    scan_profile: string;
    /** Detector and reader switches; a missing key follows the shoot type. Saved through `set_feature`, not the settings patch. */
    overrides: Overrides;
    /** The album's own vision choice, or null to follow the Settings switch again. */
    use_vlm: boolean | null;
    read_action: boolean;
    group_vehicles: boolean;
    /** Which subjects identification reads; does not need a regroup. */
    identify_scope: Scope;
    /** How likely a moment must be before it counts (0.5 to 1). */
    moments_threshold: number;
    /** Per-album identify-step switches; `identify_decision` follows `moments_detect`. */
    identify_ocr: boolean;
    identify_decision: boolean;
  } & GroupingValues) => Promise<void>;
};

export function ShootSettingsModal({
  jobLabel,
  initialProfile,
  initialSettings,
  initialOverrides,
  initialUserDefaults,
  initialVision,
  globals,
  visible,
  onClose,
  onSave,
}: Props) {
  const [profile, setProfile] = useState(initialProfile || 'motorsport');
  const [overrides, setOverrides] = useState<Overrides>(() => seedOverrides(initialProfile || 'motorsport', initialOverrides, initialSettings));
  const [policy, setPolicy] = useState<PolicySummary | null>(null);
  const [vision, setVision] = useState<boolean | null>(initialVision);
  const [identifyOcr, setIdentifyOcr] = useState(initialSettings.identify_ocr !== false);
  const [readAction, setReadAction] = useState(initialSettings.read_action !== false);
  const [groupVehicles, setGroupVehicles] = useState(initialSettings.group_vehicles !== false);
  const [identifyScope, setIdentifyScope] = useState<Scope>(() => identifyScopeOf(initialSettings.identify_scope));
  const [momentsThreshold, setMomentsThreshold] = useState(() => String(thresholdOf(initialSettings.moments_threshold)));
  const [grouping, setGrouping] = useState<GroupingValues>(() => groupingOf(initialSettings));
  const [saving, setSaving] = useState(false);
  /** Why the last save did not finish; the photographer's edits stay in the form beneath it. */
  const [saveError, setSaveError] = useState<string | null>(null);

  useEffect(() => {
    if (!visible) return;
    const nextProfile = initialProfile || 'motorsport';
    setProfile(nextProfile);
    setOverrides(seedOverrides(nextProfile, initialOverrides, initialSettings));
    setVision(initialVision);
    setIdentifyOcr(initialSettings.identify_ocr !== false);
    setReadAction(initialSettings.read_action !== false);
    setGroupVehicles(initialSettings.group_vehicles !== false);
    setIdentifyScope(identifyScopeOf(initialSettings.identify_scope));
    setMomentsThreshold(String(thresholdOf(initialSettings.moments_threshold)));
    setGrouping(groupingOf(initialSettings));
    setSaving(false);
    setSaveError(null);
  }, [visible]); // eslint-disable-line react-hooks/exhaustive-deps -- seed from saved album settings when opened

  if (!visible) return null;
  // What Settings says; the album follows it until it is set here.
  const settingsVision = globals.use_vlm !== false;
  const userDefaults = overridesOf(initialUserDefaults ?? initialSettings.user_defaults);
  const hasMomentsPolicy = (policy?.moments.vocabulary.length ?? 0) > 0;
  // The shoot's own words: a rodeo or a portrait shoot is never told about motorsport effects.
  const subject = subjectWords(policy ?? defaultSummary(profile));

  const handleSave = async () => {
    setSaving(true);
    setSaveError(null);
    try {
      const identifyDecision = hasMomentsPolicy && policy
        ? isOn(policy, 'moments_detect')
        : (overrides.moments_detect ?? initialSettings.identify_decision) !== false;
      await onSave({
        scan_profile: profile,
        overrides,
        use_vlm: vision,
        read_action: readAction,
        group_vehicles: groupVehicles,
        identify_scope: identifyScope,
        moments_threshold: thresholdOf(momentsThreshold),
        identify_ocr: identifyOcr,
        identify_decision: identifyDecision,
        ...grouping,
      });
      onClose();
    } catch (e) {
      // Stay open with every edit intact, and say what happened and which parts did save.
      setSaveError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Sheet label="Shoot settings" className="shoot-settings-modal" style={{ maxWidth: 540 }} onClose={onClose}>
        <button className="ghost icon import-close" aria-label="Close" onClick={onClose}>
          <Icon name="x" />
        </button>
        <h2>Shoot Settings</h2>
        <p className="lede">Customize processing for “{jobLabel}”</p>

        <div className="shoot-settings-body">
          <div className="field-group">
            <label className="field-label" htmlFor="shoot-type">Type of shoot</label>
            <ShootTypeSelect id="shoot-type" value={profile} onChange={(next) => { setProfile(next); if (next !== profile) setOverrides({}); }} />
            <PresetPolicy
              preset={profile}
              overrides={overrides}
              onChange={setOverrides}
              onSummary={setPolicy}
              disabled={saving}
              user={profile === initialProfile ? userDefaults : undefined}
            />
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
            <label className="field-label">Identify steps</label>
            <p className="muted shoot-stage-lede">Choose which identification passes run for this album. A step that is off shows greyed out in the run view.</p>
            <label className="shoot-toggle-row">
              <input type="checkbox" checked={identifyOcr} disabled={saving} onChange={(e) => setIdentifyOcr(e.target.checked)} />
              <div className="toggle-text">
                <b>Read plates, numbers and text (OCR)</b>
                <span>A separate first pass. Off: the vision model reads numbers on its own.</span>
              </div>
            </label>
            <label className="shoot-toggle-row">
              <input
                type="checkbox"
                checked={vision ?? settingsVision}
                disabled={saving}
                onChange={(e) => setVision(nextAlbumVision(e.target.checked, settingsVision))}
              />
              <div className="toggle-text">
                <b>Read each subject with the vision model</b>
                <span>{albumVisionHint(vision, globals)}</span>
              </div>
            </label>
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
                <span>{subject.fields.action}</span>
              </div>
            </label>

            <label className="shoot-toggle-row">
              <input
                type="checkbox"
                checked={groupVehicles}
                onChange={(e) => setGroupVehicles(e.target.checked)}
              />
              <div className="toggle-text">
                <b>{subject.fields.group}</b>
                <span>{subject.fields.link}</span>
              </div>
            </label>
          </div>

          <div className="field-group">
            <GroupingFields value={grouping} onChange={(patch) => setGrouping((current) => ({ ...current, ...patch }))} disabled={saving} idPrefix="album-grouping" />
          </div>
        </div>

        {saveError && <p className="form-error" role="alert">{saveError}</p>}
        <div className="modal-actions" style={{ display: 'flex', justifyContent: 'flex-end', gap: 10, marginTop: 20 }}>
          <button type="button" className="ghost" onClick={onClose} disabled={saving}>
            Cancel
          </button>
          <button type="button" className="primary" onClick={handleSave} disabled={saving}>
            {saving ? 'Saving…' : 'Save Shoot Settings'}
          </button>
        </div>
      </Sheet>
  );
}
