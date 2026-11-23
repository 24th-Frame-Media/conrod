import type { IdentifyLive, Job, Settings } from './types';

export type VisionOffReason = 'album' | 'settings' | 'no_service' | 'lanes';
export type VisionAction = { id: 'album' | 'service'; label: string };

export const VISION_OFF_REASONS: readonly VisionOffReason[];
/** The reason an identify snapshot carries, or null when vision is on. */
export function visionOffReason(live: Pick<IdentifyLive, 'vision_off_reason'> | null | undefined): VisionOffReason | null;
export function visionHeader(reason: VisionOffReason | null | undefined): string;
/** A friendly model name: `Qwen3-VL`; '' for no model. */
export function visionModelWords(model: string | null | undefined): string;
export function visionNodeSub(reason: VisionOffReason | null | undefined): string;
export function visionLaneWords(reason: VisionOffReason | null | undefined): { title: string; why: string; action: VisionAction | null };
/** The album's own vision choice, or null while it follows the Settings switch. */
export function albumVisionChoice(job: Pick<Job, 'settings_json'> | null | undefined): boolean | null;
export function albumVisionOn(job: Pick<Job, 'settings_json'> | null | undefined, globals: Partial<Settings> | null | undefined): boolean;
export function albumVisionHint(own: boolean | null, globals: Partial<Settings> | null | undefined): string;
export function nextAlbumVision(checked: boolean, globalOn: boolean): boolean | null;
