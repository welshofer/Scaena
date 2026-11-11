//! Builds by bullet (PLAN 2.129): a list brought in, or taken away, one paragraph at a click, as a
//! presentation app's By Bullet builds one. A click is a state (SPEC §2.2), so each bullet is a
//! step of its slide: the text is laid out whole in each, and its `paragraphs` say which of them
//! the step draws. The paragraphs a step draws anew enter by the text's own entrance, and those it
//! no longer draws leave by its exit, as a chart's marks enter and leave by its presets (§3.9).

use crate::document::{Deck, Props};

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

/// Whether `deck`'s `k`th state is a step a build by bullet made: a step of a slide, not its first
/// state, whose own props say which paragraphs a text draws, or that takes away a text the step
/// before it drew in part, as the last step of a build out does.
pub fn made(deck: &Deck, k: usize) -> bool {
    let Some(state) = deck.states.get(k).filter(|s| s.slide.is_some()) else { return false };
    let drawn =
        |s: &crate::document::State, node: &str| s.props.get(node).is_some_and(|p| p.contains_key("paragraphs"));
    let before = k.checked_sub(1).map(|j| &deck.states[j]).filter(|b| deck.slide_of(b) == deck.slide_of(state));
    state.props.keys().any(|node| drawn(state, node))
        || before.is_some_and(|b| state.remove.iter().any(|node| drawn(b, node)))
}

/// The state what is inserted in `state` enters in (lift-up 20261110, editor dogfood 6): its
/// slide's first, where `state` is a step a build by bullet made ([`made`]), so what is added
/// stands on every step of the slide, as an object a presentation app inserts stands on its slide
/// whichever build is shown; else `state`, so what is added on a step a person made enters there,
/// which is how a person builds a slide by editing.
pub fn entering<'a>(deck: &'a Deck, state: &'a str) -> &'a str {
    match deck.state_index(state) {
        Some(k) if made(deck, k) => deck.slide_of(&deck.states[k]),
        _ => state,
    }
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

    #[test]
    fn what_is_inserted_on_a_step_a_build_made_enters_on_its_first() {
        let deck: Deck = serde_json::from_value(serde_json::json!({
            "scaena": crate::FORMAT_VERSION, "canvas": { "width": 1920, "height": 1080 },
            "theme": "dusk",
            "nodes": { "list": { "type": "text", "text": "One\nTwo" }, "note": { "type": "text", "text": "Aside" } },
            "states": [
                { "id": "s", "props": { "list": { "paragraphs": [1, 1] } } },
                { "id": "s-2", "slide": "s", "props": { "list": { "paragraphs": null } } },
                { "id": "s-3", "slide": "s", "props": { "note": { "role": "lede" } } },
                { "id": "t", "props": { "list": { "paragraphs": [1, 2] } } },
                { "id": "t-2", "slide": "t", "remove": ["list"] }
            ]
        }))
        .unwrap();
        let at = |s| entering(&deck, s);
        assert_eq!(at("s-2"), "s", "a step the build made");
        assert_eq!(at("s-3"), "s-3", "a step a person made keeps what enters there");
        assert_eq!(at("s"), "s");
        assert_eq!(at("t-2"), "t", "the step a build out ends on");
        assert_eq!(at("nowhere"), "nowhere");
    }
}
