//! # scaena-paint
//!
//! Painters consume a [`scaena_core::displaylist::DisplayList`] and produce pixels.
//! They never shape text or lay anything out (SPEC §6).
//!
//! - `cpu` (default feature): `vello_cpu`: headless, deterministic; CI, agents, export.
//! - `gpu`: `vello` on `wgpu`: WebGPU in the browser, Metal on the Mac, Vulkan on Linux.
//!
//! A painter draws what the engine emits: fills and strokes in a color or a gradient,
//! glyph runs (outline, COLR, and bitmap glyphs), layers, PNG images, and shaders of every
//! kind. Both painters take their geometry and paints from the same conversions
//! (`convert`), so they can differ only in rasterization.
//!
//! Images are decoded once, when they are added to [`Assets`], to straight-alpha RGBA8;
//! the CPU painter premultiplies them, vello takes them as they are. Both filter them
//! bilinearly (SPEC §3.3).
//!
//! A shader op draws as an image of its device pixels: the CPU painter computes it
//! with the kind's CPU reference, the GPU painter with its WGSL twin
//! (`scaena_core::shader`), and either places it pixel for pixel ([`shader_jobs`]).

pub mod diff;

use peniko::{Blob, FontData};
use scaena_core::displaylist::{DL_VERSION, DisplayList, FontRef};
use scaena_core::shader::{Job, ShaderError, Spec};
use std::collections::BTreeMap;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PaintError {
    #[error("not implemented yet: {0} (see docs/PLAN.md)")]
    NotImplemented(&'static str),
    #[error("unsupported display list version {0}")]
    Version(u32),
    #[error("display list names font {0}, which the painter was not given")]
    MissingFont(String),
    #[error("display list names font index {0}, past the end of its font table")]
    FontIndex(u32),
    #[error("display list names image {0}, which the painter was not given")]
    MissingImage(String),
    #[error("{0}×{1} px is not a raster this painter can make")]
    Size(f32, f32),
    #[error("png: {0}")]
    Png(String),
    #[error("jpeg: {0}")]
    Jpeg(String),
    #[error("cannot compare a {}×{} raster with a {}×{} one", a.0, a.1, b.0, b.1)]
    Mismatch { a: (u32, u32), b: (u32, u32) },
    #[error("gpu: {0}")]
    Gpu(String),
    #[error("shader: {0}")]
    Shader(ShaderError),
    /// Pixels handed to a painter for a frame's shaders that are not theirs.
    #[error("shader pixels: {0}")]
    Shaded(String),
}

impl From<ShaderError> for PaintError {
    fn from(e: ShaderError) -> Self {
        PaintError::Shader(e)
    }
}

/// What display lists name besides themselves, loaded once per bundle and shared by every
/// frame (SPEC §6): font bytes by bundle id, and images, decoded, by content id.
#[derive(Debug, Clone, Default)]
pub struct Assets {
    blobs: BTreeMap<String, Blob<u8>>,
    images: BTreeMap<String, Arc<Picture>>,
}

/// A decoded image: straight (unpremultiplied) sRGB RGBA8, row-major, as it is seen.
#[derive(Debug)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Blob<u8>,
    /// A JPEG's own file, for an export that carries the file and not its pixels (a PDF).
    pub jpeg: Option<Jpeg>,
    /// Premultiplied, as `vello_cpu` paints it; made on first use.
    #[cfg(feature = "cpu")]
    pixmap: std::sync::OnceLock<Arc<vello_cpu::Pixmap>>,
}

/// A JPEG as its file holds it (ADR-0017): its bytes, its size as stored, and the EXIF
/// orientation that turns it to be seen.
#[derive(Debug, Clone)]
pub struct Jpeg {
    pub bytes: Blob<u8>,
    pub width: u32,
    pub height: u32,
    pub orientation: u8,
}

impl Assets {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add the bytes of the font file with bundle id `id` (its path in the bundle).
    pub fn insert_font(&mut self, id: &str, bytes: Vec<u8>) {
        self.blobs.insert(id.to_string(), Blob::new(Arc::new(bytes)));
    }

    /// Decode the PNG or JPEG `bytes` and keep it under `id`, the content id display lists
    /// name it by (`sha256:…`, SPEC §3.3).
    pub fn insert_image(&mut self, id: &str, bytes: &[u8]) -> Result<(), PaintError> {
        self.images.insert(id.to_string(), Arc::new(Picture::decode(bytes)?));
        Ok(())
    }

    /// The font a display list names: its bytes and its face index.
    pub fn font_data(&self, font: &FontRef) -> Result<FontData, PaintError> {
        let blob = self.blobs.get(&font.id).ok_or_else(|| PaintError::MissingFont(font.id.clone()))?;
        Ok(FontData::new(blob.clone(), font.index))
    }

    /// The decoded image a display list names by content id.
    pub fn image(&self, id: &str) -> Result<&Arc<Picture>, PaintError> {
        self.images.get(id).ok_or_else(|| PaintError::MissingImage(id.to_string()))
    }
}

impl Picture {
    /// A PNG or a JPEG, as 8-bit RGBA. A PNG is expanded: palettes and gray to color, 16 bits
    /// to 8, a missing alpha to opaque. A JPEG is decoded by `scaena_core::jpeg`, in integers,
    /// the same pixels on every target, and turned by its EXIF orientation (ADR-0017). Color
    /// profiles are not applied: pixels are sRGB.
    pub fn decode(bytes: &[u8]) -> Result<Picture, PaintError> {
        if scaena_core::jpeg::is_jpeg(bytes) {
            return Picture::jpeg(bytes);
        }
        let bad = |e: png::DecodingError| PaintError::Png(e.to_string());
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().map_err(bad)?;
        let (width, height) = reader.info().size();
        if width.max(height) > scaena_core::displaylist::MAX_IMAGE_SIDE {
            return Err(PaintError::Png(format!("{width} × {height} px: larger than an image may be (SPEC §3.3)")));
        }
        let size = reader.output_buffer_size().ok_or_else(|| PaintError::Png("image too large".into()))?;
        let mut buf = vec![0; size];
        let info = reader.next_frame(&mut buf).map_err(bad)?;
        let buf = &buf[..info.buffer_size()];
        let rgba: Vec<u8> = match info.color_type {
            png::ColorType::Rgba => buf.to_vec(),
            png::ColorType::Rgb => buf.as_chunks::<3>().0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect(),
            png::ColorType::GrayscaleAlpha => buf.as_chunks::<2>().0.iter().flat_map(|&[g, a]| [g, g, g, a]).collect(),
            png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            png::ColorType::Indexed => return Err(PaintError::Png("an indexed PNG the decoder did not expand".into())),
        };
        Ok(Picture {
            width: info.width,
            height: info.height,
            rgba: Blob::new(Arc::new(rgba)),
            jpeg: None,
            #[cfg(feature = "cpu")]
            pixmap: std::sync::OnceLock::new(),
        })
    }

    fn jpeg(bytes: &[u8]) -> Result<Picture, PaintError> {
        let bad = |e: scaena_core::jpeg::JpegError| PaintError::Jpeg(e.to_string());
        let header = scaena_core::jpeg::Header::read(bytes).map_err(bad)?;
        let seen = scaena_core::jpeg::decode(bytes).map_err(bad)?;
        Ok(Picture {
            width: seen.width,
            height: seen.height,
            rgba: Blob::new(Arc::new(seen.rgba)),
            jpeg: Some(Jpeg {
                bytes: Blob::new(Arc::new(bytes.to_vec())),
                width: header.width,
                height: header.height,
                orientation: header.orientation,
            }),
            #[cfg(feature = "cpu")]
            pixmap: std::sync::OnceLock::new(),
        })
    }

    /// Premultiplied, for `vello_cpu`, which knows whether it is opaque: an opaque image is
    /// drawn without blending, and hides what is under it (the same pixels, sooner).
    #[cfg(feature = "cpu")]
    fn pixmap(&self) -> Arc<vello_cpu::Pixmap> {
        self.pixmap
            .get_or_init(|| {
                let premultiplied = |c: u8, a: u8| ((u16::from(c) * u16::from(a) + 127) / 255) as u8;
                let pixels = self.rgba.data().as_chunks::<4>().0.iter().map(|&[r, g, b, a]| {
                    vello_cpu::peniko::color::PremulRgba8 {
                        r: premultiplied(r, a),
                        g: premultiplied(g, a),
                        b: premultiplied(b, a),
                        a,
                    }
                });
                let opaque = self.rgba.data().as_chunks::<4>().0.iter().all(|[.., a]| *a == 255);
                let (w, h) = (self.width as u16, self.height as u16);
                Arc::new(vello_cpu::Pixmap::from_parts_with_opacity(pixels.collect(), w, h, !opaque))
            })
            .clone()
    }
}

#[cfg(test)]
mod picture_tests {
    use super::*;

    /// `pixels`, packed rows at `depth` bits a sample, as a `w` px wide PNG of `color` type.
    fn png(color: png::ColorType, depth: png::BitDepth, w: u32, pixels: &[u8], palette: Option<&[u8]>) -> Vec<u8> {
        let mut out = Vec::new();
        let row = (w as usize * color.samples() * depth as usize).div_ceil(8);
        let mut e = png::Encoder::new(&mut out, w, (pixels.len() / row) as u32);
        e.set_color(color);
        e.set_depth(depth);
        if let Some(p) = palette {
            e.set_palette(p.to_vec());
        }
        e.write_header().unwrap().write_image_data(pixels).unwrap();
        out
    }

    #[test]
    fn every_png_color_type_decodes_to_straight_rgba8() {
        use png::{BitDepth::*, ColorType::*};
        let cases: [(&str, Vec<u8>); 6] = [
            ("rgba", png(Rgba, Eight, 1, &[10, 20, 30, 40], None)),
            ("rgb", png(Rgb, Eight, 1, &[10, 20, 30], None)),
            ("gray", png(Grayscale, Eight, 1, &[77], None)),
            ("gray+alpha", png(GrayscaleAlpha, Eight, 1, &[77, 40], None)),
            ("indexed", png(Indexed, Eight, 1, &[1], Some(&[0, 0, 0, 10, 20, 30]))),
            ("16-bit", png(Rgba, Sixteen, 1, &[10, 99, 20, 99, 30, 99, 40, 99], None)),
        ];
        let expect = |name: &str| match name {
            "rgba" | "16-bit" => [10, 20, 30, 40],
            "rgb" | "indexed" => [10, 20, 30, 255],
            "gray" => [77, 77, 77, 255],
            _ => [77, 77, 77, 40],
        };
        for (name, bytes) in cases {
            let p = Picture::decode(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!((p.width, p.height), (1, 1), "{name}");
            assert_eq!(p.rgba.data(), expect(name), "{name}");
        }
    }

    /// A JPEG is decoded as it is seen (ADR-0017): turned by its EXIF orientation, its file
    /// kept for a PDF to carry; one this decoder cannot read is an error that says why.
    #[test]
    fn a_jpeg_decodes_as_it_is_seen() {
        let fixture = |name: &str| std::fs::read(format!("../../tests/fixtures/jpeg/{name}")).unwrap();
        let stored = Picture::decode(&fixture("baseline-420.jpg")).unwrap();
        let turned = Picture::decode(&fixture("orientation-6.jpg")).unwrap();
        assert_eq!((stored.width, stored.height), (67, 45));
        assert_eq!((turned.width, turned.height), (45, 67));
        let jpeg = turned.jpeg.as_ref().unwrap();
        assert_eq!((jpeg.width, jpeg.height, jpeg.orientation), (67, 45, 6));
        assert!(stored.jpeg.as_ref().is_some_and(|j| j.orientation == 1));
        // Turned a quarter clockwise, the stored picture's top-left corner, a yellow square, is
        // seen at the top right.
        let at = |p: &Picture, x: u32, y: u32| {
            let i = ((y * p.width + x) * 4) as usize;
            p.rgba.data()[i..i + 4].to_vec()
        };
        let yellow = |px: Vec<u8>| px[0] > 200 && px[1] > 200 && px[2] < 90;
        assert!(yellow(at(&stored, 1, 1)), "{:?}", at(&stored, 1, 1));
        assert!(yellow(at(&turned, 43, 1)), "{:?}", at(&turned, 43, 1));
        let Err(PaintError::Jpeg(e)) = Picture::decode(&fixture("cmyk.jpg")) else { panic!("decoded a CMYK JPEG") };
        assert!(e.contains("CMYK"), "{e}");
    }

    #[test]
    fn a_picture_too_large_for_the_atlas_is_refused() {
        let wide = png(png::ColorType::Grayscale, png::BitDepth::One, 8200, &[0; 1025], None);
        let Err(PaintError::Png(e)) = Picture::decode(&wide) else { panic!("decoded an 8200 px image") };
        assert!(e.contains("8200 × 1 px"), "{e}");
        assert!(matches!(Picture::decode(b"GIF89a"), Err(PaintError::Png(_))));
    }
}

/// Straight (unpremultiplied) sRGB RGBA8, row-major: what PNG files and the diff use.
#[derive(Debug, Clone, PartialEq)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Raster {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }

    /// PNG bytes, compressed for keeping (goldens). The same pixels always encode to
    /// the same bytes.
    pub fn to_png(&self) -> Result<Vec<u8>, PaintError> {
        self.encode(png::Compression::Balanced)
    }

    /// PNG bytes, compressed for speed: what a render hands back. Shader grain is noise
    /// that deflate cannot shrink, and balanced compression spends a second on a 1080p
    /// mesh that this writes in 30 ms; files come out 1.5–2.5× larger.
    pub fn to_png_fast(&self) -> Result<Vec<u8>, PaintError> {
        self.encode(png::Compression::Fast)
    }

    fn encode(&self, compression: png::Compression) -> Result<Vec<u8>, PaintError> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(compression);
        let png_error = |e: png::EncodingError| PaintError::Png(e.to_string());
        let mut writer = encoder.write_header().map_err(png_error)?;
        writer.write_image_data(&self.rgba).map_err(png_error)?;
        writer.finish().map_err(png_error)?;
        Ok(out)
    }

    /// Decode an 8-bit RGBA PNG (what [`Raster::to_png`] writes), or an 8-bit RGB one as
    /// opaque (what a browser's screenshot is).
    pub fn from_png(bytes: &[u8]) -> Result<Raster, PaintError> {
        let png_error = |e: png::DecodingError| PaintError::Png(e.to_string());
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().map_err(png_error)?;
        let info = reader.info();
        let opaque = match (info.color_type, info.bit_depth) {
            (png::ColorType::Rgba, png::BitDepth::Eight) => false,
            (png::ColorType::Rgb, png::BitDepth::Eight) => true,
            (color, depth) => {
                return Err(PaintError::Png(format!("expected 8-bit RGBA or RGB, got {color:?} {depth:?}")));
            }
        };
        let (width, height) = (info.width, info.height);
        let mut pixels = vec![0; reader.output_buffer_size().unwrap_or(0)];
        reader.next_frame(&mut pixels).map_err(png_error)?;
        let rgba = match opaque {
            true => pixels.as_chunks::<3>().0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect(),
            false => pixels,
        };
        Ok(Raster { width, height, rgba })
    }
}

pub trait Painter {
    fn name(&self) -> &'static str;
    /// Paint `dl` at `scale` output pixels per canvas unit.
    fn paint(&mut self, dl: &DisplayList, fonts: &Assets, scale: f32) -> Result<Raster, PaintError>;
}

/// A painter and the assets it draws from, lent to lint for what only pixels say: the
/// background text sits on (E110, E111; `scaena_core::lint::Backdrop`).
pub struct Backdrop<'a, P: Painter> {
    pub painter: P,
    pub assets: &'a Assets,
}

impl<P: Painter> scaena_core::lint::Backdrop for Backdrop<'_, P> {
    fn paint(&mut self, dl: &DisplayList, scale: f32) -> Result<scaena_core::lint::Pixels, String> {
        let raster = self.painter.paint(dl, self.assets, scale).map_err(|e| e.to_string())?;
        Ok(scaena_core::lint::Pixels { width: raster.width, height: raster.height, rgba: raster.rgba })
    }
}

/// The most pixels a raster holds: 2^25, an 8K frame (7680 × 4320) and a little more. A
/// painter keeps its raster in memory, and beside it an image of each shader the frame
/// shows, as large as the frame for a background; in the browser an allocation that fails
/// stops the worker. A larger raster is an error (SPEC §7.1).
pub const MAX_PIXELS: u64 = 1 << 25;

/// Output size in whole pixels for `dl` at `scale`.
fn raster_size(dl: &DisplayList, scale: f32) -> Result<(u16, u16), PaintError> {
    let (w, h) = ((dl.viewport[0] * scale).round(), (dl.viewport[1] * scale).round());
    if !(1.0..=f32::from(u16::MAX)).contains(&w) || !(1.0..=f32::from(u16::MAX)).contains(&h) {
        return Err(PaintError::Size(w, h));
    }
    Ok((w as u16, h as u16))
}

/// [`raster_size`] for a painter that keeps the raster: at most [`MAX_PIXELS`]. The PDF
/// painter sizes its shaders' images by the raster it would make, and keeps none.
fn raster(dl: &DisplayList, scale: f32) -> Result<(u16, u16), PaintError> {
    let (w, h) = raster_size(dl, scale)?;
    match u64::from(w) * u64::from(h) <= MAX_PIXELS {
        true => Ok((w, h)),
        false => Err(PaintError::Size(f32::from(w), f32::from(h))),
    }
}

fn check_version(dl: &DisplayList) -> Result<(), PaintError> {
    if dl.dl == DL_VERSION { Ok(()) } else { Err(PaintError::Version(dl.dl)) }
}

/// Every shader op in `dl`, made ready to draw at `scale` (`None` for one that covers
/// no pixel), in the order a painter meets them: depth first, in paint order. A
/// painter runs each job before or as it walks the ops, then draws its pixels where
/// [`Job::bbox`] says.
#[cfg(any(feature = "cpu", feature = "gpu"))]
pub fn shader_jobs(dl: &DisplayList, scale: f32) -> Result<Vec<Option<Job>>, PaintError> {
    shader_ops(dl, scale)?.into_iter().map(|(op, device, size)| Ok(Job::new(op, device, size)?)).collect()
}

/// [`shader_jobs`]' jobs as what each is made from: its op, the transform to device pixels,
/// and the raster's size, in the same order. Another worker makes the same job from one
/// ([`Spec::job`]): the browser's helpers do (PLAN 2.28).
#[cfg(any(feature = "cpu", feature = "gpu"))]
pub fn shader_specs(dl: &DisplayList, scale: f32) -> Result<Vec<Spec>, PaintError> {
    let specs = shader_ops(dl, scale)?.into_iter();
    Ok(specs.map(|(op, device, size)| Spec { op: op.clone(), device, size }).collect())
}

/// Every shader op in `dl` at `scale`, as [`shader_jobs`] meets them, with the transform from
/// canvas units to device pixels it is drawn through and the raster's size.
#[cfg(any(feature = "cpu", feature = "gpu"))]
#[allow(clippy::type_complexity)]
fn shader_ops(
    dl: &DisplayList,
    scale: f32,
) -> Result<Vec<(&scaena_core::displaylist::Op, [f64; 6], [u32; 2])>, PaintError> {
    use scaena_core::displaylist::Op;
    fn walk<'a>(ops: &'a [Op], xf: kurbo::Affine, size: [u32; 2], out: &mut Vec<(&'a Op, [f64; 6], [u32; 2])>) {
        for op in ops {
            match op {
                Op::Layer { transform, ops, .. } => walk(ops, xf * convert::affine(transform), size, out),
                Op::Shader { .. } => out.push((op, xf.as_coeffs(), size)),
                _ => {}
            }
        }
    }
    let (w, h) = raster_size(dl, scale)?;
    let mut out = Vec::new();
    walk(&dl.ops, kurbo::Affine::scale(f64::from(scale)), [u32::from(w), u32::from(h)], &mut out);
    Ok(out)
}

/// A gradient's stops as both painters draw them (SPEC §6): in sRGB, with stops added
/// between the given ones so that blending them in sRGB follows their blend in Oklab. For
/// a painter that is not built on peniko, such as the PDF one.
#[cfg(any(feature = "cpu", feature = "gpu"))]
pub fn srgb_stops(stops: &[scaena_core::displaylist::Stop]) -> Vec<scaena_core::displaylist::Stop> {
    convert::srgb_stops(stops)
}

/// Display-list types to `kurbo`/`peniko`, shared by both painters so they receive
/// identical geometry (ADR-0004: one `kurbo`, one `peniko` in the graph).
#[cfg(any(feature = "cpu", feature = "gpu"))]
mod convert {
    use kurbo::{Affine, BezPath, Stroke};
    use peniko::color::{ColorSpaceTag, DynamicColor, HueDirection, Srgb, gradient};
    use peniko::{BlendMode, Brush, Color, ColorStop, Extend, Gradient, Mix};
    use scaena_core::displaylist::{Blend, Cap, Join, Paint, Path, PathEl};

    pub fn bez(path: &Path) -> BezPath {
        let p = |[x, y]: [f32; 2]| (f64::from(x), f64::from(y));
        let mut out = BezPath::new();
        for el in &path.0 {
            match *el {
                PathEl::MoveTo(a) => out.move_to(p(a)),
                PathEl::LineTo(a) => out.line_to(p(a)),
                PathEl::QuadTo(a, b) => out.quad_to(p(a), p(b)),
                PathEl::CurveTo(a, b, c) => out.curve_to(p(a), p(b), p(c)),
                PathEl::Close => out.close_path(),
            }
        }
        out
    }

    pub fn affine(m: &scaena_core::displaylist::Affine) -> Affine {
        Affine::new(m.map(f64::from))
    }

    /// The image pixels `src` onto the layer's rect `dst`: an image paint's transform.
    pub fn src_to_dst(src: [f32; 4], dst: [f32; 4]) -> Affine {
        let [sx, sy, sw, sh] = src.map(f64::from);
        let [dx, dy, dw, dh] = dst.map(f64::from);
        let (kx, ky) = (dw / sw, dh / sh);
        Affine::new([kx, 0.0, 0.0, ky, dx - sx * kx, dy - sy * ky])
    }

    /// `high` samples bilinearly in both painters: vello's GPU path has no bicubic, so a
    /// finer CPU filter would only make the painters disagree.
    pub fn image_quality(q: scaena_core::displaylist::Quality) -> peniko::ImageQuality {
        match q {
            scaena_core::displaylist::Quality::Low => peniko::ImageQuality::Low,
            scaena_core::displaylist::Quality::High => peniko::ImageQuality::Medium,
        }
    }

    pub fn rect([x, y, w, h]: [f32; 4]) -> kurbo::Rect {
        let [x, y, w, h] = [x, y, w, h].map(f64::from);
        kurbo::Rect::new(x, y, x + w, y + h)
    }

    pub fn stroke(width: f32, cap: Cap, join: Join, miter_limit: f32, dash: &[f32], dash_offset: f32) -> Stroke {
        let cap = match cap {
            Cap::Butt => kurbo::Cap::Butt,
            Cap::Round => kurbo::Cap::Round,
            Cap::Square => kurbo::Cap::Square,
        };
        let join = match join {
            Join::Miter => kurbo::Join::Miter,
            Join::Round => kurbo::Join::Round,
            Join::Bevel => kurbo::Join::Bevel,
        };
        Stroke::new(f64::from(width))
            .with_caps(cap)
            .with_join(join)
            .with_miter_limit(f64::from(miter_limit))
            .with_dashes(f64::from(dash_offset), dash.iter().map(|d| f64::from(*d)))
    }

    /// A display-list paint as a peniko brush: a color, or a gradient interpolated in
    /// Oklab with premultiplied alpha, as both painters draw it (SPEC §6).
    pub fn brush(paint: &Paint) -> Brush {
        let color = |c: &scaena_core::displaylist::Color| Color::from_rgba8(c.0[0], c.0[1], c.0[2], c.0[3]);
        let point = |p: &[f32; 2]| kurbo::Point::new(f64::from(p[0]), f64::from(p[1]));
        let (gradient, stops) = match paint {
            Paint::Solid(c) => return Brush::Solid(color(c)),
            Paint::Linear { start, end, stops } => (Gradient::new_linear(point(start), point(end)), stops),
            Paint::Radial { center, radius, stops } => (Gradient::new_radial(point(center), *radius), stops),
            // A sweep repeats around the turn, so one that starts off the x axis still
            // runs all the way round.
            Paint::Sweep { center, start_angle, end_angle, stops } => {
                (Gradient::new_sweep(point(center), *start_angle, *end_angle).with_extend(Extend::Repeat), stops)
            }
        };
        let stops: Vec<(f32, Color)> = stops.iter().map(|s| (s.0, color(&s.1))).collect();
        Brush::Gradient(gradient.with_stops(oklab_stops(&stops).as_slice()).with_interpolation_cs(ColorSpaceTag::Srgb))
    }

    /// The stops [`brush`] draws a gradient with, back in display-list colors.
    pub fn srgb_stops(stops: &[scaena_core::displaylist::Stop]) -> Vec<scaena_core::displaylist::Stop> {
        let stops: Vec<(f32, Color)> =
            stops.iter().map(|s| (s.0, Color::from_rgba8(s.1.0[0], s.1.0[1], s.1.0[2], s.1.0[3]))).collect();
        oklab_stops(&stops)
            .into_iter()
            .map(|s| {
                let c = s.color.to_alpha_color::<Srgb>().to_rgba8();
                scaena_core::displaylist::Stop(s.offset, scaena_core::displaylist::Color([c.r, c.g, c.b, c.a]))
            })
            .collect()
    }

    /// Stops close enough together that blending them in sRGB follows their blend in
    /// Oklab. vello's GPU ramp blends a gradient's stops in sRGB whatever color space the
    /// gradient names, while vello_cpu adds stops between them first, these same ones
    /// (`color::gradient`, to within 0.01 in Oklab). Adding them here hands both
    /// painters the same sRGB stops, so they draw the same ramp.
    fn oklab_stops(stops: &[(f32, Color)]) -> Vec<ColorStop> {
        if stops.len() < 2 {
            return stops.iter().map(|&(at, c)| ColorStop::from((at, c))).collect();
        }
        let mut out = Vec::new();
        for pair in stops.windows(2) {
            let [(a, from), (b, to)] = [pair[0], pair[1]];
            let (from, to) = (DynamicColor::from_alpha_color(from), DynamicColor::from_alpha_color(to));
            for (t, c) in gradient::<Srgb>(from, to, ColorSpaceTag::Oklab, HueDirection::default(), 0.01) {
                out.push(ColorStop::from((a + (b - a) * t, c.un_premultiply())));
            }
        }
        out
    }

    pub fn mix(blend: Blend) -> BlendMode {
        BlendMode::from(match blend {
            Blend::Normal => Mix::Normal,
            Blend::Multiply => Mix::Multiply,
            Blend::Screen => Mix::Screen,
            Blend::Overlay => Mix::Overlay,
            Blend::Darken => Mix::Darken,
            Blend::Lighten => Mix::Lighten,
            Blend::Difference => Mix::Difference,
        })
    }

    /// Whether a layer needs its own compositing group; otherwise its ops draw straight
    /// into the parent with the composed transform.
    pub fn isolated(opacity: f32, blend: Blend, clipped: bool) -> bool {
        opacity < 1.0 || blend != Blend::Normal || clipped
    }
}

#[cfg(feature = "cpu")]
pub mod cpu {
    use super::*;
    use crate::convert::{affine, bez, brush, image_quality, isolated, mix, rect, src_to_dst, stroke};
    use scaena_core::displaylist::{FillRule, Op, Paint};
    use vello_cpu::kurbo::{Affine, Rect};
    use vello_cpu::peniko::color::PremulRgba8;
    use vello_cpu::peniko::{Fill, ImageQuality, ImageSampler};
    use vello_cpu::{
        Image, ImageSource, Pixmap, PixmapMut, RasterizerSettings, RenderContext, RenderSettings, Resources,
    };
    pub use vello_cpu::{Level, RenderMode};

    /// `vello_cpu`-backed painter (PLAN 0.6).
    ///
    /// Glyphs are drawn unhinted: glifo's run builder hints by default and `vello`
    /// does not, so leaving the default would make the two painters disagree by design.
    pub struct CpuPainter {
        /// SIMD level. Rasters can differ slightly between levels (ADR-0004 finding 1);
        /// [`Level::new`] picks the host's best.
        pub level: Level,
        /// u8 (`OptimizeSpeed`, default) or f32 (`OptimizeQuality`) rasterization
        /// (ADR-0004 finding 2). vello_cpu honours `OptimizeQuality` only when built with
        /// its `f32_pipeline` feature; without it, it paints u8 whatever this says.
        pub mode: RenderMode,
        /// The threads a shader's rows are worked out on ([`Job::render_on`]); the bytes do
        /// not depend on it. [`scaena_core::shader::cores`] by default; 1 where frames are
        /// already painted on every core, as video's are.
        pub threads: usize,
        /// What a frame leaves for the next ([`Kept`]).
        pub kept: Kept,
    }

    impl Default for CpuPainter {
        fn default() -> Self {
            Self {
                level: Level::new(),
                mode: RenderMode::OptimizeSpeed,
                threads: scaena_core::shader::cores(),
                kept: Kept::default(),
            }
        }
    }

    /// What one frame of a [`CpuPainter`] leaves for the next: its render context, reset and
    /// drawn into again while frames keep their size and SIMD level. A frame then allocates
    /// little besides its pixels, where a new context allocated its tiles, strips, and
    /// coverage each time, and grew them again as the frame filled them. A context reset
    /// paints the same bytes as a new one.
    #[derive(Default)]
    pub struct Kept {
        ctx: Option<RenderContext>,
    }

    impl Kept {
        /// A context `width` × `height` at `level`, reset: the one kept, where it is that.
        fn context(&mut self, width: u16, height: u16, level: Level) -> &mut RenderContext {
            let fits = self.ctx.as_ref().is_some_and(|ctx| {
                let same = std::mem::discriminant(&ctx.render_settings().level) == std::mem::discriminant(&level);
                ctx.width() == width && ctx.height() == height && same
            });
            if !fits {
                self.ctx = None;
            }
            let settings = RenderSettings { level, ..Default::default() };
            let ctx = self.ctx.get_or_insert_with(|| RenderContext::new_with(width, height, settings));
            ctx.reset();
            ctx
        }
    }

    impl Painter for CpuPainter {
        fn name(&self) -> &'static str {
            "cpu"
        }

        fn paint(&mut self, dl: &DisplayList, fonts: &Assets, scale: f32) -> Result<Raster, PaintError> {
            check_version(dl)?;
            raster(dl, scale)?;
            let jobs = shader_jobs(dl, scale)?;
            let pixels = jobs.iter().flatten().map(|job| job.render_on(self.threads)).collect();
            self.draw(dl, fonts, scale, jobs, pixels)
        }
    }

    impl CpuPainter {
        /// [`Painter::paint`], with the pixels of the frame's shaders worked out already,
        /// elsewhere: `pixels` holds [`Job::render`]'s bytes for each of [`shader_jobs`]' jobs
        /// that covers a pixel, in their order. The browser works them out on workers of its
        /// own, each a band of rows (PLAN 2.28). Pixels that are not the jobs' size are an
        /// error.
        pub fn paint_shaded(
            &mut self,
            dl: &DisplayList,
            fonts: &Assets,
            scale: f32,
            pixels: Vec<Vec<u8>>,
        ) -> Result<Raster, PaintError> {
            check_version(dl)?;
            raster(dl, scale)?;
            let jobs = shader_jobs(dl, scale)?;
            let wanted = jobs.iter().flatten().count();
            if pixels.len() != wanted {
                let given = pixels.len();
                return Err(PaintError::Shaded(format!("the frame draws {wanted} shaders; {given} were given")));
            }
            for (i, (job, given)) in jobs.iter().flatten().zip(&pixels).enumerate() {
                let [_, _, w, h] = job.bbox();
                if given.len() != w as usize * h as usize * 4 {
                    let (bytes, given) = (w as usize * h as usize * 4, given.len());
                    let why = format!("shader {i} draws {w} × {h} pixels, {bytes} bytes; {given} were given");
                    return Err(PaintError::Shaded(why));
                }
            }
            self.draw(dl, fonts, scale, jobs, pixels)
        }

        /// `dl` at `scale`, each shader job with its pixels, `pixels` one for each job that
        /// covers a pixel.
        fn draw(
            &mut self,
            dl: &DisplayList,
            fonts: &Assets,
            scale: f32,
            jobs: Vec<Option<Job>>,
            pixels: Vec<Vec<u8>>,
        ) -> Result<Raster, PaintError> {
            let (width, height) = raster(dl, scale)?;
            let mut pixels = pixels.into_iter();
            let shaded: Vec<_> = jobs.into_iter().map(|job| Some((job?.bbox(), pixels.next()?))).collect();
            let ctx = self.kept.context(width, height, self.level);
            let mut resources = Resources::new();
            let shaded = shaded.into_iter();
            let mut cx = Cx { ctx: &mut *ctx, resources: &mut resources, store: fonts, fonts: &dl.fonts, shaded };
            cx.ops(&dl.ops, Affine::scale(f64::from(scale)))?;
            ctx.flush();
            // The frame's pixels, zeroed as `Pixmap::new` zeroes them, but by the allocator:
            // the frame covers the buffer, so packing draws every pixel over it.
            let mut rgba = vec![0u8; usize::from(width) * usize::from(height) * 4];
            let target = PixmapMut::new(width, height, &mut rgba).expect("a buffer of the frame's size");
            ctx.render_with(
                target,
                &mut resources,
                RasterizerSettings { render_mode: self.mode, ..Default::default() },
            );
            unpremultiply(&mut rgba);
            Ok(Raster { width: u32::from(width), height: u32::from(height), rgba })
        }
    }

    /// Premultiplied RGBA made straight, as a PNG or an `ImageData` takes it, in place:
    /// exactly what `Pixmap::take_unpremultiplied` computes, without its division for each
    /// opaque pixel, which it leaves as it is (`c · 255/255 + 0.5` truncates to `c`). A slide
    /// is opaque nearly everywhere, and in the browser that division was most of a frame's
    /// paint.
    fn unpremultiply(rgba: &mut [u8]) {
        for [r, g, b, a] in rgba.as_chunks_mut::<4>().0 {
            if *a != 255 && *a != 0 {
                let alpha = 255.0 / f32::from(*a);
                for c in [r, g, b] {
                    *c = (f32::from(*c) * alpha + 0.5) as u8;
                }
            }
        }
    }

    struct Cx<'a> {
        ctx: &'a mut RenderContext,
        resources: &'a mut Resources,
        store: &'a Assets,
        fonts: &'a [FontRef],
        /// One per shader op, in the order the walk meets them: where its job's pixels go,
        /// and the pixels; `None` for one that covers no pixel.
        shaded: std::vec::IntoIter<Option<([u32; 4], Vec<u8>)>>,
    }

    impl Cx<'_> {
        /// The paint the next fill or stroke draws with.
        fn set_paint(&mut self, paint: &Paint) {
            match brush(paint) {
                vello_cpu::peniko::Brush::Solid(c) => self.ctx.set_paint(c),
                vello_cpu::peniko::Brush::Gradient(g) => self.ctx.set_paint(g),
                vello_cpu::peniko::Brush::Image(_) => unreachable!("display-list paints are colors and gradients"),
            }
        }

        fn ops(&mut self, ops: &[Op], xf: Affine) -> Result<(), PaintError> {
            for op in ops {
                match op {
                    Op::Fill { path, rule, paint } => {
                        self.ctx.set_transform(xf);
                        self.ctx.set_fill_rule(match rule {
                            FillRule::NonZero => Fill::NonZero,
                            FillRule::EvenOdd => Fill::EvenOdd,
                        });
                        self.set_paint(paint);
                        self.ctx.fill_path(&bez(path));
                    }
                    Op::Stroke { path, paint, width, cap, join, miter_limit, dash, dash_offset } => {
                        self.ctx.set_transform(xf);
                        self.ctx.set_stroke(stroke(*width, *cap, *join, *miter_limit, dash, *dash_offset));
                        self.set_paint(paint);
                        self.ctx.stroke_path(&bez(path));
                    }
                    Op::Glyphs { font, size, coords, paint, glyphs, .. } => {
                        let font_ref = self.fonts.get(*font as usize).ok_or(PaintError::FontIndex(*font))?;
                        let font = self.store.font_data(font_ref)?;
                        self.ctx.set_transform(xf);
                        self.set_paint(paint);
                        self.ctx
                            .glyph_run(self.resources, &font)
                            .font_size(*size)
                            .normalized_coords(coords)
                            .hint(false)
                            .fill_glyphs(glyphs.iter().map(|g| vello_cpu::Glyph { id: g.id, x: g.x, y: g.y }));
                    }
                    Op::Layer { transform, opacity, blend, clip, ops, .. } => {
                        let child = xf * affine(transform);
                        if isolated(*opacity, *blend, clip.is_some()) {
                            self.ctx.set_transform(child);
                            let clip = clip.as_ref().map(bez);
                            self.ctx.push_layer(clip.as_ref(), Some(mix(*blend)), Some(*opacity), None, None);
                            self.ops(ops, child)?;
                            self.ctx.pop_layer();
                        } else {
                            self.ops(ops, child)?;
                        }
                    }
                    Op::Image { asset, src, dst, quality } => {
                        let picture = self.store.image(asset)?;
                        self.ctx.set_transform(xf);
                        self.ctx.set_paint(Image {
                            image: ImageSource::Pixmap(picture.pixmap()),
                            sampler: ImageSampler { quality: image_quality(*quality), ..ImageSampler::default() },
                        });
                        self.ctx.set_paint_transform(src_to_dst(*src, *dst));
                        self.ctx.set_fill_rule(Fill::NonZero);
                        self.ctx.fill_rect(&rect(*dst));
                        self.ctx.reset_paint_transform();
                    }
                    Op::Shader { rect, .. } => {
                        let shaded = self.shaded.next().expect("shader_jobs makes one job per shader op");
                        if let Some((bbox, pixels)) = shaded {
                            self.shader(bbox, pixels, *rect, xf);
                        }
                    }
                }
            }
            Ok(())
        }

        /// A shader job's pixels, the CPU reference's for the box `bbox`, filling `rect`
        /// texel for device pixel.
        fn shader(&mut self, bbox: [u32; 4], pixels: Vec<u8>, rect: scaena_core::displaylist::Rect, xf: Affine) {
            let [x, y, w, h] = bbox;
            let premultiplied = |c: u8, a: u8| ((u16::from(c) * u16::from(a) + 127) / 255) as u8;
            // The render's buffer becomes the pixmap's: RGBA8 is four bytes, as a pixel is.
            let mut pixels = bytemuck::allocation::try_cast_vec::<u8, PremulRgba8>(pixels)
                .unwrap_or_else(|(_, bytes)| bytemuck::cast_slice(&bytes).to_vec());
            // An opaque pixel is its own premultiple, and a backdrop is opaque throughout: it
            // takes no division.
            let mut opaque = true;
            for p in &mut pixels {
                if p.a != 255 {
                    (p.r, p.g, p.b) = (premultiplied(p.r, p.a), premultiplied(p.g, p.a), premultiplied(p.b, p.a));
                    opaque = false;
                }
            }
            // An opaque shader, as a backdrop is, is drawn without blending, and hides what is
            // under it: the same pixels, sooner.
            let pixmap = Pixmap::from_parts_with_opacity(pixels, w as u16, h as u16, !opaque);
            self.ctx.set_transform(xf);
            self.ctx.set_paint(Image {
                image: ImageSource::Pixmap(Arc::new(pixmap)),
                sampler: ImageSampler { quality: ImageQuality::Low, ..ImageSampler::default() },
            });
            // Texel (0, 0) on device pixel (x, y): the paint sits under the transform.
            self.ctx.set_paint_transform(xf.inverse() * Affine::translate((f64::from(x), f64::from(y))));
            self.ctx.set_fill_rule(Fill::NonZero);
            let [rx, ry, rw, rh] = rect.map(f64::from);
            self.ctx.fill_rect(&Rect::new(rx, ry, rx + rw, ry + rh));
            self.ctx.reset_paint_transform();
        }
    }

    #[cfg(test)]
    pub(crate) mod tests {
        use super::*;
        use scaena_core::displaylist::{Blend, Color, Paint, Path};

        /// PLAN 0.1 smoke test: `vello_cpu` links and rasterizes. A pixel-aligned opaque
        /// fill lands exactly, with nothing outside it, both at the architecture's
        /// baseline SIMD level and at whatever level the host detects.
        #[test]
        fn vello_cpu_fills_pixel_aligned_rect_exactly() {
            const ACCENT: [u8; 4] = [0xFF, 0x6A, 0x3D, 0xFF];
            for level in [Level::baseline(), Level::new()] {
                let mut dl = DisplayList::new([4.0, 4.0]);
                dl.ops.push(Op::Fill {
                    path: Path::rect([0.0, 0.0, 2.0, 2.0]),
                    rule: FillRule::NonZero,
                    paint: Paint::Solid(Color(ACCENT)),
                });
                let raster = CpuPainter { level, ..CpuPainter::default() }.paint(&dl, &Assets::new(), 1.0).unwrap();
                for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    assert_eq!(raster.pixel(x, y), ACCENT, "inside ({x},{y}) at {level:?}");
                }
                for (x, y) in [(2, 0), (0, 2), (2, 2), (3, 3)] {
                    assert_eq!(raster.pixel(x, y), [0; 4], "outside ({x},{y}) at {level:?}");
                }
            }
        }

        /// The painter's unpremultiply is vello's own, byte for byte: every alpha against
        /// every channel value, the impossible ones over alpha included.
        #[test]
        fn unpremultiplied_is_vellos_take_unpremultiplied_for_every_pixel() {
            let pixels = || {
                (0..=255u8)
                    .flat_map(|a| (0..=255u8).map(move |c| PremulRgba8 { r: c, g: c / 2, b: 255 - c, a }))
                    .collect::<Vec<_>>()
            };
            let mut ours: Vec<u8> = bytemuck::allocation::cast_vec(pixels());
            unpremultiply(&mut ours);
            let vellos: Vec<u8> = Pixmap::from_parts(pixels(), 256, 256)
                .take_unpremultiplied()
                .into_iter()
                .flat_map(|p| [p.r, p.g, p.b, p.a])
                .collect();
            assert_eq!(ours, vellos);
        }

        #[test]
        fn scale_maps_canvas_units_to_pixels_and_layers_compose() {
            let mut dl = DisplayList::new([4.0, 4.0]);
            dl.ops.push(Op::Layer {
                node: None,
                cell: None,
                transform: [1.0, 0.0, 0.0, 1.0, 2.0, 2.0],
                opacity: 1.0,
                blend: Blend::Normal,
                clip: None,
                ops: vec![Op::Fill {
                    path: Path::rect([0.0, 0.0, 1.0, 1.0]),
                    rule: FillRule::NonZero,
                    paint: Paint::Solid(Color([0, 0, 0, 255])),
                }],
            });
            let raster = CpuPainter::default().paint(&dl, &Assets::new(), 2.0).unwrap();
            assert_eq!((raster.width, raster.height), (8, 8));
            // The unit square at (2, 2) cu covers pixels 4..6 at 2 px per cu.
            assert_eq!(raster.pixel(4, 4), [0, 0, 0, 255]);
            assert_eq!(raster.pixel(5, 5), [0, 0, 0, 255]);
            assert_eq!(raster.pixel(3, 3)[3], 0);
            assert_eq!(raster.pixel(6, 6)[3], 0);
        }

        /// A raster past [`MAX_PIXELS`] is an error before the painter asks for its memory: a
        /// size at the canvas's ratio up to 65,535 pixels a side asked for gigabytes, and in the
        /// browser an allocation that fails stops the worker (PLAN 2.26).
        #[test]
        fn a_raster_past_max_pixels_is_an_error() {
            let dl = DisplayList::new([1920.0, 1080.0]);
            // 7724 × 4345, the first frame at its ratio past 2^25; 65280 × 36720, each side a
            // `u16`, 9.6 GB.
            for scale in [7724.0 / 1920.0, 17.0, 34.0f32] {
                let want = ((1920.0 * scale).round(), (1080.0 * scale).round());
                match CpuPainter::default().paint(&dl, &Assets::new(), scale) {
                    Err(PaintError::Size(w, h)) => assert_eq!((w, h), want),
                    other => panic!("{want:?}: {:?}", other.map(|r| (r.width, r.height))),
                }
            }
        }

        /// A three-color mesh op over `rect`.
        pub(crate) fn mesh_op(rect: [f32; 4]) -> Op {
            Op::Shader {
                kind: scaena_core::displaylist::ShaderKind::Mesh,
                seed: 7,
                t: 0.5,
                rect,
                palette: vec![Color([15, 118, 110, 255]), Color([194, 65, 12, 255]), Color([245, 196, 81, 255])],
                params: BTreeMap::new(),
            }
        }

        /// A shader's rows worked out on many threads paint what they paint on one.
        #[test]
        fn shader_threads_change_no_pixel() {
            let mut dl = DisplayList::new([200.0, 300.0]);
            dl.ops.push(mesh_op([0.0, 0.0, 200.0, 300.0]));
            let paint = |threads| {
                let mut painter = CpuPainter { threads, ..CpuPainter::default() };
                painter.paint(&dl, &Assets::new(), 1.0).unwrap().rgba
            };
            let one = paint(1);
            for threads in [2, 3, 8] {
                assert!(paint(threads) == one, "{threads} threads");
            }
        }

        /// A frame's shader pixels worked out elsewhere, in bands from the jobs' specs, paint
        /// what the painter paints working them out itself (PLAN 2.28); pixels that are not
        /// the jobs' are an error, not a panic.
        #[test]
        fn shader_pixels_from_elsewhere_paint_the_same() {
            let mut dl = DisplayList::new([200.0, 300.0]);
            dl.ops.push(mesh_op([0.0, 0.0, 200.0, 300.0]));
            dl.ops.push(Op::Layer {
                node: None,
                cell: None,
                transform: [1.0, 0.0, 0.0, 1.0, 10.0, 20.0],
                opacity: 0.5,
                blend: Blend::Normal,
                clip: None,
                ops: vec![mesh_op([0.0, 0.0, 50.0, 40.0]), mesh_op([400.0, 0.0, 10.0, 10.0])],
            });
            let scale = 1.5;
            let want = CpuPainter::default().paint(&dl, &Assets::new(), scale).unwrap();
            let specs = shader_specs(&dl, scale).unwrap();
            assert_eq!(specs.len(), 3);
            let mut pixels = Vec::new();
            for spec in &specs {
                let bytes = spec.to_bytes().unwrap();
                let Some(job) = Spec::from_bytes(&bytes).unwrap().job().unwrap() else { continue };
                let [_, _, w, h] = job.bbox();
                let mut out = Vec::new();
                for [first, rows] in scaena_core::shader::bands(h, 3) {
                    let mut band = vec![0; rows as usize * w as usize * 4];
                    job.render_rows(first, &mut band);
                    out.extend(band);
                }
                pixels.push(out);
            }
            // The third op is past the raster: no job, no pixels.
            assert_eq!(pixels.len(), 2);
            let mut painter = CpuPainter::default();
            assert!(painter.paint_shaded(&dl, &Assets::new(), scale, pixels.clone()).unwrap() == want);
            let short = pixels[..1].to_vec();
            assert!(matches!(painter.paint_shaded(&dl, &Assets::new(), scale, short), Err(PaintError::Shaded(_))));
            let mut cut = pixels;
            cut[1].pop();
            assert!(matches!(painter.paint_shaded(&dl, &Assets::new(), scale, cut), Err(PaintError::Shaded(_))));
        }

        #[test]
        fn a_shader_op_paints_the_cpu_reference_pixel_for_pixel() {
            let mut dl = DisplayList::new([50.0, 30.0]);
            dl.ops.push(Op::Layer {
                node: None,
                cell: None,
                transform: [1.0, 0.0, 0.0, 1.0, 5.0, 2.0],
                opacity: 1.0,
                blend: Blend::Normal,
                clip: None,
                ops: vec![mesh_op([5.0, 3.0, 30.0, 20.0])],
            });
            let raster = CpuPainter::default().paint(&dl, &Assets::new(), 2.0).unwrap();
            let job = shader_jobs(&dl, 2.0).unwrap().remove(0).unwrap();
            // (5, 2) + (5, 3) cu at 2 px per cu.
            assert_eq!(job.bbox(), [20, 10, 60, 40]);
            let want = job.render();
            for gy in 0..40 {
                for gx in 0..60 {
                    let i = (gy * 60 + gx) as usize * 4;
                    assert_eq!(raster.pixel(20 + gx, 10 + gy), want[i..i + 4], "({gx}, {gy})");
                }
            }
            for (x, y) in [(19, 10), (80, 10), (20, 9), (20, 50)] {
                assert_eq!(raster.pixel(x, y), [0; 4], "({x}, {y}) is outside");
            }
        }

        #[test]
        fn png_round_trips_pixels_exactly() {
            let raster = Raster { width: 2, height: 1, rgba: vec![255, 0, 0, 255, 0, 128, 255, 64] };
            for png in [raster.to_png().unwrap(), raster.to_png_fast().unwrap()] {
                assert_eq!(Raster::from_png(&png).unwrap(), raster);
            }
            assert_eq!(raster.to_png().unwrap(), raster.to_png().unwrap(), "same pixels, same bytes");
            assert_eq!(raster.to_png_fast().unwrap(), raster.to_png_fast().unwrap(), "same pixels, same bytes");
            // A browser's screenshot has no alpha: it reads as opaque.
            let mut rgb = Vec::new();
            let mut encoder = png::Encoder::new(&mut rgb, 2, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[255, 0, 0, 0, 128, 255]).unwrap();
            writer.finish().unwrap();
            assert_eq!(Raster::from_png(&rgb).unwrap().rgba, [255, 0, 0, 255, 0, 128, 255, 255]);
        }

        #[test]
        fn gradient_paints_run_their_stops_in_oklab() {
            let (red, blue) =
                (scaena_core::displaylist::Color([255, 0, 0, 255]), scaena_core::displaylist::Color([0, 0, 255, 255]));
            let mut dl = DisplayList::new([64.0, 4.0]);
            dl.ops.push(Op::Fill {
                path: Path::rect([0.0, 0.0, 64.0, 4.0]),
                rule: FillRule::NonZero,
                paint: Paint::Linear {
                    start: [0.0, 0.0],
                    end: [64.0, 0.0],
                    stops: vec![scaena_core::displaylist::Stop(0.0, red), scaena_core::displaylist::Stop(1.0, blue)],
                },
            });
            let raster = CpuPainter::default().paint(&dl, &Assets::new(), 1.0).unwrap();
            let px = |x: usize| &raster.rgba[(2 * 64 + x) * 4..(2 * 64 + x) * 4 + 4];
            assert!(px(0)[0] > 240 && px(0)[2] < 20, "{:?}", px(0));
            assert!(px(63)[2] > 240 && px(63)[0] < 20, "{:?}", px(63));
            // Halfway in Oklab, red to blue is a light violet near (140, 83, 165), not
            // sRGB's dark purple (128, 0, 128).
            let mid = px(32);
            assert!(mid[1] > 60 && mid[0] > 120 && mid[2] > 140, "{mid:?}");
        }
    }
}

#[cfg(feature = "gpu")]
pub mod gpu {
    //! `vello` on `wgpu` (PLAN 0.7).
    //!
    //! [`scene`] turns a display list into a vello scene on every target, wasm32
    //! included: the browser draws it to a WebGPU canvas (PLAN 0.8), the Mac to a
    //! `CAMetalLayer`. Surfaces are the client's job. [`GpuPainter`] is the native
    //! headless painter: it renders into a texture and reads it back, which cannot
    //! block in a browser, so it is not built for wasm32.
    //!
    //! Shader ops run first: [`Shaders::prepare`] dispatches each job's WGSL into a
    //! texture and registers it with the renderer, and the scene draws those textures
    //! where the CPU painter draws the reference's pixels.

    use super::*;
    use crate::convert::{affine, bez, brush, image_quality, isolated, mix, rect, src_to_dst, stroke};
    use scaena_core::displaylist::{FillRule, Glyph, Join, Op};
    use std::f64::consts::SQRT_2;
    use vello::Scene;
    use vello::kurbo::{Affine, Rect, Shape};
    use vello::peniko::{Fill, ImageBrush, ImageData, ImageQuality};
    use vello::wgpu;
    use vello::wgpu::util::DeviceExt;

    /// A shader job's pixels on the GPU: the device pixels they cover, and the image
    /// the renderer knows their texture by.
    #[derive(Debug, Clone)]
    pub struct ShaderImage {
        pub bbox: [u32; 4],
        pub image: ImageData,
    }

    /// Runs shader jobs' WGSL (`scaena_core::shader`). One per device; each kind's
    /// pipeline compiles on first use.
    #[derive(Default)]
    pub struct Shaders {
        pipelines: Vec<(&'static str, wgpu::ComputePipeline)>,
    }

    impl Shaders {
        pub fn new() -> Self {
            Self::default()
        }

        fn pipeline(&mut self, device: &wgpu::Device, wgsl: &'static str) -> &wgpu::ComputePipeline {
            let i = match self.pipelines.iter().position(|(src, _)| std::ptr::eq(*src, wgsl)) {
                Some(i) => i,
                None => {
                    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                        label: Some("scaena shader"),
                        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
                    });
                    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                        label: Some("scaena shader"),
                        layout: None,
                        module: &module,
                        entry_point: Some("main"),
                        compilation_options: Default::default(),
                        cache: None,
                    });
                    self.pipelines.push((wgsl, pipeline));
                    self.pipelines.len() - 1
                }
            };
            &self.pipelines[i].1
        }

        /// Record `job`'s WGSL into `encoder`. It writes RGBA8 bytes into the returned
        /// buffer, rows the returned number of bytes apart: a multiple of 256, as a copy
        /// into a texture needs.
        pub fn dispatch(
            &mut self,
            device: &wgpu::Device,
            encoder: &mut wgpu::CommandEncoder,
            job: &Job,
        ) -> (wgpu::Buffer, u32) {
            let [_, _, w, h] = job.bbox();
            let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
            let stride = (w * 4).div_ceil(align) * align;
            let uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("scaena shader uniforms"),
                contents: &job.uniforms(stride / 4),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let pixels = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("scaena shader pixels"),
                size: u64::from(stride) * u64::from(h),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let pipeline = self.pipeline(device, job.wgsl());
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("scaena shader"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: pixels.as_entire_binding() },
                ],
            });
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
            drop(pass);
            (pixels, stride)
        }

        /// Run every job on the GPU into its own texture and register each with
        /// `renderer`; pass the result to [`scene`], then to [`Shaders::release`] once
        /// the renderer has drawn it.
        pub fn prepare(
            &mut self,
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            renderer: &mut vello::Renderer,
            jobs: &[Option<Job>],
        ) -> Vec<Option<ShaderImage>> {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            let mut out = Vec::with_capacity(jobs.len());
            for job in jobs {
                let Some(job) = job else {
                    out.push(None);
                    continue;
                };
                let [_, _, w, h] = job.bbox();
                let (pixels, stride) = self.dispatch(device, &mut encoder, job);
                let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("scaena shader"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                encoder.copy_buffer_to_texture(
                    wgpu::TexelCopyBufferInfo {
                        buffer: &pixels,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(stride),
                            rows_per_image: None,
                        },
                    },
                    texture.as_image_copy(),
                    size,
                );
                let image = renderer.register_texture(texture);
                out.push(Some(ShaderImage { bbox: job.bbox(), image }));
            }
            // Submitted before the renderer's own work, which copies the textures.
            queue.submit([encoder.finish()]);
            out
        }

        /// Unregister what [`Shaders::prepare`] gave `renderer`.
        pub fn release(renderer: &mut vello::Renderer, images: Vec<Option<ShaderImage>>) {
            for image in images.into_iter().flatten() {
                renderer.unregister_texture(image.image);
            }
        }
    }

    /// The vello scene for `dl` at `scale` output pixels per canvas unit, drawing its
    /// shader ops from `shaders` ([`Shaders::prepare`] on [`shader_jobs`]). Ops map one
    /// to one onto `CpuPainter`'s calls, through the same conversions.
    pub fn scene(
        dl: &DisplayList,
        fonts: &Assets,
        scale: f32,
        shaders: &[Option<ShaderImage>],
    ) -> Result<Scene, PaintError> {
        check_version(dl)?;
        let (width, height) = raster_size(dl, scale)?;
        let mut cx = Cx {
            scene: Scene::new(),
            store: fonts,
            fonts: &dl.fonts,
            // vello layers always clip; an unclipped layer clips to the whole output.
            output: Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            shaders: shaders.iter(),
        };
        cx.ops(&dl.ops, Affine::scale(f64::from(scale)))?;
        Ok(cx.scene)
    }

    struct Cx<'a> {
        scene: Scene,
        store: &'a Assets,
        fonts: &'a [FontRef],
        output: Rect,
        shaders: std::slice::Iter<'a, Option<ShaderImage>>,
    }

    impl Cx<'_> {
        fn ops(&mut self, ops: &[Op], xf: Affine) -> Result<(), PaintError> {
            for op in ops {
                match op {
                    Op::Fill { path, rule, paint } => {
                        let rule = match rule {
                            FillRule::NonZero => Fill::NonZero,
                            FillRule::EvenOdd => Fill::EvenOdd,
                        };
                        self.scene.fill(rule, xf, &brush(paint), None, &bez(path));
                    }
                    Op::Stroke { path, paint, width, cap, join, miter_limit, dash, dash_offset } => {
                        let style = stroke(*width, *cap, *join, *miter_limit, dash, *dash_offset);
                        self.scene.stroke(&style, xf, &brush(paint), None, &bez(path));
                    }
                    Op::Glyphs { font, size, coords, paint, glyphs, .. } => {
                        let font_ref = self.fonts.get(*font as usize).ok_or(PaintError::FontIndex(*font))?;
                        let font = self.store.font_data(font_ref)?;
                        let brush = brush(paint);
                        // vello fills a whole glyph run as one path, so overlapping glyphs
                        // would share a winding count: where a base and a mark of opposite
                        // contour direction overlap, the overlap cancels to a hole. One run
                        // per glyph keeps every glyph its own fill, as vello_cpu draws them
                        // (ADR-0004 finding 6).
                        for g in glyphs {
                            self.scene
                                .draw_glyphs(&font)
                                .transform(xf)
                                .font_size(*size)
                                .normalized_coords(coords)
                                .hint(false)
                                .brush(&brush)
                                .draw(Fill::NonZero, std::iter::once(vello::Glyph { id: g.id, x: g.x, y: g.y }));
                        }
                    }
                    Op::Layer { transform, opacity, blend, clip, ops, .. } => {
                        let child = xf * affine(transform);
                        if isolated(*opacity, *blend, clip.is_some()) {
                            match clip {
                                Some(clip) => {
                                    self.scene.push_layer(Fill::NonZero, mix(*blend), *opacity, child, &bez(clip))
                                }
                                // vello layers always clip, and blend every tile the clip
                                // covers. A layer with no clip of its own clips to the pixels
                                // its ops can ink, so a label fading in costs its own tiles,
                                // not the frame's: on a software adapter, a chart's labels
                                // mid-transition took seconds a frame. Ink it cannot bound (a
                                // color glyph) clips to the whole output; none draws nothing.
                                None => {
                                    let clip = match self.ink(ops, child) {
                                        Some(ink) => ink.inflate(1.0, 1.0).expand().intersect(self.output),
                                        None => self.output,
                                    };
                                    if clip.width() <= 0.0 || clip.height() <= 0.0 {
                                        continue;
                                    }
                                    self.scene.push_layer(Fill::NonZero, mix(*blend), *opacity, Affine::IDENTITY, &clip)
                                }
                            }
                            self.ops(ops, child)?;
                            self.scene.pop_layer();
                        } else {
                            self.ops(ops, child)?;
                        }
                    }
                    Op::Image { asset, src, dst, quality } => {
                        let picture = self.store.image(asset)?;
                        let image = ImageData {
                            data: picture.rgba.clone(),
                            format: vello::peniko::ImageFormat::Rgba8,
                            alpha_type: vello::peniko::ImageAlphaType::Alpha,
                            width: picture.width,
                            height: picture.height,
                        };
                        let brush = ImageBrush::new(image).with_quality(image_quality(*quality));
                        self.scene.fill(Fill::NonZero, xf, &brush, Some(src_to_dst(*src, *dst)), &rect(*dst));
                    }
                    Op::Shader { rect, .. } => {
                        let shader = self.shaders.next().ok_or_else(|| {
                            PaintError::Gpu("a shader op with no image: run `Shaders::prepare` on `shader_jobs`".into())
                        })?;
                        if let Some(ShaderImage { bbox: [x, y, ..], image }) = shader {
                            let brush = ImageBrush::new(image.clone()).with_quality(ImageQuality::Low);
                            // Texel (0, 0) on device pixel (x, y), as the CPU painter places it.
                            let place = xf.inverse() * Affine::translate((f64::from(*x), f64::from(*y)));
                            let [rx, ry, rw, rh] = rect.map(f64::from);
                            let rect = Rect::new(rx, ry, rx + rw, ry + rh);
                            self.scene.fill(Fill::NonZero, xf, &brush, Some(place), &rect);
                        }
                    }
                }
            }
            Ok(())
        }

        /// The device pixels `ops`, drawn with `xf`, can ink: a box around every path, stroke,
        /// glyph outline, image, and shader they draw, an empty one if they draw nothing, or
        /// `None` where a bound is not known: a glyph from a color font, whose layers and
        /// bitmaps reach past its outline, or a font the display list does not name.
        fn ink(&self, ops: &[Op], xf: Affine) -> Option<Rect> {
            let mut ink: Option<Rect> = None;
            let mut add = |r: Rect| ink = Some(ink.map_or(r, |i| i.union(r)));
            for op in ops {
                match op {
                    Op::Fill { path, .. } => add(xf.transform_rect_bbox(bez(path).bounding_box())),
                    Op::Stroke { path, width, join, miter_limit, .. } => {
                        // A miter reaches `miter_limit` half widths out, a square cap √2.
                        let half = f64::from(*width) / 2.0;
                        let reach = match join {
                            Join::Miter => half * f64::from(*miter_limit).max(SQRT_2),
                            Join::Round | Join::Bevel => half * SQRT_2,
                        };
                        add(xf.transform_rect_bbox(bez(path).bounding_box().inflate(reach, reach)))
                    }
                    Op::Glyphs { font, size, coords, glyphs, .. } => {
                        if let Some(r) = self.glyph_ink(*font, *size, coords, glyphs)? {
                            add(xf.transform_rect_bbox(r));
                        }
                    }
                    Op::Image { dst, .. } => add(xf.transform_rect_bbox(rect(*dst))),
                    Op::Shader { rect: r, .. } => add(xf.transform_rect_bbox(rect(*r))),
                    Op::Layer { transform, clip, ops, .. } => {
                        let child = xf * affine(transform);
                        let inner = self.ink(ops, child)?;
                        let clipped = match clip {
                            Some(clip) => inner.intersect(child.transform_rect_bbox(bez(clip).bounding_box())),
                            None => inner,
                        };
                        if clipped.width() > 0.0 && clipped.height() > 0.0 {
                            add(clipped);
                        }
                    }
                }
            }
            Some(ink.unwrap_or(Rect::ZERO))
        }

        /// The box around `glyphs`' outlines in the run's space, from the font at `size` and
        /// `coords` as vello draws it, unhinted: `Some(None)` for a run that inks nothing,
        /// `None` for a color font or a font the display list does not name.
        fn glyph_ink(&self, font: u32, size: f32, coords: &[i16], glyphs: &[Glyph]) -> Option<Option<Rect>> {
            let data = self.store.font_data(self.fonts.get(font as usize)?).ok()?;
            let face = skrifa::FontRef::from_index(data.data.as_ref(), data.index).ok()?;
            let color = [b"COLR", b"CBDT", b"sbix", b"SVG "];
            if color.iter().any(|tag| face.table_data(skrifa::Tag::new(tag)).is_some()) {
                return None;
            }
            let location: Vec<skrifa::instance::NormalizedCoord> =
                coords.iter().map(|&c| skrifa::instance::NormalizedCoord::from_bits(c)).collect();
            let metrics = skrifa::metrics::GlyphMetrics::new(
                &face,
                skrifa::instance::Size::new(size),
                skrifa::instance::LocationRef::new(&location),
            );
            let mut ink: Option<Rect> = None;
            for g in glyphs {
                let b = metrics.bounds(skrifa::GlyphId::new(g.id))?;
                if b.x_max <= b.x_min || b.y_max <= b.y_min {
                    continue;
                }
                // Font units point up, the canvas's down.
                let (x, y) = (f64::from(g.x), f64::from(g.y));
                let r = Rect::new(
                    x + f64::from(b.x_min),
                    y - f64::from(b.y_max),
                    x + f64::from(b.x_max),
                    y - f64::from(b.y_min),
                );
                ink = Some(ink.map_or(r, |i| i.union(r)));
            }
            Some(ink)
        }
    }

    #[cfg(test)]
    mod scene_tests {
        use super::*;
        use scaena_core::displaylist::{Paint, Path};

        #[test]
        fn scene_needs_no_device_and_takes_every_paint() {
            let mut dl = DisplayList::new([4.0, 4.0]);
            dl.ops.push(Op::Fill {
                path: Path::rect([0.0, 0.0, 1.0, 1.0]),
                rule: FillRule::NonZero,
                paint: Paint::Solid(scaena_core::displaylist::Color([0, 0, 0, 255])),
            });
            dl.ops.push(Op::Fill {
                path: Path::rect([0.0, 0.0, 1.0, 1.0]),
                rule: FillRule::NonZero,
                paint: Paint::Linear { start: [0.0, 0.0], end: [1.0, 0.0], stops: vec![] },
            });
            assert!(scene(&dl, &Assets::new(), 1.0, &[]).is_ok());
        }

        /// A layer with no clip of its own clips to the pixels its ops ink: around paths,
        /// strokes as far as their joins reach, and glyph outlines, through every transform;
        /// a color font's glyphs, which reach past their outlines, are not bounded.
        #[test]
        fn a_layer_clips_to_what_it_inks() {
            use scaena_core::displaylist::{Cap, Color, Join, Path, PathEl};
            let fonts = "../../tests/fixtures/torture.scaena/fonts";
            let mut store = Assets::new();
            for file in ["RobotoSerif-VF.ttf", "NotoColorEmoji-COLRv1.ttf"] {
                store.insert_font(file, std::fs::read(format!("{fonts}/{file}")).unwrap());
            }
            let names = [
                FontRef { id: "RobotoSerif-VF.ttf".into(), index: 0 },
                FontRef { id: "NotoColorEmoji-COLRv1.ttf".into(), index: 0 },
            ];
            let cx = Cx {
                scene: Scene::new(),
                store: &store,
                fonts: &names,
                output: Rect::new(0.0, 0.0, 1920.0, 1080.0),
                shaders: [].iter(),
            };
            let black = Paint::Solid(Color([0, 0, 0, 255]));
            let fill =
                Op::Fill { path: Path::rect([10.0, 20.0, 30.0, 40.0]), rule: FillRule::NonZero, paint: black.clone() };
            let line = |join| Op::Stroke {
                path: Path(vec![PathEl::MoveTo([0.0, 50.0]), PathEl::LineTo([100.0, 50.0])]),
                paint: black.clone(),
                width: 4.0,
                cap: Cap::Butt,
                join,
                miter_limit: 4.0,
                dash: vec![],
                dash_offset: 0.0,
            };
            // `H` in each font, whose glyph ids are the subset's.
            let h = |file: &str| {
                let bytes = std::fs::read(format!("{fonts}/{file}")).unwrap();
                let face = skrifa::FontRef::new(&bytes).unwrap();
                use skrifa::MetadataProvider;
                face.charmap().map('H').unwrap_or_default().to_u32()
            };
            let ids = [h("RobotoSerif-VF.ttf"), h("NotoColorEmoji-COLRv1.ttf")];
            let glyphs = |font: u32| Op::Glyphs {
                font,
                size: 100.0,
                coords: vec![],
                paint: black.clone(),
                text: String::new(),
                glyphs: vec![Glyph { id: ids[font as usize], x: 200.0, y: 500.0 }],
                clusters: vec![],
            };
            let scale = Affine::scale(2.0);
            assert_eq!(cx.ink(std::slice::from_ref(&fill), scale), Some(Rect::new(20.0, 40.0, 80.0, 120.0)));
            assert_eq!(cx.ink(&[line(Join::Miter)], Affine::IDENTITY), Some(Rect::new(-8.0, 42.0, 108.0, 58.0)));
            let round = cx.ink(&[line(Join::Round)], Affine::IDENTITY).unwrap();
            assert!((round.y1 - 50.0 - 2.0 * std::f64::consts::SQRT_2).abs() < 1e-9, "{round:?}");
            // A Latin glyph's outline: above the baseline, right of the pen, under an em.
            let text = cx.ink(&[glyphs(0)], Affine::IDENTITY).unwrap();
            assert!(text.y0 < 500.0 && text.y1 <= 500.0 + 30.0 && text.y0 > 400.0 - 30.0, "{text:?}");
            assert!(text.x0 >= 200.0 - 10.0 && text.x1 < 300.0 && text.width() > 10.0, "{text:?}");
            assert_eq!(cx.ink(&[glyphs(1)], Affine::IDENTITY), None, "a color font is not bounded");
            // A nested layer's clip and transform: only the part of the fill inside its clip.
            let nested = Op::Layer {
                node: None,
                cell: None,
                transform: [1.0, 0.0, 0.0, 1.0, 100.0, 0.0],
                opacity: 1.0,
                blend: scaena_core::displaylist::Blend::Normal,
                clip: Some(Path::rect([0.0, 0.0, 25.0, 1000.0])),
                ops: vec![fill],
            };
            assert_eq!(cx.ink(&[nested], Affine::IDENTITY), Some(Rect::new(110.0, 20.0, 125.0, 60.0)));
            assert_eq!(cx.ink(&[], Affine::IDENTITY), Some(Rect::ZERO), "nothing inks nothing");
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub use native::GpuPainter;

    #[cfg(not(target_arch = "wasm32"))]
    mod native {
        use super::*;

        /// Headless `vello` (PLAN 0.7): renders into an `Rgba8Unorm` texture and reads it
        /// back. vello writes straight alpha (it unpremultiplies before storing), which
        /// is what [`Raster`] holds.
        ///
        /// [`GpuPainter::paint`] waits for its frame. A run of frames, a video's (PLAN
        /// 2.22), goes through [`GpuPainter::send`] and [`GpuPainter::receive`] instead,
        /// so a frame is painted while the ones before it are read back.
        pub struct GpuPainter {
            device: wgpu::Device,
            queue: wgpu::Queue,
            renderer: vello::Renderer,
            adapter: wgpu::AdapterInfo,
            shaders: Shaders,
            /// What frames are painted into, kept while they keep its size.
            target: Option<Target>,
            /// Frames sent and not yet received, oldest first.
            flying: std::collections::VecDeque<Flight>,
            /// Readback buffers no frame is using.
            spare: Vec<wgpu::Buffer>,
        }

        struct Target {
            size: wgpu::Extent3d,
            texture: wgpu::Texture,
            view: wgpu::TextureView,
        }

        /// A frame painted and on its way back: copied into `buffer`, which is mapped
        /// once the GPU has done `submitted`.
        struct Flight {
            buffer: wgpu::Buffer,
            width: u32,
            height: u32,
            /// Bytes from one row to the next in `buffer`.
            padded: u32,
            submitted: wgpu::SubmissionIndex,
            mapped: std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
        }

        impl GpuPainter {
            /// The adapter wgpu prefers for high performance. `WGPU_BACKEND` and the other
            /// `wgpu` environment variables narrow the choice. Fails on a machine with no
            /// adapter at all; on Linux without a GPU, Mesa's lavapipe (software Vulkan)
            /// is one.
            pub fn new() -> Result<Self, PaintError> {
                let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
                let options = wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    ..Default::default()
                };
                let adapter = ready(instance.request_adapter(&options))?
                    .map_err(|e| PaintError::Gpu(format!("no adapter: {e}")))?;
                // The largest textures and buffers the adapter holds, not wgpu's defaults (8192
                // pixels a side, 128 MiB a shader's pixels): a raster the CPU painter makes, this
                // one makes too, where the GPU can.
                let most = adapter.limits();
                let required_limits = wgpu::Limits {
                    max_buffer_size: most.max_buffer_size,
                    max_storage_buffer_binding_size: most.max_storage_buffer_binding_size,
                    ..wgpu::Limits::default().using_resolution(most)
                };
                let descriptor =
                    wgpu::DeviceDescriptor { label: Some("scaena"), required_limits, ..Default::default() };
                let (device, queue) = ready(adapter.request_device(&descriptor))?.map_err(gpu)?;
                Self::with_device(device, queue, adapter.get_info())
            }

            /// Paint with a device the caller already has.
            pub fn with_device(
                device: wgpu::Device,
                queue: wgpu::Queue,
                adapter: wgpu::AdapterInfo,
            ) -> Result<Self, PaintError> {
                let options = vello::RendererOptions {
                    use_cpu: false,
                    // Area coverage is the counterpart of vello_cpu's analytic coverage.
                    antialiasing_support: vello::AaSupport::area_only(),
                    num_init_threads: std::num::NonZeroUsize::new(1),
                    pipeline_cache: None,
                };
                let renderer = vello::Renderer::new(&device, options).map_err(gpu)?;
                Ok(Self {
                    device,
                    queue,
                    renderer,
                    adapter,
                    shaders: Shaders::new(),
                    target: None,
                    flying: std::collections::VecDeque::new(),
                    spare: Vec::new(),
                })
            }

            /// Which adapter paints: name, backend, and device type (a CPU adapter such
            /// as lavapipe reports `Cpu`).
            pub fn adapter(&self) -> &wgpu::AdapterInfo {
                &self.adapter
            }

            /// `job`'s pixels as its WGSL computes them on this GPU: what the CPU
            /// painter gets from [`Job::render`], for the shader parity test.
            pub fn shader_pixels(&mut self, job: &Job) -> Result<Vec<u8>, PaintError> {
                let [_, _, w, h] = job.bbox();
                let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                let (pixels, stride) = self.shaders.dispatch(&self.device, &mut encoder, job);
                let size = u64::from(stride) * u64::from(h);
                let buffer = self.readback(size);
                encoder.copy_buffer_to_buffer(&pixels, 0, &buffer, 0, size);
                self.queue.submit([encoder.finish()]);
                self.read(&buffer, w * 4, stride)
            }

            /// Paint `dl` and start reading it back, without waiting for either: the next
            /// frame can be painted while this one comes back. Frames come back from
            /// [`GpuPainter::receive`] in the order they were sent.
            pub fn send(&mut self, dl: &DisplayList, fonts: &Assets, scale: f32) -> Result<(), PaintError> {
                let flight = self.start(dl, fonts, scale)?;
                self.flying.push_back(flight);
                Ok(())
            }

            /// The oldest frame [`GpuPainter::send`] painted that has not been received,
            /// once it is back, or `None` when every frame sent has been.
            pub fn receive(&mut self) -> Result<Option<Raster>, PaintError> {
                match self.flying.pop_front() {
                    Some(flight) => self.finish(flight).map(Some),
                    None => Ok(None),
                }
            }

            fn readback(&self, size: u64) -> wgpu::Buffer {
                self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("scaena readback"),
                    size,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                })
            }

            /// Wait for `buffer`, then take `row` bytes from every `padded` bytes of it.
            fn read(&self, buffer: &wgpu::Buffer, row: u32, padded: u32) -> Result<Vec<u8>, PaintError> {
                let (tx, rx) = std::sync::mpsc::channel();
                buffer.map_async(wgpu::MapMode::Read, .., move |mapped| {
                    let _ = tx.send(mapped);
                });
                // Bounded: a GPU that never finishes is an error to report, not a hang.
                let wait = wgpu::PollType::Wait { submission_index: None, timeout: Some(GPU_TIMEOUT) };
                self.device.poll(wait).map_err(gpu)?;
                rx.recv_timeout(GPU_TIMEOUT)
                    .map_err(|_| PaintError::Gpu(format!("readback not done after {GPU_TIMEOUT:?}")))?
                    .map_err(gpu)?;
                let mut out = Vec::with_capacity(buffer.size() as usize / padded as usize * row as usize);
                for line in buffer.get_mapped_range(..).chunks_exact(padded as usize) {
                    out.extend_from_slice(&line[..row as usize]);
                }
                buffer.unmap();
                Ok(out)
            }
        }

        impl Painter for GpuPainter {
            fn name(&self) -> &'static str {
                "gpu"
            }

            fn paint(&mut self, dl: &DisplayList, fonts: &Assets, scale: f32) -> Result<Raster, PaintError> {
                let flight = self.start(dl, fonts, scale)?;
                self.finish(flight)
            }
        }

        impl GpuPainter {
            /// Run `dl`'s shaders, paint it, and queue its copy into a readback buffer:
            /// a frame on its way back.
            fn start(&mut self, dl: &DisplayList, fonts: &Assets, scale: f32) -> Result<Flight, PaintError> {
                let jobs = shader_jobs(dl, scale)?;
                self.holds(dl, scale, &jobs)?;
                let images = self.shaders.prepare(&self.device, &self.queue, &mut self.renderer, &jobs);
                let flight = self.render(dl, fonts, scale, &images);
                Shaders::release(&mut self.renderer, images);
                flight
            }

            /// Whether this GPU holds what painting `dl` at `scale` makes: its target and the
            /// buffer it is read back through, and each shader's pixels, a storage buffer of
            /// four bytes a pixel, and texture. A frame past one is an error here, as one past
            /// [`MAX_PIXELS`] is: wgpu's default error handler panics on a texture or buffer
            /// the device does not hold.
            fn holds(&self, dl: &DisplayList, scale: f32, jobs: &[Option<Job>]) -> Result<(), PaintError> {
                let limits = self.device.limits();
                let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
                let fits = |w: u32, h: u32, most: u64| {
                    let bytes = u64::from((w * 4).div_ceil(align) * align) * u64::from(h);
                    w <= limits.max_texture_dimension_2d && h <= limits.max_texture_dimension_2d && bytes <= most
                };
                let storage = limits.max_buffer_size.min(limits.max_storage_buffer_binding_size);
                let (w, h) = raster(dl, scale)?;
                let shaders_fit = jobs.iter().flatten().all(|job| {
                    let [_, _, w, h] = job.bbox();
                    fits(w, h, storage)
                });
                match fits(u32::from(w), u32::from(h), limits.max_buffer_size) && shaders_fit {
                    true => Ok(()),
                    false => Err(PaintError::Size(f32::from(w), f32::from(h))),
                }
            }

            fn render(
                &mut self,
                dl: &DisplayList,
                fonts: &Assets,
                scale: f32,
                images: &[Option<ShaderImage>],
            ) -> Result<Flight, PaintError> {
                let scene = scene(dl, fonts, scale, images)?;
                let (width, height) = raster_size(dl, scale).map(|(w, h)| (u32::from(w), u32::from(h)))?;
                let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
                if self.target.as_ref().is_none_or(|t| t.size != size) {
                    let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("scaena target"),
                        size,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
                        view_formats: &[],
                    });
                    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                    self.target = Some(Target { size, texture, view });
                }
                let target = self.target.as_ref().expect("made above");
                let params = vello::RenderParams {
                    base_color: peniko::Color::TRANSPARENT,
                    width,
                    height,
                    antialiasing_method: vello::AaConfig::Area,
                };
                self.renderer
                    .render_to_texture(&self.device, &self.queue, &scene, &target.view, &params)
                    .map_err(gpu)?;

                // A texture-to-buffer copy pads each row to 256 bytes. The queue runs in
                // order, so the next frame paints the target only once this copy is done.
                let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
                let padded = (width * 4).div_ceil(align) * align;
                let bytes = u64::from(padded) * u64::from(height);
                let buffer = match self.spare.iter().position(|b| b.size() == bytes) {
                    Some(i) => self.spare.swap_remove(i),
                    None => self.readback(bytes),
                };
                let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                encoder.copy_texture_to_buffer(
                    target.texture.as_image_copy(),
                    wgpu::TexelCopyBufferInfo {
                        buffer: &buffer,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(padded),
                            rows_per_image: None,
                        },
                    },
                    size,
                );
                let submitted = self.queue.submit([encoder.finish()]);
                let (tx, mapped) = std::sync::mpsc::channel();
                buffer.map_async(wgpu::MapMode::Read, .., move |done| {
                    let _ = tx.send(done);
                });
                Ok(Flight { buffer, width, height, padded, submitted, mapped })
            }

            /// Wait for `flight` to be back, then its pixels.
            fn finish(&mut self, flight: Flight) -> Result<Raster, PaintError> {
                let Flight { buffer, width, height, padded, submitted, mapped } = flight;
                // Bounded: a GPU that never finishes is an error to report, not a hang.
                let wait = wgpu::PollType::Wait { submission_index: Some(submitted), timeout: Some(GPU_TIMEOUT) };
                self.device.poll(wait).map_err(gpu)?;
                mapped
                    .recv_timeout(GPU_TIMEOUT)
                    .map_err(|_| PaintError::Gpu(format!("readback not done after {GPU_TIMEOUT:?}")))?
                    .map_err(gpu)?;
                let row = width as usize * 4;
                let mut rgba = Vec::with_capacity(row * height as usize);
                for line in buffer.get_mapped_range(..).chunks_exact(padded as usize) {
                    rgba.extend_from_slice(&line[..row]);
                }
                buffer.unmap();
                // Kept for the frames after it, which are the same size, or dropped.
                self.spare.retain(|b| b.size() == buffer.size());
                self.spare.push(buffer);
                // vello unpremultiplies as rgb / max(a, 1e-6), so a sliver of coverage below
                // 1/255 reads back as alpha 0 with leftover colour (Metal leaves [2, 1, 1, 0]
                // beside pixel-aligned edges). Fully transparent is fully transparent.
                for px in rgba.as_chunks_mut::<4>().0 {
                    if px[3] == 0 {
                        *px = [0; 4];
                    }
                }
                Ok(Raster { width, height, rgba })
            }
        }

        /// The longest the painter waits for one frame's work and readback.
        const GPU_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

        fn gpu(e: impl std::fmt::Display) -> PaintError {
            PaintError::Gpu(e.to_string())
        }

        /// wgpu's native futures are ready on their first poll (wgpu-core wraps its
        /// results in `std::future::ready`), so no executor is needed. Anything else is
        /// reported, not waited on.
        fn ready<T>(future: impl std::future::Future<Output = T>) -> Result<T, PaintError> {
            let mut future = std::pin::pin!(future);
            let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
            match future.as_mut().poll(&mut cx) {
                std::task::Poll::Ready(value) => Ok(value),
                std::task::Poll::Pending => {
                    Err(PaintError::Gpu("a wgpu future did not complete on its first poll".into()))
                }
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use scaena_core::displaylist::{Blend, Color, Paint, Path};

            /// A painter, or `None` with the reason printed on a machine without an
            /// adapter. `SCAENA_REQUIRE_GPU=1` (set in CI) makes that a failure instead.
            fn painter() -> Option<GpuPainter> {
                match GpuPainter::new() {
                    Ok(p) => Some(p),
                    Err(e) if std::env::var_os("SCAENA_REQUIRE_GPU").is_none() => {
                        eprintln!("skipping: {e} (set SCAENA_REQUIRE_GPU=1 to fail instead)");
                        None
                    }
                    Err(e) => panic!("SCAENA_REQUIRE_GPU is set: {e}"),
                }
            }

            #[test]
            fn vello_fills_pixel_aligned_rect_exactly() {
                const ACCENT: [u8; 4] = [0xFF, 0x6A, 0x3D, 0xFF];
                let Some(mut gpu) = painter() else { return };
                let mut dl = DisplayList::new([4.0, 4.0]);
                dl.ops.push(Op::Fill {
                    path: Path::rect([0.0, 0.0, 2.0, 2.0]),
                    rule: FillRule::NonZero,
                    paint: Paint::Solid(Color(ACCENT)),
                });
                let raster = gpu.paint(&dl, &Assets::new(), 1.0).unwrap();
                for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    assert_eq!(raster.pixel(x, y), ACCENT, "inside ({x},{y}) on {:?}", gpu.adapter());
                }
                for (x, y) in [(2, 0), (0, 2), (2, 2), (3, 3)] {
                    assert_eq!(raster.pixel(x, y), [0; 4], "outside ({x},{y}) on {:?}", gpu.adapter());
                }
            }

            #[test]
            fn frames_sent_come_back_in_order_as_paint_paints_them() {
                let Some(mut gpu) = painter() else { return };
                let fill = |c: [u8; 4], w: f32| {
                    let mut dl = DisplayList::new([w, 3.0]);
                    dl.ops.push(Op::Fill {
                        path: Path::rect([0.0, 0.0, w, 3.0]),
                        rule: FillRule::NonZero,
                        paint: Paint::Solid(Color(c)),
                    });
                    dl
                };
                // The last is wider: its rows are padded otherwise, and it needs a new target.
                let frames = [fill([255, 0, 0, 255], 5.0), fill([0, 0, 255, 128], 5.0), fill([0, 200, 0, 255], 70.0)];
                let painted: Vec<Raster> = frames.iter().map(|f| gpu.paint(f, &Assets::new(), 2.0).unwrap()).collect();
                assert!(gpu.receive().unwrap().is_none(), "nothing was sent");
                for f in &frames {
                    gpu.send(f, &Assets::new(), 2.0).unwrap();
                }
                // A frame painted meanwhile is its own, and leaves the ones sent waiting.
                let between = gpu.paint(&frames[1], &Assets::new(), 2.0).unwrap();
                assert_eq!(between.rgba, painted[1].rgba);
                for want in &painted {
                    let got = gpu.receive().unwrap().expect("a frame sent and not received");
                    assert_eq!((got.width, got.height), (want.width, want.height));
                    assert!(got.rgba == want.rgba, "a frame differs from paint's on {:?}", gpu.adapter());
                }
                assert!(gpu.receive().unwrap().is_none(), "every frame sent came back once");
            }

            #[test]
            fn a_shader_op_paints_where_the_cpu_painter_paints_it() {
                let Some(mut gpu) = painter() else { return };
                let mut dl = DisplayList::new([50.0, 30.0]);
                dl.ops.push(Op::Layer {
                    node: None,
                    cell: None,
                    transform: [1.0, 0.0, 0.0, 1.0, 5.0, 2.0],
                    opacity: 1.0,
                    blend: Blend::Normal,
                    clip: None,
                    ops: vec![crate::cpu::tests::mesh_op([5.0, 3.0, 30.0, 20.0])],
                });
                let cpu = crate::cpu::CpuPainter::default().paint(&dl, &Assets::new(), 2.0).unwrap();
                let raster = gpu.paint(&dl, &Assets::new(), 2.0).unwrap();
                // The WGSL may round a channel the other way where a value sits on a
                // threshold; the texture lands texel for pixel or the steps would be large.
                let worst = cpu.rgba.iter().zip(&raster.rgba).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
                assert!(worst <= 1, "max channel step {worst} on {:?}", gpu.adapter());
                assert_eq!(raster.pixel(19, 10), [0; 4], "nothing left of the box");
            }

            /// A frame past what this GPU holds is an error, as one past [`MAX_PIXELS`] is: a
            /// target wider than its textures, or a shader whose pixels outgrow a storage
            /// buffer. wgpu's default error handler panics on either, which stopped `render
            /// --painter gpu` past 8192 pixels a side (PLAN 2.26).
            #[test]
            fn a_frame_past_what_the_gpu_holds_is_an_error() {
                let Some(mut gpu) = painter() else { return };
                let limits = gpu.device.limits();
                let side = limits.max_texture_dimension_2d as f32;
                // A strip wider than a texture, with few pixels in all.
                let strip = DisplayList::new([side + 64.0, 8.0]);
                match gpu.paint(&strip, &Assets::new(), 1.0) {
                    Err(PaintError::Size(w, _)) => assert_eq!(w, side + 64.0),
                    other => panic!("a target {} pixels wide: {:?}", side + 64.0, other.map(|r| r.width)),
                }
                // A square of shader that a raster holds, whose pixels in rows of 256 bytes
                // outgrow a storage buffer of wgpu's default size, 128 MiB: on a GPU whose
                // buffers hold it, it paints.
                let n = 5792u32;
                let mut shaded = DisplayList::new([n as f32; 2]);
                shaded.ops.push(crate::cpu::tests::mesh_op([0.0, 0.0, n as f32, n as f32]));
                let storage = limits.max_buffer_size.min(limits.max_storage_buffer_binding_size);
                let row = u64::from(
                    (n * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT,
                );
                assert!(u64::from(n) * u64::from(n) <= MAX_PIXELS && row * u64::from(n) > 128 << 20);
                let fits = row * u64::from(n) <= storage && n <= limits.max_texture_dimension_2d;
                match (gpu.paint(&shaded, &Assets::new(), 1.0), fits) {
                    (Ok(raster), true) => assert_eq!((raster.width, raster.height), (n, n)),
                    (Err(PaintError::Size(..)), false) => {}
                    (other, _) => {
                        panic!("{n}² pixels of shader, {storage} bytes a buffer: {:?}", other.map(|r| r.width))
                    }
                }
                // The painter paints on.
                let small = gpu.paint(&DisplayList::new([4.0, 4.0]), &Assets::new(), 1.0).unwrap();
                assert_eq!((small.width, small.height), (4, 4));
            }

            #[test]
            fn a_gradient_paint_runs_in_oklab_as_the_cpu_painter_runs_it() {
                let Some(mut gpu) = painter() else { return };
                let stop = |at: f32, c: [u8; 4]| scaena_core::displaylist::Stop(at, Color(c));
                let mut dl = DisplayList::new([256.0, 4.0]);
                dl.ops.push(Op::Fill {
                    path: Path::rect([0.0, 0.0, 256.0, 4.0]),
                    rule: FillRule::NonZero,
                    paint: Paint::Linear {
                        start: [0.0, 0.0],
                        end: [256.0, 0.0],
                        stops: vec![
                            stop(0.0, [255, 0, 0, 255]),
                            stop(0.6, [0, 0, 255, 255]),
                            stop(1.0, [0, 0, 0, 255]),
                        ],
                    },
                });
                let cpu = crate::cpu::CpuPainter::default().paint(&dl, &Assets::new(), 1.0).unwrap();
                let raster = gpu.paint(&dl, &Assets::new(), 1.0).unwrap();
                // vello's GPU ramp blends stops in sRGB: given only the three, its middle
                // would be sRGB's dark purple, some 60 steps from Oklab's violet.
                let worst = cpu.rgba.iter().zip(&raster.rgba).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
                assert!(worst <= 3, "max channel step {worst} on {:?}", gpu.adapter());
                let mid = raster.pixel(77, 2);
                assert!(mid[1] > 60 && mid[0] > 120 && mid[2] > 140, "{mid:?}");
            }

            #[test]
            fn scale_maps_canvas_units_to_pixels_and_layers_compose() {
                let Some(mut gpu) = painter() else { return };
                let mut dl = DisplayList::new([4.0, 4.0]);
                let square = |a: u8| Op::Fill {
                    path: Path::rect([0.0, 0.0, 1.0, 1.0]),
                    rule: FillRule::NonZero,
                    paint: Paint::Solid(Color([0, 0, 0, a])),
                };
                dl.ops.push(Op::Layer {
                    node: None,
                    cell: None,
                    transform: [1.0, 0.0, 0.0, 1.0, 2.0, 2.0],
                    opacity: 1.0,
                    blend: Blend::Normal,
                    clip: None,
                    ops: vec![square(255)],
                });
                // An isolated layer (opacity < 1) takes the unclipped-layer path.
                dl.ops.push(Op::Layer {
                    node: None,
                    cell: None,
                    transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                    opacity: 0.5,
                    blend: Blend::Normal,
                    clip: None,
                    ops: vec![square(255)],
                });
                let raster = gpu.paint(&dl, &Assets::new(), 2.0).unwrap();
                assert_eq!((raster.width, raster.height), (8, 8));
                // The unit square at (2, 2) cu covers pixels 4..6 at 2 px per cu.
                assert_eq!(raster.pixel(4, 4), [0, 0, 0, 255]);
                assert_eq!(raster.pixel(5, 5), [0, 0, 0, 255]);
                assert_eq!(raster.pixel(3, 3)[3], 0);
                assert_eq!(raster.pixel(6, 6)[3], 0);
                // Half opacity, give or take one 8-bit step of rounding.
                assert!(raster.pixel(1, 1)[3].abs_diff(128) <= 1, "{:?}", raster.pixel(1, 1));
            }
        }
    }
}
