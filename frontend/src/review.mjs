export const filename = path => path.split(/[\\/]/).pop() || path;
/** The keeper floor, mirrored from `conrod_vision::sharpness::KEEPER_MIN_RATING` (three stars). */
export const KEEPER_MIN_RATING = .728;
/** The camera's focus area reads sharp enough to keep: its AF box is the subject. */
export const focusSharp = frame => Number.isFinite(frame.af_sharpness) && frame.af_sharpness >= KEEPER_MIN_RATING;
export const starsFor = rating => rating == null ? 0 : rating >= .958 ? 5 : rating >= .825 ? 4 : rating >= .728 ? 3 : rating >= .606 ? 2 : 1;
export const frameStars = frame => frame.manual_stars ?? starsFor(frame.rating);
export const subjectExcluded = (frame, d) => Boolean(frame.rejected || d.rejected || d.bystander || (d.cull_reason && d.stars == null && frame.manual_stars == null));
/** The frame's own fault (`images.flaw`): nothing in focus, or blown or crushed beyond recovery. A bracket member is never faulted for exposure, and a hand star outranks it. */
export const frameFlaw = frame => frame.flaw && frame.manual_stars == null && !(frame.set_id != null && frame.flaw !== 'blurred') ? frame.flaw : null;
/** One plain sentence for a frame's own fault. */
export const flawWords = flaw => flaw === 'blurred' ? 'Nothing in this frame is in focus.'
  : flaw === 'overexposed' ? 'Blown out: the highlights are clipped beyond recovery.'
  : flaw === 'underexposed' ? 'Too dark: the shadows are crushed beyond recovery.' : null;
export const frameExcluded = (frame, detections) => Boolean(frame.rejected || frameFlaw(frame) || (detections.length && detections.every(d => subjectExcluded(frame, d)) && !focusSharp(frame)));
// Burst candidates: manual stars first, then not rejected, then rating, then the engine's pick on ties.
// The engine's pick always takes rank 1; the rest of the top `top` follow the ordering.
// Rejected frames are not candidates unless the photographer rated them by hand; a missing or NaN rating sorts last.
const ratingOf = f => Number.isFinite(f.rating) ? f.rating : -1;
export function burstTopRanks(group, isPick, top = 3) {
  const eligible = group.filter(f => !f.rejected || f.manual_stars != null);
  const ordered = [...eligible].sort((a, b) =>
    Number(b.manual_stars != null) - Number(a.manual_stars != null) ||
    (b.manual_stars ?? 0) - (a.manual_stars ?? 0) ||
    ratingOf(b) - ratingOf(a) ||
    Number(isPick(b)) - Number(isPick(a)));
  const pick = eligible.find(isPick);
  const list = pick ? [pick, ...ordered.filter((f) => f !== pick)] : ordered;
  return new Map(list.slice(0, top).map((f, i) => [f.id, i + 1]));
}
export function filterFrames(frames, search, minStars, rejected) {
  const needle=search.trim().toLowerCase();
  return frames.filter(f=>(!needle||f.path.toLowerCase().includes(needle))&&frameStars(f)>=minStars&&(rejected==='all'||Boolean(f.rejected)===(rejected==='rejected')));
}
