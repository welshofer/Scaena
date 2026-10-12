//! How the estimators read words, as both of Vzestup's do (`detectedIntensity.ts`,
//! `detectedValence.ts`): a text's tokens, a lexicon's stems and words found in them, and
//! whether lexicons of English in Latin letters can read the text at all.

use crate::js;

/// A lexicon: each stem, word, or phrase and its weight, in the order they are tried.
pub type Lexicon = [(&'static str, f64)];

/// The tokens of `text` (`tokenize`): lowercased, split at anything but ASCII letters, digits,
/// `%`, `$`, `&`, and `-`, so "game-changer" and "q&a" stay whole; a lone hyphen is none.
pub fn tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !matches!(c, 'a'..='z' | '0'..='9' | '%' | '$' | '&' | '-'))
        .filter(|t| !t.is_empty() && *t != "-")
        .map(str::to_string)
        .collect()
}

/// Whether `token` is `stem`'s (`tokenMatchesStem`): the stem itself, or a word a stem of four
/// letters or more begins.
fn of_stem(token: &str, stem: &str) -> bool {
    token == stem || (stem.len() >= 4 && token.starts_with(stem))
}

/// What `stems` weigh in `tokens` (`countStemHits`): each token its first stem's weight, in
/// the lexicon's order.
pub fn stem_weight(tokens: &[String], stems: &Lexicon) -> f64 {
    let mut sum = 0.0;
    for t in tokens {
        if let Some((_, w)) = stems.iter().find(|(stem, _)| of_stem(t, stem)) {
            sum += w;
        }
    }
    sum
}

/// What `words` weigh in `tokens` (`countWordHits`): each token that is one, then each phrase
/// of several words each time it runs in `joined`, the tokens a space apart.
pub fn word_weight(tokens: &[String], joined: &str, words: &Lexicon) -> f64 {
    let mut sum = 0.0;
    for t in tokens {
        if let Some((_, w)) = words.iter().find(|(word, _)| word == t) {
            sum += w;
        }
    }
    let hay = format!(" {joined} ");
    for (phrase, w) in words.iter().filter(|(phrase, _)| phrase.contains(' ')) {
        let needle = format!(" {phrase} ");
        let mut from = 0;
        while let Some(at) = hay[from..].find(&needle) {
            sum += w;
            // The space after one match may begin the next.
            from += at + needle.len() - 1;
        }
    }
    sum
}

/// Whether the English lexicons can read `text` (`isEnglishLexiconApplicable`): not where the
/// deck's `language` is another, by its name or its tag's first part; nor where, of eight
/// letters or more, most are of a script that is not Latin.
pub fn english(text: &str, language: Option<&str>) -> bool {
    if let Some(language) = language.map(js::trim).filter(|l| !l.is_empty()) {
        let name = language.to_lowercase();
        let head = name.split(['-', '_', ' ']).next().unwrap_or_default();
        let en = |t: &str| matches!(t, "en" | "eng" | "english");
        if !en(&name) && !en(head) {
            return false;
        }
    }
    let (mut latin, mut other) = (0usize, 0usize);
    for c in text.chars() {
        match c as u32 {
            // ASCII letters, Latin-1, and Latin Extended-A and B.
            0x41..=0x5A | 0x61..=0x7A | 0xC0..=0x24F => latin += 1,
            // Greek, Cyrillic, Hebrew, Arabic, Devanagari, Thai, kana, CJK, and Hangul.
            0x370..=0x3FF
            | 0x400..=0x4FF
            | 0x590..=0x5FF
            | 0x600..=0x6FF
            | 0x900..=0x97F
            | 0xE00..=0xE7F
            | 0x3040..=0x30FF
            | 0x3400..=0x9FFF
            | 0xAC00..=0xD7A3 => other += 1,
            _ => {}
        }
    }
    !(latin + other >= 8 && other > latin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_keep_hyphens_ampersands_and_figures() {
        assert_eq!(tokens("Game-changer: Q&A — $40M, 10x - done!"), ["game-changer", "q&a", "$40m", "10x", "done"]);
        // The Kelvin sign lowercases to an ASCII k, and İ to an i and a dot that splits.
        assert_eq!(tokens("\u{212A}ick \u{130}t"), ["kick", "i", "t"]);
        assert!(tokens(" \u{2014} ").is_empty());
    }

    #[test]
    fn a_stem_matches_the_words_it_begins_and_a_word_only_itself() {
        const STEMS: [(&str, f64); 3] = [("summary", -0.5), ("summari", -0.45), ("war", 0.7)];
        let t = tokens("summary summarizes warfare war");
        // "warfare" is not "war"'s: a stem under four letters matches only itself.
        assert_eq!(stem_weight(&t, &STEMS), -0.5 - 0.45 + 0.7);
        const WORDS: [(&str, f64); 2] = [("now", 0.7), ("act now", 0.9)];
        let t = tokens("act now act now nowhere");
        assert_eq!(word_weight(&t, &t.join(" "), &WORDS), 0.7 + 0.7 + 0.9 + 0.9);
    }

    #[test]
    fn english_is_read_from_the_language_named_and_the_script() {
        assert!(english("words", Some(" en-US ")) && english("words", Some("English")) && english("words", None));
        assert!(!english("words", Some("Czech")) && !english("words", Some("de_DE")));
        assert!(!english("Это критический момент", None));
        // Too few letters to judge the script by.
        assert!(english("Это", None));
    }
}
