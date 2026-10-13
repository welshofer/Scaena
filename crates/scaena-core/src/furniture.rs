//! A slide's furniture (ADR-0028): its number and its footer, laid out where the theme places
//! them, on each slide the deck shows them on. They are no nodes of the deck: a scene draws them
//! over its nodes, nothing selects, hits, or lists them, and a reader passes over them.

use crate::Deck;
use crate::model::theme::Furniture;

/// The id the slide's number draws under in a display list: no node's, since a node's id is a
/// slug, so a reader of the display list tells it from a node's (a tagged PDF marks it a page
/// number, an artifact).
pub const NUMBER: &str = ":number";

/// The id the deck's footer draws under, as [`NUMBER`] is the number's.
pub const FOOTER: &str = ":footer";

/// Whether a display list's layer `id` draws a slide's furniture rather than a node.
pub fn is_furniture(id: &str) -> bool {
    id.starts_with(':')
}

/// Where `state` stands among the deck's slides, as its furniture numbers it: the number of its
/// slide from 1 in the deck's order, how many slides the deck has, and what its slide's first
/// state says of its furniture (`furniture`), which says it for the whole slide. Every slide
/// counts, whether it shows its number or not.
pub fn slide(deck: &Deck, state: &str) -> Option<Slide> {
    let mut number = 0;
    let mut at = None;
    let mut current: Option<&str> = None;
    let mut shows = None;
    for s in &deck.states {
        let of = deck.slide_of(s);
        if current != Some(of) {
            number += 1;
            current = Some(of);
        }
        if s.id == state {
            at = Some(number);
            // The slide's first state is the one whose id names it; a slide whose first state is
            // missing takes what the state shown says.
            let head = deck.states.iter().find(|h| h.id == of).unwrap_or(s);
            shows = head.furniture;
        }
    }
    Some(Slide { number: at?, of: number, shows })
}

/// A state's slide, as its furniture counts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slide {
    /// Its number, from 1.
    pub number: usize,
    /// How many slides the deck has.
    pub of: usize,
    /// What the slide's first state says: shown, hidden, or as its layout has it.
    pub shows: Option<bool>,
}

impl Slide {
    /// Whether the slide shows its furniture, in a state laid out in `layout`: as its first state
    /// says, else unless the theme leaves it off that layout.
    pub fn shown(&self, furniture: &Furniture, layout: Option<&str>) -> bool {
        self.shows.unwrap_or_else(|| !layout.is_some_and(|l| furniture.except.iter().any(|e| e == l)))
    }

    /// The number as `pattern` writes it: `{n}` the slide's number, `{total}` how many slides the
    /// deck has; `{n}` alone where the theme gives none.
    pub fn written(&self, pattern: Option<&str>) -> String {
        pattern.unwrap_or("{n}").replace("{n}", &self.number.to_string()).replace("{total}", &self.of.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck(states: serde_json::Value) -> Deck {
        let v = serde_json::json!({
            "scaena": crate::FORMAT_VERSION, "canvas": { "width": 1920, "height": 1080 },
            "nodes": {}, "states": states,
        });
        Deck::from_json(&v.to_string()).unwrap()
    }

    /// Slides count in the deck's order, each of a slide's steps its slide's number, and its first
    /// state says for the slide whether it shows its furniture.
    #[test]
    fn a_slide_is_numbered_by_its_place_and_its_steps_share_it() {
        let d = deck(serde_json::json!([
            { "id": "cover", "furniture": false },
            { "id": "one" },
            { "id": "two", "slide": "one" },
            { "id": "three", "furniture": true },
        ]));
        let at = |s: &str| slide(&d, s).unwrap();
        assert_eq!(at("cover"), Slide { number: 1, of: 3, shows: Some(false) });
        assert_eq!(at("one"), Slide { number: 2, of: 3, shows: None });
        assert_eq!(at("two"), Slide { number: 2, of: 3, shows: None }, "a step is its slide's number");
        assert_eq!(at("three").shows, Some(true));
        assert_eq!(slide(&d, "nowhere"), None);
        assert_eq!(at("two").written(Some("{n} / {total}")), "2 / 3");
        assert_eq!(at("three").written(None), "3");

        let furniture: Furniture = serde_json::from_value(serde_json::json!({ "except": ["title"] })).unwrap();
        assert!(!at("cover").shown(&furniture, Some("bullets")), "the slide says it hides them");
        assert!(at("three").shown(&furniture, Some("title")), "the slide says it shows them where the theme would not");
        assert!(!at("one").shown(&furniture, Some("title")), "the theme leaves them off a title");
        assert!(at("one").shown(&furniture, Some("bullets")));
        assert!(at("one").shown(&furniture, None));
    }
}
