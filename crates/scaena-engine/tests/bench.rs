//! The benchmark decks (SPEC §15) stay what they claim to be: B1 is text-heavy, 40
//! states, 4 fonts, no charts, and every state lays out and draws. A benchmark that
//! stopped rendering would time an error path.

use scaena_core::Deck;
use scaena_core::displaylist::Op;
use scaena_core::document::NodeType;
use scaena_core::lint::lint_document;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};
use std::collections::BTreeSet;

const B1: &str = "../../tests/bench/b1.scaena";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{B1}/{path}")).unwrap_or_else(|e| panic!("{B1}/{path}: {e}"))
}

/// Every glyph run in `ops`, layers included, as (font, glyph count).
fn runs(ops: &[Op], out: &mut Vec<(u32, usize)>) {
    for op in ops {
        match op {
            Op::Layer { ops, .. } => runs(ops, out),
            Op::Glyphs { font, glyphs, .. } => out.push((*font, glyphs.len())),
            _ => {}
        }
    }
}

#[test]
fn b1_is_forty_text_states_in_four_fonts_and_every_one_draws() {
    let deck = Deck::from_json(&String::from_utf8(read("deck.json")).unwrap()).unwrap();
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    let findings = lint_document(&deck, Some(&*theme));
    assert!(findings.is_empty(), "{findings:#?}");
    assert_eq!((deck.states.len(), deck.fonts.len()), (40, 4));
    assert!(deck.nodes.values().all(|n| n.node_type == NodeType::Text), "B1 is text only");

    let mut fonts = BundleFonts::new();
    for f in &deck.fonts {
        fonts.register(&f.file, read(&f.file)).unwrap();
    }
    fonts.check_theme(&theme).unwrap();
    let mut engine = Engine::new(fonts);
    let data = DataFiles::new();
    let mut files = BTreeSet::new();
    for state in &deck.states {
        let req = FrameRequest {
            deck: &deck,
            theme: &theme,
            data: &data,
            state: &state.id,
            t_ms: f64::INFINITY,
            format: None,
        };
        let dl = engine.frame(&req).unwrap_or_else(|e| panic!("{}: {e}", state.id)).display_list;
        let mut found = Vec::new();
        runs(&dl.ops, &mut found);
        assert!(found.iter().map(|(_, n)| n).sum::<usize>() > 0, "{} draws no glyphs", state.id);
        files.extend(found.iter().map(|(font, _)| dl.fonts[*font as usize].id.clone()));
    }
    // All four families set text somewhere in the deck.
    assert_eq!(files.len(), 4, "{files:?}");
}
