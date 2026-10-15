//! Builds by bullet (PLAN 2.129): a list brought in, or taken away, one paragraph at a click, as a
//! presentation app's By Bullet builds one. A click is a state (SPEC §2.2), so each bullet is a
//! step of its slide: the text is laid out whole in each, and its `paragraphs` say which of them
//! the step draws. The paragraphs a step draws anew enter by the text's own entrance, and those it
//! no longer draws leave by its exit, as a chart's marks enter and leave by its presets (§3.9).

use crate::document::Props;

/// The paragraphs of a text as `props` show it that hold words, as indices from 0: what a build by
/// bullet brings in one at a time. A blank paragraph between them comes with the one before it.
pub fn paragraphs(props: &Props) -> Vec<usize> {
    let words = crate::reading::words(props);
    crate::lists::paragraphs(&words)
        .into_iter()
        .enumerate()
        .filter(|(_, range)| words[range.clone()].chars().any(|c| !c.is_whitespace()))
        .map(|(k, _)| k)
        .collect()
}

/// How many paragraphs a text as `props` show it has, a blank one among them.
pub fn count(props: &Props) -> usize {
    crate::lists::paragraphs(&crate::reading::words(props)).len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(t: &str) -> Props {
        Props::from_iter([("text".to_string(), serde_json::Value::from(t))])
    }

    #[test]
    fn the_paragraphs_with_words_are_what_a_build_by_bullet_brings_in() {
        assert_eq!(paragraphs(&text("Plants\nChlorophyll\nWater")), [0, 1, 2]);
        assert_eq!(paragraphs(&text("One\n\n  \nTwo\n")), [0, 3], "a blank paragraph comes with the one before");
        assert_eq!(count(&text("One\n\n  \nTwo\n")), 5);
        assert_eq!(paragraphs(&text("")), Vec::<usize>::new());
    }
}
