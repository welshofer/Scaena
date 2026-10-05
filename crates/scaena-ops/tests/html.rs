//! Single-file HTML (PLAN 2.5): what `export --format html` writes, from a `scaena` built
//! after the web player it carries; and what one built before it says. `web/standalone.mjs`
//! plays the file in a browser, from its address on disk, with no network.

use scaena_ops::export::{Request as Export, export};
use std::path::{Path, PathBuf};

const EXAMPLE: &str = "../../docs/examples/revenue.deck.json";
const TORTURE: &str = "../../tests/fixtures/torture.scaena";

fn out(name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.html"))
}

/// The page `bundle` exports to, as `req` asks.
fn exported(bundle: &str, states: Option<&[&str]>, name: &str) -> Result<(String, Vec<String>), scaena_ops::OpsError> {
    let bundle = scaena_ops::open(Path::new(bundle)).unwrap();
    let states = states.map(|s| s.iter().map(|s| s.to_string()).collect());
    let req = Export { format: "html".into(), states, out: Some(out(name)), ..Export::default() };
    let done = export(&bundle, &req)?;
    let html = std::fs::read_to_string(out(name)).unwrap();
    assert_eq!(done.bytes, Some(html.len() as u64));
    Ok((html, done.pages.expect("an html export says the states it plays")))
}

#[test]
fn an_html_export_carries_its_bundle_its_states_and_how_each_reads() {
    let made = exported(EXAMPLE, Some(&["revenue", "intro"]), "revenue");
    if scaena_export::html::player().is_none() {
        let e = made.unwrap_err();
        assert_eq!(e.plan.as_deref(), Some("2.5"), "{e:?}");
        assert!(e.message.contains("just web"), "{e:?}");
        return;
    }
    let (html, states) = made.unwrap();
    assert_eq!(states, ["revenue", "intro"]);
    assert!(html.starts_with("<!doctype html>\n<html lang=\"en-US\">"), "{}", &html[..80]);
    assert!(html.contains("<title>Q3 Review</title>"));
    assert!(html.contains(
        r#"<script type="application/json" id="scaena-deck">{"name":"revenue","states":["revenue","intro"]}</script>"#
    ));
    // The engine, and nothing that loads from elsewhere.
    assert!(html.contains(r#"<script type="application/octet-stream" id="scaena-engine">"#));
    assert!(!html.contains(" src=\"") && !html.contains(" href=\""), "it loads nothing");
    assert!(html.contains("default-src 'none'"), "its policy lets nothing load");
    // The bundle as a save writes it: fonts subset and named by their content, their
    // licenses beside them, data, the theme; no manifest, no history.
    let paths: Vec<&str> = html.split(r#"data-path=""#).skip(1).map(|s| &s[..s.find('"').unwrap()]).collect();
    for path in ["deck.json", "themes/dusk.theme.json", "data/q3-revenue.csv", "fonts/OFL-Inter.txt"] {
        assert!(paths.contains(&path), "{path} in {paths:?}");
    }
    // Each family's font and its italic's (PLAN 2.40).
    let fonts: Vec<&&str> = paths.iter().filter(|p| p.ends_with(".ttf")).collect();
    assert_eq!(fonts.len(), 6, "{paths:?}");
    // Named by their content: `fonts/Inter-<hash>.ttf`, and `fonts/Inter-Italic-<hash>.ttf`.
    let hashed = |p: &str| p.rsplit_once('-').is_some_and(|(_, tail)| tail.len() == 16 + 4 && tail.ends_with(".ttf"));
    assert!(fonts.iter().all(|p| hashed(p)), "{fonts:?}");
    assert_eq!(fonts.iter().filter(|p| p.contains("-Italic-")).count(), 3, "{fonts:?}");
    assert!(!paths.iter().any(|p| *p == "manifest.json" || p.starts_with("history/")), "{paths:?}");
    // How each state reads, in the order it plays.
    let revenue = html.find(r#"<template data-state="revenue">"#).expect("revenue reads");
    let intro = html.find(r#"<template data-state="intro">"#).expect("intro reads");
    assert!(revenue < intro);
    assert!(html.contains(
        r#"<template data-state="revenue"><h1 data-node="title">Revenue doubled</h1><div role="img" data-node="rev" aria-label="Quarterly revenue by product, Q4 2025 through Q3 2026."></div><p data-node="note">Revenue in $M. Enterprise recognized on delivery.</p></template>"#
    ));
    assert!(!html.contains(r#"<template data-state="mix">"#), "it plays the states asked for");
    // The same bundle, the same file.
    assert_eq!(exported(EXAMPLE, Some(&["revenue", "intro"]), "revenue-again").unwrap().0, html);
}

#[test]
fn every_state_reads_its_text_figures_and_tables() {
    let Ok((html, states)) = exported(TORTURE, None, "torture") else {
        assert!(scaena_export::html::player().is_none(), "only a scaena without the player stops");
        return;
    };
    let deck = scaena_ops::open(Path::new(TORTURE)).unwrap().deck;
    assert_eq!(states, deck.states.iter().map(|s| s.id.clone()).collect::<Vec<_>>());
    let reads = |state: &str| {
        let open = format!(r#"<template data-state="{state}">"#);
        let at = html.find(&open).unwrap_or_else(|| panic!("{state} reads")) + open.len();
        &html[at..at + html[at..].find("</template>").unwrap()]
    };
    // A table by rows, its first row's cells headers.
    let table = reads("chart-kinds-2");
    assert!(
        table.contains(r#"<table data-node="k-table" aria-label="Revenue and growth by region."><tr><th scope="col">"#),
        "{table}"
    );
    // Pictures by their alt text; a group without it, by its members.
    let images = reads("images");
    assert!(
        images.contains(r#"<div role="img" data-node="image-zoom" aria-label="The test card's checkerboard corner, magnified about ten times."></div>"#),
        "{images}"
    );
    // Text in another language than the deck's says so.
    assert!(reads("bidi-hebrew").contains(r#" lang="he""#), "{}", reads("bidi-hebrew"));
    // Images travel named by their content.
    assert!(html.contains(r#"data-path="assets/"#));
}
