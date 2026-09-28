import { LoadableImage } from './Media';
import { StarPill } from './Stars';
import { asset } from '../lib/api';
import { filename } from '../lib/review';
import type { Frame } from '../lib/types';

/** The `burst-strip` markup shared with `StackGrid`'s expanded card: a horizontal scroller of one burst's frames. */
export function BurstFilmstrip({ frames, pickId, selectedId, culled, onSelect, onOpen }: {
  frames: Frame[]; pickId: number; selectedId: number | null;
  culled: (frame: Frame) => boolean; onSelect: (id: number) => void; onOpen?: () => void;
}) {
  return (
    <div className="burst-strip">
      {frames.map((frame) => {
        const isBest = frame.id === pickId;
        const isCulled = culled(frame);
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
              {isBest && <span className="focus keeper" style={{ position: 'absolute', top: 4, left: 4, zIndex: 2 }}>Best</span>}
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
