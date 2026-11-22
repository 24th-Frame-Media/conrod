import type { IdentifyLive, Job, Status } from '../lib/types';
import type { VisionAction } from '../lib/vision.mjs';
import { IngestScreen } from './Ingest';

type Props = {
  job: Job | undefined; live: IdentifyLive | undefined; status: Status;
  /** The vision model this album would call, for the VISION node's subline. */
  model?: string;
  /** The cloud decision service actually serving moments, for the DECIDER node. */
  decisionService?: string | null;
  paused: boolean;
  onCancel: (key: string) => void;
  onReview: () => void;
  onVisionAction?: (id: VisionAction['id']) => void;
};

/** An album while a standalone identification runs (and after): the ingest screen with only the vision stages. */
export function IdentifyScreen({ job, live, status, model, decisionService, paused, onCancel, onReview, onVisionAction }: Props) {
  return (
    <IngestScreen mode="identify" job={job} identify={live} status={status} model={model} decisionService={decisionService} paused={false} identifyPaused={paused}
      onCancelOperation={onCancel} onVisionAction={onVisionAction} onReview={onReview} onLibrary={onReview} />
  );
}
