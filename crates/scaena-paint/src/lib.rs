//! # scaena-paint
//!
//! Painters consume a [`scaena_core::displaylist::DisplayList`] and produce pixels.
//! They never shape text or lay anything out (SPEC §6).
//!
//! - `cpu` (default feature): `vello_cpu`: headless, deterministic; CI, agents, export.
//! - `gpu`: `vello` on `wgpu`: WebGPU in the browser, Metal on the Mac (PLAN 0.7).
//!
//! A painter draws only what the engine emits and the tests exercise: solid fills,
//! strokes, glyph runs (outline, COLR, and bitmap glyphs), and layers. Gradients,
//! images, and shaders return `NotImplemented` naming the PLAN task that adds them.

pub mod diff;

use peniko::{Blob, FontData};
use scaena_core::displaylist::{DL_VERSION, DisplayList, FontRef};
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
    #[error("{0}×{1} px is not a raster this painter can make")]
    Size(f32, f32),
    #[error("png: {0}")]
    Png(String),
    #[error("cannot compare a {}×{} raster with a {}×{} one", a.0, a.1, b.0, b.1)]
    Mismatch { a: (u32, u32), b: (u32, u32) },
}

/// Font bytes by bundle id: loaded once per bundle, shared by every frame (SPEC §6).
#[derive(Debug, Clone, Default)]
pub struct FontStore {
    blobs: BTreeMap<String, Blob<u8>>,
}

impl FontStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add the bytes of the font file with bundle id `id` (its path in the bundle).
    pub fn insert(&mut self, id: &str, bytes: Vec<u8>) {
        self.blobs.insert(id.to_string(), Blob::new(Arc::new(bytes)));
    }

    fn get(&self, font: &FontRef) -> Result<FontData, PaintError> {
        let blob = self.blobs.get(&font.id).ok_or_else(|| PaintError::MissingFont(font.id.clone()))?;
        Ok(FontData::new(blob.clone(), font.index))
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

    /// PNG bytes. The same pixels always encode to the same bytes.
    pub fn to_png(&self) -> Result<Vec<u8>, PaintError> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let png_error = |e: png::EncodingError| PaintError::Png(e.to_string());
        let mut writer = encoder.write_header().map_err(png_error)?;
        writer.write_image_data(&self.rgba).map_err(png_error)?;
        writer.finish().map_err(png_error)?;
        Ok(out)
    }

    /// Decode an 8-bit RGBA PNG (what [`Raster::to_png`] writes).
    pub fn from_png(bytes: &[u8]) -> Result<Raster, PaintError> {
        let png_error = |e: png::DecodingError| PaintError::Png(e.to_string());
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().map_err(png_error)?;
        let info = reader.info();
        if (info.color_type, info.bit_depth) != (png::ColorType::Rgba, png::BitDepth::Eight) {
            return Err(PaintError::Png(format!(
                "expected 8-bit RGBA, got {:?} {:?}",
                info.color_type, info.bit_depth
            )));
        }
        let (width, height) = (info.width, info.height);
        let mut rgba = vec![0; reader.output_buffer_size().unwrap_or(0)];
        reader.next_frame(&mut rgba).map_err(png_error)?;
        Ok(Raster { width, height, rgba })
    }
}

pub trait Painter {
    fn name(&self) -> &'static str;
    /// Paint `dl` at `scale` output pixels per canvas unit.
    fn paint(&mut self, dl: &DisplayList, fonts: &FontStore, scale: f32) -> Result<Raster, PaintError>;
}

/// Output size in whole pixels for `dl` at `scale`.
fn raster_size(dl: &DisplayList, scale: f32) -> Result<(u16, u16), PaintError> {
    let (w, h) = ((dl.viewport[0] * scale).round(), (dl.viewport[1] * scale).round());
    if !(1.0..=f32::from(u16::MAX)).contains(&w) || !(1.0..=f32::from(u16::MAX)).contains(&h) {
        return Err(PaintError::Size(w, h));
    }
    Ok((w as u16, h as u16))
}

fn check_version(dl: &DisplayList) -> Result<(), PaintError> {
    if dl.dl == DL_VERSION { Ok(()) } else { Err(PaintError::Version(dl.dl)) }
}

#[cfg(feature = "cpu")]
pub mod cpu {
    use super::*;
    use scaena_core::displaylist::{Blend, Cap, FillRule, Join, Op, Paint, Path, PathEl};
    use vello_cpu::color::{AlphaColor, Srgb};
    use vello_cpu::kurbo::{self, Affine, BezPath};
    use vello_cpu::peniko::{BlendMode, Fill, Mix};
    pub use vello_cpu::{Level, RenderMode};
    use vello_cpu::{Pixmap, RasterizerSettings, RenderContext, RenderSettings, Resources};

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
    }

    impl Default for CpuPainter {
        fn default() -> Self {
            Self { level: Level::new(), mode: RenderMode::OptimizeSpeed }
        }
    }

    impl Painter for CpuPainter {
        fn name(&self) -> &'static str {
            "cpu"
        }

        fn paint(&mut self, dl: &DisplayList, fonts: &FontStore, scale: f32) -> Result<Raster, PaintError> {
            check_version(dl)?;
            let (width, height) = raster_size(dl, scale)?;
            let mut ctx =
                RenderContext::new_with(width, height, RenderSettings { level: self.level, ..Default::default() });
            let mut resources = Resources::new();
            let mut cx = Cx { ctx: &mut ctx, resources: &mut resources, store: fonts, fonts: &dl.fonts };
            cx.ops(&dl.ops, Affine::scale(f64::from(scale)))?;
            ctx.flush();
            let mut pixmap = Pixmap::new(width, height);
            ctx.render_with(
                &mut pixmap,
                &mut resources,
                RasterizerSettings { render_mode: self.mode, ..Default::default() },
            );
            let rgba = pixmap.take_unpremultiplied().into_iter().flat_map(|p| [p.r, p.g, p.b, p.a]).collect();
            Ok(Raster { width: u32::from(width), height: u32::from(height), rgba })
        }
    }

    struct Cx<'a> {
        ctx: &'a mut RenderContext,
        resources: &'a mut Resources,
        store: &'a FontStore,
        fonts: &'a [FontRef],
    }

    impl Cx<'_> {
        fn ops(&mut self, ops: &[Op], xf: Affine) -> Result<(), PaintError> {
            for op in ops {
                match op {
                    Op::Fill { path, rule, paint } => {
                        self.ctx.set_transform(xf);
                        self.ctx.set_fill_rule(match rule {
                            FillRule::NonZero => Fill::NonZero,
                            FillRule::EvenOdd => Fill::EvenOdd,
                        });
                        self.ctx.set_paint(solid(paint)?);
                        self.ctx.fill_path(&bez(path));
                    }
                    Op::Stroke { path, paint, width, cap, join, miter_limit, dash, dash_offset } => {
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
                        let stroke = kurbo::Stroke::new(f64::from(*width))
                            .with_caps(cap)
                            .with_join(join)
                            .with_miter_limit(f64::from(*miter_limit))
                            .with_dashes(f64::from(*dash_offset), dash.iter().map(|d| f64::from(*d)));
                        self.ctx.set_transform(xf);
                        self.ctx.set_stroke(stroke);
                        self.ctx.set_paint(solid(paint)?);
                        self.ctx.stroke_path(&bez(path));
                    }
                    Op::Glyphs { font, size, coords, paint, glyphs } => {
                        let font_ref = self.fonts.get(*font as usize).ok_or(PaintError::FontIndex(*font))?;
                        let font = self.store.get(font_ref)?;
                        self.ctx.set_transform(xf);
                        self.ctx.set_paint(solid(paint)?);
                        self.ctx
                            .glyph_run(self.resources, &font)
                            .font_size(*size)
                            .normalized_coords(coords)
                            .hint(false)
                            .fill_glyphs(glyphs.iter().map(|g| vello_cpu::Glyph { id: g.id, x: g.x, y: g.y }));
                    }
                    Op::Layer { transform, opacity, blend, clip, ops, .. } => {
                        let child = xf * affine(transform);
                        if *opacity < 1.0 || *blend != Blend::Normal || clip.is_some() {
                            self.ctx.set_transform(child);
                            let clip = clip.as_ref().map(bez);
                            self.ctx.push_layer(clip.as_ref(), Some(mix(*blend)), Some(*opacity), None, None);
                            self.ops(ops, child)?;
                            self.ctx.pop_layer();
                        } else {
                            self.ops(ops, child)?;
                        }
                    }
                    Op::Image { .. } => return Err(PaintError::NotImplemented("image ops — PLAN 1.7")),
                    Op::Shader { .. } => return Err(PaintError::NotImplemented("shader ops — PLAN 0.11")),
                }
            }
            Ok(())
        }
    }

    fn solid(paint: &Paint) -> Result<AlphaColor<Srgb>, PaintError> {
        match paint {
            Paint::Solid(c) => Ok(AlphaColor::from_rgba8(c.0[0], c.0[1], c.0[2], c.0[3])),
            _ => Err(PaintError::NotImplemented("gradient paints — PLAN 1.10")),
        }
    }

    fn mix(blend: Blend) -> BlendMode {
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

    fn affine(m: &scaena_core::displaylist::Affine) -> Affine {
        Affine::new(m.map(f64::from))
    }

    fn bez(path: &Path) -> BezPath {
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

    #[cfg(test)]
    mod tests {
        use super::*;
        use scaena_core::displaylist::{Color, Paint};

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
                let raster =
                    CpuPainter { level, mode: RenderMode::OptimizeSpeed }.paint(&dl, &FontStore::new(), 1.0).unwrap();
                for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    assert_eq!(raster.pixel(x, y), ACCENT, "inside ({x},{y}) at {level:?}");
                }
                for (x, y) in [(2, 0), (0, 2), (2, 2), (3, 3)] {
                    assert_eq!(raster.pixel(x, y), [0; 4], "outside ({x},{y}) at {level:?}");
                }
            }
        }

        #[test]
        fn scale_maps_canvas_units_to_pixels_and_layers_compose() {
            let mut dl = DisplayList::new([4.0, 4.0]);
            dl.ops.push(Op::Layer {
                node: None,
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
            let raster = CpuPainter::default().paint(&dl, &FontStore::new(), 2.0).unwrap();
            assert_eq!((raster.width, raster.height), (8, 8));
            // The unit square at (2, 2) cu covers pixels 4..6 at 2 px per cu.
            assert_eq!(raster.pixel(4, 4), [0, 0, 0, 255]);
            assert_eq!(raster.pixel(5, 5), [0, 0, 0, 255]);
            assert_eq!(raster.pixel(3, 3)[3], 0);
            assert_eq!(raster.pixel(6, 6)[3], 0);
        }

        #[test]
        fn png_round_trips_pixels_exactly() {
            let raster = Raster { width: 2, height: 1, rgba: vec![255, 0, 0, 255, 0, 128, 255, 64] };
            let png = raster.to_png().unwrap();
            assert_eq!(Raster::from_png(&png).unwrap(), raster);
            assert_eq!(raster.to_png().unwrap(), png, "same pixels, same bytes");
        }

        #[test]
        fn unimplemented_ops_name_their_plan_task() {
            let mut dl = DisplayList::new([4.0, 4.0]);
            dl.ops.push(Op::Fill {
                path: Path::rect([0.0, 0.0, 1.0, 1.0]),
                rule: FillRule::NonZero,
                paint: Paint::Linear { start: [0.0, 0.0], end: [1.0, 0.0], stops: vec![] },
            });
            let err = CpuPainter::default().paint(&dl, &FontStore::new(), 1.0).unwrap_err();
            assert!(err.to_string().contains("PLAN 1.10"), "{err}");
        }
    }
}

#[cfg(feature = "gpu")]
pub mod gpu {
    //! `vello` on `wgpu`. Phase 0 task 0.7. Surface creation is the client's job
    //! (OffscreenCanvas/WebGPU in the browser, CAMetalLayer on the Mac).
}
