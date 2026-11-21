export type ApplyLineInput = {
  autoApply?: boolean;
  /** The album's status, as `review`/`jobs` report it. */
  status?: string | null;
  /** The "Colour for suggested picks" setting; "None" means no colour. */
  pickLabel?: string | null;
};

/** The one-line note in Review: what Conrod will apply, or already applied. */
export function applyLine(input?: ApplyLineInput): string;
