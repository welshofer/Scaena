//! The display list (SPEC §6): a serializable, painter-agnostic frame description.
//!
//! Coordinates are canvas units (`f32`, SPEC §13.4). Glyph positions are final
//! (post-shaping, post-kerning): painters never shape text. Ops are stateless and
//! in paint order; an [`Op::Layer`] scopes transform, clip, opacity, and blend for
//! its children. Fonts are referenced through the list's font table and variable
//! instances by normalized coordinates, exactly as `vello` and `vello_cpu` take
//! them. Shader ops carry parameters, not pixels.
//!
//! One type, two encodings: JSON for goldens and tooling ([`DisplayList::to_json`],
//! [`DisplayList::to_golden_json`]) and postcard for runtime
//! ([`DisplayList::to_postcard`]). Leaf types that read better as text (paths as
//! SVG data, colors as `#RRGGBBAA`) switch on `is_human_readable`, so the binary
//! form stays compact. Nothing here holds a map with unstable iteration order, so
//! encoding is a pure function of the value.

use crate::tracking::Snapshot;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io;
use thiserror::Error;

pub const DL_VERSION: u32 = 1;

/// A point `[x, y]` in canvas units.
pub type Point = [f32; 2];
/// A rectangle `[x, y, w, h]` in canvas units.
pub type Rect = [f32; 4];
/// A 2D affine matrix `[a, b, c, d, e, f]` (column-major, CSS/SVG convention).
pub type Affine = [f32; 6];
pub const IDENTITY: Affine = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

#[derive(Debug, Error)]
pub enum DlError {
    #[error("display list json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("display list postcard: {0}")]
    Postcard(#[from] postcard::Error),
    #[error("unsupported display list version {0} (this build reads {DL_VERSION})")]
    Version(u32),
    #[error("non-finite number in a display list (a {0:?} value); display lists hold finite values only")]
    NonFinite(Num),
    #[error("bad color `{0}`: expected #RRGGBB or #RRGGBBAA")]
    Color(String),
    #[error("bad path data: {0}")]
    Path(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayList {
    pub dl: u32,
    pub viewport: [f32; 2],
    /// Fonts used by `glyphs` ops, in first-use order (see [`DisplayList::font`]).
    pub fonts: Vec<FontRef>,
    /// Paint order: later ops draw over earlier ones.
    pub ops: Vec<Op>,
}

/// A font in the bundle. Painters load each one once and draw every run that names its index.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontRef {
    /// Bundle font id: its path inside the bundle until PLAN 1.4 makes fonts content-addressed.
    pub id: String,
    /// Face index inside a font collection; 0 for a single font.
    pub index: u32,
}

/// The longest side an [`Op::Image`] asset may have, in pixels: vello's image atlas is
/// 8192 px square, and an image that does not fit it is not drawn on the GPU at all.
pub const MAX_IMAGE_SIDE: u32 = 8192;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", rename_all_fields = "camelCase", deny_unknown_fields)]
pub enum Op {
    /// Children drawn with `transform` applied, clipped to `clip`, and composited at
    /// `opacity` with `blend`. A painter isolates the layer only when it has to
    /// (opacity below 1, a blend other than normal, or a clip).
    Layer {
        /// The scene node this layer draws, if any.
        node: Option<String>,
        transform: Affine,
        opacity: f32,
        blend: Blend,
        clip: Option<Path>,
        ops: Vec<Op>,
    },
    Fill {
        path: Path,
        rule: FillRule,
        paint: Paint,
    },
    Stroke {
        path: Path,
        paint: Paint,
        width: f32,
        cap: Cap,
        join: Join,
        miter_limit: f32,
        /// Dash lengths; empty means solid.
        dash: Vec<f32>,
        dash_offset: f32,
    },
    Glyphs {
        /// Index into [`DisplayList::fonts`].
        font: u32,
        size: f32,
        /// Normalized variation coordinates (F2Dot14) in the font's `fvar` axis order;
        /// empty for the default instance.
        coords: Vec<i16>,
        paint: Paint,
        glyphs: Vec<Glyph>,
    },
    Image {
        /// Content-addressed asset id, `sha256:<hex>`, of an image at most
        /// [`MAX_IMAGE_SIDE`] px a side.
        asset: String,
        /// The part of the image drawn, in its pixels.
        src: Rect,
        /// Where `src` lands, in canvas units.
        dst: Rect,
        quality: Quality,
    },
    Shader {
        kind: ShaderKind,
        seed: u64,
        /// Seconds on the global timeline (SPEC §3.8).
        t: f32,
        rect: Rect,
        palette: Vec<Color>,
        /// Typed per kind by `scaena_core::shader` (`mesh::Params`); a sorted map keeps
        /// the encoding stable.
        params: BTreeMap<String, f32>,
    },
}

/// One positioned glyph: id plus position (canvas units, in its layer's coordinate space;
/// y is the baseline). Encoded as `[id, x, y]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(from = "(u32, f32, f32)", into = "(u32, f32, f32)")]
pub struct Glyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
}

impl From<(u32, f32, f32)> for Glyph {
    fn from((id, x, y): (u32, f32, f32)) -> Self {
        Glyph { id, x, y }
    }
}

impl From<Glyph> for (u32, f32, f32) {
    fn from(g: Glyph) -> Self {
        (g.id, g.x, g.y)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", rename_all_fields = "camelCase", deny_unknown_fields)]
pub enum Paint {
    Solid(Color),
    Linear { start: Point, end: Point, stops: Vec<Stop> },
    Radial { center: Point, radius: f32, stops: Vec<Stop> },
    Sweep { center: Point, start_angle: f32, end_angle: f32, stops: Vec<Stop> },
}

/// A gradient stop `[offset, color]`, offset in `[0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stop(pub f32, pub Color);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Blend {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Difference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Cap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Join {
    #[default]
    Miter,
    Round,
    Bevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    Low,
    #[default]
    High,
}

/// SPEC §3.8 kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShaderKind {
    Mesh,
    Gradient,
    Noise,
    Grain,
    Particles,
}

// --- colors -------------------------------------------------------------------

/// sRGB with straight (unpremultiplied) alpha. JSON: `"#RRGGBBAA"`; binary: four bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color(pub [u8; 4]);

impl Color {
    pub fn to_hex(self) -> String {
        let [r, g, b, a] = self.0;
        format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
    }

    /// Parse `#RRGGBB` (opaque) or `#RRGGBBAA`, either case.
    pub fn from_hex(s: &str) -> Result<Color, DlError> {
        let bad = || DlError::Color(s.to_string());
        let hex = s.strip_prefix('#').filter(|h| h.is_ascii() && matches!(h.len(), 6 | 8)).ok_or_else(bad)?;
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
        let a = if hex.len() == 8 { byte(6)? } else { 0xFF };
        Ok(Color([byte(0)?, byte(2)?, byte(4)?, a]))
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() { s.serialize_str(&self.to_hex()) } else { self.0.serialize(s) }
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Color, D::Error> {
        if d.is_human_readable() {
            Color::from_hex(&String::deserialize(d)?).map_err(D::Error::custom)
        } else {
            Ok(Color(<[u8; 4]>::deserialize(d)?))
        }
    }
}

// --- paths --------------------------------------------------------------------

/// Vector path in canvas units. JSON: absolute SVG path data (`M L Q C Z`);
/// binary: the element list.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Path(pub Vec<PathEl>);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PathEl {
    MoveTo(Point),
    LineTo(Point),
    QuadTo(Point, Point),
    CurveTo(Point, Point, Point),
    Close,
}

impl PathEl {
    fn command(&self) -> char {
        match self {
            PathEl::MoveTo(_) => 'M',
            PathEl::LineTo(_) => 'L',
            PathEl::QuadTo(..) => 'Q',
            PathEl::CurveTo(..) => 'C',
            PathEl::Close => 'Z',
        }
    }

    fn points(&self) -> Vec<Point> {
        match *self {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => vec![p],
            PathEl::QuadTo(a, b) => vec![a, b],
            PathEl::CurveTo(a, b, c) => vec![a, b, c],
            PathEl::Close => vec![],
        }
    }

    fn for_each_point_mut(&mut self, mut f: impl FnMut(&mut Point)) {
        match self {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => f(p),
            PathEl::QuadTo(a, b) => {
                f(a);
                f(b);
            }
            PathEl::CurveTo(a, b, c) => {
                f(a);
                f(b);
                f(c);
            }
            PathEl::Close => {}
        }
    }
}

impl Path {
    /// An axis-aligned rectangle, clockwise from the top-left corner.
    pub fn rect([x, y, w, h]: Rect) -> Path {
        Path(vec![
            PathEl::MoveTo([x, y]),
            PathEl::LineTo([x + w, y]),
            PathEl::LineTo([x + w, y + h]),
            PathEl::LineTo([x, y + h]),
            PathEl::Close,
        ])
    }

    /// SVG path data. Numbers use Rust's shortest round-trip formatting, which is
    /// platform-independent, so equal paths always print equal strings.
    pub fn to_svg(&self) -> String {
        let mut out = String::new();
        for el in &self.0 {
            out.push(el.command());
            for (i, [x, y]) in el.points().iter().enumerate() {
                let sep = if i == 0 { "" } else { " " };
                let _ = write!(out, "{sep}{x} {y}");
            }
        }
        out
    }

    /// Parse the absolute `M L Q C Z` subset that [`Path::to_svg`] writes;
    /// spaces and commas both separate numbers.
    pub fn from_svg(s: &str) -> Result<Path, DlError> {
        let mut p = SvgCursor { src: s, rest: s };
        let mut els = Vec::new();
        while let Some(cmd) = p.command()? {
            els.push(match cmd {
                'M' => PathEl::MoveTo(p.point()?),
                'L' => PathEl::LineTo(p.point()?),
                'Q' => PathEl::QuadTo(p.point()?, p.point()?),
                'C' => PathEl::CurveTo(p.point()?, p.point()?, p.point()?),
                'Z' => PathEl::Close,
                other => return Err(p.error(&format!("unsupported command `{other}`"))),
            });
        }
        Ok(Path(els))
    }
}

struct SvgCursor<'a> {
    src: &'a str,
    rest: &'a str,
}

impl SvgCursor<'_> {
    fn skip_separators(&mut self) {
        self.rest = self.rest.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
    }

    fn command(&mut self) -> Result<Option<char>, DlError> {
        self.skip_separators();
        let mut chars = self.rest.chars();
        match chars.next() {
            None => Ok(None),
            Some(c) if c.is_ascii_alphabetic() => {
                self.rest = chars.as_str();
                Ok(Some(c))
            }
            Some(_) => Err(self.error("expected a command")),
        }
    }

    fn number(&mut self) -> Result<f32, DlError> {
        self.skip_separators();
        let end = self
            .rest
            .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')))
            .unwrap_or(self.rest.len());
        let (token, rest) = self.rest.split_at(end);
        match token.parse::<f32>() {
            Ok(v) if v.is_finite() => {
                self.rest = rest;
                Ok(v)
            }
            _ if token.is_empty() => Err(self.error("expected a number")),
            _ => Err(self.error(&format!("bad number `{token}`"))),
        }
    }

    fn point(&mut self) -> Result<Point, DlError> {
        Ok([self.number()?, self.number()?])
    }

    fn error(&self, why: &str) -> DlError {
        DlError::Path(format!("{why} at byte {} of `{}`", self.src.len() - self.rest.len(), self.src))
    }
}

impl Serialize for Path {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() { s.serialize_str(&self.to_svg()) } else { self.0.serialize(s) }
    }
}

impl<'de> Deserialize<'de> for Path {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Path, D::Error> {
        if d.is_human_readable() {
            Path::from_svg(&String::deserialize(d)?).map_err(D::Error::custom)
        } else {
            Ok(Path(Vec::deserialize(d)?))
        }
    }
}

// --- the list -----------------------------------------------------------------

/// What a number in a display list measures; decides how [`quantize`] rounds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Num {
    /// Canvas units: positions, sizes, widths, radii, translations.
    Length,
    /// The linear part (a, b, c, d) of an affine transform.
    Linear,
    /// Opacity, gradient offsets and angles, miter limits, shader time and parameters.
    Other,
}

impl DisplayList {
    pub fn new(viewport: [f32; 2]) -> Self {
        Self { dl: DL_VERSION, viewport, fonts: Vec::new(), ops: Vec::new() }
    }

    /// Index of `font` in the font table, appending it on first use, so the table
    /// order follows paint order.
    pub fn font(&mut self, font: FontRef) -> u32 {
        let i = self.fonts.iter().position(|f| *f == font).unwrap_or_else(|| {
            self.fonts.push(font);
            self.fonts.len() - 1
        });
        u32::try_from(i).expect("fewer than 2^32 fonts")
    }

    /// Compact JSON.
    pub fn to_json(&self) -> Result<String, DlError> {
        self.check_finite()?;
        Ok(serde_json::to_string(self)?)
    }

    /// JSON laid out for golden files: compact, except that the top-level object and
    /// every `fonts`, `ops`, and `glyphs` array put one element per line, so a golden
    /// diff reads as "this op changed" or "this glyph moved".
    pub fn to_golden_json(&self) -> Result<String, DlError> {
        self.check_finite()?;
        let mut out = Vec::new();
        self.serialize(&mut serde_json::Serializer::with_formatter(&mut out, GoldenFormatter::default()))?;
        out.push(b'\n');
        Ok(String::from_utf8(out).expect("serde_json writes UTF-8"))
    }

    pub fn from_json(s: &str) -> Result<DisplayList, DlError> {
        let dl: DisplayList = serde_json::from_str(s)?;
        dl.check_version()?;
        Ok(dl)
    }

    /// Compact binary for runtime (WASM worker, FFI).
    pub fn to_postcard(&self) -> Result<Vec<u8>, DlError> {
        self.check_finite()?;
        Ok(postcard::to_allocvec(self)?)
    }

    pub fn from_postcard(bytes: &[u8]) -> Result<DisplayList, DlError> {
        let dl: DisplayList = postcard::from_bytes(bytes)?;
        dl.check_version()?;
        dl.check_finite()?;
        Ok(dl)
    }

    fn check_version(&self) -> Result<(), DlError> {
        if self.dl == DL_VERSION { Ok(()) } else { Err(DlError::Version(self.dl)) }
    }

    /// Display lists hold finite numbers only. serde_json would write NaN as `null`
    /// and postcard would carry it silently, so encoders refuse instead.
    pub fn check_finite(&self) -> Result<(), DlError> {
        let mut bad = None;
        self.clone().visit_mut(&mut |kind, v| {
            if bad.is_none() && !v.is_finite() {
                bad = Some(kind);
            }
        });
        bad.map_or(Ok(()), |kind| Err(DlError::NonFinite(kind)))
    }

    /// Visit every `f32` with what it measures. The one walk that both [`quantize`]
    /// and [`DisplayList::check_finite`] use, so they cannot disagree about fields.
    pub fn visit_mut(&mut self, f: &mut dyn FnMut(Num, &mut f32)) {
        self.viewport.iter_mut().for_each(|v| f(Num::Length, v));
        visit_ops(&mut self.ops, f);
    }
}

fn lengths(f: &mut dyn FnMut(Num, &mut f32), vs: &mut [f32]) {
    vs.iter_mut().for_each(|v| f(Num::Length, v));
}

fn visit_ops(ops: &mut [Op], f: &mut dyn FnMut(Num, &mut f32)) {
    for op in ops {
        match op {
            Op::Layer { transform, opacity, clip, ops, .. } => {
                transform[..4].iter_mut().for_each(|v| f(Num::Linear, v));
                lengths(f, &mut transform[4..]);
                f(Num::Other, opacity);
                if let Some(clip) = clip {
                    visit_path(clip, f);
                }
                visit_ops(ops, f);
            }
            Op::Fill { path, paint, .. } => {
                visit_path(path, f);
                visit_paint(paint, f);
            }
            Op::Stroke { path, paint, width, miter_limit, dash, dash_offset, .. } => {
                visit_path(path, f);
                visit_paint(paint, f);
                f(Num::Length, width);
                f(Num::Other, miter_limit);
                lengths(f, dash);
                f(Num::Length, dash_offset);
            }
            Op::Glyphs { size, paint, glyphs, .. } => {
                f(Num::Length, size);
                visit_paint(paint, f);
                for g in glyphs {
                    f(Num::Length, &mut g.x);
                    f(Num::Length, &mut g.y);
                }
            }
            Op::Image { src, dst, .. } => {
                lengths(f, src);
                lengths(f, dst);
            }
            Op::Shader { t, rect, params, .. } => {
                f(Num::Other, t);
                lengths(f, rect);
                params.values_mut().for_each(|v| f(Num::Other, v));
            }
        }
    }
}

fn visit_path(path: &mut Path, f: &mut dyn FnMut(Num, &mut f32)) {
    for el in &mut path.0 {
        el.for_each_point_mut(|p| p.iter_mut().for_each(|v| f(Num::Length, v)));
    }
}

fn visit_paint(paint: &mut Paint, f: &mut dyn FnMut(Num, &mut f32)) {
    let (points, stops): (Vec<&mut f32>, &mut Vec<Stop>) = match paint {
        Paint::Solid(_) => return,
        Paint::Linear { start, end, stops } => (start.iter_mut().chain(end.iter_mut()).collect(), stops),
        Paint::Radial { center, radius, stops } => (center.iter_mut().chain(std::iter::once(radius)).collect(), stops),
        Paint::Sweep { center, start_angle, end_angle, stops } => {
            f(Num::Other, start_angle);
            f(Num::Other, end_angle);
            (center.iter_mut().collect(), stops)
        }
    };
    points.into_iter().for_each(|v| f(Num::Length, v));
    stops.iter_mut().for_each(|s| f(Num::Other, &mut s.0));
}

/// Round every length to 1/64 cu and every transform's linear part to 2^-16, and
/// fold -0 into +0, so golden comparisons are exact across platforms (SPEC §13.4).
/// Opacity, offsets, angles, and shader values are not lengths and are left alone.
pub fn quantize(dl: &mut DisplayList) {
    dl.visit_mut(&mut |kind, v| {
        let step = match kind {
            Num::Length => 64.0,
            Num::Linear => 65536.0,
            Num::Other => return,
        };
        *v = (*v * step).round() / step + 0.0;
    });
}

/// Paint order for a snapshot's visible nodes: ascending `z` (default 0), ties in
/// scene-graph order. The sort is stable, so the order is a pure function of the document.
pub fn paint_order(snapshot: &Snapshot) -> Vec<&str> {
    let mut nodes: Vec<(i64, &str)> = snapshot
        .nodes
        .iter()
        .map(|(id, props)| (props.get("z").and_then(Value::as_i64).unwrap_or(0), id.as_str()))
        .collect();
    nodes.sort_by_key(|&(z, _)| z);
    nodes.into_iter().map(|(_, id)| id).collect()
}

// --- golden layout --------------------------------------------------------------

/// `serde_json` formatter behind [`DisplayList::to_golden_json`]. Only line breaks
/// differ from compact output; numbers keep serde_json's shortest `f32` form.
#[derive(Default)]
struct GoldenFormatter {
    /// One entry per open container: `(expanded, has_elements)`.
    stack: Vec<(bool, bool)>,
    in_key: bool,
    key: String,
    /// The key whose value is about to be written; consumed by the value.
    value_of: Option<String>,
}

impl GoldenFormatter {
    fn newline<W: ?Sized + io::Write>(&self, w: &mut W) -> io::Result<()> {
        let depth = self.stack.iter().filter(|(expanded, _)| *expanded).count();
        w.write_all(b"\n")?;
        w.write_all("  ".repeat(depth).as_bytes())
    }

    fn element<W: ?Sized + io::Write>(&mut self, w: &mut W, first: bool) -> io::Result<()> {
        if !first {
            w.write_all(b",")?;
        }
        let top = self.stack.last_mut().expect("element outside a container");
        top.1 = true;
        if top.0 { self.newline(w) } else { Ok(()) }
    }

    fn close<W: ?Sized + io::Write>(&mut self, w: &mut W, bracket: &[u8]) -> io::Result<()> {
        let (expanded, has_elements) = self.stack.pop().expect("balanced containers");
        if expanded && has_elements {
            self.newline(w)?;
        }
        w.write_all(bracket)
    }
}

impl serde_json::ser::Formatter for GoldenFormatter {
    fn begin_object<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.value_of = None;
        self.stack.push((self.stack.is_empty(), false));
        w.write_all(b"{")
    }

    fn end_object<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.close(w, b"}")
    }

    fn begin_object_key<W: ?Sized + io::Write>(&mut self, w: &mut W, first: bool) -> io::Result<()> {
        self.element(w, first)?;
        self.in_key = true;
        self.key.clear();
        Ok(())
    }

    fn end_object_key<W: ?Sized + io::Write>(&mut self, _w: &mut W) -> io::Result<()> {
        self.in_key = false;
        Ok(())
    }

    fn begin_object_value<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.value_of = Some(self.key.clone());
        w.write_all(b":")
    }

    fn end_object_value<W: ?Sized + io::Write>(&mut self, _w: &mut W) -> io::Result<()> {
        self.value_of = None;
        Ok(())
    }

    fn begin_array<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        let expanded = matches!(self.value_of.take().as_deref(), Some("fonts" | "ops" | "glyphs"));
        self.stack.push((expanded, false));
        w.write_all(b"[")
    }

    fn end_array<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.close(w, b"]")
    }

    fn begin_array_value<W: ?Sized + io::Write>(&mut self, w: &mut W, first: bool) -> io::Result<()> {
        self.element(w, first)
    }

    fn write_string_fragment<W: ?Sized + io::Write>(&mut self, w: &mut W, fragment: &str) -> io::Result<()> {
        if self.in_key {
            self.key.push_str(fragment);
        }
        w.write_all(fragment.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INK: Color = Color([0x16, 0x14, 0x0F, 0xFF]);

    /// Every op kind and every paint kind, with awkward numbers.
    fn sample() -> DisplayList {
        let mut dl = DisplayList::new([1920.0, 1080.0]);
        let serif = dl.font(FontRef { id: "fonts/RobotoSerif-VF.ttf".into(), index: 0 });
        dl.ops.push(Op::Fill {
            path: Path::rect([0.0, 0.0, 1920.0, 1080.0]),
            rule: FillRule::NonZero,
            paint: Paint::Solid(Color([0xFB, 0xFA, 0xF6, 0xFF])),
        });
        dl.ops.push(Op::Shader {
            kind: ShaderKind::Mesh,
            seed: 7,
            t: 0.25,
            rect: [0.0, 0.0, 1920.0, 1080.0],
            palette: vec![Color([0x0F, 0x76, 0x6E, 0xFF]), Color([0xC2, 0x41, 0x0C, 0x80])],
            params: [("points".to_string(), 5.0), ("drift".to_string(), 0.12)].into_iter().collect(),
        });
        dl.ops.push(Op::Layer {
            node: Some("title".into()),
            transform: [0.999_962, 0.008_727, -0.008_727, 0.999_962, 96.0, -3.5],
            opacity: 0.3,
            blend: Blend::Multiply,
            clip: Some(Path(vec![
                PathEl::MoveTo([96.0, 96.5]),
                PathEl::QuadTo([100.25, -0.125], [200.0, 96.0]),
                PathEl::CurveTo([1.0, 2.0], [3.0, 4.0], [5.0, 6.0]),
                PathEl::Close,
            ])),
            ops: vec![Op::Glyphs {
                font: serif,
                size: 64.0,
                coords: vec![0, -16384, 2048, 16384],
                paint: Paint::Solid(INK),
                glyphs: vec![Glyph { id: 38, x: 96.0, y: 300.015_63 }, Glyph { id: 72, x: 131.5, y: 300.015_63 }],
            }],
        });
        dl.ops.push(Op::Stroke {
            path: Path(vec![PathEl::MoveTo([0.0, 0.0]), PathEl::LineTo([10.0, -10.0])]),
            paint: Paint::Linear {
                start: [0.0, 0.0],
                end: [10.0, 0.0],
                stops: vec![Stop(0.0, INK), Stop(1.0, Color([0, 0, 0, 0]))],
            },
            width: 2.0,
            cap: Cap::Round,
            join: Join::Bevel,
            miter_limit: 4.0,
            dash: vec![4.0, 2.5],
            dash_offset: 1.0,
        });
        dl.ops.push(Op::Fill {
            path: Path::rect([10.0, 10.0, 20.0, 20.0]),
            rule: FillRule::EvenOdd,
            paint: Paint::Radial { center: [20.0, 20.0], radius: 10.0, stops: vec![Stop(0.5, INK)] },
        });
        dl.ops.push(Op::Fill {
            path: Path::rect([10.0, 10.0, 20.0, 20.0]),
            rule: FillRule::NonZero,
            paint: Paint::Sweep {
                center: [20.0, 20.0],
                start_angle: 0.0,
                end_angle: 360.0,
                stops: vec![Stop(0.0, INK)],
            },
        });
        dl.ops.push(Op::Image {
            asset: "sha256:00ff".into(),
            src: [0.0, 0.0, 64.0, 64.0],
            dst: [8.0, 8.0, 32.0, 32.0],
            quality: Quality::High,
        });
        dl
    }

    #[test]
    fn json_round_trips_in_both_layouts() {
        let dl = sample();
        assert_eq!(DisplayList::from_json(&dl.to_json().unwrap()).unwrap(), dl);
        let golden = dl.to_golden_json().unwrap();
        assert_eq!(DisplayList::from_json(&golden).unwrap(), dl);
        assert_eq!(
            DisplayList::from_json(&golden).unwrap().to_golden_json().unwrap(),
            golden,
            "golden layout is canonical"
        );
    }

    #[test]
    fn postcard_round_trips_and_encoding_is_stable() {
        let dl = sample();
        let bytes = dl.to_postcard().unwrap();
        assert_eq!(DisplayList::from_postcard(&bytes).unwrap(), dl);
        assert_eq!(dl.clone().to_postcard().unwrap(), bytes, "same value, same bytes");
        assert!(bytes.len() * 2 < dl.to_json().unwrap().len(), "binary is the compact form ({} bytes)", bytes.len());
    }

    #[test]
    fn golden_layout_is_pinned() {
        let mut dl = DisplayList::new([1920.0, 1080.0]);
        let font = dl.font(FontRef { id: "fonts/a.ttf".into(), index: 0 });
        dl.ops.push(Op::Fill {
            path: Path::rect([0.0, 0.0, 2.0, 1.5]),
            rule: FillRule::NonZero,
            paint: Paint::Solid(INK),
        });
        dl.ops.push(Op::Layer {
            node: Some("t".into()),
            transform: IDENTITY,
            opacity: 1.0,
            blend: Blend::Normal,
            clip: None,
            ops: vec![Op::Glyphs {
                font,
                size: 64.0,
                coords: vec![],
                paint: Paint::Solid(INK),
                glyphs: vec![Glyph { id: 1, x: 0.5, y: 2.0 }, Glyph { id: 2, x: 30.25, y: 2.0 }],
            }],
        });
        let expected = r##"{
  "dl":1,
  "viewport":[1920.0,1080.0],
  "fonts":[
    {"id":"fonts/a.ttf","index":0}
  ],
  "ops":[
    {"fill":{"path":"M0 0L2 0L2 1.5L0 1.5Z","rule":"nonzero","paint":{"solid":"#16140FFF"}}},
    {"layer":{"node":"t","transform":[1.0,0.0,0.0,1.0,0.0,0.0],"opacity":1.0,"blend":"normal","clip":null,"ops":[
      {"glyphs":{"font":0,"size":64.0,"coords":[],"paint":{"solid":"#16140FFF"},"glyphs":[
        [1,0.5,2.0],
        [2,30.25,2.0]
      ]}}
    ]}}
  ]
}
"##;
        assert_eq!(dl.to_golden_json().unwrap(), expected);
    }

    #[test]
    fn every_finite_f32_survives_json_exactly() {
        // serde_json parses numbers as f64 then narrows; check that never double-rounds.
        // (A one-off sweep of 10^6 patterns found no failures; this keeps a sample in CI.)
        let mut s: u32 = 0x9E37_79B9;
        for _ in 0..50_000 {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let v = f32::from_bits(s);
            if v.is_finite() {
                let back: f32 = serde_json::from_str(&serde_json::to_string(&v).unwrap()).unwrap();
                assert_eq!(back.to_bits(), v.to_bits(), "{v:e}");
            }
        }
    }

    #[test]
    fn non_finite_numbers_are_refused_by_every_encoder() {
        let mut dl = sample();
        if let Op::Layer { ops, .. } = &mut dl.ops[2]
            && let Op::Glyphs { glyphs, .. } = &mut ops[0]
        {
            glyphs[1].x = f32::NAN;
        }
        assert!(matches!(dl.to_json(), Err(DlError::NonFinite(Num::Length))));
        assert!(matches!(dl.to_golden_json(), Err(DlError::NonFinite(Num::Length))));
        assert!(matches!(dl.to_postcard(), Err(DlError::NonFinite(Num::Length))));
    }

    #[test]
    fn unknown_versions_and_fields_are_rejected() {
        let mut dl = sample();
        dl.dl = 2;
        let json = serde_json::to_string(&dl).unwrap();
        assert!(matches!(DisplayList::from_json(&json), Err(DlError::Version(2))));
        assert!(matches!(DisplayList::from_postcard(&postcard::to_allocvec(&dl).unwrap()), Err(DlError::Version(2))));
        let json = sample().to_json().unwrap().replacen("\"rule\"", "\"bogus\":1,\"rule\"", 1);
        assert!(DisplayList::from_json(&json).is_err());
    }

    #[test]
    fn svg_paths_round_trip_and_reject_garbage() {
        let path = Path(vec![
            PathEl::MoveTo([-3.25, 0.015_625]),
            PathEl::LineTo([1920.0, 1e-3]),
            PathEl::QuadTo([0.1, 0.2], [123_456.79, -0.0]),
            PathEl::CurveTo([1.0, 2.0], [3.0, 4.0], [5.0, 6.0]),
            PathEl::Close,
        ]);
        let svg = path.to_svg();
        assert_eq!(svg, "M-3.25 0.015625L1920 0.001Q0.1 0.2 123456.79 -0C1 2 3 4 5 6Z");
        assert_eq!(Path::from_svg(&svg).unwrap(), path);
        assert_eq!(Path::from_svg(" M 0,0 L 1,2 Z ").unwrap().to_svg(), "M0 0L1 2Z");
        for bad in ["M0", "X0 0", "M0 NaN", "M0 inf", "0 0", "M0 0L1"] {
            assert!(Path::from_svg(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn colors_are_hex_in_json_and_bytes_in_postcard() {
        assert_eq!(Color::from_hex("#ff6a3d").unwrap(), Color([0xFF, 0x6A, 0x3D, 0xFF]));
        assert_eq!(Color::from_hex("#FF6A3D80").unwrap().to_hex(), "#FF6A3D80");
        for bad in ["#12345", "123456", "#GG0000", "#ÿÿÿ", "#1234567"] {
            assert!(Color::from_hex(bad).is_err(), "{bad}");
        }
        assert_eq!(postcard::to_allocvec(&INK).unwrap(), INK.0);
    }

    #[test]
    fn quantize_rounds_lengths_and_linear_parts_separately() {
        let mut dl = sample();
        quantize(&mut dl);
        let Op::Layer { transform, opacity, ops, .. } = &dl.ops[2] else { panic!() };
        // A half-degree rotation survives: the linear part rounds to 2^-16, not 1/64.
        assert_eq!(transform[1], (0.008_727_f32 * 65536.0).round() / 65536.0);
        assert!(transform[1] > 0.0);
        assert_eq!(transform[5], -3.5);
        assert_eq!(*opacity, 0.3, "opacity is not a length");
        let Op::Glyphs { glyphs, .. } = &ops[0] else { panic!() };
        assert_eq!(glyphs[0].y, 300.0 + 1.0 / 64.0, "rounded to 1/64 cu");
        let mut negative_zero = DisplayList::new([1.0, 1.0]);
        negative_zero.ops.push(Op::Fill {
            path: Path(vec![PathEl::MoveTo([-0.001, 0.0])]),
            rule: FillRule::NonZero,
            paint: Paint::Solid(INK),
        });
        quantize(&mut negative_zero);
        let Op::Fill { path, .. } = &negative_zero.ops[0] else { panic!() };
        assert_eq!(path.to_svg(), "M0 0", "-0 folds into +0");
        let mut twice = dl.clone();
        quantize(&mut twice);
        assert_eq!(twice, dl, "quantize is idempotent");
    }

    #[test]
    fn font_table_is_first_use_order_without_duplicates() {
        let mut dl = DisplayList::new([1.0, 1.0]);
        let a = FontRef { id: "fonts/a.ttf".into(), index: 0 };
        let b = FontRef { id: "fonts/b.ttf".into(), index: 0 };
        assert_eq!((dl.font(b.clone()), dl.font(a.clone()), dl.font(b.clone())), (0, 1, 0));
        assert_eq!(dl.fonts, vec![b, a]);
    }

    #[test]
    fn paint_order_is_z_then_scene_graph_order() {
        let deck = crate::Deck::from_json(include_str!("../../../docs/examples/revenue.deck.json")).unwrap();
        let mut snaps = crate::resolve_states(&deck).unwrap();
        assert_eq!(paint_order(&snaps[0]), ["bg", "title", "subtitle"]);
        // Push the background to the front: z decides; equal z keeps scene-graph order.
        snaps[1].nodes["bg"].insert("z".into(), serde_json::json!(5));
        snaps[1].nodes["note"].insert("z".into(), serde_json::json!(-1));
        assert_eq!(paint_order(&snaps[1]), ["note", "title", "rev", "bg"]);
    }
}
