//! A bundle's files and what uses each (PLAN 2.59), on the revenue example in Dusk and on the
//! torture deck: its images, fonts, and data, what names each, and the nodes drawn from it in
//! the states that show them so.

use scaena_core::Deck;
use scaena_core::files::{BundleFile, Kind, Named, files};
use scaena_core::model::Theme;
use serde_json::{Value, json};

const EXAMPLE: &str = include_str!("../../../docs/examples/revenue.deck.json");
const DUSK: &str = include_str!("../../../docs/examples/themes/dusk.theme.json");
const TORTURE: &str = include_str!("../../../tests/fixtures/torture.scaena/deck.json");
const TORTURE_THEME: &str = include_str!("../../../tests/fixtures/torture.scaena/theme.json");

/// The example bundle's files, and some it does not name: an image, a data file, a license.
const HELD: [&str; 13] = [
    "deck.json",
    "manifest.json",
    "themes/dusk.theme.json",
    "fonts/Fraunces-VF.ttf",
    "fonts/Fraunces-Italic-VF.ttf",
    "fonts/Inter-VF.ttf",
    "fonts/Inter-Italic-VF.ttf",
    "fonts/JetBrainsMono-VF.ttf",
    "fonts/JetBrainsMono-Italic-VF.ttf",
    "fonts/OFL.txt",
    "data/q3-revenue.csv",
    "data/old.csv",
    "assets/stray.png",
];

fn listed(doc: &Value) -> Vec<BundleFile> {
    let deck: Deck = serde_json::from_value(doc.clone()).unwrap();
    let held: Vec<(String, u64)> = HELD.iter().map(|p| (p.to_string(), 100)).collect();
    files(&deck, &Theme::from_json(DUSK).unwrap(), &held).unwrap()
}

fn find<'a>(all: &'a [BundleFile], path: &str) -> &'a BundleFile {
    all.iter().find(|f| f.path == path).unwrap_or_else(|| panic!("{path} is not listed"))
}

/// Each node drawn from `f`, with its states.
fn used(f: &BundleFile) -> Vec<(&str, Vec<&str>)> {
    f.used.iter().map(|u| (u.node.as_str(), u.states.iter().map(String::as_str).collect())).collect()
}

#[test]
fn each_file_says_what_names_it_and_the_nodes_drawn_from_it() {
    let doc: Value = serde_json::from_str(EXAMPLE).unwrap();
    let all = listed(&doc);
    // Images, then fonts, then data, each by path: not the deck, its theme, its manifest, or a
    // license beside the fonts.
    let kinds: Vec<(&str, Kind)> = all.iter().map(|f| (f.path.as_str(), f.kind)).collect();
    assert_eq!(
        kinds,
        [
            ("assets/stray.png", Kind::Image),
            ("fonts/Fraunces-Italic-VF.ttf", Kind::Font),
            ("fonts/Fraunces-VF.ttf", Kind::Font),
            ("fonts/Inter-Italic-VF.ttf", Kind::Font),
            ("fonts/Inter-VF.ttf", Kind::Font),
            ("fonts/JetBrainsMono-Italic-VF.ttf", Kind::Font),
            ("fonts/JetBrainsMono-VF.ttf", Kind::Font),
            ("data/old.csv", Kind::Data),
            ("data/q3-revenue.csv", Kind::Data),
        ]
    );
    // The chart's data, by its source, read in the states that show the chart.
    let q3 = find(&all, "data/q3-revenue.csv");
    assert_eq!(q3.named, [Named::Source { source: "q3".into() }]);
    assert_eq!(used(q3), [("rev", vec!["revenue", "mix"])]);
    // Fraunces is the display family's: the title, a display or a headline in every state.
    let fraunces = find(&all, "fonts/Fraunces-VF.ttf");
    let display = Named::Theme { family: "display".into() };
    assert_eq!(fraunces.named, [Named::Font { family: "Fraunces".into(), style: None }, display]);
    assert_eq!(used(fraunces), [("title", vec!["intro", "revenue", "mix", "close"])]);
    // Inter is the body's, and the display family falls back to it: every text, the chart's
    // ticks and values too.
    let inter = find(&all, "fonts/Inter-VF.ttf");
    let every = vec!["intro", "revenue", "mix", "close"];
    assert_eq!(
        used(inter),
        [
            ("title", every),
            ("subtitle", vec!["intro"]),
            ("rev", vec!["revenue", "mix"]),
            ("note", vec!["revenue", "mix"])
        ]
    );
    // A face nothing sets in is named all the same, and stays.
    let italic = find(&all, "fonts/Fraunces-Italic-VF.ttf");
    assert!(italic.used.is_empty(), "{:?}", italic.used);
    assert_eq!(
        italic.named,
        [
            Named::Font { family: "Fraunces".into(), style: Some("italic".into()) },
            Named::Theme { family: "display".into() }
        ]
    );
    // What nothing names says so.
    for stray in ["assets/stray.png", "data/old.csv"] {
        let f = find(&all, stray);
        assert!(f.named.is_empty() && f.used.is_empty(), "{f:?}");
    }
}

#[test]
fn italic_text_is_set_from_the_italic_face_and_a_run_in_another_role_from_its_family() {
    let mut doc: Value = serde_json::from_str(EXAMPLE).unwrap();
    // The note set italic, and the subtitle with a run in the code role.
    doc["nodes"]["note"]["style"] = json!({ "italic": true });
    doc["nodes"]["subtitle"]["runs"] = json!([{ "text": "This quarter " }, { "text": "changed", "role": "code" }]);
    doc["nodes"]["subtitle"].as_object_mut().unwrap().remove("text");
    let all = listed(&doc);
    assert_eq!(used(find(&all, "fonts/Inter-Italic-VF.ttf")), [("note", vec!["revenue", "mix"])]);
    let inter: Vec<&str> = used(find(&all, "fonts/Inter-VF.ttf")).into_iter().map(|(n, _)| n).collect();
    assert!(!inter.contains(&"note"), "the note is italic: {inter:?}");
    assert_eq!(used(find(&all, "fonts/JetBrainsMono-VF.ttf")), [("subtitle", vec!["intro"])]);
}

#[test]
fn an_image_is_named_by_its_nodes_and_its_evidence_and_used_where_they_show() {
    let mut doc: Value = serde_json::from_str(TORTURE).unwrap();
    doc["spine"] = json!({ "sections": [{ "id": "s", "beats": [{
        "id": "card", "claim": "The test card.", "evidence": ["assets/test-card.png"], "states": ["images"]
    }] }] });
    let deck: Deck = serde_json::from_value(doc).unwrap();
    let held = [("assets/test-card.png".to_string(), 4096), ("data/bars.csv".to_string(), 64)];
    let all = files(&deck, &Theme::from_json(TORTURE_THEME).unwrap(), &held).unwrap();
    let card = find(&all, "assets/test-card.png");
    assert_eq!(card.bytes, 4096);
    assert!(card.named.contains(&Named::Node { node: "image-cover".into() }), "{:?}", card.named);
    assert_eq!(card.named.last(), Some(&Named::Evidence { beat: "card".into() }));
    let cover = card.used.iter().find(|u| u.node == "image-cover").unwrap();
    assert_eq!(cover.states, ["images"]);
    // The containers' photos, where that state shows them.
    assert!(card.used.iter().any(|u| u.node == "board-photo" && u.states.contains(&"containers".to_string())));
}
