import type { Job } from './types';

export const etaText = (seconds: number | null | undefined): string =>
  seconds == null ? '' : seconds < 90 ? `~${Math.ceil(seconds)}s left` : seconds < 5400 ? `~${Math.ceil(seconds / 60)} min left` : `~${(seconds / 3600).toFixed(1)} h left`;
export const words = (key: string): string => key.replaceAll('_', ' ');
export const pct = (done: number, total: number): number => (total > 0 ? Math.min(100, Math.round((done * 100) / total)) : 0);
export const shortDate = (unix: number): string => new Date(unix * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });
export const plural = (n: number, one: string, many = `${one}s`): string => `${n.toLocaleString()} ${n === 1 ? one : many}`;

/** The one-line state of an album, in the words the Python home screen uses for its cover. */
export function jobState(job: Job): string {
  if (job.status === 'done') return 'Ready to review';
  if (job.status === 'indexed') return 'Indexed — ready to cull';
  if (job.status === 'scanning') return `Scanning ${pct(job.done, job.total)}%`;
  if (job.status === 'stopped' || job.status === 'incomplete') return `Stopped at ${pct(job.done, job.total)}%`;
  return job.done < job.total ? `In progress ${pct(job.done, job.total)}%` : job.status;
}

/** A stable, muted cover tint per album so the wall of cards is not one grey. */
export const coverTint = (id: number): string => `linear-gradient(135deg, hsl(${(id * 47) % 360} 32% 15%), hsl(${(id * 47 + 40) % 360} 38% 21%))`;
