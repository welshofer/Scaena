//! The intensity a slide's words read at (Vzestup's `detectedIntensity.ts`), on the 0–1 scale a
//! slide declares its own on: a curated arousal lexicon and the shape of the prose (exclamations,
//! capitals, imperatives, the second person, superlatives, figures), per token, squashed around
//! a neutral baseline. An independent reading, not an unbiased one: an English keyword count
//! misreads irony, jargon, and other languages, but not where the generator grading its own
//! slides would, so where the two disagree, the generator could not have laundered why. Every
//! weight is a designed prior, none fitted.

use super::lexicon;
use crate::js;
use regex_lite::Regex;
use std::sync::OnceLock;

/// The estimator's version, raised where a change would move what it reads; 2 reads nothing
/// in a language the lexicons cannot.
pub const DETECTED_INTENSITY_VERSION: u32 = 2;

// The lexicons: stems match the words they begin, four letters or more (see
// [`lexicon::stem_weight`]); words, and phrases, only themselves. Arousal weighs about 0.4 to
// 1; dampeners, which read procedural, −0.2 to −0.6.

/// High-arousal stems.
const AROUSAL_STEMS: [(&str, f64); 77] = [
    // — urgency / crisis —
    ("critical", 1.0),
    ("crisis", 1.0),
    ("crises", 1.0),
    ("urgent", 1.0),
    ("urgenc", 1.0),
    ("emergenc", 1.0),
    ("imperativ", 0.9),
    ("catastroph", 1.0),
    ("disaster", 0.95),
    ("threat", 0.85),
    ("danger", 0.85),
    ("jeopard", 0.85),
    ("peril", 0.85),
    ("alarm", 0.8),
    ("severe", 0.8),
    ("dire", 0.85),
    // — change / transformation —
    ("breakthrough", 0.95),
    ("transform", 0.85),
    ("revolution", 0.9),
    ("disrupt", 0.85),
    ("reinvent", 0.8),
    ("overhaul", 0.75),
    ("reimagin", 0.75),
    // — magnitude / momentum verbs —
    ("surge", 0.85),
    ("soar", 0.9),
    ("skyrocket", 0.95),
    ("plummet", 0.9),
    ("collapse", 0.9),
    ("crash", 0.85),
    ("plunge", 0.85),
    ("explod", 0.9),
    ("accelerat", 0.7),
    ("spike", 0.7),
    ("escalat", 0.75),
    // — stakes / outcome —
    ("unprecedented", 0.85),
    ("monumental", 0.8),
    ("staggering", 0.85),
    ("massive", 0.7),
    ("enormous", 0.7),
    ("dramatic", 0.75),
    ("decisive", 0.7),
    ("pivotal", 0.75),
    // — competitive / win-lose framing —
    ("conquer", 0.8),
    ("dominat", 0.75),
    ("defeat", 0.75),
    ("destroy", 0.85),
    // — reveal / insight (from intensity_phases vocab) —
    ("reveal", 0.6),
    ("insight", 0.55),
    ("significant", 0.55),
    ("remarkabl", 0.65),
    ("extraordinar", 0.85),
    ("astonish", 0.85),
    ("shock", 0.85),
    ("stun", 0.8),
    // — opportunity / momentum —
    ("opportunity", 0.55),
    ("momentum", 0.6),
    ("thrive", 0.6),
    ("triumph", 0.8),
    ("victor", 0.75),
    // — failure / loss —
    ("doom", 0.9),
    ("bankrupt", 0.85),
    // — moral / call —
    ("fight", 0.7),
    ("battle", 0.7),
    ("struggl", 0.65),
    // — emotionally charged —
    ("fear", 0.75),
    ("outrage", 0.85),
    ("fury", 0.85),
    ("passion", 0.65),
    ("thrill", 0.75),
    // — intensifying adjectives —
    ("vital", 0.7),
    ("essential", 0.6),
    ("crucial", 0.75),
    ("paramount", 0.75),
    // — finance momentum —
    ("windfall", 0.7),
    ("meltdown", 0.95),
    // — narrative escalation —
    ("inflection", 0.6),
    ("tipping", 0.6),
];

/// High-arousal whole words, and phrases, where a stem would match too much ("window", "nowhere").
const AROUSAL_WORDS: [(&str, f64); 27] = [
    ("now", 0.7),
    ("must", 0.8),
    ("win", 0.7),
    ("wins", 0.7),
    ("won", 0.6),
    ("lose", 0.75),
    ("loses", 0.75),
    ("lost", 0.7),
    ("loss", 0.7),
    ("losses", 0.7),
    ("fail", 0.7),
    ("fails", 0.7),
    ("risk", 0.7),
    ("risks", 0.7),
    ("stake", 0.7),
    ("stakes", 0.75),
    ("war", 0.7),
    ("die", 0.8),
    ("death", 0.8),
    ("demand", 0.6),
    ("demands", 0.6),
    ("act now", 0.9),
    ("game-changer", 0.85),
    ("game-changing", 0.85),
    ("wake-up", 0.8),
    ("do-or-die", 0.95),
    ("all-in", 0.75),
];

/// Low-arousal, clinical stems, which pull a slide down even where it slips in a charged word.
const DAMPENER_STEMS: [(&str, f64); 60] = [
    ("overview", -0.5),
    ("summary", -0.5),
    ("summari", -0.45),
    ("appendix", -0.6),
    ("agenda", -0.45),
    ("introduct", -0.3),
    ("background", -0.35),
    ("context", -0.3),
    ("status", -0.5),
    ("update", -0.45),
    ("recap", -0.45),
    ("outline", -0.45),
    ("inventory", -0.5),
    ("checklist", -0.45),
    ("glossary", -0.55),
    ("reference", -0.4),
    ("footnote", -0.5),
    ("disclaimer", -0.5),
    // — connective / procedural —
    ("respectively", -0.5),
    ("additionally", -0.4),
    ("furthermore", -0.35),
    ("moreover", -0.35),
    ("however", -0.25),
    ("therefore", -0.25),
    ("accordingly", -0.35),
    ("subsequently", -0.3),
    ("consequently", -0.25),
    ("notwithstanding", -0.35),
    // — neutral descriptors (from intensity_phases setup/avoid vocab) —
    ("typical", -0.35),
    ("standard", -0.35),
    ("routine", -0.45),
    ("ordinary", -0.4),
    ("general", -0.3),
    ("baseline", -0.4),
    ("nominal", -0.4),
    ("unchanged", -0.5),
    ("stable", -0.4),
    ("steady", -0.35),
    ("maintain", -0.35),
    ("ongoing", -0.3),
    ("existing", -0.3),
    ("current", -0.25),
    // — list / table scaffolding —
    ("item", -0.35),
    ("list", -0.4),
    ("table", -0.35),
    ("column", -0.4),
    ("section", -0.35),
    ("figure", -0.3),
    // — hedging —
    ("perhaps", -0.35),
    ("maybe", -0.35),
    ("possibly", -0.3),
    ("approximately", -0.3),
    ("roughly", -0.25),
    ("somewhat", -0.35),
    ("slightly", -0.3),
    ("minor", -0.35),
    // — administrivia —
    ("schedule", -0.3),
    ("logistics", -0.4),
    ("housekeeping", -0.5),
    ("miscellaneous", -0.5),
];

/// Low-arousal whole words: short connectives and abbreviations.
const DAMPENER_WORDS: [(&str, f64); 11] = [
    ("etc", -0.4),
    ("ie", -0.35),
    ("eg", -0.35),
    ("na", -0.35), // "N/A"
    ("tbd", -0.35),
    ("fyi", -0.4),
    ("note", -0.2),
    ("notes", -0.2),
    ("row", -0.3),
    ("rows", -0.3),
    ("q&a", -0.4),
];

/// Verbs that open a rallying sentence ("Move now", "Seize the moment"): a closed set.
const IMPERATIVE_OPENERS: [&str; 39] = [
    "act",
    "move",
    "seize",
    "stop",
    "start",
    "fight",
    "win",
    "build",
    "launch",
    "decide",
    "choose",
    "imagine",
    "consider",
    "remember",
    "join",
    "lead",
    "push",
    "demand",
    "embrace",
    "ignite",
    "transform",
    "reimagine",
    "ask",
    "look",
    "listen",
    "go",
    "drive",
    "rise",
    "rally",
    "commit",
    "double",
    "ship",
    "deliver",
    "make",
    "take",
    "break",
    "challenge",
    "beat",
    "own",
];

/// The audience addressed.
const SECOND_PERSON: [&str; 5] = ["you", "your", "youre", "yours", "yourself"];

/// Superlatives as whole words; a token of six letters or more ending in "est" is one too.
const SUPERLATIVE_WORDS: [&str; 12] = [
    "most",
    "never",
    "always",
    "only",
    "every",
    "none",
    "nothing",
    "everything",
    "ultimate",
    "unrivaled",
    "unrivalled",
    "unmatched",
];

/// Words of magnitude, beside `%`, a `$` figure, and a multiple ("10x").
const MAGNITUDE_WORDS: [&str; 13] = [
    "double",
    "doubled",
    "triple",
    "tripled",
    "quadruple",
    "half",
    "halved",
    "tenfold",
    "billion",
    "billions",
    "trillion",
    "million",
    "millions",
];

/// What each signal weighs (Vzestup's `W`): the lexicons' sums per token, each shape of the
/// prose per token, the neutral baseline, and the logistic's slope.
struct Weights {
    lex_arousal_per_tok: f64,
    lex_dampener_per_tok: f64,
    exclamation: f64,
    question: f64,
    all_caps: f64,
    imperative: f64,
    second_person: f64,
    superlative: f64,
    magnitude: f64,
    /// Where signal-free prose lands: moderate, not flat.
    baseline: f64,
    sigmoid_slope: f64,
}

const W: Weights = Weights {
    lex_arousal_per_tok: 4.0,
    lex_dampener_per_tok: 3.5,
    exclamation: 3.0,
    question: 1.0,
    all_caps: 3.5,
    imperative: 2.2,
    second_person: 1.2,
    superlative: 1.6,
    magnitude: 2.0,
    baseline: 0.42,
    sigmoid_slope: 3.2,
};

/// The shapes of a slide's prose, counted (`StructuralSignals`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Shape {
    exclamations: usize,
    questions: usize,
    /// Tokens of three or more ASCII letters and digits, all capitals.
    all_caps: usize,
    /// Sentences that open on one of [`IMPERATIVE_OPENERS`].
    imperatives: usize,
    second_person: usize,
    superlatives: usize,
    magnitudes: usize,
}

/// The most of each shape a slide is counted for (`FEATURE_CAP`), so no one shape runs away
/// with the score.
const FEATURE_CAP: Shape = Shape {
    exclamations: 3,
    questions: 3,
    all_caps: 4,
    imperatives: 2,
    second_person: 4,
    superlatives: 4,
    magnitudes: 5,
};

/// `$` before a figure, and a multiple ("10x", "2.5 X"), as Vzestup's regexes find them:
/// JavaScript's `\s`, ASCII `\b` and `\d`.
fn figures() -> &'static (Regex, Regex) {
    static FIGURES: OnceLock<(Regex, Regex)> = OnceLock::new();
    FIGURES.get_or_init(|| {
        let space = js::SPACE;
        let dollars = Regex::new(&format!(r"\${space}?\d")).expect("a figure");
        let multiples = Regex::new(&format!(r"\b\d+(?:\.\d+)?{space}?[xX]\b")).expect("a figure");
        (dollars, multiples)
    })
}

/// The shapes of `text`, whose tokens are `tokens` (`scanStructural`).
fn shape(text: &str, tokens: &[String]) -> Shape {
    let all_caps = (text.split(|c: char| !c.is_ascii_alphanumeric()))
        .filter(|t| {
            t.len() >= 3 && t.bytes().any(|b| b.is_ascii_uppercase()) && !t.bytes().any(|b| b.is_ascii_lowercase())
        })
        .count();
    let imperatives = (text.split(['.', '!', '?']))
        .filter(|sentence| {
            let lowered = js::trim(sentence).to_lowercase();
            let end = lowered.find(|c: char| !c.is_ascii_lowercase()).unwrap_or(lowered.len());
            end > 0 && IMPERATIVE_OPENERS.contains(&&lowered[..end])
        })
        .count();
    let among = |set: &[&str]| tokens.iter().filter(|t| set.contains(&t.as_str())).count();
    let superlatives = (tokens.iter())
        .filter(|t| SUPERLATIVE_WORDS.contains(&t.as_str()) || (t.len() >= 6 && t.ends_with("est")))
        .count();
    let (dollars, multiples) = figures();
    Shape {
        exclamations: text.matches('!').count(),
        questions: text.matches('?').count(),
        all_caps,
        imperatives,
        second_person: among(&SECOND_PERSON),
        superlatives,
        magnitudes: text.matches('%').count()
            + dollars.find_iter(text).count()
            + multiples.find_iter(text).count()
            + among(&MAGNITUDE_WORDS),
    }
}

/// The logistic curve, centred on `mid`.
fn logistic(x: f64, mid: f64, slope: f64) -> f64 {
    1.0 / (1.0 + js::exp(-slope * (x - mid)))
}

/// The intensity `text`'s words read at, 0 to 1 (`estimateDetectedIntensity`), `text` a slide's
/// title, body, and notes: none for fewer than three tokens, which carry too little to read,
/// or for text the English lexicons cannot read, in `language` where the deck names one.
pub fn detected(text: &str, language: Option<&str>) -> Option<f64> {
    let tokens = lexicon::tokens(text);
    if tokens.len() < 3 || !lexicon::english(text, language) {
        return None;
    }
    let count = tokens.len() as f64;
    let joined = tokens.join(" ");
    let arousal =
        lexicon::stem_weight(&tokens, &AROUSAL_STEMS) + lexicon::word_weight(&tokens, &joined, &AROUSAL_WORDS);
    // At most zero: dampeners weigh below it.
    let dampener =
        lexicon::stem_weight(&tokens, &DAMPENER_STEMS) + lexicon::word_weight(&tokens, &joined, &DAMPENER_WORDS);
    let s = shape(text, &tokens);
    let cap = |n: usize, most: usize| n.min(most) as f64;
    let structure = (W.exclamation * cap(s.exclamations, FEATURE_CAP.exclamations)
        + W.question * cap(s.questions, FEATURE_CAP.questions)
        + W.all_caps * cap(s.all_caps, FEATURE_CAP.all_caps)
        + W.imperative * cap(s.imperatives, FEATURE_CAP.imperatives)
        + W.second_person * cap(s.second_person, FEATURE_CAP.second_person)
        + W.superlative * cap(s.superlatives, FEATURE_CAP.superlatives)
        + W.magnitude * cap(s.magnitudes, FEATURE_CAP.magnitudes))
        / count;
    // The net deviation from neutral: about 0 for signal-free prose.
    let raw = W.lex_arousal_per_tok * (arousal / count) + W.lex_dampener_per_tok * (dampener / count) + structure;
    let squashed = logistic(raw, 0.0, W.sigmoid_slope);
    // Neutral prose moves to the baseline; the extremes keep theirs.
    let shifted = squashed + (W.baseline - 0.5) * (1.0 - (2.0 * squashed - 1.0).abs());
    Some(js::clamp(shifted, 0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two anchors the gap detector is calibrated against.
    const FLAT_STATUS: &str = "Q3 status update. Items: A, B, C. No changes.";
    const CLIMAX_RALLY: &str =
        "This is the moment. We either move NOW or lose the market \u{2014} $40M is on the table!";

    fn read(text: &str) -> f64 {
        detected(text, None).unwrap_or_else(|| panic!("no reading of {text:?}"))
    }

    #[test]
    fn calibration_bands_are_pinned() {
        assert!(read(FLAT_STATUS) <= 0.35);
        let clinical = "Inventory overview. The following items remain in stock: widgets, gaskets, and bolts respectively. Status: unchanged. Additionally, the summary appendix lists current baseline figures.";
        assert!(read(clinical) <= 0.35);
        assert!(read(CLIMAX_RALLY) >= 0.65);
        let crisis = "This is a crisis. Our revenue is collapsing \u{2014} a 40% plunge in one quarter. We must act now or the company fails. This is the breakthrough moment that decides everything!";
        assert!(read(crisis) >= 0.65);
        let neutral =
            "Our new approach improves the workflow and helps teams collaborate more effectively across departments.";
        let v = read(neutral);
        assert!(v > 0.3 && v < 0.6, "{v}");
        assert!(read(CLIMAX_RALLY) - read(FLAT_STATUS) > 0.4);
    }

    #[test]
    fn too_few_tokens_read_as_nothing() {
        for text in ["", "   \n\t  ", "hello world", "Crisis now!"] {
            assert_eq!(detected(text, None), None, "{text:?}");
        }
        assert!(detected("we shipped the thing", None).is_some_and(f64::is_finite));
    }

    #[test]
    fn readings_are_the_same_every_time_and_within_0_to_1() {
        let text = "This is a crisis. We must act now or we lose everything!";
        let a = read(text);
        assert_eq!(a.to_bits(), read(text).to_bits());
        assert_eq!(a.to_bits(), read(text).to_bits());
        for text in [
            FLAT_STATUS,
            CLIMAX_RALLY,
            "CRISIS! URGENT! MUST ACT NOW! DISASTER! COLLAPSE! WAR! $100M 50% 10x!!!",
            "overview summary appendix agenda status update recap outline inventory",
            "a perfectly ordinary sentence with nothing remarkable about it at all",
        ] {
            let v = read(text);
            assert!((0.0..=1.0).contains(&v), "{text}: {v}");
        }
    }

    #[test]
    fn charged_words_and_marks_never_lower_it() {
        let base = "The team reviewed the quarterly report and discussed the";
        let steps = [
            format!("{base} roadmap for the coming period."),
            format!("{base} critical roadmap for the coming period."),
            format!("{base} critical roadmap for the coming period. This is urgent."),
            format!("{base} critical roadmap for the coming period. This is urgent. We must act now!"),
            format!("{base} critical roadmap for the coming period. This is URGENT. We must act NOW! $40M is at risk!"),
        ];
        let mut previous = f64::NEG_INFINITY;
        for step in &steps {
            let v = read(step);
            assert!(v >= previous - 1e-9, "{step}: {v} after {previous}");
            previous = v;
        }
    }

    #[test]
    fn capitals_and_exclamations_raise_it() {
        let lower = read("we need to move quickly on this important decision today for the team");
        let caps = read("we need to MOVE QUICKLY on this IMPORTANT decision TODAY for the team");
        assert!(caps > lower);
        let calm = read("We are launching the new product line this quarter.");
        let loud = read("We are launching the new product line this quarter!");
        assert!(loud > calm);
    }

    #[test]
    fn the_fixtures_read_procedural_and_charged() {
        let list = "Appendix A: reference table. Section 1 lists items; section 2 lists items; figures and columns follow respectively. Status: unchanged. Additionally, see the summary outline for the standard baseline.";
        assert!(read(list) <= 0.35);
        let cry = "Now is the moment to fight. The market is collapsing and our rivals are surging \u{2014} but we have the breakthrough. We must seize it NOW or lose everything we have built!";
        assert!(read(cry) >= 0.65);
        assert_eq!(DETECTED_INTENSITY_VERSION, 2);
    }

    #[test]
    fn another_language_reads_as_nothing() {
        assert!(detected(CLIMAX_RALLY, None).is_some());
        assert_eq!(detected(CLIMAX_RALLY, Some("Czech")), None);
        assert_eq!(detected(CLIMAX_RALLY, Some("de-DE")), None);
        assert!(detected(CLIMAX_RALLY, Some("en-US")).is_some());
        assert!(detected(CLIMAX_RALLY, Some("English")).is_some());
        let russian = "\u{42d}\u{442}\u{43e} \u{43a}\u{440}\u{438}\u{442}\u{438}\u{447}\u{435}\u{441}\u{43a}\u{438}\u{439} \u{43c}\u{43e}\u{43c}\u{435}\u{43d}\u{442}. \u{41c}\u{44b} \u{434}\u{43e}\u{43b}\u{436}\u{43d}\u{44b} \u{434}\u{435}\u{439}\u{441}\u{442}\u{432}\u{43e}\u{432}\u{430}\u{442}\u{44c} \u{441}\u{435}\u{439}\u{447}\u{430}\u{441} \u{438}\u{43b}\u{438} \u{43f}\u{43e}\u{442}\u{435}\u{440}\u{44f}\u{442}\u{44c} \u{440}\u{44b}\u{43d}\u{43e}\u{43a}.";
        assert_eq!(detected(russian, None), None);
    }

    #[test]
    fn shapes_are_counted_as_vzestup_counts_them() {
        let text = "Act now! You WIN BIG. Ship it?? 10x and 2.5 X growth, $ 5 and $40, 12%. The greatest, most";
        let s = shape(text, &lexicon::tokens(text));
        let want = Shape {
            exclamations: 1,
            questions: 2,
            all_caps: 2,
            imperatives: 2,
            second_person: 1,
            superlatives: 2,
            magnitudes: 5,
        };
        assert_eq!(s, want);
    }
}
