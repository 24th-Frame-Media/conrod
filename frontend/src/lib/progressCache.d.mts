export const PROGRESS_KEEP: number;
export function rememberProgress<T extends { job_id: number; state?: string }>(
  all: Record<number, T>, order: readonly number[], payload: T, limit?: number,
): { all: Record<number, T>; order: number[] };
