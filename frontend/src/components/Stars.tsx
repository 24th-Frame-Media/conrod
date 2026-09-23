import { frameStars, verdictOf } from '../lib/review';
import type { Frame } from '../lib/types';

/** The small pill over a photograph: the stars it has, tinted by verdict; accent-outlined once given by hand. */
export function StarPill({ frame }: { frame: Frame }) {
  const stars = frameStars(frame);
  const byHand = frame.manual_stars != null;
  const title = byHand ? `${stars} stars, given by you. This is what Write XMP will use` : stars ? `${stars} stars, measured on the subject` : 'Not rated yet';
  return <span className={`focus stars ${verdictOf(stars)}${byHand ? ' by-hand' : ''}`} title={title}>{stars ? '★'.repeat(stars) : '—'}</span>;
}

/** Five stars to click; the star that is already on clears back to the measured rating. */
export function StarControl({ value, onChange }: { value: number; onChange: (stars: number | null) => void }) {
  return (
    <div className="stars-control" role="group" aria-label="Star rating">
      {[1, 2, 3, 4, 5].map((n) => (
        <button key={n} className={`star${n <= value ? ' on' : ''}`} aria-label={`${n} star${n === 1 ? '' : 's'}`}
          aria-pressed={n === value} onClick={() => onChange(n)}>★</button>
      ))}
    </div>
  );
}

/** "3 stars and up" in one click. Clicking the active star clears the filter. */
export function StarFilter({ value, onChange }: { value: number; onChange: (n: number) => void }) {
  return (
    <div className="star-filter" role="group" aria-label="Filter by star rating">
      <button className={`star-filter-clear${value ? '' : ' on'}`} title="Show every rating" onClick={() => onChange(0)}>Any</button>
      {[1, 2, 3, 4, 5].map((n) => (
        <button key={n} className={`star${value && n <= value ? ' on' : ''}`} title={`${n} star${n === 1 ? '' : 's'} and up`}
          onClick={() => onChange(value === n ? 0 : n)}>★</button>
      ))}
    </div>
  );
}
