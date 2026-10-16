//! A haiku placed in one patch (PLAN O.4d): a new Haiku slide in every shipped theme, linting
//! clean, its notes naming the model and the date while nothing on the slide does; the slide's
//! empty verse place; and a verse selected, replaced with its runs kept.

use scaena_core::writing::haiku::About;
use scaena_ops::create::{Create, create};
use scaena_ops::haiku::{Place, Writer, credited, placing};
use scaena_ops::lint::{View, lint_state_in};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const SHIPPED: [(&str, &str); 4] = [
    ("dusk", "../../docs/examples/themes/dusk.theme.json"),
    ("daybreak", "../../docs/examples/authorability/themes/daybreak.theme.json"),
    ("ember", "../../docs/examples/themes/ember.theme.json"),
    ("gallery", "../../docs/examples/themes/gallery.theme.json"),
];

const POEM: [&str; 3] = ["Winter rain on glass", "the projector hums along", "slides wait in the dark"];

fn made(name: &str, theme: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("haiku-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    let req = Create { theme: theme.into(), title: Some("A talk".into()), ..Create::default() };
    assert!(create(&dir, &req).unwrap().created, "{name} makes a deck");
    dir
}

fn lines() -> Vec<String> {
    POEM.iter().map(|l| l.to_string()).collect()
}

fn by() -> Writer {
    Writer { model: "test-model-1".into(), provider: Some("Anthropic".into()) }
}

/// The deck `patch` makes of `deck`.
fn applied(deck: &scaena_core::Deck, files: &View, patch: &[Value]) -> scaena_core::Deck {
    let doc =
        scaena_core::patch::compile(&deck.to_value().unwrap(), patch, files).unwrap_or_else(|e| panic!("{e:?}")).doc;
    scaena_core::Deck::from_value(&doc).unwrap()
}

#[test]
fn a_haiku_slide_in_one_patch_names_its_model_in_the_notes_alone() {
    let about = About::Topic { topic: "a rainy keynote".into() };
    let credit = credited(&by(), "October 15, 2026", &about);
    assert_eq!(
        credit,
        "Haiku written by test-model-1 with your Anthropic key on October 15, 2026, about a rainy keynote."
    );
    for (name, theme) in SHIPPED {
        let b = scaena_ops::open(&made(name, theme)).unwrap();
        let t = scaena_ops::theme(&b).unwrap();
        let files = View::of(&b);
        let first = b.deck.states[0].id.clone();
        for attribution in [Some("after Bashō"), None] {
            let placed = placing(&b.deck, &t, &first, &lines(), attribution, &Place::New, &credit).unwrap();
            let deck = applied(&b.deck, &files, &placed.patch);
            let state = deck.states.iter().find(|s| s.id == placed.state).unwrap();
            assert_eq!(state.layout.as_deref(), Some("haiku"), "{name}");
            assert_eq!(state.notes.as_deref(), Some(credit.as_str()), "{name}");
            assert_eq!(deck.nodes[&placed.node].props["text"], json!(POEM.join("\n")), "{name}");
            // The attribution is what the person gave, or waits empty: never filled in for them.
            let said: Vec<&str> =
                state.props.keys().filter_map(|id| deck.nodes[id].props.get("text")?.as_str()).collect();
            assert_eq!(said.contains(&"after Bashō"), attribution.is_some(), "{name}: {said:?}");
            assert!(!said.iter().any(|t| t.contains("Who wrote it")), "{name}: {said:?}");
            // Nothing on the slide names the model.
            let shown = serde_json::to_string(&deck.nodes).unwrap();
            assert!(!shown.contains("test-model-1"), "{name}");
            let found = lint_state_in(&deck, &files, &placed.state).unwrap().findings;
            let errors: Vec<_> = found
                .iter()
                .filter(|f| {
                    f.state.as_deref() == Some(placed.state.as_str())
                        && f.severity == scaena_core::lint::Severity::Error
                })
                .map(|f| format!("{} {}", f.code, f.message))
                .collect();
            assert!(errors.is_empty(), "{name}: {errors:?}");
        }
    }
}

#[test]
fn a_haiku_fills_the_slides_empty_verse_place_or_replaces_the_verse_selected() {
    let (_, theme) = SHIPPED[0];
    let b = scaena_ops::open(&made("places", theme)).unwrap();
    let t = scaena_ops::theme(&b).unwrap();
    let files = View::of(&b);
    let first = b.deck.states[0].id.clone();
    let credit = "Haiku written by test-model-1 with your Anthropic key on today, about this slide.";
    // A Haiku slide whose poem was taken away: its verse place waits.
    let started = scaena_ops::states::starting(&b.deck, &t, &first, Some("haiku")).unwrap();
    let kept: Vec<Value> =
        started.patch.into_iter().filter(|op| !(op["op"] == "add_node" && op["node"]["at"]["in"] == "poem")).collect();
    let deck = applied(&b.deck, &files, &kept);
    let placed = placing(&deck, &t, &started.id, &lines(), None, &Place::Slot { slot: "poem".into() }, credit).unwrap();
    let filled = applied(&deck, &files, &placed.patch);
    assert_eq!(filled.nodes[&placed.node].props["text"], json!(POEM.join("\n")));
    assert_eq!(filled.nodes[&placed.node].props["at"]["in"], "poem");
    assert_eq!(filled.states.iter().find(|s| s.id == started.id).unwrap().notes.as_deref(), Some(credit));

    // A verse selected, its first word in a look of its own: replaced by the least changes, the
    // look kept on the word the new poem keeps.
    let mut runs = filled.clone();
    let node = runs.nodes.get_mut(&placed.node).unwrap();
    node.props.shift_remove("text");
    node.props.insert(
        "runs".into(),
        json!([{ "text": "Winter", "emphasis": "high" }, { "text": " rain on glass\nthe projector hums along\nslides wait in the dark" }]),
    );
    let again = ["Winter rain on stone", "the projector hums along", "slides wait in the dark"].map(String::from);
    let over =
        placing(&runs, &t, &started.id, &again, None, &Place::Over { node: placed.node.clone() }, credit).unwrap();
    assert!(over.patch.iter().any(|op| op["op"] == "replace_text"));
    let replaced = applied(&runs, &files, &over.patch);
    let after = &replaced.nodes[&placed.node].props["runs"];
    assert_eq!(after[0], json!({ "text": "Winter", "emphasis": "high" }), "{after}");
    let words: String = after.as_array().unwrap().iter().map(|r| r["text"].as_str().unwrap()).collect();
    assert_eq!(words, again.join("\n"));
    // The notes keep what they said, the credit after them.
    let notes = replaced.states.iter().find(|s| s.id == started.id).unwrap().notes.clone().unwrap();
    assert_eq!(notes, format!("{credit}\n\n{credit}"));
}
