//! Export a projection (SPEC §10): the spine, PDF (PLAN 1.20), PNG and SVG per state,
//! video (1.21), and single-file HTML (2.5). An export is written where `out` says; the
//! spine is returned when it is not.

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
use scaena_export::html::Standalone;
use scaena_export::pdf::{Page, Prepared, prepared};
use scaena_export::svg::{SvgSettings, svg};
use scaena_export::video::{Chapter, Codec, VideoSettings};
use scaena_paint::cpu::CpuPainter;
use scaena_paint::{Assets, Painter as _};
use scaena_store::{SaveOptions, StoreError};
use schemars::JsonSchema;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub use scaena_export::pdf::PdfSettings;

/// What to export, and where.
#[derive(Debug, Clone, Default)]
pub struct Request {
    /// `pdf`, `png`, `svg`, `mp4`, `webm`, `prores`, `html`, or `spine`.
    pub format: String,
    /// The states a frame export draws, in this order: for png and svg, an image each;
    /// for pdf, a page each; for video, each state's part of the timeline; for html, the
    /// states it plays. Without it, png, svg, and html take every state, a PDF each slide
    /// at its last state, and a video plays the whole timeline.
    pub states: Option<Vec<String>>,
    /// Where it is written: a file for pdf, video, html, and the spine; a directory for
    /// png and svg, an image per state named for it.
    pub out: Option<PathBuf>,
    /// `WxH` pixels for png, svg, and video, in the canvas's aspect ratio; the canvas's
    /// size without it.
    pub size: Option<String>,
    /// A video's frames a second: 60 without it.
    pub fps: Option<u32>,
    /// A video's sound track, any file ffmpeg reads, from the first frame: cut where the
    /// frames end, or carried on in silence until they do.
    pub audio: Option<PathBuf>,
    /// What paints a video's frames: the CPU painter, or vello on the GPU in a build with
    /// the `gpu` feature (PLAN 2.22). Every other export paints with the CPU painter.
    pub painter: crate::render::Painter,
}

/// What an export wrote (SPEC §7.1).
#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
pub struct Exported {
    pub format: String,
    /// Where it was written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out: Option<String>,
    /// The spine projection, for `spine` when it is not written (SPEC §10;
    /// `docs/schema/spine.schema.json`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spine: Option<scaena_core::spine::SpineProjection>,
    /// The state each page draws, in order: a PDF's pages, the images of png and svg, or
    /// the states a single-file HTML plays.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<Vec<String>>,
    /// The files written for png and svg, in the order of `pages`; for the spine, each
    /// beat's renders.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
    /// Pixels, width and height: of each image, of the video, or of the spine's
    /// thumbnails.
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
    /// A video's chapters: the spine's beats as it plays them, each titled by its claim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapters: Option<Vec<Chapter>>,
    /// What painted a video's frames: `cpu` or `gpu`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub painter: Option<String>,
    /// The GPU that painted them: its name, backend, and kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    /// The bytes written: the document, the video, the page, or every image together.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    /// An export the MCP server is still running when it stops waiting for it, and how
    /// far it has got (SPEC §7.2). Nothing else is said until it is done.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub running: Option<Running>,
}

/// How far an export that is still going has got (SPEC §7.2).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Running {
    /// `done` of `of`, counting `unit`: frames, pages, images, or beats.
    pub done: u64,
    pub of: u64,
    pub unit: String,
    /// How long it has run, ms.
    pub elapsed_ms: u64,
    /// What to do next.
    pub next: String,
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

/// How far an export has got: `done` of `of` frames of a video, pages of a PDF, images
/// of png and svg, or beats of the spine. Shared with whoever waits for the export: the
/// MCP server tells a client that stopped waiting how it is going (SPEC §7.2).
#[derive(Debug, Default)]
pub struct Progress {
    done: AtomicU64,
    of: AtomicU64,
}

impl Progress {
    /// `(done, of)`.
    pub fn get(&self) -> (u64, u64) {
        (self.done.load(Ordering::Relaxed), self.of.load(Ordering::Relaxed))
    }

    fn start(&self, of: usize) {
        self.done.store(0, Ordering::Relaxed);
        self.of.store(of as u64, Ordering::Relaxed);
    }

    fn step(&self) {
        self.done.fetch_add(1, Ordering::Relaxed);
    }
}

/// What a format's progress counts.
pub fn unit(format: &str) -> &'static str {
    match format.parse::<Format>() {
        Ok(Format::Mp4 | Format::Webm | Format::Prores) => "frames",
        Ok(Format::Pdf) => "pages",
        Ok(Format::Spine) => "beats",
        Ok(Format::Html) => "states",
        _ => "images",
    }
}

/// The page a single-file export fills in (PLAN 2.5), as this build carries it: none in one built
/// before `just web`. `scaena serve` serves it beside the editor, which fills it in to export one
/// (PLAN 2.54).
pub fn single_file_page() -> Option<&'static str> {
    scaena_export::html::player()
}

/// The bundle's deck exported as `req` asks, written to `req.out`.
pub fn export(b: &Bundle, req: &Request) -> Result<Exported, OpsError> {
    export_watched(b, req, &Progress::default())
}

/// [`export`], saying how far it has got in `progress` as it goes.
pub fn export_watched(b: &Bundle, req: &Request, progress: &Progress) -> Result<Exported, OpsError> {
    let format: Format = req.format.parse().map_err(OpsError::new)?;
    let states = req.states.as_deref();
    let video = match format {
        Format::Mp4 => Some(Codec::H264),
        Format::Webm => Some(Codec::Vp9),
        Format::Prores => Some(Codec::ProRes),
        _ => None,
    };
    if req.size.is_some() && !matches!(format, Format::Png | Format::Svg | Format::Spine) && video.is_none() {
        return Err(OpsError::new("--size sets the pixels of png, svg, video, and the spine's thumbnails"));
    }
    if req.fps.is_some() && video.is_none() {
        return Err(OpsError::new("--fps sets a video's frame rate: mp4, webm, or prores"));
    }
    if req.audio.is_some() && video.is_none() {
        return Err(OpsError::new("--audio sets a video's sound track: mp4, webm, or prores"));
    }
    if req.painter == crate::render::Painter::Gpu && video.is_none() {
        return Err(OpsError::new(
            "--painter gpu paints a video's frames: mp4, webm, or prores; every other export paints with the CPU painter",
        ));
    }
    if req.painter == crate::render::Painter::Gpu && cfg!(not(feature = "gpu")) {
        return Err(OpsError::not_built("`--painter gpu` needs a build with `--features gpu` (PLAN 2.22)", "2.22"));
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
            let mut exported = spine(b, out, req.size.as_deref(), progress)?;
            exported.out = shown;
            Ok(exported)
        }
        Format::Pdf => {
            let out = out.ok_or_else(|| OpsError::new("`export --format pdf` writes a file: give it --out FILE"))?;
            let (bytes, pages) = document(b, states, &PdfSettings::default(), progress)?;
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
            let mut exported = images(b, format, states, out, req.size.as_deref(), progress)?;
            exported.out = shown;
            Ok(exported)
        }
        Format::Mp4 | Format::Webm | Format::Prores => {
            let codec = video.expect("a video format");
            let out = out.ok_or_else(|| {
                OpsError::new(format!("`export --format {format_name}` writes a video: give it --out FILE"))
            })?;
            let mut exported = export_video(b, codec, req, out, progress)?;
            exported.out = shown;
            Ok(exported)
        }
        Format::Html => {
            let out = out.ok_or_else(|| OpsError::new("`export --format html` writes a file: give it --out FILE"))?;
            let page = scaena_export::html::player().ok_or_else(|| {
                OpsError::not_built(
                    "this scaena was built without the web player a single-file export carries: build it with \
                     `just web`, then build scaena again",
                    "2.5",
                )
            })?;
            let (html, pages) = standalone(b, states, page, progress)?;
            write(out, html.as_bytes())?;
            Ok(Exported {
                format: format_name,
                out: shown,
                pages: Some(pages),
                bytes: written(html.len()),
                ..Exported::default()
            })
        }
    }
}

/// The deck as one HTML file that plays offline (PLAN 2.5): `page`, the web player built
/// with the engine, filled in with the bundle and how each state it plays reads. The bundle
/// is what a save writes, fonts subset to what the deck draws, without its manifest or its
/// history: the page plays it, and records nothing.
fn standalone(
    b: &Bundle,
    states: Option<&[String]>,
    page: &str,
    progress: &Progress,
) -> Result<(String, Vec<String>), OpsError> {
    let saved = || {
        let opts = SaveOptions { subset_fonts: true, now: String::new(), history: false };
        let subset = |font: &str, bytes: &[u8], chars: &BTreeSet<char>| {
            scaena_store::subset::subset(bytes, chars).map_err(|e| StoreError::Subset(font.to_string(), e))
        };
        Ok(b.saving_with(&opts, |_| Ok(None), subset)?.files)
    };
    standalone_with(b, states, page, &bundle_name(b), progress, saved)
}

/// The deck as one HTML file that plays offline, as [`standalone`] makes it, named `name`, the
/// bundle in it what `saved` gives: what a save writes with fonts subset to what the deck draws,
/// no history recorded, and no time (`SaveOptions { subset_fonts: true, .. }`). In the browser,
/// that is the page's own save, its fonts subset by the subsetter's module (PLAN 2.54).
pub fn standalone_with(
    b: &Bundle,
    states: Option<&[String]>,
    page: &str,
    name: &str,
    progress: &Progress,
    saved: impl FnOnce() -> Result<BTreeMap<String, Vec<u8>>, OpsError>,
) -> Result<(String, Vec<String>), OpsError> {
    let pages = named(&b.deck, states)?;
    progress.start(pages.len());
    let theme = crate::theme(b)?;
    let data = data_files(b)?;
    let (mut engine, _) = engine(b, &theme)?;
    let snapshots = scaena_core::resolve_states(&b.deck)?;
    let mut read = Vec::with_capacity(pages.len());
    for state in &pages {
        let list = at_rest(&mut engine, b, &theme, &data, state)?;
        let snap = snapshots.iter().find(|s| &s.state_id == state).expect("`named` checked the state");
        read.push((state.clone(), scaena_core::reading::html(&b.deck, snap, &list)));
        progress.step();
    }
    let files: BTreeMap<String, Vec<u8>> =
        (saved()?.into_iter()).filter(|(path, _)| path != "manifest.json" && !path.starts_with("history/")).collect();
    let standalone = Standalone { deck: &b.deck, name, files: &files, states: &read };
    let html = scaena_export::html::html(page, &standalone).map_err(|e| OpsError::new(e.to_string()))?;
    Ok((html, pages))
}

/// A bundle's name: its directory's or zip's, or its deck file's, without `.scaena`,
/// `.deck.json`, or `.json`.
fn bundle_name(b: &Bundle) -> String {
    let root = std::fs::canonicalize(&b.root).unwrap_or_else(|_| b.root.clone());
    let file = if b.deck_file == "deck.json" { root.file_name() } else { Path::new(&b.deck_file).file_name() };
    let file = file.map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    let name = [".scaena", ".deck.json", ".json"].iter().find_map(|end| file.strip_suffix(end)).unwrap_or(&file);
    if name.is_empty() { "deck".into() } else { name.to_string() }
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

/// The width of a beat's thumbnail, pixels, when `--size` does not say.
pub const THUMBNAIL_WIDTH: f32 = 480.0;

/// The spine projection (SPEC §10), placed on the global timeline. Written to `out`, it
/// takes each beat's renders, in `renders/` beside it: the state that shows the beat at
/// rest as a thumbnail (`<beat>.png`, `size` or [`THUMBNAIL_WIDTH`] wide), and in each of
/// the deck's other formats at that format's canvas size (`<beat>@9x16.png`).
fn spine(b: &Bundle, out: Option<&Path>, size: Option<&str>, progress: &Progress) -> Result<Exported, OpsError> {
    let theme = crate::theme(b)?;
    let data = data_files(b)?;
    let (mut engine, assets) = engine(b, &theme)?;
    let timeline = engine.timeline(&b.deck, &theme, &data)?;
    let mut projection = scaena_core::spine::projection(&b.deck, Some(&timeline));
    let canvas = [b.deck.canvas.width as f32, b.deck.canvas.height as f32];
    let scale = match size {
        Some(size) => scale_for(size, canvas)?,
        None => THUMBNAIL_WIDTH / canvas[0],
    };
    let Some(out) = out else {
        return Ok(Exported { format: "spine".into(), spine: Some(projection), ..Exported::default() });
    };
    let dir = out.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    // The formats that lay the deck out on another canvas than its own.
    let own = [b.deck.canvas.width, b.deck.canvas.height];
    let others: Vec<String> = (b.deck.formats.iter())
        .filter(|f| scaena_core::model::Format::parse(f).is_some_and(|f| f.canvas(own) != own))
        .cloned()
        .collect();
    let (mut files, mut total) = (Vec::new(), 0_u64);
    let mut painter = CpuPainter::default();
    let mut draw = |list: &scaena_core::displaylist::DisplayList, scale: f32, name: String| {
        let png = painter.paint(list, &assets, scale)?.to_png()?;
        let path = dir.join(&name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| OpsError::new(format!("making {}: {e}", parent.display())))?;
        }
        write(&path, &png)?;
        total += png.len() as u64;
        files.push(path.display().to_string());
        Ok::<String, OpsError>(name)
    };
    let mut pixels = None;
    progress.start(projection.beats.values().filter(|entry| entry.state.is_some()).count());
    for (beat, entry) in &mut projection.beats {
        let Some(state) = entry.state.clone() else { continue };
        let list = at_rest(&mut engine, b, &theme, &data, &state)?;
        pixels = Some(list.viewport.map(|v| (v * scale).round() as u32));
        entry.thumbnail = Some(draw(&list, scale, format!("renders/{beat}.png"))?);
        for format in &others {
            let req = FrameRequest {
                deck: &b.deck,
                theme: &theme,
                data: &data,
                state: &state,
                t_ms: f64::INFINITY,
                format: Some(format),
            };
            let list = engine.frame(&req)?.display_list;
            let name = format!("renders/{beat}@{}.png", format.replace(':', "x"));
            entry.formats.insert(format.clone(), draw(&list, 1.0, name)?);
        }
        progress.step();
    }
    let text = serde_json::to_string_pretty(&projection)? + "\n";
    write(out, text.as_bytes())?;
    total += text.len() as u64;
    Ok(Exported { format: "spine".into(), files: Some(files), size: pixels, bytes: Some(total), ..Exported::default() })
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
    scaena_core::sort::by_key(&mut pages, |(s, _)| order.iter().position(|o| o == s).unwrap_or(usize::MAX));
    Ok(pages.into_iter().map(|(_, state)| state.to_string()).collect())
}

/// The bundle's engine, and the assets its display lists name: fonts by bundle id, images
/// by content id.
fn engine(b: &Bundle, theme: &Theme) -> Result<(Engine, Assets), OpsError> {
    let (engine, files) = engine_files(b, theme)?;
    let mut assets = Assets::new();
    for (id, bytes) in files.fonts {
        assets.insert_font(&id, bytes);
    }
    for (id, bytes) in &files.images {
        assets.insert_image(id, bytes)?;
    }
    Ok((engine, assets))
}

/// The bytes of the files a bundle's display lists name: fonts by bundle id, images by content
/// id.
#[derive(Default)]
struct Named {
    fonts: BTreeMap<String, Vec<u8>>,
    images: BTreeMap<String, Vec<u8>>,
}

/// The bundle's engine, and the bytes of the files its display lists name.
fn engine_files(b: &Bundle, theme: &Theme) -> Result<(Engine, Named), OpsError> {
    let mut fonts = BundleFonts::new();
    let mut named = Named::default();
    for (id, bytes) in b.read_fonts()? {
        named.fonts.insert(id.clone(), bytes.clone());
        fonts.register(&id, bytes)?;
    }
    fonts.check_theme(theme)?;
    let mut images = BundleImages::new();
    for (path, bytes) in b.read_images()? {
        let info = images.register(&path, &bytes)?;
        named.images.insert(info.id, bytes);
    }
    Ok((Engine::new(fonts).with_images(images), named))
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

/// The deck as a PDF, and the state each page draws, written as `settings` say: `export`
/// takes [`PdfSettings::default`].
pub fn pdf_document(
    b: &Bundle,
    states: Option<&[String]>,
    settings: &PdfSettings,
) -> Result<(Vec<u8>, Vec<String>), OpsError> {
    document(b, states, settings, &Progress::default())
}

fn document(
    b: &Bundle,
    states: Option<&[String]>,
    settings: &PdfSettings,
    progress: &Progress,
) -> Result<(Vec<u8>, Vec<String>), OpsError> {
    let (laid, pages) = pdf_laid_out(b, states, progress)?;
    let bytes = prepared(&laid, settings).map_err(|e| OpsError::new(e.to_string()))?;
    Ok((bytes, pages))
}

/// The deck's PDF laid out, and the state each page draws (PLAN 2.54): its pages at rest, as
/// [`pdf_pages`] orders them, and the bytes of the fonts and images they name. Drawn here, it is
/// `export --format pdf`; in the browser, the editor's module lays it out and the PDF's own
/// module draws it ([`scaena_export::pdf::prepared`]), the same bytes.
pub fn pdf_laid_out(
    b: &Bundle,
    states: Option<&[String]>,
    progress: &Progress,
) -> Result<(Prepared, Vec<String>), OpsError> {
    let pages = pdf_pages(&b.deck, states)?;
    progress.start(pages.len());
    let theme = crate::theme(b)?;
    let data = data_files(b)?;
    let (mut engine, named) = engine_files(b, &theme)?;
    let mut drawn = Vec::with_capacity(pages.len());
    for state in &pages {
        drawn.push(Page { state: state.clone(), list: at_rest(&mut engine, b, &theme, &data, state)? });
        progress.step();
    }
    let laid = Prepared { deck: b.deck.clone(), pages: drawn, fonts: named.fonts, images: named.images };
    Ok((laid, pages))
}

/// An image of each state at rest, in `dir`: `<state>.png` by the CPU painter, or
/// `<state>.svg`.
fn images(
    b: &Bundle,
    format: Format,
    states: Option<&[String]>,
    dir: &Path,
    size: Option<&str>,
    progress: &Progress,
) -> Result<Exported, OpsError> {
    let pages = named(&b.deck, states)?;
    progress.start(pages.len());
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
        progress.step();
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

    /// Its chapters (SPEC §10): each run of states that one beat names, titled by the
    /// beat's claim, from where the run starts to where the next chapter does. A state no
    /// beat names is a chapter of its slide, titled by the slide's id; a state two beats
    /// name is the first's. A deck without a spine has none.
    pub fn chapters(&self, deck: &scaena_core::Deck) -> Vec<Chapter> {
        let Some(spine) = &deck.spine else { return Vec::new() };
        let mut beats: HashMap<&str, &scaena_core::document::Beat> = HashMap::new();
        for beat in spine.sections.iter().flat_map(|s| &s.beats) {
            for state in &beat.states {
                beats.entry(state).or_insert(beat);
            }
        }
        let slides: HashMap<&str, &str> = deck.states.iter().map(|s| (s.id.as_str(), deck.slide_of(s))).collect();
        let mut chapters: Vec<Chapter> = Vec::new();
        for (slot, &start) in self.plays.iter().zip(&self.starts) {
            if slot.span + slot.hold <= 0.0 {
                // No frame shows it.
                continue;
            }
            let state = slot.state.as_str();
            let (beat, title) = match beats.get(state) {
                Some(beat) => (Some(beat.id.clone()), beat.claim.clone()),
                None => (None, slides.get(state).copied().unwrap_or(state).to_string()),
            };
            if chapters.last().is_some_and(|c| c.beat == beat && c.title == title) {
                continue;
            }
            if let Some(last) = chapters.last_mut() {
                last.end = start;
            }
            chapters.push(Chapter { beat, title, start, end: start });
        }
        if let Some(last) = chapters.last_mut() {
            last.end = self.duration_ms();
        }
        chapters
    }
}

/// The deck's global timeline as a video (SPEC §2.4, §10): the states `states` names, each
/// its cue and then its hold, or every state, sampled `fps` times a second.
fn export_video(
    b: &Bundle,
    codec: Codec,
    req: &Request,
    out: &Path,
    progress: &Progress,
) -> Result<Exported, OpsError> {
    let (states, size, audio) = (req.states.as_deref(), req.size.as_deref(), req.audio.as_deref());
    let fps = req.fps.unwrap_or(60);
    let theme = crate::theme(b)?;
    let data = data_files(b)?;
    let (mut engine, assets) = engine(b, &theme)?;
    let reel = Reel::new(&engine.timeline(&b.deck, &theme, &data)?, states, fps)?;
    let canvas = [b.deck.canvas.width as f32, b.deck.canvas.height as f32];
    let scale = match size {
        Some(size) => scale_for(size, canvas)?,
        None => 1.0,
    };
    let chapters = reel.chapters(&b.deck);
    let painter = match req.painter {
        crate::render::Painter::Cpu => scaena_export::video::Painter::Cpu,
        crate::render::Painter::Gpu => scaena_export::video::Painter::Gpu,
    };
    let settings = VideoSettings {
        codec,
        painter,
        scale,
        fps,
        audio: audio.map(Path::to_path_buf),
        chapters: chapters.clone(),
        ..VideoSettings::default()
    };
    // Each state's cue is laid out once, when its first frame comes; frames only sample it.
    progress.start(reel.frames as usize);
    let mut k = 0;
    let mut cue: Option<(usize, scaena_engine::sample::Transition)> = None;
    let next = || {
        if k == reel.frames {
            return None;
        }
        let (at, ms) = reel.at(k);
        k += 1;
        progress.step();
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
        chapters: (!chapters.is_empty()).then_some(chapters),
        painter: Some(if painter == scaena_export::video::Painter::Gpu { "gpu" } else { "cpu" }.into()),
        adapter: encoded.adapter,
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
    fn chapters_are_the_beats_as_the_video_plays_them() {
        // `a` builds in `a2`; `x` has no frame; no beat names `c`; `b` is named twice.
        let deck: scaena_core::Deck = serde_json::from_value(json!({
            "scaena": scaena_core::FORMAT_VERSION, "canvas": { "width": 1920, "height": 1080 }, "nodes": {},
            "states": [{ "id": "a" }, { "id": "a2", "slide": "a" }, { "id": "x" }, { "id": "c" }, { "id": "c2", "slide": "c" }, { "id": "b" }],
            "spine": { "sections": [{ "id": "s", "beats": [
                { "id": "built", "claim": "A builds.", "states": ["a", "a2", "x"] },
                { "id": "then", "claim": "Then B.", "states": ["b"] },
                { "id": "again", "claim": "B again.", "states": ["b"] }] }] }
        }))
        .unwrap();
        let timeline = Timeline::new([
            ("a".into(), 500.0, 1000.0),
            ("a2".into(), 300.0, 1000.0),
            ("x".into(), 0.0, 0.0),
            ("c".into(), 0.0, 1000.0),
            ("c2".into(), 0.0, 1000.0),
            ("b".into(), 200.0, 650.0),
        ]);
        let reel = Reel::new(&timeline, None, 10).unwrap();
        let chapters: Vec<_> = (reel.chapters(&deck).into_iter()).map(|c| (c.beat, c.title, c.start, c.end)).collect();
        let beat = |id: &str| Some(id.to_string());
        assert_eq!(
            chapters,
            [
                (beat("built"), "A builds.".to_string(), 0.0, 2800.0),
                (None, "c".to_string(), 2800.0, 4800.0),
                // `b`'s 850 ms end on a frame: the video runs to 5700.
                (beat("then"), "Then B.".to_string(), 4800.0, 5700.0),
            ]
        );
        // The states asked for, as they play: `b`, then `a`.
        let asked = ["b".to_string(), "a".to_string()];
        let reel = Reel::new(&timeline, Some(&asked), 10).unwrap();
        let titles: Vec<_> = reel.chapters(&deck).into_iter().map(|c| (c.title, c.start, c.end)).collect();
        assert_eq!(titles, [("Then B.".to_string(), 0.0, 850.0), ("A builds.".to_string(), 850.0, 2400.0)]);
        // No spine, no chapters.
        let plain = scaena_core::Deck { spine: None, ..deck };
        assert!(Reel::new(&timeline, None, 10).unwrap().chapters(&plain).is_empty());
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
