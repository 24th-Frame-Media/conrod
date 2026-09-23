import { useState, useEffect } from 'react';
import { Icon } from './basics';
import { PROFILE_GROUPS, MIXED_PROFILE, type Settings } from '../lib/types';

type Props = {
  jobId: number;
  jobLabel: string;
  initialProfile: string;
  initialSettings: Settings;
  visible: boolean;
  onClose: () => void;
  onSave: (patch: {
    scan_profile: string;
    read_numbers: boolean;
    read_plates: boolean;
    group_vehicles: boolean;
  }) => Promise<void>;
};

export function ShootSettingsModal({
  jobId,
  jobLabel,
  initialProfile,
  initialSettings,
  visible,
  onClose,
  onSave,
}: Props) {
  const [profile, setProfile] = useState(initialProfile || 'motorsport');
  const [readNumbers, setReadNumbers] = useState(initialSettings.read_numbers !== false);
  const [readPlates, setReadPlates] = useState(initialSettings.read_plates !== false);
  const [groupVehicles, setGroupVehicles] = useState(initialSettings.group_vehicles !== false);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (visible) {
      setProfile(initialProfile || 'motorsport');
      setReadNumbers(initialSettings.read_numbers !== false);
      setReadPlates(initialSettings.read_plates !== false);
      setGroupVehicles(initialSettings.group_vehicles !== false);
      setSaving(false);
    }
  }, [visible, initialProfile, initialSettings]);

  if (!visible) return null;

  const handleSave = async () => {
    setSaving(true);
    try {
      await onSave({
        scan_profile: profile,
        read_numbers: readNumbers,
        read_plates: readPlates,
        group_vehicles: groupVehicles,
      });
      onClose();
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      className="import-overlay"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="import-dialog shoot-settings-modal" style={{ maxWidth: 540 }}>
        <button className="ghost icon import-close" aria-label="Close" onClick={onClose}>
          <Icon name="x" />
        </button>
        <h2>Shoot Settings</h2>
        <p className="lede">Customize processing for “{jobLabel}”</p>

        <div className="shoot-settings-body">
          <div className="field-group">
            <label className="field-label">Type of shoot</label>
            <select
              className="shoot-preset-select"
              value={profile}
              onChange={(e) => setProfile(e.target.value)}
            >
              {PROFILE_GROUPS.map((group) => (
                <optgroup key={group.id} label={group.title}>
                  <option value={group.id}>{group.title} (General)</option>
                  {group.children.map((child) => (
                    <option key={child.id} value={child.id}>
                      {group.title} · {child.title}
                    </option>
                  ))}
                </optgroup>
              ))}
              <optgroup label="Other">
                <option value={MIXED_PROFILE.id}>{MIXED_PROFILE.title} (Unspecialised)</option>
              </optgroup>
            </select>
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
  );
}
