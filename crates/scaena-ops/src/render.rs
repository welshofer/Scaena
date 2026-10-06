//! Render a state (PLAN 0.6, 0.7): the bundle's fonts → `Engine::frame` → a painter → PNG.
//! Timings are wall clock here, in the client; the render path never reads a clock.

use crate::lint::data_files;
use crate::{Context, OpsError};
use scaena_core::displaylist::DisplayList;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::images::BundleImages;
use scaena_engine::{Engine, FrameRequest};
use scaena_paint::Assets;
use scaena_paint::cpu::CpuPainter;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Instant;

/// Which painter draws the frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Painter {
    /// `vello_cpu`: every machine, the goldens' painter.
    #[default]
    Cpu,
    /// vello on the GPU, in a build with the `gpu` feature (PLAN 0.7).
    Gpu,
}

/// What to render.
#[derive(Debug, Clone, Default)]
pub struct Request {
    pub state: String,
    /// Ms into the state's cue (its transition, then its motions); at rest without it.
    pub t: Option<f64>,
    /// One of the deck's `formats`, laid out again with the theme's template set for it.
    pub format: Option<String>,
    /// `WxH` pixels, the canvas's aspect ratio; the canvas's size without it.
    pub size: Option<String>,
    pub painter: Painter,
}

/// A rendered frame, and what each stage took.
#[derive(Debug, Clone)]
pub struct Rendered {
    pub png: Vec<u8>,
    pub display_list: DisplayList,
    /// The display list's digest (`DisplayList::digest`): one digest, one drawing.
    pub digest: String,
    /// The state's span: its transition and motions, ms.
    pub span_ms: f64,
    /// Pixels: width, height.
    pub size: [u32; 2],
    pub painter: &'static str,
    /// The GPU adapter, for the GPU painter.
    pub adapter: Option<String>,
    pub ms: Timings,
}

/// What each stage of a render took, ms, wall clock (SPEC §15).
#[derive(Debug, Clone, Copy, Default, Serialize, JsonSchema)]
pub struct Timings {
    /// Opening the bundle: the deck, the theme, the fonts' and data's bytes.
    pub load: f64,
    /// Registering the fonts and images.
    pub fonts: f64,
    /// Layout and sampling: the display list.
    pub frame: f64,
    /// Starting the painter.
    pub init: f64,
    pub paint: f64,
    /// Encoding the PNG.
    pub png: f64,
    pub total: f64,
}

/// Render `req.state` of the bundle at `path`.
pub fn render(path: &Path, req: &Request) -> Result<Rendered, OpsError> {
    if let Some(t) = req.t.filter(|t| !t.is_finite() || *t < 0.0) {
        return Err(OpsError::new(format!("--t {t}: expected a finite, non-negative number of milliseconds")));
    }
    let start = Instant::now();
    let mut lap = {
        let mut last = start;
        move || {
            let now = Instant::now();
            let ms = (now - last).as_secs_f64() * 1e3;
            last = now;
            ms
        }
    };
    let b = crate::open(path)?;
    let theme = crate::theme(&b)?;
    let files = b.read_fonts()?;
    let data = data_files(&b)?;
    let load = lap();

    let mut fonts = BundleFonts::new();
    let mut store = Assets::new();
    for (id, bytes) in files {
        store.insert_font(&id, bytes.clone());
        fonts.register(&id, bytes)?;
    }
    fonts.check_theme(&theme)?;
    let mut images = BundleImages::new();
    for (path, bytes) in b.read_images()? {
        let info = images.register(&path, &bytes)?;
        store.insert_image(&info.id, &bytes)?;
    }
    let register = lap();

    let frame_req = FrameRequest {
        deck: &b.deck,
        theme: &theme,
        data: &data,
        state: &req.state,
        t_ms: req.t.unwrap_or(f64::INFINITY),
        format: req.format.as_deref(),
    };
    let frame = Engine::new(fonts).with_images(images).frame(&frame_req)?;
    let (dl, span_ms) = (frame.display_list, frame.duration_ms);
    let digest = dl.digest().map_err(|e| OpsError::new(e.to_string()))?;
    let layout = lap();

    let scale = match req.size.as_deref() {
        Some(size) => scale_for(size, dl.viewport)?,
        None => 1.0,
    };
    // Built after the frame, so a state the engine cannot draw costs no GPU start-up.
    let (mut painter, adapter): (Box<dyn scaena_paint::Painter>, Option<String>) = match req.painter {
        Painter::Cpu => (Box::new(CpuPainter::default()), None),
        #[cfg(feature = "gpu")]
        Painter::Gpu => {
            let gpu = scaena_paint::gpu::GpuPainter::new()?;
            let info = gpu.adapter();
            let adapter = format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type);
            (Box::new(gpu), Some(adapter))
        }
        #[cfg(not(feature = "gpu"))]
        Painter::Gpu => {
            return Err(OpsError::not_built("`--painter gpu` needs a build with `--features gpu` (PLAN 0.7)", "0.7"));
        }
    };
    let init = lap();
    let raster = painter.paint(&dl, &store, scale)?;
    let paint = lap();
    let png = raster.to_png_fast()?;
    let encode = lap();
    let total = (Instant::now() - start).as_secs_f64() * 1e3;
    let us = |ms: f64| (ms * 1e3).round() / 1e3;
    Ok(Rendered {
        png,
        display_list: dl,
        digest,
        span_ms,
        size: [raster.width, raster.height],
        painter: painter.name(),
        adapter,
        ms: Timings {
            load: us(load),
            fonts: us(register),
            frame: us(layout),
            init: us(init),
            paint: us(paint),
            png: us(encode),
            total: us(total),
        },
    })
}

/// `WxH` → output pixels per canvas unit. The size must have the canvas's aspect ratio,
/// to the nearest pixel: painters scale uniformly and never stretch.
pub fn scale_for(size: &str, canvas: [f32; 2]) -> Result<f32, OpsError> {
    let parsed = size.split_once('x').and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)));
    let (w, h) = parsed
        .filter(|&(w, h)| w > 0 && h > 0)
        .with_context(|| format!("--size `{size}`: expected WxH, like 1920x1080"))?;
    let scale = w as f32 / canvas[0];
    if (canvas[1] * scale).round() != h as f32 {
        return Err(OpsError::new(format!(
            "--size {w}x{h} does not have the canvas's aspect ratio ({}x{} cu)",
            canvas[0], canvas[1]
        )));
    }
    Ok(scale)
}
