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
//!   names. A page reads in paint order, each node as [`crate::reading`] says: a heading
//!   or paragraph, a figure with its alt text, a table by rows of header and data
//!   cells. What no node reads (the page's background, decoration, a container's panel)
//!   is an artifact. The spine's sections are the document's outline.

use crate::ExportError;
use crate::reading::{self, Kind, Reading};
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
    Artifact, ArtifactType, ContentTag, Identifier, Node, TableHeaderScope, Tag, TagGroup, TagKind, TagTree,
};
use krilla::text::{Font, GlyphId};
use krilla::{Document, SerializeSettings};
use kurbo::Affine;
use scaena_core::Deck;
use scaena_core::displaylist::{
    Blend, Cap, Color, DisplayList, FillRule, FontRef, Join, Op, Paint, Path, PathEl, Quality,
};
use scaena_core::document::Section;
use scaena_core::shader::Job;
use scaena_paint::Assets;
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::num::NonZeroU16;
use std::ops::Range;
use std::sync::Arc;

/// Points to a canvas unit: a page is its canvas at 2 units to the point.
pub const POINTS_PER_UNIT: f32 = 0.5;

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

/// `deck` as a tagged PDF of `pages`, in order, drawing from `assets`.
pub fn pdf(deck: &Deck, pages: &[Page], assets: &Assets, settings: &PdfSettings) -> Result<Vec<u8>, ExportError> {
    let snapshots = scaena_core::resolve_states(deck).map_err(|e| ExportError::Pdf(e.to_string()))?;
    let mut document = Document::new_with(SerializeSettings::default());
    let mut fonts = Fonts::default();
    let mut structure: Vec<Vec<Node>> = Vec::with_capacity(pages.len());
    for page in pages {
        let dl = &page.list;
        let readings = match snapshots.iter().find(|s| s.state_id == page.state) {
            Some(snap) => reading::readings(deck, snap),
            None => return Err(ExportError::Pdf(format!("`{}` is not a state of the deck", page.state))),
        };
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
            page: KRect::from_xywh(0.0, 0.0, w, h),
            error: None,
        };
        let nodes = cx.read(&dl.ops, Affine::scale(f64::from(scale)));
        let error = cx.error.take();
        surface.pop();
        surface.finish();
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
    for (section, at) in &sections {
        let mut sect = TagGroup::new(Tag::Section);
        for &i in at {
            sect.push(page(i));
        }
        tree.push(sect);
        let first = at.iter().copied().min().unwrap_or_default();
        outline.push_child(OutlineNode::new(heading(section), XyzDestination::new(first, Point::from_xy(0.0, 0.0))));
    }
    for i in rest {
        tree.push(page(i));
    }
    document.set_tag_tree(tree);
    if !sections.is_empty() {
        document.set_outline(outline);
    }
    let mut metadata = Metadata::new().creator("Scaena".to_string());
    if let Some(title) = meta.and_then(|m| m.title.clone()) {
        metadata = metadata.title(title);
    }
    if let Some(lang) = lang {
        metadata = metadata.language(lang);
    }
    document.set_metadata(metadata);
    document.finish().map_err(|e| ExportError::Pdf(format!("{e:?}")))
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

/// What a section's bookmark says: its title, else its first beat's claim.
fn heading(section: &Section) -> String {
    (section.title.clone())
        .or_else(|| section.beats.first().map(|b| b.claim.clone()))
        .unwrap_or_else(|| section.id.clone())
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
    /// The page, in points: what its background covers.
    page: Option<KRect>,
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
            let reading = node.as_deref().and_then(|id| self.readings.get(id));
            let alt = reading.and_then(|r| r.alt.clone());
            // A layer drawn at an opacity or in a blend mode is a group (a form XObject)
            // in the PDF, and content in one can only be tagged as a whole.
            let whole = *opacity < 1.0 || *blend != Blend::Normal;
            match reading.map_or(Kind::Group, |r| r.kind) {
                Kind::Artifact => self.artifact(Artifact::new(ArtifactType::Layout, None), op, xf),
                kind @ (Kind::Heading(_) | Kind::Paragraph | Kind::Figure) => {
                    let id = self.tagged(op, xf);
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
                    for child in inner {
                        match child {
                            Op::Layer { cell: Some([row, column]), .. } => {
                                let id = self.tagged(child, xf);
                                cells.insert((*row, *column), id);
                            }
                            // Its rules.
                            _ => self.artifact(Artifact::new(ArtifactType::Layout, None), child, xf),
                        }
                    }
                    let rows = self.rows(cells);
                    self.close(pushed);
                    let mut table = Vec::with_capacity(rows.len());
                    for (row, cells) in rows {
                        let cells = cells.into_iter().map(|id| {
                            let cell: TagKind =
                                if row == 0 { Tag::TH(TableHeaderScope::Column).into() } else { Tag::TD.into() };
                            Node::from(TagGroup::with_children(cell, vec![id.into()]))
                        });
                        table.push(Node::from(TagGroup::with_children(Tag::TR, cells.collect())));
                    }
                    out.push(element(Tag::Table.with_summary(alt).into(), reading, table));
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
        let id = self.surface.start_tagged(ContentTag::Other);
        self.ops(std::slice::from_ref(op), xf);
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
                Op::Glyphs { font: index, size, coords, paint, text, glyphs, clusters } => {
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

    /// The `src` part of image `asset` drawn into `dst`, clipped to it.
    fn image(&mut self, asset: &str, src: [f32; 4], dst: [f32; 4], quality: Quality) -> Result<(), ExportError> {
        let picture = self.assets.image(asset).map_err(|e| ExportError::Pdf(e.to_string()))?;
        let image = Image::from_custom(Pixels::of_picture(asset, picture), quality == Quality::High)
            .map_err(ExportError::Pdf)?;
        let (Some(clip), Some(size)) =
            (path(&Path::rect(dst)), Size::from_wh(picture.width as f32, picture.height as f32))
        else {
            return Ok(());
        };
        let [sx, sy, sw, sh] = src;
        let [dx, dy, dw, dh] = dst;
        let (kx, ky) = (dw / sw, dh / sh);
        self.surface.push_clip_path(&clip, &KRule::NonZero);
        self.surface.push_transform(&Transform::from_row(kx, 0.0, 0.0, ky, dx - sx * kx, dy - sy * ky));
        self.surface.draw_image(image, size);
        self.surface.pop();
        self.surface.pop();
        Ok(())
    }

    /// The CPU reference's pixels for `job` inside `rect`, texel for shader pixel.
    fn shader(&mut self, job: &Job, rect: [f32; 4], xf: Affine) -> Result<(), ExportError> {
        let [x, y, w, h] = job.bbox();
        let (Some(clip), Some(size)) = (path(&Path::rect(rect)), Size::from_wh(w as f32, h as f32)) else {
            return Ok(());
        };
        let rgba = job.render();
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

/// `rgba`, `w` × `h` pixels, as a JPEG at `quality`, if every pixel is opaque: JPEG has
/// no alpha, and a shader drawn translucent keeps what is under it.
fn jpeg(rgba: &[u8], w: u32, h: u32, quality: u8) -> Option<Vec<u8>> {
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
