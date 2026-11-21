export type WizardChoice = { id: string; title: string; hint: string };
export type WizardGroup = { id: string; title: string; choices: WizardChoice[] };
export type PeopleAnswer = 'none' | 'around' | 'faces';
/** The profile the wizard saves, as the engine stores it in the global `user_profile` setting. */
export type UserProfile = {
  done?: boolean;
  default_preset?: string;
  shoots?: string[];
  blur?: string;
  all?: Record<string, boolean>;
  presets?: Record<string, Record<string, boolean>>;
};
export type WizardAnswers = {
  selected: string[];
  defaultId: string | null;
  people: Record<string, PeopleAnswer>;
  /** Only the readers the photographer changed. */
  reads: Record<string, boolean>;
  /** `null`: each shoot type keeps its own level. */
  blur: string | null;
  momentsRank: boolean;
};
export type SummaryLine = { preset: string; title: string; looks: string[]; reads: string[]; moments: string; blur: string };

export const WIZARD_GROUPS: WizardGroup[];
export const WIZARD_PRESETS: string[];
export const PEOPLE_PRESETS: string[];
export const PEOPLE_ANSWERS: { id: PeopleAnswer; title: string; hint: string }[];
export const READ_LABELS: Record<string, string>;
export const READ_KEYS: readonly string[];
export function choiceTitle(id: string): string;
export function profileOf(settings: unknown): UserProfile;
export function profileDefaults(profile: UserProfile | null | undefined, preset: string): Record<string, boolean>;
export function shouldAutoOpenWizard(args: { profile: UserProfile | null | undefined; albums: number; whatsNewSeen: string | null | undefined }): boolean;
export function prefilledReads(selected: string[], people?: Record<string, PeopleAnswer>): Record<string, boolean>;
export function readerApplies(selected: string[], key: string, people?: Record<string, PeopleAnswer>): string[];
export function buildProfile(args: { selected: string[]; people?: Record<string, PeopleAnswer>; reads?: Record<string, boolean>; blur?: string | null; momentsRank?: boolean }): UserProfile;
export function answersFromProfile(profile: UserProfile | null | undefined): WizardAnswers;
export function orderSelected(selected: string[], defaultId: string | null | undefined): string[];
export function summaryLines(profile: UserProfile | null | undefined): SummaryLine[];
export function listWords(items: string[]): string;
