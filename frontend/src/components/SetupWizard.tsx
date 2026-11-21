import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { call, engine } from '../lib/api';
import { canInstall, sizeWords } from '../lib/decider.mjs';
import {
  PEOPLE_ANSWERS, PEOPLE_PRESETS, READ_KEYS, READ_LABELS, WIZARD_GROUPS, answersFromProfile, buildProfile, choiceTitle, listWords,
  orderSelected, prefilledReads, profileOf, readerApplies, summaryLines, type PeopleAnswer,
} from '../lib/wizard.mjs';
import type { DeciderStatus, Settings } from '../lib/types';
import { Icon, Portal } from './basics';
import { StrictnessSlider } from './StrictnessSlider';

type Props = {
  visible: boolean;
  settings: Settings;
  /** The settings the engine saved, so the rest of the app sees the new profile and default shoot type. */
  onSaved: (settings: Settings) => void;
  onClose: () => void;
  /** Closes the wizard (after saving) and opens Settings, for the vision services. */
  onOpenSettings: () => void;
  toast: (message: string, options?: { tone?: 'ok' | 'error' }) => void;
};

const STEPS = ['What you shoot', 'People and readers', 'How picky', 'This computer', 'All set'] as const;
const gb = (bytes: number) => (bytes / 2 ** 30).toFixed(1) + ' GB';

/**
 * First-run setup: five short screens that tune Conrod to the photographer. Every answer is optional (Enter or Next
 * keeps what Conrod does today; Skip setup keeps all of it) and nothing is saved until the last screen. The answers are
 * the photographer's own defaults per shoot type (`user_profile`): a new album copies them, the album can still
 * change any switch, and Conrod's own policy fills in the rest.
 */
export function SetupWizard({ visible, settings, onSaved, onClose, onOpenSettings, toast }: Props) {
  const initial = useMemo(() => answersFromProfile(profileOf(settings)), [settings, visible]); // eslint-disable-line react-hooks/exhaustive-deps -- seed each time it opens
  const [step, setStep] = useState(0);
  const [selected, setSelected] = useState<string[]>(initial.selected);
  const [defaultId, setDefaultId] = useState<string | null>(initial.defaultId);
  const [people, setPeople] = useState<Record<string, PeopleAnswer>>(initial.people);
  const [reads, setReads] = useState<Record<string, boolean>>(initial.reads);
  const [blur, setBlur] = useState<string | null>(initial.blur);
  const [momentsRank, setMomentsRank] = useState(initial.momentsRank);
  const [status, setStatus] = useState<DeciderStatus | null>(null);
  const [useDecider, setUseDecider] = useState<boolean | null>(null);
  const [saving, setSaving] = useState(false);
  const sheet = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!visible) return;
    setStep(0);
    setSelected(initial.selected); setDefaultId(initial.defaultId); setPeople(initial.people); setReads(initial.reads);
    setBlur(initial.blur); setMomentsRank(initial.momentsRank); setUseDecider(null); setStatus(null);
  }, [visible]); // eslint-disable-line react-hooks/exhaustive-deps -- reseed on open only
  useEffect(() => {
    if (visible && step === 3 && !status) void engine.deciderStatus().then(setStatus).catch(() => undefined);
  }, [visible, step, status]);

  const ordered = useMemo(() => orderSelected(selected, defaultId), [selected, defaultId]);
  const asked = ordered.filter((id) => PEOPLE_PRESETS.includes(id));
  const answerFor = (id: string): PeopleAnswer => people[id] ?? 'around';
  const prefill = useMemo(() => prefilledReads(ordered, people), [ordered, people]);
  const checked = (key: string) => (typeof reads[key] === 'boolean' ? reads[key] : prefill[key]);
  const profile = useMemo(() => buildProfile({ selected: ordered, people, reads, blur, momentsRank }), [ordered, people, reads, blur, momentsRank]);
  const lines = useMemo(() => summaryLines(profile), [profile]);

  const modelReady = status != null && canInstall(status);
  const installed = status != null && status.installed && !status.needs_install && status.state !== 'no_gpu' && status.state !== 'no_vram';
  const wantDecider = useDecider ?? (installed ? status.enabled : false);

  // Focus the first control of each screen, so the keyboard picks up where the eyes are.
  useEffect(() => {
    if (!visible) return;
    const first = sheet.current?.querySelector<HTMLElement>('[data-wizard-focus], input, select');
    first?.focus();
  }, [visible, step]);

  const save = useCallback(async (skip: boolean, then?: () => void) => {
    setSaving(true);
    try {
      const old = profileOf(settings);
      // Skipping keeps whatever profile there was and only marks the wizard as done.
      const next = skip ? { ...old, done: true } : profile;
      const patch: Record<string, unknown> = { user_profile: next };
      if (!skip && next.default_preset) patch.scan_profile = next.default_preset;
      const install = !skip && modelReady && wantDecider;
      if (!skip && status != null && (install || installed) && wantDecider !== status.enabled) patch.use_decider = wantDecider;
      const saved = await call<Settings>('save_settings', patch);
      onSaved(saved);
      if (install) {
        try {
          await engine.installDecider();
          toast('Installing the decision model. It keeps going in the background.', { tone: 'ok' });
        } catch (e) {
          toast(String(e), { tone: 'error' });
        }
      }
      if (!skip) toast('Conrod is set up. Change any of it from Settings.', { tone: 'ok' });
      onClose();
      then?.();
    } catch (e) {
      toast(String(e), { tone: 'error' });
    } finally {
      setSaving(false);
    }
  }, [settings, profile, modelReady, wantDecider, installed, status, onSaved, onClose, toast]);

  const last = step === STEPS.length - 1;
  const next = () => { if (last) void save(false); else setStep((s) => s + 1); };
  const back = () => setStep((s) => Math.max(0, s - 1));
  const skip = () => void save(true);

  // Enter goes on, Escape skips the whole setup. A focused button or list keeps its own Enter.
  useEffect(() => {
    if (!visible) return;
    const onKey = (e: KeyboardEvent) => {
      if (saving) return;
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); skip(); return; }
      if (e.key !== 'Enter' || e.shiftKey || e.ctrlKey || e.metaKey || e.altKey) return;
      const tag = (e.target as HTMLElement | null)?.tagName;
      if (tag === 'BUTTON' || tag === 'SELECT' || tag === 'A' || tag === 'TEXTAREA') return;
      e.preventDefault();
      next();
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  });

  if (!visible) return null;
  const toggleShoot = (id: string) => setSelected((cur) => (cur.includes(id) ? cur.filter((x) => x !== id) : [...cur, id]));
  const defaultShown = ordered[0] ?? null;

  return (
    <Portal>
      <div className="import-overlay wizard-overlay">
        <div className="import-dialog wizard-sheet" role="dialog" aria-modal="true" aria-labelledby="wizard-title" ref={sheet}>
          <button type="button" className="ghost icon import-close" aria-label="Skip setup" title="Skip setup (Esc)" onClick={skip} disabled={saving}><Icon name="x" /></button>
          <p className="wizard-step muted" aria-live="polite">Step {step + 1} of {STEPS.length}</p>
          <ol className="wizard-dots" aria-hidden="true">
            {STEPS.map((name, i) => <li key={name} className={i === step ? 'on' : i < step ? 'done' : ''} />)}
          </ol>

          {step === 0 && (
            <section aria-label={STEPS[0]}>
              <h2 id="wizard-title">What do you shoot?</h2>
              <p className="lede">Pick everything you shoot, or skip this and Conrod stays as it is. It uses your answers to decide what to look for and what to read.</p>
              {WIZARD_GROUPS.map((group) => (
                <fieldset className="wizard-group" key={group.id}>
                  <legend>{group.title}</legend>
                  <div className="wizard-chips">
                    {group.choices.map((choice) => (
                      <label key={choice.id} className={`wizard-chip${selected.includes(choice.id) ? ' on' : ''}`} title={choice.hint}>
                        <input type="checkbox" checked={selected.includes(choice.id)} onChange={() => toggleShoot(choice.id)} />
                        <span>{choice.title}</span>
                      </label>
                    ))}
                  </div>
                </fieldset>
              ))}
              {ordered.length > 1 && (
                <div className="wizard-default">
                  <label htmlFor="wizard-default">New imports start as</label>
                  <select id="wizard-default" value={defaultShown ?? ''} onChange={(e) => setDefaultId(e.target.value)}>
                    {ordered.map((id) => <option key={id} value={id}>{choiceTitle(id)}</option>)}
                  </select>
                </div>
              )}
              {ordered.length === 1 && <p className="muted wizard-note">New imports will start as {choiceTitle(ordered[0])}.</p>}
            </section>
          )}

          {step === 1 && (
            <section aria-label={STEPS[1]}>
              <h2 id="wizard-title">People, and what to read</h2>
              <p className="lede">Both are optional. Anything you leave alone keeps Conrod's usual choice for that shoot.</p>
              {asked.map((id) => (
                <fieldset className="wizard-ask" key={id}>
                  <legend>At {choiceTitle(id).toLowerCase()}, who is in your pictures?</legend>
                  {PEOPLE_ANSWERS.map((option) => (
                    <label key={option.id} className="wizard-radio">
                      <input type="radio" name={`people-${id}`} checked={answerFor(id) === option.id} onChange={() => setPeople((cur) => ({ ...cur, [id]: option.id }))} />
                      <span><b>{option.title}</b><small>{option.hint}</small></span>
                    </label>
                  ))}
                </fieldset>
              ))}
              <fieldset className="wizard-ask">
                <legend>What should Conrod read?</legend>
                <p className="muted wizard-note">Ticked ones come from your shoots. A reader only works where it can: plates come off cars, names off faces.</p>
                <div className="wizard-reads">
                  {READ_KEYS.map((key) => {
                    const where = readerApplies(ordered, key, people);
                    const dead = ordered.length > 0 && where.length === 0;
                    return (
                      <label key={key} className={`wizard-read${checked(key) && !dead ? ' on' : ''}`}>
                        <input type="checkbox" checked={checked(key) && !dead} disabled={dead} onChange={(e) => setReads((cur) => ({ ...cur, [key]: e.target.checked }))} />
                        <span>
                          <b>{READ_LABELS[key]}</b>
                          {ordered.length > 0 && <small>{dead ? 'Nothing you shoot can use this' : `For ${listWords(where.map((id) => choiceTitle(id).toLowerCase()))}`}</small>}
                        </span>
                      </label>
                    );
                  })}
                </div>
              </fieldset>
            </section>
          )}

          {step === 2 && (
            <section aria-label={STEPS[2]}>
              <h2 id="wizard-title">How picky should Conrod be?</h2>
              <p className="lede">This is how soft a frame can be before Conrod marks it as blurred.</p>
              <fieldset className="wizard-ask">
                <legend>Blur check</legend>
                <label className="wizard-radio">
                  <input type="radio" name="blur" checked={blur === null} onChange={() => setBlur(null)} />
                  <span><b>Match each shoot type</b><small>Strict for portraits and property, forgiving for concerts and rally. The usual choice.</small></span>
                </label>
                <label className="wizard-radio">
                  <input type="radio" name="blur" checked={blur !== null} onChange={() => setBlur(blur ?? 'moderate')} />
                  <span><b>Always the same level</b><small>One level for every new album. You can still change it per album.</small></span>
                </label>
                {blur !== null && <StrictnessSlider value={blur} onChange={setBlur} />}
              </fieldset>
              <fieldset className="wizard-ask">
                <legend>Moments</legend>
                <label className="wizard-radio">
                  <input type="checkbox" checked={momentsRank} onChange={(e) => setMomentsRank(e.target.checked)} />
                  <span>
                    <b>Let moments lift frames</b>
                    <small>
                      Conrod spots a first kiss, tyre smoke or a jump at its peak, and uses it to choose between frames that are equally sharp.
                      It can be wrong, so a moment it is not sure about is only labelled Maybe. It never rescues a blurred frame and never pushes a sharp one down.
                    </small>
                  </span>
                </label>
              </fieldset>
            </section>
          )}

          {step === 3 && (
            <section aria-label={STEPS[3]}>
              <h2 id="wizard-title">This computer</h2>
              <p className="lede">Conrod can use an optional on-device decision model to judge moments more accurately. It needs a graphics card with enough video memory.</p>
              <div className="wizard-hardware" role="group" aria-label="Graphics card and decision model">
                {!status && <p className="muted">Checking your graphics card</p>}
                {status && (
                  <>
                    <p><b>{status.gpu ? status.gpu.name : 'No graphics card found'}</b>
                      {status.gpu && <span className="muted"> {gb(status.gpu.total)} video memory, {gb(status.gpu.free)} free</span>}</p>
                    {(status.state === 'no_gpu' || status.state === 'no_vram') && (
                      <p className="muted">The decision model cannot run here ({status.state === 'no_gpu' ? 'no graphics card it can use' : `it needs about ${gb(status.needs.int4)} free and this has less`}). Conrod works fine without it: moments are judged by the vision model instead.</p>
                    )}
                    {modelReady && (
                      <label className="wizard-radio">
                        <input type="checkbox" checked={wantDecider} onChange={(e) => setUseDecider(e.target.checked)} />
                        <span>
                          <b>{status.installed && status.download_bytes === 0 ? 'Use the decision model' : `Download and use the decision model (${sizeWords(status.download_bytes)})`}</b>
                          <small>Downloaded once, checked against a pinned hash and resumed if interrupted. It keeps going in the background after setup, and holds a few GB of video memory while it works.</small>
                        </span>
                      </label>
                    )}
                    {installed && !modelReady && (
                      <label className="wizard-radio">
                        <input type="checkbox" checked={wantDecider} onChange={(e) => setUseDecider(e.target.checked)} />
                        <span><b>Use the decision model</b><small>It is already installed on this computer.</small></span>
                      </label>
                    )}
                    {status.state === 'installing' && <p className="muted">It is downloading now. It will be ready when it finishes.</p>}
                  </>
                )}
              </div>
              <p className="muted wizard-note">
                Vision services (the model that reads cars and people) are set up in Settings.{' '}
                <button type="button" className="wizard-link" onClick={() => void save(false, onOpenSettings)} disabled={saving}>Finish and open Settings</button>
              </p>
            </section>
          )}

          {step === 4 && (
            <section aria-label={STEPS[4]}>
              <h2 id="wizard-title">All set</h2>
              {lines.length === 0 ? (
                <p className="lede">You did not pick a shoot type, so Conrod keeps its usual choices. You can set it up again from Settings any time.</p>
              ) : (
                <>
                  <p className="lede">Here is what Conrod will do. New albums start from this; each album can still change any of it.</p>
                  <ul className="wizard-summary">
                    {lines.map((line, i) => (
                      <li key={line.preset}>
                        <b>{line.title}</b>{i === 0 && <> <span className="wizard-tag">new imports</span></>}
                        <p>
                          Looks for {listWords(line.looks) || 'nothing in particular'}.
                          {line.reads.length > 0 ? ` Reads ${listWords(line.reads)}.` : ' Reads no text.'}
                          {line.moments ? ` ${line.moments[0].toUpperCase()}${line.moments.slice(1)}.` : ''}
                          {` Blur check: ${line.blur}.`}
                        </p>
                      </li>
                    ))}
                  </ul>
                </>
              )}
              {status && modelReady && wantDecider && <p className="muted wizard-note">The decision model will start downloading when you finish.</p>}
            </section>
          )}

          <div className="wizard-actions">
            <button type="button" className="ghost" onClick={skip} disabled={saving}>Skip setup</button>
            <span className="wizard-spacer" />
            {step > 0 && <button type="button" className="ghost" onClick={back} disabled={saving}>Back</button>}
            <button type="button" className="primary" data-wizard-focus={step === 4 ? '' : undefined} onClick={next} disabled={saving}>
              {last ? (saving ? 'Saving' : lines.length ? 'Save and finish' : 'Finish') : step === 0 && selected.length === 0 ? 'Skip this step' : 'Next'}
            </button>
          </div>
        </div>
      </div>
    </Portal>
  );
}
