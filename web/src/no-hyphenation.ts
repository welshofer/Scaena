// The hyphenation patterns a single-file export's worker is built with (PLAN 2.5): none to
// fetch. Its module, the player's, has every language compiled in (ADR-0015).
export const hyphenation: Record<string, string> = {};

/** The syllable table, which its module never asks for: it counts no verse (PLAN O.4a). */
export const verse: string | undefined = undefined;
