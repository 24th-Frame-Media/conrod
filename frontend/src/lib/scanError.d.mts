import type { Job, ScanArgs, ScanProgress, Status } from './types';

export type ScanErrorKind = 'disk_full' | 'folder_missing' | 'nas_unreachable' | 'model_failed' | 'unknown';
export type ScanFailure = {
  kind: ScanErrorKind;
  /** The short heading ("The disk is full"). */
  title: string;
  /** One plain sentence with the next step and a Resume prompt. */
  message: string;
  /** The raw engine text, for a tooltip or the activity log. */
  detail: string;
  resumable: boolean;
  jobId?: number;
  label?: string;
  raw?: string;
  /** The original scan request, so the banner's Resume can retry it. */
  args?: ScanArgs;
};

export function scanErrorKind(raw?: string, root?: string): ScanErrorKind;
export function classifyScanError(raw?: string, options?: { root?: string; label?: string }): ScanFailure;
export function scanFailureFor(input?: {
  job?: Pick<Job, 'id' | 'label' | 'root' | 'status' | 'error_samples'> | null;
  progress?: Pick<ScanProgress, 'state'> | null;
  status?: Pick<Status, 'tasks'> | null;
  seed?: ScanFailure | null;
}): ScanFailure | null;
