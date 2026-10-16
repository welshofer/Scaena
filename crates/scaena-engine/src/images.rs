//! Image nodes (SPEC §3.3): a bundle's PNGs and JPEGs, placed in their box.
//!
//! The engine needs only each image's size and its content id; painters decode the
//! pixels (`scaena_paint::Resources`). `fit` says how the image meets its box: `cover`
//! (the default) fills the box and crops the image, `contain` shows the whole image
//! inside the box, and `fill` stretches it. `focal` is a point of the image, in fractions
//! (`[0.5, 0.5]` by default): the box and the image line up at it, as CSS
//! `object-position` does with percentages, so `cover` keeps it in view. `crop` cuts the
//! image to `[x, y, w, h]`, fractions of it, before anything else, so a sharper file of
//! the same picture keeps the crop. `radius` rounds the corners of what shows, and `mask` shows
//! it only inside a shape, an oval or a polygon of the shape library (PLAN 2.113). `adjust` sets
//! its light and color (PLAN 2.117): the display list names the image's pixels so adjusted by an
//! asset id of their own, which the painters make from the image's (`scaena_core::adjust`).
//!
//! Every picture takes its theme's frame (PLAN T.3a), and no image says otherwise: a keyline at
//! the picture's edge, then a mat of a theme color around it, drawn inside the box as fills, the
//! picture fitted, cropped, and focused within what they leave. `radius` then rounds the frame's
//! outer corners, the picture's own concentric inside them, and a mask's shape is drawn larger
//! for each band.
//!
//! Images are PNGs and JPEGs, each read by one decoder in Rust alone, the same pixels everywhere
//! (SPEC §13): a JPEG by `scaena_core::jpeg`, in integers (ADR-0017). A JPEG's size is its size as
//! seen, turned by its EXIF orientation, as its pixels are.

use crate::EngineError;
use crate::charts::{RoundRect, lerp};
use crate::shadows::Shadow;
use crate::shapes::{Border, path_of, polyline};
use crate::text::TextLayout;
use crate::theme::Theme;
use scaena_core::adjust::Adjust;
use scaena_core::displaylist::{
    Blend, Cap, Color, FillRule, IDENTITY, Join, MAX_IMAGE_SIDE, Op, Paint, Path, Quality, Rect,
};
use scaena_core::document::{NodeType, Props};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// What the engine knows of an image: its content id, as display lists name it, its size in
/// pixels, and whether it is a photo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageInfo {
    /// `sha256:<hex>` of the file's bytes.
    pub id: String,
    pub width: u32,
    pub height: u32,
    /// A JPEG, which hides all that is under it; a PNG may let some show through.
    pub photo: bool,
    /// A PNG that says it may let some of what is under it show: an alpha channel, or a color it
    /// makes transparent (`scaena_core::looks::see_through`). It casts no shadow (PLAN 2.115).
    pub clear: bool,
}

impl ImageInfo {
    /// A PNG's or a JPEG's id and size, from its header; the pixels are the painters' to decode.
    pub fn read(bytes: &[u8]) -> Result<ImageInfo, String> {
        let Picture { width, height, largest } = Picture::read(bytes)?;
        if width.max(height) > largest {
            return Err(format!("{width} × {height} px; images are at most {largest} px a side (SPEC §3.3)"));
        }
        let hex: String = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
        let (photo, clear) = (scaena_core::jpeg::is_jpeg(bytes), scaena_core::looks::see_through(bytes));
        Ok(ImageInfo { id: format!("sha256:{hex}"), width, height, photo, clear })
    }
}

/// What an editor reads of a picture before it joins the bundle (SPEC §3.3): its size as it is
/// seen, and the longest side an image may have, so one too large is offered as a smaller copy
/// rather than refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    /// The longest side an image may have, in pixels ([`MAX_IMAGE_SIDE`]).
    pub largest: u32,
}

impl Picture {
    /// The size of the PNG or JPEG `bytes` as it is seen, from its header: a JPEG's turned by its
    /// EXIF orientation. One of any size is read; what is neither, or a JPEG this engine cannot
    /// decode, is refused, saying why.
    pub fn read(bytes: &[u8]) -> Result<Picture, String> {
        const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
        let (width, height) = if bytes.starts_with(SIGNATURE) {
            // The first chunk is IHDR: length, type, then width and height, big-endian.
            let ihdr = bytes.get(8..24).filter(|h| &h[4..8] == b"IHDR").ok_or("a PNG with no IHDR chunk")?;
            let be = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
            (be(&ihdr[8..12]), be(&ihdr[12..16]))
        } else if scaena_core::jpeg::is_jpeg(bytes) {
            scaena_core::jpeg::Header::read(bytes).map_err(|e| e.to_string())?.size()
        } else {
            return Err("neither a PNG nor a JPEG; images are PNGs and JPEGs (SPEC §3.3)".into());
        };
        if width == 0 || height == 0 {
            return Err("an image with no pixels".into());
        }
        Ok(Picture { width, height, largest: MAX_IMAGE_SIDE })
    }

    /// Whether an image shows it as it is: no side longer than [`Picture::largest`].
    pub fn fits(&self) -> bool {
        self.width.max(self.height) <= self.largest
    }
}

/// A bundle's images by path, as image nodes name them.
#[derive(Debug, Clone, Default)]
pub struct BundleImages(BTreeMap<String, ImageInfo>);

impl BundleImages {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the image file at bundle path `path`.
    pub fn register(&mut self, path: &str, bytes: &[u8]) -> Result<ImageInfo, EngineError> {
        let info = ImageInfo::read(bytes).map_err(|e| EngineError::Data(format!("{path}: {e}")))?;
        self.0.insert(path.to_string(), info.clone());
        Ok(info)
    }

    pub fn get(&self, path: &str) -> Option<&ImageInfo> {
        self.0.get(path)
    }
}

/// How an image meets its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fit {
    Cover,
    Contain,
    Fill,
}

/// An image node, resolved and placed in its box.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageNode {
    /// The box, canvas units.
    pub rect: Rect,
    info: ImageInfo,
    fit: Fit,
    focal: [f32; 2],
    /// Of the image, in fractions.
    crop: Rect,
    radius: f32,
    mask: Option<Mask>,
    /// The shadow it casts (PLAN 2.115), under what shows: none under a mask but an oval.
    shadow: Option<Shadow>,
    /// Its light and color (PLAN 2.117).
    adjust: Adjust,
    /// Between two states whose adjustments differ: the one it leaves, drawn under the one it
    /// takes, which fades in this much.
    under: Option<(Adjust, f32)>,
    /// Its border (PLAN 2.118), stroked around what shows as a shape's outline is, and how much
    /// of it shows: less than all only between a state that draws it and one that does not.
    border: Option<(Border, f32)>,
    /// How much of the box's foot a credit set below the picture keeps (ADR-0030): the picture
    /// shows in the rest.
    foot: f32,
    /// What the engine draws over the picture (ADR-0030): a pin, its label, and a credit.
    pub marks: Box<Marks>,
    /// Its theme's frame (PLAN T.3a).
    frame: Option<Frame>,
}

/// A picture's frame, as its theme says it (PLAN T.3a): its keyline and its mat, each a width,
/// canvas units, and a color, from the picture out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub keyline: Option<(f32, Color)>,
    pub mat: Option<(f32, Color)>,
}

impl Frame {
    /// How wide it is, canvas units: its bands together.
    pub fn width(&self) -> f32 {
        self.keyline.map_or(0.0, |k| k.0) + self.mat.map_or(0.0, |m| m.0)
    }
}

/// What the engine draws over a picture (ADR-0030, decisions 13–14): a pin, its label beside it,
/// and the credit a fetched picture's service asks, each box-local, laid out once per snapshot.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Marks {
    pub pin: Option<PinMark>,
    pub label: Option<Card>,
    pub credit: Option<Card>,
}

/// A pin, drawn with its tip on the point of the picture it marks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PinMark {
    /// Its tip, box-local.
    pub tip: [f32; 2],
    /// How tall it is, canvas units.
    pub height: f32,
    pub color: Color,
    /// The ring around it, in the surface's color, so it reads on any map.
    pub ring: Color,
}

/// Words the engine sets on a picture: a pin's label or a credit, on a card of the surface, or a
/// credit below the picture with none.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    /// The card, box-local; where the words stand, where there is no card.
    pub rect: Rect,
    /// The card's fill: the surface, or none for a credit set below the picture.
    pub fill: Option<Color>,
    pub radius: f32,
    /// Where the words' layout stands, box-local.
    pub origin: [f32; 2],
    pub text: TextLayout,
}

/// The pin's outline on its 256 by 256 view: where its tip and its head's middle stand, and its
/// width and height there.
const PIN_TIP: [f32; 2] = [128.0, 240.48];
const PIN_HEAD: f32 = 104.0;
const PIN_TOP: f32 = 16.0;
const PIN_WIDTH: f32 = 176.0;

impl PinMark {
    /// Its width, canvas units.
    pub fn width(&self) -> f32 {
        self.height * PIN_WIDTH / (PIN_TIP[1] - PIN_TOP)
    }

    /// The middle of its head, box-local: where a label lines up.
    pub fn head(&self) -> [f32; 2] {
        let k = self.height / (PIN_TIP[1] - PIN_TOP);
        [self.tip[0], self.tip[1] - (PIN_TIP[1] - PIN_HEAD) * k]
    }

    /// The box it is drawn in, box-local.
    pub fn bounds(&self) -> Rect {
        let w = self.width();
        [self.tip[0] - w / 2.0, self.tip[1] - self.height, w, self.height]
    }

    /// Its outline, box-local: Phosphor's `map-pin`, fill weight, its tip on [`PinMark::tip`].
    pub fn path(&self) -> Path {
        let k = f64::from(self.height / (PIN_TIP[1] - PIN_TOP));
        let at = kurbo::Affine::translate((
            f64::from(self.tip[0]) - f64::from(PIN_TIP[0]) * k,
            f64::from(self.tip[1]) - f64::from(PIN_TIP[1]) * k,
        )) * kurbo::Affine::scale(k);
        let bez = kurbo::BezPath::from_svg(crate::pin::MAP_PIN).expect("the pin's path parses");
        path_of(&(at * bez))
    }

    /// What it draws: its ring of the surface, then the pin in its color.
    pub fn ops(&self) -> Vec<Op> {
        let path = self.path();
        let ring = (self.height / 16.0).max(1.0);
        vec![
            Op::Stroke {
                path: path.clone(),
                paint: Paint::Solid(self.ring),
                width: 2.0 * ring,
                cap: Cap::Round,
                join: Join::Round,
                miter_limit: 4.0,
                dash: Vec::new(),
                dash_offset: 0.0,
            },
            Op::Fill { path, rule: FillRule::NonZero, paint: Paint::Solid(self.color) },
        ]
    }
}

impl Card {
    /// The card's fill, under its words: none for a credit below the picture.
    pub fn ground(&self) -> Option<Op> {
        let fill = self.fill?;
        let [x, y, w, h] = self.rect;
        Some(Op::Fill {
            path: RoundRect::rounded(x, y, w, h, self.radius.min(w.min(h) / 2.0)).path(),
            rule: FillRule::NonZero,
            paint: Paint::Solid(fill),
        })
    }
}

/// The shape an image shows inside (PLAN 2.113).
#[derive(Debug, Clone, PartialEq)]
enum Mask {
    Oval,
    /// A polygon of the shape library: its points, fractions of what shows.
    Polygon(Vec<[f32; 2]>),
}

impl Mask {
    /// `name`, as a deck writes it: `ellipse`, or a polygon of the shape library.
    fn named(name: &str) -> Option<Mask> {
        if name == "ellipse" {
            return Some(Mask::Oval);
        }
        let (_, points, _) = scaena_core::inserts::polygons().into_iter().find(|(n, _, _)| *n == name)?;
        Some(Mask::Polygon(points.into_iter().map(|[x, y]| [x as f32, y as f32]).collect()))
    }

    /// Its outline over `[x, y, w, h]`, box-local.
    fn outline(&self, [x, y, w, h]: Rect) -> Path {
        match self {
            Mask::Oval => {
                let (rx, ry) = (f64::from(w) / 2.0, f64::from(h) / 2.0);
                let oval = kurbo::Ellipse::new((f64::from(x) + rx, f64::from(y) + ry), (rx, ry), 0.0);
                path_of(&kurbo::Shape::to_path(&oval, 0.1))
            }
            Mask::Polygon(points) => polyline(points.iter().map(|[px, py]| [x + px * w, y + py * h]), true),
        }
    }
}

impl ImageNode {
    pub fn resolve(props: &Props, theme: &Theme, images: &BundleImages, rect: Rect) -> Result<ImageNode, EngineError> {
        let src = props
            .get("src")
            .and_then(Value::as_str)
            .ok_or_else(|| EngineError::Layout("an image needs `src`".into()))?;
        let info =
            images.get(src).cloned().ok_or_else(|| EngineError::Data(format!("image `{src}` is not in the bundle")))?;
        let fit = match props.get("fit").and_then(Value::as_str) {
            None | Some("cover") => Fit::Cover,
            Some("contain") => Fit::Contain,
            Some("fill") => Fit::Fill,
            Some(other) => return Err(EngineError::Layout(format!("image fit `{other}`: cover, contain, or fill"))),
        };
        let pair = |key: &str, default: [f32; 2]| -> Result<[f32; 2], EngineError> {
            match props.get(key) {
                None => Ok(default),
                Some(v) => serde_json::from_value::<[f32; 2]>(v.clone())
                    .map_err(|_| EngineError::Layout(format!("`{key}` {v}: expected [x, y], fractions of the image"))),
            }
        };
        let focal = pair("focal", [0.5, 0.5])?.map(|v| v.clamp(0.0, 1.0));
        let crop = match props.get("crop") {
            None => [0.0, 0.0, 1.0, 1.0],
            Some(v) => {
                let [x, y, w, h] = serde_json::from_value::<[f32; 4]>(v.clone()).map_err(|_| {
                    EngineError::Layout(format!("`crop` {v}: expected [x, y, w, h], fractions of the image"))
                })?;
                if !(w > 0.0 && h > 0.0 && x >= 0.0 && y >= 0.0 && x + w <= 1.0 + 1e-6 && y + h <= 1.0 + 1e-6) {
                    return Err(EngineError::Layout(format!("`crop` {v} is not a part of the image")));
                }
                [x, y, w, h]
            }
        };
        let radius = match props.get("radius") {
            Some(r) => theme.length(r, rect[2].min(rect[3]))?.max(0.0),
            None => 0.0,
        };
        let mask = match props.get("mask") {
            None | Some(Value::Null) => None,
            Some(v) => Some(v.as_str().and_then(Mask::named).ok_or_else(|| {
                EngineError::Layout(format!("image `mask` {v}: `ellipse`, or a shape of the library by its name"))
            })?),
        };
        // Under no mask but an oval, from a picture that hides all under it (PLAN 2.115).
        let shadow = Shadow::of(theme, props.get("shadow"))?;
        let shadow = shadow.filter(|_| scaena_core::looks::casts(NodeType::Image, props, info.clear));
        let adjust = match props.get("adjust") {
            None | Some(Value::Null) => Adjust::default(),
            Some(Value::Object(a)) => {
                let fraction = |key: &str| -> Result<f64, EngineError> {
                    match a.get(key) {
                        None | Some(Value::Null) => Ok(0.0),
                        Some(v) => v.as_f64().filter(|v| (-1.0..=1.0).contains(v)).ok_or_else(|| {
                            EngineError::Layout(format!("image `adjust/{key}` {v}: a number from -1 to 1"))
                        }),
                    }
                };
                Adjust::from_fractions(
                    fraction("exposure")?,
                    fraction("contrast")?,
                    fraction("saturation")?,
                    fraction("warmth")?,
                )
            }
            Some(v) => {
                return Err(EngineError::Layout(format!(
                    "image `adjust` {v}: {{ exposure, contrast, saturation, warmth }}, each from -1 to 1"
                )));
            }
        };
        let border = props.get("stroke").map(|s| Border::of(theme, s)).transpose()?.map(|b| (b, 1.0));
        Ok(ImageNode {
            rect,
            info,
            fit,
            focal,
            crop,
            radius,
            mask,
            shadow,
            adjust,
            under: None,
            border,
            foot: 0.0,
            marks: Box::default(),
            frame: theme.frame()?,
        })
    }

    /// The image with `foot` of its box's foot kept for a credit below the picture (ADR-0030).
    pub fn keeping(self, foot: f32) -> ImageNode {
        ImageNode { foot: foot.clamp(0.0, self.rect[3]), ..self }
    }

    /// Where the picture shows, box-local.
    pub fn shown(&self) -> Rect {
        self.placement().1
    }

    /// The point of the picture at `at`, in fractions of the whole image as `crop` is, box-local
    /// where it shows: where a pin's tip stands (ADR-0030). `None` where it does not show.
    pub fn spot(&self, at: [f32; 2]) -> Option<[f32; 2]> {
        let (src, [dx, dy, dw, dh]) = self.placement();
        let [ix, iy] = [at[0] * self.info.width as f32, at[1] * self.info.height as f32];
        if src[2] <= 0.0 || src[3] <= 0.0 {
            return None;
        }
        let [x, y] = [dx + (ix - src[0]) / src[2] * dw, dy + (iy - src[1]) / src[3] * dh];
        let inside = |v: f32, lo: f32, len: f32| v >= lo - 1e-3 && v <= lo + len + 1e-3;
        (inside(x, dx, dw) && inside(y, dy, dh)).then_some([x, y])
    }

    /// The same image in another box.
    pub fn at(&self, rect: Rect) -> ImageNode {
        ImageNode { rect, ..self.clone() }
    }

    /// Whether `other` shows the same picture the same way, perhaps in another box.
    pub fn same_image(&self, other: &ImageNode) -> bool {
        (&self.info, self.fit, self.focal, self.crop, &self.mask)
            == (&other.info, other.fit, other.focal, other.crop, &other.mask)
    }

    /// `a` to `b`, `p` of the way: the box, the radius, and the shadow move, where the two
    /// adjust the picture apart `b`'s fades in over `a`'s (PLAN 2.117), and a border that both
    /// draw moves, its paint and its width, as a shape's stroke does, one that only one draws
    /// fading in or out (PLAN 2.118).
    pub fn lerp(a: &ImageNode, b: &ImageNode, p: f32) -> ImageNode {
        let rect = [0, 1, 2, 3].map(|k| lerp(a.rect[k], b.rect[k], p));
        let shadow = Shadow::between(a.shadow, b.shadow, p);
        // Past either end on a spring, the box overshoots; what fades does not.
        let fade = p.clamp(0.0, 1.0);
        let under = (a.adjust != b.adjust).then_some((a.adjust, fade));
        let border = match (&a.border, &b.border) {
            (Some((x, _)), None) => Some((x.clone(), 1.0 - fade)),
            (None, Some((y, _))) => Some((y.clone(), fade)),
            (Some((x, u)), Some((y, v))) => {
                let moved = if x == y { y.clone() } else { Border::lerp(x, y, fade) };
                Some((moved, lerp(*u, *v, fade)))
            }
            (None, None) => None,
        };
        ImageNode { rect, radius: lerp(a.radius, b.radius, p).max(0.0), shadow, under, border, ..b.clone() }
    }

    /// How far in from the box's edge its frame stands (PLAN T.3a), box-local, and how much of
    /// each band's width it draws: all of it, but in a box so small that the frame would take more
    /// than half its shorter side, where every band narrows alike.
    fn inset(&self) -> (f32, f32) {
        let Some(frame) = self.frame else { return (0.0, 1.0) };
        let [_, _, w, h] = self.rect;
        let room = w.min((h - self.foot).max(0.0)).max(0.0) / 4.0;
        let width = frame.width();
        if width <= room { (width, 1.0) } else { (room, room / width) }
    }

    /// The pixels of the image that show, and where in the box, box-local: `(src, dst)`.
    /// Within its frame (PLAN T.3a), where its theme gives one.
    pub fn placement(&self) -> (Rect, Rect) {
        let [_, _, w, h] = self.rect;
        // A credit set below keeps the box's foot (ADR-0030).
        let h = (h - self.foot).max(0.0);
        let (t, _) = self.inset();
        let (src, [dx, dy, dw, dh]) = self.fitted((w - 2.0 * t).max(0.0), (h - 2.0 * t).max(0.0));
        (src, [dx + t, dy + t, dw, dh])
    }

    /// The pixels of the image that show, and where in a box `w` by `h` at the origin.
    fn fitted(&self, w: f32, h: f32) -> (Rect, Rect) {
        let (iw, ih) = (self.info.width as f32, self.info.height as f32);
        let [fx, fy] = self.focal;
        // The crop, in pixels.
        let [cx, cy, cw, ch] = [self.crop[0] * iw, self.crop[1] * ih, self.crop[2] * iw, self.crop[3] * ih];
        match self.fit {
            Fit::Fill => ([cx, cy, cw, ch], [0.0, 0.0, w, h]),
            Fit::Cover => {
                let s = (w / cw).max(h / ch);
                let (sw, sh) = (w / s, h / s);
                ([cx + (cw - sw) * fx, cy + (ch - sh) * fy, sw, sh], [0.0, 0.0, w, h])
            }
            Fit::Contain => {
                let s = (w / cw).min(h / ch);
                let (dw, dh) = (cw * s, ch * s);
                ([cx, cy, cw, ch], [(w - dw) * fx, (h - dh) * fy, dw, dh])
            }
        }
    }

    /// The point of the image drawn under `at` (canvas units), in fractions of the part its
    /// crop keeps: what `focal` names, so that a focal point picked where the image shows
    /// keeps that point in view (PLAN 2.45). `None` off the image.
    pub fn point(&self, at: [f32; 2]) -> Option<[f32; 2]> {
        let (src, [dx, dy, dw, dh]) = self.placement();
        let [px, py] = [at[0] - self.rect[0], at[1] - self.rect[1]];
        if dw <= 0.0 || dh <= 0.0 || px < dx || py < dy || px > dx + dw || py > dy + dh {
            return None;
        }
        let (iw, ih) = (self.info.width as f32, self.info.height as f32);
        let [cx, cy, cw, ch] = [self.crop[0] * iw, self.crop[1] * ih, self.crop[2] * iw, self.crop[3] * ih];
        // The image's pixel under the point, then where it is in the crop.
        let [ix, iy] = [src[0] + (px - dx) / dw * src[2], src[1] + (py - dy) / dh * src[3]];
        Some([((ix - cx) / cw).clamp(0.0, 1.0), ((iy - cy) / ch).clamp(0.0, 1.0)])
    }

    /// The image as a pointer crops it (PLAN 2.74), canvas units as laid out: where its whole is
    /// drawn, at the scale and the offset its crop and its fit draw it at, and where the part that
    /// shows stands; with its `crop` (fractions of the image), its `focal` (fractions of the crop),
    /// its fit, and its size in pixels.
    pub fn framing(&self) -> (Rect, Rect, Rect, [f32; 2], &'static str, [u32; 2]) {
        let (src, dst) = self.placement();
        let (iw, ih) = (self.info.width as f32, self.info.height as f32);
        let [sx, sy] = [dst[2] / src[2].max(1e-6), dst[3] / src[3].max(1e-6)];
        let [x, y] = [self.rect[0] + dst[0], self.rect[1] + dst[1]];
        let whole = [x - src[0] * sx, y - src[1] * sy, iw * sx, ih * sy];
        let fit = match self.fit {
            Fit::Cover => "cover",
            Fit::Contain => "contain",
            Fit::Fill => "fill",
        };
        (whole, [x, y, dst[2], dst[3]], self.crop, self.focal, fit, [self.info.width, self.info.height])
    }

    /// What the image draws, box-local: the image, its pixels as adjusted (PLAN 2.117), clipped
    /// to its mask's shape if it has one (PLAN 2.113), else to rounded corners if it has them.
    pub fn ops(&self) -> Vec<Op> {
        let (src, dst) = self.placement();
        let drawn = |adjust: Adjust| Op::Image { asset: adjust.name(&self.info.id), src, dst, quality: Quality::High };
        let faded = |op: Op, opacity: f32| {
            if opacity >= 1.0 {
                return op;
            }
            Op::Layer {
                node: None,
                cell: None,
                transform: IDENTITY,
                opacity,
                blend: Blend::Normal,
                clip: None,
                ops: vec![op],
            }
        };
        // Between two adjustments, the one it takes fades in over the one it leaves: a photo,
        // which hides what is under it, over itself as it was; a PNG, which may let it show,
        // as the other fades out.
        let images = match self.under {
            None => vec![drawn(self.adjust)],
            Some((under, p)) => {
                let below = if self.info.photo { 1.0 } else { 1.0 - p };
                vec![faded(drawn(under), below), faded(drawn(self.adjust), p)]
            }
        };
        // Its frame (PLAN T.3a), from the picture out: the radius rounds its outer edge, and each
        // edge inside it is rounded concentric with that one, the picture's too.
        let (t, k) = self.inset();
        let out = |d: f32| [dst[0] - d, dst[1] - d, dst[2] + 2.0 * d, dst[3] + 2.0 * d];
        let rounding = |d: f32| (self.radius - (t - d)).max(0.0);
        let shape = |d: f32| {
            let [x, y, w, h] = out(d);
            match &self.mask {
                Some(mask) => mask.outline([x, y, w, h]),
                None => RoundRect::rounded(x, y, w, h, rounding(d).min(w.min(h) / 2.0)).path(),
            }
        };
        let ring = |inner: f32, outer: f32, color: Color| {
            let mut path = shape(outer);
            path.0.extend(shape(inner).0);
            Op::Fill { path, rule: FillRule::EvenOdd, paint: Paint::Solid(color) }
        };
        let mut frame = Vec::new();
        if let Some(f) = self.frame {
            let keyline = f.keyline.map_or(0.0, |(w, _)| w * k);
            frame.extend(f.keyline.map(|(_, color)| ring(0.0, keyline, color)));
            frame.extend(f.mat.map(|(_, color)| ring(keyline, t, color)));
        }
        // What shows, its frame and all: the shadow is under it and the border around it.
        let edge = out(t);
        let [_, _, ew, eh] = edge;
        // Its shadow under what shows (PLAN 2.115), its outline the picture's: rounded as it is,
        // or an oval.
        let shadow = self.shadow.map(|s| match &self.mask {
            Some(Mask::Oval) => s.oval(edge),
            _ => s.op(edge, self.radius.min(ew.min(eh) / 2.0)),
        });
        // Its border (PLAN 2.118) on top, around what shows as a shape's outline is stroked: the
        // mask's shape, the rounded corners, or the picture's edge.
        let border = self.border.as_ref().map(|(b, shown)| {
            let [x, y, w, h] = edge;
            let outline = match &self.mask {
                Some(mask) => mask.outline([0.0, 0.0, w, h]),
                None => RoundRect::rounded(0.0, 0.0, w, h, self.radius.min(w.min(h) / 2.0)).path(),
            };
            Op::Layer {
                node: None,
                cell: None,
                transform: [1.0, 0.0, 0.0, 1.0, x, y],
                opacity: *shown,
                blend: Blend::Normal,
                clip: None,
                ops: vec![b.op(outline, w, h)],
            }
        });
        let clip = match &self.mask {
            Some(_) => shape(0.0),
            None if rounding(0.0) > 0.0 => shape(0.0),
            None => return shadow.into_iter().chain(frame).chain(images).chain(border).collect(),
        };
        let layer = Op::Layer {
            node: None,
            cell: None,
            transform: IDENTITY,
            opacity: 1.0,
            blend: Blend::Normal,
            clip: Some(clip),
            ops: images,
        };
        shadow.into_iter().chain(frame).chain([layer]).chain(border).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scaena_core::displaylist::{Paint, PathEl};
    use serde_json::json;

    /// A PNG header for an image of `w` × `h`: all the engine reads.
    /// A PNG's header, its pixels in color with no alpha.
    fn png(w: u32, h: u32) -> Vec<u8> {
        png_of(w, h, 2)
    }

    /// A PNG's header, its pixels of color type `color`: 6 is in color with alpha.
    fn png_of(w: u32, h: u32, color: u8) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        b.extend(w.to_be_bytes());
        b.extend(h.to_be_bytes());
        b.extend([8, color, 0, 0, 0]);
        b
    }

    fn node(props: Value, rect: Rect) -> ImageNode {
        let mut images = BundleImages::new();
        images.register("assets/photo.png", &png(400, 200)).unwrap();
        images.register("assets/logo.png", &png_of(400, 200, 6)).unwrap();
        let theme = Theme::from_json(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
        ImageNode::resolve(&serde_json::from_value(props).unwrap(), &theme, &images, rect).unwrap()
    }

    /// In Dusk with a frame (PLAN T.3a): a keyline of 2 in its ink, then a mat of 8 in its card.
    fn framed(props: Value, rect: Rect) -> ImageNode {
        let mut images = BundleImages::new();
        images.register("assets/photo.png", &png(400, 200)).unwrap();
        let mut raw: Value = serde_json::from_str(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
        raw["pictures"] = json!({ "frame": {
            "mat": { "color": "surface-2", "width": 8 },
            "keyline": { "color": "onSurface", "width": 2 }
        } });
        let theme = Theme::from_json(&raw.to_string()).unwrap();
        ImageNode::resolve(&serde_json::from_value(props).unwrap(), &theme, &images, rect).unwrap()
    }

    /// Where a path reaches, as `[x, y, w, h]`: the bounds of its points.
    fn reach(path: &Path) -> Rect {
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for el in &path.0 {
            let points = match *el {
                PathEl::MoveTo(p) | PathEl::LineTo(p) => vec![p],
                PathEl::QuadTo(a, b) => vec![a, b],
                PathEl::CurveTo(a, b, c) => vec![a, b, c],
                PathEl::Close => vec![],
            };
            for [x, y] in points {
                lo = [lo[0].min(x), lo[1].min(y)];
                hi = [hi[0].max(x), hi[1].max(y)];
            }
        }
        [lo[0], lo[1], hi[0] - lo[0], hi[1] - lo[1]]
    }

    /// A frame's bands as drawn: each ring's color and where its outer edge reaches.
    fn bands(ops: &[Op]) -> Vec<(Color, Rect)> {
        ops.iter()
            .filter_map(|op| match op {
                Op::Fill { path, rule: FillRule::EvenOdd, paint: Paint::Solid(c) } => Some((*c, reach(path))),
                _ => None,
            })
            .collect()
    }

    /// A picture's frame (PLAN T.3a): its theme's keyline, then its mat, drawn inside the box as
    /// rings, and the picture fitted, cropped, and focused within what they leave; the radius
    /// rounds the frame's outer edge and the picture's concentric inside it; a box too small for
    /// it narrows every band alike; and a theme that frames nothing draws as it did.
    #[test]
    fn a_picture_takes_its_themes_frame_inside_its_box() {
        let box_ = [0.0, 0.0, 100.0, 100.0];
        let n = framed(json!({ "src": "assets/photo.png" }), box_);
        // The middle square of the 2:1 picture, in what the frame leaves: 10 in from each side.
        assert_eq!(n.placement(), ([100.0, 0.0, 200.0, 200.0], [10.0, 10.0, 80.0, 80.0]));
        let ops = n.ops();
        let theme = Theme::from_json(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
        let (ink, card) = (theme.color("onSurface").unwrap(), theme.color("surface-2").unwrap());
        assert_eq!(bands(&ops), vec![(ink, [8.0, 8.0, 84.0, 84.0]), (card, box_)], "keyline, then mat");
        assert!(matches!(ops.last(), Some(Op::Image { dst: [10.0, 10.0, 80.0, 80.0], .. })), "the picture on top");

        // Its focal point and its crop, within the frame.
        let n = framed(json!({ "src": "assets/photo.png", "focal": [0, 0.5] }), box_);
        assert_eq!(n.placement(), ([0.0, 0.0, 200.0, 200.0], [10.0, 10.0, 80.0, 80.0]));
        assert_eq!(n.point([10.0, 10.0]), Some([0.0, 0.0]), "the picture's corner at the frame's inside");
        assert_eq!(n.point([5.0, 5.0]), None, "the mat is not the picture");
        let n = framed(json!({ "src": "assets/photo.png", "fit": "fill", "crop": [0.5, 0, 0.5, 1] }), box_);
        assert_eq!(n.placement(), ([200.0, 0.0, 200.0, 200.0], [10.0, 10.0, 80.0, 80.0]));

        // A contained picture's frame hugs it.
        let n = framed(json!({ "src": "assets/photo.png", "fit": "contain" }), box_);
        assert_eq!(n.placement().1, [10.0, 30.0, 80.0, 40.0]);
        assert_eq!(bands(&n.ops())[1].1, [0.0, 20.0, 100.0, 60.0]);

        // A radius rounds the frame's outer edge, the picture's 10 less inside it; a shadow falls
        // under the frame.
        let n = framed(json!({ "src": "assets/photo.png", "radius": 16, "shadow": "soft" }), box_);
        let ops = n.ops();
        let Op::Shadow { rect, radius, .. } = ops[0] else { panic!("the shadow first: {ops:?}") };
        assert_eq!((rect[2], rect[3], radius), (100.0, 100.0, 16.0));
        let Some(Op::Layer { clip: Some(clip), .. }) = ops.last() else { panic!("a rounded picture: {ops:?}") };
        assert_eq!(reach(clip), [10.0, 10.0, 80.0, 80.0]);
        assert!(clip.0.iter().any(|el| matches!(el, PathEl::CurveTo(..) | PathEl::QuadTo(..))), "rounded by 6");

        // In a box of 20, the frame takes a quarter of a side: 5, each band at half its width.
        let n = framed(json!({ "src": "assets/photo.png" }), [0.0, 0.0, 20.0, 20.0]);
        assert_eq!(n.placement().1, [5.0, 5.0, 10.0, 10.0]);
        assert_eq!(bands(&n.ops())[0].1, [4.0, 4.0, 12.0, 12.0], "a keyline of 1");

        // Dusk frames nothing: the picture fills its box.
        assert_eq!(theme.frame().unwrap(), None);
        assert!(matches!(node(json!({ "src": "assets/photo.png" }), box_).ops().as_slice(), [Op::Image { .. }]));
    }

    /// Shadows (PLAN 2.115): an image casts its shadow under what shows, a contained picture's
    /// own part, rounded as it is; under an oval, the oval's; under another mask, none.
    #[test]
    fn an_image_casts_its_shadow_under_what_shows() {
        // A 2:1 picture contained in a square: it shows across the middle, 100 × 50.
        let ops =
            node(json!({ "src": "assets/photo.png", "fit": "contain", "shadow": "soft" }), [0.0, 0.0, 100.0, 100.0])
                .ops();
        let Op::Shadow { rect, radius, .. } = ops[0] else { panic!("the shadow first: {ops:?}") };
        assert_eq!((rect, radius), ([0.0, 31.0, 100.0, 50.0], 0.0), "under the picture, fallen 6 below it");
        assert!(matches!(ops[1], Op::Image { .. }));
        let oval =
            node(json!({ "src": "assets/photo.png", "mask": "ellipse", "shadow": "soft" }), [0.0, 0.0, 80.0, 40.0]);
        assert!(
            matches!(oval.ops()[0], Op::Layer { transform: [2.0, 0.0, 0.0, 1.0, 0.0, 0.0], .. }),
            "an oval's is the oval's"
        );
        let star = node(json!({ "src": "assets/photo.png", "mask": "star", "shadow": "soft" }), [0.0, 0.0, 80.0, 80.0]);
        assert!(!star.ops().iter().any(|op| matches!(op, Op::Shadow { .. })), "a star casts none");
        // A picture that may let what is under it show, a logo on nothing, casts none: a box's
        // shadow would show through it.
        let logo = node(json!({ "src": "assets/logo.png", "shadow": "soft" }), [0.0, 0.0, 80.0, 80.0]);
        assert!(!logo.ops().iter().any(|op| matches!(op, Op::Shadow { .. })), "a clear PNG casts none");
    }

    /// Adjustments (PLAN 2.117): the display list names the picture's pixels as adjusted, and
    /// between two adjustments the one taken fades in over the one left: over a photo as it was,
    /// over a PNG's as that fades out.
    #[test]
    fn an_adjusted_image_names_its_pixels_and_fades_from_one_adjustment_to_another() {
        let asset = |op: &Op| match op {
            Op::Image { asset, .. } => asset.clone(),
            Op::Layer { ops, .. } => match ops.as_slice() {
                [Op::Image { asset, .. }] => asset.clone(),
                ops => panic!("{ops:?}"),
            },
            op => panic!("{op:?}"),
        };
        let opacity = |op: &Op| match op {
            Op::Layer { opacity, .. } => *opacity,
            _ => 1.0,
        };
        let rect = [0.0, 0.0, 100.0, 50.0];
        let plain = node(json!({ "src": "assets/photo.png" }), rect);
        let mono = node(json!({ "src": "assets/photo.png", "adjust": { "saturation": -1, "warmth": 0.25 } }), rect);
        let id = asset(&plain.ops()[0]);
        assert!(!id.contains('+'), "a picture as it is keeps its own id: {id}");
        assert_eq!(asset(&mono.ops()[0]), format!("{id}+adjust(0,0,-1000,250)"));
        assert!(plain.same_image(&mono), "the same picture, so it moves, and its adjustments fade");

        // A PNG may let what is under it show: one fades out as the other fades in.
        let dissolved = ImageNode::lerp(&plain, &mono, 0.25).ops();
        assert_eq!(dissolved.iter().map(asset).collect::<Vec<_>>(), [asset(&plain.ops()[0]), asset(&mono.ops()[0])]);
        assert_eq!(dissolved.iter().map(opacity).collect::<Vec<_>>(), [0.75, 0.25]);
        // A photo hides it: the one taken fades in over the one left, whole.
        let photo = |n: &ImageNode| ImageNode { info: ImageInfo { photo: true, ..n.info.clone() }, ..n.clone() };
        let jpeg = ImageNode::lerp(&photo(&plain), &photo(&mono), 0.25).ops();
        assert_eq!(jpeg.iter().map(opacity).collect::<Vec<_>>(), [1.0, 0.25]);
        // The same adjustment on both sides draws once.
        assert_eq!(ImageNode::lerp(&mono, &mono, 0.5).ops().len(), 1);
        // Inside a mask, both are clipped to it.
        let oval = node(json!({ "src": "assets/photo.png", "mask": "ellipse", "adjust": { "exposure": 0.5 } }), rect);
        let clipped = ImageNode::lerp(&plain.at(rect), &ImageNode { mask: oval.mask.clone(), ..plain.clone() }, 0.5);
        assert!(matches!(clipped.ops().as_slice(), [Op::Layer { clip: Some(_), .. }]));
        let both = ImageNode::lerp(&ImageNode { mask: oval.mask.clone(), ..plain.clone() }, &oval, 0.5).ops();
        assert!(matches!(both.as_slice(), [Op::Layer { clip: Some(_), ops, .. }] if ops.len() == 2), "{both:?}");

        // A value outside -1 to 1, or not a number, is said.
        let mut images = BundleImages::new();
        images.register("assets/photo.png", &png(400, 200)).unwrap();
        let theme = Theme::from_json(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
        for bad in [json!({ "exposure": 2 }), json!({ "warmth": "warm" }), json!("mono")] {
            let props = serde_json::from_value(json!({ "src": "assets/photo.png", "adjust": bad })).unwrap();
            assert!(ImageNode::resolve(&props, &theme, &images, rect).is_err(), "{bad}");
        }
    }

    /// Borders (PLAN 2.118): an image's stroke goes around what shows, on top of the picture, as a
    /// shape's outline is stroked: the edge of a contained picture, or an oval; and between two
    /// states a border that only one draws fades.
    #[test]
    fn an_images_border_goes_around_what_shows() {
        let rect = [0.0, 0.0, 100.0, 100.0];
        let stroke = json!({ "paint": "accent", "width": 4 });
        let framed = node(json!({ "src": "assets/photo.png", "fit": "contain", "stroke": stroke }), rect);
        let ops = framed.ops();
        assert!(matches!(ops[0], Op::Image { .. }), "the picture first: {ops:?}");
        let Some(Op::Layer { transform, opacity, ops: inner, clip: None, .. }) = ops.last() else { panic!("{ops:?}") };
        assert_eq!(*transform, [1.0, 0.0, 0.0, 1.0, 0.0, 25.0], "around the 2:1 picture contained across the middle");
        assert_eq!(*opacity, 1.0);
        let [Op::Stroke { width, path, .. }] = inner.as_slice() else { panic!("{inner:?}") };
        assert_eq!(*width, 4.0);
        assert!(!path.0.iter().any(|el| matches!(el, PathEl::CurveTo(..))), "a square-cornered edge");

        // An oval's border is the oval, outside the oval's clip so none of it is cut.
        let oval = node(json!({ "src": "assets/photo.png", "mask": "ellipse", "stroke": stroke }), rect).ops();
        let [Op::Layer { clip: Some(_), .. }, Op::Layer { clip: None, ops: inner, .. }] = oval.as_slice() else {
            panic!("{oval:?}")
        };
        let [Op::Stroke { path, .. }] = inner.as_slice() else { panic!("{inner:?}") };
        assert!(path.0.iter().any(|el| matches!(el, PathEl::CurveTo(..))), "the oval's curve");

        // Drawn by one state and not the next, it fades.
        let plain = node(json!({ "src": "assets/photo.png", "fit": "contain" }), rect);
        let shown = |ops: Vec<Op>| match ops.last() {
            Some(Op::Layer { opacity, clip: None, .. }) => *opacity,
            _ => panic!("{ops:?}"),
        };
        assert_eq!(shown(ImageNode::lerp(&plain, &framed, 0.25).ops()), 0.25);
        assert_eq!(shown(ImageNode::lerp(&framed, &plain, 0.25).ops()), 0.75);
        assert_eq!(ImageNode::lerp(&plain, &plain, 0.5).ops().len(), 1, "none where neither draws one");

        // Drawn by both, it moves from one to the other: its width, and its paint in Oklab.
        let heavy = json!({ "paint": "ink", "width": 12 });
        let other = node(json!({ "src": "assets/photo.png", "fit": "contain", "stroke": heavy }), rect);
        let stroked = |ops: Vec<Op>| match ops.last() {
            Some(Op::Layer { opacity, clip: None, ops, .. }) => match ops.as_slice() {
                [Op::Stroke { width, paint: Paint::Solid(c), .. }] => (*opacity, *width, *c),
                _ => panic!("{ops:?}"),
            },
            _ => panic!("{ops:?}"),
        };
        let (_, _, from) = stroked(framed.ops());
        let (_, _, to) = stroked(other.ops());
        let (opacity, width, between) = stroked(ImageNode::lerp(&framed, &other, 0.5).ops());
        assert_eq!((opacity, width), (1.0, 8.0), "whole, halfway from 4 to 12");
        assert!(between != from && between != to, "a color between {from:?} and {to:?}: {between:?}");
    }

    #[test]
    fn the_header_gives_the_size_and_the_bytes_the_id() {
        let info = ImageInfo::read(&png(400, 200)).unwrap();
        assert_eq!((info.width, info.height), (400, 200));
        assert!(info.id.starts_with("sha256:") && info.id.len() == 7 + 64);
        // A JPEG's size is as it is seen: this one is stored 67 × 45 and turned a quarter.
        let photo = include_bytes!("../../../tests/fixtures/jpeg/orientation-6.jpg");
        let info = ImageInfo::read(photo).unwrap();
        assert_eq!((info.width, info.height), (45, 67));
        assert!(ImageInfo::read(&[0xFF, 0xD8, 0xFF, 0xE0]).unwrap_err().contains("JPEG"));
        let cmyk = include_bytes!("../../../tests/fixtures/jpeg/cmyk.jpg");
        assert_eq!(ImageInfo::read(cmyk).map(|i| (i.width, i.height)), Ok((67, 45)), "a CMYK photo is read");
        assert!(ImageInfo::read(b"GIF89a").unwrap_err().contains("neither a PNG nor a JPEG"));
        assert!(ImageInfo::read(&png(8192, 1)).is_ok());
        assert!(ImageInfo::read(&png(8193, 1)).unwrap_err().contains("at most 8192 px a side"));
        assert!(ImageInfo::read(&png(0, 1)).unwrap_err().contains("no pixels"));
    }

    /// A picture of any size says how large it is, as it is seen, and whether an image shows it as
    /// it is: what an editor offers a smaller copy of.
    #[test]
    fn a_picture_says_its_size_whatever_it_is() {
        let fits = Picture::read(&png(8192, 6144)).unwrap();
        assert_eq!(fits, Picture { width: 8192, height: 6144, largest: 8192 });
        assert!(fits.fits());
        let large = Picture::read(&png(11656, 8742)).unwrap();
        assert_eq!((large.width, large.height), (11656, 8742));
        assert!(!large.fits());
        // A JPEG too large to decode, as it is seen: stored 9000 × 45, turned a quarter.
        let mut turned = include_bytes!("../../../tests/fixtures/jpeg/orientation-6.jpg").to_vec();
        let sof = turned.windows(2).position(|w| w == [0xFF, 0xC0]).unwrap();
        turned[sof + 7..sof + 9].copy_from_slice(&9000u16.to_be_bytes());
        let seen = Picture::read(&turned).unwrap();
        assert_eq!((seen.width, seen.height, seen.fits()), (45, 9000, false));
        assert!(ImageInfo::read(&turned).unwrap_err().contains("45 × 9000 px"));
        assert!(Picture::read(b"GIF89a").unwrap_err().contains("neither a PNG nor a JPEG"));
    }

    /// The point under the pointer is the point of the image drawn there, in fractions of
    /// its crop, whatever the fit: what a focal point picked there names (PLAN 2.45).
    #[test]
    fn the_point_under_the_pointer_is_the_images_own() {
        // A 400 × 200 image covering a square box shows its middle half: the box's left edge is
        // the image's quarter, its middle the image's middle, its right edge three quarters.
        let cover = node(json!({ "src": "assets/photo.png" }), [100.0, 0.0, 100.0, 100.0]);
        assert_eq!(cover.point([100.0, 50.0]), Some([0.25, 0.5]));
        assert_eq!(cover.point([150.0, 0.0]), Some([0.5, 0.0]));
        assert_eq!(cover.point([200.0, 100.0]), Some([0.75, 1.0]));
        assert_eq!(cover.point([99.0, 50.0]), None, "off the box");
        // Picked there, the point lines up with the same point of the box: in view.
        let picked = node(json!({ "src": "assets/photo.png", "focal": [0.25, 0.5] }), [100.0, 0.0, 100.0, 100.0]);
        assert_eq!(picked.point([125.0, 50.0]), Some([0.25, 0.5]));
        // Contained, it shows whole, in a band across the box; above and below it, nothing.
        let contain = node(json!({ "src": "assets/photo.png", "fit": "contain" }), [0.0, 0.0, 100.0, 100.0]);
        assert_eq!(contain.point([50.0, 50.0]), Some([0.5, 0.5]));
        assert_eq!(contain.point([0.0, 25.0]), Some([0.0, 0.0]));
        assert_eq!(contain.point([50.0, 10.0]), None);
        // A crop is what the fractions are of.
        let cropped = node(
            json!({ "src": "assets/photo.png", "fit": "fill", "crop": [0.5, 0, 0.5, 1] }),
            [0.0, 0.0, 100.0, 100.0],
        );
        assert_eq!(cropped.point([50.0, 50.0]), Some([0.5, 0.5]));
        assert_eq!(cropped.point([100.0, 100.0]), Some([1.0, 1.0]));
    }

    #[test]
    fn cover_fills_the_box_and_crops_around_the_focal_point() {
        // A 2:1 image in a square box: the middle square shows.
        let n = node(json!({ "src": "assets/photo.png" }), [0.0, 0.0, 100.0, 100.0]);
        assert_eq!(n.placement(), ([100.0, 0.0, 200.0, 200.0], [0.0, 0.0, 100.0, 100.0]));
        // Focal at the left edge: the left square.
        let n = node(json!({ "src": "assets/photo.png", "focal": [0, 0.5] }), [0.0, 0.0, 100.0, 100.0]);
        assert_eq!(n.placement().0, [0.0, 0.0, 200.0, 200.0]);
    }

    #[test]
    fn contain_shows_it_all_and_fill_stretches() {
        let n = node(json!({ "src": "assets/photo.png", "fit": "contain" }), [0.0, 0.0, 100.0, 100.0]);
        assert_eq!(n.placement(), ([0.0, 0.0, 400.0, 200.0], [0.0, 25.0, 100.0, 50.0]));
        let n =
            node(json!({ "src": "assets/photo.png", "fit": "fill", "crop": [0.5, 0, 0.5, 1] }), [0.0, 0.0, 10.0, 10.0]);
        assert_eq!(n.placement(), ([200.0, 0.0, 200.0, 200.0], [0.0, 0.0, 10.0, 10.0]));
    }

    #[test]
    fn a_radius_clips_what_shows() {
        let n = node(json!({ "src": "assets/photo.png", "radius": 8 }), [0.0, 0.0, 100.0, 100.0]);
        let ops = n.ops();
        let [Op::Layer { clip: Some(_), ops: inner, .. }] = ops.as_slice() else { panic!("{ops:?}") };
        assert!(matches!(inner.as_slice(), [Op::Image { .. }]));
        assert!(matches!(
            node(json!({ "src": "assets/photo.png" }), [0.0, 0.0, 9.0, 9.0]).ops().as_slice(),
            [Op::Image { .. }]
        ));
    }

    /// A mask shows the image only inside its shape (PLAN 2.113), fitted to what shows, in place of
    /// the radius: a star's ten points, an oval's curves; a name the library lacks says what it takes.
    #[test]
    fn a_mask_shows_the_image_inside_its_shape() {
        let clip = |props: Value, rect: Rect| match node(props, rect).ops().as_slice() {
            [Op::Layer { clip: Some(clip), ops, .. }] if matches!(ops.as_slice(), [Op::Image { .. }]) => clip.clone(),
            other => panic!("{other:?}"),
        };
        let points = |el: &PathEl| match *el {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => vec![p],
            PathEl::QuadTo(a, b) => vec![a, b],
            PathEl::CurveTo(a, b, c) => vec![a, b, c],
            PathEl::Close => vec![],
        };
        let star = clip(json!({ "src": "assets/photo.png", "mask": "star", "radius": 8 }), [0.0, 0.0, 100.0, 50.0]);
        let corners: Vec<[f32; 2]> = star.0.iter().flat_map(points).collect();
        assert_eq!(corners.len(), 10, "{star:?}");
        assert!(corners.iter().all(|[x, y]| (0.0..=100.0).contains(x) && (0.0..=50.0).contains(y)));
        assert_eq!(corners[0], [50.0, 0.0], "its first point at the top middle of what shows");
        // Contained, the oval rounds the part the image fills, not the box.
        let oval =
            clip(json!({ "src": "assets/photo.png", "mask": "ellipse", "fit": "contain" }), [0.0, 0.0, 100.0, 100.0]);
        assert!(oval.0.iter().any(|el| matches!(el, PathEl::CurveTo(..) | PathEl::QuadTo(..))), "{oval:?}");
        let ys: Vec<f32> = oval.0.iter().flat_map(points).map(|[_, y]| y).collect();
        let (top, bottom) = ys.iter().fold((f32::MAX, f32::MIN), |(lo, hi), y| (lo.min(*y), hi.max(*y)));
        assert!((top - 25.0).abs() < 0.01 && (bottom - 75.0).abs() < 0.01, "a 2:1 picture in a square: {top} {bottom}");
        let mut images = BundleImages::new();
        images.register("assets/photo.png", &png(400, 200)).unwrap();
        let theme = Theme::from_json(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
        let props = serde_json::from_value(json!({ "src": "assets/photo.png", "mask": "blob" })).unwrap();
        let err = ImageNode::resolve(&props, &theme, &images, [0.0, 0.0, 9.0, 9.0]).unwrap_err().to_string();
        assert!(err.contains("`ellipse`, or a shape of the library"), "{err}");
    }
}
