import { Maintenance } from '../components/Maintenance';
import { useEffect, useState } from 'react';
import { engine } from '../lib/api';
import { words } from '../lib/format';
import type { Settings as SettingsMap, SettingValue, Toaster } from '../lib/types';
import type { Runner } from '../state/hooks';

const GROUPS: [string, string[]][] = [
  ['Detection & focus', ['detect_conf', 'min_box_fraction', 'max_vehicles_per_frame', 'include_cars', 'include_bikes', 'include_trucks', 'sharp_at', 'blurred_below', 'auto_reject_below_stars', 'cull_blurred', 'burst_gap']],
  ['Identification', ['read_plates', 'read_numbers', 'read_text', 'use_vlm', 'vlm_provider', 'vlm_model', 'vlm_host', 'vlm_extra_hosts', 'vlm_api_key', 'vlm_timeout', 'use_known_vehicles', 'normalise_names']],
  ['Metadata & desktop', ['write_rating', 'write_label', 'overwrite_rating', 'overwrite_label', 'write_caption', 'overwrite_caption', 'write_plate_keyword', 'write_sidecar_for_raw', 'keyword_prefix', 'close_to_tray']],
];

/** [label, hint]; a key without an entry falls back to its own name. */
const META: Record<string, [string, string?]> = {
  detect_conf: ['Detection confidence', 'Lower finds more vehicles, and more false ones.'],
  min_box_fraction: ['Smallest subject', 'As a fraction of the frame.'],
  max_vehicles_per_frame: ['Vehicles per frame'],
  sharp_at: ['Counts as sharp at', 'Subject sharpness at or above this.'],
  blurred_below: ['Counts as blurred below'],
  auto_reject_below_stars: ['Auto-reject below (stars)', '0 never rejects on its own.'],
  cull_blurred: ['Cull blurred frames', 'A red rating, never a deletion.'],
  burst_gap: ['Burst gap (seconds)', 'Frames closer than this belong to one pass.'],
  use_vlm: ['Use the vision model', 'Names make, model, colour and team.'],
  vlm_provider: ['Vision provider'], vlm_model: ['Vision model'], vlm_host: ['Vision host'], vlm_extra_hosts: ['Extra hosts'],
  vlm_api_key: ['API key', 'Stored on this computer only.'], vlm_timeout: ['Timeout (seconds)'],
  use_known_vehicles: ['Use known vehicles', 'Fill blanks from cars you have met.'],
  normalise_names: ['Tidy makes and models'],
  write_sidecar_for_raw: ['Write sidecars next to RAW files'],
  keyword_prefix: ['Keyword prefix'],
  close_to_tray: ['Keep running in the tray when closed'],
};
const labelOf = (key: string) => META[key]?.[0] ?? words(key).replace(/^./, (c) => c.toUpperCase());

function Control({ name, value, onChange }: { name: string; value: SettingValue; onChange: (v: SettingValue) => void }) {
  if (typeof value === 'boolean') return <input type="checkbox" aria-label={labelOf(name)} checked={value} onChange={(e) => onChange(e.target.checked)} />;
  if (typeof value === 'number') return <input type="number" step="any" aria-label={labelOf(name)} value={value} onChange={(e) => onChange(Number(e.target.value))} />;
  return <input type={name.includes('api_key') ? 'password' : 'text'} aria-label={labelOf(name)} value={value} spellCheck={false} onChange={(e) => onChange(e.target.value)} />;
}

type Props = { settings: SettingsMap; onSaved: (s: SettingsMap) => void; run: Runner; toast: Toaster };

/** Settings shared with the Python library; Save writes the whole block back. */
export function Settings({ settings, onSaved, run, toast }: Props) {
  const [draft, setDraft] = useState<SettingsMap>(settings);
  useEffect(() => setDraft(settings), [settings]);
  const dirty = JSON.stringify(draft) !== JSON.stringify(settings);
  const save = () => void run(async () => { onSaved(await engine.saveSettings(draft)); toast('Settings saved.', { tone: 'ok' }); });
  const learn = () => void run(async () => {
    const out = await engine.trainTaste();
    toast(typeof out.n === 'number' ? `Learned from ${out.n} of your ratings.` : 'Learned from your ratings.', { tone: 'ok' });
  });
  return (
    <div className="screen">
      <div className="pane wide">
        <h2>Settings</h2>
        <p className="lede">Shared with your existing Conrod library. Defaults are what measured best on real frames.</p>
        {GROUPS.map(([title, keys]) => (
          <section className="setting-group" key={title}>
            <h3>{title}</h3>
            {keys.filter((k) => draft[k] !== undefined).map((k) => {
              const value = draft[k];
              return (
                <label className="setting" key={k}>
                  <span><span className="label">{labelOf(k)}</span>{META[k]?.[1] && <span className="hint">{META[k]?.[1]}</span>}</span>
                  <Control name={k} value={value} onChange={(v) => setDraft((d) => ({ ...d, [k]: v }))} />
                </label>
              );
            })}
          </section>
        ))}
        <Maintenance />
        <div className="actions-row">
          <button className="primary" disabled={!dirty} onClick={save}>Save</button>
          <button className="ghost" disabled={!dirty} onClick={() => setDraft(settings)}>Discard changes</button>
          <button className="ghost" title="Fit Conrod's star ratings to the ones you have given by hand." onClick={learn}>Learn from my ratings</button>
          {dirty && <span className="muted">Unsaved changes</span>}
        </div>
      </div>
    </div>
  );
}
