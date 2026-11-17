import { useEffect, useRef } from 'react';
import { LoadableImage } from './Media';
import { KeeperMark, RankBadge, StarPill } from './Stars';
import { DecisionPill } from './DecisionPill';
import { asset } from '../lib/api';
import { compositionLabels, panComparison, relativeCleaner, stackClutter } from '../lib/composition.mjs';
import { filename } from '../lib/review';
import type { Frame, MarkValues } from '../lib/types';

/** The `burst-strip` markup shared with `StackGrid`'s expanded card: a horizontal scroller of one burst's frames, in shooting order. */
export function BurstFilmstrip({ frames, keeperIds, selectedId, culled, rankOf, composition, onSelect, onOpen, onDecide }: {
  frames: Frame[]; keeperIds: ReadonlySet<number>; selectedId: number | null;
  culled: (frame: Frame) => boolean; rankOf?: (frame: Frame) => number | undefined;
  /** Show each frame's background against the stack's own median (the expanded stack strip does; the loupe and compare strips are compact). */
  composition?: boolean;
  onSelect: (id: number) => void; onOpen?: () => void;
  /** Keep or cull one frame from its pill; the pill stops the click, so it never selects or opens the frame. */
  onDecide?: (id: number, values: MarkValues) => void;
}) {
  const strip = useRef<HTMLDivElement>(null);
  // One baseline per stack: an absolute clutter threshold says "busy" on a crowded track for every frame.
  const clutter = composition ? stackClutter(frames) : null;
  const cleanestId = clutter && clutter.cleanestIds.length ? panComparison(frames)?.id ?? null : null;
  // Keep the selected frame in view along the strip (only this strip scrolls, never the page around it).
  useEffect(() => {
    const el = strip.current, item = el?.querySelector<HTMLElement>('.burst-strip-item.selected');
    if (!el || !item) return;
    if (item.offsetLeft < el.scrollLeft) el.scrollLeft = item.offsetLeft - 4;
    else if (item.offsetLeft + item.offsetWidth > el.scrollLeft + el.clientWidth) el.scrollLeft = item.offsetLeft + item.offsetWidth - el.clientWidth + 4;
  }, [selectedId]);
  return (
    <div className="burst-strip" ref={strip}>
      {frames.map((frame) => {
        const isBest = keeperIds.has(frame.id);
        const isCulled = culled(frame);
        const rank = rankOf?.(frame);
        const selected = selectedId === frame.id;
        // The strip never repeats the inspector's absolute "Busy"/"Clean"; it compares frames against their own stack.
        const labels = composition ? compositionLabels(frame).filter((l) => l.id !== 'clutter').slice(0, 3) : [];
        const cleanest = composition && cleanestId === frame.id;
        const cleaner = composition && !cleanest && relativeCleaner(clutter?.median, frame.comp_clutter);
        // The pill is a sibling of the button: a button cannot nest another button, and the click never reaches the card.
        return (
          <div key={frame.id} className={`burst-strip-item${selected ? ' selected' : ''}`}>
            <button
              className={`${selected ? 'selected' : ''}${isBest ? ' burst-best-card' : ''}`}
              style={isCulled ? { opacity: 0.6 } : undefined}
              onClick={() => onSelect(frame.id)}
              onDoubleClick={onOpen}
            >
              <div style={{ position: 'relative' }}>
                <LoadableImage src={asset(frame.thumb_path)} alt={filename(frame.path)} />
                {isBest && <KeeperMark label={frame.picked ? 'Keeper, picked by you' : 'Keeper, the engine\u2019s best of the burst'} />}
                {rank != null && !isBest && <RankBadge rank={rank} />}
                {isCulled && <span className="cull-tag">Culled</span>}
              </div>
              <StarPill frame={frame} />
              <small>{filename(frame.path)}</small>
              {(cleanest || cleaner || labels.length > 0) && (
                <span className="comp-labels">
                  {cleanest && <span className="tag cleanest" title="Calmest background of the frames measured in this stack">Cleanest background</span>}
                  {cleaner && <span className="tag" title="At least a tenth calmer than this stack's median background">Cleaner background</span>}
                  {labels.map((label) => <span key={label.id} className={label.tone === 'ok' ? 'tag' : `tag ${label.tone}`}>{label.text}</span>)}
                </span>
              )}
            </button>
            <DecisionPill frame={frame} onDecide={(values) => onDecide?.(frame.id, values)} />
          </div>
        );
      })}
    </div>
  );
}
