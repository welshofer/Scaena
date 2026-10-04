//! SVG (PLAN 1.21; SPEC §10): a frame drawn from its display list as an SVG document.
//!
//! - The canvas is the `viewBox`; `width` and `height` are the pixels the SVG is drawn
//!   for, `scale` to the unit. What SVG cannot say as vectors is drawn at that size.
//! - A layer is a group: its transform, clip, opacity, and blend (CSS `mix-blend-mode`),
//!   each group isolated where a painter isolates the layer. A node's layer names the
//!   node it draws (`data-node`).
//! - Paths fill and stroke as paths, in a color or a linear or radial gradient with the
//!   stops both raster painters draw (`scaena_paint::srgb_stops`: their Oklab blend
//!   spelled out in sRGB).
//! - Glyphs are outlines. Painters never shape text (SPEC §6), and an SVG that named
//!   fonts would be laid out again by whatever opened it. Each glyph is a path, where
//!   the display list puts it, unhinted, as both painters draw it. Over a run lies its
//!   text, transparent and stretched across the run, so the SVG copies and searches as
//!   the deck reads.
//! - Images embed as PNG at their own resolution, clipped to the part an op draws, and
//!   ask to be filtered as the painters filter them: `smooth` (bilinear) or by the
//!   nearest pixel.
//! - What SVG cannot draw is an image of the CPU painter's pixels at the SVG's size:
//!   shaders (their CPU reference, placed pixel for pixel as the painter places them,
//!   SPEC §3.8), color glyphs (COLR and bitmap emoji), and sweep gradients.
//! - The same display list writes the same bytes. Ids are numbered in the order the
//!   walk makes them, after a prefix taken from the list's digest, so the SVGs of two
//!   frames inlined in one page do not take each other's clips and gradients.

use crate::ExportError;
use base64::Engine as _;
use kurbo::{Affine, Rect as KRect};
use scaena_core::displaylist::{
    Blend, Cap, Color, DL_VERSION, DisplayList, FillRule, FontRef, IDENTITY, Join, Op, Paint, Path, PathEl, Quality,
    Stop,
};
use scaena_core::shader::Job;
use scaena_paint::cpu::CpuPainter;
use scaena_paint::{Assets, Painter as _};
use skrifa::MetadataProvider;
use skrifa::instance::{LocationRef, NormalizedCoord, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use std::collections::HashMap;
use std::fmt::Write as _;

/// How an SVG is drawn.
#[derive(Debug, Clone)]
pub struct SvgSettings {
    /// Pixels to the canvas unit: the SVG's `width` and `height`, and the size its images
    /// of shaders, color glyphs, and sweep gradients are drawn at.
    pub scale: f32,
    /// The document's title.
    pub title: Option<String>,
    /// The document's language (BCP 47).
    pub lang: Option<String>,
}

impl Default for SvgSettings {
    fn default() -> Self {
        Self { scale: 1.0, title: None, lang: None }
    }
}

/// `list` as an SVG document, drawing from `assets`.
pub fn svg(list: &DisplayList, assets: &Assets, settings: &SvgSettings) -> Result<String, ExportError> {
    if list.dl != DL_VERSION {
        return Err(ExportError::Svg(format!("display list version {} (this build reads {DL_VERSION})", list.dl)));
    }
    // A digest holds only finite numbers, so this also checks the list.
    let digest = list.digest().map_err(|e| ExportError::Svg(e.to_string()))?;
    let scale = settings.scale;
    let [vw, vh] = list.viewport;
    let (w, h) = ((vw * scale).round(), (vh * scale).round());
    if !scale.is_finite() || !(1.0..=f32::from(u16::MAX)).contains(&w) || !(1.0..=f32::from(u16::MAX)).contains(&h) {
        return Err(ExportError::Svg(format!("a {w} × {h} px drawing")));
    }
    let jobs = scaena_paint::shader_jobs(list, scale).map_err(|e| ExportError::Svg(e.to_string()))?;
    let mut cx = Cx {
        assets,
        table: &list.fonts,
        jobs: jobs.into_iter(),
        size: [w, h],
        prefix: format!("s{}", &digest[..8]),
        next: 0,
        defs: String::new(),
        body: String::new(),
        images: HashMap::new(),
        outlines: HashMap::new(),
    };
    cx.ops(&list.ops, Affine::scale(f64::from(scale)))?;

    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = write!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" version=\"1.1\" \
         width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {vw} {vh}\" style=\"isolation:isolate\""
    );
    if let Some(lang) = &settings.lang {
        let _ = write!(out, " xml:lang=\"{}\"", escape(lang));
    }
    out.push_str(">\n");
    if let Some(title) = &settings.title {
        let _ = writeln!(out, "<title>{}</title>", escape(title));
    }
    if !cx.defs.is_empty() {
        let _ = write!(out, "<defs>\n{}</defs>\n", cx.defs);
    }
    out.push_str(&cx.body);
    out.push_str("</svg>\n");
    Ok(out)
}

struct Cx<'a> {
    assets: &'a Assets,
    /// The display list's font table.
    table: &'a [FontRef],
    /// One per shader op, in the order the walk meets them.
    jobs: std::vec::IntoIter<Option<Job>>,
    /// The drawing, in pixels.
    size: [f32; 2],
    prefix: String,
    next: usize,
    defs: String,
    body: String,
    /// Images written into `defs`, by asset id and whether they are filtered smoothly.
    images: HashMap<(String, bool), String>,
    /// Glyph outlines at a size, about their origin with y down: by font, instance, and
    /// size, then by glyph.
    outlines: HashMap<Face, HashMap<u32, Vec<PathEl>>>,
}

impl Cx<'_> {
    fn id(&mut self) -> String {
        self.next += 1;
        format!("{}-{}", self.prefix, self.next)
    }

    /// `ops` drawn in the current coordinates, which `xf` maps to the SVG's pixels.
    fn ops(&mut self, ops: &[Op], xf: Affine) -> Result<(), ExportError> {
        for op in ops {
            match op {
                Op::Layer { node, transform, opacity, blend, clip, ops: inner, .. } => {
                    let mut g = String::from("<g");
                    if let Some(node) = node {
                        let _ = write!(g, " data-node=\"{}\"", escape(node));
                    }
                    if *transform != IDENTITY {
                        let _ = write!(g, " transform=\"{}\"", matrix(*transform));
                    }
                    if let Some(clip) = clip {
                        let id = self.clip(clip);
                        let _ = write!(g, " clip-path=\"url(#{id})\"");
                    }
                    if *opacity < 1.0 {
                        g.push_str(" opacity=\"");
                        num(&mut g, opacity.max(0.0));
                        g.push('"');
                    }
                    if *blend != Blend::Normal {
                        let _ = write!(g, " style=\"mix-blend-mode:{}\"", css_blend(*blend));
                    }
                    self.body.push_str(&g);
                    self.body.push_str(">\n");
                    self.ops(inner, xf * affine(transform))?;
                    self.body.push_str("</g>\n");
                }
                Op::Fill { path, rule, paint } => {
                    if matches!(paint, Paint::Sweep { .. }) {
                        self.raster(op, xf, bounds(path))?;
                        continue;
                    }
                    let d = path_data(&path.0, 0.0, 0.0);
                    if d.is_empty() {
                        continue;
                    }
                    let paint = self.paint(paint, "fill");
                    let rule = if *rule == FillRule::EvenOdd { " fill-rule=\"evenodd\"" } else { "" };
                    let _ = writeln!(self.body, "<path d=\"{d}\"{paint}{rule}/>");
                }
                Op::Stroke { path, paint, width, cap, join, miter_limit, dash, dash_offset } => {
                    if matches!(paint, Paint::Sweep { .. }) {
                        // Past the path by half the width, and a miter's reach past that.
                        let reach = f64::from(width * miter_limit.max(2.0) * 0.5);
                        self.raster(op, xf, bounds(path).inflate(reach, reach))?;
                        continue;
                    }
                    let d = path_data(&path.0, 0.0, 0.0);
                    if d.is_empty() {
                        continue;
                    }
                    let mut s = format!("<path d=\"{d}\" fill=\"none\"{}", self.paint(paint, "stroke"));
                    s.push_str(" stroke-width=\"");
                    num(&mut s, width.max(0.0));
                    s.push('"');
                    s.push_str(match cap {
                        Cap::Butt => "",
                        Cap::Round => " stroke-linecap=\"round\"",
                        Cap::Square => " stroke-linecap=\"square\"",
                    });
                    s.push_str(match join {
                        Join::Miter => "",
                        Join::Round => " stroke-linejoin=\"round\"",
                        Join::Bevel => " stroke-linejoin=\"bevel\"",
                    });
                    // SVG's default limit is 4, kurbo's 10: always said.
                    if *join == Join::Miter {
                        s.push_str(" stroke-miterlimit=\"");
                        num(&mut s, miter_limit.max(1.0));
                        s.push('"');
                    }
                    if !dash.is_empty() && dash.iter().all(|d| *d >= 0.0) && dash.iter().any(|d| *d > 0.0) {
                        s.push_str(" stroke-dasharray=\"");
                        for (i, d) in dash.iter().enumerate() {
                            if i > 0 {
                                s.push(' ');
                            }
                            num(&mut s, *d);
                        }
                        s.push('"');
                        if *dash_offset != 0.0 {
                            s.push_str(" stroke-dashoffset=\"");
                            num(&mut s, *dash_offset);
                            s.push('"');
                        }
                    }
                    s.push_str("/>\n");
                    self.body.push_str(&s);
                }
                Op::Glyphs { .. } => self.glyphs(op, xf)?,
                Op::Image { asset, src, dst, quality } => self.image(asset, *src, *dst, *quality)?,
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

    /// A clip path for `path`, in the coordinates of the element that names it.
    fn clip(&mut self, path: &Path) -> String {
        let id = self.id();
        let d = path_data(&path.0, 0.0, 0.0);
        let _ =
            writeln!(self.defs, "<clipPath id=\"{id}\" clipPathUnits=\"userSpaceOnUse\"><path d=\"{d}\"/></clipPath>");
        id
    }

    /// The attributes that fill or stroke (`kind`) with `paint`; a gradient goes into
    /// `defs`. Sweeps are drawn as images before they get here.
    fn paint(&mut self, paint: &Paint, kind: &str) -> String {
        match paint {
            Paint::Solid(c) => color(kind, *c),
            Paint::Linear { start, end, stops } => {
                let id = self.id();
                let mut g = format!("<linearGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" x1=\"");
                num(&mut g, start[0]);
                g.push_str("\" y1=\"");
                num(&mut g, start[1]);
                g.push_str("\" x2=\"");
                num(&mut g, end[0]);
                g.push_str("\" y2=\"");
                num(&mut g, end[1]);
                g.push_str("\">\n");
                stop_elements(&mut g, stops);
                g.push_str("</linearGradient>\n");
                self.defs.push_str(&g);
                format!(" {kind}=\"url(#{id})\"")
            }
            Paint::Radial { center, radius, stops } => {
                let id = self.id();
                let mut g = format!("<radialGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" cx=\"");
                num(&mut g, center[0]);
                g.push_str("\" cy=\"");
                num(&mut g, center[1]);
                g.push_str("\" r=\"");
                num(&mut g, radius.max(0.0));
                g.push_str("\">\n");
                stop_elements(&mut g, stops);
                g.push_str("</radialGradient>\n");
                self.defs.push_str(&g);
                format!(" {kind}=\"url(#{id})\"")
            }
            Paint::Sweep { stops, .. } => {
                // Not reached: an op painted with a sweep is drawn as an image. Its
                // first color is the least wrong thing to say.
                stops.first().map_or_else(String::new, |s| color(kind, s.1))
            }
        }
    }

    /// A glyph run: each glyph's outline as a path, and the run's text over them for a
    /// reader. A run with a color glyph (COLR, or a PNG bitmap, which painters draw before
    /// an outline) is drawn as an image.
    fn glyphs(&mut self, op: &Op, xf: Affine) -> Result<(), ExportError> {
        let Op::Glyphs { font, size, coords, paint, text, glyphs, .. } = op else { return Ok(()) };
        if glyphs.is_empty() {
            return Ok(());
        }
        let font_ref = self
            .table
            .get(*font as usize)
            .ok_or_else(|| ExportError::Svg(format!("display list names font index {font}, past its font table")))?;
        let data = self.assets.font_data(font_ref).map_err(|e| ExportError::Svg(e.to_string()))?;
        let face = skrifa::FontRef::from_index(data.data.data(), font_ref.index)
            .map_err(|e| ExportError::Svg(format!("font {} does not read: {e}", font_ref.id)))?;
        let location: Vec<NormalizedCoord> = coords.iter().map(|&c| NormalizedCoord::from_bits(c)).collect();
        let advances = face.glyph_metrics(Size::new(*size), LocationRef::new(&location));
        let colors = face.color_glyphs();
        let bitmaps = face.bitmap_strikes();
        let colored = glyphs.iter().any(|g| {
            let id = skrifa::GlyphId::new(g.id);
            colors.get(id).is_some()
                || bitmaps
                    .glyph_for_size(Size::new(*size), id)
                    .is_some_and(|b| matches!(b.data, skrifa::bitmap::BitmapData::Png(_)))
        });
        if colored || matches!(paint, Paint::Sweep { .. }) {
            // Each glyph's em, and room around it for what a color glyph draws past it.
            let s = f64::from(*size);
            let mut area = KRect::new(f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
            for g in glyphs {
                let (x, y) = (f64::from(g.x), f64::from(g.y));
                area = area.union(KRect::new(x - s, y - 1.5 * s, x + 2.0 * s, y + s));
            }
            self.raster(op, xf, area)?;
        } else {
            let key = (*font, coords.clone(), size.to_bits());
            let outlines = self.outlines.entry(key).or_default();
            let collection = face.outline_glyphs();
            // A path per glyph, as the painters fill them: in one path, two glyphs that
            // overlap with opposite windings (a base and its mark) would cancel out.
            let mut paths = Vec::with_capacity(glyphs.len());
            for g in glyphs {
                let els = outlines.entry(g.id).or_insert_with(|| {
                    let mut pen = Pen(Vec::new());
                    if let Some(glyph) = collection.get(skrifa::GlyphId::new(g.id)) {
                        let settings = DrawSettings::unhinted(Size::new(*size), LocationRef::new(&location));
                        if glyph.draw(settings, &mut pen).is_err() {
                            pen.0.clear();
                        }
                    }
                    pen.0
                });
                let d = path_data(els, g.x, g.y);
                if !d.is_empty() {
                    paths.push(d);
                }
            }
            match paths.as_slice() {
                [] => {}
                [d] => {
                    let paint = self.paint(paint, "fill");
                    let _ = writeln!(self.body, "<path d=\"{d}\"{paint}/>");
                }
                paths => {
                    let paint = self.paint(paint, "fill");
                    let _ = write!(self.body, "<g{paint}>");
                    for d in paths {
                        let _ = write!(self.body, "<path d=\"{d}\"/>");
                    }
                    self.body.push_str("</g>\n");
                }
            }
        }
        // The text, where a reader finds it: from the run's left to its right, the last
        // glyph's advance past its origin.
        if !text.is_empty() && *size > 0.0 {
            let left = glyphs.iter().map(|g| g.x).fold(f32::INFINITY, f32::min);
            let right = glyphs
                .iter()
                .map(|g| g.x + advances.advance_width(skrifa::GlyphId::new(g.id)).unwrap_or(0.0))
                .fold(f32::NEG_INFINITY, f32::max);
            let mut t = String::from("<text x=\"");
            num(&mut t, left);
            t.push_str("\" y=\"");
            num(&mut t, glyphs[0].y);
            t.push_str("\" font-size=\"");
            num(&mut t, *size);
            if right > left {
                t.push_str("\" textLength=\"");
                num(&mut t, right - left);
                t.push_str("\" lengthAdjust=\"spacingAndGlyphs");
            }
            let _ = writeln!(t, "\" fill-opacity=\"0\" xml:space=\"preserve\">{}</text>", escape(text));
            self.body.push_str(&t);
        }
        Ok(())
    }

    /// The `src` part of image `asset` drawn into `dst`, clipped to it. Each image is
    /// written once into `defs` for each way it is filtered, and used where it is drawn.
    fn image(&mut self, asset: &str, src: [f32; 4], dst: [f32; 4], quality: Quality) -> Result<(), ExportError> {
        let [sx, sy, sw, sh] = src;
        let [dx, dy, dw, dh] = dst;
        if sw <= 0.0 || sh <= 0.0 || dw <= 0.0 || dh <= 0.0 {
            return Ok(());
        }
        let key = (asset.to_string(), quality == Quality::High);
        let id = match self.images.get(&key) {
            Some(id) => id.clone(),
            None => {
                let picture = self.assets.image(asset).map_err(|e| ExportError::Svg(e.to_string()))?;
                let png = png(picture.width, picture.height, picture.rgba.data())?;
                let id = self.id();
                // Painters sample `high` bilinearly, CSS's `smooth`, which only a style may
                // say (a viewer that does not know it uses its own smooth filter), and
                // `low` by the nearest pixel. The property is the image's: a `use` does not
                // pass it on.
                let rendering = match quality {
                    Quality::High => "style=\"image-rendering:smooth\"",
                    Quality::Low => "image-rendering=\"optimizeSpeed\"",
                };
                let _ = writeln!(
                    self.defs,
                    "<image id=\"{id}\" width=\"{}\" height=\"{}\" {rendering} \
                     xlink:href=\"data:image/png;base64,{}\"/>",
                    picture.width,
                    picture.height,
                    base64::engine::general_purpose::STANDARD.encode(png)
                );
                self.images.insert(key, id.clone());
                id
            }
        };
        let (kx, ky) = (dw / sw, dh / sh);
        let place = matrix([kx, 0.0, 0.0, ky, dx - sx * kx, dy - sy * ky]);
        let clip = self.clip(&Path::rect(dst));
        let _ =
            writeln!(self.body, "<g clip-path=\"url(#{clip})\"><use xlink:href=\"#{id}\" transform=\"{place}\"/></g>");
        Ok(())
    }

    /// The CPU reference's pixels for `job` inside `rect`, texel for pixel.
    fn shader(&mut self, job: &Job, rect: [f32; 4], xf: Affine) -> Result<(), ExportError> {
        let [x, y, w, h] = job.bbox();
        let Some(place) = placed(xf, f64::from(x), f64::from(y)) else { return Ok(()) };
        let png = png(w, h, &job.render_on(scaena_core::shader::cores()))?;
        let clip = self.clip(&Path::rect(rect));
        let _ = writeln!(
            self.body,
            "<g clip-path=\"url(#{clip})\"><image width=\"{w}\" height=\"{h}\" transform=\"{place}\" \
             image-rendering=\"optimizeSpeed\" xlink:href=\"data:image/png;base64,{}\"/></g>",
            base64::engine::general_purpose::STANDARD.encode(png)
        );
        Ok(())
    }

    /// `op` drawn by the CPU painter, pixel for pixel, as an image: within `area` (in the
    /// op's coordinates, enough to hold all it draws) and the drawing, cropped to the
    /// pixels it covers.
    fn raster(&mut self, op: &Op, xf: Affine, area: KRect) -> Result<(), ExportError> {
        let device = xf.transform_rect_bbox(area).intersect(KRect::new(
            0.0,
            0.0,
            f64::from(self.size[0]),
            f64::from(self.size[1]),
        ));
        let (x0, y0) = (device.x0.floor(), device.y0.floor());
        let (x1, y1) = (device.x1.ceil(), device.y1.ceil());
        if !(x1 > x0 && y1 > y0) {
            return Ok(());
        }
        let mut list = DisplayList::new([(x1 - x0) as f32, (y1 - y0) as f32]);
        list.fonts = self.table.to_vec();
        let local = (Affine::translate((-x0, -y0)) * xf).as_coeffs().map(|v| v as f32);
        list.ops.push(Op::Layer {
            node: None,
            cell: None,
            transform: local,
            opacity: 1.0,
            blend: Blend::Normal,
            clip: None,
            ops: vec![op.clone()],
        });
        let raster =
            CpuPainter::default().paint(&list, self.assets, 1.0).map_err(|e| ExportError::Svg(e.to_string()))?;
        let Some([cx, cy, cw, ch]) = covered(&raster) else { return Ok(()) };
        let Some(place) = placed(xf, x0 + f64::from(cx), y0 + f64::from(cy)) else { return Ok(()) };
        let mut pixels = Vec::with_capacity((cw * ch * 4) as usize);
        for row in cy..cy + ch {
            let start = ((row * raster.width + cx) * 4) as usize;
            pixels.extend_from_slice(&raster.rgba[start..start + (cw * 4) as usize]);
        }
        let png = png(cw, ch, &pixels)?;
        let _ = writeln!(
            self.body,
            "<image width=\"{cw}\" height=\"{ch}\" transform=\"{place}\" image-rendering=\"optimizeSpeed\" \
             xlink:href=\"data:image/png;base64,{}\"/>",
            base64::engine::general_purpose::STANDARD.encode(png)
        );
        Ok(())
    }
}

/// The transform that puts an image's pixel (0, 0) on the SVG's pixel (`x`, `y`) from
/// coordinates that `xf` maps to its pixels; none where `xf` flattens them.
fn placed(xf: Affine, x: f64, y: f64) -> Option<String> {
    if xf.determinant().abs() < 1e-12 {
        return None;
    }
    let m = (xf.inverse() * Affine::translate((x, y))).as_coeffs().map(|v| v as f32);
    Some(matrix(m))
}

/// The pixels a raster covers: `[x, y, width, height]`, or none.
fn covered(r: &scaena_paint::Raster) -> Option<[u32; 4]> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..r.height {
        for x in 0..r.width {
            if r.rgba[((y * r.width + x) * 4 + 3) as usize] != 0 {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    (x1 > x0).then(|| [x0, y0, x1 - x0, y1 - y0])
}

/// The box around `path`'s points, control points included.
fn bounds(path: &Path) -> KRect {
    let mut r = KRect::new(f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut add = |[x, y]: [f32; 2]| r = r.union_pt((f64::from(x), f64::from(y)));
    for el in &path.0 {
        match *el {
            PathEl::MoveTo(a) | PathEl::LineTo(a) => add(a),
            PathEl::QuadTo(a, b) => {
                add(a);
                add(b);
            }
            PathEl::CurveTo(a, b, c) => {
                add(a);
                add(b);
                add(c);
            }
            PathEl::Close => {}
        }
    }
    r
}

/// A font at an instance and a size: its index in the display list's font table, its
/// normalized coordinates, and the bits of its size.
type Face = (u32, Vec<i16>, u32);

/// Collects a glyph's outline with y down, about its origin.
struct Pen(Vec<PathEl>);

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push(PathEl::MoveTo([x, -y]));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push(PathEl::LineTo([x, -y]));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.push(PathEl::QuadTo([cx0, -cy0], [x, -y]));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.push(PathEl::CurveTo([cx0, -cy0], [cx1, -cy1], [x, -y]));
    }
    fn close(&mut self) {
        self.0.push(PathEl::Close);
    }
}

/// SVG path data for `els`, moved by (`dx`, `dy`), to a thousandth of a unit.
fn path_data(els: &[PathEl], dx: f32, dy: f32) -> String {
    let mut d = String::new();
    let point = |d: &mut String, [x, y]: [f32; 2], first: bool| {
        if !first {
            d.push(' ');
        }
        num(d, x + dx);
        d.push(' ');
        num(d, y + dy);
    };
    for el in els {
        match *el {
            PathEl::MoveTo(a) => {
                d.push('M');
                point(&mut d, a, true);
            }
            PathEl::LineTo(a) => {
                d.push('L');
                point(&mut d, a, true);
            }
            PathEl::QuadTo(a, b) => {
                d.push('Q');
                point(&mut d, a, true);
                point(&mut d, b, false);
            }
            PathEl::CurveTo(a, b, c) => {
                d.push('C');
                point(&mut d, a, true);
                point(&mut d, b, false);
                point(&mut d, c, false);
            }
            PathEl::Close => d.push('Z'),
        }
    }
    d
}

/// A gradient's stops as both raster painters draw them.
fn stop_elements(out: &mut String, stops: &[Stop]) {
    for Stop(offset, c) in scaena_paint::srgb_stops(stops) {
        out.push_str("<stop offset=\"");
        num(out, offset.clamp(0.0, 1.0));
        let [r, g, b, a] = c.0;
        let _ = write!(out, "\" stop-color=\"#{r:02x}{g:02x}{b:02x}\"");
        if a < 255 {
            out.push_str(" stop-opacity=\"");
            num(out, f32::from(a) / 255.0);
            out.push('"');
        }
        out.push_str("/>\n");
    }
}

/// `fill` or `stroke` in color `c`, with its opacity where it has one.
fn color(kind: &str, c: Color) -> String {
    let [r, g, b, a] = c.0;
    let mut s = format!(" {kind}=\"#{r:02x}{g:02x}{b:02x}\"");
    if a < 255 {
        let _ = write!(s, " {kind}-opacity=\"");
        num(&mut s, f32::from(a) / 255.0);
        s.push('"');
    }
    s
}

fn css_blend(blend: Blend) -> &'static str {
    match blend {
        Blend::Normal => "normal",
        Blend::Multiply => "multiply",
        Blend::Screen => "screen",
        Blend::Overlay => "overlay",
        Blend::Darken => "darken",
        Blend::Lighten => "lighten",
        Blend::Difference => "difference",
    }
}

/// `matrix(a b c d e f)`, each number as it is: a rotation's sine rounded to a
/// thousandth would swing a long line by units.
fn matrix(m: [f32; 6]) -> String {
    let [a, b, c, d, e, f] = m.map(|v| if v.is_finite() && v != 0.0 { v } else { 0.0 });
    format!("matrix({a} {b} {c} {d} {e} {f})")
}

fn affine(m: &[f32; 6]) -> Affine {
    Affine::new(m.map(f64::from))
}

/// `v` to a thousandth, without trailing zeros: lengths in canvas units, opacities, and
/// offsets, where a thousandth is below what any painter resolves.
fn num(out: &mut String, v: f32) {
    let v = if v.is_finite() { v } else { 0.0 };
    let start = out.len();
    let _ = write!(out, "{v:.3}");
    while out.ends_with('0') {
        out.pop();
    }
    if out.ends_with('.') {
        out.pop();
    }
    if &out[start..] == "-0" {
        out.truncate(start);
        out.push('0');
    }
}

/// `s` as XML character data or an attribute value: markup escaped, and the characters
/// XML 1.0 cannot hold left out.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if u32::from(c) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

/// Straight-alpha RGBA8 pixels as a PNG as small as it stays lossless: indexed when they
/// hold 256 colors or fewer, else RGB when opaque, else RGBA. The same pixels always
/// encode to the same bytes.
pub(crate) fn png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, ExportError> {
    let pixels = rgba.as_chunks::<4>().0;
    let opaque = pixels.iter().all(|p| p[3] == 255);
    let mut palette: Vec<[u8; 4]> = Vec::new();
    let mut index: HashMap<[u8; 4], u8> = HashMap::new();
    let mut indices = Vec::with_capacity(pixels.len());
    for p in pixels {
        let i = match index.get(p) {
            Some(&i) => i,
            None if palette.len() < 256 => {
                let i = palette.len() as u8;
                palette.push(*p);
                index.insert(*p, i);
                i
            }
            None => {
                palette.clear();
                break;
            }
        };
        indices.push(i);
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Balanced);
    let data: std::borrow::Cow<[u8]> = if !palette.is_empty() && indices.len() == pixels.len() {
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_palette(palette.iter().flat_map(|&[r, g, b, _]| [r, g, b]).collect::<Vec<u8>>());
        if !opaque {
            let mut alpha: Vec<u8> = palette.iter().map(|p| p[3]).collect();
            while alpha.last() == Some(&255) {
                alpha.pop();
            }
            encoder.set_trns(alpha);
        }
        indices.into()
    } else if opaque {
        encoder.set_color(png::ColorType::Rgb);
        pixels.iter().flat_map(|&[r, g, b, _]| [r, g, b]).collect::<Vec<u8>>().into()
    } else {
        encoder.set_color(png::ColorType::Rgba);
        rgba.into()
    };
    let bad = |e: png::EncodingError| ExportError::Svg(format!("png: {e}"));
    let mut writer = encoder.write_header().map_err(bad)?;
    writer.write_image_data(&data).map_err(bad)?;
    writer.finish().map_err(bad)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
        let p = scaena_paint::Picture::decode(bytes).unwrap();
        (p.width, p.height, p.rgba.data().to_vec())
    }

    #[test]
    fn png_keeps_every_pixel_in_the_smallest_form() {
        // Two colors, one translucent: indexed, with the alpha in tRNS.
        let few = [[255, 0, 0, 255], [0, 0, 255, 128]].repeat(8).concat();
        let bytes = png(4, 4, &few).unwrap();
        assert_eq!(decode(&bytes), (4, 4, few.clone()));
        assert_eq!(bytes[25], 3, "indexed");
        // 300 opaque colors: RGB.
        let many: Vec<u8> = (0..300u32).flat_map(|i| [(i % 256) as u8, (i / 256) as u8, 7, 255]).collect();
        let bytes = png(300, 1, &many).unwrap();
        assert_eq!(decode(&bytes), (300, 1, many.clone()));
        assert_eq!(bytes[25], 2, "rgb");
        // And translucent: RGBA.
        let clear: Vec<u8> = many.as_chunks::<4>().0.iter().flat_map(|&[r, g, b, _]| [r, g, b, 9]).collect();
        let bytes = png(300, 1, &clear).unwrap();
        assert_eq!(decode(&bytes), (300, 1, clear.clone()));
        assert_eq!(bytes[25], 6, "rgba");
        assert_eq!(png(300, 1, &clear).unwrap(), bytes, "same pixels, same bytes");
    }

    #[test]
    fn numbers_print_to_a_thousandth_and_text_escapes() {
        let mut s = String::new();
        for v in [1.0, 0.5, -0.0001, 12.34567, f32::NAN, -3.25] {
            num(&mut s, v);
            s.push(' ');
        }
        assert_eq!(s, "1 0.5 0 12.346 0 -3.25 ");
        assert_eq!(escape("a<b & \"c\"\u{1}\n"), "a&lt;b &amp; &quot;c&quot;\n");
        assert_eq!(matrix([1.0, 0.5, -0.5, 1.0, 10.0, -0.0]), "matrix(1 0.5 -0.5 1 10 0)");
    }

    #[test]
    fn layers_paths_and_gradients_say_what_they_draw() {
        let mut list = DisplayList::new([100.0, 50.0]);
        list.ops.push(Op::Layer {
            node: Some("card".into()),
            cell: None,
            transform: [1.0, 0.0, 0.0, 1.0, 10.0, 5.0],
            opacity: 0.5,
            blend: Blend::Multiply,
            clip: Some(Path::rect([0.0, 0.0, 40.0, 20.0])),
            ops: vec![
                Op::Fill {
                    path: Path::rect([0.0, 0.0, 40.0, 20.0]),
                    rule: FillRule::EvenOdd,
                    paint: Paint::Linear {
                        start: [0.0, 0.0],
                        end: [40.0, 0.0],
                        stops: vec![Stop(0.0, Color([255, 0, 0, 255])), Stop(1.0, Color([0, 0, 255, 255]))],
                    },
                },
                Op::Stroke {
                    path: Path::rect([0.0, 0.0, 40.0, 20.0]),
                    paint: Paint::Solid(Color([0, 0, 0, 128])),
                    width: 2.0,
                    cap: Cap::Round,
                    join: Join::Miter,
                    miter_limit: 10.0,
                    dash: vec![4.0, 2.0],
                    dash_offset: 1.0,
                },
            ],
        });
        let s =
            svg(&list, &Assets::new(), &SvgSettings { scale: 2.0, title: Some("A & B".into()), lang: None }).unwrap();
        assert!(s.contains("width=\"200\" height=\"100\" viewBox=\"0 0 100 50\""), "{s}");
        assert!(s.contains("<title>A &amp; B</title>"));
        assert!(s.contains("<g data-node=\"card\" transform=\"matrix(1 0 0 1 10 5)\" clip-path=\"url(#"));
        assert!(s.contains("opacity=\"0.5\" style=\"mix-blend-mode:multiply\">"));
        assert!(
            s.contains("<linearGradient") && s.contains("gradientUnits=\"userSpaceOnUse\" x1=\"0\" y1=\"0\" x2=\"40\"")
        );
        // Oklab's blend, spelled out in sRGB stops between the two.
        assert!(s.matches("<stop ").count() > 2);
        assert!(s.contains("fill-rule=\"evenodd\""));
        assert!(s.contains(
            "fill=\"none\" stroke=\"#000000\" stroke-opacity=\"0.502\" stroke-width=\"2\" stroke-linecap=\"round\" \
             stroke-miterlimit=\"10\" stroke-dasharray=\"4 2\" stroke-dashoffset=\"1\"/>"
        ));
        assert_eq!(
            s,
            svg(&list, &Assets::new(), &SvgSettings { scale: 2.0, title: Some("A & B".into()), lang: None }).unwrap()
        );
    }
}
