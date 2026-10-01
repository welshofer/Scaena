//! # scaena-wasm
//!
//! The engine in the browser (PLAN 0.8; SPEC §9.2 grows this into the player). A page
//! hands over a bundle's `deck.json`, theme, and fonts, then asks for frames:
//!
//! - [`Player::frame`] returns a state's display list, postcard-encoded (SPEC §6).
//!   Native and WASM builds of the engine must produce the same bytes; the smoke
//!   check in `www/` compares them with the native goldens' digests.
//! - `Player::paint` draws it into a canvas with `vello` on WebGPU ([`Canvas`]),
//!   through `scaena_paint::gpu::scene`, the scene the native GPU painter renders.
//!
//! [`Session`] is the same engine surface in plain Rust, so it is tested natively.

use scaena_core::Deck;
use scaena_core::displaylist::DisplayList;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, EngineError, FrameRequest};
use scaena_paint::{FontStore, PaintError};
use wasm_bindgen::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("deck.json: {0}")]
    Deck(String),
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error(transparent)]
    Paint(#[from] PaintError),
    #[error("add every font before the first frame: the engine is built from them then")]
    FontAfterFrame,
}

/// One bundle's engine: the deck, its theme and fonts, and the layout engine built
/// from them on the first frame.
pub struct Session {
    deck: Deck,
    theme: Theme,
    /// Fonts registered so far; the engine takes them on the first frame.
    pending: Option<BundleFonts>,
    engine: Option<Engine>,
    /// The same fonts, as painters read them.
    store: FontStore,
}

impl Session {
    pub fn new(deck_json: &str, theme_json: &str) -> Result<Self, Error> {
        Ok(Self {
            deck: Deck::from_json(deck_json).map_err(|e| Error::Deck(e.to_string()))?,
            theme: Theme::from_json(theme_json)?,
            pending: Some(BundleFonts::new()),
            engine: None,
            store: FontStore::new(),
        })
    }

    /// Register a font file under its bundle id (its path in the bundle, as the deck's
    /// `fonts[].file` names it).
    pub fn add_font(&mut self, id: &str, bytes: Vec<u8>) -> Result<(), Error> {
        let fonts = self.pending.as_mut().ok_or(Error::FontAfterFrame)?;
        fonts.register(id, bytes.clone())?;
        self.store.insert(id, bytes);
        Ok(())
    }

    pub fn states(&self) -> Vec<String> {
        self.deck.states.iter().map(|s| s.id.clone()).collect()
    }

    /// The display list for `state`, `t_ms` into its transition (`f64::INFINITY`: at rest).
    pub fn frame(&mut self, state: &str, t_ms: f64) -> Result<DisplayList, Error> {
        if self.engine.is_none() {
            let fonts = self.pending.take().ok_or(Error::FontAfterFrame)?;
            fonts.check_theme(&self.theme)?;
            self.engine = Some(Engine::new(fonts));
        }
        let engine = self.engine.as_mut().expect("built above");
        let req = FrameRequest { deck: &self.deck, theme: &self.theme, state, t_ms };
        Ok(engine.frame(&req)?.display_list)
    }

    pub fn fonts(&self) -> &FontStore {
        &self.store
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

    pub fn states(&self) -> Vec<String> {
        self.0.states()
    }

    /// The display list for `state` at `t_ms` (`Infinity`: at rest), postcard-encoded.
    pub fn frame(&mut self, state: &str, t_ms: f64) -> Result<Vec<u8>, JsError> {
        self.0.frame(state, t_ms).map_err(js)?.to_postcard().map_err(js)
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
        adapter: String,
    }

    #[wasm_bindgen]
    impl Canvas {
        /// WebGPU on `canvas`, at its `width` × `height` attributes.
        pub async fn attach(canvas: web_sys::HtmlCanvasElement) -> Result<Canvas, JsError> {
            let size = (canvas.width(), canvas.height());
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::BROWSER_WEBGPU,
                ..wgpu::InstanceDescriptor::new_without_display_handle()
            });
            let surface = instance.create_surface(wgpu::SurfaceTarget::Canvas(canvas)).map_err(js)?;
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
            surface.configure(
                &device,
                &wgpu::SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    format,
                    width: size.0,
                    height: size.1,
                    present_mode: wgpu::PresentMode::Fifo,
                    desired_maximum_frame_latency: 2,
                    alpha_mode,
                    view_formats: vec![],
                },
            );
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
            let target = device
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
                .create_view(&wgpu::TextureViewDescriptor::default());
            let blitter = wgpu::util::TextureBlitter::new(&device, format);
            let info = adapter.get_info();
            // Browsers withhold adapter names; say what is known.
            let adapter = match info.name.as_str() {
                "" => format!("{:?}", info.backend),
                name => format!("{name} ({:?}, {:?})", info.backend, info.device_type),
            };
            Ok(Canvas { device, queue, surface, renderer, target, blitter, size, adapter })
        }

        /// Which adapter paints: name, backend, device type.
        #[wasm_bindgen(getter)]
        pub fn adapter(&self) -> String {
            self.adapter.clone()
        }
    }

    #[wasm_bindgen]
    impl Player {
        /// Paint `state` at `t_ms` into `canvas`, scaled to the canvas width.
        pub fn paint(&mut self, canvas: &mut Canvas, state: &str, t_ms: f64) -> Result<(), JsError> {
            let dl = self.0.frame(state, t_ms).map_err(js)?;
            let (width, height) = canvas.size;
            let scene = scaena_paint::gpu::scene(&dl, self.0.fonts(), width as f32 / dl.viewport[0]).map_err(js)?;
            let params = vello::RenderParams {
                base_color: vello::peniko::Color::TRANSPARENT,
                width,
                height,
                antialiasing_method: vello::AaConfig::Area,
            };
            canvas
                .renderer
                .render_to_texture(&canvas.device, &canvas.queue, &scene, &canvas.target, &params)
                .map_err(js)?;
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
        s
    }

    fn fnv1a(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3))
    }

    /// The session is the engine the native tests drive: its frames hash to the
    /// native goldens' digests (`raw.fnv1a`). The browser smoke check compares the
    /// WASM build against the same file.
    #[test]
    fn frames_match_the_native_raw_digests() {
        let mut s = torture();
        let expected = std::fs::read_to_string("../../tests/golden/torture/raw.fnv1a").unwrap();
        for line in expected.lines() {
            let (state, digest) = line.split_once(' ').unwrap();
            let bytes = s.frame(state, f64::INFINITY).unwrap().to_postcard().unwrap();
            assert_eq!(format!("{:016x}", fnv1a(&bytes)), digest, "{state}");
        }
    }

    #[test]
    fn fonts_come_before_the_first_frame() {
        let mut s = torture();
        s.frame("axes", f64::INFINITY).unwrap();
        let err = s.add_font("fonts/late.ttf", vec![]).unwrap_err();
        assert!(matches!(err, Error::FontAfterFrame), "{err}");
        assert_eq!(s.states().len(), 23);
    }
}
