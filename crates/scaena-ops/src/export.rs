//! Export a projection (SPEC §10): the spine, PDF (PLAN 1.20), PNG and SVG per state, and
//! video (1.21); single-file HTML (2.5) names the task that builds it. An export is
//! written where `out` says; the spine is also returned.

use crate::lint::data_files;
use crate::render::scale_for;
use crate::{Bundle, OpsError};
use scaena_core::timeline::Slot;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::images::BundleImages;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};
use scaena_export::Format;
use scaena_export::pdf::{Page, PdfSettings, pdf};
use scaena_export::svg::{SvgSettings, svg};
use scaena_export::video::{Codec, VideoSettings};
use scaena_paint::cpu::CpuPainter;
use scaena_paint::{Assets, Painter as _};
use schemars::JsonSchema;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// What to export, and where.
#[derive(Debug, Clone, Default)]
pub struct Request {
    /// `pdf`, `png`, `svg`, `mp4`, `webm`, `prores`, `html`, or `spine`.
    pub format: String,
    /// The states a frame export draws, in this order: for png and svg, an image each;
    /// for pdf, a page each; for video, each state's part of the timeline. Without it,
    /// png and svg draw every state, a PDF each slide at its last state, and a video
    /// plays the whole timeline.
    pub states: Option<Vec<String>>,
    /// Where it is written: a file for pdf, video, and the spine; a directory for png
    /// and svg, an image per state named for it.
    pub out: Option<PathBuf>,
    /// `WxH` pixels for png, svg, and video, in the canvas's aspect ratio; the canvas's
    /// size without it.
    pub size: Option<String>,
    /// A video's frames a second: 60 without it.
    pub fps: Option<u32>,
    /// A video's sound track, any file ffmpeg reads, from the first frame: cut where the
    /// frames end, or carried on in silence until they do.
    pub audio: Option<PathBuf>,
}

/// What an export wrote (SPEC §7.1).
#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
pub struct Exported {
    pub format: String,
    /// Where it was written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out: Option<String>,
    /// The spine, for `spine`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spine: Option<serde_json::Map<String, serde_json::Value>>,
    /// The state each page draws, in order: a PDF's pages, or the images of png and svg.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<Vec<String>>,
    /// The files written for png and svg, in the order of `pages`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
    /// Pixels, width and height: of each image, or of the video.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<[u32; 2]>,
    /// A video's frames.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frames: Option<u64>,
    /// A video's frames a second.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<u32>,
    /// How long a video runs, ms: its frames at its rate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,
    /// When each state plays in a video, in order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline: Option<Vec<Played>>,
    /// The bytes written: the document, the video, or every image together.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
}

/// A state's part of a video, ms: its cue (`span`, its transition and motions), then its
/// `hold`, from `start`.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Played {
    pub state: String,
    /// When it starts, ms into the video.
    pub start: f64,
    pub span: f64,
    pub hold: f64,
}

/// The bundle's deck exported as `req` asks, written to `req.out`.
pub fn export(b: &Bundle, req: &Request) -> Result<Exported, OpsError> {
    let format: Format = req.format.parse().map_err(OpsError::new)?;
    let states = req.states.as_deref();
    let video = match format {
        Format::Mp4 => Some(Codec::H264),
        Format::Webm => Some(Codec::Vp9),
        Format::Prores => Some(Codec::ProRes),
        _ => None,
    };
    if req.size.is_some() && !matches!(format, Format::Png | Format::Svg) && video.is_none() {
        return Err(OpsError::new("--size sets the pixels of png, svg, and video"));
    }
    if req.fps.is_some() && video.is_none() {
        return Err(OpsError::new("--fps sets a video's frame rate: mp4, webm, or prores"));
    }
    if req.audio.is_some() && video.is_none() {
        return Err(OpsError::new("--audio sets a video's sound track: mp4, webm, or prores"));
    }
    let written = |bytes: usize| Some(bytes as u64);
    let out = req.out.as_deref();
    let shown = out.map(|p| p.display().to_string());
    let format_name = req.format.clone();
    match format {
        Format::Spine if states.is_some() => {
            Err(OpsError::new("--states picks the frames of png, svg, pdf, and video; spine is the whole spine"))
        }
        Format::Spine => {
            let serde_json::Value::Object(spine) = scaena_export::spine_json(&b.deck) else {
                unreachable!("the spine projection is an object")
            };
            let mut bytes = None;
            if let Some(out) = out {
                let text = serde_json::to_string_pretty(&spine)? + "\n";
                write(out, text.as_bytes())?;
                bytes = written(text.len());
            }
            Ok(Exported { format: format_name, out: shown, spine: Some(spine), bytes, ..Exported::default() })
        }
        Format::Pdf => {
            let out = out.ok_or_else(|| OpsError::new("`export --format pdf` writes a file: give it --out FILE"))?;
            let (bytes, pages) = pdf_document(b, states, PdfSettings::default().shader_scale)?;
            write(out, &bytes)?;
            Ok(Exported {
                format: format_name,
                out: shown,
                pages: Some(pages),
                bytes: written(bytes.len()),
                ..Exported::default()
            })
        }
        Format::Png | Format::Svg => {
            let out = out.ok_or_else(|| {
                OpsError::new(format!("`export --format {format_name}` writes an image per state: give it --out DIR"))
            })?;
            let mut exported = images(b, format, states, out, req.size.as_deref())?;
            exported.out = shown;
            Ok(exported)
        }
        Format::Mp4 | Format::Webm | Format::Prores => {
            let codec = video.expect("a video format");
            let out = out.ok_or_else(|| {
                OpsError::new(format!("`export --format {format_name}` writes a video: give it --out FILE"))
            })?;
            let fps = req.fps.unwrap_or(60);
            let mut exported = export_video(b, codec, states, out, req.size.as_deref(), fps, req.audio.as_deref())?;
            exported.out = shown;
            Ok(exported)
        }
        Format::Html => Err(OpsError::not_built(
            format!("`export --format {format_name}` is not implemented yet — see docs/PLAN.md task 2.5"),
            "2.5",
        )),
    }
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), OpsError> {
    std::fs::write(path, bytes).map_err(|e| OpsError::new(format!("writing {}: {e}", path.display())))
}

/// Every state `states` names, checked, in its order; every state of the deck without it.
fn named(deck: &scaena_core::Deck, states: Option<&[String]>) -> Result<Vec<String>, OpsError> {
    let Some(states) = states else { return Ok(deck.states.iter().map(|s| s.id.clone()).collect()) };
    for s in states {
        if !deck.states.iter().any(|st| &st.id == s) {
            return Err(OpsError::new(format!("--states names `{s}`, which is not a state of the deck")));
        }
    }
    Ok(states.to_vec())
}

/// The states a PDF draws: those asked for, in that order; else each slide at its last
/// state, in the spine's order (SPEC §3.11): the slides of the states its beats name, as
/// it names them, then the slides no beat names, in the deck's order.
pub fn pdf_pages(deck: &scaena_core::Deck, states: Option<&[String]>) -> Result<Vec<String>, OpsError> {
    if states.is_some() {
        return named(deck, states);
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

/// The bundle's engine, and the assets its display lists name: fonts by bundle id, images
/// by content id.
fn engine(b: &Bundle, theme: &Theme) -> Result<(Engine, Assets), OpsError> {
    let mut fonts = BundleFonts::new();
    let mut assets = Assets::new();
    for (id, bytes) in b.read_fonts()? {
        assets.insert_font(&id, bytes.clone());
        fonts.register(&id, bytes)?;
    }
    fonts.check_theme(theme)?;
    let mut images = BundleImages::new();
    for (path, bytes) in b.read_images()? {
        let info = images.register(&path, &bytes)?;
        assets.insert_image(&info.id, &bytes)?;
    }
    Ok((Engine::new(fonts).with_images(images), assets))
}

/// A state at rest, laid out on the deck's canvas.
fn at_rest(
    engine: &mut Engine,
    b: &Bundle,
    theme: &Theme,
    data: &DataFiles,
    state: &str,
) -> Result<scaena_core::displaylist::DisplayList, OpsError> {
    let req = FrameRequest { deck: &b.deck, theme, data, state, t_ms: f64::INFINITY, format: None };
    Ok(engine.frame(&req)?.display_list)
}

/// The deck as a PDF, and the state each page draws; shaders drawn at `shader_scale`
/// pixels to the canvas unit (2 when `export` makes one).
pub fn pdf_document(
    b: &Bundle,
    states: Option<&[String]>,
    shader_scale: f32,
) -> Result<(Vec<u8>, Vec<String>), OpsError> {
    let pages = pdf_pages(&b.deck, states)?;
    let theme = crate::theme(b)?;
    let data = data_files(b)?;
    let (mut engine, assets) = engine(b, &theme)?;
    let mut drawn = Vec::with_capacity(pages.len());
    for state in &pages {
        drawn.push(Page { state: state.clone(), list: at_rest(&mut engine, b, &theme, &data, state)? });
    }
    let bytes =
        pdf(&b.deck, &drawn, &assets, &PdfSettings { shader_scale }).map_err(|e| OpsError::new(e.to_string()))?;
    Ok((bytes, pages))
}

/// An image of each state at rest, in `dir`: `<state>.png` by the CPU painter, or
/// `<state>.svg`.
fn images(
    b: &Bundle,
    format: Format,
    states: Option<&[String]>,
    dir: &Path,
    size: Option<&str>,
) -> Result<Exported, OpsError> {
    let pages = named(&b.deck, states)?;
    let theme = crate::theme(b)?;
    let data = data_files(b)?;
    let (mut engine, assets) = engine(b, &theme)?;
    if dir.is_file() {
        return Err(OpsError::new(format!(
            "--out {} is a file: an image per state goes in a directory",
            dir.display()
        )));
    }
    std::fs::create_dir_all(dir).map_err(|e| OpsError::new(format!("making {}: {e}", dir.display())))?;
    let meta = b.deck.meta.as_ref();
    let (title, lang) = (meta.and_then(|m| m.title.clone()), meta.and_then(|m| m.lang.clone()));
    let (ext, svg_format) = if format == Format::Svg { ("svg", true) } else { ("png", false) };
    let (mut files, mut total, mut pixels) = (Vec::new(), 0_u64, [0, 0]);
    let mut painter = CpuPainter::default();
    for state in &pages {
        let list = at_rest(&mut engine, b, &theme, &data, state)?;
        let scale = match size {
            Some(size) => scale_for(size, list.viewport)?,
            None => 1.0,
        };
        let bytes = if svg_format {
            let settings = SvgSettings { scale, title: title.clone(), lang: lang.clone() };
            pixels = list.viewport.map(|v| (v * scale).round() as u32);
            svg(&list, &assets, &settings).map_err(|e| OpsError::new(format!("{state}: {e}")))?.into_bytes()
        } else {
            let raster = painter.paint(&list, &assets, scale)?;
            pixels = [raster.width, raster.height];
            raster.to_png()?
        };
        let file = dir.join(format!("{state}.{ext}"));
        write(&file, &bytes)?;
        total += bytes.len() as u64;
        files.push(file.display().to_string());
    }
    Ok(Exported {
        format: ext.into(),
        pages: Some(pages),
        files: Some(files),
        size: Some(pixels),
        bytes: Some(total),
        ..Exported::default()
    })
}

/// Where a video's frames fall on the deck's global timeline (SPEC §2.4): the states it
/// plays, end to end, each its cue and then its hold, sampled `fps` times a second.
#[derive(Debug, Clone, PartialEq)]
pub struct Reel {
    /// The states it plays, in order, as the timeline places them.
    pub plays: Vec<Slot>,
    /// When each starts in the video, ms.
    pub starts: Vec<f64>,
    pub frames: u64,
    pub fps: u32,
}

impl Reel {
    /// The states `states` names, in that order, or the whole timeline; `fps` frames a
    /// second, enough to cover it.
    pub fn new(
        timeline: &scaena_core::timeline::Timeline,
        states: Option<&[String]>,
        fps: u32,
    ) -> Result<Reel, OpsError> {
        if fps == 0 || fps > scaena_export::video::MAX_FPS {
            return Err(OpsError::new(format!(
                "--fps {fps}: a video takes 1 to {} frames a second",
                scaena_export::video::MAX_FPS
            )));
        }
        let plays: Vec<Slot> = match states {
            None => timeline.slots.clone(),
            Some(states) => states
                .iter()
                .map(|s| {
                    let slot = timeline.slot(s).cloned();
                    slot.ok_or_else(|| OpsError::new(format!("--states names `{s}`, which is not a state of the deck")))
                })
                .collect::<Result<_, _>>()?,
        };
        let mut starts = Vec::with_capacity(plays.len());
        let mut length = 0.0;
        for slot in &plays {
            starts.push(length);
            length += slot.span + slot.hold;
        }
        if length <= 0.0 {
            return Err(OpsError::new(
                "the timeline is 0 ms long: a video plays each state's transition and motions, then its `hold` \
                 (SPEC §2.4); give the states a hold",
            ));
        }
        let frames = (length * f64::from(fps) / 1000.0).ceil() as u64;
        Ok(Reel { plays, starts, frames, fps })
    }

    /// Frame `k`: the state playing `k / fps` seconds in (its index in `plays`), and how far
    /// into its cue, ms. A state whose cue and hold are both 0 has no frame.
    pub fn at(&self, k: u64) -> (usize, f64) {
        let ms = k as f64 * 1000.0 / f64::from(self.fps);
        let i = self.starts.partition_point(|&s| s <= ms).saturating_sub(1);
        (i, ms - self.starts[i])
    }

    /// How long the video runs, ms: its frames at its rate.
    pub fn duration_ms(&self) -> f64 {
        self.frames as f64 * 1000.0 / f64::from(self.fps)
    }
}

/// The deck's global timeline as a video (SPEC §2.4, §10): the states `states` names, each
/// its cue and then its hold, or every state, sampled `fps` times a second.
fn export_video(
    b: &Bundle,
    codec: Codec,
    states: Option<&[String]>,
    out: &Path,
    size: Option<&str>,
    fps: u32,
    audio: Option<&Path>,
) -> Result<Exported, OpsError> {
    let theme = crate::theme(b)?;
    let data = data_files(b)?;
    let (mut engine, assets) = engine(b, &theme)?;
    let reel = Reel::new(&engine.timeline(&b.deck, &theme, &data)?, states, fps)?;
    let canvas = [b.deck.canvas.width as f32, b.deck.canvas.height as f32];
    let scale = match size {
        Some(size) => scale_for(size, canvas)?,
        None => 1.0,
    };
    let settings = VideoSettings { codec, scale, fps, audio: audio.map(Path::to_path_buf), ..VideoSettings::default() };
    // Each state's cue is laid out once, when its first frame comes; frames only sample it.
    let mut k = 0;
    let mut cue: Option<(usize, scaena_engine::sample::Transition)> = None;
    let next = || {
        if k == reel.frames {
            return None;
        }
        let (at, ms) = reel.at(k);
        k += 1;
        if cue.as_ref().is_none_or(|(i, _)| *i != at) {
            match engine.transition(&b.deck, &theme, &data, &reel.plays[at].state) {
                Ok(t) => cue = Some((at, t)),
                Err(e) => return Some(Err(scaena_export::ExportError::Video(e.to_string()))),
            }
        }
        cue.as_ref().map(|(_, transition)| Ok(transition.frame(ms)))
    };
    let encoded = scaena_export::video::encode(out, canvas, &settings, &assets, next)
        .map_err(|e| OpsError::new(e.to_string()))?;
    let bytes = std::fs::metadata(out).map(|m| m.len()).ok();
    let played = (reel.plays.iter().zip(&reel.starts))
        .map(|(slot, &start)| Played { state: slot.state.clone(), start, span: slot.span, hold: slot.hold })
        .collect();
    Ok(Exported {
        format: codec.name().into(),
        size: Some(encoded.size),
        frames: Some(encoded.frames),
        fps: Some(fps),
        duration_ms: Some(reel.duration_ms()),
        timeline: Some(played),
        bytes,
        ..Exported::default()
    })
}

#[cfg(test)]
mod tests {
    use super::{Reel, pdf_pages};
    use scaena_core::timeline::Timeline;
    use serde_json::json;

    #[test]
    fn a_reel_plays_each_state_then_its_hold_and_passes_over_empty_ones() {
        let timeline = Timeline::new([("a".into(), 500.0, 1000.0), ("b".into(), 0.0, 0.0), ("c".into(), 300.0, 0.0)]);
        let reel = Reel::new(&timeline, None, 10).unwrap();
        assert_eq!((reel.starts.as_slice(), reel.frames, reel.duration_ms()), (&[0.0, 1500.0, 1500.0][..], 18, 1800.0));
        // Frame k is k tenths of a second in: `a`'s cue and hold, then `c`, never `b`.
        assert_eq!(reel.at(0), (0, 0.0));
        assert_eq!(reel.at(7), (0, 700.0));
        assert_eq!(reel.at(15), (2, 0.0));
        assert_eq!(reel.at(17), (2, 200.0));
        // The states asked for, in that order: `c`, then `a`.
        let asked = ["c".to_string(), "a".to_string()];
        let reel = Reel::new(&timeline, Some(&asked), 30).unwrap();
        assert_eq!((reel.starts.as_slice(), reel.frames), (&[0.0, 300.0][..], 54));
        assert_eq!(reel.at(9), (1, 0.0));
        let err = |states: Option<&[String]>, fps| Reel::new(&timeline, states, fps).unwrap_err().to_string();
        assert!(err(Some(&["b".to_string()]), 30).contains("0 ms long"));
        assert!(err(Some(&["z".to_string()]), 30).contains("`z`"));
        assert!(err(None, 0).contains("1 to 240"));
    }

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
