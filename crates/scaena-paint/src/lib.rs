//! # scaena-paint
//!
//! Painters consume a [`scaena_core::displaylist::DisplayList`] and produce pixels.
//! They never shape text or lay anything out (SPEC §6).
//!
//! - `cpu` (default feature): `vello_cpu`: headless, deterministic; CI, agents, export.
//! - `gpu`: `vello` on `wgpu`: WebGPU in the browser, Metal on the Mac, Vulkan on Linux.
//!
//! A painter draws only what the engine emits and the tests exercise: solid fills,
//! strokes, glyph runs (outline, COLR, and bitmap glyphs), and layers. Gradients,
//! images, and shaders return `NotImplemented` naming the PLAN task that adds them.
//! Both painters take their geometry from the same conversions (`convert`), so they
//! can differ only in rasterization.

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
    #[error("gpu: {0}")]
    Gpu(String),
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

/// Display-list types to `kurbo`/`peniko`, shared by both painters so they receive
/// identical geometry (ADR-0004: one `kurbo`, one `peniko` in the graph).
#[cfg(any(feature = "cpu", feature = "gpu"))]
mod convert {
    use crate::PaintError;
    use kurbo::{Affine, BezPath, Stroke};
    use peniko::{BlendMode, Color, Mix};
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

    pub fn solid(paint: &Paint) -> Result<Color, PaintError> {
        match paint {
            Paint::Solid(c) => Ok(Color::from_rgba8(c.0[0], c.0[1], c.0[2], c.0[3])),
            _ => Err(PaintError::NotImplemented("gradient paints — PLAN 1.10")),
        }
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
    use crate::convert::{affine, bez, isolated, mix, solid, stroke};
    use scaena_core::displaylist::{FillRule, Op};
    use vello_cpu::kurbo::Affine;
    use vello_cpu::peniko::Fill;
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
                        self.ctx.set_transform(xf);
                        self.ctx.set_stroke(stroke(*width, *cap, *join, *miter_limit, dash, *dash_offset));
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
                    Op::Image { .. } => return Err(PaintError::NotImplemented("image ops — PLAN 1.7")),
                    Op::Shader { .. } => return Err(PaintError::NotImplemented("shader ops — PLAN 0.11")),
                }
            }
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
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
    //! `vello` on `wgpu` (PLAN 0.7).
    //!
    //! [`scene`] turns a display list into a vello scene on every target, wasm32
    //! included: the browser draws it to a WebGPU canvas (PLAN 0.8), the Mac to a
    //! `CAMetalLayer`. Surfaces are the client's job. [`GpuPainter`] is the native
    //! headless painter: it renders into a texture and reads it back, which cannot
    //! block in a browser, so it is not built for wasm32.

    use super::*;
    use crate::convert::{affine, bez, isolated, mix, solid, stroke};
    use scaena_core::displaylist::{FillRule, Op};
    use vello::Scene;
    use vello::kurbo::{Affine, Rect};
    use vello::peniko::Fill;

    /// The vello scene for `dl` at `scale` output pixels per canvas unit. Ops map one to
    /// one onto `CpuPainter`'s calls, through the same conversions.
    pub fn scene(dl: &DisplayList, fonts: &FontStore, scale: f32) -> Result<Scene, PaintError> {
        check_version(dl)?;
        let (width, height) = raster_size(dl, scale)?;
        let mut cx = Cx {
            scene: Scene::new(),
            store: fonts,
            fonts: &dl.fonts,
            // vello layers always clip; an unclipped layer clips to the whole output.
            output: Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
        };
        cx.ops(&dl.ops, Affine::scale(f64::from(scale)))?;
        Ok(cx.scene)
    }

    struct Cx<'a> {
        scene: Scene,
        store: &'a FontStore,
        fonts: &'a [FontRef],
        output: Rect,
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
                        self.scene.fill(rule, xf, solid(paint)?, None, &bez(path));
                    }
                    Op::Stroke { path, paint, width, cap, join, miter_limit, dash, dash_offset } => {
                        let style = stroke(*width, *cap, *join, *miter_limit, dash, *dash_offset);
                        self.scene.stroke(&style, xf, solid(paint)?, None, &bez(path));
                    }
                    Op::Glyphs { font, size, coords, paint, glyphs } => {
                        let font_ref = self.fonts.get(*font as usize).ok_or(PaintError::FontIndex(*font))?;
                        let font = self.store.get(font_ref)?;
                        let brush = solid(paint)?;
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
                                .brush(brush)
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
                                None => self.scene.push_layer(
                                    Fill::NonZero,
                                    mix(*blend),
                                    *opacity,
                                    Affine::IDENTITY,
                                    &self.output,
                                ),
                            }
                            self.ops(ops, child)?;
                            self.scene.pop_layer();
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

    #[cfg(test)]
    mod scene_tests {
        use super::*;
        use scaena_core::displaylist::{Paint, Path};

        #[test]
        fn scene_needs_no_device_and_names_unimplemented_paints() {
            let mut dl = DisplayList::new([4.0, 4.0]);
            dl.ops.push(Op::Fill {
                path: Path::rect([0.0, 0.0, 1.0, 1.0]),
                rule: FillRule::NonZero,
                paint: Paint::Solid(scaena_core::displaylist::Color([0, 0, 0, 255])),
            });
            assert!(scene(&dl, &FontStore::new(), 1.0).is_ok());
            dl.ops.push(Op::Fill {
                path: Path::rect([0.0, 0.0, 1.0, 1.0]),
                rule: FillRule::NonZero,
                paint: Paint::Linear { start: [0.0, 0.0], end: [1.0, 0.0], stops: vec![] },
            });
            let err = scene(&dl, &FontStore::new(), 1.0).err().unwrap();
            assert!(err.to_string().contains("PLAN 1.10"), "{err}");
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub use native::GpuPainter;

    #[cfg(not(target_arch = "wasm32"))]
    mod native {
        use super::*;
        use vello::wgpu;

        /// Headless `vello` (PLAN 0.7): renders into an `Rgba8Unorm` texture and reads it
        /// back. vello writes straight alpha (it unpremultiplies before storing), which
        /// is what [`Raster`] holds.
        pub struct GpuPainter {
            device: wgpu::Device,
            queue: wgpu::Queue,
            renderer: vello::Renderer,
            adapter: wgpu::AdapterInfo,
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
                let descriptor = wgpu::DeviceDescriptor { label: Some("scaena"), ..Default::default() };
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
                Ok(Self { device, queue, renderer, adapter })
            }

            /// Which adapter paints: name, backend, and device type (a CPU adapter such
            /// as lavapipe reports `Cpu`).
            pub fn adapter(&self) -> &wgpu::AdapterInfo {
                &self.adapter
            }
        }

        impl Painter for GpuPainter {
            fn name(&self) -> &'static str {
                "gpu"
            }

            fn paint(&mut self, dl: &DisplayList, fonts: &FontStore, scale: f32) -> Result<Raster, PaintError> {
                let scene = scene(dl, fonts, scale)?;
                let (width, height) = raster_size(dl, scale).map(|(w, h)| (u32::from(w), u32::from(h)))?;
                let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
                let target = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("scaena target"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let params = vello::RenderParams {
                    base_color: peniko::Color::TRANSPARENT,
                    width,
                    height,
                    antialiasing_method: vello::AaConfig::Area,
                };
                let view = target.create_view(&wgpu::TextureViewDescriptor::default());
                self.renderer.render_to_texture(&self.device, &self.queue, &scene, &view, &params).map_err(gpu)?;

                // A texture-to-buffer copy pads each row to 256 bytes.
                let row = width * 4;
                let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
                let padded = row.div_ceil(align) * align;
                let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("scaena readback"),
                    size: u64::from(padded) * u64::from(height),
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                encoder.copy_texture_to_buffer(
                    target.as_image_copy(),
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
                self.queue.submit([encoder.finish()]);
                let (tx, rx) = std::sync::mpsc::channel();
                buffer.map_async(wgpu::MapMode::Read, .., move |mapped| {
                    let _ = tx.send(mapped);
                });
                self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(gpu)?;
                rx.recv().map_err(|_| PaintError::Gpu("readback callback never ran".into()))?.map_err(gpu)?;
                let mut rgba = Vec::with_capacity(row as usize * height as usize);
                for line in buffer.get_mapped_range(..).chunks_exact(padded as usize) {
                    rgba.extend_from_slice(&line[..row as usize]);
                }
                buffer.unmap();
                Ok(Raster { width, height, rgba })
            }
        }

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
                let raster = gpu.paint(&dl, &FontStore::new(), 1.0).unwrap();
                for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    assert_eq!(raster.pixel(x, y), ACCENT, "inside ({x},{y}) on {:?}", gpu.adapter());
                }
                for (x, y) in [(2, 0), (0, 2), (2, 2), (3, 3)] {
                    assert_eq!(raster.pixel(x, y), [0; 4], "outside ({x},{y}) on {:?}", gpu.adapter());
                }
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
                    transform: [1.0, 0.0, 0.0, 1.0, 2.0, 2.0],
                    opacity: 1.0,
                    blend: Blend::Normal,
                    clip: None,
                    ops: vec![square(255)],
                });
                // An isolated layer (opacity < 1) takes the unclipped-layer path.
                dl.ops.push(Op::Layer {
                    node: None,
                    transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                    opacity: 0.5,
                    blend: Blend::Normal,
                    clip: None,
                    ops: vec![square(255)],
                });
                let raster = gpu.paint(&dl, &FontStore::new(), 2.0).unwrap();
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
