//! Export a projection (SPEC §10): the spine, and PDF (PLAN 1.20); frames and video
//! (1.21) and single-file HTML (2.5) name the tasks that build them.

use crate::lint::data_files;
use crate::{Bundle, OpsError};
use scaena_engine::fonts::BundleFonts;
use scaena_engine::images::BundleImages;
use scaena_engine::{Engine, FrameRequest};
use scaena_export::Format;
use scaena_export::pdf::{Page, PdfSettings, pdf};
use scaena_paint::Assets;
use std::collections::HashMap;

/// An export: the spine as JSON, or a document's bytes and what is in it.
#[derive(Debug, Clone)]
pub enum Export {
    Spine(serde_json::Value),
    /// A PDF, and the state each of its pages draws, in order.
    Pdf {
        bytes: Vec<u8>,
        pages: Vec<String>,
    },
}

/// The bundle's deck exported as `format`. `states` picks the states a frame export
/// draws; without it a PDF draws each slide once, at its last state (SPEC §10). The
/// spine is the whole spine and takes none.
pub fn export(b: &Bundle, format: &str, states: Option<&[String]>) -> Result<Export, OpsError> {
    let parsed: Format = format.parse().map_err(OpsError::new)?;
    let plan = match parsed {
        Format::Spine if states.is_some() => {
            return Err(OpsError::new(
                "--states picks the frames of png, pdf, svg, mp4, webm, and html; spine is the whole spine",
            ));
        }
        Format::Spine => return Ok(Export::Spine(scaena_export::spine_json(&b.deck))),
        Format::Pdf => return export_pdf(b, states, PdfSettings::default().shader_scale),
        Format::Html => "2.5",
        _ => "1.21",
    };
    Err(OpsError::not_built(
        format!("`export --format {format}` is not implemented yet — see docs/PLAN.md task {plan}"),
        plan,
    ))
}

/// The states a PDF draws: those asked for, in that order; else each slide at its last
/// state, in the spine's order (SPEC §3.11): the slides of the states its beats name, as
/// it names them, then the slides no beat names, in the deck's order.
pub fn pdf_pages(deck: &scaena_core::Deck, states: Option<&[String]>) -> Result<Vec<String>, OpsError> {
    if let Some(states) = states {
        for s in states {
            if !deck.states.iter().any(|st| &st.id == s) {
                return Err(OpsError::new(format!("--states names `{s}`, which is not a state of the deck")));
            }
        }
        return Ok(states.to_vec());
    }
    let slide: HashMap<&str, &str> = deck.states.iter().map(|s| (s.id.as_str(), deck.slide_of(s))).collect();
    let mut order: Vec<&str> = Vec::new();
    let named = deck.spine.iter().flat_map(|s| &s.sections).flat_map(|s| &s.beats).flat_map(|b| &b.states);
    for s in named.filter_map(|id| slide.get(id.as_str())) {
        if !order.contains(s) {
            order.push(s);
        }
    }
    let mut pages: Vec<(&str, &str)> = Vec::new();
    for (i, state) in deck.states.iter().enumerate() {
        let next = deck.states.get(i + 1);
        if next.is_none_or(|n| deck.slide_of(n) != deck.slide_of(state)) {
            pages.push((deck.slide_of(state), &state.id));
        }
    }
    // Stable: the slides no beat names keep the deck's order, after the rest.
    pages.sort_by_key(|(s, _)| order.iter().position(|o| o == s).unwrap_or(usize::MAX));
    Ok(pages.into_iter().map(|(_, state)| state.to_string()).collect())
}

/// The deck as a PDF, its shaders drawn at `shader_scale` pixels to the canvas unit (2
/// when `export` makes one).
pub fn export_pdf(b: &Bundle, states: Option<&[String]>, shader_scale: f32) -> Result<Export, OpsError> {
    let pages = pdf_pages(&b.deck, states)?;
    let theme = crate::theme(b)?;
    let data = data_files(b)?;
    let mut fonts = BundleFonts::new();
    let mut assets = Assets::new();
    for (id, bytes) in b.read_fonts()? {
        assets.insert_font(&id, bytes.clone());
        fonts.register(&id, bytes)?;
    }
    fonts.check_theme(&theme)?;
    let mut images = BundleImages::new();
    for (path, bytes) in b.read_images()? {
        let info = images.register(&path, &bytes)?;
        assets.insert_image(&info.id, &bytes)?;
    }
    let mut engine = Engine::new(fonts).with_images(images);
    let mut drawn = Vec::with_capacity(pages.len());
    for state in &pages {
        let req = FrameRequest { deck: &b.deck, theme: &theme, data: &data, state, t_ms: f64::INFINITY, format: None };
        drawn.push(Page { state: state.clone(), list: engine.frame(&req)?.display_list });
    }
    let bytes =
        pdf(&b.deck, &drawn, &assets, &PdfSettings { shader_scale }).map_err(|e| OpsError::new(e.to_string()))?;
    Ok(Export::Pdf { bytes, pages })
}

#[cfg(test)]
mod tests {
    use super::pdf_pages;
    use serde_json::json;

    #[test]
    fn pages_follow_the_spine_then_the_deck() {
        // Slide `a` builds in two states; the spine tells `b` before `a`, and names no `c`.
        let deck: scaena_core::Deck = serde_json::from_value(json!({
            "scaena": scaena_core::FORMAT_VERSION, "canvas": { "width": 1920, "height": 1080 }, "nodes": {},
            "states": [{ "id": "a" }, { "id": "a2", "slide": "a" }, { "id": "c" }, { "id": "b" }],
            "spine": { "sections": [{ "id": "s", "beats": [
                { "id": "first", "claim": "B comes first.", "states": ["b"] },
                { "id": "then", "claim": "Then A.", "states": ["a"] }] }] }
        }))
        .unwrap();
        assert_eq!(pdf_pages(&deck, None).unwrap(), ["b", "a2", "c"]);
        let asked = ["c".to_string(), "a".to_string()];
        assert_eq!(pdf_pages(&deck, Some(&asked)).unwrap(), asked);
        let wrong = ["z".to_string()];
        assert!(pdf_pages(&deck, Some(&wrong)).unwrap_err().to_string().contains("`z`"));
    }
}
