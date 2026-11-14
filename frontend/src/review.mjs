export const filename = path => path.split(/[\\/]/).pop() || path;
export const starsFor = rating => rating == null ? 0 : rating >= .958 ? 5 : rating >= .825 ? 4 : rating >= .728 ? 3 : rating >= .606 ? 2 : 1;
export const frameStars = frame => frame.manual_stars ?? starsFor(frame.rating);
export const subjectExcluded = (frame, d) => Boolean(frame.rejected || d.rejected || d.bystander || (d.cull_reason && d.stars == null && frame.manual_stars == null));
export const frameExcluded = (frame, detections) => Boolean(frame.rejected || (detections.length && detections.every(d => subjectExcluded(frame, d))));
// Burst candidates: manual stars first, then not rejected, then rating, then the engine's pick on ties.
// The engine's pick always takes rank 1; the rest of the top `top` follow the ordering.
export function burstTopRanks(group, isPick, top = 3) {
  const ordered = [...group].sort((a, b) =>
    Number(b.manual_stars != null) - Number(a.manual_stars != null) ||
    (b.manual_stars ?? 0) - (a.manual_stars ?? 0) ||
    Number(!b.rejected) - Number(!a.rejected) ||
    (b.rating ?? -1) - (a.rating ?? -1) ||
    Number(isPick(b)) - Number(isPick(a)));
  const pick = group.find(isPick);
  const list = pick ? [pick, ...ordered.filter((f) => f !== pick)] : ordered;
  return new Map(list.slice(0, top).map((f, i) => [f.id, i + 1]));
}
export function filterFrames(frames, search, minStars, rejected) {
  const needle=search.trim().toLowerCase();
  return frames.filter(f=>(!needle||f.path.toLowerCase().includes(needle))&&frameStars(f)>=minStars&&(rejected==='all'||Boolean(f.rejected)===(rejected==='rejected')));
}
