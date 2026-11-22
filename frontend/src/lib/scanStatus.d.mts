import type { Job, ScanProgress, ScanStatus } from './types';

/** The scan state an album's stored status implies when nothing live answers. */
export function stateFromJob(job: Job | null | undefined, progress?: ScanProgress | null): ScanStatus['state'] | 'idle';

export type ScanView = {
  /** Where the snapshot came from: the live event stream, the entry read, or the album record alone. */
  source: 'live' | 'status' | 'job';
  state: ScanStatus['state'] | 'idle';
  progress: ScanProgress | null;
  queuePosition: number | null;
  errors: number;
  errorSamples: string[];
  resumeStage: string | null;
  paused: boolean;
  terminal: boolean;
};

/** Which snapshot the Scan screen draws, and what the album's scan is doing when none does. */
export function scanView(input?: {
  job?: Job | null;
  live?: { progress: ScanProgress; at: number } | null;
  hydrate?: { status: ScanStatus; at: number } | null;
  activeJob?: number | null;
}): ScanView;
