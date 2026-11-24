export const FOCUSABLE: string;
export function trapTarget(count: number, current: number, shift: boolean): number | null;
export type SheetStack = { push(token: object): void; remove(token: object): void; isTop(token: object): boolean; readonly size: number };
export function createSheetStack(): SheetStack;
