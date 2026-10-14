//! A slide started from a layout of the theme (PLAN 3.30): every layout of every shipped theme,
//! started on a new deck, puts its words in its slots, and lint finds nothing in it.

use scaena_ops::create::{Create, create};
use scaena_ops::lint::{View, lint_state_in};
use scaena_ops::states::starting;
use std::path::{Path, PathBuf};

/// The shipped themes, by name: Dusk, its light twin Daybreak, Ember, and Gallery.
const SHIPPED: [(&str, &str); 4] = [
    ("dusk", "../../docs/examples/themes/dusk.theme.json"),
    ("daybreak", "../../docs/examples/authorability/themes/daybreak.theme.json"),
    ("ember", "../../docs/examples/themes/ember.theme.json"),
    ("gallery", "../../docs/examples/themes/gallery.theme.json"),
];

/// A new bundle in `theme`, titled, at a place of its own.
fn made(name: &str, theme: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("starting-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    let req = Create { theme: theme.into(), title: Some("A library".into()), ..Create::default() };
    assert!(create(&dir, &req).unwrap().created, "{name} makes a deck");
    dir
}

#[test]
fn every_shipped_layout_starts_a_slide_that_lint_finds_nothing_in() {
    // Every layout's findings in every theme, said at once.
    let mut wrong: Vec<String> = Vec::new();
    for (name, theme) in SHIPPED {
        let b = scaena_ops::open(&made(name, theme)).unwrap();
        let t = scaena_ops::theme(&b).unwrap();
        let files = View::of(&b);
        let first = b.deck.states[0].id.clone();
        assert!(t.layouts.len() >= 25, "{name} has its library: {}", t.layouts.len());
        for (layout, template) in &t.layouts {
            let started = starting(&b.deck, &t, &first, Some(layout)).unwrap();
            let doc = scaena_core::patch::compile(&b.deck.to_value().unwrap(), &started.patch, &files)
                .unwrap_or_else(|e| panic!("{name}'s {layout}: {e:?}"))
                .doc;
            let deck = scaena_core::Deck::from_value(&doc).unwrap();
            // A text in each slot that holds words, a shape in each that holds one, and nothing in
            // the slots that wait for a picture or a figure.
            use scaena_core::model::theme::Prompt;
            let made = |s: &scaena_core::model::theme::Slot| match &s.prompt {
                Some(Prompt::Shape(_)) => true,
                Some(_) => s.role.is_some(),
                None => false,
            };
            let filled = template.slots.values().filter(|s| made(s)).count();
            let shown = deck.states.iter().find(|s| s.id == started.id).unwrap();
            assert_eq!(shown.props.len(), filled, "{name}'s {layout}: {:?}", shown.props.keys());
            assert!(template.slots.values().any(|s| s.prompt.is_some()), "{name}'s {layout} says what goes in it");
            let found = lint_state_in(&deck, &files, &started.id).unwrap().findings;
            for f in found.iter().filter(|f| f.state.as_deref() == Some(started.id.as_str())) {
                wrong.push(format!("{name}'s {layout}: {} {}", f.code, f.message));
            }
        }
    }
    assert!(wrong.is_empty(), "lint finds {} things:\n{}", wrong.len(), wrong.join("\n"));
}

/// What each layout's picture shows (PLAN 3.31): every layout of every shipped theme draws as the
/// slide it starts, with what each waiting slot's `example` says there: a shader in a picture's
/// place, under the cards and the words, each with a seed of its own; a chart or a table of rows
/// made up for it, written inline. It compiles on a new deck, as an editor draws it, and the deck it
/// draws on keeps none of it.
#[test]
fn every_shipped_layout_draws_its_example_in_each_slot_that_waits() {
    use scaena_core::model::theme::Example;
    let mut drawn = 0;
    for (name, theme) in SHIPPED {
        let b = scaena_ops::open(&made(&format!("{name}-examples"), theme)).unwrap();
        let t = scaena_ops::theme(&b).unwrap();
        let files = View::of(&b);
        let first = b.deck.states[0].id.clone();
        for (layout, template) in &t.layouts {
            let shown = scaena_ops::states::example(&b.deck, &t, &first, layout).unwrap();
            let doc = scaena_core::patch::compile(&b.deck.to_value().unwrap(), &shown.patch, &files)
                .unwrap_or_else(|e| panic!("{name}'s {layout}: {e:?}"))
                .doc;
            let deck = scaena_core::Deck::from_value(&doc).unwrap();
            let mut seeds = Vec::new();
            for (slot, s) in &template.slots {
                let Some(example) = &s.example else { continue };
                let node = deck
                    .nodes
                    .values()
                    .map(|n| serde_json::to_value(n).unwrap())
                    .find(|n| {
                        n["at"]["in"] == *slot && ["shader", "chart", "table"].contains(&n["type"].as_str().unwrap())
                    })
                    .unwrap_or_else(|| panic!("{name}'s {layout}: nothing in `{slot}`"));
                match example {
                    Example::Picture(preset) => {
                        assert_eq!(
                            (node["type"].as_str(), node["preset"].as_str()),
                            (Some("shader"), Some(preset.as_str()))
                        );
                        assert_eq!(node["z"], -2, "{name}'s {layout}: `{slot}` lies under the cards and the words");
                        seeds.push(node["seed"].as_u64().unwrap());
                    }
                    Example::Chart(kind) => {
                        assert_eq!(node["type"], "chart");
                        assert_eq!(node["kind"], serde_json::to_value(kind).unwrap());
                    }
                    Example::Table(_) => assert_eq!(node["type"], "table"),
                }
                drawn += 1;
            }
            let distinct: std::collections::BTreeSet<_> = seeds.iter().collect();
            assert_eq!(distinct.len(), seeds.len(), "{name}'s {layout}: each picture its own seed");
        }
        assert!(b.deck.data.is_empty(), "{name}'s deck keeps none of the rows made up for a picture");
    }
    assert!(drawn > 150, "examples drawn: {drawn}");
}

/// A title slide's byline (its `kicker`) stands under the title and the line on what the talk is
/// about, or along the top, in every shipped theme: never under the picture beside them, where it
/// reads as the picture's caption.
#[test]
fn a_title_slides_byline_is_no_caption_of_its_picture() {
    for (name, theme) in SHIPPED {
        let t: scaena_core::model::theme::Theme =
            serde_json::from_str(&std::fs::read_to_string(theme).unwrap()).unwrap();
        let slots = &t.layouts["title"].slots;
        let span = |slot: &str| -> [u64; 2] {
            let col = serde_json::to_value(slots[slot].col).unwrap();
            match col {
                serde_json::Value::Array(ab) => [ab[0].as_u64().unwrap(), ab[1].as_u64().unwrap()],
                one => [one.as_u64().unwrap(); 2],
            }
        };
        let (kicker, art, title) = (span("kicker"), span("art"), span("title"));
        assert!(kicker[1] < art[0] || kicker[0] > art[1], "{name}: the byline {kicker:?} under the picture {art:?}");
        assert_eq!(kicker[0], title[0], "{name}: the byline starts where the title does");
    }
}
