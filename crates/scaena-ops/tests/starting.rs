//! A slide started from a layout of the theme (PLAN 3.30): every layout of every shipped theme,
//! started on a new deck, puts its words in its slots, and lint finds nothing in it.

use scaena_ops::create::{Create, create};
use scaena_ops::lint::{View, lint_state_in};
use scaena_ops::states::starting;
use std::path::{Path, PathBuf};

/// The shipped themes, by name: Dusk, its light twin Daybreak, and Ember.
const SHIPPED: [(&str, &str); 3] = [
    ("dusk", "../../docs/examples/themes/dusk.theme.json"),
    ("daybreak", "../../docs/examples/authorability/themes/daybreak.theme.json"),
    ("ember", "../../docs/examples/themes/ember.theme.json"),
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
            let here: Vec<_> = found.iter().filter(|f| f.state.as_deref() == Some(started.id.as_str())).collect();
            assert!(here.is_empty(), "{name}'s {layout}: {here:#?}");
        }
    }
}
