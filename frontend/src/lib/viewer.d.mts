/** The AF fields the review payload carries per frame; all optional and null until the EXIF pass finds them. */
export type AfFrame = {
  af_x1?: number | null;
  af_y1?: number | null;
  af_x2?: number | null;
  af_y2?: number | null;
};

/** The frame's AF box as 0..1 of the photo, or null when the payload carries none. */
export function afBox(frame: AfFrame | null | undefined): [number, number, number, number] | null;
/** Where the camera focused, as the AF box's centre in rounded percent: "56% × 58%", or null. */
export function afLabel(frame: AfFrame | null | undefined): string | null;
