import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { call, engine } from '../lib/api';
import { buildFacets, buildFacts, defaultFilters, visibleFrames, type Filters } from '../lib/review';
import type { Frame, MarkValues, Review } from '../lib/types';
import type { Runner } from './hooks';

const emptyReview: Review = { frames: [], detections: [] };

/** One album's frames and detections, the selection inside them, and the toolbar that narrows them. */
export function useReview(jobId: number | null, run: Runner) {
  const [review, setReview] = useState<Review>(emptyReview);
  const [selected, setSelected] = useState<number | null>(null);
  const [filters, setFilters] = useState<Filters>(defaultFilters);
  const current = useRef(jobId);
  useEffect(() => { current.current = jobId; });

  const refresh = useCallback(async () => {
    if (jobId == null) return;
    const r = await engine.review(jobId);
    if (current.current !== jobId) return; // the album changed while this was in flight
    setReview(r);
    setSelected((s) => (r.frames.some((f) => f.id === s) ? s : null));
  }, [jobId]);

  useEffect(() => {
    setReview(emptyReview); setSelected(null); setFilters(defaultFilters);
    if (jobId != null) void run(async () => { await refresh(); await call('filling', {jobId}); });
  }, [jobId, refresh, run]);

  const facts = useMemo(() => buildFacts(review), [review]);
  const facets = useMemo(() => buildFacets(review.frames, facts), [review.frames, facts]);
  const frames = useMemo(() => visibleFrames(review.frames, facts, filters), [review.frames, facts, filters]);
  const frame: Frame | null = useMemo(() => review.frames.find((f) => f.id === selected) ?? null, [review.frames, selected]);
  const patchFilters = useCallback((patch: Partial<Filters>) => setFilters((f) => ({ ...f, ...patch })), []);

  useEffect(() => {
    setSelected((currentId) => frames.some((item) => item.id === currentId) ? currentId : frames[0]?.id ?? null);
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

  const mark = useCallback((values: MarkValues, id: number | null = selected) => {
    if (id == null) return Promise.resolve(undefined);
    return run(async () => {
      await engine.mark(id, values);
      if (values.rejected && id === selected) {
        const kept = frames.filter((f) => f.id !== id && !facts.get(f.id)?.cull);
        const index = frames.findIndex((f) => f.id === id);
        setSelected(frames.slice(index + 1).find((f) => f.id !== id && !facts.get(f.id)?.cull)?.id ?? kept.at(-1)?.id ?? id);
      }
      setReview((r) => ({
        ...r,
        detections: r.detections.map((d) => d.image_id !== id ? d : {
          ...d,
          ...('stars' in values ? { stars: values.stars ?? null } : {}),
          ...('rejected' in values ? { rejected: values.rejected ? 1 : 0, ...(!values.rejected ? { cull_reason: null } : {}) } : {}),
        }),
        frames: r.frames.map((f) => (f.id !== id ? f : {
          ...f,
          ...('stars' in values ? { manual_stars: values.stars ?? null } : {}),
          ...('rejected' in values ? { rejected: values.rejected ? 1 : 0 } : {}),
        })),
      }));
    });
  }, [selected, run, frames, facts]);

  return { review, refresh, selected, setSelected, filters, patchFilters, facts, facets, frames, frame, step, mark };
}
export type ReviewModel = ReturnType<typeof useReview>;
