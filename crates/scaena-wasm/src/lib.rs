//! # scaena-wasm
//!
//! The engine in the browser (PLAN 0.8; SPEC §9.2 grows this into the player). A page
//! hands over a bundle's `deck.json` and theme, then its other files (fonts, images, data)
//! by their paths in the bundle, then asks for frames:
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
//!   its engine, applies a finding's fix, and inspects a state (PLAN 2.3, [`editor`]). It
//!   opens a bundle from its files or a `.scaena` zip and saves it as `scaena save` does,
//!   fonts subset in the module (PLAN 2.4, `store`). Without it (`--no-default-features
//!   --features gpu,cpu`), the module is the player's alone, a fifth smaller: the one a
//!   single-file HTML export carries (PLAN 2.5).
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
use std::collections::BTreeMap;
use wasm_bindgen::prelude::*;

#[cfg(feature = "editor")]
pub mod assistant;
#[cfg(feature = "editor")]
pub mod editor;
#[cfg(feature = "editor")]
mod store;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("deck.json: {0}")]
    Deck(String),
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error(transparent)]
    Paint(#[from] PaintError),
    #[error("the deck names {0}, which is not in the bundle: hand it over with addFile")]
    Missing(String),
    #[error("compile a source before linting or fixing it")]
    NothingCompiled,
    #[error("{0}")]
    Ops(String),
    /// An operation the assistant called stopped (PLAN 2.6): why, as its MCP tool says it.
    #[cfg(feature = "editor")]
    #[error("{}", .0.message)]
    Tool(scaena_ops::OpsError),
}

/// One bundle's engine: the deck, its theme, every file of the bundle handed over, and the
/// layout engine built from the fonts and images the deck names.
pub struct Session {
    deck: Deck,
    theme: Theme,
    /// Every file of the bundle handed over, by its path inside it: the deck's as opened,
    /// the theme's, fonts, images, data, and any other (a font's license, the history).
    /// The engine and its data are drawn from it as the deck names them; validation finds
    /// the bundle's files in it, and a save writes the deck as edited into it (PLAN 2.4).
    files: BTreeMap<String, Vec<u8>>,
    /// The data files the deck names, as the engine reads them.
    data: DataFiles,
    /// The layout engine, built on the first frame from the fonts and images the deck
    /// names, and again on the first frame after it names others.
    engine: Option<Engine>,
    /// The transition last sampled, so the frames of one transition lay out once.
    transition: Option<(String, Transition)>,
    /// The format frames are laid out in (SPEC §3.4); `None` for the deck's own canvas.
    format: Option<String>,
    /// The fonts and images the engine was built from, as painters read them.
    store: Assets,
    /// The theme's JSON, as handed over: what lint and a save read (PLAN 2.3–2.4).
    #[cfg(feature = "editor")]
    theme_json: String,
    /// The source the editor compiled last (PLAN 2.3).
    #[cfg(feature = "editor")]
    edit: Option<editor::Edit>,
    /// What the layout rules found in every state the last time they ran on all of them:
    /// kept for the states a lint of one state does not lay out.
    #[cfg(feature = "editor")]
    laid: Vec<scaena_core::Finding>,
    /// Fonts subset by the page's subsetter for a save, by path: the characters each keeps,
    /// and its bytes (PLAN 2.4).
    #[cfg(feature = "editor")]
    subsets: BTreeMap<String, (String, Vec<u8>)>,
}

/// The files a deck's engine is built from: its fonts and its images.
fn drawn_from(deck: &Deck) -> (Vec<&str>, Vec<String>) {
    (deck.fonts.iter().map(|f| f.file.as_str()).collect(), deck.image_files())
}

/// The files a deck reads its data from: its sources that name one.
fn read_from(deck: &Deck) -> Vec<&str> {
    deck.data.values().filter_map(|s| s.source.as_str()).collect()
}

impl Session {
    pub fn new(deck_json: &str, theme_json: &str) -> Result<Self, Error> {
        let deck = Deck::from_json(deck_json).map_err(|e| Error::Deck(e.to_string()))?;
        let mut files = BTreeMap::from([("deck.json".to_string(), deck_json.as_bytes().to_vec())]);
        if let Some(path) = deck.theme.as_ref().and_then(|t| t.as_str()) {
            files.insert(path.to_string(), theme_json.as_bytes().to_vec());
        }
        Ok(Self {
            deck,
            theme: Theme::from_json(theme_json)?,
            files,
            data: DataFiles::new(),
            engine: None,
            transition: None,
            format: None,
            store: Assets::new(),
            #[cfg(feature = "editor")]
            theme_json: theme_json.to_string(),
            #[cfg(feature = "editor")]
            edit: None,
            #[cfg(feature = "editor")]
            laid: Vec::new(),
            #[cfg(feature = "editor")]
            subsets: BTreeMap::new(),
        })
    }

    /// Show `deck` from now on, with the files already handed over: an edit (PLAN 2.3). A
    /// format the deck no longer lists falls back to its own canvas. A deck that names other
    /// fonts or images builds the engine again on the next frame.
    pub fn set_deck(&mut self, deck: Deck) {
        if self.format.as_ref().is_some_and(|f| !deck.formats.contains(f)) {
            self.format = None;
        }
        if drawn_from(&deck) != drawn_from(&self.deck) {
            self.engine = None;
        }
        if read_from(&deck) != read_from(&self.deck) {
            self.data = DataFiles::new();
            for path in read_from(&deck) {
                if let Some(bytes) = self.files.get(path) {
                    self.data.insert(path, bytes.clone());
                }
            }
        }
        self.deck = deck;
        self.transition = None;
    }

    /// Hand over a file of the bundle by its path inside it (for a font, the deck's
    /// `fonts[].file`; for an image, its nodes' `src`; for data, its sources'): any file,
    /// at any time. One the deck draws with builds the engine again on the next frame;
    /// one it names nowhere waits until it does, and a save carries it.
    pub fn add_file(&mut self, path: &str, bytes: Vec<u8>) {
        let (fonts, images) = drawn_from(&self.deck);
        if fonts.contains(&path) || images.iter().any(|p| p == path) {
            self.engine = None;
        }
        if read_from(&self.deck).contains(&path) {
            self.data.insert(path, bytes.clone());
            self.transition = None;
        }
        self.files.insert(path.to_string(), bytes);
    }

    /// The image files the deck names: each to hand over with [`Session::add_file`].
    pub fn image_files(&self) -> Vec<String> {
        self.deck.image_files()
    }

    /// Every file of the bundle the session holds, by its path inside it, sorted.
    pub fn files(&self) -> Vec<String> {
        self.files.keys().cloned().collect()
    }

    /// The file at `path` in the bundle, as it was handed over.
    pub fn file(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }

    /// Build the engine from the fonts and images the deck names, if it is not built yet.
    fn build(&mut self) -> Result<&mut Engine, Error> {
        if self.engine.is_none() {
            let files = &self.files;
            let file = |path: &str| files.get(path).ok_or_else(|| Error::Missing(path.to_string()));
            let (mut fonts, mut images, mut store) = (BundleFonts::new(), BundleImages::new(), Assets::new());
            for font in &self.deck.fonts {
                let bytes = file(&font.file)?;
                fonts.register(&font.file, bytes.clone())?;
                store.insert_font(&font.file, bytes.clone());
            }
            for path in self.deck.image_files() {
                let bytes = file(&path)?;
                let info = images.register(&path, bytes)?;
                store.insert_image(&info.id, bytes)?;
            }
            fonts.check_theme(&self.theme)?;
            self.engine = Some(Engine::new(fonts).with_images(images));
            self.store = store;
            self.transition = None;
        }
        Ok(self.engine.as_mut().expect("built above"))
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

    /// The fonts and images the engine was built from, as painters read them.
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

    /// Hand over a file of the bundle by its path inside it: a font, an image, a data
    /// file, or any other. At any time: one the deck draws with builds the engine again on
    /// the next frame.
    #[wasm_bindgen(js_name = addFile)]
    pub fn add_file(&mut self, path: &str, bytes: Vec<u8>) {
        self.0.add_file(path, bytes);
    }

    /// The image files the deck names, each to hand over with `addFile`.
    #[wasm_bindgen(js_name = imageFiles)]
    pub fn image_files(&self) -> Vec<String> {
        self.0.image_files()
    }

    /// Every file of the bundle the session holds, by its path inside it, sorted.
    pub fn files(&self) -> Vec<String> {
        self.0.files()
    }

    /// The file at `path` in the bundle, as it was handed over.
    pub fn file(&self, path: &str) -> Option<Vec<u8>> {
        self.0.file(path).map(<[u8]>::to_vec)
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

/// The bundle a page opens, edits, and saves (PLAN 2.4, SPEC §9.2).
#[cfg(feature = "editor")]
#[wasm_bindgen]
impl Player {
    /// A `.scaena` zip's bytes, opened.
    #[wasm_bindgen(js_name = fromZip)]
    pub fn from_zip(bytes: &[u8]) -> Result<Player, JsError> {
        Session::from_zip(bytes).map(Player).map_err(js)
    }

    /// What a save that subsets needs subset, as JSON: `{ chars, fonts }`, the characters
    /// the deck can draw and each font file to keep them of. The page subsets each with the
    /// subsetter's own module (`scaena-subset`) and hands it back with `addSubset`.
    pub fn subsetting(&self) -> Result<String, JsError> {
        let (chars, fonts) = self.0.subsetting().map_err(js)?;
        Ok(serde_json::json!({ "chars": chars, "fonts": fonts }).to_string())
    }

    /// `font`, subset to `chars` (as `subsetting` gave them), for the next save that subsets.
    #[wasm_bindgen(js_name = addSubset)]
    pub fn add_subset(&mut self, font: &str, chars: &str, bytes: Vec<u8>) {
        self.0.add_subset(font, chars, bytes);
    }

    /// The bundle with the deck shown, saved as `scaena save` saves one (SPEC §3.1), at
    /// `now` (RFC 3339), with fonts subset to what the deck can draw if `subset`: each from
    /// `addSubset`, for the characters the deck can draw now.
    pub fn save(&self, now: &str, subset: bool) -> Result<SavedBundle, JsError> {
        self.0.save(now, subset).map(SavedBundle).map_err(js)
    }

    /// Go on from `saved`, once the page has written it where it keeps the bundle: its
    /// files and its deck, which names them by their content. The source is the saved
    /// deck's from then on.
    pub fn adopt(&mut self, saved: &SavedBundle) -> Result<(), JsError> {
        self.0.adopt(&saved.0).map_err(js)
    }

    /// Where a file dropped on the page goes in the bundle: a font under `fonts/` and a
    /// data file under `data/`, by its name; anything else, an image above all, under
    /// `assets/`, named by its SHA-256, as a save names it.
    pub fn place(name: &str, bytes: &[u8]) -> String {
        scaena_store::place(name, bytes)
    }
}

/// The assistant's tools (PLAN 2.6, SPEC §11): MCP's operations on this bundle.
#[cfg(feature = "editor")]
#[wasm_bindgen]
impl Player {
    /// The tools the assistant has, by their MCP names.
    #[wasm_bindgen(js_name = toolNames)]
    pub fn tool_names() -> Vec<String> {
        assistant::TOOLS.iter().map(|t| t.to_string()).collect()
    }

    /// Call the tool `name` with `args` (JSON), as its MCP tool takes them less `bundle`,
    /// `out`, and `painter`. A tool that stops says why in the result, as an MCP tool's
    /// error result does, rather than throwing: either way it is what the model is told.
    pub fn tool(&mut self, name: &str, args: &str) -> ToolResult {
        let called = match serde_json::from_str(args) {
            Ok(args) => self.0.tool(name, args),
            Err(e) => Err(Error::Ops(format!("{name}: the arguments are not JSON: {e}"))),
        };
        match called {
            Ok(c) => ToolResult { json: c.result, error: false, edited: c.edited, frame: c.frame },
            Err(e) => {
                let json = assistant::failure(&e).to_string();
                ToolResult { json, error: true, edited: false, frame: None }
            }
        }
    }
}

/// What a tool returned (PLAN 2.6).
#[cfg(feature = "editor")]
#[wasm_bindgen]
pub struct ToolResult {
    json: String,
    error: bool,
    edited: bool,
    frame: Option<scaena_paint::Raster>,
}

#[cfg(feature = "editor")]
#[wasm_bindgen]
impl ToolResult {
    /// What the tool returned, as JSON: its MCP tool's result, or why it stopped.
    #[wasm_bindgen(getter)]
    pub fn json(&self) -> String {
        self.json.clone()
    }

    /// Whether it stopped: `json` then says why (`{ message, plan?, op? }`).
    #[wasm_bindgen(getter)]
    pub fn error(&self) -> bool {
        self.error
    }

    /// Whether it changed the deck: the page takes the source again.
    #[wasm_bindgen(getter)]
    pub fn edited(&self) -> bool {
        self.edited
    }

    /// `deck_render`'s frame, `[width, height]` pixels.
    #[wasm_bindgen(getter)]
    pub fn size(&self) -> Option<Vec<u32>> {
        self.frame.as_ref().map(|r| vec![r.width, r.height])
    }

    /// `deck_render`'s frame as RGBA, for an `ImageData`; empty from any other tool.
    pub fn pixels(&self) -> wasm_bindgen::Clamped<Vec<u8>> {
        wasm_bindgen::Clamped(self.frame.as_ref().map(|r| r.rgba.clone()).unwrap_or_default())
    }
}

/// A save, in memory: the files of the saved bundle, by their paths inside it.
#[cfg(feature = "editor")]
#[wasm_bindgen]
pub struct SavedBundle(scaena_store::Saving);

#[cfg(feature = "editor")]
#[wasm_bindgen]
impl SavedBundle {
    /// Every file of the saved bundle, by its path inside it, sorted.
    pub fn paths(&self) -> Vec<String> {
        self.0.files.keys().cloned().collect()
    }

    pub fn file(&self, path: &str) -> Option<Vec<u8>> {
        self.0.files.get(path).cloned()
    }

    /// The files of the bundle as it was that the save renamed or rewrote: where the save
    /// replaces the bundle it came from, those `paths` does not hold go.
    pub fn replaced(&self) -> Vec<String> {
        self.0.replaced.iter().cloned().collect()
    }

    /// What the save did, as JSON: `{ renamed: [[from, to]], subset: [[font, before,
    /// after]], manifest }`, sizes in bytes.
    pub fn summary(&self) -> Result<String, JsError> {
        serde_json::to_string(&self.0.saved).map_err(js)
    }

    /// The saved bundle as a `.scaena` zip's bytes, the same for the same bundle.
    pub fn zip(&self) -> Result<Vec<u8>, JsError> {
        scaena_store::zip(&self.0.files).map_err(js)
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
            s.add_file(&id, std::fs::read(format!("{BUNDLE}/{id}")).unwrap());
        }
        for path in ["data/bars.csv", "data/bars-next.csv"] {
            s.add_file(path, std::fs::read(format!("{BUNDLE}/{path}")).unwrap());
        }
        for path in s.image_files() {
            s.add_file(&path, std::fs::read(format!("{BUNDLE}/{path}")).unwrap());
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
    fn a_deck_that_draws_with_other_files_builds_the_engine_again() {
        let mut s = torture();
        let drawn = s.frame("images", f64::INFINITY).unwrap().digest().unwrap();
        // A file the deck does not name waits: the engine stands.
        let card = std::fs::read(format!("{BUNDLE}/assets/test-card.png")).unwrap();
        s.add_file("assets/copy.png", card);
        assert!(s.engine.is_some());
        // A deck that shows it builds the engine again, and draws it: the same picture.
        let rename = |deck: &Deck, from: &str, to: &str| {
            let json = deck.to_json().unwrap().replace(from, to);
            Deck::from_json(&json).unwrap()
        };
        s.set_deck(rename(&s.deck, "assets/test-card.png", "assets/copy.png"));
        assert!(s.engine.is_none());
        assert_eq!(s.frame("images", f64::INFINITY).unwrap().digest().unwrap(), drawn);
        // One it names that was never handed over is named in the error.
        s.set_deck(rename(&s.deck, "assets/copy.png", "assets/absent.png"));
        let err = s.frame("images", f64::INFINITY).unwrap_err();
        assert!(matches!(&err, Error::Missing(p) if p == "assets/absent.png"), "{err}");
        assert_eq!(s.states().len(), 47);
    }
}
