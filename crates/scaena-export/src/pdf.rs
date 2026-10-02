//! PDF (PLAN 1.20; SPEC §10): pages drawn from display lists by `krilla`.
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

use crate::ExportError;
use krilla::color::rgb;
use krilla::geom::{Path as KPath, PathBuilder, Point, Size, Transform};
use krilla::image::{BitsPerComponent, CustomImage, Image, ImageColorspace};
use krilla::metadata::Metadata;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{
    Fill, FillRule as KRule, LineCap, LineJoin, LinearGradient, Paint as KPaint, RadialGradient, SpreadMethod,
    Stop as KStop, Stroke, StrokeDash, SweepGradient,
};
use krilla::surface::Surface;
use krilla::text::{Font, GlyphId};
use krilla::{Document, SerializeSettings};
use kurbo::Affine;
use scaena_core::displaylist::{
    Blend, Cap, Color, DisplayList, FillRule, FontRef, Join, Op, Paint, Path, PathEl, Quality,
};
use scaena_core::shader::Job;
use scaena_paint::Assets;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

/// Points to a canvas unit: a page is its canvas at 2 units to the point.
pub const POINTS_PER_UNIT: f32 = 0.5;

/// How a document is written.
#[derive(Debug, Clone)]
pub struct PdfSettings {
    /// Device pixels to the canvas unit at which shaders are drawn: 2, twice the canvas.
    pub shader_scale: f32,
    pub title: Option<String>,
    /// BCP 47, the deck's `meta.lang`.
    pub lang: Option<String>,
}

impl Default for PdfSettings {
    fn default() -> Self {
        Self { shader_scale: 2.0, title: None, lang: None }
    }
}

/// A PDF of `pages`, one page per display list, drawing from `assets`.
pub fn pdf(pages: &[DisplayList], assets: &Assets, settings: &PdfSettings) -> Result<Vec<u8>, ExportError> {
    let mut document = Document::new_with(SerializeSettings::default());
    let mut fonts = Fonts::default();
    for dl in pages {
        let [w, h] = dl.viewport.map(|v| v * POINTS_PER_UNIT);
        let page_settings =
            PageSettings::from_wh(w, h).ok_or_else(|| ExportError::Pdf(format!("a {w}×{h} pt page")))?;
        let mut page = document.start_page_with(page_settings);
        let mut surface = page.surface();
        surface.push_transform(&Transform::from_scale(POINTS_PER_UNIT, POINTS_PER_UNIT));
        let scale = settings.shader_scale;
        let jobs = scaena_paint::shader_jobs(dl, scale).map_err(|e| ExportError::Pdf(e.to_string()))?;
        let mut cx = Cx { surface: &mut surface, assets, fonts: &mut fonts, table: &dl.fonts, jobs: jobs.into_iter() };
        cx.ops(&dl.ops, Affine::scale(f64::from(scale)))?;
        surface.pop();
        surface.finish();
        page.finish();
    }
    let mut metadata = Metadata::new().creator("Scaena".to_string());
    if let Some(title) = &settings.title {
        metadata = metadata.title(title.clone());
    }
    if let Some(lang) = &settings.lang {
        metadata = metadata.language(lang.clone());
    }
    document.set_metadata(metadata);
    document.finish().map_err(|e| ExportError::Pdf(format!("{e:?}")))
}

struct Cx<'a, 's> {
    surface: &'a mut Surface<'s>,
    assets: &'a Assets,
    fonts: &'a mut Fonts,
    /// The display list's font table.
    table: &'a [FontRef],
    /// One per shader op, in the order the walk meets them.
    jobs: std::vec::IntoIter<Option<Job>>,
}

impl Cx<'_, '_> {
    /// `ops` in the current coordinates, which `xf` maps to shader pixels.
    fn ops(&mut self, ops: &[Op], xf: Affine) -> Result<(), ExportError> {
        for op in ops {
            match op {
                Op::Layer { transform, opacity, blend, clip, ops, .. } => {
                    self.surface.push_transform(&matrix(transform));
                    let mut pushed = 1;
                    if let Some(clip) = clip.as_ref().and_then(path) {
                        self.surface.push_clip_path(&clip, &KRule::NonZero);
                        pushed += 1;
                    }
                    // The layer composites as one: at its opacity, in its blend mode.
                    if *blend != Blend::Normal {
                        self.surface.push_blend_mode(blend_mode(*blend));
                        pushed += 1;
                    }
                    if *opacity < 1.0 {
                        self.surface.push_opacity(unit(*opacity));
                        pushed += 1;
                    } else if *blend != Blend::Normal {
                        self.surface.push_isolated();
                        pushed += 1;
                    }
                    self.ops(ops, xf * affine(transform))?;
                    for _ in 0..pushed {
                        self.surface.pop();
                    }
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
                Op::Glyphs { font, size, coords, paint, text, glyphs, clusters } => {
                    let Some(first) = glyphs.first() else { continue };
                    let font_ref = self.table.get(*font as usize).ok_or_else(|| {
                        ExportError::Pdf(format!("display list names font index {font}, past its font table"))
                    })?;
                    let font = self.fonts.get(self.assets, font_ref, coords)?;
                    let (paint, opacity) = kpaint(paint);
                    self.surface.set_stroke(None);
                    self.surface.set_fill(Some(Fill { paint, opacity, rule: KRule::NonZero }));
                    let placed = place(glyphs, *size, text, clusters);
                    self.surface.draw_glyphs(Point::from_xy(first.x, first.y), &placed, font, text, *size, false);
                }
                Op::Image { asset, src, dst, quality } => {
                    let picture = self.assets.image(asset).map_err(|e| ExportError::Pdf(e.to_string()))?;
                    let image = Image::from_custom(Pixels::of_picture(asset, picture), *quality == Quality::High)
                        .map_err(ExportError::Pdf)?;
                    let Some(clip) = path(&Path::rect(*dst)) else { continue };
                    self.surface.push_clip_path(&clip, &KRule::NonZero);
                    let [sx, sy, sw, sh] = *src;
                    let [dx, dy, dw, dh] = *dst;
                    let (kx, ky) = (dw / sw, dh / sh);
                    self.surface.push_transform(&Transform::from_row(kx, 0.0, 0.0, ky, dx - sx * kx, dy - sy * ky));
                    let size = Size::from_wh(picture.width as f32, picture.height as f32);
                    if let Some(size) = size {
                        self.surface.draw_image(image, size);
                    }
                    self.surface.pop();
                    self.surface.pop();
                }
                Op::Shader { rect, .. } => {
                    let job = self.jobs.next().expect("shader_jobs makes one job per shader op");
                    if let Some(job) = job {
                        self.shader(&job, *rect, xf)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// The CPU reference's pixels for `job` inside `rect`, texel for shader pixel.
    fn shader(&mut self, job: &Job, rect: [f32; 4], xf: Affine) -> Result<(), ExportError> {
        let [x, y, w, h] = job.bbox();
        let Some(clip) = path(&Path::rect(rect)) else { return Ok(()) };
        let image = Image::from_custom(Pixels::of_rgba(job.render(), w, h), true).map_err(ExportError::Pdf)?;
        self.surface.push_clip_path(&clip, &KRule::NonZero);
        // Texel (0, 0) on shader pixel (x, y): undo the layers' transforms and the scale.
        let place = xf.inverse() * Affine::translate((f64::from(x), f64::from(y)));
        self.surface.push_transform(&from_affine(place));
        if let Some(size) = Size::from_wh(w as f32, h as f32) {
            self.surface.draw_image(image, size);
        }
        self.surface.pop();
        self.surface.pop();
        Ok(())
    }
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

/// The run's glyphs at `size`, placed from the first: each advances to the next, and
/// sits as far off the first's baseline as the display list puts it. Each says its
/// cluster: from its start to the next larger one, or the end of `text`.
fn place(glyphs: &[scaena_core::displaylist::Glyph], size: f32, text: &str, clusters: &[u32]) -> Vec<Placed> {
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
            advance: glyphs.get(i + 1).map_or(0.0, |next| next.x - glyphs[i].x) * em,
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
