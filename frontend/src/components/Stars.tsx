import { frameStars, verdictOf } from '../lib/review';
import type { Frame } from '../lib/types';

/** The small pill over a photograph: the stars it has, tinted by verdict; accent-outlined once given by hand. */
export function StarPill({ frame }: { frame: Frame }) {
  const stars = frameStars(frame);
  const byHand = frame.manual_stars != null;
  const title = byHand ? `${stars} stars, given by you. This is what Write XMP will use` : stars ? `${stars} stars, measured on the subject` : 'Not rated yet';
  return <span className={`focus stars ${verdictOf(stars)}${byHand ? ' by-hand' : ''}`} title={title}>{stars ? '★'.repeat(stars) : '—'}</span>;
}

/** The china-pencil keeper tick: the burst's best frame, in amber. Used for "Best" and nowhere else. */
export function KeeperMark({ inline = false, label = 'Best of the burst' }: { inline?: boolean; label?: string }) {
  return (
    <span className={`keeper-mark${inline ? ' inline' : ''}`} role="img" aria-label={label} title={label}>
      <svg width="14" height="14" viewBox="0 0 14 14" fill="none" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M2.2 7.7C3.4 8.3 4.4 9.5 5.2 11C6.8 7.4 9 4.7 12 2.7" stroke="currentColor" strokeWidth="1.8" />
        <path d="M4.6 9.6C6.2 6.9 8.3 4.6 10.8 3.2" stroke="currentColor" strokeWidth=".8" opacity=".55" />
      </svg>
    </span>
  );
}

/** Marks a runner-up in its burst (#2, #3); the burst's best frame carries the keeper mark instead. Quiet mono text, same corner style as the stars. */
export function RankBadge({ rank }: { rank: number }) {
  return <span className="focus rank" title={`Burst candidate #${rank}`} aria-label={`Burst candidate number ${rank}`}>#{rank}</span>;
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
