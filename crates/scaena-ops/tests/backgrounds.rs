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
    assert_eq!(labels, [("None", None), ("Backdrop", Some("Mesh Gradient")), ("Texture", Some("Noise"))]);
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
    let lacks = background(&b, "intro", Some("aurora")).unwrap_err();
    assert_eq!(lacks.message, "the theme has no shader preset `aurora`");
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
