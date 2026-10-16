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

/// A new bundle in `theme`, titled, at a place of its own, in the fonts the theme ships with
/// (`scaena_ops::shipped`'s, `docs/examples/fonts`): Daybreak's file sits beside the authorability
/// deck's fonts, subset to that deck, and a new deck in Daybreak takes the shipped ones.
fn made(name: &str, theme: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("starting-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    let shipped = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("starting-{name}-theme"));
    let _ = std::fs::remove_dir_all(&shipped);
    std::fs::create_dir_all(shipped.join("themes")).unwrap();
    std::fs::create_dir_all(shipped.join("fonts")).unwrap();
    for font in std::fs::read_dir("../../docs/examples/fonts").unwrap() {
        let font = font.unwrap().path();
        std::fs::copy(&font, shipped.join("fonts").join(font.file_name().unwrap())).unwrap();
    }
    let copy = shipped.join("themes").join(Path::new(theme).file_name().unwrap());
    std::fs::copy(theme, &copy).unwrap();
    let req = Create { theme: copy, title: Some("A library".into()), ..Create::default() };
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
                    Example::Map(preset) => {
                        // A map's place: the picture, then a pin and a label over it (PLAN O.1e).
                        assert_eq!(
                            (node["preset"].as_str(), &node["z"]),
                            (Some(preset.as_str()), &serde_json::json!(-2))
                        );
                        let over: Vec<_> = (deck.nodes.values())
                            .map(|n| serde_json::to_value(n).unwrap())
                            .filter(|n| n["at"]["in"] == *slot && n["type"] != "shader")
                            .collect();
                        assert!(
                            over.iter().any(|n| n["kind"] == "path" && n["fill"] == "accent"),
                            "{name}'s {layout}: a pin"
                        );
                        assert!(
                            over.iter().any(|n| n["type"] == "text" && n["role"] == "label"),
                            "{name}'s {layout}: a label"
                        );
                        seeds.push(node["seed"].as_u64().unwrap());
                    }
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

/// A list's points are read from the back of the room: each shipped theme's list layouts set them
/// in a role larger than its body, as Statements' credo sets its own, so three of them do not sit
/// small in the top of the slot.
#[test]
fn a_lists_points_are_set_larger_than_the_body() {
    for (name, path) in SHIPPED {
        let theme = scaena_core::model::Theme::from_json(&std::fs::read_to_string(path).unwrap()).unwrap();
        let size = |role: &str| theme.typography.roles[role].size;
        for (layout, slot) in
            [("bullets", "body"), ("numbered", "body"), ("bullets-photo", "body"), ("agenda-photo", "body")]
        {
            let role = theme.layouts[layout].slots[slot].role.clone().unwrap();
            assert!(size(&role) > size("body"), "{name}'s {layout} sets its points in {role}, no larger than the body");
        }
    }
}

/// A slide started in a deck that reads right to left (PLAN T.5a): every layout of every shipped
/// theme stands mirrored, each node's box as far from the canvas's right edge as it stands from
/// its left in the same deck in English, a theme's shape pointing the other way in its box, and
/// lint finds nothing in it.
#[test]
fn every_shipped_layout_starts_a_slide_mirrored_where_the_deck_reads_right_to_left() {
    use scaena_engine::FrameRequest;
    use std::collections::BTreeMap;
    let mut wrong: Vec<String> = Vec::new();
    let mut compared = 0;
    for (name, theme) in SHIPPED {
        let b = scaena_ops::open(&made(&format!("{name}-mirrored"), theme)).unwrap();
        let t = scaena_ops::theme(&b).unwrap();
        let files = View::of(&b);
        let data = scaena_ops::lint::data_in(&b.deck, &files).unwrap();
        let mut engine = scaena_ops::lint::engine_with(&b, &t, None).unwrap();
        let width = b.deck.canvas.width as f32;
        let first = b.deck.states[0].id.clone();
        for layout in t.layouts.keys() {
            let mut laid: Vec<(BTreeMap<String, [f32; 4]>, scaena_core::Deck, String)> = Vec::new();
            for lang in ["en", "ar"] {
                let mut deck = b.deck.clone();
                deck.meta.get_or_insert_with(Default::default).lang = Some(lang.into());
                let started = starting(&deck, &t, &first, Some(layout)).unwrap();
                let doc = scaena_core::patch::compile(&deck.to_value().unwrap(), &started.patch, &files)
                    .unwrap_or_else(|e| panic!("{name}'s {layout} in {lang}: {e:?}"))
                    .doc;
                let deck = scaena_core::Deck::from_value(&doc).unwrap();
                let req = FrameRequest {
                    deck: &deck,
                    theme: &t,
                    data: &data,
                    state: &started.id,
                    t_ms: f64::INFINITY,
                    format: None,
                };
                let scene = engine.at_rest(&req).unwrap();
                let boxes = scene.boxes().into_iter().map(|b| (b.node, b.rect)).collect();
                if lang == "ar" {
                    let found = lint_state_in(&deck, &files, &started.id).unwrap().findings;
                    for f in found.iter().filter(|f| f.state.as_deref() == Some(started.id.as_str())) {
                        wrong.push(format!("{name}'s {layout} in Arabic: {} {}", f.code, f.message));
                    }
                }
                laid.push((boxes, deck, started.id));
            }
            let [(en, en_deck, _), (ar, ar_deck, _)] = <[_; 2]>::try_from(laid).ok().unwrap();
            assert_eq!(en.keys().collect::<Vec<_>>(), ar.keys().collect::<Vec<_>>(), "{name}'s {layout}");
            for (id, [x, y, w, h]) in &en {
                let r = ar[id];
                let mirrored = [width - x - w, *y, *w, *h];
                if r.iter().zip(mirrored).any(|(a, b)| (a - b).abs() > 0.01) {
                    wrong.push(format!("{name}'s {layout}: `{id}` stands at {r:?} in Arabic, not {mirrored:?}"));
                }
                compared += 1;
                let points = |deck: &scaena_core::Deck| deck.nodes[id.as_str()].props.get("points").cloned();
                if let (Some(en), Some(ar)) = (points(&en_deck), points(&ar_deck)) {
                    let en: Vec<[f64; 2]> = serde_json::from_value(en).unwrap();
                    let ar: Vec<[f64; 2]> = serde_json::from_value(ar).unwrap();
                    // The deck writes a number to 16 significant digits, so a point flipped
                    // reads back within a rounding of `1 - x`, not always to the bit.
                    let flipped = scaena_core::direction::flipped(&en);
                    assert!(
                        ar.len() == flipped.len()
                            && ar
                                .iter()
                                .zip(&flipped)
                                .all(|(a, b)| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9),
                        "{name}'s {layout}: `{id}` points the other way: {ar:?}, not {flipped:?}"
                    );
                }
            }
        }
    }
    assert!(compared > 1000, "boxes compared: {compared}");
    assert!(wrong.is_empty(), "{} wrong:\n{}", wrong.len(), wrong.join("\n"));
}
