import { useEffect, useState } from 'react';
import { useRun } from '../state/hooks';
import { FilePick, Icon, Portal, useBackdropClose } from '../components/basics';
import { ShootTypeSelect } from '../components/ShootTypeSelect';
import { StrictnessSlider } from '../components/StrictnessSlider';
import { GroupingFields, groupingOf, type GroupingValues } from '../components/GroupingFields';
import { call, chooseFolder } from '../lib/api';
import { filename } from '../lib/review';
import { WORKFLOW_OPTIONS, workflowModeOf } from '../lib/workflow.mjs';
import { profileParent, type ScanArgs, type Settings, type Status } from '../lib/types';

export type ScanDraft = { root: string; label: string };
/** What the sheet remembers for next time (and what the new album copies): shoot type, strictness, burst gap, stacking. */
export type ImportDefaults = {
  scan_profile: string; blur_strictness?: string; burst_gap: number;
  grouping_stacks: boolean; grouping_sensitivity: string; grouping_pause: number;
  grouping_keep_pans: boolean; grouping_focus_anchor: boolean;
  /** `both` | `cull` | `identify`; travels with the scan and becomes the album's own setting. */
  workflow_mode: string;
};
type Props = {
  draft: ScanDraft; setDraft: (d: ScanDraft) => void; profile: string; setProfile: (p: string) => void;
  settings: Settings; status: Status; busy: boolean;
  onStart: (args: ScanArgs, defaults: ImportDefaults) => void;
  visible: boolean; onClose: () => void;
};

const DEFAULT_GAP = 1.5;

/** Import sheet: folder, label, shoot type, strictness, and one Scan button. */
export function ImportDialog({ draft, setDraft, profile, setProfile, settings, status, busy, onStart, visible, onClose }: Props) {
  const run = useRun();
  const backdrop = useBackdropClose(onClose);
  const [recursive, setRecursive] = useState(true);
  const [strictness, setStrictness] = useState('moderate');
  const [strictnessTouched, setStrictnessTouched] = useState(false);
  const [burstGap, setBurstGap] = useState(String(DEFAULT_GAP));
  const [grouping, setGrouping] = useState<GroupingValues>(groupingOf(settings));
  const [workflow, setWorkflow] = useState(() => workflowModeOf(settings.workflow_mode));
  const [readPlates, setReadPlates] = useState(true);
  const [readNumbers, setReadNumbers] = useState(true);
  const [includePeople, setIncludePeople] = useState(false);
  const [entries, setEntries] = useState('');
  useEffect(() => {
    if (!visible) return;
    setStrictness(typeof settings.blur_strictness === 'string' ? settings.blur_strictness : 'moderate');
    setStrictnessTouched(false);
    setBurstGap(String(Number(settings.burst_gap) > 0 ? settings.burst_gap : DEFAULT_GAP));
    setGrouping(groupingOf(settings));
    setWorkflow(workflowModeOf(settings.workflow_mode));
    setReadPlates(settings.read_plates !== false);
    setReadNumbers(settings.read_numbers !== false);
    setEntries('');
  }, [visible]); // eslint-disable-line react-hooks/exhaustive-deps -- seed from the saved defaults each time the sheet opens

  const parent = profileParent(profile);
  const vehicles = parent === 'motorsport' || parent === 'mix';
  const scanning = status.activeJob !== null;
  const gap = Number(burstGap);
  const gapOk = Number.isFinite(gap) && gap > 0 && gap <= 60;

  const setRoot = (root: string) => {
    // The label follows the folder name until the user types their own.
    const following = !draft.label || draft.label === filename(draft.root);
    setDraft({ root, label: following ? (root ? filename(root) : '') : draft.label });
  };
  const browse = async () => { const path = await chooseFolder(); if (path) setRoot(path); };
  const start = () => {
    onStart(
      { root: draft.root.trim(), label: draft.label.trim() || filename(draft.root), profile, recursive, stage: 'all', readPlates: vehicles && readPlates, readNumbers: vehicles && readNumbers, includePeople: parent === 'motorsport' && includePeople },
      { scan_profile: profile, blur_strictness: strictnessTouched ? strictness : undefined, burst_gap: gapOk ? gap : DEFAULT_GAP, ...grouping, workflow_mode: workflow },
    );
    onClose();
  };
  if (!visible) return null;
  return (
    <Portal>
    <div className="import-overlay" {...backdrop}>
      <div className="import-dialog import-sheet" role="dialog" aria-label="Import photos">
        <button className="ghost icon import-close" aria-label="Close" onClick={onClose}><Icon name="x" /></button>
        <h2>Import photos</h2>
        <p className="lede">Choose a folder and what you shot. Conrod reads, sorts and picks keepers in one go.</p>

        <div className="import-field">
          <label htmlFor="import-folder">Folder</label>
          <div className="field-row">
            <input id="import-folder" value={draft.root} spellCheck={false} placeholder="D:\Photos\2026\09\01" onChange={(e) => setRoot(e.target.value)} />
            <button className="ghost" onClick={browse}>Browse…</button>
          </div>
          <small className="muted">Or drop a folder anywhere on the window.</small>
        </div>
        <div className="import-field">
          <label htmlFor="import-label">Album name</label>
          <input id="import-label" value={draft.label} placeholder="Taken from the folder name" onChange={(e) => setDraft({ ...draft, label: e.target.value })} />
        </div>
        <div className="import-field">
          <label htmlFor="import-shoot">Shoot type</label>
          <ShootTypeSelect id="import-shoot" value={profile} onChange={setProfile} />
        </div>
        <div className="import-field">
          <label htmlFor="import-workflow">Workflow</label>
          <select id="import-workflow" value={workflow} onChange={(e) => setWorkflow(workflowModeOf(e.target.value))}>
            {WORKFLOW_OPTIONS.map((option) => <option key={option.id} value={option.id}>{option.label}</option>)}
          </select>
          <small className="muted">{(WORKFLOW_OPTIONS.find((option) => option.id === workflow) ?? WORKFLOW_OPTIONS[0]).hint}</small>
        </div>
        <div className="import-field">
          <label>Strictness</label>
          <StrictnessSlider value={strictness} onChange={(v) => { setStrictness(v); setStrictnessTouched(true); }} />
        </div>

        <details className="import-advanced">
          <summary>Advanced</summary>
          <div className="import-advanced-body">
            <label className="import-check"><input type="checkbox" checked={recursive} onChange={(e) => setRecursive(e.target.checked)} /><span><b>Include subfolders</b></span></label>
            {vehicles && <>
              <label className="import-check"><input type="checkbox" checked={readPlates} onChange={(e) => setReadPlates(e.target.checked)} /><span><b>Read registration plates</b><small>Plate detector and OCR, then the vision model.</small></span></label>
              <label className="import-check"><input type="checkbox" checked={readNumbers} onChange={(e) => setReadNumbers(e.target.checked)} /><span><b>Read race numbers</b><small>Door, roundel and fairing numbers. Turn off when there are none.</small></span></label>
            </>}
            {parent === 'motorsport' && (
              <label className="import-check"><input type="checkbox" checked={includePeople} onChange={(e) => setIncludePeople(e.target.checked)} /><span><b>Also detect people and faces</b><small>The main subject still decides focus.</small></span></label>
            )}
            {!vehicles && <p className="muted import-note">This shoot type has no detection toggles. Subjects, sharpness and faces are always checked.</p>}
            <div className="import-gap">
              <label htmlFor="import-gap"><b>Burst gap</b><small>Frames closer together than this share a burst. Changing it later regroups instantly.</small></label>
              <span className="import-gap-input"><input id="import-gap" type="number" min={0.1} max={60} step={0.1} value={burstGap} aria-invalid={!gapOk} onChange={(e) => setBurstGap(e.target.value)} /><span className="muted">s</span></span>
            </div>
            <GroupingFields value={grouping} onChange={(patch) => setGrouping((current) => ({ ...current, ...patch }))} idPrefix="import" />
            {vehicles && (
              <div className="field-row import-entries"><FilePick label="Entry list CSV…" accept=".csv,text/csv" onFile={(file) => void run(async () => { const result = await call<{ count: number }>('import_entries', { csv: await file.text() }); setEntries(`${result.count} entries loaded`); })} /><span className="muted">{entries}</span></div>
            )}
          </div>
        </details>

        <div className="actions-row">
          <button className="primary" disabled={!draft.root.trim() || busy || scanning || !gapOk} onClick={start}>{busy ? 'Starting…' : 'Scan'}</button>
          {scanning && <span className="muted busy-note">A scan is already running. Stop it from the Scan screen to start another.</span>}
        </div>
      </div>
    </div>
    </Portal>
  );
}
