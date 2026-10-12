//! The tone a slide's words read in (Vzestup's `detectedValence.ts`), on the −1 to +1 scale a
//! slide declares its own on: grave below zero, hopeful above. A curated lexicon of positive and
//! negative stems and words, per token, through `tanh`; no shape of the prose, no negation, no
//! irony. Separable from intensity by construction: a crisis and a breakthrough read alike in
//! arousal and opposite here. Like intensity, an independent reading, not an unbiased one.

use super::lexicon;
use crate::js;

/// The estimator's version, raised where a change would move what it reads; 2 reads nothing
/// in a language the lexicons cannot.
pub const DETECTED_VALENCE_VERSION: u32 = 2;

/// How steeply the net per token reads as tone: about three strong words in thirty land near
/// ±0.6, and past a net of 0.1 the sign is most of what it says.
const TANH_SLOPE: f64 = 10.0;

// The lexicons, precise rather than broad: ambiguous words ("issue", "record", "challenge")
// are left out. Positive weighs 0.4 to 1, negative −0.4 to −1.

/// Positive stems.
const POSITIVE_STEMS: [(&str, f64); 54] = [
    // — success / achievement —
    ("success", 0.8),
    ("succeed", 0.8),
    ("achiev", 0.7),
    ("accomplish", 0.7),
    ("triumph", 0.85),
    ("victor", 0.8),
    ("excellen", 0.7),
    ("outstanding", 0.75),
    ("exceptional", 0.7),
    ("stellar", 0.75),
    ("superb", 0.8),
    ("impressive", 0.65),
    ("remarkabl", 0.6),
    // — growth / gain —
    ("growth", 0.6),
    ("grow", 0.5),
    ("gain", 0.6),
    ("improv", 0.6),
    ("enhanc", 0.5),
    ("upgrad", 0.5),
    ("benefit", 0.6),
    ("advantag", 0.6),
    ("profit", 0.6),
    ("surplus", 0.6),
    ("boost", 0.55),
    ("rebound", 0.65),
    ("recover", 0.6),
    // — opportunity / momentum —
    ("opportunit", 0.65),
    ("breakthrough", 0.85),
    ("innovat", 0.6),
    ("pioneer", 0.6),
    ("thrive", 0.75),
    ("prosper", 0.75),
    ("flourish", 0.75),
    ("progress", 0.6),
    ("milestone", 0.55),
    ("unlock", 0.6),
    ("empower", 0.7),
    // — sentiment —
    ("delight", 0.85),
    ("excit", 0.6),
    ("thrill", 0.6),
    ("celebrat", 0.8),
    ("proud", 0.7),
    ("confiden", 0.6),
    ("optimis", 0.7),
    ("hope", 0.6),
    ("promis", 0.55),
    ("reward", 0.65),
    // — strength / health —
    ("strength", 0.5),
    ("strong", 0.5),
    ("robust", 0.55),
    ("resilien", 0.6),
    ("healthy", 0.55),
    ("vibrant", 0.6),
    ("premier", 0.5),
];

/// Positive whole words, and phrases.
const POSITIVE_WORDS: [(&str, f64); 12] = [
    ("win", 0.7),
    ("wins", 0.7),
    ("won", 0.6),
    ("gains", 0.6),
    ("best", 0.6),
    ("great", 0.6),
    ("good", 0.5),
    ("ahead", 0.55),
    ("ready", 0.4),
    ("win-win", 0.8),
    ("on track", 0.6),
    ("all-time high", 0.75),
];

/// Negative stems.
const NEGATIVE_STEMS: [(&str, f64); 72] = [
    // — failure / loss —
    ("fail", -0.8),
    ("failur", -0.85),
    ("loss", -0.7),
    ("lose", -0.7),
    ("declin", -0.7),
    ("decreas", -0.55),
    ("shrink", -0.6),
    ("slump", -0.65),
    ("downturn", -0.75),
    ("recession", -0.8),
    ("stagnat", -0.6),
    ("deficit", -0.7),
    ("shortfall", -0.7),
    ("overrun", -0.6),
    // — danger / crisis —
    ("crisis", -0.85),
    ("crise", -0.85),
    ("catastroph", -0.95),
    ("disaster", -0.9),
    ("calamit", -0.9),
    ("emergenc", -0.7),
    ("threat", -0.7),
    ("danger", -0.7),
    ("jeopard", -0.7),
    ("peril", -0.7),
    ("doom", -0.85),
    ("fatal", -0.8),
    // — weakness / vulnerability —
    ("weak", -0.55),
    ("fragil", -0.55),
    ("vulnerab", -0.6),
    ("obsolet", -0.6),
    ("outdated", -0.5),
    // — struggle / harm —
    ("struggl", -0.6),
    ("hardship", -0.7),
    ("obstacle", -0.55),
    ("setback", -0.7),
    ("hurdle", -0.5),
    ("damag", -0.7),
    ("harm", -0.65),
    ("hurt", -0.6),
    // — collapse / insolvency —
    ("collaps", -0.85),
    ("plummet", -0.8),
    ("plunge", -0.75),
    ("bankrupt", -0.85),
    ("insolven", -0.85),
    // — ops failures —
    ("layoff", -0.8),
    ("cutback", -0.7),
    ("downsiz", -0.75),
    ("breach", -0.7),
    ("outage", -0.7),
    ("defect", -0.65),
    ("fault", -0.6),
    ("churn", -0.55),
    ("attrition", -0.6),
    ("delay", -0.55),
    // — sentiment —
    ("problem", -0.6),
    ("concern", -0.5),
    ("worri", -0.6),
    ("anxiet", -0.65),
    ("anxious", -0.65),
    ("dread", -0.75),
    ("panic", -0.8),
    ("frustrat", -0.7),
    ("disappoint", -0.7),
    ("regret", -0.65),
    ("blame", -0.6),
    // — conflict —
    ("dispute", -0.6),
    ("conflict", -0.6),
    ("lawsuit", -0.7),
    ("litigat", -0.65),
    ("penalt", -0.6),
    // — generic negative —
    ("negative", -0.5),
    ("poor", -0.55),
];

/// Negative whole words, and phrases.
const NEGATIVE_WORDS: [(&str, f64); 14] = [
    ("lost", -0.7),
    ("loses", -0.7),
    ("fails", -0.8),
    ("down", -0.45),
    ("behind", -0.55),
    ("worst", -0.7),
    ("worse", -0.6),
    ("bad", -0.55),
    ("risk", -0.5),
    ("risks", -0.5),
    ("miss", -0.5),
    ("missed", -0.55),
    ("at risk", -0.6),
    ("fell short", -0.7),
];

/// The tone `text`'s words read in, −1 to +1 (`estimateDetectedValence`), `text` a slide's
/// title, body, and notes: none for fewer than three tokens, or for text the English lexicons
/// cannot read, in `language` where the deck names one; never a 0 made up for no signal.
pub fn detected(text: &str, language: Option<&str>) -> Option<f64> {
    let tokens = lexicon::tokens(text);
    if tokens.len() < 3 || !lexicon::english(text, language) {
        return None;
    }
    let joined = tokens.join(" ");
    let positive =
        lexicon::stem_weight(&tokens, &POSITIVE_STEMS) + lexicon::word_weight(&tokens, &joined, &POSITIVE_WORDS);
    // At most zero: negative words weigh below it.
    let negative =
        lexicon::stem_weight(&tokens, &NEGATIVE_STEMS) + lexicon::word_weight(&tokens, &joined, &NEGATIVE_WORDS);
    let net_per_tok = (positive + negative) / tokens.len() as f64;
    Some(js::clamp(js::tanh(TANH_SLOPE * net_per_tok), -1.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> f64 {
        detected(text, None).unwrap_or_else(|| panic!("no reading of {text:?}"))
    }

    #[test]
    fn too_few_tokens_read_as_nothing_and_a_reading_is_the_same_every_time() {
        assert_eq!(detected("", None), None);
        assert_eq!(detected("Hi all", None), None);
        let text = "Layoffs and mounting losses pushed us toward bankruptcy.";
        assert_eq!(read(text).to_bits(), read(text).to_bits());
        let v = read("disaster catastrophe collapse bankruptcy crisis doom losses layoffs breach");
        assert!((-1.0..=1.0).contains(&v));
        assert_eq!(DETECTED_VALENCE_VERSION, 2);
    }

    #[test]
    fn hopeful_prose_reads_positive_and_grave_negative() {
        let hopeful = read(
            "Strong growth this quarter \u{2014} a real breakthrough. The team should be proud of an outstanding result.",
        );
        assert!(hopeful > 0.2, "{hopeful}");
        let grave = read(
            "Revenue is collapsing. Layoffs, a security breach, and mounting losses have pushed us toward bankruptcy and disaster.",
        );
        assert!(grave < -0.2, "{grave}");
        let neutral =
            read("This section reviews the standard quarterly items in order, with the usual tables and figures.");
        assert!(neutral.abs() <= 0.2, "{neutral}");
        // High in arousal, and grave: the second axis.
        let crisis =
            read("This is an urgent crisis \u{2014} a severe threat we must confront before it becomes a catastrophe.");
        assert!(crisis < 0.0, "{crisis}");
    }
}
