export const filename = path => path.split(/[\\/]/).pop() || path;
export const starsFor = rating => rating == null ? 0 : rating >= .958 ? 5 : rating >= .825 ? 4 : rating >= .728 ? 3 : rating >= .606 ? 2 : 1;
export const frameStars = frame => frame.manual_stars ?? starsFor(frame.rating);
export const subjectExcluded = (frame, d) => Boolean(frame.rejected || d.rejected || d.bystander || (d.cull_reason && d.stars == null && frame.manual_stars == null));
export const frameExcluded = (frame, detections) => Boolean(frame.rejected || (detections.length && detections.every(d => subjectExcluded(frame, d))));
export function filterFrames(frames, search, minStars, rejected) {
  const needle=search.trim().toLowerCase();
  return frames.filter(f=>(!needle||f.path.toLowerCase().includes(needle))&&frameStars(f)>=minStars&&(rejected==='all'||Boolean(f.rejected)===(rejected==='rejected')));
}
