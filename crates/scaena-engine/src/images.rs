//! Image nodes (SPEC §3.3): a bundle's PNGs, placed in their box.
//!
//! The engine needs only each image's size and its content id; painters decode the
//! pixels (`scaena_paint::Resources`). `fit` says how the image meets its box: `cover`
//! (the default) fills the box and crops the image, `contain` shows the whole image
//! inside the box, and `fill` stretches it. `focal` is a point of the image, in fractions
//! (`[0.5, 0.5]` by default): the box and the image line up at it, as CSS
//! `object-position` does with percentages, so `cover` keeps it in view. `crop` cuts the
//! image to `[x, y, w, h]`, fractions of it, before anything else, so a sharper file of
//! the same picture keeps the crop. `radius` rounds the corners of what shows.
//!
//! Images are PNG in v1: one pure-Rust decoder, the same pixels everywhere (SPEC §13).

use crate::EngineError;
use crate::charts::{RoundRect, lerp};
use crate::theme::Theme;
use scaena_core::displaylist::{Blend, IDENTITY, MAX_IMAGE_SIDE, Op, Quality, Rect};
use scaena_core::document::Props;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// What the engine knows of an image: its content id, as display lists name it, and its
/// size in pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageInfo {
    /// `sha256:<hex>` of the file's bytes.
    pub id: String,
    pub width: u32,
    pub height: u32,
}

impl ImageInfo {
    /// A PNG's id and size, from its header; the pixels are the painters' to decode.
    pub fn read(bytes: &[u8]) -> Result<ImageInfo, String> {
        const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
        if !bytes.starts_with(SIGNATURE) {
            let what = if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) { "a JPEG" } else { "not a PNG" };
            return Err(format!("{what}; images are PNG in v1 (SPEC §3.3)"));
        }
        // The first chunk is IHDR: length, type, then width and height, big-endian.
        let ihdr = bytes.get(8..24).filter(|h| &h[4..8] == b"IHDR").ok_or("a PNG with no IHDR chunk")?;
        let be = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        let (width, height) = (be(&ihdr[8..12]), be(&ihdr[12..16]));
        if width == 0 || height == 0 {
            return Err("a PNG with no pixels".into());
        }
        if width.max(height) > MAX_IMAGE_SIDE {
            return Err(format!("{width} × {height} px; images are at most {MAX_IMAGE_SIDE} px a side (SPEC §3.3)"));
        }
        let hex: String = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
        Ok(ImageInfo { id: format!("sha256:{hex}"), width, height })
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
        Ok(ImageNode { rect, info, fit, focal, crop, radius })
    }

    /// The same image in another box.
    pub fn at(&self, rect: Rect) -> ImageNode {
        ImageNode { rect, ..self.clone() }
    }

    /// Whether `other` shows the same picture the same way, perhaps in another box.
    pub fn same_image(&self, other: &ImageNode) -> bool {
        (&self.info, self.fit, self.focal, self.crop) == (&other.info, other.fit, other.focal, other.crop)
    }

    /// `a` to `b`, `p` of the way: the box and the radius move.
    pub fn lerp(a: &ImageNode, b: &ImageNode, p: f32) -> ImageNode {
        let rect = [0, 1, 2, 3].map(|k| lerp(a.rect[k], b.rect[k], p));
        ImageNode { rect, radius: lerp(a.radius, b.radius, p).max(0.0), ..b.clone() }
    }

    /// The pixels of the image that show, and where in the box, box-local: `(src, dst)`.
    pub fn placement(&self) -> (Rect, Rect) {
        let [_, _, w, h] = self.rect;
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

    /// What the image draws, box-local: the image, clipped to rounded corners if it has
    /// them.
    pub fn ops(&self) -> Vec<Op> {
        let (src, dst) = self.placement();
        let image = Op::Image { asset: self.info.id.clone(), src, dst, quality: Quality::High };
        if self.radius <= 0.0 {
            return vec![image];
        }
        let [x, y, w, h] = dst;
        let r = self.radius.min(w.min(h) / 2.0);
        let clip = RoundRect { x, y, w, h, top_radius: r, bottom_radius: r }.path();
        vec![Op::Layer {
            node: None,
            cell: None,
            transform: IDENTITY,
            opacity: 1.0,
            blend: Blend::Normal,
            clip: Some(clip),
            ops: vec![image],
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A PNG header for an image of `w` × `h`: all the engine reads.
    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        b.extend(w.to_be_bytes());
        b.extend(h.to_be_bytes());
        b.extend([8, 6, 0, 0, 0]);
        b
    }

    fn node(props: Value, rect: Rect) -> ImageNode {
        let mut images = BundleImages::new();
        images.register("assets/photo.png", &png(400, 200)).unwrap();
        let theme = Theme::from_json(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
        ImageNode::resolve(&serde_json::from_value(props).unwrap(), &theme, &images, rect).unwrap()
    }

    #[test]
    fn the_header_gives_the_size_and_the_bytes_the_id() {
        let info = ImageInfo::read(&png(400, 200)).unwrap();
        assert_eq!((info.width, info.height), (400, 200));
        assert!(info.id.starts_with("sha256:") && info.id.len() == 7 + 64);
        assert!(ImageInfo::read(&[0xFF, 0xD8, 0xFF, 0xE0]).unwrap_err().contains("JPEG"));
        assert!(ImageInfo::read(&png(8192, 1)).is_ok());
        assert!(ImageInfo::read(&png(8193, 1)).unwrap_err().contains("at most 8192 px a side"));
        assert!(ImageInfo::read(&png(0, 1)).unwrap_err().contains("no pixels"));
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
}
