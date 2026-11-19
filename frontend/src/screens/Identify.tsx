import type { IdentifyLive, Job, Status } from '../lib/types';
import { IngestScreen } from './Ingest';

type Props = {
  job: Job | undefined; live: IdentifyLive | undefined; status: Status;
  /** The vision model this album would call, for the VISION node's subline. */
  model?: string;
  paused: boolean;
  onCancel: (key: string) => void;
  onReview: () => void;
};

/** An album while a standalone identification runs (and after): the ingest screen with only the vision stages. */
export function IdentifyScreen({ job, live, status, model, paused, onCancel, onReview }: Props) {
  return (
    <IngestScreen mode="identify" job={job} identify={live} status={status} model={model} paused={false} identifyPaused={paused}
      onCancelOperation={onCancel} onReview={onReview} onLibrary={onReview} />
  );
}
