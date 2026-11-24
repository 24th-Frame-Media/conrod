type Target = { closest?: (selector: string) => unknown } | null | undefined;
export function ownsActivationKeys(target: Target | EventTarget): boolean;
export function zoomKeyIgnored(target: Target | EventTarget, key: string): boolean;
