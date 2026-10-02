//! Export a projection (SPEC §10): the spine, and PDF (PLAN 1.20); frames and video
//! (1.21) and single-file HTML (2.5) name the tasks that build them.

use crate::lint::data_files;
use crate::{Bundle, OpsError};
use scaena_engine::fonts::BundleFonts;
use scaena_engine::images::BundleImages;
use scaena_engine::{Engine, FrameRequest};
use scaena_export::Format;
use scaena_export::pdf::{PdfSettings, pdf};
use scaena_paint::Assets;

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

/// The states a PDF draws: those asked for, in that order, else each slide's last.
pub fn pdf_pages(deck: &scaena_core::Deck, states: Option<&[String]>) -> Result<Vec<String>, OpsError> {
    if let Some(states) = states {
        for s in states {
            if !deck.states.iter().any(|st| &st.id == s) {
                return Err(OpsError::new(format!("--states names `{s}`, which is not a state of the deck")));
            }
        }
        return Ok(states.to_vec());
    }
    let mut pages: Vec<String> = Vec::new();
    for (i, state) in deck.states.iter().enumerate() {
        let next = deck.states.get(i + 1);
        if next.is_none_or(|n| deck.slide_of(n) != deck.slide_of(state)) {
            pages.push(state.id.clone());
        }
    }
    Ok(pages)
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
    let mut lists = Vec::with_capacity(pages.len());
    for state in &pages {
        let req = FrameRequest { deck: &b.deck, theme: &theme, data: &data, state, t_ms: f64::INFINITY, format: None };
        lists.push(engine.frame(&req)?.display_list);
    }
    let meta = b.deck.meta.as_ref();
    let settings = PdfSettings {
        shader_scale,
        title: meta.and_then(|m| m.title.clone()),
        lang: meta.and_then(|m| m.lang.clone()),
    };
    let bytes = pdf(&lists, &assets, &settings).map_err(|e| OpsError::new(e.to_string()))?;
    Ok(Export::Pdf { bytes, pages })
}
