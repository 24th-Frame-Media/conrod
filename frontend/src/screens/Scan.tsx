import type { IdentifyLive, Job, Settings, Status } from '../lib/types';
import type { VisionAction } from '../lib/vision.mjs';
import type { Runner } from '../state/hooks';
import type { ScanLive } from '../state/useScanProgress';
import { IngestScreen } from './Ingest';

type Props = {
  job: Job | undefined; live: ScanLive | undefined; running: boolean; paused: boolean;
  /** Albums waiting their turn, first to start first. */
  queue?: Job[];
  identify?: IdentifyLive; status: Status; model?: string; identifyPaused: boolean;
  run: Runner; onSettingsSaved: (s: Settings) => void;
  onPause: () => void; onResume: () => void; onCancel: () => void; onCancelOperation: (key: string) => void;
  onReview: () => void; onLibrary: () => void;
  onVisionAction?: (id: VisionAction['id']) => void;
  onUnqueue?: (job: Job) => void;
};

/** An album while it scans (or just after): the unified ingest screen with every stage. Review is locked until it is done. */
export function ScanScreen({ live, identify, ...rest }: Props) {
  return <IngestScreen mode="ingest" scan={live} identify={identify} {...rest} />;
}
