import { useMemo, useEffect, useState, useRef, type MutableRefObject } from 'react';
import { Confirm, Empty, Icon, Modal } from '../components/basics';
import { Facets } from '../components/Facets';
import { Inspector } from '../components/Inspector';
import { PhotoGrid } from '../components/PhotoGrid';
import { StackGrid, type StackView } from '../components/StackGrid';
import { burstRankMap, decisionOf, defaultFilters, filename, vehicleGroups, type FrameFacts } from '../lib/review';
import { buildStacks, groupKeyOf, isSingleton, makeStack, nextUndecided, singletonIds, softBursts as softBurstMapOf, stepFlat, stepInStack as stepWithinStack, stepRow, stepStack, suggestionSizes, type Stack } from '../lib/stacks.mjs';
import { edgeFrameId, stepInStack } from '../lib/navigation.mjs';
import { reviewedStacks, stackDecision } from '../lib/decisions.mjs';
import { tasteStacks } from '../lib/taste.mjs';
import { albumSetting } from '../lib/album.mjs';
import { albumVisionChoice, albumVisionOn } from '../lib/vision.mjs';
import { WORKFLOW_OPTIONS, pipelineSteps, workflowIncludesCull, workflowIncludesIdentify, workflowLabel, workflowModeOf, type PipelineStep } from '../lib/workflow.mjs';
import { everythingCount, hasReadings, identifyActionLabel, identifyNewLabel, identifyScopeOf, keeperCount } from '../lib/identify.mjs';
import { RERUN_STAGES, anyRerunRunning, rerunDoneText, rerunHint, rerunLabel, rerunStartedText, runningRerun, type RerunStage } from '../lib/rerun.mjs';
import { call, engine } from '../lib/api';
import { applyLine } from '../lib/apply.mjs';
import { etaText, pct } from '../lib/format';
import { useLocalFlag, useRun } from '../state/hooks';
import { useToast } from '../state/toast';
import { MenuDropdown, type MenuItem } from '../components/MenuDropdown';
import { ShootSettingsModal } from '../components/ShootSettingsModal';
import { featureLabel, runSaveSteps, saveFailureMessage, type SaveStep } from '../lib/shootSave.mjs';
import { ShootStatsModal } from '../components/ShootStatsModal';
import { ExportSheet } from '../components/ExportSheet';
import { LightroomImport } from '../components/LightroomImport';
import { Identifications } from '../components/Identifications';
import type { Frame, Job, KnownVehicle, MarkValues, Scope, Settings, Status, Task } from '../lib/types';
import { profileLabel, profileParent } from '../lib/types';
import { vlmModelLabel } from '../lib/vlm';
import { facetsFor } from '../lib/facets.mjs';
import { usePresetPolicy } from '../state/usePresetPolicy';
import { isOn, overridesOf } from '../lib/presetPolicy.mjs';
import { identifyTitle as identifyBannerTitle, subjectWords } from '../lib/subjects.mjs';
import { thresholdOf } from '../lib/moments.mjs';
import type { ReviewModel } from '../state/useReview';
import { Viewer } from './Viewer';
import { Compare } from './Compare';

/** G/E/C hotkeys switch between the grid, the loupe (Viewer) and the compare view. */
export type ReviewView = 'grid' | 'loupe' | 'compare';
/** How App's global hotkey handler reaches into whichever view Review currently owns. */
export type ReviewVerbs = {
  view: ReviewView; setView: (v: ReviewView) => void;
  /** `repeat` is true on a held key's auto-repeat: within-stack movement continues, a jump into the next stack does not. */
  stepFrame: (delta: number, repeat?: boolean) => void; stepBurst: (delta: number) => void; nextBurst: (delta: number) => void;
  nextUndecided: () => void; enter: () => void; escape: () => void;
  mark: (values: MarkValues) => void; rejected: () => boolean; picked: () => boolean;
  /** B / Shift+B: keep this frame and cull its stack, or cull the whole stack. */
  stackAction: (mode: 'keep' | 'cull') => void;
  undo: () => void; redo: () => void;
};
export const noReviewVerbs: ReviewVerbs = {
  view: 'grid', setView: () => {}, stepFrame: () => {}, stepBurst: () => {}, nextBurst: () => {}, nextUndecided: () => {},
  enter: () => {}, escape: () => {}, mark: () => {}, rejected: () => false, picked: () => false,
  stackAction: () => {},
  undo: () => {}, redo: () => {},
};

function currentItem(task: Task): string {
  const detail = task.detail?.trim();
  if (!detail) return 'Preparing the next photograph…';
  return detail.split(/[\\/]/).at(-1) ?? detail;
}

function IdentificationBanner({ task, settings, title, onSkip }: { task: Task; settings: Settings; title: string; onSkip: () => void }) {
  const models = [
    settings.read_plates && 'plate reader',
    (settings.read_plates || settings.read_numbers) && 'OCR',
    settings.use_vlm && (vlmModelLabel(settings) ?? 'no service'),
    settings.group_vehicles && 'visual matcher',
  ].filter(Boolean) as string[];
  const percent = task.total > 0 ? Math.min(100, Math.round((task.done / task.total) * 100)) : 0;

  return (
    <section className="identify-strip" aria-live="polite">
      <div className="identify-strip-main">
        <div className="identify-strip-left">
          <span className="work-spinner" />
          <span className="identify-strip-title">{title}</span>
          <span className="identify-strip-pct">{percent}%</span>
          <span className="identify-strip-sep">·</span>
          <span className="identify-strip-file" title={task.detail}>{currentItem(task)}</span>
          <span className="identify-strip-count">({task.done.toLocaleString()} of {task.total.toLocaleString()})</span>
        </div>
        <div className="identify-strip-right">
          {models.length > 0 && <span className="identify-strip-models">Using {models.join(' · ')}</span>}
          <button
            type="button"
            className="ghost small identify-strip-skip"
            title="Stop reading identity details, keep current results, and continue to the album."
            onClick={onSkip}
          >
            Skip
          </button>
        </div>
      </div>
      <div className="identify-progress"><i style={{ width: `${percent}%` }} /></div>
    </section>
  );
}

/** The steps the album's workflow actually runs: the mode decides which exist, the data decides what is done. */
function PipelineStepper({ steps }: { steps: PipelineStep[] }) {
  return (
    <nav className="pipeline" aria-label="Processing pipeline">
      {steps.map((step, i) => (
        <span key={step.key} className={`pipeline-step${step.done ? ' done' : ''}${step.active ? ' active' : ''}`}>
          <span className="pipeline-dot">{step.done ? <Icon name="check" size={12} /> : i + 1}</span>
          <span className="pipeline-label">{step.label}</span>
        </span>
      ))}
    </nav>
  );
}

type Props = {
  rv: ReviewModel; jobs: Job[]; jobId: number | null; scanning: boolean; status: Status; settings: Settings;
  showBoxes: boolean; onToggleBoxes: () => void; verbs: MutableRefObject<ReviewVerbs>;
  onPickJob: (id: number) => void; onIdentify: (scope?: Scope, onlyNew?: boolean) => void; onCull: () => void;
  onHelp: () => void; onOpenViewer: () => void; onLibrary: () => void;
  onEdit: (detectionId: number, updates: Record<string, string | string[] | null>) => unknown;
  onAcceptAll: () => void; onSkipIdentify: () => void;
  /** Accept the album's readings as they stand: no model run, Review opens. */
  onSkipAlbumIdentify: () => void;
  onWrite: (dryRun: boolean) => void;
  /** Reopen the live identification flow from the stepper while a run is going. */
  onWatchIdentify: () => void;
  /** Reopen the scan while it is still running, from the "frames already measured" banner. */
  onWatchScan: () => void;
  /** Album settings changed (preset, toggles): reload the job list so the new values show. */
  onJobsChanged: () => Promise<unknown> | void;
  /** Dev start only (`?mock=1&lightroom=…`): open the sheet or dialog a screenshot needs. */
  lightroomDev?: 'export' | 'sent' | 'import' | 'imported' | 'stats';
  /** Nonce incremented by a disabled flowchart node's shortcut to open this
   *  album's shoot settings modal, even if it was opened previously. */
  openShootSettings?: number;
};

/** A burst/duplicate/vehicle group with the words the card shows. */
function describe(stack: Stack<Frame>, facts: Map<number, FrameFacts>, ranks: Map<number, number>, extra?: { title: string; subtitle: string; photoWord?: string }): StackView {
  if (extra) return { ...stack, ...extra, ranks };
  const x = facts.get(stack.lead.id);
  const title = x?.vehicle || x?.plate || (x?.number && `#${x.number}`) || filename(stack.lead.path);
  const n = stack.frames.length;
  const shots = n === stack.total ? `${n} shot${n === 1 ? '' : 's'}` : `${n} of ${stack.total} shots`;
  return { ...stack, title, subtitle: `${shots}${x?.plate ? ` · ${x.plate}` : ''}${x?.number ? ` · #${x.number}` : ''}${x?.team ? ` · ${x.team}` : ''}`, ranks };
}

const byFrame = (stacks: StackView[]) => {
  const map = new Map<number, StackView>();
  for (const s of stacks) for (const f of s.frames) if (!map.has(f.id)) map.set(f.id, s);
  return map;
};

/**
 * The one reset that cannot be undone: a typed RESET guards the red button, and
 * the dialog says exactly what goes and what stays. Kept apart from `Confirm`
 * because it has a field, a busy state and a "Cancel first" focus.
 */
function ResetAlbumDialog({ onCancel, onConfirm }: { onCancel: () => void; onConfirm: () => Promise<void> }) {
  const [typed, setTyped] = useState('');
  const [busy, setBusy] = useState(false);
  const ready = typed.trim().toLowerCase() === 'reset' && !busy;
  return (
    <Modal label="Reset this album?" className="small-dialog" onClose={() => { if (!busy) onCancel(); }}>
      <div className="dialog-head"><h3>Reset this album?</h3></div>
      <div className="dialog-body">
        <p>This puts <strong>this album</strong> back to the moment it was created. It deletes, here only:</p>
        <ul className="reset-ledger">
          <li>every measurement and cull verdict</li>
          <li>every star, pick and reject</li>
          <li>every identification (numbers, plates, names)</li>
          <li>the autofocus reads and the grouping</li>
        </ul>
        <p>It keeps the album and its settings, ready to scan again. <strong>Your photos and XMP sidecars are never touched.</strong></p>
        <label className="reset-confirm">
          <span>Type <strong>RESET</strong> to confirm</span>
          <input value={typed} onChange={(e) => setTyped(e.target.value)} placeholder="RESET"
            spellCheck={false} autoComplete="off" disabled={busy} aria-label="Type RESET to confirm" />
        </label>
      </div>
      <div className="actions-row end">
        <button className="ghost" onClick={onCancel} disabled={busy} autoFocus>Cancel</button>
        <button className="danger solid" disabled={!ready}
          onClick={() => { setBusy(true); void onConfirm().finally(() => setBusy(false)); }}>
          {busy ? 'Resetting…' : 'Reset album'}
        </button>
      </div>
    </Modal>
  );
}

/** Toolbar (album, view, search, stars, sort, actions), facets, the card grid and the inspector. */
export function ReviewScreen({ rv, jobs, jobId, scanning, status, settings, showBoxes, onToggleBoxes, verbs, onPickJob, onIdentify, onCull, onHelp, onOpenViewer, onLibrary, onEdit, onAcceptAll, onSkipIdentify, onSkipAlbumIdentify, onWrite, onJobsChanged, onWatchIdentify, onWatchScan, lightroomDev, openShootSettings = 0 }: Props) {
  const [known, setKnown] = useState<KnownVehicle[]>([]);
  const [confirmReset, setConfirmReset] = useState<'reset_ratings' | 'reset_identifications' | 'reset_detections' | 'reset_album' | 'rescan' | null>(null);
  const [shootSettingsOpen, setShootSettingsOpen] = useState(false);
  // Open shoot settings when triggered from a disabled flowchart node shortcut.
  useEffect(() => { if (openShootSettings) setShootSettingsOpen(true); }, [openShootSettings]);
  const [statsOpen, setStatsOpen] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [lightroomOpen, setLightroomOpen] = useState(false);
  const [autoAdvance, setAutoAdvance] = useLocalFlag('conrod.autoAdvance', true);
  const [crossStacks, setCrossStacks] = useLocalFlag('conrod.crossStacks', true);
  const [cols, setCols] = useState(4);
  const run = useRun();
  const toast = useToast();
  useEffect(() => { void engine.known().then(setKnown); }, [jobId]);
  // Dev start only: the export sheet, the import dialog and the stats modal each need to be open in
  // a headless screenshot, and there is no URL the browser can click its way to on its own.
  useEffect(() => {
    if (!lightroomDev) return;
    if (lightroomDev === 'stats') { setStatsOpen(true); return; }
    if (lightroomDev === 'import' || lightroomDev === 'imported') { setLightroomOpen(true); return; }
    setExportOpen(true);
  }, [lightroomDev]);
  // A stage rerun runs in the background. When its operation key leaves the status
  // list, the finished task carries the counts: toast them and reload the album.
  const lastRerun = useRef<string | null>(null);
  useEffect(() => {
    const key = jobId == null ? undefined : status.operations.find((k) => k.startsWith('Rerunning ') && k.endsWith(`:${jobId}`));
    if (key) { lastRerun.current = key; return; }
    const finished = lastRerun.current;
    if (!finished || jobId == null) return;
    lastRerun.current = null;
    const stage = finished.slice('Rerunning '.length, finished.lastIndexOf(':'));
    const task = status.tasks.find((t) => t.label.startsWith(`Rerunning ${stage}`) && t.label.includes(`album ${jobId}`));
    if (task?.state === 'done') toast(rerunDoneText(stage, task.detail), { tone: 'ok' });
    else if (task?.state === 'failed') toast(task.error || `${rerunLabel(stage)} failed.`, { tone: 'error' });
    void rv.refresh();
    void onJobsChanged();
  }, [status.operations, status.tasks, jobId, rv, toast, onJobsChanged]);
  const { filters, patchFilters, frames: allFrames, review, facts, keepers, selected, frame, grouping, expanded } = rv;
  // While the scan runs, Review shows only the frames whose measurement has finished; the rest
  // arrive through the usual 4 s refresh. Bursts that are ready keep their normal actions.
  const frames = useMemo(() => (scanning ? allFrames.filter((f) => f.status === 'done') : allFrames), [allFrames, scanning]);
  const currentJob = jobs.find((j) => j.id === jobId);
  const currentProfile = String(currentJob?.scan_profile || settings.scan_profile || 'motorsport');
  // Vehicle-only controls (by-vehicle grouping, "needs a look" for unread numbers and plates) go away for shoots with no vehicles to read.
  const albumOverrides = overridesOf(albumSetting(currentJob, 'overrides', settings));
  const userDefaults = overridesOf(albumSetting(currentJob, 'user_defaults', settings));
  const policy = usePresetPolicy(currentProfile, albumOverrides, userDefaults);
  // Derive subject wording for this album from its resolved shoot policy rather
  // than hardcoding a single shoot type.
  const subject = subjectWords(policy);
  const vehicleBrowse = facetsFor(currentProfile, { ...userDefaults, ...albumOverrides }).some((k) => k === 'number' || k === 'plate' || k === 'make');
  const groupingNow = grouping === 'vehicles' && !vehicleBrowse ? 'bursts' : grouping;
  const identifyTask = jobId == null ? undefined : status.tasks.find((task) => (task.state === 'running' || task.state === 'paused') && /^Identifying\b/.test(task.label) && task.label.includes(`album ${jobId}`));
  const identifying = Boolean(identifyTask || (jobId != null && status.operations.some((key) => key === `Identifying:${jobId}`)));
  const scanTask = scanning ? status.tasks.find((task) => (task.state === 'running' || task.state === 'paused') && /^Scanning\b/.test(task.label)) : undefined;

  // The one filtered model: every stack below is built from `frames`, the list the toolbar already narrowed.
  const ranks = useMemo(() => burstRankMap(review.frames, keepers), [review.frames, keepers]);
  // Whole-album, like `keepers`: a filter must not change which frame a burst suggests.
  const suggestions = useMemo(() => suggestionSizes(review.frames, facts), [review.frames, facts]);
  // Frames alone in their burst: the flat grid draws them as plain frames, with no keeper mark or burst rank.
  const singletons = useMemo(() => singletonIds(review.frames), [review.frames]);
  // Bursts the engine refused to keep anything from: the grid says "No keeper".
  const softBursts = useMemo(() => softBurstMapOf(review.frames, facts), [review.frames, facts]);
  // Whole-album burst stacks for the progress readout, so narrowing the view does not change the denominator.
  const albumBursts = useMemo(() => buildStacks(review.frames.filter((f) => f.status === 'done'), { facts, keepers, all: review.frames }), [review.frames, facts, keepers]);
  const progress = useMemo(() => reviewedStacks(albumBursts, (f) => decisionOf(f, facts.get(f.id)) === 'undecided'), [albumBursts, facts]);
  const acceptSuggestion = (id: number) => { void rv.mark({ picked: true }, id); };
  const burstStacks = useMemo(() => {
    const stacks = buildStacks(frames.filter((f) => f.status === 'done'), { facts, keepers, all: review.frames }).map((s) => describe(s, facts, ranks));
    // "Taste": highest inside each burst first, bursts by the taste of their lead.
    return filters.sort === 'taste' ? tasteStacks(stacks) : stacks;
  }, [frames, facts, keepers, review.frames, ranks, filters.sort]);
  const shown = useMemo<StackView[]>(() => {
    if (groupingNow === 'bursts') return burstStacks;
    if (groupingNow === 'dupes') return buildStacks(frames.filter((f) => f.status === 'done'), { facts, keepers, all: review.frames, dupes: true }).map((s) => describe(s, facts, ranks));
    if (groupingNow === 'vehicles') {
      const order = new Map(frames.map((f, i) => [f.id, i]));
      return vehicleGroups(review, new Set(order.keys()), identifying)
        .map((g) => describe(makeStack(`v:${g.key}`, g.frames, { facts, keepers, total: g.total }), facts, ranks, { title: g.title, subtitle: g.subtitle, photoWord: 'photos' }))
        .sort((a, b) => (order.get(a.lead.id) ?? 0) - (order.get(b.lead.id) ?? 0));
    }
    return [];
  }, [groupingNow, burstStacks, frames, facts, keepers, review, ranks, identifying]);
  const stacked = groupingNow !== 'flat';
  // Loupe and compare walk bursts even when the grid is flat; duplicate and vehicle grouping walk their own stacks.
  const navStacks = groupingNow === 'dupes' || groupingNow === 'vehicles' ? shown : burstStacks;
  const stackOf = useMemo(() => byFrame(navStacks), [navStacks]);
  const shownOf = useMemo(() => byFrame(shown), [shown]);
  // A one-frame stack is drawn as a plain frame: it has no strip to open, whatever `expanded` still says.
  const expandedStack = expanded != null ? shownOf.get(expanded) ?? null : null;
  const expandedKey = expandedStack != null && !isSingleton(expandedStack) ? expandedStack.key : null;
  const selectedKey = selected != null ? shownOf.get(selected)?.key ?? null : null;

  // Active review layout (grid, loupe viewer, or compare); switched via G/E/C hotkeys through `verbs`.
  const [view, setView] = useState<ReviewView>('grid');
  const [idsOpen, setIdsOpen] = useState(false);
  const [comparePane, setComparePane] = useState<'left' | 'right'>('left');
  const [compareRightId, setCompareRightId] = useState<number | null>(null);
  const compareRight = useMemo(() => {
    if (!frame) return null;
    if (compareRightId != null) return frames.find((f) => f.id === compareRightId) ?? null;
    // Default right pane: the burst's keeper if this frame isn't it, else the next frame of the burst.
    const stack = stackOf.get(frame.id);
    if (!stack) return null;
    const keeper = stack.keepers.find((f) => f.id !== frame.id);
    if (keeper) return keeper;
    const index = stack.frames.findIndex((f) => f.id === frame.id);
    return stack.frames[index + 1] ?? stack.frames[index - 1] ?? null;
  }, [frame, compareRightId, frames, stackOf]);
  useEffect(() => { setCompareRightId(null); setComparePane('left'); }, [frame?.id]);
  const activeCompareFrame = comparePane === 'left' ? frame : compareRight;

  const openViewer = () => { setView('loupe'); onOpenViewer(); };
  /** Show a stack's strip, except on a one-frame stack: that is just a frame, and gets no strip. */
  const expandOnto = (id: number | null) => {
    const target = id == null ? null : shownOf.get(id) ?? stackOf.get(id) ?? null;
    rv.setExpanded(target != null && !isSingleton(target) ? id : null);
  };
  /** Move the selection; an open strip follows into a real stack, never onto a one-frame stack. */
  const goTo = (id: number | null, keepStrip = expanded != null) => {
    if (id == null) return;
    rv.setSelected(id);
    if (keepStrip) expandOnto(id);
  };
  const undecided = (f: Frame) => decisionOf(f, facts.get(f.id)) === 'undecided';
  const stackUndecided = (s: Stack<Frame>) => !s.frames.some((f) => f.picked) && s.frames.some(undecided);
  const jumpStack = (delta: number) => {
    const base = view === 'compare' ? compareRight ?? frame : frame;
    const next = stepStack(navStacks, base?.id ?? null, delta);
    if (view === 'compare') { if (next != null) setCompareRightId(next); } else goTo(next);
  };
  const jumpUndecided = () => {
    const base = view === 'compare' ? compareRight ?? frame : frame;
    if (!stacked && view === 'grid') { goTo(nextUndecided(frames, base?.id ?? null, undecided)); return; }
    const leads = navStacks.map((s) => s.lead);
    const current = base ? stackOf.get(base.id)?.lead.id ?? null : null;
    const byLead = new Map(navStacks.map((s) => [s.lead.id, s]));
    const next = nextUndecided(leads, current, (f) => stackUndecided(byLead.get(f.id)!));
    if (view === 'compare') { if (next != null) setCompareRightId(next); } else goTo(next);
  };
  /** After P or X: X moves to the next undecided frame of the burst (then the next burst), P to the next burst. */
  const advanceFrom = (id: number, values: MarkValues): number | null => {
    if (!stacked && view === 'grid') return stepFlat(frames, id, 1);
    const stack = stackOf.get(id);
    if (!stack) return null;
    if (values.rejected) {
      const within = nextUndecided(stack.frames, id, (f) => f.id !== id && undecided(f), { wrap: false });
      if (within != null) return within;
    }
    const next = stepStack(navStacks, id, 1);
    return next === id ? null : next;
  };
  /** The whole group behind a frame for a bulk decision; the filter may hide frames, but B should settle the burst. */
  const stackFramesOf = (f: Frame): Frame[] => {
    if (groupingNow === 'dupes') return review.frames.filter((x) => groupKeyOf(x, true) === groupKeyOf(f, true));
    if (groupingNow === 'vehicles') return stackOf.get(f.id)?.frames ?? [f];
    return review.frames.filter((x) => groupKeyOf(x) === groupKeyOf(f));
  };
  /**
   * One-key stack decision: `keep` keeps the focused frame and culls the rest, `cull` culls all. One markMany call
   * means one undo step, and no auto-advance: the view moves to the next stack that still has undecided frames.
   */
  const stackAction = (mode: 'keep' | 'cull', stack?: StackView) => {
    const target = stack
      ? stack.frames.find((f) => f.id === selected) ?? stack.lead
      : view === 'compare' ? activeCompareFrame : frame;
    if (!target) return;
    const members = stack ? stack.frames : stackFramesOf(target);
    const changes = stackDecision(members, mode === 'keep' ? target.id : 'cull');
    void rv.markMany(changes);
    if (!stacked) return;
    const settled = new Set(changes.map((c) => c.id));
    const currentLead = (stack?.lead ?? stackOf.get(target.id)?.lead)?.id ?? null;
    const byLead = new Map(navStacks.map((s) => [s.lead.id, s]));
    const holdsUndecided = (lead: Frame) => {
      const s = byLead.get(lead.id);
      return s ? s.frames.some((f) => !settled.has(f.id) && undecided(f)) : false;
    };
    let next = nextUndecided(navStacks.map((s) => s.lead), currentLead, holdsUndecided);
    if (next === currentLead) next = null;
    if (next != null) goTo(next, true);
  };

  verbs.current = {
    view,
    setView,
    stepFrame: (delta, repeat = false) => {
      if (view === 'compare') { const next = stepWithinStack(navStacks, (compareRight ?? frame)?.id ?? null, delta); if (next != null) setCompareRightId(next); return; }
      if (view === 'grid' && !stacked) { goTo(stepFlat(frames, selected, delta)); return; }
      const list = view === 'grid' ? shown : navStacks;
      const byFrame = view === 'grid' ? shownOf : stackOf;
      const stack = selected != null ? byFrame.get(selected) ?? null : null;
      if (!stack) {
        const next = stepWithinStack(list, selected, delta) ?? stepStack(list, selected, delta);
        goTo(next);
        return;
      }
      const direction = delta > 0 ? 'forward' : 'back';
      // A collapsed card is the stack's cover: → enters the stack on its first frame, as shot.
      if (view === 'grid' && expandedKey !== stack.key && delta > 0 && selected === stack.lead.id && stack.frames.length > 1) {
        rv.setSelected(stack.frames[0].id);
        rv.setExpanded(stack.frames[0].id);
        return;
      }
      // First/Last in the loupe send ±Infinity: those stay inside the stack.
      const bounded = delta === 1 || delta === -1;
      // In the grid, ← on the first frame of an open strip closes it onto the cover first; the next ←
      // crosses into the previous stack, like the loupe. A held key repeats within its stack, but one
      // keydown can cross at most one edge.
      const atCover = view === 'grid' && expandedKey !== stack.key;
      const edge = crossStacks && !repeat && (view !== 'grid' || direction === 'forward' || atCover);
      const step = bounded ? stepInStack(list, list.indexOf(stack), selected, direction, { crossStacks: edge }) : null;
      if (step == null) {
        const next = bounded ? null : stepWithinStack(list, selected, delta);
        if (next != null) {
          goTo(next);
        } else if (bounded && view === 'grid' && direction === 'back' && expandedKey === stack.key && selected === stack.frames[0]?.id) {
          // Back from the first frame closes the strip onto the cover.
          rv.setSelected(stack.lead.id);
          rv.setExpanded(null);
        }
        return;
      }
      if ('id' in step) {
        rv.setSelected(step.id);
        if (view === 'grid' ? expanded != null || stack.frames.length > 1 : expanded != null) rv.setExpanded(step.id);
        return;
      }
      const id = edgeFrameId(list[step.stackIndex], direction);
      // Landing on the next stack shows its strip, but a one-frame stack has none: it is just a frame.
      goTo(id, true);
    },
    stepBurst: (delta) => {
      if (view === 'compare') { const next = stepStack(navStacks, (compareRight ?? frame)?.id ?? null, delta); if (next != null) setCompareRightId(next); return; }
      if (view === 'grid' && !stacked) { goTo(stepRow(frames, selected, delta, cols)); return; }
      goTo(stepStack(view === 'grid' ? shown : navStacks, selected, delta));
    },
    nextBurst: jumpStack,
    nextUndecided: jumpUndecided,
    enter: () => {
      if (view !== 'grid' || !frame) return;
      const stack = shownOf.get(frame.id);
      if (stacked && stack && stack.frames.length > 1 && expandedKey !== stack.key) rv.setExpanded(frame.id);
      else openViewer();
    },
    escape: () => {
      if (view !== 'grid') setView('grid');
      else rv.setExpanded(null);
    },
    mark: (values) => {
      const target = view === 'compare' ? activeCompareFrame : frame;
      if (!target) return;
      const next = autoAdvance && view !== 'compare' && (values.picked === true || values.rejected === true) ? advanceFrom(target.id, values) : null;
      void rv.mark(values, target.id);
      goTo(next);
    },
    rejected: () => Boolean(activeCompareFrame && facts.get(activeCompareFrame.id)?.cull),
    picked: () => Boolean(activeCompareFrame?.picked),
    stackAction,
    undo: () => { const id = rv.undo(); if (id != null) goTo(id); },
    redo: () => { const id = rv.redo(); if (id != null) goTo(id); },
  };
  const viewerStack = frame ? stackOf.get(frame.id) ?? null : null;
  const compareStack = (compareRight ?? frame) ? stackOf.get((compareRight ?? frame)!.id) ?? null : null;
  const viewerPosition = (() => {
    if (!frame) return '';
    if (!viewerStack) return `${(frames.findIndex((f) => f.id === frame.id) + 1).toLocaleString()} / ${frames.length.toLocaleString()}`;
    const i = viewerStack.frames.findIndex((f) => f.id === frame.id);
    return `Frame ${i + 1} of ${viewerStack.frames.length} · burst ${(navStacks.indexOf(viewerStack) + 1).toLocaleString()} of ${navStacks.length.toLocaleString()}`;
  })();
  const toggleStack = (stack: StackView) => {
    const here = selected != null && stack.frames.some((f) => f.id === selected);
    const target = here ? selected! : stack.lead.id;
    rv.setSelected(target);
    // A one-frame stack is a frame: a click opens it in the loupe, exactly like a frame inside an open strip.
    if (isSingleton(stack)) { rv.setExpanded(null); openViewer(); return; }
    rv.setExpanded(expandedKey === stack.key ? null : target);
  };

  if (jobId == null) {
    return (
      <div className="screen">
        <Empty icon="folder" title="Choose a shoot to get started.">
          <p>Open an album from the library to review it here.</p>
          <button className="primary" onClick={onLibrary}>Open library</button>
        </Empty>
      </div>
    );
  }
  const filtered = filters.search !== '' || filters.facet !== null || filters.view !== 'all' || filters.minStars > 0;
  const emptyState = (
    <Empty icon="images" title={review.frames.length ? 'Nothing matches these filters.' : rv.loading ? 'Loading this album…' : scanning ? 'Waiting for the first frames…' : 'No photographs to show yet.'}>
      {filtered && <button className="ghost" onClick={() => patchFilters({ ...defaultFilters })}>Clear filters</button>}
    </Empty>
  );
  const excluded = review.frames.filter((f) => facts.get(f.id)?.cull).length;
  const pending = review.frames.filter((f) => !facts.get(f.id)?.cull && f.status !== 'done').length;
  // What each identify scope could still read. The keeper count follows the album's effective keepers (`rv.keepers`:
  // a manual pick when a burst has one, else the engine's suggestion), so it never re-derives the suggestion from
  // `burst_pick`; everything counts every unreviewed subject, culled frames included.
  const keeperEligible = keeperCount(review.frames, facts, keepers);
  const everythingEligible = everythingCount(review.frames, facts);
  const identifyScope = identifyScopeOf(albumSetting(currentJob, 'identify_scope', settings));
  const scopeEligible = identifyScope === 'everything' ? everythingEligible : keeperEligible;
  const busy = scanning || status.operations.some((key) => key.endsWith(`:${jobId}`));
  // Stage reruns: one at a time across the library, and never beside a scan or any
  // other background operation on this album. `running` is the stage on this album.
  const running = runningRerun(status.operations, jobId);
  const rerunBusy = busy || anyRerunRunning(status.operations);
  const doRerun = (stage: RerunStage) => {
    if (jobId == null || rerunBusy) return;
    void run(async () => {
      const out = await engine.rerunStage(jobId, stage);
      if (out?.operation) toast(rerunStartedText(stage));
      else { await rv.refresh(); toast(rerunDoneText(stage, null), { tone: 'ok' }); }
    });
  };
  const currentParent = profileParent(currentProfile);
  const kind = currentParent === 'portrait' ? 'portrait'
    : currentParent === 'motorsport' && !isOn(policy, 'vehicles') ? 'mixed'
    : currentParent === 'motorsport' ? (currentJob?.include_people || isOn(policy, 'faces') ? 'motorsport+people' : 'motorsport')
    : 'mixed';
  const albumSettings: Settings = {
    ...settings,
    // Per-album identify-step switches (OCR, decision, vision).
    identify_ocr: albumSetting(currentJob, 'identify_ocr', settings) as boolean | undefined ?? true,
    identify_decision: albumSetting(currentJob, 'identify_decision', settings) as boolean | undefined ?? true,
    use_decider: albumSetting(currentJob, 'use_decider', settings) as boolean | undefined ?? Boolean(settings.use_decider),
    read_numbers: albumSetting(currentJob, 'read_numbers', settings) as boolean | undefined ?? true,
    read_plates: albumSetting(currentJob, 'read_plates', settings) as boolean | undefined ?? true,
    read_action: albumSetting(currentJob, 'read_action', settings) as boolean | undefined ?? true,
    group_vehicles: albumSetting(currentJob, 'group_vehicles', settings) as boolean | undefined ?? true,
    grouping_stacks: albumSetting(currentJob, 'grouping_stacks', settings) as boolean | undefined ?? true,
    grouping_sensitivity: String(albumSetting(currentJob, 'grouping_sensitivity', settings) ?? 'normal'),
    grouping_pause: Number(albumSetting(currentJob, 'grouping_pause', settings) ?? 0.8),
    grouping_keep_pans: albumSetting(currentJob, 'grouping_keep_pans', settings) as boolean | undefined ?? true,
    grouping_focus_anchor: albumSetting(currentJob, 'grouping_focus_anchor', settings) as boolean | undefined ?? true,
    identify_scope: identifyScopeOf(albumSetting(currentJob, 'identify_scope', settings)),
    moments_threshold: thresholdOf(albumSetting(currentJob, 'moments_threshold', settings)),
    use_vlm: albumVisionOn(currentJob, settings),
  };
  /** Album-scoped changes go to this album only; the global defaults are edited in Settings. */
  const saveAlbum = async (patch: Record<string, unknown>) => {
    await engine.updateJobSettings(jobId, patch);
    await onJobsChanged();
    await rv.refresh();
  };
  const workflow = workflowModeOf(albumSetting(currentJob, 'workflow_mode', settings));
  // The stepper reads the album's own data, not `job.status`: a long-finished album can still say "scanning".
  const scanned = Boolean(currentJob && currentJob.total > 0 && currentJob.done >= currentJob.total) && !scanning;
  // `read_at` marks what Identify actually read; `reviewed` is only acceptance, so an accepted album stays identified.
  const hasRead = hasReadings(review.frames, facts);
  const identified = scopeEligible === 0 && review.detections.length > 0;
  const identifyStale = Boolean(albumSetting(currentJob, 'native_identify_stale', settings));
  const steps = pipelineSteps(workflow, { scanned, identified, identifying, scanning });
  // "Run again" offers each stage the album has actually run: detect/rate/pick need
  // a finished scan, identify needs the identify half of the workflow, moments needs
  // the moment lane on. A stage that never ran here is not offered.
  const rerunAvailable: Record<RerunStage, boolean> = {
    detect: scanned,
    rate: scanned,
    identify: workflowIncludesIdentify(workflow) && review.detections.length > 0,
    moments: isOn(policy, 'moments_detect'),
    pick: scanned,
  };
  const rerunStageItems: MenuItem[] = RERUN_STAGES.filter((stage) => rerunAvailable[stage]).map((stage) => ({
    id: `rerun_${stage}`,
    label: rerunLabel(stage, { vehicles: vehicleBrowse }),
    hint: rerunHint(stage),
    disabled: rerunBusy,
    onClick: () => doRerun(stage),
  }));
  const rerunItems: MenuItem[] = [
    ...rerunStageItems,
    ...(rerunStageItems.length > 0 ? [{ type: 'divider' as const }] : []),
    {
      id: 'group',
      label: `Re-group ${subject.many}`,
      hint: `Cluster matching ${subject.many} across different passes and bursts`,
      disabled: rerunBusy,
      onClick: () => {
        void run(async () => {
          await call('group', { jobId });
          await rv.refresh();
          toast(`${subject.title} re-grouped.`);
        });
      },
    },
    {
      id: 'rescan',
      label: 'Rescan from pixels…',
      hint: 'Re-read every photo and re-measure sharpness; keeps your edits',
      disabled: rerunBusy,
      onClick: () => setConfirmReset('rescan'),
    },
  ];
  const pickWorkflow = (value: string) => {
    void run(async () => {
      await saveAlbum({ workflow_mode: workflowModeOf(value) });
      toast(`Workflow: ${workflowLabel(value)}.`);
    });
  };
  // One label per scope for the split button and its menu, so the two cannot disagree. The workflow only decides
  // whether identification exists at all; the album's scope decides what it reads.
  const identifyLabels = {
    keepers: identifyActionLabel('keepers', keeperEligible),
    everything: identifyActionLabel('everything', everythingEligible),
  };
  const identifyTitle = identifyScope === 'everything'
    ? `Read ${subject.fields.readData} on every subject, culled frames included`
    : `Read ${subject.fields.readData} on the keeper you picked, or the suggested one`;

  const primaryStep = !busy && pending > 0 && workflowIncludesCull(workflow)
    ? {
        label: `Suggest culls (${pending.toLocaleString()})`,
        onClick: onCull,
        title: 'Run AI sharpness scoring and keeper selection on unculled photos',
      }
    : identifying
    ? {
        label: 'Identifying…',
        disabled: true,
        onClick: () => {},
        title: 'Identification is currently running',
      }
    : !busy && scopeEligible > 0 && workflowIncludesIdentify(workflow)
    ? {
        label: hasRead ? identifyNewLabel(scopeEligible) : identifyActionLabel(identifyScope, scopeEligible),
        onClick: () => onIdentify(identifyScope, true),
        title: hasRead
          ? `${scopeEligible.toLocaleString()} subject${scopeEligible === 1 ? '' : 's'} found since this album was identified; read only those`
          : identifyTitle,
      }
    : !identifying && workflowIncludesIdentify(workflow) && review.detections.some((d) => !d.reviewed)
    ? {
        label: 'Accept all',
        onClick: onAcceptAll,
        title: 'Accept all pending detections and identifications',
      }
    : {
        label: 'Export',
        onClick: () => setExportOpen(true),
        title: 'Export: write ratings, tags, and vehicle metadata to XMP sidecars',
      };

  const actionItems: MenuItem[] = [
    ...(workflowIncludesCull(workflow) ? [{
      id: 'cull',
      label: 'Suggest culls',
      hint: 'Re-evaluate sharpness and keepers for unculled photos',
      badge: pending > 0 ? pending : undefined,
      disabled: busy || pending === 0,
      onClick: onCull,
    }] : []),
    ...(workflowIncludesIdentify(workflow) ? [
      {
        id: 'identify_keepers',
        label: identifyLabels.keepers,
        hint: 'Read plates, numbers, and VLM on your keeper, or the suggested one',
        badge: keeperEligible > 0 ? keeperEligible.toLocaleString() : undefined,
        disabled: busy || identifying || keeperEligible === 0,
        onClick: () => onIdentify('keepers', true),
      },
      {
        id: 'identify_everything',
        label: identifyLabels.everything,
        hint: 'Read every subject, culled frames included; bystanders skipped',
        badge: everythingEligible > 0 ? everythingEligible.toLocaleString() : undefined,
        disabled: busy || identifying || everythingEligible === 0,
        onClick: () => onIdentify('everything', true),
      },
      {
        id: 'skip_identify',
        label: 'Skip identify',
        hint: 'Accept what is on disk as it stands and go to Review; nothing is read',
        badge: scopeEligible > 0 ? scopeEligible.toLocaleString() : undefined,
        disabled: busy || identifying || !hasRead || scopeEligible === 0,
        onClick: onSkipAlbumIdentify,
      },
      {
        id: 'identified',
        label: 'Identifications…',
        hint: 'See what was read on each photo, and accept it one by one',
        disabled: !review.detections.some((d) => d.number || d.plate || (d.attributes && d.attributes !== '{}')),
        onClick: () => setIdsOpen(true),
      },
      {
        id: 'accept_all',
        label: 'Accept all identifications',
        hint: 'Mark all unreviewed detections in this shoot as accepted',
        disabled: !review.detections.some((d) => !d.reviewed),
        onClick: onAcceptAll,
      },
    ] : []),
    {
      id: 'write',
      label: 'Export…',
      hint: 'Review what leaves Conrod, then write XMP sidecars',
      onClick: () => setExportOpen(true),
    },
    {
      id: 'dry_run',
      label: 'Dry run (preview XMP)',
      hint: 'List what would be written without touching files on disk',
      onClick: () => onWrite(true),
    },
  ];

  const manageItems: MenuItem[] = [
    {
      id: 'shoot_settings',
      label: 'Shoot settings & preset…',
      hint: `Preset: ${profileLabel(currentProfile)}`,
      onClick: () => setShootSettingsOpen(true),
    },
    { type: 'divider' },
    { type: 'header', label: 'Culling' },
    {
      type: 'checkbox',
      id: 'toggle_auto_advance',
      label: 'Auto-advance after P and X',
      hint: 'P goes to the next burst, X to the next undecided frame',
      checked: autoAdvance,
      onChange: (checked) => setAutoAdvance(checked),
    },
    {
      type: 'checkbox',
      id: 'toggle_cross_stacks',
      label: 'Arrow keys continue into the next stack',
      hint: '→ on a stack\u2019s last frame opens the next stack; ← on its first goes back',
      checked: crossStacks,
      onChange: (checked) => setCrossStacks(checked),
    },
    { type: 'divider' },
    { type: 'header', label: 'Album resets' },
    {
      id: 'reset_ratings',
      label: 'Remove all ratings…',
      hint: 'Clears manual stars and restores automated ratings',
      danger: true,
      onClick: () => setConfirmReset('reset_ratings'),
    },
    {
      id: 'reset_identifications',
      label: 'Remove all identifications…',
      hint: 'Clears plates, numbers, and detected subject names',
      danger: true,
      onClick: () => setConfirmReset('reset_identifications'),
    },
    {
      id: 'reset_detections',
      label: 'Reset detections & crops…',
      hint: 'Removes bounding boxes and cached crop files',
      danger: true,
      onClick: () => setConfirmReset('reset_detections'),
    },
    {
      id: 'reset_all',
      label: 'Reset all…',
      danger: true,
      hint: 'Back to the moment this album was created. Deletes every measurement, star, pick and identification here.',
      onClick: () => setConfirmReset('reset_album'),
    },
    { type: 'divider' },
    {
      id: 'rename',
      label: 'Rename shoot…',
      onClick: () => {
        const currentName = currentJob?.label ?? '';
        const next = window.prompt('Rename album to:', currentName);
        if (next && next.trim() && next.trim() !== currentName) {
          void run(async () => {
            await call('rename_job', { jobId, label: next.trim() });
            await rv.refresh();
            toast('Album renamed.');
          });
        }
      },
    },
    {
      id: 'stats',
      label: 'Shoot statistics…',
      hint: 'Frames, gear, exposure and quality',
      onClick: () => setStatsOpen(true),
    },
    {
      id: 'lightroom_import',
      label: 'Import from Lightroom…',
      hint: 'Read the flags, stars and colour labels you gave this shoot in Lightroom Classic',
      onClick: () => setLightroomOpen(true),
    },
  ];

  return (
    <div className="review">
      <div className="toolbar">
        <select aria-label="Album" value={jobId} onChange={(e) => onPickJob(Number(e.target.value))}>
          {jobs.map((j) => <option key={j.id} value={j.id}>{j.label}</option>)}
        </select>
        {currentProfile && (
          <button
            type="button"
            className="pill scan-type clickable"
            title={`Preset: ${profileLabel(currentProfile)} (Click to change shoot preset or detection options)`}
            onClick={() => setShootSettingsOpen(true)}
          >
            {profileLabel(currentProfile)}<Icon name="sliders" />
          </button>
        )}
        <input id="search" type="search" aria-label="Search photos" placeholder="Search" spellCheck={false}
          value={filters.search} onChange={(e) => patchFilters({ search: e.target.value })} />
        <select aria-label="Photo filter" value={filters.view} onChange={(e) => patchFilters({ view: e.target.value as typeof filters.view })}>
          <option value="all">All photos</option><option value="keepers">Keepers</option><option value="undecided">Undecided</option><option value="rejected">Rejected</option><option value="top3">Top 3 per burst</option><option value="review">Needs review</option>{(vehicleBrowse || filters.view === 'needsLook') && <option value="needsLook">Needs a look</option>}
        </select>
        {stacked && workflow !== 'identify' && (
          <span className="reviewed-count" title="Bursts where every frame is kept or culled, over the whole album">
            reviewed {progress.reviewed.toLocaleString()} / {progress.total.toLocaleString()} bursts
          </span>
        )}
        <select aria-label="Group photos" value={groupingNow} onChange={(e) => rv.setGrouping(e.target.value as typeof grouping)}>
          <option value="bursts">Group: Burst stacks</option>{vehicleBrowse && <option value="vehicles">Group: By vehicle</option>}<option value="dupes">Group: Duplicates</option><option value="flat">Group: Individual photos</option>
        </select>
        <select aria-label="Photo order" value={!review.tasteHead && filters.sort === 'taste' ? 'frame' : filters.sort} onChange={(e) => patchFilters({ sort: e.target.value as typeof filters.sort })}>
          <option value="frame">Filename order</option><option value="best">Highest rated first</option><option value="worst">Lowest rated first</option><option value="pick">Keepers first</option>{review.tasteHead && <option value="taste">Taste (learned)</option>}
        </select>
        <div className="toolbar-actions">
          <MenuDropdown
            splitAction={primaryStep}
            items={actionItems}
            title="Workflow actions"
            align="right"
          />
          <MenuDropdown
            label="Manage shoot"
            variant="ghost"
            items={manageItems}
            align="right"
          />
          <button className="ghost icon" title="Keyboard shortcuts (?)" aria-label="Keyboard shortcuts" onClick={onHelp}><Icon name="keyboard" /></button>
        </div>
      </div>
      <div className="pipeline-row">
        <label className="pipeline-workflow" title="What this album runs: culling, identification, or both">
          <span>Workflow</span>
          <select aria-label="Workflow" value={workflow} onChange={(e) => pickWorkflow(e.target.value)}>
            {WORKFLOW_OPTIONS.map((option) => <option key={option.id} value={option.id}>{option.label}</option>)}
          </select>
        </label>
        <PipelineStepper steps={steps} />
        {(rerunItems.length > 0 || running) && (
          <MenuDropdown
            className="pipeline-rerun"
            variant="ghost"
            align="left"
            title="Run one step again on this album, without redoing the whole thing"
            label={running
              ? <span className="pipeline-rerun-running"><span className="work-spinner" />{rerunLabel(running, { vehicles: vehicleBrowse })}…</span>
              : 'Run again'}
            items={rerunItems}
          />
        )}
        <span
          className="apply-note"
          title="Conrod's stars and suggested picks reach the files, and the Lightroom plug-in, without a separate command."
        >
          {applyLine({ autoApply: settings.auto_apply !== false, status: currentJob?.status, pickLabel: typeof settings.pick_label === 'string' ? settings.pick_label : null })}
        </span>
        {identifying && (
          <button type="button" className="ghost small pipeline-watch" onClick={onWatchIdentify}
            title="Open the live identification trace">
            <Icon name="eye" size={13} /> Watch identification
          </button>
        )}
      </div>
      <ShootSettingsModal
        jobLabel={currentJob?.label ?? ''}
        initialProfile={currentProfile}
        initialSettings={albumSettings}
        initialOverrides={albumOverrides}
        initialUserDefaults={userDefaults}
        initialVision={albumVisionChoice(currentJob)}
        globals={settings}
        visible={shootSettingsOpen}
        onClose={() => setShootSettingsOpen(false)}
        onSave={async (patch) => {
          const { overrides: wanted, ...rest } = patch;
          // There is no single engine call for the whole patch, so run the parts in order, stop at the first
          // failure and report exactly what was saved: never half-applied in silence.
          let rescan = false;
          const steps: SaveStep[] = [{ label: 'Shoot settings', run: () => engine.updateJobSettings(jobId, rest) }];
          // Each changed detector or reader goes through `set_feature`: it re-rates, and finds faces for a shoot that had none.
          for (const key of new Set([...Object.keys(wanted), ...Object.keys(albumOverrides)])) {
            if (wanted[key] === albumOverrides[key]) continue;
            steps.push({ label: featureLabel(key), run: async () => { const result = await engine.setFeature(jobId, key, wanted[key] ?? null); rescan = rescan || result.needsRescan; } });
          }
          // A grouping choice only shows once the album is regrouped; a preset or detection change is safe to regroup too.
          steps.push({ label: 'Regroup stacks', run: () => call('regroup', { jobId }) });
          const outcome = await runSaveSteps(steps);
          // Whatever did save is real: refresh so the screen shows it, then tell the modal why it stays open.
          await Promise.resolve(onJobsChanged()).catch(() => undefined);
          await rv.refresh().catch(() => undefined);
          if (outcome.failed) throw new Error(saveFailureMessage(outcome));
          if (rescan) toast('Rescan this album to look for the newly switched-on subjects.');
          toast('Shoot settings updated for this album and its stacks regrouped.');
        }}
      />
      <ShootStatsModal
        visible={statsOpen}
        jobId={jobId}
        jobLabel={currentJob?.label ?? ''}
        onClose={() => setStatsOpen(false)}
        onStatsLoaded={() => void rv.refresh()}
      />
      <Identifications
        visible={idsOpen}
        jobId={jobId}
        albumLabel={currentJob?.label ?? ''}
        identifying={identifying}
        onClose={() => setIdsOpen(false)}
        onOpenFrame={(id) => { setIdsOpen(false); rv.setSelected(id); setView('grid'); }}
        onAccepted={() => { void onJobsChanged(); void rv.refresh(); }}
        toast={toast}
      />
      <ExportSheet
        visible={exportOpen}
        jobId={jobId}
        albumLabel={currentJob?.label ?? ''}
        frames={review.frames}
        keepers={rv.keepers}
        dryRunFirst={false}
        sendOnOpen={lightroomDev === 'sent'}
        onClose={() => setExportOpen(false)}
        onWrite={onWrite}
      />
      <LightroomImport
        visible={lightroomOpen}
        jobId={jobId}
        albumLabel={currentJob?.label ?? ''}
        autoRun={lightroomDev === 'imported'}
        onClose={() => setLightroomOpen(false)}
        onImported={() => void rv.refresh()}
      />
      {confirmReset === 'reset_ratings' && (
        <Confirm
          title="Remove all ratings?"
          action="Remove ratings"
          onCancel={() => setConfirmReset(null)}
          onConfirm={() => {
            void run(async () => {
              await call('reset_ratings', { jobId });
              await rv.refresh();
              toast('All ratings and rejections cleared.');
            });
            setConfirmReset(null);
          }}
        >
          <p>This removes all manual star ratings and rejection flags for this album, restoring original automated ratings. Original photograph files remain untouched.</p>
        </Confirm>
      )}
      {confirmReset === 'reset_identifications' && (
        <Confirm
          title="Remove all identifications?"
          action="Remove identifications"
          onCancel={() => setConfirmReset(null)}
          onConfirm={() => {
            void run(async () => {
              await call('reset_identifications', { jobId });
              await rv.refresh();
              toast(subject.fields.removedDone);
            });
            setConfirmReset(null);
          }}
        >
          <p>This removes all {subject.fields.removed} detected or assigned in this album. Original photograph files and photo ratings remain untouched.</p>
        </Confirm>
      )}
      {confirmReset === 'reset_detections' && (
        <Confirm
          title="Reset detections and crops?"
          action="Reset detections"
          onCancel={() => setConfirmReset(null)}
          onConfirm={() => {
            void run(async () => {
              await call('reset_detections', { jobId });
              await rv.refresh();
              toast('Detections and cached crops reset.');
            });
            setConfirmReset(null);
          }}
        >
          <p>This removes all detections, bounding boxes, and cached crop images for this album. Original photograph files stay untouched.</p>
        </Confirm>
      )}
      {confirmReset === 'rescan' && (
        <Confirm
          title="Rescan this album?"
          action="Rescan"
          onCancel={() => setConfirmReset(null)}
          onConfirm={() => {
            void run(async () => {
              await call('rescan', { jobId });
              await rv.refresh();
              toast('Rescan started. Progress in Activity.');
            });
            setConfirmReset(null);
          }}
        >
          <p>This re-reads every photograph in this album and re-measures its sharpness, so it can take a while. Boxes, stars, rejections, reviews, numbers and names are kept.</p>
        </Confirm>
      )}
      {confirmReset === 'reset_album' && (
        <ResetAlbumDialog
          onCancel={() => setConfirmReset(null)}
          onConfirm={async () => {
            const done = await run(async () => {
              await call('reset_album', { jobId });
              await rv.refresh();
              toast('Album reset to its imported state.');
              return true;
            });
            if (done) setConfirmReset(null);
          }}
        />
      )}
      {scanning && (
        <section className="identify-banner scan-running" aria-live="polite">
          <div className="identify-copy">
            <div className="identify-title">
              <Icon name="eye" size={16} />
              <div>
                <b>Scan in progress{scanTask && scanTask.total > 0 ? ` — ${pct(scanTask.done, scanTask.total)}% of this stage` : ''}</b>
                <span>Review is showing the frames already measured. Verdicts on finished bursts can still change; more frames arrive as the scan runs.{scanTask?.eta != null ? ` About ${etaText(scanTask.eta)} left in this stage.` : ''}</span>
              </div>
              <button className="ghost small" onClick={onWatchScan}>Watch scan</button>
            </div>
          </div>
        </section>
      )}
      {identifyTask && <IdentificationBanner task={identifyTask} settings={albumSettings} title={identifyBannerTitle(policy)} onSkip={onSkipIdentify} />}
      {hasRead && !identifying && (scopeEligible > 0 || identifyStale) && (
        <section className="identify-banner identify-again" aria-live="polite">
          <div className="identify-copy">
            <div className="identify-title">
              <Icon name="check" size={16} />
              <div>
                <b>This album is identified</b>
                <span>
                  {[
                    scopeEligible > 0 ? `${scopeEligible.toLocaleString()} new ${scopeEligible === 1 ? subject.one : subject.many} to read` : null,
                    identifyStale ? 'the reader settings changed since it was read' : null,
                  ].filter(Boolean).join(' · ')}
                </span>
              </div>
            </div>
            <p className="muted">
              What is on disk stays as it is unless you ask to read it again.
            </p>
          </div>
          <div className="identify-actions">
            {scopeEligible > 0 && (
              <button type="button" className="primary" onClick={() => onIdentify(identifyScope, true)}>
                Identify new only
              </button>
            )}
            <button type="button" className="ghost" onClick={onSkipAlbumIdentify}>Skip</button>
          </div>
        </section>
      )}
      <div className="review-main">
        <Facets preset={currentProfile} overrides={{ ...userDefaults, ...albumOverrides }} facets={rv.facets} facet={filters.facet} onPick={(facet) => patchFilters({ facet })} />
        {stacked
          ? <StackGrid stacks={shown} selectedKey={selectedKey} selectedId={selected} expandedKey={expandedKey} keeperIds={keepers} facts={facts} suggestions={suggestions} softBursts={softBursts}
              culled={(f) => Boolean(facts.get(f.id)?.cull)} onToggle={toggleStack} onSelect={rv.setSelected} onOpen={openViewer} onPick={acceptSuggestion}
              onDecide={(id, values) => void rv.mark(values, id)} onReject={(id, rejected) => void rv.mark({ rejected }, id)} onStackAction={(stack, mode) => stackAction(mode, stack)}
              ariaLabel={groupingNow === 'dupes' ? 'Duplicate stacks' : groupingNow === 'vehicles' ? 'Vehicle stacks' : 'Burst stacks'}
              empty={emptyState} />
          : <PhotoGrid frames={frames} facts={facts} ranks={ranks} keepers={keepers} singletons={singletons} suggestions={suggestions} selected={selected} onSelect={rv.setSelected} onOpen={openViewer} onCols={setCols}
              onReject={(id, rejected) => void rv.mark({ rejected }, id)} onPick={acceptSuggestion} onDecide={(id, values) => void rv.mark(values, id)} empty={emptyState} />}
        <Inspector frame={frame} detections={frame ? facts.get(frame.id)?.dets ?? [] : []} allDetections={review.detections} allFrames={review.frames} known={known} scanning={scanning} kind={kind}
          why={frame ? facts.get(frame.id)?.why ?? null : null}
          moments={{ vocabulary: policy.moments.vocabulary, threshold: thresholdOf(albumSetting(currentJob, 'moments_threshold', settings)), rankOn: isOn(policy, 'moments_rank'), detectOn: isOn(policy, 'moments_detect') }}
          onMark={(values) => void rv.mark(values)} onOpen={openViewer} onEdit={onEdit} />
      </div>
      {view === 'loupe' && frame && (
        <Viewer rv={rv} run={run} showBoxes={showBoxes} onToggleBoxes={onToggleBoxes} onClose={() => setView('grid')}
          stack={viewerStack} keeperIds={keepers} onStep={(d) => verbs.current.stepFrame(d)} position={viewerPosition} />
      )}
      {view === 'compare' && frame && (
        <Compare
          rv={rv}
          run={run}
          showBoxes={showBoxes}
          onToggleBoxes={onToggleBoxes}
          right={compareRight}
          activePane={comparePane}
          onSetActivePane={setComparePane}
          onPickFilmstrip={setCompareRightId}
          onClose={() => setView('grid')}
          stack={compareStack}
          keeperIds={keepers}
        />
      )}
    </div>
  );
}
