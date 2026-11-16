import { useMemo, useEffect, useState, type MutableRefObject } from 'react';
import { Confirm, Empty, Icon } from '../components/basics';
import { Facets } from '../components/Facets';
import { Inspector } from '../components/Inspector';
import { PhotoGrid } from '../components/PhotoGrid';
import { StackGrid, type StackView } from '../components/StackGrid';
import { burstRankMap, decisionOf, defaultFilters, filename, vehicleGroups, type FrameFacts } from '../lib/review';
import { buildStacks, makeStack, nextUndecided, softBursts as softBurstMapOf, stepFlat, stepInStack, stepRow, stepStack, suggestionSizes, type Stack } from '../lib/stacks.mjs';
import { tasteStacks } from '../lib/taste.mjs';
import { albumSetting } from '../lib/album.mjs';
import { call, engine } from '../lib/api';
import { useLocalFlag, useRun } from '../state/hooks';
import { useToast } from '../state/toast';
import { MenuDropdown, type MenuItem } from '../components/MenuDropdown';
import { ShootSettingsModal } from '../components/ShootSettingsModal';
import { ExportSheet } from '../components/ExportSheet';
import { Identifications } from '../components/Identifications';
import type { Frame, Job, KnownVehicle, MarkValues, Settings, Status, Task } from '../lib/types';
import { profileLabel, profileParent } from '../lib/types';
import { facetsFor } from '../lib/facets.mjs';
import type { ReviewModel } from '../state/useReview';
import { subjectExcluded } from '../review.mjs';
import { Viewer } from './Viewer';
import { Compare } from './Compare';

/** G/E/C hotkeys switch between the grid, the loupe (Viewer) and the compare view. */
export type ReviewView = 'grid' | 'loupe' | 'compare';
/** How App's global hotkey handler reaches into whichever view Review currently owns (mirrors Train's `verbs` ref). */
export type ReviewVerbs = {
  view: ReviewView; setView: (v: ReviewView) => void;
  stepFrame: (delta: number) => void; stepBurst: (delta: number) => void; nextBurst: (delta: number) => void;
  nextUndecided: () => void; enter: () => void; escape: () => void;
  mark: (values: MarkValues) => void; rejected: () => boolean; picked: () => boolean;
  undo: () => void; redo: () => void;
};
export const noReviewVerbs: ReviewVerbs = {
  view: 'grid', setView: () => {}, stepFrame: () => {}, stepBurst: () => {}, nextBurst: () => {}, nextUndecided: () => {},
  enter: () => {}, escape: () => {}, mark: () => {}, rejected: () => false, picked: () => false,
  undo: () => {}, redo: () => {},
};

function currentItem(task: Task): string {
  const detail = task.detail?.trim();
  if (!detail) return 'Preparing the next photograph…';
  return detail.split(/[\\/]/).at(-1) ?? detail;
}

function IdentificationBanner({ task, settings, onSkip }: { task: Task; settings: Settings; onSkip: () => void }) {
  const models = [
    settings.read_plates && 'plate reader',
    (settings.read_plates || settings.read_numbers) && 'OCR',
    settings.use_vlm && String(settings.vlm_model || 'vision model'),
    settings.group_vehicles && 'visual matcher',
  ].filter(Boolean) as string[];
  const percent = task.total > 0 ? Math.min(100, Math.round((task.done / task.total) * 100)) : 0;

  return (
    <section className="identify-strip" aria-live="polite">
      <div className="identify-strip-main">
        <div className="identify-strip-left">
          <span className="work-spinner" />
          <span className="identify-strip-title">Identifying vehicles</span>
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

function PipelineStepper({ job, scanning, identified, identifying, written }: { job: Job | undefined; scanning: boolean; identified: boolean; identifying: boolean; written: boolean }) {
  const steps = [
    { key: 'scan', label: 'Scanned', done: Boolean(job && job.status !== 'scanning'), active: scanning },
    { key: 'cull', label: 'Culled', done: Boolean(job && (job.status === 'done' || job.status === 'indexed')), active: false },
    { key: 'identify', label: 'Identified', done: identified && !identifying, active: identifying },
    { key: 'write', label: 'XMP written', done: written, active: false },
  ];
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
  onPickJob: (id: number) => void; onIdentify: () => void; onCull: () => void;
  onHelp: () => void; onOpenViewer: () => void; onLibrary: () => void;
  onEdit: (detectionId: number, updates: Record<string, string | string[] | null>) => unknown;
  onAcceptAll: () => void; onSkipIdentify: () => void; onWrite: (dryRun: boolean) => void;
  /** Album settings changed (preset, toggles): reload the job list so the new values show. */
  onJobsChanged: () => Promise<unknown> | void;
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

/** Toolbar (album, view, search, stars, sort, actions), facets, the card grid and the inspector. */
export function ReviewScreen({ rv, jobs, jobId, scanning, status, settings, showBoxes, onToggleBoxes, verbs, onPickJob, onIdentify, onCull, onHelp, onOpenViewer, onLibrary, onEdit, onAcceptAll, onSkipIdentify, onWrite, onJobsChanged }: Props) {
  const [known, setKnown] = useState<KnownVehicle[]>([]);
  const [confirmReset, setConfirmReset] = useState<'reset_ratings' | 'reset_identifications' | 'reset_detections' | 'rescan' | null>(null);
  const [shootSettingsOpen, setShootSettingsOpen] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [autoAdvance, setAutoAdvance] = useLocalFlag('conrod.autoAdvance', true);
  const [cols, setCols] = useState(4);
  const run = useRun();
  const toast = useToast();
  useEffect(() => { void engine.known().then(setKnown); }, [jobId]);
  const { filters, patchFilters, frames, review, facts, keepers, selected, frame, grouping, expanded } = rv;
  const currentJob = jobs.find((j) => j.id === jobId);
  const currentProfile = String(currentJob?.scan_profile || settings.scan_profile || 'motorsport');
  // Vehicle-only controls (by-vehicle grouping, "needs a look" for unread numbers and plates) go away for shoots with no vehicles to read.
  const vehicleBrowse = facetsFor(currentProfile).some((k) => k === 'number' || k === 'plate' || k === 'make');
  const groupingNow = grouping === 'vehicles' && !vehicleBrowse ? 'bursts' : grouping;
  const identifyTask = jobId == null ? undefined : status.tasks.find((task) => (task.state === 'running' || task.state === 'paused') && /^Identifying\b/.test(task.label) && task.label.includes(`album ${jobId}`));
  const identifying = Boolean(identifyTask || (jobId != null && status.operations.some((key) => key === `Identifying:${jobId}`)));

  // The one filtered model: every stack below is built from `frames`, the list the toolbar already narrowed.
  const ranks = useMemo(() => burstRankMap(review.frames, keepers), [review.frames, keepers]);
  // Whole-album, like `keepers`: a filter must not change which frame a burst suggests.
  const suggestions = useMemo(() => suggestionSizes(review.frames, facts), [review.frames, facts]);
  // Bursts the engine refused to keep anything from: the grid says "No keeper".
  const softBursts = useMemo(() => softBurstMapOf(review.frames, facts), [review.frames, facts]);
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
  const expandedKey = expanded != null ? shownOf.get(expanded)?.key ?? null : null;
  const selectedKey = selected != null ? shownOf.get(selected)?.key ?? null : null;

  // 1.3: which of grid / loupe (Viewer) / compare is showing; G/E/C reach this through `verbs`.
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
  /** Move the selection; an open stack stays open on the stack the selection lands in. */
  const goTo = (id: number | null) => {
    if (id == null) return;
    rv.setSelected(id);
    if (expanded != null) rv.setExpanded(id);
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

  verbs.current = {
    view,
    setView,
    stepFrame: (delta) => {
      if (view === 'compare') { const next = stepInStack(navStacks, (compareRight ?? frame)?.id ?? null, delta); if (next != null) setCompareRightId(next); return; }
      if (view === 'grid' && !stacked) { goTo(stepFlat(frames, selected, delta)); return; }
      const next = stepInStack(view === 'grid' ? shown : navStacks, selected, delta) ?? stepStack(view === 'grid' ? shown : navStacks, selected, delta);
      if (next == null) return;
      rv.setSelected(next);
      if (view === 'grid' && (stackOf.get(next)?.frames.length ?? 0) > 1) rv.setExpanded(next);
      else if (expanded != null) rv.setExpanded(next);
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
  const eligible = review.frames.filter((f) => (facts.get(f.id)?.dets ?? []).some((d) => !d.reviewed && !subjectExcluded(f, d))).length;
  const busy = scanning || status.operations.some((key) => key.endsWith(`:${jobId}`));
  const currentParent = profileParent(currentProfile);
  const kind = currentParent === 'portrait' ? 'portrait'
    : currentParent === 'motorsport' ? (currentJob?.include_people ? 'motorsport+people' : 'motorsport')
    : 'mixed';
  const albumSettings: Settings = {
    ...settings,
    read_numbers: albumSetting(currentJob, 'read_numbers', settings) as boolean | undefined ?? true,
    read_plates: albumSetting(currentJob, 'read_plates', settings) as boolean | undefined ?? true,
    group_vehicles: albumSetting(currentJob, 'group_vehicles', settings) as boolean | undefined ?? true,
  };
  /** Album-scoped changes go to this album only; the global defaults are edited in Settings. */
  const saveAlbum = async (patch: Record<string, unknown>) => {
    await engine.updateJobSettings(jobId, patch);
    await onJobsChanged();
    await rv.refresh();
  };

  const primaryStep = !busy && pending > 0
    ? {
        label: `Suggest culls (${pending.toLocaleString()})`,
        onClick: onCull,
        title: 'Run AI sharpness scoring and keeper selection on unculled photos',
      }
    : !busy && pending === 0 && eligible > 0 && !identifying
    ? {
        label: `Identify kept photos (${eligible.toLocaleString()})`,
        onClick: onIdentify,
        title: 'Run plate, number, and vision model identification on kept photos',
      }
    : identifying
    ? {
        label: 'Identifying…',
        disabled: true,
        onClick: () => {},
        title: 'Identification is currently running',
      }
    : !identifying && review.detections.some((d) => !d.reviewed)
    ? {
        label: 'Accept all',
        onClick: onAcceptAll,
        title: 'Accept all pending detections and identifications',
      }
    : {
        label: 'Write XMP',
        onClick: () => setExportOpen(true),
        title: 'Export: write ratings, tags, and vehicle metadata to XMP sidecars',
      };

  const actionItems: MenuItem[] = [
    {
      id: 'cull',
      label: 'Suggest culls',
      hint: 'Re-evaluate sharpness and keepers for unculled photos',
      badge: pending > 0 ? pending : undefined,
      disabled: busy || pending === 0,
      onClick: onCull,
    },
    {
      id: 'identify',
      label: 'Identify kept photos',
      hint: 'Run plates, numbers, and VLM on unreviewed kept photos',
      badge: eligible > 0 ? eligible : undefined,
      disabled: busy || identifying || eligible === 0,
      onClick: onIdentify,
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
    { type: 'divider' },
    { type: 'header', label: 'Reprocessing' },
    {
      id: 'rescore',
      label: 'Re-score sharpness',
      hint: 'Re-measure focus and sharpness across all frames',
      onClick: () => {
        void run(async () => {
          await call('rescore', { jobId });
          await rv.refresh();
          toast('Sharpness re-scoring started. Progress in Activity.');
        });
      },
    },
    {
      id: 'rescan',
      label: 'Rescan from pixels…',
      hint: 'Re-read every photo and re-measure sharpness; keeps your edits',
      onClick: () => setConfirmReset('rescan'),
    },
    {
      id: 'pick_keepers',
      label: 'Re-pick burst keepers',
      hint: 'Select the best sharpest photo in each burst sequence',
      onClick: () => {
        void run(async () => {
          await call('pick_keepers', { jobId });
          await rv.refresh();
          toast('Burst keepers updated.');
        });
      },
    },
    {
      id: 'group',
      label: 'Re-group vehicles',
      hint: 'Cluster matching vehicles across different passes and bursts',
      onClick: () => {
        void run(async () => {
          await call('group', { jobId });
          await rv.refresh();
          toast('Vehicles re-grouped.');
        });
      },
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
    { type: 'header', label: 'Detection options' },
    {
      type: 'checkbox',
      id: 'toggle_numbers',
      label: 'Read race numbers',
      hint: 'Detect vehicle competition numbers (#0–999)',
      checked: albumSettings.read_numbers !== false,
      onChange: (checked) => {
        void run(async () => {
          await saveAlbum({ read_numbers: checked });
          toast(`Race number detection turned ${checked ? 'on' : 'off'}.`);
        });
      },
    },
    {
      type: 'checkbox',
      id: 'toggle_plates',
      label: 'Read licence plates',
      hint: 'Detect vehicle licence and registration plates',
      checked: albumSettings.read_plates !== false,
      onChange: (checked) => {
        void run(async () => {
          await saveAlbum({ read_plates: checked });
          toast(`Licence plate detection turned ${checked ? 'on' : 'off'}.`);
        });
      },
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
      hint: 'Clears plates, numbers, and detected car names',
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
      id: 'summary',
      label: 'Shoot summary',
      onClick: () => {
        void run(async () => {
          const s = await call<{ images: Record<string, number>; counts: Record<string, number> }>('summary', { jobId });
          toast(`${s.images.scanned} scanned · ${s.images.written} written · ${s.images.errors} errors · ${s.counts.numbered} numbered · ${s.counts.plated} plated · ${s.counts.to_review} to review`);
        });
      },
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
            style={{ alignSelf: 'center' }}
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
        <select aria-label="Group photos" value={groupingNow} onChange={(e) => rv.setGrouping(e.target.value as typeof grouping)}>
          <option value="bursts">Group: Burst stacks</option>{vehicleBrowse && <option value="vehicles">Group: By vehicle</option>}<option value="dupes">Group: Duplicates</option><option value="flat">Group: Individual photos</option>
        </select>
        <select aria-label="Photo order" value={!review.tasteHead && filters.sort === 'taste' ? 'frame' : filters.sort} onChange={(e) => patchFilters({ sort: e.target.value as typeof filters.sort })}>
          <option value="frame">Filename order</option><option value="best">Highest rated first</option><option value="worst">Lowest rated first</option><option value="pick">Keepers first</option>{review.tasteHead && <option value="taste">Taste (learned)</option>}
        </select>
        <div className="spacer" />
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
      <PipelineStepper job={currentJob} scanning={scanning} identified={!eligible || review.detections.some((d) => d.reviewed)} identifying={identifying} written={false} />
      <ShootSettingsModal
        jobId={jobId}
        jobLabel={currentJob?.label ?? ''}
        initialProfile={currentProfile}
        initialSettings={albumSettings}
        visible={shootSettingsOpen}
        onClose={() => setShootSettingsOpen(false)}
        onSave={async (patch) => {
          await saveAlbum(patch);
          toast('Shoot settings updated for this album.');
        }}
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
        albumLabel={currentJob?.label ?? ''}
        frames={review.frames}
        keepers={rv.keepers}
        dryRunFirst={false}
        onClose={() => setExportOpen(false)}
        onWrite={onWrite}
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
              toast('All vehicle identifications removed.');
            });
            setConfirmReset(null);
          }}
        >
          <p>This removes all vehicle plates, race numbers, and models detected or assigned in this album. Original photograph files and photo ratings remain untouched.</p>
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
      {identifyTask && <IdentificationBanner task={identifyTask} settings={settings} onSkip={onSkipIdentify} />}
      <div className="review-main">
        <Facets preset={currentProfile} facets={rv.facets} facet={filters.facet} onPick={(facet) => patchFilters({ facet })} />
        {stacked
          ? <StackGrid stacks={shown} selectedKey={selectedKey} selectedId={selected} expandedKey={expandedKey} keeperIds={keepers} suggestions={suggestions} softBursts={softBursts}
              culled={(f) => Boolean(facts.get(f.id)?.cull)} onToggle={toggleStack} onSelect={rv.setSelected} onOpen={openViewer} onPick={acceptSuggestion}
              ariaLabel={groupingNow === 'dupes' ? 'Duplicate stacks' : groupingNow === 'vehicles' ? 'Vehicle stacks' : 'Burst stacks'}
              empty={emptyState} />
          : <PhotoGrid frames={frames} facts={facts} ranks={ranks} keepers={keepers} suggestions={suggestions} selected={selected} onSelect={rv.setSelected} onOpen={openViewer} onCols={setCols}
              onReject={(id, rejected) => void rv.mark({ rejected }, id)} onPick={acceptSuggestion} empty={emptyState} />}
        <Inspector frame={frame} detections={frame ? facts.get(frame.id)?.dets ?? [] : []} allDetections={review.detections} allFrames={review.frames} known={known} scanning={scanning} kind={kind}
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
