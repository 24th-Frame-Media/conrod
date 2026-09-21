import { useState } from 'react';
import { useRun } from '../state/hooks';
import { FilePick, Icon } from '../components/basics';
import { call, chooseFolder } from '../lib/api';
import { etaText, pct } from '../lib/format';
import { filename } from '../lib/review';
import { MIXED_PROFILE, PROFILE_GROUPS, profileParent, type ModelInfo, type ScanArgs, type Settings, type Status } from '../lib/types';
import type { StatusActions } from '../components/TitleBar';

export type ScanDraft = { root: string; label: string };
type Props = {
  draft: ScanDraft; setDraft: (d: ScanDraft) => void; profile: string; setProfile: (p: string) => void;
  models: ModelInfo[]; settings: Settings; status: Status; busy: boolean; actions: StatusActions;
  onStart: (args: ScanArgs) => void; onReviewActive: () => void;
};

const NOTES: [string, string, string][] = [
  ['01', 'Review while we work', 'Results appear progressively. No waiting for the entire folder.'],
  ['02', 'Your call, always', 'Set stars and rejects by hand. Your decisions stay with the album.'],
  ['03', 'Pick up where you left off', 'Pause or stop a scan and resume unfinished photos later.'],
];

/** Pick a folder and a scan type; while a scan runs the rail shows it live with pause / stop. */
export function Scan({ draft, setDraft, profile, setProfile, models, settings, status, busy, actions, onStart, onReviewActive }: Props) {
  const run = useRun();
  const [recursive, setRecursive] = useState(true);
  const [stage, setStage] = useState<'index' | 'cull' | 'all'>('cull');
  const [readPlates, setReadPlates] = useState(() => settings.read_plates !== false);
  const [readNumbers, setReadNumbers] = useState(() => settings.read_numbers !== false);
  const [entries, setEntries] = useState('');
  const selectedParent = profileParent(profile);
  const selectedGroup = PROFILE_GROUPS.find((group) => group.id === selectedParent);
  const vehicleIdentity = selectedParent === 'motorsport' || selectedParent === 'mix';
  const contextualTargets = selectedParent === 'portrait'
    ? [
        ['People', profile === 'portrait-group' ? 'Find every person and judge group-wide focus.' : profile === 'portrait-couple' ? 'Prioritise both people, expressions and interaction.' : 'Prioritise people, expressions and subject focus.'],
        ['Faces and eyes', profile === 'portrait-pets' ? 'Find human and animal faces; judge visible eyes.' : 'Find faces and judge eye focus where visible.'],
        ...(profile === 'portrait-pets' ? [['Pets', 'Find cats and dogs; prioritise their face, eyes and pose.']] : []),
      ]
    : selectedParent === 'event'
      ? [
          ['People', profile === 'event-shows' ? 'Find performers and audience subjects.' : profile === 'event-parties' ? 'Prioritise people, expressions and interaction.' : 'Find the main people and subjects throughout the event.'],
          ['Faces', 'Keep visible faces available for naming during review.'],
          ...(profile === 'event-parties' ? [] : [['Vehicles', 'Include prominent vehicles without making them the default subject.']]),
        ]
      : selectedParent === 'mix'
        ? [['People and faces', 'Inspect people, faces and eyes alongside vehicles.'], ['Vehicles', 'Inspect visible cars and motorcycles without assuming a dominant subject.']]
        : [];
  const active = status.tasks.find((t) => t.state === 'running' || t.state === 'paused');
  const scanning = status.activeJob !== null;
  const browse = async () => {
    const path = await chooseFolder();
    if (path) setDraft({ root: path, label: draft.label || filename(path) });
  };
  return (
    <div className="screen">
      <div className="scanner">
        <div className="scan-main">
          <h2>Scan a shoot</h2>
          <p className="lede">Fast, subject-aware culling. Focus is measured on the subject, so a blurred background never costs you a great pan.</p>
          <div className="new-scan open">
            <div className="field-row">
              <input aria-label="Photo folder" value={draft.root} spellCheck={false} placeholder="D:\Photos\2026\09\01"
                onChange={(e) => setDraft({ ...draft, root: e.target.value })} />
              <button className="ghost" onClick={browse}>Browse…</button>
            </div>
            <div className="field-row">
              <input aria-label="Album name" value={draft.label} placeholder="Name for this album (optional)"
                onChange={(e) => setDraft({ ...draft, label: e.target.value })} />
            </div>
            <div className="field-row"><label><input type="checkbox" checked={recursive} onChange={e => setRecursive(e.target.checked)} /> Include subfolders</label>
              <select aria-label="Processing stage" value={stage} onChange={e => setStage(e.target.value as typeof stage)}><option value="index">Index only</option><option value="cull">Cull and rate</option><option value="all">Cull and identify</option></select></div>
            <div className="field-row"><FilePick label="Entry list CSV…" accept=".csv,text/csv" onFile={file => void run(async () => { const result = await call<{count: number}>('import_entries', {csv: await file.text()}); setEntries(`${result.count} entries loaded`); })} /><span>{entries}</span></div>
            <h3 className="step-label">What did you shoot?</h3>
            <div className="profiles" role="radiogroup" aria-label="Scan type">
              {PROFILE_GROUPS.map((group) => {
                const selected = selectedParent === group.id;
                return (
                  <button key={group.id} className={`profile${selected ? ' selected' : ''}`} role="radio" aria-checked={selected} onClick={() => setProfile(group.id)}>
                    <span className="radio" /><span className="profile-text"><b>{group.title}</b><small>{group.hint}</small></span>
                  </button>
                );
              })}
              <button role="radio" aria-checked={profile === MIXED_PROFILE.id} className={`profile${profile === MIXED_PROFILE.id ? ' selected' : ''}`} onClick={() => setProfile(MIXED_PROFILE.id)}>
                <span className="radio" /><span className="profile-text"><b>{MIXED_PROFILE.title}</b><small>{MIXED_PROFILE.hint}</small></span>
              </button>
              {selectedGroup && (
                  <label className="preset-select"><span>{selectedGroup.title} preset</span>
                    <select aria-label={`${selectedGroup.title} preset`} value={profile} onChange={(event) => setProfile(event.target.value)}>
                      <option value={selectedGroup.id}>Baseline</option>
                      {selectedGroup.children.map((child) => <option key={child.id} value={child.id}>{child.title}</option>)}
                    </select>
                  </label>
              )}
            </div>
            <h3 className="step-label">What should it look for?</h3>
            <div className="scan-targets">
              {vehicleIdentity && <>
                <label><input type="checkbox" checked={readPlates} onChange={(e) => setReadPlates(e.target.checked)} /><span><b>Registration plates</b><small>Use the plate detector and OCR; tune the vision prompt for registered vehicles.</small></span></label>
                <label><input type="checkbox" checked={readNumbers} onChange={(e) => setReadNumbers(e.target.checked)} /><span><b>Race numbers</b><small>Read door, roundel and fairing numbers; tune the vision prompt for competition vehicles.</small></span></label>
              </>}
              {contextualTargets.map(([title, description]) => <label className="fixed-target" key={title}><input type="checkbox" checked readOnly tabIndex={-1} /><span><b>{title}</b><small>{description}</small></span></label>)}
            </div>
            <div className="actions-row">
              <button className="primary" disabled={!draft.root || busy || scanning} onClick={() => onStart({ root: draft.root, label: draft.label, profile, recursive, stage, readPlates: vehicleIdentity && readPlates, readNumbers: vehicleIdentity && readNumbers })}>
                {busy ? 'Preparing scan…' : 'Start scan'}
              </button>
              {scanning && <span className="muted busy-note">A scan is already running. Stop it from the activity menu, top right, to start another.</span>}
            </div>
          </div>
          <div className="dropzone">
            <Icon name="drop" size={22} />
            <div><b>Drop a folder anywhere on this window</b><span>It fills in the path above. Nothing starts until you press Start scan.</span></div>
          </div>
        </div>
        <aside className="rail">
          {scanning && (
            <div className="rail-card">
              <h4>{active?.state === 'paused' ? 'Paused' : 'Scanning'}</h4>
              <div className="bar"><div className="fill" style={{ width: `${pct(active?.done ?? 0, active?.total ?? 0)}%` }} /></div>
              <div className="big-stat mono">{(active?.done ?? 0).toLocaleString()} / {(active?.total ?? 0).toLocaleString()}</div>
              <div className="big-label">{etaText(active?.eta) || 'estimating…'}</div>
              {active?.detail && <p className="muted mono scan-note">{active.detail}</p>}
              <div className="rail-actions">
                <button className="ghost" onClick={active?.state === 'paused' ? actions.resume : actions.pause}>{active?.state === 'paused' ? 'Resume' : 'Pause'}</button>
                <button className="ghost danger" onClick={actions.stop}>Stop scan</button>
                <button className="primary" onClick={onReviewActive}>Review results</button>
              </div>
            </div>
          )}
          <div className="rail-card">
            <h4>Built for your workflow</h4>
            {NOTES.map(([n, title, body]) => <div className="note" key={n}><b>{n}</b><div><h5>{title}</h5><p>{body}</p></div></div>)}
          </div>
          <div className="rail-card">
            <h4>Model readiness</h4>
            <ul className="model-list">
              {models.map((m) => <li key={m.file}><span className={`dot ${m.ready ? 'ok' : 'no'}`} />{m.name}<small>{m.ready ? 'Available' : 'Not installed'}</small></li>)}
            </ul>
          </div>
        </aside>
      </div>
    </div>
  );
}
