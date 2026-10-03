//! # scaena-export
//!
//! Projections (SPEC §10). All consume the same resolved document; the spine
//! (`scaena_core::spine`) is the contract for anything that is not the deck.
//!
//! | format | status |
//! |---|---|
//! | png per state (the CPU painter, in `scaena-ops`) | PLAN 1.21, done |
//! | svg per state (vector paths and outlined glyphs, with their text; [`svg`]) | PLAN 1.21, done |
//! | pdf (krilla: vector paths, text, shaders as images, tagged by [`reading`]; [`pdf`]) | PLAN 1.20, done |
//! | mp4 / webm / prores: the global timeline's frames piped to ffmpeg, a chapter per beat ([`video`]) | PLAN 1.21–1.22, done |
//! | single-file html: the web player, the engine, and the bundle in one file that plays offline, read aloud from [`reading`] ([`html`]) | PLAN 2.5, done |
//! | spine json, on the timeline, with per-beat renders (`scaena_core::spine`; drawn in `scaena-ops`) | PLAN 1.22, done |

pub mod html;
pub mod pdf;
pub mod reading;
pub mod svg;
pub mod video;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("not implemented yet: {0} (see docs/PLAN.md)")]
    NotImplemented(&'static str),
    #[error("pdf: {0}")]
    Pdf(String),
    #[error("svg: {0}")]
    Svg(String),
    #[error("video: {0}")]
    Video(String),
    #[error("html: {0}")]
    Html(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Pdf,
    Png,
    Svg,
    Mp4,
    Webm,
    /// ProRes 422 HQ, in a QuickTime movie.
    Prores,
    Html,
    Spine,
}

impl std::str::FromStr for Format {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "pdf" => Format::Pdf,
            "png" => Format::Png,
            "svg" => Format::Svg,
            "mp4" => Format::Mp4,
            "webm" => Format::Webm,
            "prores" => Format::Prores,
            "html" => Format::Html,
            "spine" => Format::Spine,
            other => return Err(format!("unknown export format `{other}`")),
        })
    }
}
