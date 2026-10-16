// The hyphenation patterns a single-file export's worker is built with (PLAN 2.5): none to
// fetch. Its module, the player's, has every language compiled in (ADR-0015).
export const hyphenation: Record<string, string> = {};

/** A forecast's icons, which the player's module has compiled in too (ADR-0030). */
export const weatherIcons: string | undefined = undefined;
