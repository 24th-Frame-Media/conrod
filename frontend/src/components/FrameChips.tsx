import type { FrameFacts } from '../lib/review';

/** The plate / number / vehicle / colour / team chips under a photo. Shared by the flat grid and a lone frame in the stack grid. */
export function FrameChips({ facts }: { facts?: FrameFacts }) {
  const swatch = facts?.colourCss && CSS.supports('color', facts.colourCss) ? facts.colourCss : undefined;
  return (
    <div className="chips">
      {facts?.plate && <span className="fact plate">{facts.plate}</span>}
      {facts?.number && <span className="fact number">#{facts.number}</span>}
      {facts?.vehicle && <span className="tag">{facts.vehicle}</span>}
      {facts?.colour && <span className="tag"><i className="swatch" style={{ background: swatch }} />{facts.colour}</span>}
      {facts?.team && <span className="fact team">{facts.team}</span>}
    </div>
  );
}
