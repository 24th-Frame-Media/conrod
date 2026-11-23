export const SOURCE: Readonly<{ poll: number; hydrate: number; live: number }>;
export type Clock = { now(wall?: number): number; peek(): number };
export function makeClock(): Clock;
/** The one shared monotonic clock the whole app uses. */
export const clock: Clock;
export type Stamp = { at: number; source: string; seq?: number };
export function orderWins(current: Stamp | null | undefined, incoming: Stamp | null | undefined): boolean;
export function applySnapshot(current: any, incoming: any): any;
