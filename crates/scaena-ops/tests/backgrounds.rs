//! A slide's background (PLAN 2.148): the theme's presets offered, one given a slide as one patch
//! that the deck takes, written where it stays on the slide, and None taking it away; a slide
//! with a chart says why backgrounds stay off it; a preset the theme lacks is refused.

use scaena_ops::backgrounds::{background, backgrounding, offered};
use scaena_ops::lint::View;
use scaena_store::SaveOptions;
use serde_json::Value;
use std::path::{Path, PathBuf};

const REVENUE: &str = "../../docs/examples/revenue.deck.json";

/// The revenue example, saved where a test may write it.
fn revenue(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("backgrounds-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    let opts = SaveOptions { subset_fonts: false, now: "2026-10-22T00:00:00Z".into(), history: false };
    scaena_ops::open(Path::new(REVENUE)).unwrap().save(&dir, &opts).unwrap();
    dir
}

/// The shaders `state` shows, each by its preset.
fn shown(dir: &Path, state: &str) -> Vec<Option<String>> {
    let b = scaena_ops::open(dir).unwrap();
    let snaps = scaena_core::resolve_states(&b.deck).unwrap();
    let snap = snaps.iter().find(|s| s.state_id == state).unwrap();
    let shader = |id: &String| b.deck.nodes[id].node_type == scaena_core::document::NodeType::Shader;
    (snap.nodes.iter().filter(|(id, _)| shader(id)))
        .map(|(_, p)| p.get("preset").and_then(Value::as_str).map(String::from))
        .collect()
}

#[test]
fn a_slide_takes_a_background_and_gives_it_back() {
    let dir = revenue("take");
    let b = scaena_ops::open(&dir).unwrap();
    let theme = scaena_ops::theme(&b).unwrap();
    let listed = offered(&b.deck, &theme, &View::of(&b), "intro").unwrap();
    let labels: Vec<(&str, Option<&str>)> =
        listed.options.iter().map(|o| (o.label.as_str(), o.said.as_deref())).collect();
    // None, then each preset the theme has, in its order: Dusk's first two, then any it gains.
    assert_eq!(labels[..3], [("None", None), ("Backdrop", Some("Mesh Gradient")), ("Texture", Some("Noise"))]);
    let presets = theme.shaders.as_ref().and_then(|s| s.presets.as_ref()).map_or(0, |p| p.len());
    assert_eq!(labels.len(), 1 + presets);
    let close = shown(&dir, "close");

    // Checked as a patch is, nothing written.
    let (dry, _) = backgrounding(&b, "intro", Some("texture")).unwrap();
    assert!(!dry.refused && dry.errors == 0, "{dry:?}");
    assert_eq!(shown(&dir, "intro"), [None], "nothing written");

    let made = background(&b, "intro", Some("texture")).unwrap();
    assert!(made.applied, "{made:?}");
    assert_eq!(shown(&dir, "intro"), [Some("texture".to_string())]);
    assert_eq!(shown(&dir, "close"), close, "the close keeps its own");
    assert!(shown(&dir, "revenue").is_empty(), "it stays on its slide");

    // The slide's now; None takes it away.
    let b = scaena_ops::open(&dir).unwrap();
    let listed = offered(&b.deck, &theme, &View::of(&b), "intro").unwrap();
    assert!(listed.options[2].current && listed.options[2].patch.is_empty());
    assert!(background(&b, "intro", None).unwrap().applied);
    assert!(shown(&dir, "intro").is_empty());
    assert_eq!(shown(&dir, "close"), close);

    // A preset the theme lacks.
    let b = scaena_ops::open(&dir).unwrap();
    let lacks = background(&b, "intro", Some("nowhere")).unwrap_err();
    assert_eq!(lacks.message, "the theme has no shader preset `nowhere`");
}

#[test]
fn a_slide_with_a_chart_says_why_backgrounds_stay_off_it() {
    let dir = revenue("held");
    let b = scaena_ops::open(&dir).unwrap();
    let theme = scaena_ops::theme(&b).unwrap();
    let listed = offered(&b.deck, &theme, &View::of(&b), "mix").unwrap();
    assert_eq!(listed.first, "revenue");
    assert_eq!(listed.held.as_deref(), Some(scaena_core::words::BACKGROUND_HELD));
    assert!(listed.options[0].current);
}

/// A new deck in Dusk, on its title slide, in the fonts the theme ships with.
fn dusk(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("backgrounds-{test}"));
    let shipped = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("backgrounds-{test}-theme"));
    for d in [&dir, &shipped] {
        let _ = std::fs::remove_dir_all(d);
    }
    std::fs::create_dir_all(shipped.join("themes")).unwrap();
    std::fs::create_dir_all(shipped.join("fonts")).unwrap();
    for font in std::fs::read_dir("../../docs/examples/fonts").unwrap() {
        let font = font.unwrap().path();
        std::fs::copy(&font, shipped.join("fonts").join(font.file_name().unwrap())).unwrap();
    }
    let theme = shipped.join("themes/dusk.theme.json");
    std::fs::copy("../../docs/examples/themes/dusk.theme.json", &theme).unwrap();
    let req = scaena_ops::create::Create { theme, title: Some("High Country Trails".into()), ..Default::default() };
    assert!(scaena_ops::create::create(&dir, &req).unwrap().created);
    let b = scaena_ops::open(&dir).unwrap();
    let started =
        scaena_ops::states::starting(&b.deck, &scaena_ops::theme(&b).unwrap(), "start", Some("title")).unwrap();
    assert!(scaena_ops::patch::patch(&b, &Value::Array(started.patch), false).unwrap().applied);
    dir
}

/// The contrast errors lint finds in the bundle at `dir`.
fn faint(dir: &Path) -> Vec<String> {
    let found = scaena_ops::lint::lint(&scaena_ops::open(dir).unwrap()).unwrap().findings;
    found.iter().filter(|f| matches!(f.code.as_str(), "E110" | "E111")).map(|f| format!("{f:?}")).collect()
}

#[test]
fn a_background_keeps_the_slides_words_readable() {
    let dir = dusk("readable");
    let b = scaena_ops::open(&dir).unwrap();
    let theme = scaena_ops::theme(&b).unwrap();
    let first = b.deck.states.last().unwrap().id.clone();
    assert!(faint(&dir).is_empty(), "the title slide reads as made");

    // Backdrop alone leaves a text too faint to read over it…
    let full = scaena_ops::backgrounds::setting(&b.deck, &theme, &View::of(&b), &first, Some("backdrop")).unwrap();
    let ours = |op: &Value| matches!(op["op"].as_str(), Some("add_node" | "hide_node" | "remove_node" | "show_node"));
    let bare: Vec<Value> = full.iter().filter(|op| ours(op)).cloned().collect();
    assert!(bare.len() < full.len(), "the patch sets a color too: {full:?}");
    let (bare, _) = scaena_ops::patch::patching(&b, &Value::Array(bare), None).unwrap();
    assert!(bare.added.iter().any(|f| f.code == "E110"), "backdrop leaves a text faint: {:?}", bare.added);

    // …which the patch sets in a color that reads there: one patch, one undo step.
    let made = background(&b, &first, Some("backdrop")).unwrap();
    assert!(made.applied, "{made:?}");
    assert_eq!(faint(&dir), Vec::<String>::new());

    // None leaves the color, which still reads.
    let b = scaena_ops::open(&dir).unwrap();
    let recolored = b.deck.to_value().unwrap();
    assert!(background(&b, &first, None).unwrap().applied);
    let after = scaena_ops::open(&dir).unwrap().deck.to_value().unwrap();
    assert!(faint(&dir).is_empty());
    let colors = |doc: &Value| doc.to_string().matches("\"color\"").count();
    assert_eq!(colors(&after), colors(&recolored), "the colors stay");
}
