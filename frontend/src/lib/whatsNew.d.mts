/** One release in `whatsNew.json`: a plain-English summary and the changes, grouped as New, Improved and Fixed. */
export type WhatsNewSection = { title: string; items: string[] };
export type WhatsNewEntry = {
  version: string;
  /** YYYY-MM-DD, the day the release was tagged. */
  date?: string;
  /** One sentence for the top of the release. */
  highlights?: string;
  sections: WhatsNewSection[];
  unreleased?: boolean;
};

export const SEEN_KEY: string;

/** Oldest first; a pre-release sorts before its stable release. */
export function compareVersions(a: string, b: string): number;
export function isVersion(value: unknown): boolean;

/** Released entries only, newest first. */
export function orderEntries(entries: WhatsNewEntry[] | null | undefined): WhatsNewEntry[];

/** True once per new version: this build has an entry and the version last seen is older or missing. */
export function shouldShowWhatsNew(args: {
  current: string | null | undefined;
  lastSeen: string | null | undefined;
  entries: WhatsNewEntry[] | null | undefined;
}): boolean;

/** `fresh`: released since `lastSeen` up to `current` (at least the newest), newest first; `earlier`: the rest. */
export function splitEntries(args: {
  entries: WhatsNewEntry[] | null | undefined;
  current: string | null | undefined;
  lastSeen: string | null | undefined;
}): { fresh: WhatsNewEntry[]; earlier: WhatsNewEntry[] };

type KeyValueStore = { getItem(key: string): string | null; setItem(key: string, value: string): void };
export function readSeen(storage: KeyValueStore | null | undefined): string | null;
export function writeSeen(storage: KeyValueStore | null | undefined, version: string | null | undefined): string | null;

/** "20 Nov 2026" from YYYY-MM-DD; null for anything else. */
export function formatDate(date: string | null | undefined): string | null;
