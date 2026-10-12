//! Narrative health's constants (Vzestup's `healthConstants.ts`), each the value Vzestup keeps.
//! Most are designed priors, an owner's years of coaching speakers set down as numbers; the
//! fidelity slope and the intensity gap were calibrated against real decks, the keyword ceiling
//! is where real decks were seen to top out, and the tone gap is a first cut waiting on
//! telemetry. None is fitted to human ratings yet.

/// How hard arc fidelity punishes the deck's distance from the curve: `1 − cost × slope`, so a
/// normalised cost of 0 reads 1 and one of about 0.83 reads 0. Calibrated in 2026-05 to Splat's
/// gentler reading; it was 2, which read good decks in the sixties.
pub const DTW_FIDELITY_SLOPE: f64 = 1.2;

/// What a cell of the warp weighs in the setup, the deck's first half: a soft open is forgiven.
pub const STAKE_WEIGHT_SETUP: f64 = 1.0;
/// What a cell weighs at the climax, from half the deck to 85% of it: a flat peak costs most.
pub const STAKE_WEIGHT_CLIMAX: f64 = 2.0;
/// What a cell weighs in the close, past 85% of the deck: more than the open, less than the peak.
pub const STAKE_WEIGHT_CLOSE: f64 = 1.5;
/// Where the setup ends, a fraction of the way through the deck.
pub const STAKE_SETUP_BOUNDARY: f64 = 0.5;
/// Where the close begins, a fraction of the way through the deck.
pub const STAKE_CLOSE_BOUNDARY: f64 = 0.85;

/// How far a slide's intensity strays from the curve before it bends it: a quarter of the
/// scale, where a coach notices a slide in the wrong gear.
pub const DEVIANT_INTENSITY_DELTA: f64 = 0.25;

/// The drift between neighbouring windows the top fifth of drifts must pass to mark a topic
/// shift, so a coherent deck's largest drifts are not taken for one.
pub const TOPIC_SHIFT_FLOOR: f64 = 0.12;
/// The share of window drifts, largest first, that may mark a topic shift: the top fifth.
pub const TOPIC_QUINTILE: f64 = 0.2;
/// The spread of neighbours' similarities below which the deck is flat, and a pair a spread
/// below the mean is rounding, not a break.
pub const STAT_BREAK_MIN_STD: f64 = 0.001;
/// A window's width, in slides: the slides evaluated over this, held between the least and
/// the most below.
pub const WINDOW_SIZE_DIVISOR: usize = 4;
/// The narrowest window.
pub const WINDOW_SIZE_MIN: usize = 2;
/// The widest window.
pub const WINDOW_SIZE_MAX: usize = 3;
/// Where keyword overlap between neighbouring slides tops out on real decks, which the keyword
/// fallback's coherence is read against: measured, not chosen.
pub const KEYWORD_OVERLAP_CEILING: f64 = 0.4;

/// What pacing weighs in the overall score, where coherence reads embeddings.
pub const COMPOSITE_WEIGHT_PACING: f64 = 0.2;
/// What content balance weighs in the overall score.
pub const COMPOSITE_WEIGHT_BALANCE: f64 = 0.25;
/// What coherence weighs in the overall score: the most, since a deck that does not flow fails.
pub const COMPOSITE_WEIGHT_COHERENCE: f64 = 0.3;
/// What arc fidelity weighs in the overall score.
pub const COMPOSITE_WEIGHT_ARC: f64 = 0.25;

/// What pacing weighs where coherence reads keywords, and is left out: the same weight, the
/// three over their sum.
pub const COMPOSITE_FALLBACK_WEIGHT_PACING: f64 = COMPOSITE_WEIGHT_PACING;
/// What content balance weighs where coherence is left out.
pub const COMPOSITE_FALLBACK_WEIGHT_BALANCE: f64 = COMPOSITE_WEIGHT_BALANCE;
/// What arc fidelity weighs where coherence is left out.
pub const COMPOSITE_FALLBACK_WEIGHT_ARC: f64 = COMPOSITE_WEIGHT_ARC;

/// How far a slide's declared intensity may sit from what its words read before it is a gap.
/// Launched at 0.35; tightened on 2026-06-13, when a real deck's slides read 0.30 to 0.34
/// below what they declared and passed, to match [`DEVIANT_INTENSITY_DELTA`].
pub const INTENSITY_GAP_THRESHOLD: f64 = 0.25;

/// How far a slide's declared tone may sit from what its words read, on the scale of −1 to 1,
/// before it is a gap: generous, as the intensity gap was at first, since the lexicon reads
/// most prose mildly.
pub const TONE_GAP_THRESHOLD: f64 = 0.6;
