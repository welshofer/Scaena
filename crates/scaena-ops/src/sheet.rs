//! A theme's contact sheet (PLAN T.0e, ADR-0031 decision 8): every layout of the theme, started
//! on a new deck as Add Slide starts it, with its examples ([`crate::states::pictured`]), drawn
//! small on one PNG, a row or more for each of its sections in the theme's order. A person reviews
//! a theme's sheet once; after that, each start's display-list digest catches drift, as the golden
//! display lists do (`tests/golden/themes/<slug>.digests.json`).

use crate::create::{Create, creating};
use crate::lint::View;
use crate::{Context, OpsError};
use scaena_core::Deck;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};
use scaena_paint::cpu::CpuPainter;
use scaena_paint::{Assets, Painter, Raster};
use schemars::JsonSchema;
use serde::Serialize;
use std::path::Path;

/// How high each start is drawn on the sheet, in pixels.
pub const HEIGHT: u32 = 180;
/// Starts to a row.
const COLUMNS: u32 = 6;
/// Room between starts, around the sheet, and above each section after the first.
const GAP: u32 = 12;
const MARGIN: u32 = 24;
const SECTION: u32 = 36;
/// The sheet's ground, a gray a light theme's slides and a dark theme's both stand off.
const GROUND: [u8; 4] = [0x6E, 0x6E, 0x6E, 0xFF];

/// One layout's start.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Start {
    /// The layout, by its key in the theme.
    pub layout: String,
    /// Its section (`group`).
    pub group: Option<String>,
    /// The display list of the slide it starts, at rest: [`scaena_core::displaylist::DisplayList::digest`].
    pub digest: String,
}

/// A theme's sheet: each layout's start, in the theme's order, and the PNG when it was drawn.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Sheet {
    pub theme: String,
    pub starts: Vec<Start>,
    /// The sheet's pixels: width, height.
    pub size: Option<[u32; 2]>,
    #[serde(skip)]
    pub png: Option<Vec<u8>>,
}

/// The sheet of `theme`: a theme that ships, by name, or a theme file, its fonts beside it or
/// above it. Drawn as a PNG with `paint`, else only its starts' digests.
pub fn sheet(theme: &str, paint: bool) -> Result<Sheet, OpsError> {
    let path = Path::new(theme);
    if !path.exists() {
        #[cfg(feature = "shipped")]
        if let Some(t) = crate::shipped::theme(theme) {
            let font = |file: &str| -> Result<Vec<u8>, OpsError> {
                let bytes = crate::shipped::font(file)
                    .with_context(|| format!("{} names `{file}`, which does not ship", t.name))?;
                Ok(bytes.to_vec())
            };
            return sheet_of(Path::new(t.file), t.text.to_string(), &font, paint);
        }
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {theme}"))?;
    let font = |file: &str| -> Result<Vec<u8>, OpsError> {
        let found = path
            .ancestors()
            .skip(1)
            .map(|dir| dir.join(file))
            .find(|p| p.is_file())
            .with_context(|| format!("the theme's font `{file}` is not beside {theme} or above it"))?;
        Ok(std::fs::read(&found)?)
    };
    sheet_of(path, text, &font, paint)
}

/// The sheet of the theme `text`, named as `file` names it, each font it names read by `font`.
pub fn sheet_of(
    file: &Path,
    text: String,
    font: &dyn Fn(&str) -> Result<Vec<u8>, OpsError>,
    paint: bool,
) -> Result<Sheet, OpsError> {
    let theme = Theme::from_json(&text)?;
    let name = theme.name.clone();
    let req = Create { theme: file.into(), title: Some("A contact sheet".into()), ..Create::default() };
    let no_data =
        |p: &Path| -> Result<Vec<u8>, OpsError> { Err(OpsError::new(format!("no data file {}", p.display()))) };
    let (created, made) = creating(&req, text, font, &no_data)?;
    let made = made.with_context(|| format!("{name} makes no deck: {:?}", created.findings))?;
    let nothing = scaena_store::Files::Zip(Default::default());
    let files = View { base: &nothing, pending: made.files };
    // Drawn as a picker draws a layout: without the deck's slide number and footer.
    let mut base = made.deck.to_value()?;
    if let Some(deck) = base.as_object_mut() {
        deck.remove("furniture");
    }
    let mut fonts = BundleFonts::new();
    let mut store = Assets::new();
    for f in &made.deck.fonts {
        let bytes = files.read(&f.file)?;
        store.insert_font(&f.file, bytes.clone());
        fonts.register(&f.file, bytes)?;
    }
    fonts.check_theme(&theme)?;
    let mut engine = Engine::new(fonts);
    let mut painter = CpuPainter::default();
    let data = DataFiles::new();
    let first = made.deck.states[0].id.clone();
    let (mut starts, mut pictures) = (Vec::new(), Vec::new());
    for (layout, template) in &theme.layouts {
        let started = crate::states::pictured(&made.deck, &theme, &first, Some(layout))?;
        let doc = scaena_core::patch::compile(&base, &started.patch, &files)
            .map_err(|e| OpsError::new(format!("{name}'s {layout}: {e:?}")))?
            .doc;
        let deck = Deck::from_value(&doc).map_err(|e| OpsError::new(format!("{name}'s {layout}: {e}")))?;
        let req = FrameRequest {
            deck: &deck,
            theme: &theme,
            data: &data,
            state: &started.id,
            t_ms: f64::INFINITY,
            format: None,
        };
        let list = engine.frame(&req)?.display_list;
        let digest = list.digest().map_err(|e| OpsError::new(e.to_string()))?;
        if paint {
            pictures.push(painter.paint(&list, &store, HEIGHT as f32 / list.viewport[1])?);
        }
        starts.push(Start { layout: layout.clone(), group: template.group.clone(), digest });
    }
    let (png, size) = match paint {
        true => {
            let sheet = laid(&starts, pictures);
            (Some(sheet.to_png()?), Some([sheet.width, sheet.height]))
        }
        false => (None, None),
    };
    Ok(Sheet { theme: name, starts, size, png })
}

/// The pictures on one sheet: `COLUMNS` to a row, each section starting a row of its own.
fn laid(starts: &[Start], pictures: Vec<Raster>) -> Raster {
    let width = pictures.iter().map(|p| p.width).max().unwrap_or(0);
    // Where each picture goes: its row, then its column.
    let mut places = Vec::with_capacity(starts.len());
    let (mut row, mut column, mut sections) = (0u32, 0u32, 0u32);
    for (i, start) in starts.iter().enumerate() {
        let new_section = i > 0 && start.group != starts[i - 1].group;
        if i > 0 && (new_section || column == COLUMNS) {
            row += 1;
            column = 0;
        }
        if new_section {
            sections += 1;
        }
        places.push((row, column, sections));
        column += 1;
    }
    let rows = places.last().map_or(0, |p| p.0 + 1);
    let w = 2 * MARGIN + COLUMNS * width + (COLUMNS - 1) * GAP;
    let h = 2 * MARGIN + rows * HEIGHT + rows.saturating_sub(1) * GAP + sections * SECTION;
    let mut rgba = GROUND.repeat((w * h) as usize);
    for ((row, column, sections), picture) in places.into_iter().zip(&pictures) {
        let (x, y) = (MARGIN + column * (width + GAP), MARGIN + row * (HEIGHT + GAP) + sections * SECTION);
        for py in 0..picture.height.min(h - y) {
            let from = (py * picture.width * 4) as usize;
            let to = (((y + py) * w + x) * 4) as usize;
            let n = (picture.width.min(w - x) * 4) as usize;
            rgba[to..to + n].copy_from_slice(&picture.rgba[from..from + n]);
        }
    }
    Raster { width: w, height: h, rgba }
}
