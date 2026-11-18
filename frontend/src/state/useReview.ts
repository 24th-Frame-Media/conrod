import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { call, engine } from '../lib/api';
import { buildFacets, buildFacts, defaultFilters, keeperIds, visibleFrames, type Filters } from '../lib/review';
import { defaultPrefs, parsePrefs, prefsKey, type Grouping } from '../lib/album.mjs';
import { nearestVisible } from '../lib/stacks.mjs';
import { capture, record, redo as redoHistory, undo as undoHistory, type UndoEntry } from '../lib/undo.mjs';
import type { Frame, MarkValues, Review } from '../lib/types';
import type { Runner } from './hooks';

const emptyReview: Review = { frames: [], detections: [] };

/** One frame's local values after a mark, without waiting for a reload. */
const patchReview = (r: Review, id: number, values: MarkValues): Review => ({
  ...r,
  detections: r.detections.map((d) => d.image_id !== id ? d : {
    ...d,
    ...('stars' in values ? { stars: values.stars ?? null } : {}),
    ...('rejected' in values ? { rejected: values.rejected ? 1 : 0, ...(!values.rejected ? { cull_reason: null } : {}) } : {}),
  }),
  frames: r.frames.map((f) => (f.id !== id ? f : {
    ...f,
    ...('stars' in values ? { manual_stars: values.stars ?? null } : {}),
    ...('rejected' in values ? { rejected: values.rejected ? 1 : 0, ...(values.rejected ? { picked: 0 } : {}) } : {}),
    ...('picked' in values ? { picked: values.picked ? 1 : 0, ...(values.picked ? { rejected: 0 } : {}) } : {}),
  })),
});

const loadPrefs = (jobId: number | null) => {
  if (jobId == null) return defaultPrefs;
  try { return parsePrefs(localStorage.getItem(prefsKey(jobId))); } catch { return defaultPrefs; }
};
const filtersFor = (jobId: number | null): Filters => {
  const { view, sort, minStars } = loadPrefs(jobId);
  return { ...defaultFilters, view, sort, minStars };
};

/**
 * One album's frames and detections, the selection inside them, and the toolbar that narrows them.
 * Grouping, sort and filter are remembered per album, so a trip to the library and back changes nothing.
 */
export function useReview(jobId: number | null, run: Runner) {
  const [review, setReview] = useState<Review>(emptyReview);
  /** True while the album's first payload is in flight, so an empty grid can say it is loading, not empty. */
  const [loading, setLoading] = useState(jobId != null);
  const [selected, setSelected] = useState<number | null>(null);
  const [filters, setFilters] = useState<Filters>(() => filtersFor(jobId));
  const [grouping, setGrouping] = useState<Grouping>(() => loadPrefs(jobId).grouping);
  /** The frame a stack was opened on; the open stack is whichever one holds it now, so regrouping cannot close it. */
  const [expanded, setExpanded] = useState<number | null>(null);
  const [viewJob, setViewJob] = useState(jobId);
  const current = useRef(jobId);
  useEffect(() => { current.current = jobId; });

  if (viewJob !== jobId) {
    setViewJob(jobId);
    setFilters(filtersFor(jobId));
    setGrouping(loadPrefs(jobId).grouping);
    setReview(emptyReview); setSelected(null); setExpanded(null); setLoading(jobId != null);
  }

  useEffect(() => {
    if (jobId == null) return;
    try { localStorage.setItem(prefsKey(jobId), JSON.stringify({ grouping, view: filters.view, sort: filters.sort, minStars: filters.minStars })); } catch { /* not persisted */ }
  }, [jobId, grouping, filters.view, filters.sort, filters.minStars]);

  const refresh = useCallback(async () => {
    if (jobId == null) return;
    setLoading(true);
    try {
      const r = await engine.review(jobId);
      if (current.current !== jobId) return; // the album changed while this was in flight
      setReview(r);
    } finally {
      if (current.current === jobId) setLoading(false);
    }
  }, [jobId]);

  useEffect(() => {
    if (jobId != null) void run(async () => { await refresh(); await call('filling', {jobId}); });
  }, [jobId, refresh, run]);

  const facts = useMemo(() => buildFacts(review), [review]);
  const facets = useMemo(() => buildFacets(review.frames, facts), [review.frames, facts]);
  /** Every burst's keeper: manual picks if any, else the engine's pick. Always over the whole album. */
  const keepers = useMemo(() => keeperIds(review.frames, facts), [review.frames, facts]);
  const frames = useMemo(() => visibleFrames(review.frames, facts, filters, keepers), [review.frames, facts, filters, keepers]);
  const frame: Frame | null = useMemo(() => review.frames.find((f) => f.id === selected) ?? null, [review.frames, selected]);
  const patchFilters = useCallback((patch: Partial<Filters>) => setFilters((f) => ({ ...f, ...patch })), []);

  // A filter that hides the selection moves it to the nearest frame still on screen, never back to the first frame.
  const previous = useRef<number[]>([]);
  useEffect(() => {
    const visible = new Set(frames.map((f) => f.id));
    const before = previous.current;
    previous.current = frames.map((f) => f.id);
    setSelected((id) => {
      if (id != null && visible.has(id)) return id;
      if (id != null) return nearestVisible(before, visible, id) ?? frames[0]?.id ?? null;
      return frames[0]?.id ?? null;
    });
  }, [frames]);

  const step = useCallback((delta: number) => {
    setSelected((currentId) => {
      if (!frames.length) return null;
      const currentIndex = frames.findIndex((f) => f.id === currentId);
      const origin = currentIndex < 0 ? (delta < 0 ? frames.length : -1) : currentIndex;
      const index = Math.max(0, Math.min(frames.length - 1, origin + delta));
      return frames[index]?.id ?? currentId;
    });
  }, [frames]);

  /** The changes this visit can step back through, newest last; a new mark clears the redo side. */
  const past = useRef<UndoEntry[]>([]);
  const future = useRef<UndoEntry[]>([]);
  /** One engine call per frame, then one local patch for the whole list: a burst decision costs n marks, not n reloads. */
  const applyMany = useCallback((changes: { id: number; values: MarkValues }[]) => run(async () => {
    for (const change of changes) await engine.mark(change.id, change.values);
    setReview((r) => changes.reduce((next, change) => patchReview(next, change.id, change.values), r));
  }), [run]);

  /**
   * One undo step for any number of frames. The previous state of each frame is captured here, because the engine's
   * per-photo `mark` has no batch endpoint; undo replays every captured restore in one go.
   */
  const markMany = useCallback((changes: { id: number; values: MarkValues }[]) => {
    if (!changes.length) return Promise.resolve(undefined);
    const entries = changes.map((change) => ({
      id: change.id,
      restore: capture(review.frames.find((f) => f.id === change.id) ?? null, change.values),
      after: change.values,
    }));
    past.current = record(past.current, { id: entries[0].id, changes: entries });
    future.current = [];
    return applyMany(changes);
  }, [review.frames, applyMany]);

  const mark = useCallback((values: MarkValues, id: number | null = selected) => {
    if (id == null) return Promise.resolve(undefined);
    return markMany([{ id, values }]);
  }, [selected, markMany]);

  /** Undo one decision (a single mark or a whole stack action); returns the frame the view can go back to. */
  const undo = useCallback((): number | null => {
    const step = undoHistory(past.current, future.current);
    if (!step) return null;
    ({ past: past.current, future: future.current } = step);
    void applyMany(step.entry.changes.map((change) => ({ id: change.id, values: change.restore })));
    return step.entry.id;
  }, [applyMany]);

  const redo = useCallback((): number | null => {
    const step = redoHistory(past.current, future.current);
    if (!step) return null;
    ({ past: past.current, future: future.current } = step);
    void applyMany(step.entry.changes.map((change) => ({ id: change.id, values: change.after })));
    return step.entry.id;
  }, [applyMany]);

  return { review, refresh, loading, selected, setSelected, filters, patchFilters, grouping, setGrouping, expanded, setExpanded, facts, facets, keepers, frames, frame, step, mark, markMany, undo, redo };
}
export type ReviewModel = ReturnType<typeof useReview>;
