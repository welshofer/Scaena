//! PDF (PLAN 1.20; SPEC §10): a deck's pages drawn from display lists by `krilla`, and
//! tagged with how the deck reads.
//!
//! - A page is a frame's canvas at 2 units to the point: a 1920 × 1080 canvas is the
//!   13⅓ × 7½ in widescreen page.
//! - Paths fill and stroke as vectors. Gradients take the stops both raster painters
//!   draw (`scaena_paint::srgb_stops`: their Oklab blend spelled out in sRGB).
//! - Text is text. Each glyph run is set in its font, a variable instance at its
//!   coordinates, embedded as a subset of the glyphs the document draws, every glyph
//!   where the display list puts it. Each glyph says the text of its cluster (SPEC §6),
//!   so the PDF copies and searches as the deck reads.
//! - Images embed at their own resolution, clipped to the part an op draws.
//! - Shaders draw as images of their CPU reference at `shader_scale` pixels to the unit
//!   (SPEC §3.8), placed as the CPU painter places them.
//! - The PDF is tagged (SPEC §3.12). Its structure follows the spine: a section per
//!   spine section, holding the pages of its beats' slides, then the pages no beat
//!   names. A page reads in paint order, each node as [`scaena_core::reading`] says: a heading
//!   or paragraph, a figure with its alt text, a table by rows of header and data
//!   cells. What no node reads (the page's background, decoration, a container's panel)
//!   is an artifact. The spine's sections are the document's outline.

use crate::ExportError;
use krilla::action::LinkAction;
use krilla::annotation::{Annotation, LinkAnnotation, Target};
use krilla::color::rgb;
use krilla::destination::XyzDestination;
use krilla::geom::{Path as KPath, PathBuilder, Point, Rect as KRect, Size, Transform};
use krilla::image::{BitsPerComponent, CustomImage, Image, ImageColorspace};
use krilla::metadata::Metadata;
use krilla::num::NormalizedF32;
use krilla::outline::{Outline, OutlineNode};
use krilla::page::PageSettings;
use krilla::paint::{
    Fill, FillRule as KRule, LineCap, LineJoin, LinearGradient, Paint as KPaint, RadialGradient, SpreadMethod,
    Stop as KStop, Stroke, StrokeDash, SweepGradient,
};
use krilla::surface::Surface;
use krilla::tagging::{
    Artifact, ArtifactType, ContentTag, Identifier, ListNumbering, Node, SpanTag, TableHeaderScope, Tag, TagGroup,
    TagKind, TagTree,
};
use krilla::text::{Font, GlyphId};
use krilla::{Document, SerializeSettings};
use kurbo::Affine;
use scaena_core::Deck;
use scaena_core::displaylist::{
    Blend, Cap, Color, DisplayList, FillRule, FontRef, Join, LinkTarget, Op, Paint, Path, PathEl, Quality,
};
use scaena_core::document::Section;
use scaena_core::model::values::ListKind;
use scaena_core::reading::{self, Kind, Reading};
use scaena_core::shader::Job;
use scaena_paint::Painter as _;
use scaena_paint::{Assets, cpu::CpuPainter};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::num::NonZeroU16;
use std::ops::Range;
use std::sync::Arc;

/// Points to a canvas unit: a page is its canvas at 2 units to the point, the engine's 144 to the
/// inch, which the safe area is measured in (PLAN 3.29).
pub const POINTS_PER_UNIT: f32 = 72.0 / scaena_engine::guides::UNITS_PER_INCH;

/// A page: the state it draws, and its frame.
#[derive(Debug, Clone)]
pub struct Page {
    pub state: String,
    pub list: DisplayList,
}

/// How a document is written.
#[derive(Debug, Clone)]
pub struct PdfSettings {
    /// Device pixels to the canvas unit at which shaders are drawn: 2, twice the canvas.
    pub shader_scale: f32,
    /// The JPEG quality, 1 to 100, of a shader drawn opaque: 90. A mesh with grain at
    /// twice a 1080p canvas is about 1 MB so, and 15 MB kept whole. A shader that lets
    /// what is under it show keeps every pixel, as does every shader without a quality.
    pub shader_quality: Option<u8>,
}

impl Default for PdfSettings {
    fn default() -> Self {
        Self { shader_scale: 2.0, shader_quality: Some(90) }
    }
}

/// A deck's pages laid out for a PDF (PLAN 2.54): the deck, each page's state and frame, and the
/// bytes of the fonts and images the frames name. Where no engine is, as in the PDF's own module
/// in the browser, [`prepared`] draws it: the editor's module lays the pages out, and hands this
/// over as bytes ([`Prepared::to_bytes`]).
#[derive(Debug, Clone)]
pub struct Prepared {
    pub deck: Deck,
    pub pages: Vec<Page>,
    /// Font files, by the id the frames name each by.
    pub fonts: BTreeMap<String, Vec<u8>>,
    /// PNG files, by their content id.
    pub images: BTreeMap<String, Vec<u8>>,
}

/// What [`Prepared::to_bytes`] begins with.
const PREPARED: &[u8; 4] = b"SPDF";

/// [`Prepared`]'s header, as JSON: the deck, and each page's state, then each font's id and
/// each image's, each with the length of its bytes.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    deck: String,
    pages: Vec<(String, usize)>,
    fonts: Vec<(String, usize)>,
    images: Vec<(String, usize)>,
}

impl Prepared {
    /// As bytes: `SPDF`; the length of a JSON header, four bytes, little-endian; the header,
    /// which holds the deck, and each page's state and each file's id, each with the length of
    /// its bytes; then each page's frame (postcard, as the engine's module hands frames over),
    /// each font's bytes, and each image's, in the header's order.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ExportError> {
        let bad = |e: &dyn std::fmt::Display| ExportError::Pdf(e.to_string());
        let deck = self.deck.to_json().map_err(|e| bad(&e))?;
        let lists: Vec<Vec<u8>> =
            (self.pages.iter()).map(|p| p.list.to_postcard()).collect::<Result<_, _>>().map_err(|e| bad(&e))?;
        let pages = self.pages.iter().zip(&lists).map(|(p, list)| (p.state.clone(), list.len())).collect();
        let sized = |files: &BTreeMap<String, Vec<u8>>| files.iter().map(|(id, b)| (id.clone(), b.len())).collect();
        let header = Header { deck, pages, fonts: sized(&self.fonts), images: sized(&self.images) };
        let header = serde_json::to_vec(&header).map_err(|e| bad(&e))?;
        let length = u32::try_from(header.len()).map_err(|_| bad(&"the deck is over 4 GB as JSON"))?;
        let blobs: Vec<&[u8]> = (lists.iter().map(Vec::as_slice))
            .chain(self.fonts.values().map(Vec::as_slice))
            .chain(self.images.values().map(Vec::as_slice))
            .collect();
        let mut out = Vec::with_capacity(8 + header.len() + blobs.iter().map(|b| b.len()).sum::<usize>());
        out.extend_from_slice(PREPARED);
        out.extend_from_slice(&length.to_le_bytes());
        out.extend_from_slice(&header);
        for blob in blobs {
            out.extend_from_slice(blob);
        }
        Ok(out)
    }

    /// From [`Prepared::to_bytes`]' bytes; any others are an error that says why.
    pub fn from_bytes(bytes: &[u8]) -> Result<Prepared, ExportError> {
        let bad = |why: &str| ExportError::Pdf(format!("not a deck laid out for a PDF: {why}"));
        let rest = bytes.strip_prefix(PREPARED).ok_or_else(|| bad("it does not begin `SPDF`"))?;
        let (length, rest) = rest.split_first_chunk::<4>().ok_or_else(|| bad("it ends early"))?;
        let length = u32::from_le_bytes(*length) as usize;
        let (header, mut blobs) = rest.split_at_checked(length).ok_or_else(|| bad("its header ends early"))?;
        let header: Header = serde_json::from_slice(header).map_err(|e| bad(&e.to_string()))?;
        let deck = Deck::from_json(&header.deck).map_err(|e| bad(&e.to_string()))?;
        let mut take = |what: &str, n: usize| -> Result<&[u8], ExportError> {
            let (blob, next) = blobs.split_at_checked(n).ok_or_else(|| bad(&format!("{what} ends early")))?;
            blobs = next;
            Ok(blob)
        };
        let mut pages = Vec::with_capacity(header.pages.len());
        for (state, n) in header.pages {
            let list = DisplayList::from_postcard(take(&format!("`{state}`'s frame"), n)?)
                .map_err(|e| bad(&format!("`{state}`'s frame: {e}")))?;
            pages.push(Page { state, list });
        }
        let mut files = |sized: Vec<(String, usize)>| -> Result<BTreeMap<String, Vec<u8>>, ExportError> {
            let mut taken = BTreeMap::new();
            for (id, n) in sized {
                let file = take(&format!("`{id}`"), n)?.to_vec();
                taken.insert(id, file);
            }
            Ok(taken)
        };
        let fonts = files(header.fonts)?;
        let images = files(header.images)?;
        if !blobs.is_empty() {
            return Err(bad(&format!("it goes on past its last file ({} B)", blobs.len())));
        }
        Ok(Prepared { deck, pages, fonts, images })
    }
}

/// `prepared` as a tagged PDF: [`pdf`], drawing from the fonts and images it carries.
pub fn prepared(prepared: &Prepared, settings: &PdfSettings) -> Result<Vec<u8>, ExportError> {
    let mut assets = Assets::new();
    for (id, bytes) in &prepared.fonts {
        assets.insert_font(id, bytes.clone());
    }
    for (id, bytes) in &prepared.images {
        assets.insert_image(id, bytes).map_err(|e| ExportError::Pdf(format!("{id}: {e}")))?;
    }
    pdf(&prepared.deck, &prepared.pages, &assets, settings)
}

/// `deck` as a tagged PDF of `pages`, in order, drawing from `assets`. krilla reads each font
/// with a reader of its own and subsets it as the document finishes, and neither expects a
/// damaged font: a panic in either is an error that says so (PLAN 2.25).
pub fn pdf(deck: &Deck, pages: &[Page], assets: &Assets, settings: &PdfSettings) -> Result<Vec<u8>, ExportError> {
    let written = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| write(deck, pages, assets, settings)));
    written.unwrap_or_else(|panic| {
        let why = (panic.downcast_ref::<&str>().map(|s| s.to_string()))
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        Err(ExportError::Pdf(format!("krilla stopped writing the PDF ({why}): a font file may be damaged")))
    })
}

fn write(deck: &Deck, pages: &[Page], assets: &Assets, settings: &PdfSettings) -> Result<Vec<u8>, ExportError> {
    let snapshots = scaena_core::resolve_states(deck).map_err(|e| ExportError::Pdf(e.to_string()))?;
    let mut document = Document::new_with(SerializeSettings::default());
    let mut fonts = Fonts::default();
    let mut photos = HashMap::new();
    let mut structure: Vec<Vec<Node>> = Vec::with_capacity(pages.len());
    // Each page's headline, which bookmarks it where no section's title or beat's claim does.
    let mut headlines: Vec<Option<String>> = Vec::with_capacity(pages.len());
    for page in pages {
        let dl = &page.list;
        assets.frame();
        let Some(snap) = snapshots.iter().find(|s| s.state_id == page.state) else {
            return Err(ExportError::Pdf(format!("`{}` is not a state of the deck", page.state)));
        };
        let readings = reading::readings(deck, snap);
        headlines.push(reading::headline(deck, snap));
        let written: HashMap<String, String> =
            snap.nodes.iter().filter_map(|(id, props)| Some((id.clone(), as_written(props)?))).collect();
        let [w, h] = dl.viewport.map(|v| v * POINTS_PER_UNIT);
        let page_settings =
            PageSettings::from_wh(w, h).ok_or_else(|| ExportError::Pdf(format!("a {w}×{h} pt page")))?;
        let scale = settings.shader_scale;
        let jobs = scaena_paint::shader_jobs(dl, scale).map_err(|e| ExportError::Pdf(e.to_string()))?;
        let mut kpage = document.start_page_with(page_settings);
        let mut surface = kpage.surface();
        surface.push_transform(&Transform::from_scale(POINTS_PER_UNIT, POINTS_PER_UNIT));
        let mut cx = Cx {
            surface: &mut surface,
            assets,
            fonts: &mut fonts,
            table: &dl.fonts,
            jobs: jobs.into_iter(),
            quality: settings.shader_quality,
            readings: &readings,
            written: &written,
            page: KRect::from_xywh(0.0, 0.0, w, h),
            pixels: kurbo::Rect::new(0.0, 0.0, f64::from(dl.viewport[0] * scale), f64::from(dl.viewport[1] * scale)),
            photos: &mut photos,
            error: None,
        };
        let nodes = cx.read(&dl.ops, Affine::scale(f64::from(scale)));
        let error = cx.error.take();
        surface.pop();
        surface.finish();
        // Its links (PLAN 2.70): a web address opens, and a state goes to the page of its slide.
        for link in dl.links() {
            let [x, y, w, h] = link.bounds().map(|v| v * POINTS_PER_UNIT);
            let Some(rect) = KRect::from_xywh(x, y, w, h) else { continue };
            let (target, alt) = match &link.target {
                LinkTarget::Href(href) => (Target::Action(LinkAction::new(href.clone()).into()), href.clone()),
                LinkTarget::State(state) => {
                    let Some(at) = page_of(deck, pages, state) else { continue };
                    let destination = XyzDestination::new(at, Point::from_xy(0.0, 0.0));
                    (Target::Destination(destination.into()), format!("To page {}", at + 1))
                }
            };
            kpage.add_annotation(Annotation::new_link(LinkAnnotation::new(rect, target), Some(alt)));
        }
        kpage.finish();
        if let Some(e) = error {
            return Err(e);
        }
        structure.push(nodes);
    }

    let meta = deck.meta.as_ref();
    let lang = meta.and_then(|m| m.lang.clone());
    let (sections, rest) = reading_order(deck, pages);
    let mut taken: Vec<Option<Vec<Node>>> = structure.into_iter().map(Some).collect();
    let mut page = |i: usize| TagGroup::with_children(Tag::Div, taken[i].take().unwrap_or_default());
    let mut tree = TagTree::new().with_lang(lang.clone());
    let mut outline = Outline::new();
    let top = |i: usize| XyzDestination::new(i, Point::from_xy(0.0, 0.0));
    for (n, (section, at)) in sections.iter().enumerate() {
        let mut sect = TagGroup::new(Tag::Section);
        for &i in at {
            sect.push(page(i));
        }
        tree.push(sect);
        let first = at.iter().copied().min().unwrap_or_default();
        let named = heading(section, headlines.get(first).cloned().flatten(), n + 1);
        outline.push_child(OutlineNode::new(named, top(first)));
    }
    for &i in &rest {
        tree.push(page(i));
        // A page no beat names is bookmarked by what it says, never by an id (lift-up ATTR-5).
        if !sections.is_empty() {
            let named = headlines.get(i).cloned().flatten().unwrap_or_else(|| format!("Slide {}", i + 1));
            outline.push_child(OutlineNode::new(named, top(i)));
        }
    }
    document.set_tag_tree(tree);
    if !sections.is_empty() {
        document.set_outline(outline);
    }
    let mut metadata = Metadata::new().creator("Scaena".to_string());
    if let Some(title) = meta.and_then(|m| m.title.clone()) {
        metadata = metadata.title(title);
    }
    // Who wrote it, what it is about, and when it was made, from the deck and never the clock,
    // so the bytes stay the deck's own and the browser's PDF stays the CLI's (lift-up 10-24b
    // ATTR-4). krilla writes the one date as both the made and the modified date.
    if let Some(author) = meta.and_then(|m| m.author.clone()).filter(|a| !a.trim().is_empty()) {
        metadata = metadata.authors(vec![author]);
    }
    if let Some(description) = meta.and_then(|m| m.description.clone()).filter(|d| !d.trim().is_empty()) {
        metadata = metadata.description(description);
    }
    if let Some(made) = meta.and_then(|m| m.created.as_deref()).and_then(date) {
        metadata = metadata.creation_date(made);
    }
    if let Some(lang) = lang {
        metadata = metadata.language(lang);
    }
    document.set_metadata(metadata);
    document.finish().map_err(|e| ExportError::Pdf(format!("{e:?}")))
}

/// `text`, an ISO 8601 date and time as a deck writes `meta.created`, as a PDF's date: in UTC
/// where it ends in `Z`, as written where it names no zone. One with another offset, or none a
/// PDF can hold, is left out rather than guessed.
fn date(text: &str) -> Option<krilla::metadata::DateTime> {
    let utc = text.trim().ends_with('Z');
    let at = scaena_core::format::read_iso(text).ok()?.civil();
    let year = u16::try_from(at.year).ok()?;
    let d = krilla::metadata::DateTime::new(year)
        .month(at.month as u8)
        .day(at.day as u8)
        .hour(at.hour as u8)
        .minute(at.minute as u8)
        .second(at.second as u8);
    Some(if utc { d.utc_offset_hour(0).utc_offset_minute(0) } else { d })
}

/// The page that shows `state`: its slide's (PLAN 2.70). None where no page shows that slide.
fn page_of(deck: &Deck, pages: &[Page], state: &str) -> Option<usize> {
    let slide = deck.slide_of(deck.states.iter().find(|s| s.id == state)?);
    pages.iter().position(|p| deck.states.iter().find(|s| s.id == p.state).is_some_and(|s| deck.slide_of(s) == slide))
}

/// The pages in reading order (SPEC §3.12): each spine section with the pages of its
/// beats' slides, in spine order, then the pages no beat names, in page order.
fn reading_order<'d>(deck: &'d Deck, pages: &[Page]) -> (Vec<(&'d Section, Vec<usize>)>, Vec<usize>) {
    let slides: HashMap<&str, &str> = deck.states.iter().map(|s| (s.id.as_str(), deck.slide_of(s))).collect();
    let slide = |state: &str| slides.get(state).copied();
    let mut placed = vec![false; pages.len()];
    let mut sections = Vec::new();
    for section in deck.spine.iter().flat_map(|s| &s.sections) {
        let mut at = Vec::new();
        for named in section.beats.iter().flat_map(|b| &b.states).filter_map(|s| slide(s)) {
            for (i, page) in pages.iter().enumerate() {
                if !placed[i] && slide(&page.state) == Some(named) {
                    placed[i] = true;
                    at.push(i);
                }
            }
        }
        if !at.is_empty() {
            sections.push((section, at));
        }
    }
    let rest = (0..pages.len()).filter(|&i| !placed[i]).collect();
    (sections, rest)
}

/// What the `n`th section's bookmark says: its title, else its first beat's claim, else its first
/// page's headline, else "Section n"; never its id (lift-up ATTR-5).
fn heading(section: &Section, headline: Option<String>, n: usize) -> String {
    let said = |s: &String| !s.trim().is_empty();
    (section.title.clone().filter(said))
        .or_else(|| section.beats.first().map(|b| b.claim.clone()).filter(said))
        .or(headline.filter(said))
        .unwrap_or_else(|| format!("Section {n}"))
}

struct Cx<'a, 's> {
    surface: &'a mut Surface<'s>,
    assets: &'a Assets,
    fonts: &'a mut Fonts,
    /// The display list's font table.
    table: &'a [FontRef],
    /// One per shader op, in the order the walk meets them.
    jobs: std::vec::IntoIter<Option<Job>>,
    /// The JPEG quality of an opaque shader's image, or none to keep it whole.
    quality: Option<u8>,
    /// How each node on the page reads, by id.
    readings: &'a HashMap<String, Reading>,
    /// Each text on the page as written, by its node's id: its `text`, or its runs' texts
    /// end to end (PLAN 2.88).
    written: &'a HashMap<String, String>,
    /// The page, in points: what its background covers.
    page: Option<KRect>,
    /// The page in the pixels shaders and shadows are drawn in: what a shadow's image is cut to.
    pixels: kurbo::Rect,
    /// Each photo as the document carries it, by its id and whether it is filtered: made once.
    photos: &'a mut HashMap<(String, bool), Image>,
    /// The first thing that did not draw. Drawing goes on past it, so that every push
    /// is popped and every tagged section ended.
    error: Option<ExportError>,
}

impl Cx<'_, '_> {
    fn fail(&mut self, e: ExportError) {
        self.error.get_or_insert(e);
    }

    /// `ops` drawn where content may be tagged (in the page's own content, outside any
    /// tagged section), each node's content tagged as it reads, in paint order: a
    /// page's reading order (SPEC §3.12). Returns the structure they make.
    fn read(&mut self, ops: &[Op], xf: Affine) -> Vec<Node> {
        let mut out = Vec::new();
        for op in ops {
            let Op::Layer { node, transform, opacity, blend, clip, ops: inner, .. } = op else {
                // Drawn outside every node: the page's background.
                self.artifact(Artifact::new(ArtifactType::Background, self.page), op, xf);
                continue;
            };
            // A slide's number and footer (ADR-0028): pagination, which a reader passes over.
            if let Some(id) = node.as_deref().filter(|id| scaena_engine::furniture::is_furniture(id)) {
                let kind = if id == scaena_engine::furniture::NUMBER {
                    ArtifactType::PageNumber
                } else {
                    ArtifactType::Footer
                };
                self.artifact(Artifact::new(kind, None), op, xf);
                continue;
            }
            // A notes page's notes (`scaena_engine::notes`): a paragraph after the slide's figure.
            if node.as_deref() == Some(scaena_engine::notes::NOTES) {
                let id = self.tagged(op, xf);
                out.push(element(Tag::P.into(), None, vec![id.into()]));
                continue;
            }
            let reading = node.as_deref().and_then(|id| self.readings.get(id));
            let alt = reading.and_then(|r| r.alt.clone());
            // A layer drawn at an opacity or in a blend mode is a group (a form XObject)
            // in the PDF, and content in one can only be tagged as a whole.
            let whole = *opacity < 1.0 || *blend != Blend::Normal;
            match reading.map_or(Kind::Group, |r| r.kind) {
                Kind::Artifact => self.artifact(Artifact::new(ArtifactType::Layout, None), op, xf),
                // A text that is a list (ADR-0018): each paragraph's marker and words, drawn in
                // layers of their own, tagged as the list's labels and bodies.
                Kind::Heading(_) | Kind::Paragraph
                    if !whole && alt.is_none() && reading.is_some_and(|r| !r.items.is_empty()) =>
                {
                    let pushed = self.open(transform, *opacity, *blend, clip.as_ref());
                    let xf = xf * affine(transform);
                    let mut parts: BTreeMap<(u32, u32), Identifier> = BTreeMap::new();
                    for child in inner {
                        match child {
                            Op::Layer { cell: Some([paragraph, part]), .. } => {
                                let id = self.tagged(child, xf);
                                parts.insert((*paragraph, *part), id);
                            }
                            _ => self.artifact(Artifact::new(ArtifactType::Layout, None), child, xf),
                        }
                    }
                    self.close(pushed);
                    let items = &reading.expect("a list's reading").items;
                    let paras: Vec<Para> = items
                        .iter()
                        .enumerate()
                        .map(|(k, item)| {
                            let (label, body) = (parts.remove(&(k as u32, 0)), parts.remove(&(k as u32, 1)));
                            match item {
                                Some(item) => Para::Item(item.depth(), item.kind, label, body),
                                None => Para::Plain(body),
                            }
                        })
                        .collect();
                    out.push(element(Tag::Div.into(), reading, list_nodes(&paras)));
                }
                // A fetched picture's credit (ADR-0030): the picture tagged as the figure, then its
                // credit as a paragraph after it, with its links.
                Kind::Figure if !whole && inner.iter().any(is_credit) => {
                    let pushed = self.open(transform, *opacity, *blend, clip.as_ref());
                    let xf = xf * affine(transform);
                    let picture: Vec<Op> = inner.iter().filter(|op| !is_credit(op)).cloned().collect();
                    let figure = self.surface.start_tagged(ContentTag::Other);
                    self.ops(&picture, xf);
                    self.surface.end_tagged();
                    let credits: Vec<Identifier> =
                        inner.iter().filter(|op| is_credit(op)).map(|op| self.tagged(op, xf)).collect();
                    self.close(pushed);
                    out.push(element(Tag::Figure(alt.clone()).into(), reading, vec![figure.into()]));
                    for credit in credits {
                        out.push(element(Tag::P.into(), reading, vec![credit.into()]));
                    }
                }
                kind @ (Kind::Heading(_) | Kind::Paragraph | Kind::Figure) => {
                    let written = node.as_deref().and_then(|id| self.written.get(id)).map(String::as_str);
                    let id = self.spoken(op, xf, written.filter(|_| kind != Kind::Figure));
                    let tag: TagKind = match kind {
                        Kind::Heading(level) => {
                            Tag::Hn(NonZeroU16::new(level.into()).unwrap_or(NonZeroU16::MIN), None).into()
                        }
                        Kind::Paragraph => Tag::P.into(),
                        _ => Tag::Figure(alt.clone()).into(),
                    };
                    // A text's alt text is said instead of its words.
                    let tag = if kind == Kind::Figure { tag } else { tag.with_alt_text(alt) };
                    out.push(element(tag, reading, vec![id.into()]));
                }
                Kind::Table if !whole => {
                    let pushed = self.open(transform, *opacity, *blend, clip.as_ref());
                    let xf = xf * affine(transform);
                    let mut cells = BTreeMap::new();
                    let mut credits = Vec::new();
                    // What a cell that draws no glyphs says: a forecast's sky (ADR-0030).
                    let mut said: BTreeMap<(u32, u32), String> = BTreeMap::new();
                    for child in inner {
                        match child {
                            // Its data's credit (ADR-0030): a paragraph after it.
                            op if is_credit(op) => credits.push(self.tagged(op, xf)),
                            Op::Layer { cell: Some([row, column]), label, .. } => {
                                let id = self.tagged(child, xf);
                                cells.insert((*row, *column), id);
                                if let Some(label) = label {
                                    said.insert((*row, *column), label.clone());
                                }
                            }
                            // Its rules.
                            _ => self.artifact(Artifact::new(ArtifactType::Layout, None), child, xf),
                        }
                    }
                    let rows = self.rows(cells);
                    self.close(pushed);
                    let mut table = Vec::with_capacity(rows.len());
                    for (row, cells) in rows {
                        let cells = cells.into_iter().enumerate().map(|(column, id)| {
                            let cell: TagKind =
                                if row == 0 { Tag::TH(TableHeaderScope::Column).into() } else { Tag::TD.into() };
                            let cell = cell.with_alt_text(said.get(&(row, column as u32)).cloned());
                            Node::from(TagGroup::with_children(cell, vec![id.into()]))
                        });
                        table.push(Node::from(TagGroup::with_children(Tag::TR, cells.collect())));
                    }
                    out.push(element(Tag::Table.with_summary(alt).into(), reading, table));
                    for credit in credits {
                        out.push(element(Tag::P.into(), reading, vec![credit.into()]));
                    }
                }
                Kind::Group if !whole && alt.is_none() => {
                    let pushed = self.open(transform, *opacity, *blend, clip.as_ref());
                    out.extend(self.read(inner, xf * affine(transform)));
                    self.close(pushed);
                }
                // Read as one: a group with alt text, or a group or table drawn as one.
                Kind::Group | Kind::Table => {
                    if alt.is_some() {
                        let id = self.tagged(op, xf);
                        out.push(element(Tag::Figure(alt).into(), reading, vec![id.into()]));
                    } else if self.reads(inner) {
                        let id = self.tagged(op, xf);
                        out.push(element(Tag::Div.into(), reading, vec![id.into()]));
                    } else {
                        self.artifact(Artifact::new(ArtifactType::Layout, None), op, xf);
                    }
                }
            }
        }
        out
    }

    /// Whether anything in `ops` is read: a node that is not an artifact.
    fn reads(&self, ops: &[Op]) -> bool {
        ops.iter().any(|op| match op {
            Op::Layer { node, ops, .. } => match node.as_deref().and_then(|id| self.readings.get(id)) {
                Some(Reading { kind: Kind::Artifact, .. }) => false,
                Some(Reading { kind: Kind::Group, .. }) | None => self.reads(ops),
                Some(_) => true,
            },
            _ => false,
        })
    }

    /// `op` drawn as one tagged section of content.
    fn tagged(&mut self, op: &Op, xf: Affine) -> Identifier {
        self.spoken(op, xf, None)
    }

    /// `op` drawn as one tagged section of content, a text's words: where `written`, its
    /// text as written, differs from what the glyphs show by case alone, a span whose actual
    /// text is `written` (PLAN 2.88). A copy, a search, or a screen reader then takes "Annual
    /// meeting" where the page shows ANNUAL MEETING.
    fn spoken(&mut self, op: &Op, xf: Affine, written: Option<&str>) -> Identifier {
        let ops = std::slice::from_ref(op);
        let actual = written.and_then(|w| by_case(&reading::drawn(ops), w));
        let id = match &actual {
            Some(actual) => {
                let span = SpanTag { actual_text: Some(actual), ..SpanTag::empty() };
                self.surface.start_tagged(ContentTag::Span(span))
            }
            None => self.surface.start_tagged(ContentTag::Other),
        };
        self.ops(ops, xf);
        self.surface.end_tagged();
        id
    }

    /// `op` drawn as an artifact: content no one reads.
    fn artifact(&mut self, artifact: Artifact, op: &Op, xf: Affine) {
        self.surface.start_tagged(ContentTag::Artifact(artifact));
        self.ops(std::slice::from_ref(op), xf);
        self.surface.end_tagged();
    }

    /// A table's rows, its first drawn to its last, each with a cell per column: where a
    /// row has no text in a column (a null), an empty cell, tagged here.
    fn rows(&mut self, mut cells: BTreeMap<(u32, u32), Identifier>) -> Vec<(u32, Vec<Identifier>)> {
        let columns = cells.keys().map(|&(_, c)| c + 1).max().unwrap_or(0);
        let (Some(&(first, _)), Some(&(last, _))) = (cells.keys().next(), cells.keys().next_back()) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for row in first..=last {
            let mut line = Vec::with_capacity(columns as usize);
            for column in 0..columns {
                line.push(cells.remove(&(row, column)).unwrap_or_else(|| {
                    let id = self.surface.start_tagged(ContentTag::Other);
                    self.surface.end_tagged();
                    id
                }));
            }
            rows.push((row, line));
        }
        rows
    }

    /// Opens a layer: its transform, clip, blend, and opacity. Returns the pushes to pop.
    fn open(&mut self, transform: &[f32; 6], opacity: f32, blend: Blend, clip: Option<&Path>) -> usize {
        self.surface.push_transform(&matrix(transform));
        let mut pushed = 1;
        if let Some(clip) = clip.and_then(path) {
            self.surface.push_clip_path(&clip, &KRule::NonZero);
            pushed += 1;
        }
        // The layer composites as one: at its opacity, in its blend mode.
        if blend != Blend::Normal {
            self.surface.push_blend_mode(blend_mode(blend));
            pushed += 1;
        }
        if opacity < 1.0 {
            self.surface.push_opacity(unit(opacity));
            pushed += 1;
        } else if blend != Blend::Normal {
            self.surface.push_isolated();
            pushed += 1;
        }
        pushed
    }

    fn close(&mut self, pushed: usize) {
        for _ in 0..pushed {
            self.surface.pop();
        }
    }

    /// `ops` drawn in the current coordinates, which `xf` maps to shader pixels.
    fn ops(&mut self, ops: &[Op], xf: Affine) {
        for op in ops {
            match op {
                Op::Layer { transform, opacity, blend, clip, ops, .. } => {
                    let pushed = self.open(transform, *opacity, *blend, clip.as_ref());
                    self.ops(ops, xf * affine(transform));
                    self.close(pushed);
                }
                Op::Fill { path: p, rule, paint } => {
                    if let Some(p) = path(p) {
                        let (paint, opacity) = kpaint(paint);
                        let rule = match rule {
                            FillRule::NonZero => KRule::NonZero,
                            FillRule::EvenOdd => KRule::EvenOdd,
                        };
                        self.surface.set_stroke(None);
                        self.surface.set_fill(Some(Fill { paint, opacity, rule }));
                        self.surface.draw_path(&p);
                    }
                }
                Op::Stroke { path: p, paint, width, cap, join, miter_limit, dash, dash_offset } => {
                    if let Some(p) = path(p) {
                        let (paint, opacity) = kpaint(paint);
                        let dash = (!dash.is_empty()).then(|| StrokeDash { array: dash.clone(), offset: *dash_offset });
                        self.surface.set_fill(None);
                        self.surface.set_stroke(Some(Stroke {
                            paint,
                            width: *width,
                            miter_limit: *miter_limit,
                            line_cap: match cap {
                                Cap::Butt => LineCap::Butt,
                                Cap::Round => LineCap::Round,
                                Cap::Square => LineCap::Square,
                            },
                            line_join: match join {
                                Join::Miter => LineJoin::Miter,
                                Join::Round => LineJoin::Round,
                                Join::Bevel => LineJoin::Bevel,
                            },
                            opacity,
                            dash,
                        }));
                        self.surface.draw_path(&p);
                        self.surface.set_stroke(None);
                    }
                }
                Op::Glyphs { font: index, size, coords, paint, text, glyphs, clusters, .. } => {
                    let Some(first) = glyphs.first() else { continue };
                    let font = match self.font(*index, coords) {
                        Ok(font) => font,
                        Err(e) => {
                            self.fail(e);
                            continue;
                        }
                    };
                    let (paint, opacity) = kpaint(paint);
                    self.surface.set_stroke(None);
                    self.surface.set_fill(Some(Fill { paint, opacity, rule: KRule::NonZero }));
                    let last = glyphs.last().map_or(0.0, |g| self.advance(*index, coords, *size, g.id));
                    let placed = place(glyphs, *size, text, clusters, last);
                    self.surface.draw_glyphs(Point::from_xy(first.x, first.y), &placed, font, text, *size, false);
                }
                Op::Image { asset, src, dst, quality } => {
                    if let Err(e) = self.image(asset, *src, *dst, *quality) {
                        self.fail(e);
                    }
                }
                // A link's area draws nothing (PLAN 2.70): its annotation is the page's.
                Op::Link { .. } => {}
                Op::Shadow { .. } => {
                    if let Err(e) = self.shadow(op, xf) {
                        self.fail(e);
                    }
                }
                Op::Shader { rect, .. } => {
                    let job = self.jobs.next().expect("shader_jobs makes one job per shader op");
                    if let Some(job) = job
                        && let Err(e) = self.shader(&job, *rect, xf)
                    {
                        self.fail(e);
                    }
                }
            }
        }
    }

    /// Glyph `id`'s advance at `size` in font `index`, at the instance `coords` name: the
    /// room a run's last glyph takes, which the display list does not say. A run's box
    /// in a group reaches as far as its advances (0 where the font does not say).
    fn advance(&self, index: u32, coords: &[i16], size: f32, id: u32) -> f32 {
        use skrifa::MetadataProvider;
        use skrifa::instance::{LocationRef, NormalizedCoord, Size};
        let Some(font) = self.table.get(index as usize) else { return 0.0 };
        let Ok(data) = self.assets.font_data(font) else { return 0.0 };
        let Ok(face) = skrifa::FontRef::from_index(data.data.data(), font.index) else { return 0.0 };
        let location: Vec<NormalizedCoord> = coords.iter().map(|&c| NormalizedCoord::from_bits(c)).collect();
        face.glyph_metrics(Size::new(size), LocationRef::new(&location))
            .advance_width(skrifa::GlyphId::new(id))
            .unwrap_or(0.0)
    }

    /// Font `index` of the display list's table, at the instance `coords` name.
    fn font(&mut self, index: u32, coords: &[i16]) -> Result<Font, ExportError> {
        let font = self
            .table
            .get(index as usize)
            .ok_or_else(|| ExportError::Pdf(format!("display list names font index {index}, past its font table")))?;
        self.fonts.get(self.assets, font, coords)
    }

    /// The `src` part of image `asset` drawn into `dst`, clipped to it. A photo goes in as its
    /// own JPEG, what it says beyond its picture left out, turned as it is seen (ADR-0017); any
    /// other image, as its pixels.
    fn image(&mut self, asset: &str, src: [f32; 4], dst: [f32; 4], quality: Quality) -> Result<(), ExportError> {
        // A picture whose pixels do not decode leaves its box empty (E107).
        let picture = match self.assets.image(asset) {
            Err(e) if e.undecodable() => return Ok(()),
            picture => picture.map_err(|e| ExportError::Pdf(e.to_string()))?,
        };
        let filtered = quality == Quality::High;
        let (image, size, turn) = match &picture.jpeg {
            Some(jpeg) => {
                let key = (asset.to_string(), filtered);
                let image = match self.photos.get(&key) {
                    Some(image) => image.clone(),
                    None => {
                        let bare = scaena_core::jpeg::stripped(jpeg.bytes.data())
                            .map_err(|e| ExportError::Pdf(format!("{asset}: {e}")))?;
                        let image = Image::from_jpeg(bare.into(), filtered).map_err(ExportError::Pdf)?;
                        self.photos.insert(key, image.clone());
                        image
                    }
                };
                let (w, h) = (jpeg.width as f32, jpeg.height as f32);
                (image, Size::from_wh(w, h), turned(jpeg.orientation, w, h))
            }
            None => {
                let image =
                    Image::from_custom(Pixels::of_picture(asset, &picture), filtered).map_err(ExportError::Pdf)?;
                (image, Size::from_wh(picture.width as f32, picture.height as f32), Transform::identity())
            }
        };
        let (Some(clip), Some(size)) = (path(&Path::rect(dst)), size) else {
            return Ok(());
        };
        let [sx, sy, sw, sh] = src;
        let [dx, dy, dw, dh] = dst;
        let (kx, ky) = (dw / sw, dh / sh);
        self.surface.push_clip_path(&clip, &KRule::NonZero);
        self.surface.push_transform(&Transform::from_row(kx, 0.0, 0.0, ky, dx - sx * kx, dy - sy * ky));
        self.surface.push_transform(&turn);
        self.surface.draw_image(image, size);
        self.surface.pop();
        self.surface.pop();
        self.surface.pop();
        Ok(())
    }

    /// A shadow (PLAN 2.115) as the CPU painter blurs it, an image of what it reaches on the page
    /// with its alpha kept: PDF has no blur. Drawn at the scale shaders are, pixel for pixel, and
    /// cut to the page, as the SVG's is, so a large one is no larger an image than the page.
    fn shadow(&mut self, op: &Op, xf: Affine) -> Result<(), ExportError> {
        let Op::Shadow { rect: [x, y, w, h], blur, .. } = op else { return Ok(()) };
        let reach = f64::from(*blur) * 3.0;
        let area =
            kurbo::Rect::new(f64::from(*x), f64::from(*y), f64::from(x + w), f64::from(y + h)).inflate(reach, reach);
        let device = xf.transform_rect_bbox(area).intersect(self.pixels);
        let (x0, y0, x1, y1) = (device.x0.floor(), device.y0.floor(), device.x1.ceil(), device.y1.ceil());
        if !(x1 - x0 >= 1.0 && y1 - y0 >= 1.0) {
            return Ok(());
        }
        let mut list = DisplayList::new([(x1 - x0) as f32, (y1 - y0) as f32]);
        list.ops.push(Op::Layer {
            node: None,
            cell: None,
            label: None,
            transform: (Affine::translate((-x0, -y0)) * xf).as_coeffs().map(|v| v as f32),
            opacity: 1.0,
            blend: Blend::Normal,
            clip: None,
            ops: vec![op.clone()],
        });
        let raster =
            CpuPainter::default().paint(&list, self.assets, 1.0).map_err(|e| ExportError::Pdf(e.to_string()))?;
        let Some(size) = Size::from_wh(raster.width as f32, raster.height as f32) else { return Ok(()) };
        let pixels = Pixels::of_rgba(raster.rgba, raster.width, raster.height);
        let image = Image::from_custom(pixels, true).map_err(ExportError::Pdf)?;
        // Pixel (0, 0) on device pixel (x0, y0): undo the layers' transforms and the scale.
        self.surface.push_transform(&from_affine(xf.inverse() * Affine::translate((x0, y0))));
        self.surface.draw_image(image, size);
        self.surface.pop();
        Ok(())
    }

    /// The CPU reference's pixels for `job` inside `rect`, texel for shader pixel.
    fn shader(&mut self, job: &Job, rect: [f32; 4], xf: Affine) -> Result<(), ExportError> {
        let [x, y, w, h] = job.bbox();
        let (Some(clip), Some(size)) = (path(&Path::rect(rect)), Size::from_wh(w as f32, h as f32)) else {
            return Ok(());
        };
        let rgba = job.render_on(scaena_core::shader::cores());
        let image = match self.quality.and_then(|q| jpeg(&rgba, w, h, q)) {
            Some(jpeg) => Image::from_jpeg(jpeg.into(), true),
            None => Image::from_custom(Pixels::of_rgba(rgba, w, h), true),
        }
        .map_err(ExportError::Pdf)?;
        self.surface.push_clip_path(&clip, &KRule::NonZero);
        // Texel (0, 0) on shader pixel (x, y): undo the layers' transforms and the scale.
        let place = xf.inverse() * Affine::translate((f64::from(x), f64::from(y)));
        self.surface.push_transform(&from_affine(place));
        self.surface.draw_image(image, size);
        self.surface.pop();
        self.surface.pop();
        Ok(())
    }
}

/// What turns a JPEG stored `w` × `h` to be seen by EXIF orientation `o`: the stored picture's
/// space onto the space of the picture as seen, which is `h` × `w` where `o` turns it a quarter.
fn turned(o: u8, w: f32, h: f32) -> Transform {
    match o {
        2 => Transform::from_row(-1.0, 0.0, 0.0, 1.0, w, 0.0),
        3 => Transform::from_row(-1.0, 0.0, 0.0, -1.0, w, h),
        4 => Transform::from_row(1.0, 0.0, 0.0, -1.0, 0.0, h),
        5 => Transform::from_row(0.0, 1.0, 1.0, 0.0, 0.0, 0.0),
        6 => Transform::from_row(0.0, 1.0, -1.0, 0.0, h, 0.0),
        7 => Transform::from_row(0.0, -1.0, -1.0, 0.0, h, w),
        8 => Transform::from_row(0.0, -1.0, 1.0, 0.0, 0.0, w),
        _ => Transform::identity(),
    }
}

/// `rgba`, `w` × `h` pixels, as a JPEG at `quality`, if every pixel is opaque: JPEG has
/// no alpha, and a shader drawn translucent keeps what is under it.
pub fn jpeg(rgba: &[u8], w: u32, h: u32, quality: u8) -> Option<Vec<u8>> {
    let pixels = rgba.as_chunks::<4>().0;
    if pixels.iter().any(|p| p[3] != 255) {
        return None;
    }
    let rgb: Vec<u8> = pixels.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100))
        .encode(&rgb, w, h, image::ExtendedColorType::Rgb8)
        .ok()?;
    Some(out)
}

/// A node's element: `tag`, in the node's language, over `children`.
/// A text's paragraph as a list reads it (ADR-0018): an item at its level, of its kind, with
/// the content its marker and its words were drawn in; or a paragraph that is no item.
enum Para {
    Item(u8, ListKind, Option<Identifier>, Option<Identifier>),
    Plain(Option<Identifier>),
}

/// `paras` as structure: each run of items a list (`L`) of items (`LI`), each its label
/// (`Lbl`) and body (`LBody`), with the deeper items after it a list inside it; each paragraph
/// that is no item a `P`.
fn list_nodes(paras: &[Para]) -> Vec<Node> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < paras.len() {
        match &paras[i] {
            Para::Plain(body) => {
                out.push(TagGroup::with_children(Tag::P, body.iter().map(|&id| id.into()).collect()).into());
                i += 1;
            }
            &Para::Item(level, kind, ..) => {
                let (list, next) = list_at(paras, i, level, kind);
                out.push(list);
                i = next;
            }
        }
    }
    out
}

/// The list of `kind` at `level` that starts at `paras[i]`, and where it ends.
fn list_at(paras: &[Para], mut i: usize, level: u8, kind: ListKind) -> (Node, usize) {
    let mut items = Vec::new();
    while let Some(&Para::Item(l, k, label, body)) = paras.get(i) {
        if l != level || k != kind {
            break;
        }
        let mut item: Vec<Node> = Vec::new();
        if let Some(id) = label {
            item.push(TagGroup::with_children(Tag::Lbl, vec![id.into()]).into());
        }
        item.push(TagGroup::with_children(Tag::LBody, body.iter().map(|&id| id.into()).collect()).into());
        i += 1;
        while let Some(&Para::Item(deeper, k, ..)) = paras.get(i) {
            if deeper <= level {
                break;
            }
            let (inner, next) = list_at(paras, i, deeper, k);
            item.push(inner);
            i = next;
        }
        items.push(TagGroup::with_children(Tag::LI, item).into());
    }
    let numbering = match kind {
        ListKind::Bullet => ListNumbering::Disc,
        ListKind::Number => ListNumbering::Decimal,
    };
    (TagGroup::with_children(Tag::L(numbering), items).into(), i)
}

/// Whether `op` is the layer a fetched picture's credit is drawn in, or the credit at the foot of a
/// chart or a table whose data was fetched (ADR-0030).
fn is_credit(op: &Op) -> bool {
    use scaena_core::fetched::{CREDIT_CELL, FOOT_CELL};
    matches!(op, Op::Layer { cell: Some(cell), .. } if *cell == CREDIT_CELL || *cell == FOOT_CELL)
}

fn element(tag: TagKind, reading: Option<&Reading>, children: Vec<Node>) -> Node {
    let lang = reading.and_then(|r| r.lang.clone());
    TagGroup::with_children(tag.with_lang(lang), children).into()
}

/// Fonts made once per document: a font's file at an instance.
#[derive(Default)]
struct Fonts(HashMap<(FontRef, Vec<i16>), Font>);

impl Fonts {
    fn get(&mut self, assets: &Assets, font: &FontRef, coords: &[i16]) -> Result<Font, ExportError> {
        let key = (font.clone(), coords.to_vec());
        if let Some(f) = self.0.get(&key) {
            return Ok(f.clone());
        }
        let data = assets.font_data(font).map_err(|e| ExportError::Pdf(e.to_string()))?;
        let (bytes, _) = data.data.clone().into_raw_parts();
        let made = match user_coords(data.data.data(), font.index, coords) {
            Some(axes) if !axes.is_empty() => Font::new_variable(bytes.into(), font.index, &axes),
            _ => Font::new(bytes.into(), font.index),
        }
        .ok_or_else(|| ExportError::Pdf(format!("font {} does not read", font.id)))?;
        self.0.insert(key, made.clone());
        Ok(made)
    }
}

/// The user-space axis values that normalize to `coords` (F2Dot14, the font's `fvar`
/// order, `avar` applied): krilla takes an instance by its axis values. Each axis is
/// found by bisection, as normalizing is monotonic.
fn user_coords(bytes: &[u8], index: u32, coords: &[i16]) -> Option<Vec<(krilla::text::Tag, f32)>> {
    use skrifa::MetadataProvider;
    let font = skrifa::FontRef::from_index(bytes, index).ok()?;
    let axes = font.axes();
    let mut out = Vec::new();
    for (i, axis) in axes.iter().enumerate() {
        let want = coords.get(i).copied().unwrap_or(0);
        let normal = |v: f32| axes.location([(axis.tag(), v)]).coords().get(i).map_or(0, |c| c.to_bits());
        let (mut lo, mut hi) = (axis.min_value(), axis.max_value());
        let mut value = axis.default_value();
        if want != 0 {
            for _ in 0..40 {
                let mid = 0.5 * (lo + hi);
                match normal(mid).cmp(&want) {
                    std::cmp::Ordering::Less => lo = mid,
                    std::cmp::Ordering::Greater => hi = mid,
                    std::cmp::Ordering::Equal => {
                        value = mid;
                        break;
                    }
                }
                value = mid;
            }
        }
        let tag = axis.tag().to_be_bytes();
        out.push((krilla::text::Tag::new(&tag), value));
    }
    Some(out)
}

/// A glyph where the display list puts it, saying its cluster. krilla takes a glyph's
/// metrics in ems and scales them by the size it asks at.
struct Placed {
    id: u32,
    text: Range<usize>,
    /// To the next glyph, in ems.
    advance: f32,
    /// Above the run's first baseline, in ems: up is positive, as in OpenType.
    rise: f32,
}

impl krilla::text::Glyph for Placed {
    fn glyph_id(&self) -> GlyphId {
        GlyphId::new(self.id)
    }
    fn text_range(&self) -> Range<usize> {
        self.text.clone()
    }
    fn x_advance(&self, size: f32) -> f32 {
        self.advance * size
    }
    fn x_offset(&self, _: f32) -> f32 {
        0.0
    }
    fn y_offset(&self, size: f32) -> f32 {
        self.rise * size
    }
    fn y_advance(&self, _: f32) -> f32 {
        0.0
    }
    fn location(&self) -> Option<krilla::surface::Location> {
        None
    }
}

/// The run's glyphs at `size`, placed from the first: each advances to the next, the
/// last by `last`, and sits as far off the first's baseline as the display list puts
/// it. Each says its cluster: from its start to the next larger one, or the end of
/// `text`.
fn place(
    glyphs: &[scaena_core::displaylist::Glyph],
    size: f32,
    text: &str,
    clusters: &[u32],
    last: f32,
) -> Vec<Placed> {
    let mut starts: Vec<usize> = clusters.iter().map(|&c| c as usize).collect();
    starts.sort_unstable();
    starts.dedup();
    let range = |i: usize| match clusters.get(i) {
        Some(&c) => {
            let c = c as usize;
            c..starts.get(starts.partition_point(|&s| s <= c)).copied().unwrap_or(text.len())
        }
        None => 0..0,
    };
    let y0 = glyphs[0].y;
    let em = if size > 0.0 { size.recip() } else { 0.0 };
    (0..glyphs.len())
        .map(|i| Placed {
            id: glyphs[i].id,
            text: range(i),
            advance: glyphs.get(i + 1).map_or(last, |next| next.x - glyphs[i].x) * em,
            rise: (y0 - glyphs[i].y) * em,
        })
        .collect()
}

/// Straight-alpha RGBA8 pixels as a PDF image: the color in one channel, the alpha in
/// a soft mask, keyed by what they are.
#[derive(Clone)]
struct Pixels(Arc<Planes>);

struct Planes {
    rgb: Vec<u8>,
    /// None when every pixel is opaque.
    alpha: Option<Vec<u8>>,
    size: (u32, u32),
    key: u128,
}

impl Pixels {
    fn of_picture(id: &str, picture: &scaena_paint::Picture) -> Pixels {
        let mut hasher = std::hash::DefaultHasher::new();
        id.hash(&mut hasher);
        Pixels::split(picture.rgba.data(), picture.width, picture.height, u128::from(hasher.finish()))
    }

    fn of_rgba(rgba: Vec<u8>, w: u32, h: u32) -> Pixels {
        let mut hasher = std::hash::DefaultHasher::new();
        rgba.hash(&mut hasher);
        Pixels::split(&rgba, w, h, u128::from(hasher.finish()) << 1 | 1)
    }

    fn split(rgba: &[u8], w: u32, h: u32, key: u128) -> Pixels {
        let pixels = rgba.as_chunks::<4>().0;
        let rgb: Vec<u8> = pixels.iter().flat_map(|&[r, g, b, _]| [r, g, b]).collect();
        let alpha: Vec<u8> = pixels.iter().map(|p| p[3]).collect();
        let alpha = alpha.iter().any(|&a| a != 255).then_some(alpha);
        Pixels(Arc::new(Planes { rgb, alpha, size: (w, h), key }))
    }
}

impl Hash for Pixels {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.key.hash(state);
    }
}

impl CustomImage for Pixels {
    fn color_channel(&self) -> &[u8] {
        &self.0.rgb
    }
    fn alpha_channel(&self) -> Option<&[u8]> {
        self.0.alpha.as_deref()
    }
    fn bits_per_component(&self) -> BitsPerComponent {
        BitsPerComponent::Eight
    }
    fn size(&self) -> (u32, u32) {
        self.0.size
    }
    fn icc_profile(&self) -> Option<&[u8]> {
        None
    }
    fn color_space(&self) -> ImageColorspace {
        ImageColorspace::Rgb
    }
}

fn unit(v: f32) -> NormalizedF32 {
    NormalizedF32::new(v.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE)
}

fn rgb_of(c: Color) -> rgb::Color {
    rgb::Color::new(c.0[0], c.0[1], c.0[2])
}

fn alpha_of(c: Color) -> NormalizedF32 {
    unit(f32::from(c.0[3]) / 255.0)
}

/// A display-list paint as krilla's, and the opacity it fills or strokes at.
fn kpaint(paint: &Paint) -> (KPaint, NormalizedF32) {
    let stops = |stops: &[scaena_core::displaylist::Stop]| -> Vec<KStop> {
        scaena_paint::srgb_stops(stops)
            .into_iter()
            .map(|s| KStop { offset: unit(s.0), color: rgb_of(s.1).into(), opacity: alpha_of(s.1) })
            .collect()
    };
    match paint {
        Paint::Solid(c) => (rgb_of(*c).into(), alpha_of(*c)),
        Paint::Linear { start, end, stops: s } => (
            LinearGradient {
                x1: start[0],
                y1: start[1],
                x2: end[0],
                y2: end[1],
                transform: Transform::identity(),
                spread_method: SpreadMethod::Pad,
                stops: stops(s),
                anti_alias: false,
            }
            .into(),
            NormalizedF32::ONE,
        ),
        Paint::Radial { center, radius, stops: s } => (
            RadialGradient {
                fx: center[0],
                fy: center[1],
                fr: 0.0,
                cx: center[0],
                cy: center[1],
                cr: *radius,
                transform: Transform::identity(),
                spread_method: SpreadMethod::Pad,
                stops: stops(s),
                anti_alias: false,
            }
            .into(),
            NormalizedF32::ONE,
        ),
        Paint::Sweep { center, start_angle, end_angle, stops: s } => (
            SweepGradient {
                cx: center[0],
                cy: center[1],
                start_angle: start_angle.to_degrees(),
                end_angle: end_angle.to_degrees(),
                transform: Transform::identity(),
                spread_method: SpreadMethod::Repeat,
                stops: stops(s),
                anti_alias: false,
            }
            .into(),
            NormalizedF32::ONE,
        ),
    }
}

fn blend_mode(blend: Blend) -> krilla::blend::BlendMode {
    use krilla::blend::BlendMode as B;
    match blend {
        Blend::Normal => B::Normal,
        Blend::Multiply => B::Multiply,
        Blend::Screen => B::Screen,
        Blend::Overlay => B::Overlay,
        Blend::Darken => B::Darken,
        Blend::Lighten => B::Lighten,
        Blend::Difference => B::Difference,
    }
}

fn path(p: &Path) -> Option<KPath> {
    let mut b = PathBuilder::new();
    for el in &p.0 {
        match *el {
            PathEl::MoveTo([x, y]) => b.move_to(x, y),
            PathEl::LineTo([x, y]) => b.line_to(x, y),
            PathEl::QuadTo([x1, y1], [x, y]) => b.quad_to(x1, y1, x, y),
            PathEl::CurveTo([x1, y1], [x2, y2], [x, y]) => b.cubic_to(x1, y1, x2, y2, x, y),
            PathEl::Close => b.close(),
        }
    }
    b.finish()
}

/// `[a, b, c, d, e, f]`: x' = a·x + c·y + e, y' = b·x + d·y + f.
fn matrix(m: &[f32; 6]) -> Transform {
    Transform::from_row(m[0], m[1], m[2], m[3], m[4], m[5])
}

fn affine(m: &[f32; 6]) -> Affine {
    Affine::new(m.map(f64::from))
}

fn from_affine(a: Affine) -> Transform {
    let [a, b, c, d, e, f] = a.as_coeffs().map(|v| v as f32);
    Transform::from_row(a, b, c, d, e, f)
}

/// A text's words as written: its `text`, or its runs' texts end to end. None for a node that
/// sets no text.
fn as_written(props: &scaena_core::document::Props) -> Option<String> {
    if let Some(text) = props.get("text").and_then(Value::as_str) {
        return Some(text.to_string());
    }
    let runs = props.get("runs")?.as_array()?;
    Some(runs.iter().filter_map(|r| r.get("text").and_then(Value::as_str)).collect())
}

/// `written`, where the text a node's glyphs show (`shown`) says the same words in other
/// letters' case: capitals, lower case, or title case, as a theme's role sets them. Each is
/// taken with its spaces and line breaks as single spaces and its soft hyphens left out.
/// None where they read alike, or say other words (a list's markers, a figure).
fn by_case(shown: &str, written: &str) -> Option<String> {
    let plain = |s: &str| s.replace('\u{AD}', "").split_whitespace().collect::<Vec<_>>().join(" ");
    let (shown, written) = (plain(shown), plain(written));
    (shown != written && shown.to_uppercase() == written.to_uppercase()).then_some(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use scaena_core::displaylist::Color;

    /// A deck of one empty state on a canvas `w` × `h`, and a page of it that draws `ops`.
    fn page(w: f32, h: f32, ops: Vec<Op>) -> (Deck, Page) {
        let deck = serde_json::json!({
            "scaena": scaena_core::FORMAT_VERSION,
            "canvas": { "width": w, "height": h },
            "nodes": {},
            "states": [{ "id": "only" }]
        });
        let deck = Deck::from_json(&deck.to_string()).unwrap();
        let mut list = DisplayList::new([w, h]);
        list.ops = ops;
        (deck, Page { state: "only".into(), list })
    }

    /// A shadow larger than the page is drawn as far as the page, which a raster holds, and not
    /// as an image past what one may be (PLAN 2.115).
    #[test]
    fn a_shadow_larger_than_the_page_is_drawn_to_its_edge() {
        let black = Color([0, 0, 0, 64]);
        for (w, h, blur) in [(3840.0, 2160.0, 10.0), (1920.0, 1080.0, 500.0)] {
            let shadow = Op::Shadow { rect: [0.0, 0.0, w, h], radius: 0.0, blur, color: black };
            let (deck, page) = page(w, h, vec![shadow]);
            let written = pdf(&deck, &[page], &Assets::default(), &PdfSettings::default());
            assert!(written.is_ok(), "{w} × {h}, a blur of {blur}: {written:?}");
        }
    }
}
