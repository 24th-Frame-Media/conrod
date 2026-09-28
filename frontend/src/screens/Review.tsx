import { Fragment, useMemo, CSSProperties, useEffect, useState, type MutableRefObject } from 'react';
import { Confirm, Empty, Icon } from '../components/basics';
import { Facets } from '../components/Facets';
import { Inspector } from '../components/Inspector';
import { PhotoGrid } from '../components/PhotoGrid';
import { LoadableImage } from '../components/Media';
import { burstGroups, burstPickOf, defaultFilters, frameStars, parseAttributes, filename } from '../lib/review';
import { StarPill } from '../components/Stars';
import { BurstFilmstrip } from '../components/BurstFilmstrip';
import { asset, call, engine } from '../lib/api';
import { useRun } from '../state/hooks';
import { useToast } from '../state/toast';
import { MenuDropdown, type MenuItem } from '../components/MenuDropdown';
import { ShootSettingsModal } from '../components/ShootSettingsModal';
import type { Frame, Job, KnownVehicle, MarkValues, Settings, Status, Task } from '../lib/types';
import { profileLabel, profileParent } from '../lib/types';
import type { ReviewModel } from '../state/useReview';
import { subjectExcluded } from '../review.mjs';
import { Viewer } from './Viewer';
import { Compare } from './Compare';

/** G/E/C hotkeys switch between the grid, the loupe (Viewer) and the compare view. */
export type ReviewView = 'grid' | 'loupe' | 'compare';
/** How App's global hotkey handler reaches into whichever view Review currently owns (mirrors Train's `verbs` ref). */
export type ReviewVerbs = { view: ReviewView; setView: (v: ReviewView) => void; stepFrame: (delta: number) => void; stepBurst: (delta: number) => void; mark: (values: MarkValues) => void; rejected: boolean };
export const noReviewVerbs: ReviewVerbs = { view: 'grid', setView: () => {}, stepFrame: () => {}, stepBurst: () => {}, mark: () => {}, rejected: false };

type Stack = { key: string; frames: Frame[]; pick: Frame; title: string; subtitle: string; photoWord?: string };

/** Shared renderer for grouped-photo grids (vehicle identity or burst grouping): identical markup either way. */
function StackGrid({ stacks, rv, onOpenViewer, ariaLabel, emptyTitle }: {
  stacks: Stack[]; rv: ReviewModel; onOpenViewer: () => void; ariaLabel: string; emptyTitle: string;
}) {
  const [expanded, setExpanded] = useState<string | null>(null);
  return <div className="burst-grid" role="list" aria-label={ariaLabel}>
    {stacks.map((group) => {
      const open = expanded === group.key;
      const isSelected = group.frames.some((f) => f.id === rv.selected);
      return <Fragment key={group.key}>
        <article className={`burst-stack${open ? ' open' : ''}${isSelected ? ' selected' : ''}`} onClick={() => { rv.setSelected(group.pick.id); setExpanded(open ? null : group.key); }}>
          <div className="burst-layers" style={{ '--layers': Math.min(group.frames.length, 4) } as CSSProperties}>
            <LoadableImage src={asset(group.pick.thumb_path)} alt={group.title} />
            <StarPill frame={group.pick} />
            <span className="stack-count">{group.frames.length}</span>
            <span className="focus keeper">Best</span>
          </div>
          <div className="burst-caption">
            <b>{group.title}</b>
            <span>{group.subtitle}</span>
          </div>
        </article>
        {open && <div className="burst-expanded">
          <div className="burst-expanded-head">
            <b>{group.title} · {group.frames.length} {group.photoWord ?? 'frames'}</b>
            <span className="muted" style={{ marginLeft: 8 }}>· Best photo first</span>
          </div>
          <BurstFilmstrip
            frames={group.frames}
            pickId={group.pick.id}
            selectedId={rv.selected}
            culled={(frame) => Boolean(rv.facts.get(frame.id)?.cull)}
            onSelect={rv.setSelected}
            onOpen={onOpenViewer}
          />
        </div>}
      </Fragment>;
    })}
    {!stacks.length && <Empty icon="images" title={emptyTitle} />}
  </div>;
}

function VehicleStacks({ rv, onOpenViewer, identifying }: { rv: ReviewModel; onOpenViewer: () => void; identifying: boolean }) {
  const visible = useMemo(() => new Set(rv.frames.map((frame) => frame.id)), [rv.frames]);
  const stacks = useMemo(() => {
    const groups = new Map<string, typeof rv.review.detections>();
    for (const detection of rv.review.detections.filter((d) => (d.region_type ?? 'vehicle') === 'vehicle')) {
      const attrs = parseAttributes(detection);
      const identity = attrs.plate || (attrs.race_number && `${attrs.race_number}-${attrs.make ?? ''}-${attrs.model ?? ''}`);
      const frame = rv.review.frames.find((item) => item.id === detection.image_id);
      const key = detection.group_key != null
        ? `group-${detection.group_key}`
        : identity
        ? `identity-${identity}`
        : identifying && frame?.burst_key != null
        ? `processing-burst-${frame.burst_key}`
        : `single-${detection.id}`;
      const list = groups.get(key); if (list) list.push(detection); else groups.set(key, [detection]);
    }
    return [...groups.entries()].map(([key, detections]) => {
      const frames = [...new Map(detections.map((d) => [d.image_id, rv.review.frames.find((frame) => frame.id === d.image_id)])).values()]
        .filter((frame): frame is Frame => Boolean(frame));
      const pick = frames.find((frame) => rv.facts.get(frame.id)?.pick) ?? [...frames].sort((a, b) => frameStars(b) - frameStars(a))[0];
      const attrs = parseAttributes(detections[0]);
      const processing = identifying && key.startsWith('processing-');
      const title = processing ? 'Being identified' : [attrs.make, attrs.model].filter(Boolean).join(' ') || (attrs.plate || (attrs.race_number && `#${attrs.race_number}`) || 'Unidentified vehicle');
      const subtitle = processing
        ? `Analysing ${frames.length} photos`
        : `${frames.length} shot${frames.length === 1 ? '' : 's'}${attrs.plate ? ` · ${attrs.plate}` : ''}${attrs.race_number ? ` · #${attrs.race_number}` : ''}${attrs.team ? ` · ${attrs.team}` : ''}`;
      return { key, frames, pick, title, subtitle, photoWord: 'photos' as const };
    }).filter((group): group is typeof group & { pick: Frame } => Boolean(group.pick) && group.frames.some((f) => visible.has(f.id)))
      .sort((a, b) => a.pick.id - b.pick.id);
  }, [rv.review.detections, rv.review.frames, rv.facts, identifying, visible]);

  return <StackGrid stacks={stacks} rv={rv} onOpenViewer={onOpenViewer} ariaLabel="Vehicle stacks" emptyTitle="No vehicles match these filters." />;
}

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
          <span className="pipeline-dot">{step.done ? '✓' : i + 1}</span>
          <span className="pipeline-label">{step.label}</span>
        </span>
      ))}
    </nav>
  );
}

/** Burst stacks, or with `dupes` the shoot-wide re-shoot stacks (`dupe_key`, 2+ frames only). */
function BurstStacks({ rv, onOpenViewer, dupes = false }: { rv: ReviewModel; onOpenViewer: () => void; dupes?: boolean }) {
  const visibleIds = useMemo(() => new Set(rv.frames.map((f) => f.id)), [rv.frames]);
  const bursts = useMemo(() => {
    const map = new Map<string, Frame[]>();
    for (const frame of rv.review.frames.filter((item) => item.status === 'done')) {
      const group = dupes ? frame.dupe_key : frame.burst_key;
      const key = group == null ? `frame-${frame.id}` : `${dupes ? 'dupe' : 'burst'}-${group}`;
      const list = map.get(key); if (list) list.push(frame); else map.set(key, [frame]);
    }
    const scoreOf = (f: Frame) => {
      // Manual stars outrank everything (50 max)
      const stars = (f.manual_stars ?? 0) * 10;
      // Detector burst pick bonus
      const isPick = (rv.facts.get(f.id)?.pick || Boolean(f.burst_pick)) ? 2.0 : 0.0;
      // Measured rating
      const rating = (f.rating ?? 0) * 5.0;
      return stars + isPick + rating;
    };
    return [...map]
      .filter(([, rawFrames]) => rawFrames.some((f) => visibleIds.has(f.id)) && (!dupes || rawFrames.length > 1))
      .map(([key, rawFrames]) => {
        const sortedByBest = [...rawFrames].sort((a, b) => scoreOf(b) - scoreOf(a));
        const pick = sortedByBest.find((f) => !rv.facts.get(f.id)?.cull) ?? sortedByBest[0];
        const remaining = rawFrames.filter((f) => f.id !== pick.id).sort((a, b) => a.id - b.id);
        const frames = [pick, ...remaining];
        const facts = rv.facts.get(pick.id);
        const title = facts?.vehicle || facts?.plate || (facts?.number && `#${facts.number}`) || filename(pick.path);
        const subtitle = `${frames.length} shot${frames.length === 1 ? '' : 's'}${facts?.plate ? ` · ${facts.plate}` : ''}${facts?.number ? ` · #${facts.number}` : ''}${facts?.team ? ` · ${facts.team}` : ''}`;
        return { key, frames, pick, title, subtitle };
      })
      .sort((a, b) => a.pick.id - b.pick.id);
  }, [rv.review.frames, rv.facts, visibleIds, dupes]);

  return <StackGrid stacks={bursts} rv={rv} onOpenViewer={onOpenViewer} ariaLabel={dupes ? 'Duplicate stacks' : 'Burst stacks'}
    emptyTitle={dupes ? 'No re-shot frames found across this shoot.' : 'No burst stacks match these filters.'} />;
}

type Props = {
  rv: ReviewModel; jobs: Job[]; jobId: number | null; scanning: boolean; status: Status; settings: Settings;
  showBoxes: boolean; onToggleBoxes: () => void; verbs: MutableRefObject<ReviewVerbs>;
  onPickJob: (id: number) => void; onIdentify: () => void; onCull: () => void;
  onHelp: () => void; onOpenViewer: () => void; onLibrary: () => void;
  onEdit: (detectionId: number, updates: Record<string, string | string[] | null>) => unknown;
  onAcceptAll: () => void; onSkipIdentify: () => void; onWrite: (dryRun: boolean) => void;
};

/** Toolbar (album, view, search, stars, sort, actions), facets, the card grid and the inspector. */
export function ReviewScreen({ rv, jobs, jobId, scanning, status, settings, showBoxes, onToggleBoxes, verbs, onPickJob, onIdentify, onCull, onHelp, onOpenViewer, onLibrary, onEdit, onAcceptAll, onSkipIdentify, onWrite }: Props) {
  const [known, setKnown] = useState<KnownVehicle[]>([]);
  const [grouping, setGrouping] = useState<'bursts' | 'vehicles' | 'dupes' | 'flat'>('bursts');
  const [confirmReset, setConfirmReset] = useState<'reset_ratings' | 'reset_identifications' | 'reset_detections' | null>(null);
  const [shootSettingsOpen, setShootSettingsOpen] = useState(false);
  const run = useRun();
  const toast = useToast();
  useEffect(() => { void engine.known().then(setKnown); }, [jobId]);
  const { filters, patchFilters, frames, review, facts, selected, frame } = rv;

  // 1.3: which of grid / loupe (Viewer) / compare is showing; G/E/C reach this through `verbs`.
  const [view, setView] = useState<ReviewView>('grid');
  const [comparePane, setComparePane] = useState<'left' | 'right'>('left');
  const [compareRightId, setCompareRightId] = useState<number | null>(null);
  const compareRight = useMemo(() => {
    if (!frame) return null;
    if (compareRightId != null) return frames.find((f) => f.id === compareRightId) ?? null;
    // Default right pane: the current burst's pick if this frame isn't it, else the next frame.
    const burstPick = frame.burst_key != null
      ? frames.find((f) => f.burst_key === frame.burst_key && f.id !== frame.id && (facts.get(f.id)?.pick || Boolean(f.burst_pick)))
      : undefined;
    if (burstPick) return burstPick;
    const index = frames.findIndex((f) => f.id === frame.id);
    return index >= 0 && index < frames.length - 1 ? frames[index + 1] : null;
  }, [frame, compareRightId, frames, facts]);
  useEffect(() => { setCompareRightId(null); setComparePane('left'); }, [frame?.id]);
  const activeCompareFrame = comparePane === 'left' ? frame : compareRight;

  // 1.4: the burst groups behind the loupe/compare filmstrip and the up/down (across bursts) vs. left/right (within
  // a burst, clamped at its edges) hotkeys. Grid view keeps the plain flat-list step it always had.
  const groups = useMemo(() => burstGroups(frames), [frames]);
  const withinBurst = (base: Frame | null, delta: number): Frame | null => {
    if (!base) return null;
    const group = groups.find((g) => g.some((f) => f.id === base.id));
    if (!group) return base;
    const index = group.findIndex((f) => f.id === base.id);
    return group[Math.max(0, Math.min(group.length - 1, index + delta))] ?? base;
  };
  const acrossBursts = (base: Frame | null, delta: number): Frame | null => {
    if (!base) return null;
    const index = groups.findIndex((g) => g.some((f) => f.id === base.id));
    if (index < 0) return base;
    const group = groups[Math.max(0, Math.min(groups.length - 1, index + delta))];
    return burstPickOf(group, facts);
  };

  verbs.current = {
    view,
    setView,
    stepFrame: (delta) => {
      if (view === 'grid') { rv.step(delta); return; }
      if (view !== 'compare') { const next = withinBurst(frame, delta); if (next) rv.setSelected(next.id); return; }
      const next = withinBurst(compareRight ?? frame, delta);
      if (next) setCompareRightId(next.id);
    },
    stepBurst: (delta) => {
      if (view === 'grid') { rv.step(delta); return; }
      if (view !== 'compare') { const next = acrossBursts(frame, delta); if (next) rv.setSelected(next.id); return; }
      const next = acrossBursts(compareRight ?? frame, delta);
      if (next) setCompareRightId(next.id);
    },
    mark: (values) => void rv.mark(values, view === 'compare' ? activeCompareFrame?.id : undefined),
    rejected: Boolean(activeCompareFrame && facts.get(activeCompareFrame.id)?.cull),
  };
  const openViewer = () => { setView('loupe'); onOpenViewer(); };
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
  const filtered = filters.search !== '' || filters.facet !== null;
  const identifyTask = status.tasks.find((task) => (task.state === 'running' || task.state === 'paused') && /^Identifying\b/.test(task.label) && task.label.includes(`album ${jobId}`));
  const identifying = Boolean(identifyTask || status.operations.some((key) => key === `Identifying:${jobId}`));
  const excluded = review.frames.filter((f) => facts.get(f.id)?.cull).length;
  const pending = review.frames.filter((f) => !facts.get(f.id)?.cull && f.status !== 'done').length;
  const eligible = review.frames.filter((f) => (facts.get(f.id)?.dets ?? []).some((d) => !d.reviewed && !subjectExcluded(f, d))).length;
  const busy = scanning || status.operations.some((key) => key.endsWith(`:${jobId}`));
  const currentJob = jobs.find((j) => j.id === jobId);
  const currentProfile = String(currentJob?.scan_profile || settings.scan_profile || 'motorsport');
  const currentParent = profileParent(currentProfile);
  const kind = currentParent === 'portrait' ? 'portrait'
    : currentParent === 'motorsport' ? (currentJob?.include_people ? 'motorsport+people' : 'motorsport')
    : 'mixed';

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
        onClick: () => onWrite(false),
        title: 'Write ratings, tags, and vehicle metadata to XMP sidecars',
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
      id: 'accept_all',
      label: 'Accept all identifications',
      hint: 'Mark all unreviewed detections in this shoot as accepted',
      disabled: !review.detections.some((d) => !d.reviewed),
      onClick: onAcceptAll,
    },
    {
      id: 'write',
      label: 'Write XMP',
      hint: 'Write ratings, stars, and tags directly to photo sidecars',
      onClick: () => onWrite(false),
    },
    {
      id: 'dry_run',
      label: 'Dry run (preview XMP)',
      hint: 'Simulate XMP write without touching files on disk',
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
      checked: settings.read_numbers !== false,
      onChange: (checked) => {
        void run(async () => {
          await engine.updateJobSettings(jobId, { read_numbers: checked });
          await call('save_settings', { ...settings, read_numbers: checked });
          await rv.refresh();
          toast(`Race number detection turned ${checked ? 'on' : 'off'}.`);
        });
      },
    },
    {
      type: 'checkbox',
      id: 'toggle_plates',
      label: 'Read licence plates',
      hint: 'Detect vehicle licence and registration plates',
      checked: settings.read_plates !== false,
      onChange: (checked) => {
        void run(async () => {
          await engine.updateJobSettings(jobId, { read_plates: checked });
          await call('save_settings', { ...settings, read_plates: checked });
          await rv.refresh();
          toast(`Licence plate detection turned ${checked ? 'on' : 'off'}.`);
        });
      },
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
            {profileLabel(currentProfile)} ⚙
          </button>
        )}
        <input id="search" type="search" aria-label="Search photos" placeholder="Search make, team, plate…" spellCheck={false}
          value={filters.search} onChange={(e) => patchFilters({ search: e.target.value })} />
        <select aria-label="Photo filter" value={filters.view} onChange={(e) => patchFilters({ view: e.target.value as typeof filters.view })}>
          <option value="all">All photos</option><option value="kept">Kept photos</option><option value="rejected">Culled / rejected</option><option value="picks">Burst picks</option><option value="review">Needs review</option><option value="needsLook">Needs a look</option>
        </select>
        <select aria-label="Group photos" value={grouping} onChange={(e) => setGrouping(e.target.value as typeof grouping)}>
          <option value="bursts">Group: Burst stacks</option><option value="vehicles">Group: By vehicle</option><option value="dupes">Group: Duplicates</option><option value="flat">Group: Individual photos</option>
        </select>
        <select aria-label="Photo order" value={filters.sort} onChange={(e) => patchFilters({ sort: e.target.value as typeof filters.sort })}>
          <option value="frame">Filename order</option><option value="best">Highest rated first</option><option value="worst">Lowest rated first</option><option value="pick">Burst picks first</option>
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
        initialSettings={settings}
        visible={shootSettingsOpen}
        onClose={() => setShootSettingsOpen(false)}
        onSave={async (patch) => {
          await engine.updateJobSettings(jobId, patch);
          await call('save_settings', {
            ...settings,
            scan_profile: patch.scan_profile,
            read_numbers: patch.read_numbers,
            read_plates: patch.read_plates,
            group_vehicles: patch.group_vehicles,
          });
          await rv.refresh();
          toast('Shoot settings updated.');
        }}
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
      {identifyTask && <IdentificationBanner task={identifyTask} settings={settings} onSkip={onSkipIdentify} />}
      <div className="review-main">
        <Facets facets={rv.facets} facet={filters.facet} onPick={(facet) => patchFilters({ facet })} />
        {grouping === 'dupes' ? <BurstStacks rv={rv} onOpenViewer={openViewer} dupes />
          : grouping === 'bursts' || filters.view === 'stacks'
          ? <BurstStacks rv={rv} onOpenViewer={openViewer} />
          : grouping === 'vehicles' || filters.view === 'vehicles' ? <VehicleStacks rv={rv} onOpenViewer={openViewer} identifying={identifying} />
          : <PhotoGrid frames={frames} facts={facts} selected={selected} onSelect={rv.setSelected} onOpen={openViewer}
              onReject={(id, rejected) => void rv.mark({ rejected }, id)}
              empty={
                <Empty icon="images" title={review.frames.length ? 'Nothing matches these filters.' : scanning ? 'Waiting for the first frames…' : 'No photographs to show yet.'}>
                  {filtered && <button className="ghost" onClick={() => patchFilters({ ...defaultFilters })}>Clear filters</button>}
                </Empty>
              } />}
        <Inspector frame={frame} detections={frame ? facts.get(frame.id)?.dets ?? [] : []} allDetections={review.detections} allFrames={review.frames} known={known} scanning={scanning} kind={kind}
          onMark={(values) => void rv.mark(values)} onOpen={openViewer} onEdit={onEdit} />
      </div>
      {view === 'loupe' && frame && (
        <Viewer rv={rv} run={run} showBoxes={showBoxes} onToggleBoxes={onToggleBoxes} onClose={() => setView('grid')} />
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
        />
      )}
    </div>
  );
}
