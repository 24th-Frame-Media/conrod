export const STRICTNESS = [
  { id: 'critical', name: 'Critical', hint: 'Stars need visibly crisp edges. For track and race work.' },
  { id: 'strict', name: 'Strict', hint: 'Flags any softness. Best for sharp, short-lens work.' },
  { id: 'moderate', name: 'Moderate', hint: 'Suits long lenses and heat haze. The usual choice.' },
  { id: 'lenient', name: 'Lenient', hint: 'Only clearly blurred frames count as soft.' },
] as const;

type Props = { value: string; onChange: (id: string) => void };

/** The strictness levels as one four-stop slider; it becomes the album's blur strictness. */
export function StrictnessSlider({ value, onChange }: Props) {
  const at = Math.max(0, STRICTNESS.findIndex((s) => s.id === value));
  return (
    <div className="strictness">
      <input type="range" min={0} max={STRICTNESS.length - 1} step={1} value={at} aria-label="Strictness" aria-valuetext={STRICTNESS[at].name}
        onChange={(e) => onChange(STRICTNESS[Number(e.target.value)].id)} />
      <div className="strictness-stops" aria-hidden="true">
        {STRICTNESS.map((s, i) => (
          <button key={s.id} type="button" tabIndex={-1} className={i === at ? 'on' : ''} onClick={() => onChange(s.id)}>{s.name}</button>
        ))}
      </div>
      <small className="muted">{STRICTNESS[at].hint}</small>
    </div>
  );
}
