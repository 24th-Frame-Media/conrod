import { useEffect, useRef } from 'react';
import { LoadableImage } from './Media';
import { KeeperMark, RankBadge, StarPill } from './Stars';
import { asset } from '../lib/api';
import { filename } from '../lib/review';
import type { Frame } from '../lib/types';

/** The `burst-strip` markup shared with `StackGrid`'s expanded card: a horizontal scroller of one burst's frames, in shooting order. */
export function BurstFilmstrip({ frames, keeperIds, selectedId, culled, rankOf, onSelect, onOpen }: {
  frames: Frame[]; keeperIds: ReadonlySet<number>; selectedId: number | null;
  culled: (frame: Frame) => boolean; rankOf?: (frame: Frame) => number | undefined;
  onSelect: (id: number) => void; onOpen?: () => void;
}) {
  const strip = useRef<HTMLDivElement>(null);
  // Keep the selected frame in view along the strip (only this strip scrolls, never the page around it).
  useEffect(() => {
    const el = strip.current, item = el?.querySelector<HTMLElement>('button.selected');
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
        return (
          <button
            key={frame.id}
            className={`${selectedId === frame.id ? 'selected' : ''}${isBest ? ' burst-best-card' : ''}`}
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
          </button>
        );
      })}
    </div>
  );
}
