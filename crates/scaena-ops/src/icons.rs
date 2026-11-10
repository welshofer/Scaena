//! The icons that ship (PLAN 2.104): Phosphor 2.1.1's, MIT, every icon but brands, in its
//! regular and fill weights, each path on a view 256 square (`docs/icons/phosphor.json`, written
//! by `scripts/build_icons.py`). An editor finds them by their words and inserts one as a path
//! shape on its view ([`scaena_core::inserts::icon`]); the CLI and the MCP server find them for an
//! agent, which adds one by `add_node`.
//!
//! An icon is found when each word asked begins a word of its name, its tags, or its categories,
//! or, five letters or more, stands inside its name: `arrow up` finds `arrow-up` and
//! `arrow-circle-up`, and `plane` finds `airplane`. Its name asked whole comes first; then the icons whose names say every word,
//! begin with each, or hold each; then those found by their tags ([`rank`]); among each, the
//! shorter name first, then by name. The browser's picker finds them by the same rule
//! (`web/src/icons.ts`).

use scaena_core::inserts::Icon;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const PHOSPHOR: &str = include_str!("../../../docs/icons/phosphor.json");

/// A weight an icon is drawn in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Weight {
    /// Drawn in outline, Phosphor's regular weight.
    #[default]
    Regular,
    /// Drawn solid.
    Fill,
}

#[derive(Deserialize)]
struct Set {
    view: [f64; 4],
    icons: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    name: String,
    tags: Vec<String>,
    categories: Vec<String>,
    regular: String,
    fill: String,
}

/// The most icons an agent's `icon_find` returns at once (lift-up 20261025c SEC-3): a call is a
/// bounded read, whatever `limit` asks.
pub const MOST: usize = 50;

/// An icon found: its name, its tags and categories, and the icon to insert in the weight asked,
/// left out where only names were asked for ([`Icons::names_only`]).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Found {
    pub name: String,
    pub tags: Vec<String>,
    pub categories: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
}

/// What `scaena icons` prints and `icon_find` returns: the icons found, best first, at most as
/// many as asked, and how many the words found in all.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Icons {
    pub icons: Vec<Found>,
    pub found: usize,
}

fn set() -> &'static Set {
    static SET: OnceLock<Set> = OnceLock::new();
    SET.get_or_init(|| {
        serde_json::from_str(PHOSPHOR).expect("docs/icons/phosphor.json is the set build_icons.py writes")
    })
}

/// How many icons ship.
pub fn count() -> usize {
    set().icons.len()
}

/// `text`'s words, lowercase: what is not a letter or a digit parts them.
fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase)
}

/// How well an icon named `name`, tagged `tags`, in `categories` answers `asked` (each word
/// lowercase), best 0: its name asked whole; then each word a word of its name, each word
/// beginning one, each word of five letters or more inside one (`plane` in `airplane`); then
/// each word a word of its name, tags, or categories, and each word beginning one. None where a
/// word is in none.
pub fn rank(asked: &[String], name: &str, tags: &[String], categories: &[String]) -> Option<u8> {
    if asked.is_empty() || asked.join("-") == name {
        return Some(0);
    }
    let all = |pool: &[String], hit: fn(&str, &str) -> bool| asked.iter().all(|w| pool.iter().any(|n| hit(n, w)));
    let exact: fn(&str, &str) -> bool = |n, w| n == w;
    let begins: fn(&str, &str) -> bool = |n, w| n.starts_with(w);
    // Inside a name, a word of five letters or more: `plane` in `airplane`, not `team` in `steam`.
    let inside: fn(&str, &str) -> bool = |n, w| w.chars().count() >= 5 && n.contains(w);
    let mut every: Vec<String> = words(name).collect();
    if all(&every, exact) {
        return Some(1);
    }
    if all(&every, begins) {
        return Some(2);
    }
    if all(&every, inside) {
        return Some(3);
    }
    every.extend(tags.iter().chain(categories).flat_map(|t| words(t)));
    if all(&every, exact) {
        return Some(4);
    }
    all(&every, begins).then_some(5)
}

/// The icons `query` finds, best first, at most `limit`, each in `weight`; every icon, by name,
/// for no words.
pub fn find(query: &str, weight: Weight, limit: usize) -> Icons {
    let set = set();
    let asked: Vec<String> = words(query).collect();
    let mut found: Vec<(u8, &Entry)> =
        set.icons.iter().filter_map(|e| Some((rank(&asked, &e.name, &e.tags, &e.categories)?, e))).collect();
    // Every icon, for no words, by name; else the best first, the shorter name first among them.
    let long = |e: &Entry| if asked.is_empty() { 0 } else { e.name.len() };
    scaena_core::sort::by(&mut found, |(r, a), (s, b)| (r, long(a), &a.name).cmp(&(s, long(b), &b.name)));
    let all = found.len();
    let icons = found.into_iter().take(limit).map(|(_, e)| Found {
        name: e.name.clone(),
        tags: e.tags.clone(),
        categories: e.categories.clone(),
        icon: Some(Icon {
            name: e.name.clone(),
            path: match weight {
                Weight::Regular => e.regular.clone(),
                Weight::Fill => e.fill.clone(),
            },
            view: set.view,
        }),
    });
    Icons { icons: icons.collect(), found: all }
}

impl Icons {
    /// The icons by their names, tags, and categories alone, without the path each is drawn
    /// from: what an agent's first search reads, which a search for the one chosen, with its
    /// path, follows (lift-up 20261025c SEC-3).
    pub fn names_only(mut self) -> Self {
        for found in &mut self.icons {
            found.icon = None;
        }
        self
    }
}

/// The icon named `name` in `weight`, if one ships.
pub fn named(name: &str, weight: Weight) -> Option<Icon> {
    let set = set();
    let e = set.icons.iter().find(|e| e.name == name)?;
    let path = match weight {
        Weight::Regular => e.regular.clone(),
        Weight::Fill => e.fill.clone(),
    };
    Some(Icon { name: e.name.clone(), path, view: set.view })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_set_ships_whole_and_each_icon_draws() {
        assert!(count() > 1400, "{}", count());
        let theme: scaena_core::model::theme::Theme =
            serde_json::from_str(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
        let deck = scaena_core::Deck::from_json(include_str!("../../../docs/examples/revenue.deck.json")).unwrap();
        let every = find("", Weight::Regular, usize::MAX);
        assert_eq!((every.icons.len(), every.found), (count(), count()));
        for found in every.icons.into_iter().step_by(97) {
            let insert = scaena_core::inserts::icon(
                [deck.canvas.width, deck.canvas.height],
                &theme,
                found.icon.as_ref().unwrap(),
            );
            // A view in whole numbers is written whole, as the set writes it (PLAN G.23).
            assert_eq!(insert.node["view"], serde_json::json!([0, 0, 256, 256]));
            let path = &found.icon.as_ref().unwrap().path;
            assert!(kurbo_like(path), "{}: {}", found.name, &path[..40]);
        }
    }

    /// Absolute moves, lines, cubics, and closes alone, as `build_icons.py` writes them.
    fn kurbo_like(d: &str) -> bool {
        d.starts_with('M') && d.chars().all(|c| "MLCZ0123456789.,-".contains(c))
    }

    #[test]
    fn an_icon_is_found_by_its_name_first_then_by_its_tags() {
        let names = |q: &str| find(q, Weight::Regular, 6).icons.into_iter().map(|f| f.name).collect::<Vec<_>>();
        // The name asked whole comes first; then names that hold each word, shorter first.
        let up = names("arrow up");
        assert_eq!(up[0], "arrow-up", "{up:?}");
        assert!(up.iter().all(|n| n.contains("arrow") && n.contains("up")), "{up:?}");
        // A word begins a word: `plane` finds the airplanes by their names.
        assert!(names("plane").iter().any(|n| n.starts_with("airplane")), "{:?}", names("plane"));
        // A tag finds what its name does not say: a savings acorn.
        assert!(find("savings", Weight::Regular, usize::MAX).icons.iter().any(|f| f.name == "acorn"));
        // At most as many as asked, and how many were found.
        let arrows = find("arrow", Weight::Regular, 5);
        assert!(arrows.icons.len() == 5 && arrows.found > 50, "{}", arrows.found);
        // Brands do not ship, and a word nothing begins finds nothing.
        assert!(named("github-logo", Weight::Regular).is_none());
        assert!(names("zzqx").is_empty());
        // A short word is not found inside a name: `team` finds people, not a steaming bowl.
        assert_eq!(names("team")[0], "users", "{:?}", names("team"));
    }

    #[test]
    fn each_weight_is_drawn_apart_on_one_view() {
        let (regular, fill) = (named("heart", Weight::Regular).unwrap(), named("heart", Weight::Fill).unwrap());
        assert_ne!(regular.path, fill.path);
        assert_eq!((regular.view, fill.view), ([0.0, 0.0, 256.0, 256.0], [0.0, 0.0, 256.0, 256.0]));
        assert_eq!(serde_json::to_value(Weight::Fill).unwrap(), "fill");
    }
}
