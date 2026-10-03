//! # scaena-wasm
//!
//! The engine in the browser (PLAN 0.8; SPEC §9.2 grows this into the player). A page
//! hands over a bundle's `deck.json`, theme, fonts, and data files, then asks for
//! frames:
//!
//! - [`Player::frame`] returns a state's display list, postcard-encoded (SPEC §6).
//!   Native and WASM builds of the engine must produce the same bytes; the smoke
//!   check in `www/` compares them with the native goldens' digests.
//! - `Player::paint` draws it into a canvas with `vello` on WebGPU ([`Canvas`]),
//!   through `scaena_paint::gpu::scene`, the scene the native GPU painter renders. The
//!   canvas is a page's, or an `OffscreenCanvas` a worker paints (PLAN 2.1).
//! - [`Player::pixels`] paints it with `vello_cpu`, the painter the goldens hold, into
//!   RGBA pixels: the web player's fallback where WebGPU is missing (SPEC §9.2).
//!
//! - With the `editor` feature, a session compiles `.scn` as it is typed, lints it with
//!   its engine, applies a finding's fix, and inspects a state (PLAN 2.3, [`editor`]).
//!
//! [`Session`] is the same engine surface in plain Rust, so it is tested natively.

use scaena_core::Deck;
use scaena_core::displaylist::DisplayList;
use scaena_core::timeline::Timeline;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::images::BundleImages;
use scaena_engine::sample::Transition;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, EngineError, FrameRequest, project};
use scaena_paint::{Assets, PaintError};
use wasm_bindgen::prelude::*;

#[cfg(feature = "editor")]
pub mod editor;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("deck.json: {0}")]
    Deck(String),
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error(transparent)]
    Paint(#[from] PaintError),
    #[error("add every font and image before the first frame: the engine is built from them then")]
    AfterFrame,
    #[error("compile a source before linting or fixing it")]
    NothingCompiled,
    #[error("{0}")]
    Ops(String),
}

/// One bundle's engine: the deck, its theme, fonts, and data files, and the layout
/// engine built from them on the first frame.
pub struct Session {
    deck: Deck,
    theme: Theme,
    data: DataFiles,
    /// Fonts and images registered so far; the engine takes them on the first frame.
    pending: Option<(BundleFonts, BundleImages)>,
    engine: Option<Engine>,
    /// The transition last sampled, so the frames of one transition lay out once.
    transition: Option<(String, Transition)>,
    /// The format frames are laid out in (SPEC §3.4); `None` for the deck's own canvas.
    format: Option<String>,
    /// The same fonts and images, as painters read them.
    store: Assets,
    /// The theme's file, as the deck names it, and its JSON; and the fonts and images
    /// handed over, by path: what validation finds in the bundle.
    theme_path: Option<String>,
    theme_json: String,
    font_paths: Vec<String>,
    image_paths: Vec<String>,
    /// The source the editor compiled last (PLAN 2.3).
    #[cfg(feature = "editor")]
    edit: Option<editor::Edit>,
    /// What the layout rules found in every state the last time they ran on all of them:
    /// kept for the states a lint of one state does not lay out.
    #[cfg(feature = "editor")]
    laid: Vec<scaena_core::Finding>,
}

impl Session {
    pub fn new(deck_json: &str, theme_json: &str) -> Result<Self, Error> {
        let deck = Deck::from_json(deck_json).map_err(|e| Error::Deck(e.to_string()))?;
        let theme_path = deck.theme.as_ref().and_then(|t| t.as_str()).map(str::to_string);
        Ok(Self {
            deck,
            theme: Theme::from_json(theme_json)?,
            data: DataFiles::new(),
            pending: Some((BundleFonts::new(), BundleImages::new())),
            engine: None,
            transition: None,
            format: None,
            store: Assets::new(),
            theme_path,
            theme_json: theme_json.to_string(),
            font_paths: Vec::new(),
            image_paths: Vec::new(),
            #[cfg(feature = "editor")]
            edit: None,
            #[cfg(feature = "editor")]
            laid: Vec::new(),
        })
    }

    /// Show `deck` from now on, with the fonts, images, and data already handed over: an
    /// edit (PLAN 2.3). A format the deck no longer lists falls back to its own canvas.
    pub fn set_deck(&mut self, deck: Deck) {
        if self.format.as_ref().is_some_and(|f| !deck.formats.contains(f)) {
            self.format = None;
        }
        self.deck = deck;
        self.transition = None;
    }

    /// Build the engine from the fonts and images handed over, if it is not built yet.
    fn build(&mut self) -> Result<&mut Engine, Error> {
        if self.engine.is_none() {
            let (fonts, images) = self.pending.take().ok_or(Error::AfterFrame)?;
            fonts.check_theme(&self.theme)?;
            self.engine = Some(Engine::new(fonts).with_images(images));
        }
        Ok(self.engine.as_mut().expect("built above"))
    }

    /// Register a font file under its bundle id (its path in the bundle, as the deck's
    /// `fonts[].file` names it).
    pub fn add_font(&mut self, id: &str, bytes: Vec<u8>) -> Result<(), Error> {
        let (fonts, _) = self.pending.as_mut().ok_or(Error::AfterFrame)?;
        fonts.register(id, bytes.clone())?;
        self.store.insert_font(id, bytes);
        self.font_paths.push(id.to_string());
        Ok(())
    }

    /// The image files the deck names: each to add with [`Session::add_image`].
    pub fn image_files(&self) -> Vec<String> {
        self.deck.image_files()
    }

    /// Register an image file under its bundle path, as image nodes' `src` names it.
    pub fn add_image(&mut self, path: &str, bytes: Vec<u8>) -> Result<(), Error> {
        let (_, images) = self.pending.as_mut().ok_or(Error::AfterFrame)?;
        let info = images.register(path, &bytes)?;
        self.store.insert_image(&info.id, &bytes)?;
        self.image_paths.push(path.to_string());
        Ok(())
    }

    /// Register a data file under its bundle path, as the deck's `data.*.source` names it.
    pub fn add_data(&mut self, path: &str, bytes: Vec<u8>) {
        self.data.insert(path, bytes);
    }

    pub fn states(&self) -> Vec<String> {
        self.deck.states.iter().map(|s| s.id.clone()).collect()
    }

    /// The formats the deck is also laid out in, as it writes them (`9:16`).
    pub fn formats(&self) -> Vec<String> {
        self.deck.formats.clone()
    }

    /// Lay frames out in `format`, one of the deck's `formats` (SPEC §3.4), or on the
    /// deck's own canvas (`None`). Timelines and frames from then on are in it.
    pub fn set_format(&mut self, format: Option<&str>) -> Result<(), Error> {
        project(&self.deck, &self.theme, format)?;
        if self.format.as_deref() != format {
            self.format = format.map(str::to_string);
            self.transition = None;
        }
        Ok(())
    }

    /// The canvas frames are laid out on, `[width, height]` canvas units: the deck's, or
    /// its format's.
    pub fn canvas_size(&self) -> Result<[f64; 2], Error> {
        let (deck, _) = project(&self.deck, &self.theme, self.format.as_deref())?;
        Ok([deck.canvas.width, deck.canvas.height])
    }

    /// The deck's states end to end, ms (SPEC §2.4): each state's start, its span (its
    /// transition and motions), and its hold. Builds the engine, as a frame does.
    pub fn timeline(&mut self) -> Result<Timeline, Error> {
        self.build()?;
        let (deck, theme) = project(&self.deck, &self.theme, self.format.as_deref())?;
        let engine = self.engine.as_mut().expect("built above");
        Ok(engine.timeline(deck.as_ref(), theme.as_ref(), &self.data)?)
    }

    /// The span of `state`, ms: its transition and every motion of its cue. Past it, the
    /// state is at rest.
    pub fn duration(&mut self, state: &str) -> Result<f64, Error> {
        let timeline = self.timeline()?;
        Ok(timeline.slot(state).ok_or_else(|| EngineError::UnknownState(state.to_string()))?.span)
    }

    /// The display list for `state`, `t_ms` into its cue (`f64::INFINITY`: at rest). At
    /// rest, the state laid out alone. Inside its cue, a sample of it, laid out on the
    /// first such frame and kept: the frames of one cue lay out once (SPEC §5).
    pub fn frame(&mut self, state: &str, t_ms: f64) -> Result<DisplayList, Error> {
        // The engine is built by now: the span took it.
        let span = self.duration(state)?;
        let engine = self.engine.as_mut().expect("built for the span");
        let format = self.format.as_deref();
        if t_ms.is_nan() || t_ms >= span {
            let req = FrameRequest { deck: &self.deck, theme: &self.theme, data: &self.data, state, t_ms, format };
            return Ok(engine.frame(&req)?.display_list);
        }
        let cached = self.transition.as_ref().is_some_and(|(s, _)| s == state);
        if !cached {
            let (deck, theme) = project(&self.deck, &self.theme, format)?;
            let transition = engine.transition(&deck, &theme, &self.data, state)?;
            self.transition = Some((state.to_string(), transition));
        }
        Ok(self.transition.as_ref().expect("set above").1.frame(t_ms))
    }

    /// The fonts and images, as painters read them.
    pub fn assets(&self) -> &Assets {
        &self.store
    }

    /// `state` at `t_ms`, painted by the CPU painter `width` pixels wide; the height keeps
    /// the canvas's aspect.
    #[cfg(feature = "cpu")]
    pub fn pixels(&mut self, state: &str, t_ms: f64, width: u32) -> Result<scaena_paint::Raster, Error> {
        use scaena_paint::Painter;
        let dl = self.frame(state, t_ms)?;
        let scale = width as f32 / dl.viewport[0];
        Ok(scaena_paint::cpu::CpuPainter::default().paint(&dl, &self.store, scale)?)
    }
}

fn js(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

/// [`Session`] for JavaScript.
#[wasm_bindgen]
pub struct Player(Session);

#[wasm_bindgen]
impl Player {
    #[wasm_bindgen(constructor)]
    pub fn new(deck_json: &str, theme_json: &str) -> Result<Player, JsError> {
        Session::new(deck_json, theme_json).map(Player).map_err(js)
    }

    #[wasm_bindgen(js_name = addFont)]
    pub fn add_font(&mut self, id: &str, bytes: Vec<u8>) -> Result<(), JsError> {
        self.0.add_font(id, bytes).map_err(js)
    }

    #[wasm_bindgen(js_name = addData)]
    pub fn add_data(&mut self, path: &str, bytes: Vec<u8>) {
        self.0.add_data(path, bytes);
    }

    /// The image files the deck names, each to add with `addImage`.
    #[wasm_bindgen(js_name = imageFiles)]
    pub fn image_files(&self) -> Vec<String> {
        self.0.image_files()
    }

    #[wasm_bindgen(js_name = addImage)]
    pub fn add_image(&mut self, path: &str, bytes: Vec<u8>) -> Result<(), JsError> {
        self.0.add_image(path, bytes).map_err(js)
    }

    pub fn states(&self) -> Vec<String> {
        self.0.states()
    }

    /// The formats the deck is also laid out in (`9:16`).
    pub fn formats(&self) -> Vec<String> {
        self.0.formats()
    }

    /// Lay frames out in one of the deck's formats, or on its own canvas (`undefined`).
    #[wasm_bindgen(js_name = setFormat)]
    pub fn set_format(&mut self, format: Option<String>) -> Result<(), JsError> {
        self.0.set_format(format.as_deref()).map_err(js)
    }

    /// The canvas frames are laid out on, `[width, height]`: the deck's, or its format's.
    #[wasm_bindgen(js_name = canvasSize)]
    pub fn canvas_size(&self) -> Result<Vec<f64>, JsError> {
        self.0.canvas_size().map(Vec::from).map_err(js)
    }

    /// The span of `state`, ms: its transition and its motions. Past it, at rest.
    pub fn duration(&mut self, state: &str) -> Result<f64, JsError> {
        self.0.duration(state).map_err(js)
    }

    /// The deck's states end to end, as JSON: `[{ "state", "start", "span", "hold" }]`,
    /// ms (SPEC §2.4). What a player auto-advances by, and a video samples.
    pub fn timeline(&mut self) -> Result<String, JsError> {
        let slots: Vec<serde_json::Value> = (self.0.timeline().map_err(js)?.slots.iter())
            .map(|s| serde_json::json!({ "state": s.state, "start": s.start, "span": s.span, "hold": s.hold }))
            .collect();
        serde_json::to_string(&slots).map_err(js)
    }

    /// The display list for `state` at `t_ms` (`Infinity`: at rest), postcard-encoded.
    pub fn frame(&mut self, state: &str, t_ms: f64) -> Result<Vec<u8>, JsError> {
        self.0.frame(state, t_ms).map_err(js)?.to_postcard().map_err(js)
    }

    /// `state` at `t_ms` (`Infinity`: at rest), painted by `vello_cpu` `width` pixels wide,
    /// the height keeping the canvas's aspect: straight-alpha sRGB, four bytes a pixel, row
    /// by row, as `new ImageData(pixels, width)` takes them.
    #[cfg(feature = "cpu")]
    pub fn pixels(&mut self, state: &str, t_ms: f64, width: u32) -> Result<wasm_bindgen::Clamped<Vec<u8>>, JsError> {
        Ok(wasm_bindgen::Clamped(self.0.pixels(state, t_ms, width).map_err(js)?.rgba))
    }
}

/// The source editor (PLAN 2.3): every result as JSON, as `editor`'s types serialize.
#[cfg(feature = "editor")]
#[wasm_bindgen]
impl Player {
    /// The deck as canonical `.scn`: what the editor opens on.
    pub fn source(&self) -> String {
        self.0.source()
    }

    /// Compile `source`: `{ error?, findings, states, valid }`, each place in it in UTF-16
    /// offsets. A deck that validates is what frames show from now on.
    pub fn compile(&mut self, source: &str) -> Result<String, JsError> {
        serde_json::to_string(&self.0.compile(source)).map_err(js)
    }

    /// Lint the deck compiled last, laid out by this engine: `{ findings, laid, whole }`.
    /// With `state`, the layout rules run on that state alone, the one being edited, and
    /// the other states keep what they found when they last ran on every state.
    pub fn lint(&mut self, state: Option<String>) -> Result<String, JsError> {
        serde_json::to_string(&self.0.lint(state.as_deref()).map_err(js)?).map_err(js)
    }

    /// The source compiled last with `patch` (JSON: a finding's `fix`) applied.
    pub fn fix(&self, patch: &str) -> Result<String, JsError> {
        let patch: Vec<serde_json::Value> = serde_json::from_str(patch).map_err(js)?;
        self.0.fix(&patch).map_err(js)
    }

    /// `state` inspected: its nodes resolved, each text node's look, what its overrides
    /// set, and its cue.
    pub fn inspect(&mut self, state: &str) -> Result<String, JsError> {
        serde_json::to_string(&self.0.inspect(state).map_err(js)?).map_err(js)
    }
}

#[cfg(all(feature = "gpu", target_arch = "wasm32"))]
pub use web::Canvas;

#[cfg(all(feature = "gpu", target_arch = "wasm32"))]
mod web {
    use super::*;
    use vello::wgpu;

    /// Panics and GPU errors go to the browser console; by default both are silent.
    #[wasm_bindgen(start)]
    pub fn start() {
        std::panic::set_hook(Box::new(|panic| web_sys::console::error_1(&panic.to_string().into())));
    }

    /// A canvas `vello` paints through WebGPU. vello renders into a storage texture
    /// (canvas textures cannot be storage-bound in every format), and a blit copies it
    /// to the canvas.
    #[wasm_bindgen]
    pub struct Canvas {
        device: wgpu::Device,
        queue: wgpu::Queue,
        surface: wgpu::Surface<'static>,
        renderer: vello::Renderer,
        target: wgpu::TextureView,
        blitter: wgpu::util::TextureBlitter,
        size: (u32, u32),
        /// How the surface is configured, kept to configure it again at another size.
        config: wgpu::SurfaceConfiguration,
        adapter: String,
        shaders: scaena_paint::gpu::Shaders,
    }

    /// The texture vello draws a frame into, `size` pixels.
    fn target(device: &wgpu::Device, size: (u32, u32)) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("scaena target"),
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

    #[wasm_bindgen]
    impl Canvas {
        /// WebGPU on `canvas`, at its `width` × `height` attributes.
        pub async fn attach(canvas: web_sys::HtmlCanvasElement) -> Result<Canvas, JsError> {
            let size = (canvas.width(), canvas.height());
            Canvas::on(wgpu::SurfaceTarget::Canvas(canvas), size).await
        }

        /// WebGPU on an `OffscreenCanvas`, at its `width` × `height`: in a worker, a page's
        /// canvas handed over with `transferControlToOffscreen` (PLAN 2.1).
        #[wasm_bindgen(js_name = attachOffscreen)]
        pub async fn attach_offscreen(canvas: web_sys::OffscreenCanvas) -> Result<Canvas, JsError> {
            let size = (canvas.width(), canvas.height());
            Canvas::on(wgpu::SurfaceTarget::OffscreenCanvas(canvas), size).await
        }

        /// Paint at `width` × `height` pixels from now on, as the canvas element's
        /// attributes have just been set: another format's canvas (SPEC §3.4).
        pub fn resize(&mut self, width: u32, height: u32) {
            if (width, height) == self.size {
                return;
            }
            (self.config.width, self.config.height) = (width, height);
            self.surface.configure(&self.device, &self.config);
            self.target = target(&self.device, (width, height));
            self.size = (width, height);
        }

        /// Which adapter paints: name, backend, device type.
        #[wasm_bindgen(getter)]
        pub fn adapter(&self) -> String {
            self.adapter.clone()
        }
    }

    impl Canvas {
        /// WebGPU on `canvas`, `size` pixels.
        async fn on(canvas: wgpu::SurfaceTarget<'static>, size: (u32, u32)) -> Result<Canvas, JsError> {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::BROWSER_WEBGPU,
                ..wgpu::InstanceDescriptor::new_without_display_handle()
            });
            let surface = instance.create_surface(canvas).map_err(js)?;
            let options = wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            };
            let adapter = instance.request_adapter(&options).await.map_err(js)?;
            let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.map_err(js)?;
            device
                .on_uncaptured_error(std::sync::Arc::new(|e| web_sys::console::error_1(&format!("wgpu: {e}").into())));
            let caps = surface.get_capabilities(&adapter);
            let format = caps
                .formats
                .iter()
                .copied()
                .find(|f| matches!(f, wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm))
                .ok_or_else(|| JsError::new("the canvas offers neither rgba8unorm nor bgra8unorm"))?;
            let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) {
                wgpu::CompositeAlphaMode::Opaque
            } else {
                caps.alpha_modes[0]
            };
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: size.0,
                height: size.1,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
                alpha_mode,
                view_formats: vec![],
            };
            surface.configure(&device, &config);
            let renderer = vello::Renderer::new(
                &device,
                vello::RendererOptions {
                    use_cpu: false,
                    antialiasing_support: vello::AaSupport::area_only(),
                    num_init_threads: None,
                    pipeline_cache: None,
                },
            )
            .map_err(js)?;
            let target = target(&device, size);
            let blitter = wgpu::util::TextureBlitter::new(&device, format);
            let info = adapter.get_info();
            // Browsers withhold adapter names; say what is known.
            let adapter = match info.name.as_str() {
                "" => format!("{:?}", info.backend),
                name => format!("{name} ({:?}, {:?})", info.backend, info.device_type),
            };
            let shaders = scaena_paint::gpu::Shaders::new();
            Ok(Canvas { device, queue, surface, renderer, target, blitter, size, config, adapter, shaders })
        }
    }

    #[wasm_bindgen]
    impl Player {
        /// Paint `state` at `t_ms` into `canvas`, scaled to the canvas width.
        pub fn paint(&mut self, canvas: &mut Canvas, state: &str, t_ms: f64) -> Result<(), JsError> {
            let dl = self.0.frame(state, t_ms).map_err(js)?;
            let (width, height) = canvas.size;
            let scale = width as f32 / dl.viewport[0];
            // Shader ops run as compute passes into textures first; the scene draws them.
            let jobs = scaena_paint::shader_jobs(&dl, scale).map_err(js)?;
            let images = canvas.shaders.prepare(&canvas.device, &canvas.queue, &mut canvas.renderer, &jobs);
            let params = vello::RenderParams {
                base_color: vello::peniko::Color::TRANSPARENT,
                width,
                height,
                antialiasing_method: vello::AaConfig::Area,
            };
            let drawn = scaena_paint::gpu::scene(&dl, self.0.assets(), scale, &images).map_err(js).and_then(|scene| {
                canvas
                    .renderer
                    .render_to_texture(&canvas.device, &canvas.queue, &scene, &canvas.target, &params)
                    .map_err(js)
            });
            scaena_paint::gpu::Shaders::release(&mut canvas.renderer, images);
            drawn?;
            let frame = match canvas.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
                _ => return Err(JsError::new("the canvas has no texture to draw into")),
            };
            let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = canvas.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            canvas.blitter.copy(&canvas.device, &mut encoder, &canvas.target, &view);
            canvas.queue.submit([encoder.finish()]);
            frame.present();
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

    fn torture() -> Session {
        let read = |p: &str| std::fs::read_to_string(format!("{BUNDLE}/{p}")).unwrap();
        let mut s = Session::new(&read("deck.json"), &read("theme.json")).unwrap();
        for f in [
            "RobotoSerif-VF.ttf",
            "EBGaramond-VF.ttf",
            "NotoSansHebrew-VF.ttf",
            "NotoSansArabic-VF.ttf",
            "NotoColorEmoji-COLRv1.ttf",
        ] {
            let id = format!("fonts/{f}");
            s.add_font(&id, std::fs::read(format!("{BUNDLE}/{id}")).unwrap()).unwrap();
        }
        for path in ["data/bars.csv", "data/bars-next.csv"] {
            s.add_data(path, std::fs::read(format!("{BUNDLE}/{path}")).unwrap());
        }
        for path in s.image_files() {
            s.add_image(&path, std::fs::read(format!("{BUNDLE}/{path}")).unwrap()).unwrap();
        }
        s
    }

    /// Set `s` up for the golden frame `name`, and say which state it is and when: `state`
    /// at rest, or `state@fraction` of the transition into it, each in the deck's own canvas
    /// or, after `~`, in a format (`9x16` for `9:16`).
    fn golden(s: &mut Session, name: &str) -> (String, f64) {
        let (frame, format) = match name.split_once('~') {
            Some((frame, format)) => (frame, Some(format.replace('x', ":"))),
            None => (name, None),
        };
        s.set_format(format.as_deref()).unwrap();
        match frame.split_once('@') {
            Some((state, at)) => (state.to_string(), at.parse::<f64>().unwrap() * s.duration(state).unwrap()),
            None => (frame.to_string(), f64::INFINITY),
        }
    }

    /// The session is the engine the native tests drive: its frames hash to the
    /// native goldens' digests (`raw.fnv1a`). The browser smoke check compares the
    /// WASM build against the same file.
    #[test]
    fn frames_match_the_native_raw_digests() {
        let mut s = torture();
        let expected = std::fs::read_to_string("../../tests/golden/torture/raw.fnv1a").unwrap();
        for line in expected.lines() {
            let (name, digest) = line.split_once(' ').unwrap();
            let (state, t) = golden(&mut s, name);
            assert_eq!(s.frame(&state, t).unwrap().digest().unwrap(), digest, "{name}");
        }
    }

    /// The CPU painter paints a session's frames as the golden rasters hold them (SPEC
    /// §13.5), from the files a page hands over: images, color glyphs, shaders, a frame of a
    /// transition, and another format's canvas.
    #[cfg(feature = "cpu")]
    #[test]
    fn pixels_match_the_golden_rasters() {
        use scaena_paint::{Raster, diff};
        let mut s = torture();
        for name in ["images", "emoji", "shaders", "morph@0.5", "formats~9x16"] {
            let png = std::fs::read(format!("../../tests/golden/torture/{name}.png")).unwrap();
            let expected = Raster::from_png(&png).unwrap();
            let (state, t) = golden(&mut s, name);
            let got = s.pixels(&state, t, expected.width).unwrap();
            assert_eq!((got.width, got.height), (expected.width, expected.height), "{name}");
            let d = diff::compare(&expected, &got).unwrap();
            assert!(d.passes(), "{name}: {d}");
        }
    }

    #[test]
    fn fonts_and_images_come_before_the_first_frame() {
        let mut s = torture();
        s.frame("axes", f64::INFINITY).unwrap();
        let err = s.add_font("fonts/late.ttf", vec![]).unwrap_err();
        assert!(matches!(err, Error::AfterFrame), "{err}");
        let err = s.add_image("assets/late.png", vec![]).unwrap_err();
        assert!(matches!(err, Error::AfterFrame), "{err}");
        assert_eq!(s.states().len(), 47);
    }
}
