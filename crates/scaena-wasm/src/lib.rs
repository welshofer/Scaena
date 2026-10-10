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
//!   RGBA pixels: the web player's fallback where WebGPU is missing (SPEC §9.2). A frame
//!   can also be held while its shaders' rows are worked out in bands on other workers,
//!   each with an instance of this module ([`Player::shading`], [`shader_rows`]): the
//!   module has no threads (PLAN 2.28).
//!
//! - With the `editor` feature, a session compiles `.scn` as it is typed, lints it with
//!   its engine, applies a finding's fix, and inspects a state (PLAN 2.3, [`editor`]). It
//!   opens a bundle from its files or a `.scaena` zip and saves it as `scaena save` does,
//!   fonts subset in the module (PLAN 2.4, `store`). Without it (`--no-default-features
//!   --features gpu,cpu`), the module is the player's alone, a fifth smaller: the one a
//!   single-file HTML export carries (PLAN 2.5).
//!
//! [`Session`] is the same engine surface in plain Rust, so it is tested natively.

use scaena_session::*;
use wasm_bindgen::prelude::*;

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

    /// The deck's states end to end, as JSON: `[{ "state", "slide", "start", "span", "hold" }]`,
    /// ms (SPEC §2.4), `slide` the slide each builds on. What a player auto-advances by, and a
    /// video samples.
    pub fn timeline(&mut self) -> Result<String, JsError> {
        serde_json::to_string(&self.0.slots().map_err(js)?).map_err(js)
    }

    /// The display list for `state` at `t_ms` (`Infinity`: at rest), postcard-encoded.
    pub fn frame(&mut self, state: &str, t_ms: f64) -> Result<Vec<u8>, JsError> {
        self.0.frame(state, t_ms).map_err(js)?.to_postcard().map_err(js)
    }

    /// What identifies `state`'s drawing at rest, in the format shown: its display list's
    /// digest (FNV-1a over its postcard bytes, as the goldens keep it). One digest, one
    /// drawing, so a thumbnail painted from it stands until it changes (PLAN 2.35).
    pub fn digest(&mut self, state: &str) -> Result<String, JsError> {
        self.0.frame(state, f64::INFINITY).map_err(js)?.digest().map_err(js)
    }

    /// The link drawn at `x`, `y` (canvas units) in `state` at rest, in the format shown, as JSON
    /// (PLAN 2.70): `{ "href" }` or `{ "state" }`, where a click there goes; `null` off every link.
    #[wasm_bindgen(js_name = linkAt)]
    pub fn link_at(&mut self, state: &str, x: f32, y: f32) -> Result<String, JsError> {
        serde_json::to_string(&self.0.link_at(state, [x, y]).map_err(js)?).map_err(js)
    }

    /// How `state` reads at rest, in the format shown, as HTML (SPEC §3.12): each node it
    /// shows that is read, in paint order, an element that names it (`data-node`).
    pub fn reading(&mut self, state: &str) -> Result<String, JsError> {
        self.0.reading(state).map_err(js)
    }

    /// `state` at `t_ms` (`Infinity`: at rest), painted by `vello_cpu` `width` pixels wide,
    /// the height keeping the canvas's aspect: straight-alpha sRGB, four bytes a pixel, row
    /// by row, as `new ImageData(pixels, width)` takes them.
    #[cfg(feature = "cpu")]
    pub fn pixels(&mut self, state: &str, t_ms: f64, width: u32) -> Result<wasm_bindgen::Clamped<Vec<u8>>, JsError> {
        Ok(wasm_bindgen::Clamped(self.0.pixels(state, t_ms, width).map_err(js)?.rgba))
    }

    /// `state` at `t_ms` in `format`, one of the deck's formats or its own canvas
    /// (`undefined`), painted `height` pixels high as `pixels` paints the canvas, whichever
    /// format it shows, the width keeping the format's aspect (`pixels.length / 4 / height`):
    /// what the editor paints beside the canvas, a format at a time, as it plays (PLAN 2.62).
    /// Each format lays each state out once.
    #[cfg(feature = "editor")]
    #[wasm_bindgen(js_name = pixelsIn)]
    pub fn pixels_in(
        &mut self,
        format: Option<String>,
        state: &str,
        t_ms: f64,
        height: u32,
    ) -> Result<wasm_bindgen::Clamped<Vec<u8>>, JsError> {
        Ok(wasm_bindgen::Clamped(self.0.pixels_in(format.as_deref(), state, t_ms, height).map_err(js)?.rgba))
    }

    /// Begin judging the layouts `state` may take (PLAN 2.92): how many there are. Each is
    /// judged by a `layoutsStep`, so that the worker answers what else it is asked between them.
    #[cfg(feature = "editor")]
    #[wasm_bindgen(js_name = layoutsBegin)]
    pub fn layouts_begin(&mut self, state: &str) -> Result<usize, JsError> {
        self.0.layouts_begin(state).map_err(js)
    }

    /// Judge the next layout: the state laid out in it, linted in every format, and drawn in
    /// the format shown. Whether any is left. A deck, its files, or the format changed since
    /// `layoutsBegin` ends the round, an error.
    #[cfg(feature = "editor")]
    #[wasm_bindgen(js_name = layoutsStep)]
    pub fn layouts_step(&mut self) -> Result<bool, JsError> {
        self.0.layouts_step().map_err(js)
    }

    /// The layouts judged since `layoutsBegin`, best first, each painted at rest `height`
    /// pixels high in the format shown, and the round ends: JSON, an array of `scaena inspect
    /// --layouts`' suggestions, each with its picture's `width` and `height`. `layoutPixels(i)`
    /// takes the `i`th picture's pixels, as `pixels` gives a frame's.
    #[cfg(feature = "editor")]
    #[wasm_bindgen(js_name = layoutSuggestions)]
    pub fn layout_suggestions(&mut self, height: u32) -> Result<String, JsError> {
        serde_json::to_string(&self.0.layouts_painted(height).map_err(js)?).map_err(js)
    }

    /// The pixels of the `i`th picture `layoutSuggestions` painted last, taken: a second call
    /// gives none.
    #[cfg(feature = "editor")]
    #[wasm_bindgen(js_name = layoutPixels)]
    pub fn layout_pixels(&mut self, i: usize) -> wasm_bindgen::Clamped<Vec<u8>> {
        wasm_bindgen::Clamped(self.0.layout_pixels(i))
    }
}

/// A frame whose shaders' rows are worked out on other workers (PLAN 2.28). The module has no
/// threads; a page that has cores to spare starts workers of its own, each with this module
/// ([`engine_module`]), and spreads a full-canvas shader's bands over them:
///
/// 1. `shading` holds `pixels`' frame and says how many shaders it draws;
/// 2. for each, `shaderBands` splits its rows, `shaderSpec` goes to the other workers, which
///    work out their bands with [`shader_rows`], and `shade` works out one here;
/// 3. `takeRows` takes each band another worker worked out;
/// 4. `paintShaded` paints the frame: `pixels`' bytes.
#[cfg(feature = "cpu")]
#[wasm_bindgen]
impl Player {
    pub fn shading(&mut self, state: &str, t_ms: f64, width: u32) -> Result<u32, JsError> {
        self.0.shading(state, t_ms, width).map(|n| n as u32).map_err(js)
    }

    /// Shader `i`'s spec, which [`shader_rows`] makes its job again from.
    #[wasm_bindgen(js_name = shaderSpec)]
    pub fn shader_spec(&mut self, i: u32) -> Result<Vec<u8>, JsError> {
        self.0.shader_spec(i as usize).map_err(js)
    }

    /// Shader `i`'s rows split among `workers`: each band's first row, then its rows, flat.
    #[wasm_bindgen(js_name = shaderBands)]
    pub fn shader_bands(&mut self, i: u32, workers: u32) -> Result<Vec<u32>, JsError> {
        Ok(self.0.shader_bands(i as usize, workers as usize).map_err(js)?.concat())
    }

    /// Work out shader `i`'s rows `first..first + rows` here.
    pub fn shade(&mut self, i: u32, first: u32, rows: u32) -> Result<(), JsError> {
        self.0.shade(i as usize, first, rows).map_err(js)
    }

    /// Take shader `i`'s rows from `first` as another worker worked them out ([`shader_rows`]).
    #[wasm_bindgen(js_name = takeRows)]
    pub fn take_rows(&mut self, i: u32, first: u32, bytes: &js_sys::Uint8Array) -> Result<(), JsError> {
        let row = self.0.shader_row(i as usize).map_err(js)?;
        let length = bytes.length() as usize;
        if !length.is_multiple_of(row) {
            return Err(JsError::new(&format!("{length} bytes are not whole rows of shader {i}, {row} bytes each")));
        }
        bytes.copy_to(self.0.shader_band(i as usize, first, (length / row) as u32).map_err(js)?);
        Ok(())
    }

    /// The frame held, painted: what `pixels` returns for it.
    #[wasm_bindgen(js_name = paintShaded)]
    pub fn paint_shaded(&mut self) -> Result<wasm_bindgen::Clamped<Vec<u8>>, JsError> {
        Ok(wasm_bindgen::Clamped(self.0.shaded().map_err(js)?.rgba))
    }

    /// The frame held, painted and put on `onto` at its top left: `paintShaded`'s pixels, which
    /// the canvas copies from the module's memory, with no copy out of it first.
    #[wasm_bindgen(js_name = putShaded)]
    pub fn put_shaded(&mut self, onto: &web_sys::OffscreenCanvasRenderingContext2d) -> Result<(), JsError> {
        let raster = self.0.shaded().map_err(js)?;
        let said = |e: JsValue| JsError::new(&format!("the frame could not go on the canvas: {e:?}"));
        // A view of the module's memory, which nothing allocates into before the canvas has
        // copied it.
        let image = web_sys::ImageData::new_with_u8_clamped_array_and_sh(
            wasm_bindgen::Clamped(&raster.rgba),
            raster.width,
            raster.height,
        )
        .map_err(said)?;
        onto.put_image_data(&image, 0.0, 0.0).map_err(said)
    }
}

/// Rows `first..first + rows` of the shader whose spec `Player.shaderSpec` gave: what another
/// worker, holding no deck, works out for the one that does (PLAN 2.28).
#[cfg(feature = "cpu")]
#[wasm_bindgen(js_name = shaderRows)]
pub fn shader_rows_js(spec: &[u8], first: u32, rows: u32) -> Result<Vec<u8>, JsError> {
    shader_rows(spec, first, rows).map_err(js)
}

/// Where this thread's engine gets a language's hyphenation patterns that its module leaves out
/// (ADR-0015): `loader`, given the language's code (`de`), returns the bytes of its file
/// (`hyphenation/de.bin`) as a `Uint8Array`, or anything else where it has none. It is asked the
/// first time a text hyphenates in that language, in the middle of a layout, so it answers at
/// once. What it returns is held to the file's SHA-256. The player's module has every language
/// compiled in, and never asks.
#[cfg(feature = "cpu")]
#[wasm_bindgen(js_name = setHyphenation)]
pub fn set_hyphenation(loader: js_sys::Function) {
    scaena_engine::hyphen::set_loader(move |code| {
        let bytes = loader.call1(&JsValue::NULL, &JsValue::from_str(code)).ok()?;
        bytes.dyn_into::<js_sys::Uint8Array>().ok().map(|bytes| bytes.to_vec())
    });
}

/// The module this is, compiled: a page hands it to the workers it starts beside the
/// engine's, which instantiate it rather than fetch and compile it again (PLAN 2.28).
#[wasm_bindgen(js_name = engineModule)]
pub fn engine_module() -> JsValue {
    wasm_bindgen::module()
}

/// The source editor (PLAN 2.3): every result as JSON, as `editor`'s types serialize.
#[cfg(feature = "editor")]
#[wasm_bindgen]
impl Player {
    /// The deck as canonical `.scn`: what the editor opens on.
    pub fn source(&self) -> String {
        self.0.source()
    }

    /// Whether the deck shown is the one compiled from `source`, nothing written over it since.
    #[wasm_bindgen(js_name = compiledFrom)]
    pub fn compiled_from(&self, source: &str) -> bool {
        self.0.compiled_from(source)
    }

    /// Compile `source`: `{ error?, findings, states, valid }`, each place in it in UTF-16
    /// offsets. A deck that validates is what frames show from now on.
    pub fn compile(&mut self, source: &str) -> Result<String, JsError> {
        serde_json::to_string(&self.0.compile(source)).map_err(js)
    }

    /// Lint the deck compiled last, laid out by this engine: `{ findings, laid, whole }`.
    /// With `state`, the layout rules run on that state alone, the one being edited, and
    /// the other states keep what they found when they last ran on every state, in the
    /// formats the deck still lists, each while it has every node it had then.
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

/// Direct manipulation (ADR-0013): what stands where in a state at rest, and where a node may
/// go, for the editor's canvas. The player's module leaves it out.
#[cfg(feature = "editor")]
#[wasm_bindgen]
impl Player {
    /// Each visible node's box in `state` at rest, in the format shown, as JSON (ADR-0013):
    /// `[{ "node", "rect": [x, y, w, h], "parent"?, "draws", "transform"?, "locked"? }]`, canvas
    /// units, those that draw in paint order, then the containers and groups that only hold
    /// others. `transform` is where its own and its containers' draw it from `rect` (SPEC
    /// §3.3), `[a, b, c, d, e, f]`, where something moves it; `locked`, where the node is locked
    /// (PLAN 2.95), the node whose lock holds it: itself, or what holds it.
    pub fn boxes(&mut self, state: &str) -> Result<String, JsError> {
        serde_json::to_string(&self.0.boxes_json(state).map_err(js)?).map_err(js)
    }

    /// The nodes that draw at `x`, `y` (canvas units) in `state` at rest, in the format shown,
    /// topmost first, as JSON (ADR-0013): `[{ "node", "rect", "containers", "transform"?,
    /// "locked"? }]`, each node's containers innermost first, read through its transform as a
    /// box's is; `locked` where the node is locked (PLAN 2.95), which a pointer passes over: the
    /// node whose lock holds it, itself or the innermost container locked.
    pub fn hit(&mut self, state: &str, x: f32, y: f32) -> Result<String, JsError> {
        serde_json::to_string(&self.0.hits_json(state, [x, y]).map_err(js)?).map_err(js)
    }

    /// The chart mark or table row drawn at `x`, `y` (canvas units) in `state` at rest, in the
    /// format shown, as JSON (PLAN 2.64): `{ "node", "source", "key", "rows", "outline", "rect",
    /// "transform"? }`, or `null` where the topmost node there is no chart or table, or the
    /// point falls between its marks. `rows` are the rows of `source` it was made from, from 0,
    /// as the source's sheet numbers them; `outline` is SVG path data, canvas units, as laid
    /// out, and `transform` where it is drawn from there, as a box's.
    #[wasm_bindgen(js_name = markAt)]
    pub fn mark_at(&mut self, state: &str, x: f32, y: f32) -> Result<String, JsError> {
        let found = self.0.mark_at(state, [x, y]).map_err(js)?;
        serde_json::to_string(&found.map(mark_json)).map_err(js)
    }

    /// What `rows` of data source `source` draw in `state` at rest, in the format shown, as
    /// JSON (PLAN 2.64): each chart mark and table row made from any of them, in paint order,
    /// as `markAt` gives one.
    #[wasm_bindgen(js_name = marksOf)]
    pub fn marks_of(&mut self, state: &str, source: &str, rows: Vec<u32>) -> Result<String, JsError> {
        let rows: Vec<usize> = rows.into_iter().map(|r| r as usize).collect();
        let marks: Vec<serde_json::Value> =
            self.0.marks_of(state, source, &rows).map_err(js)?.into_iter().map(mark_json).collect();
        serde_json::to_string(&marks).map_err(js)
    }

    /// The chart annotation drawn at `x`, `y` (canvas units) in `state` at rest, in the format
    /// shown, as JSON (PLAN 2.67): `{ "node", "index", "kind", "text", "outline", "rect",
    /// "transform"? }`, `index` its place among the chart's `annotations`; or `null` where the
    /// topmost node there is no chart, or the point is on none of its annotations. A highlight
    /// draws nothing of its own: a mark it picks out names it (`markAt`'s `notes.highlighted`).
    #[wasm_bindgen(js_name = noteAt)]
    pub fn note_at(&mut self, state: &str, x: f32, y: f32) -> Result<String, JsError> {
        let found = self.0.note_at(state, [x, y]).map_err(js)?;
        serde_json::to_string(&found.map(note_json)).map_err(js)
    }

    /// Chart `node`'s marks and annotations in `state` at rest, as JSON (PLAN 2.75): `{ marks,
    /// notes }`, each as `markAt` and `noteAt` give one, its marks in data order and its
    /// annotations in the order it writes them; `null` for a node that is no chart.
    #[wasm_bindgen(js_name = marksIn)]
    pub fn marks_in(&mut self, state: &str, node: &str) -> Result<String, JsError> {
        let found = self.0.marks_in(state, node).map_err(js)?.map(|(marks, notes)| {
            serde_json::json!({
                "marks": marks.into_iter().map(mark_json).collect::<Vec<_>>(),
                "notes": notes.into_iter().map(note_json).collect::<Vec<_>>(),
            })
        });
        serde_json::to_string(&found).map_err(js)
    }

    /// Where a callout of chart `node` dropped at `x`, `y` (canvas units) in `state` at rest
    /// would stand, as JSON (PLAN 2.67): an annotation's `at`, on the mark there or at the
    /// category or x nearest across and the value there; `null` for a node that is no chart,
    /// or a donut.
    #[wasm_bindgen(js_name = calloutAt)]
    pub fn callout_at(&mut self, state: &str, node: &str, x: f32, y: f32) -> Result<String, JsError> {
        serde_json::to_string(&self.0.callout_at(state, node, [x, y]).map_err(js)?).map_err(js)
    }

    /// The layout `state` uses and its slots in the format shown, as JSON (PLAN 2.71): `{
    /// "layout", "slots": [{ "name", "rect", "col"?, "row"?, "own" }] }`, each slot's box in
    /// canvas units, its cells as the theme writes them, and whether the format shown writes it
    /// itself; `null` for a state that names no layout.
    pub fn layout(&mut self, state: &str) -> Result<String, JsError> {
        let found = self.0.layout(state).map_err(js)?;
        let out = found.map(|(layout, slots)| serde_json::json!({ "layout": layout, "slots": slots }));
        serde_json::to_string(&out).map_err(js)
    }

    /// Shape `node`'s outline in `state` at rest, as JSON (PLAN 2.68): `{ "node", "kind",
    /// "rect", "transform"?, "points", "fewest", "radius"?, "radii" }`, its points fractions of
    /// its box, a rect's radius and the theme's radius steps in canvas units; `null` for a node
    /// the state does not draw, or one that is no shape.
    pub fn outline(&mut self, state: &str, node: &str) -> Result<String, JsError> {
        serde_json::to_string(&self.0.outline(state, node).map_err(js)?).map_err(js)
    }

    /// Image `node`'s framing in `state` at rest, as JSON (PLAN 2.74): `{ "node", "rect",
    /// "transform"?, "whole", "shown", "crop", "focal", "fit", "size" }`, where its whole image and
    /// the part that shows are drawn in canvas units as laid out, its crop in fractions of the image,
    /// and its focal point in fractions of the crop; `null` for a node the state does not draw, or
    /// one that is no image.
    pub fn framing(&mut self, state: &str, node: &str) -> Result<String, JsError> {
        serde_json::to_string(&self.0.framing(state, node).map_err(js)?).map_err(js)
    }

    /// The point of image `node` drawn under `x`, `y` in `state` at rest, as JSON: `[x, y]`,
    /// fractions of the part its crop keeps, which a focal point picked there names; `null` off
    /// the image (PLAN 2.45).
    #[wasm_bindgen(js_name = focalAt)]
    pub fn focal_at(&mut self, state: &str, node: &str, x: f32, y: f32) -> Result<String, JsError> {
        // To a thousandth, as a person would write it.
        let at = self.0.focal_at(state, node, [x, y]).map_err(js)?.map(|p| p.map(|v| (v * 1000.0).round() / 1000.0));
        serde_json::to_string(&at).map_err(js)
    }

    /// Where `node` may go in `state` at rest, in the format shown, as JSON (ADR-0013): `{
    /// "by", "parent"?, "cell", "columns"?, "rows"?, "slots"?, "flow"?, "within", "snaps" }`,
    /// as `scaena inspect --targets` gives it.
    pub fn targets(&mut self, state: &str, node: &str) -> Result<String, JsError> {
        let found = self.0.targets(state, node).map_err(js)?.clone();
        serde_json::to_string(&scaena_ops::inspect::Targets::from(found)).map_err(js)
    }

    /// Where the box `x`, `y`, `w`, `h` (`node`'s cell as a drag left it) lands in `state`
    /// when it snaps `how` (`move`, `resize`, `slot`, `free`, `order`), as JSON: `{ "cell",
    /// "patch", "guides"? }`, the patch the place ops that put the node there, kept to `state`
    /// when they `fork`; `null` where nothing places the node that way. `guides`, each `[x1,
    /// y1, x2, y2]`, are where the box's edges or its middle meet another box's or the
    /// canvas's (PLAN 2.57); off the grid (`free`), the box goes first the least way that
    /// brings one onto another within `reach` canvas units. Asked with each move of a drag, it
    /// lays nothing out.
    #[allow(clippy::too_many_arguments)]
    pub fn snap(
        &mut self,
        state: &str,
        node: &str,
        how: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fork: bool,
        reach: f32,
    ) -> Result<String, JsError> {
        let how: scaena_ops::inspect::SnapMode = how.parse().map_err(|e: String| JsError::new(&e))?;
        let guided = self.0.guided(state, node, how, [x, y, w, h], fork, reach).map_err(js)?;
        let out = match guided {
            None => serde_json::Value::Null,
            Some((snapped, guides)) => with_guides(serde_json::to_value(snapped).map_err(js)?, &guides),
        };
        serde_json::to_string(&out).map_err(js)
    }

    /// `nodes`, children of one container, moved together `dx`, `dy` canvas units in `state`
    /// at rest (PLAN 2.42), as a drag of the first snaps it, `free` off the grid; as JSON: `{
    /// "landed": [{ "node", "cell" }], "patch", "guides"? }`, the patch made in `state` or kept
    /// there to `fork` it, or `null` where nothing moves them. `guides` are where the box around
    /// them meets another's or the canvas's (PLAN 2.57); `free`, it goes first the least way
    /// that brings an edge, or its middle, onto another's within `reach`.
    #[allow(clippy::too_many_arguments)]
    pub fn together(
        &mut self,
        state: &str,
        nodes: Vec<String>,
        dx: f32,
        dy: f32,
        free: bool,
        fork: bool,
        reach: f32,
    ) -> Result<String, JsError> {
        let moved = self.0.together(state, &nodes, [dx, dy], free, fork, reach).map_err(js)?;
        let out = match moved {
            None => serde_json::Value::Null,
            Some((arranged, guides)) => with_guides(serde_json::to_value(arranged).map_err(js)?, &guides),
        };
        serde_json::to_string(&out).map_err(js)
    }

    /// The theme's grid in the format shown, as the editor's guides draw it (PLAN 2.57), as
    /// JSON: `{ "canvas": [w, h], "columns": [[start, end]], "rows": [[start, end]],
    /// "baselines": [y], "safe": [x, y, w, h] }`, canvas units: the gutters between the tracks,
    /// the margins around them, a line every pitch of the baseline grid from the top margin, and
    /// what lies inside the safe area's strip (PLAN 3.29).
    pub fn grid(&self) -> Result<String, JsError> {
        let g = self.0.grid().map_err(js)?;
        let out = serde_json::json!({
            "canvas": g.canvas, "columns": g.columns, "rows": g.rows, "baselines": g.baselines, "safe": g.safe,
        });
        serde_json::to_string(&out).map_err(js)
    }

    /// `nodes`, children of one container, arranged in `state` at rest, in the format shown
    /// (PLAN 2.42), `how` JSON: `{ "align": "left" }`, `{ "spread": "across" }`, `{ "order":
    /// "front" }`, or `{ "by": [dx, dy], "free"? }`, moved together as a drag of the first
    /// snaps it. As JSON: `{ "landed": [{ "node", "cell" }], "patch" }`, the patch made in
    /// `state`, or kept there to `fork` it; `null` where nothing moves them that way.
    pub fn arranging(&mut self, state: &str, nodes: Vec<String>, how: &str, fork: bool) -> Result<String, JsError> {
        let asked: scaena_ops::arrange::Asked = serde_json::from_str(how).map_err(js)?;
        let how = asked.how().map_err(|e| JsError::new(&e.message))?;
        serde_json::to_string(&self.0.arranging(state, &nodes, how, fork).map_err(js)?).map_err(js)
    }

    /// Each text of the deck that `query` matches (PLAN 2.47), `query` JSON: `{ "find",
    /// "case"?, "words"? }`. As JSON: `[{ "node", "state", "states", "lives", "text",
    /// "matches" }]`, once for each place a text is written, `matches` in characters.
    pub fn find(&self, query: &str) -> Result<String, JsError> {
        let query = serde_json::from_str(query).map_err(js)?;
        serde_json::to_string(&self.0.find(&query).map_err(js)?).map_err(js)
    }

    /// The patch that replaces what `query` matches with `with` (PLAN 2.47), as JSON: every
    /// match, a `replace_text` where each text lives; or, with `one`, `[text, match]` into what
    /// `find` gives, that match alone.
    pub fn replacing(&self, query: &str, with: &str, one: Option<Vec<u32>>) -> Result<String, JsError> {
        let query = serde_json::from_str(query).map_err(js)?;
        let one = match one.as_deref() {
            None => None,
            Some(&[i, k]) => Some([i as usize, k as usize]),
            Some(other) => return Err(JsError::new(&format!("one match is [text, match], not {other:?}"))),
        };
        serde_json::to_string(&self.0.replacing(&query, with, one).map_err(js)?).map_err(js)
    }

    /// Draw `nodes`, and what they hold, `dx`, `dy` canvas units from where they stand in the
    /// frames at rest that follow, laying nothing out: what a drag shows as it moves. With
    /// none, every node stands where it is.
    #[wasm_bindgen(js_name = setMoving)]
    pub fn set_moving(&mut self, nodes: Vec<String>, dx: f32, dy: f32) {
        self.0.set_moving((!nodes.is_empty()).then_some((nodes, [dx, dy])));
    }

    /// Paint the frames that follow through `view`, `[x, y, w, h]` canvas units: the part of
    /// the canvas the editor's preview shows zoomed in, painted at the size shown (PLAN 2.46).
    /// With none, the whole canvas.
    #[wasm_bindgen(js_name = setView)]
    pub fn set_view(&mut self, view: Option<Vec<f32>>) -> Result<(), JsError> {
        let view = match view.as_deref() {
            None => None,
            Some(&[x, y, w, h]) => Some([x, y, w, h]),
            Some(other) => return Err(JsError::new(&format!("a view is [x, y, w, h], not {other:?}"))),
        };
        self.0.set_view(view).map_err(js)
    }

    /// Frames at rest draw the deck `ops` (a patch, JSON) would make, laid out once a frame,
    /// without making it: a resize shows its text reflowed when it pauses. Without `ops`,
    /// the deck as it is.
    pub fn preview(&mut self, ops: Option<String>) -> Result<(), JsError> {
        let ops: Option<Vec<serde_json::Value>> = ops.map(|o| serde_json::from_str(&o)).transpose().map_err(js)?;
        self.0.preview(ops.as_deref()).map_err(js)
    }

    /// The states `ops` (a patch, JSON) would change what shows in, by id, as JSON: what the
    /// editor says of a drag before it is dropped ("in 3 states"). Nothing is made.
    pub fn reach(&self, ops: &str) -> Result<String, JsError> {
        let ops: Vec<serde_json::Value> = serde_json::from_str(ops).map_err(js)?;
        serde_json::to_string(&self.0.reach(&ops).map_err(js)?).map_err(js)
    }

    /// Where a caret stands in `node`'s text in `state` at rest, in the format shown, as JSON
    /// (ADR-0013, PLAN 2.32), its offsets in UTF-16 code units, as the page counts a string:
    /// `{ "text", "lines": [{ "top", "bottom", "x", "start", "end", "broken", "chars": [[offset,
    /// lead, trail]] }], "items": [{ "kind", "level", "marker" } | null] }`, canvas units, `items`
    /// each paragraph as a list's item (PLAN 2.69); `null` for a node that is no text there.
    pub fn carets(&mut self, state: &str, node: &str) -> Result<String, JsError> {
        serde_json::to_string(&self.0.carets_json(state, node).map_err(js)?).map_err(js)
    }

    /// What an inspector offers for `node` as `state` shows it, as JSON (ADR-0013, PLAN 2.33):
    /// `{ node, type, state, fields }`, as `scaena inspect --choices` says it.
    pub fn choices(&self, state: &str, node: &str) -> Result<String, JsError> {
        serde_json::to_string(&self.0.choices(state, node).map_err(js)?).map_err(js)
    }

    /// `node`'s look as `state` shows it, as JSON (PLAN 2.58): `{ node, type, props: [{ prop,
    /// value? }] }`, as `scaena inspect --look` says it; what ⌥⌘C picks up.
    pub fn look(&self, state: &str, node: &str) -> Result<String, JsError> {
        serde_json::to_string(&self.0.look(state, node).map_err(js)?).map_err(js)
    }

    /// `look` (JSON, as `look` gives it) put on `nodes` in `state`, as JSON (PLAN 2.58): `{
    /// patch, took, same, refused: [{ node, why }] }`, as `scaena inspect --look --onto` says
    /// it; what ⌥⌘V makes.
    pub fn putting(&self, state: &str, look: &str, nodes: Vec<String>) -> Result<String, JsError> {
        let look: scaena_core::looks::Look = serde_json::from_str(look).map_err(js)?;
        serde_json::to_string(&self.0.putting(state, &look, &nodes).map_err(js)?).map_err(js)
    }

    /// What an inspector offers for the characters `from` to `to` (Unicode scalar values) of
    /// `node`'s text in `state`, as JSON (PLAN 2.38): `{ node, type, state, fields }`, each
    /// field a look a run takes, which `style_text` sets.
    #[wasm_bindgen(js_name = characterChoices)]
    pub fn character_choices(&self, state: &str, node: &str, from: usize, to: usize) -> Result<String, JsError> {
        serde_json::to_string(&self.0.character_choices(state, node, from, to).map_err(js)?).map_err(js)
    }

    /// What ⌘B gives the characters `from` to `to` (Unicode scalar values) of `node`'s text
    /// in `state`, as JSON: `style_text`'s `look` (PLAN 2.38).
    pub fn bolding(&mut self, state: &str, node: &str, from: usize, to: usize) -> Result<String, JsError> {
        serde_json::to_string(&self.0.bolding(state, node, from, to).map_err(js)?).map_err(js)
    }

    /// What ⌘I gives the characters `from` to `to` (Unicode scalar values) of `node`'s text
    /// in `state`, as JSON: `style_text`'s `look` (PLAN 2.40).
    pub fn italicizing(&mut self, state: &str, node: &str, from: usize, to: usize) -> Result<String, JsError> {
        serde_json::to_string(&self.0.italicizing(state, node, from, to).map_err(js)?).map_err(js)
    }

    /// The theme the deck names and the theme files the bundle holds, as JSON: `{ current,
    /// files }` (PLAN 2.39).
    pub fn themes(&self) -> Result<String, JsError> {
        let (current, files) = self.0.themes();
        serde_json::to_string(&serde_json::json!({ "current": current, "files": files })).map_err(js)
    }

    /// The theme frames are drawn in, as JSON `{ theme, text }` (PLAN 2.61): the theme file the deck
    /// names, by its path, or `(inline)` for one written in the deck, and its JSON as text; none
    /// where the deck names no theme.
    #[wasm_bindgen(js_name = themeText)]
    pub fn theme_text(&self) -> Option<String> {
        self.0.theme_text().map(|t| t.to_string())
    }

    /// What an inspector offers for `state` itself, as JSON (PLAN 2.36): `{ state, fields }`,
    /// as `scaena inspect --state-choices` says it.
    #[wasm_bindgen(js_name = stateChoices)]
    pub fn state_choices(&self, state: &str) -> Result<String, JsError> {
        serde_json::to_string(&self.0.state_choices(state).map_err(js)?).map_err(js)
    }

    /// `state`'s layers, as JSON (PLAN 2.50): `[{ node, type, shown, children? }]`, topmost
    /// first, as `scaena inspect --layers` says them.
    pub fn layers(&self, state: &str) -> Result<String, JsError> {
        serde_json::to_string(&self.0.layers(state).map_err(js)?).map_err(js)
    }

    /// What may be inserted, as JSON (PLAN 2.34): `[{ label, node, id, start }]`, as `scaena
    /// inspect --inserts` says it.
    pub fn inserts(&self) -> Result<String, JsError> {
        serde_json::to_string(&self.0.inserts()).map_err(js)
    }

    /// The patch that inserts what `inserts` offers `n`th in `state`, about `x`, `y` (canvas
    /// units), or in the room nearest it where that is taken (PLAN 2.79), as JSON: `{ id, cell,
    /// patch }` (PLAN 2.34). `named`, a dropped file's name, names it, and `with` (a JSON object)
    /// sets properties of its own on it: a pasted sheet's table its columns (PLAN 2.96).
    pub fn inserting(
        &mut self,
        state: &str,
        n: usize,
        x: f32,
        y: f32,
        named: Option<String>,
        with: Option<String>,
    ) -> Result<String, JsError> {
        let with: Option<serde_json::Map<String, serde_json::Value>> =
            with.as_deref().map(serde_json::from_str).transpose().map_err(js)?;
        let added = self.0.inserting_with(state, n, [x, y], named.as_deref(), with.as_ref()).map_err(js)?;
        serde_json::to_string(&added).map_err(js)
    }

    /// The patch that draws what `inserts` offers `n`th in `state`, in the box a drag from `x0`,
    /// `y0` to `x1`, `y1` covers (canvas units), snapped to the grid or, `free`, where it was
    /// drawn, as JSON: `{ id, cell, patch }` (PLAN 2.48).
    #[allow(clippy::too_many_arguments)]
    pub fn drawing(
        &mut self,
        state: &str,
        n: usize,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        free: bool,
    ) -> Result<String, JsError> {
        serde_json::to_string(&self.0.drawing(state, n, [[x0, y0], [x1, y1]], free).map_err(js)?).map_err(js)
    }

    /// The patch that copies `node` beside it in `state`, as JSON: `{ id, cell, patch }`
    /// (PLAN 2.34).
    pub fn duplicating(&mut self, state: &str, node: &str) -> Result<String, JsError> {
        serde_json::to_string(&self.0.duplicating(state, node).map_err(js)?).map_err(js)
    }

    /// The patch that puts `nodes` in a new group where they stand in `state` (PLAN 2.43), as
    /// JSON: `{ id, patch }`.
    pub fn grouping(&self, state: &str, nodes: Vec<String>) -> Result<String, JsError> {
        serde_json::to_string(&self.0.grouping(state, &nodes)).map_err(js)
    }

    /// What a copy of `nodes`, as `state` shows them, holds (PLAN 2.37, 2.42), as JSON: what
    /// goes on the clipboard, as `application/x-scaena+json` and as text. The first is the node
    /// copied; several are children of one container, as the canvas selects them.
    pub fn copying(&mut self, state: &str, nodes: Vec<String>) -> Result<String, JsError> {
        let nodes: Vec<&str> = nodes.iter().map(String::as_str).collect();
        serde_json::to_string(&self.0.copying(state, &nodes).map_err(js)?).map_err(js)
    }

    /// The patch that pastes what the clipboard holds in `state` about `x`, `y` (PLAN 2.37), as
    /// JSON: `{ id, cell, also?, patch, files, findings }`, `also` the copies of the others
    /// copied with the first (PLAN 2.42). A clip as [`Player::copying`] gives it pastes what it
    /// holds; other text, a text in the theme's body role.
    pub fn pasting(&mut self, text: &str, state: &str, x: f32, y: f32) -> Result<String, JsError> {
        let clip = match scaena_ops::clipboard::read(text).map_err(js)? {
            Some(clip) => clip,
            None => scaena_ops::clipboard::of_text(self.0.deck(), self.0.theme(), text).map_err(js)?,
        };
        serde_json::to_string(&self.0.pasting(&clip, state, [x, y]).map_err(js)?).map_err(js)
    }

    /// The patch that deletes `node` from `state`, with what it holds there, as JSON: `hide_node`
    /// for each, or `remove_node` for one no state shows after; with `everywhere`, `remove_node`
    /// for each (PLAN 2.34).
    pub fn deleting(&self, state: &str, node: &str, everywhere: bool) -> Result<String, JsError> {
        serde_json::to_string(&self.0.deleting(state, node, everywhere).map_err(js)?).map_err(js)
    }

    /// The patch that adds a state after `state`, the state shown, as JSON: `{ id, patch }`,
    /// `what` a `step` of its slide or a `slide` of its own (PLAN 2.35).
    #[wasm_bindgen(js_name = addingState)]
    pub fn adding_state(&self, state: &str, what: &str) -> Result<String, JsError> {
        let what = serde_json::from_value(serde_json::Value::String(what.into())).map_err(js)?;
        serde_json::to_string(&self.0.adding_state(state, what).map_err(js)?).map_err(js)
    }

    /// The deck in another theme, by `user` at `at` (RFC 3339), as JSON: what `theme --apply`
    /// says (PLAN 2.39). `path` is a theme file in the bundle, or, with `text`, where that
    /// theme goes; `fonts` maps the paths its families give to their bytes.
    pub fn retheme(
        &mut self,
        path: &str,
        text: Option<String>,
        fonts: &js_sys::Map,
        at: Option<String>,
    ) -> Result<String, JsError> {
        let mut given = std::collections::BTreeMap::new();
        fonts.for_each(&mut |bytes, path| {
            if let Some(path) = path.as_string() {
                given.insert(path, js_sys::Uint8Array::new(&bytes).to_vec());
            }
        });
        let at = at.as_deref().and_then(store::seconds);
        serde_json::to_string(&self.0.retheme(path, text.as_deref(), given, at).map_err(js)?).map_err(js)
    }

    /// Make `ops` (JSON: a `replace_text` typed on the canvas, or a `style_text` given to the
    /// characters selected there) as `user` at `at` (RFC 3339), validated and refused as a
    /// patch is, but not linted (ADR-0013, PLAN 2.32, 2.38). Whether it changed the deck.
    pub fn typed(&mut self, ops: &str, at: Option<String>) -> Result<bool, JsError> {
        let ops: serde_json::Value = serde_json::from_str(ops).map_err(js)?;
        self.0.typed(&ops, at.as_deref().and_then(store::seconds)).map_err(js)
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

    /// A new deck (PLAN 2.12), as `deck_create` makes one: the theme file named `file`
    /// (`dusk.theme.json`), whose text is `theme`; the fonts its families name, each in `fonts`
    /// by the path the theme gives it (`fonts/Inter-VF.ttf` to its bytes); and one state with
    /// nothing on it, titled `title`. Kept nowhere until it is saved.
    pub fn create(file: &str, theme: &str, title: &str, fonts: &js_sys::Map) -> Result<Player, JsError> {
        let mut given = std::collections::BTreeMap::new();
        fonts.for_each(&mut |bytes, path| {
            if let Some(path) = path.as_string() {
                given.insert(path, js_sys::Uint8Array::new(&bytes).to_vec());
            }
        });
        Session::create(file, theme, &given, title).map(Player).map_err(js)
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
    /// `addSubset`, for the characters the deck can draw now. A bundle that keeps a history
    /// has the save recorded in it by `history`, the history's own module (`scaena-history`,
    /// PLAN 2.9): the page's edits by the user, and each tool's by its caller. Without it,
    /// the history is carried as it is.
    pub fn save(&self, now: &str, subset: bool, history: Option<History>) -> Result<SavedBundle, JsError> {
        let record =
            history.as_ref().map(|h| move |held: &[u8], changes: &str| h.record(held, changes).map_err(|e| said(&e)));
        let record = record.as_ref().map(|r| r as &store::Recorder);
        self.0.save(now, subset, record).map(SavedBundle).map_err(js)
    }

    /// Begin a history with the next save, where the bundle keeps none (PLAN 2.87): it holds
    /// the deck as saved, and the files it is drawn from, as its first version, and each save
    /// after records the edits since, as `scaena save --history` begins one.
    #[wasm_bindgen(js_name = keepHistory)]
    pub fn keep_history(&mut self) {
        self.0.keep_history();
    }

    /// Whether the bundle keeps a history, or the next save begins one.
    #[wasm_bindgen(js_name = keepsHistory)]
    pub fn keeps_history(&self) -> bool {
        self.0.keeps_history()
    }

    /// Go on from `saved`, once the page has written it where it keeps the bundle: its
    /// files and its deck, which names them by their content. The source is the saved
    /// deck's from then on.
    pub fn adopt(&mut self, saved: &SavedBundle) -> Result<(), JsError> {
        self.0.adopt(&saved.0).map_err(js)
    }

    /// `state` at rest in the format shown, as a PNG `width` pixels wide, painted by the CPU
    /// painter (PLAN 2.54): what `scaena export --format png --size` writes for it.
    pub fn png(&mut self, state: &str, width: u32) -> Result<Vec<u8>, JsError> {
        self.0.png(state, width).map_err(js)
    }

    /// The deck's pages laid out for its PDF, as bytes the PDF's own module (`scaena-pdf`)
    /// draws: the PDF `scaena export --format pdf` writes (PLAN 2.54).
    #[wasm_bindgen(js_name = pdfLaidOut)]
    pub fn pdf_laid_out(&self) -> Result<Vec<u8>, JsError> {
        self.0.pdf_laid_out().map_err(js)
    }

    /// The deck as one HTML file that plays offline (PLAN 2.54): `page`, the single-file
    /// player's page, filled in with the bundle as `scaena export --format html` fills it, and
    /// named `name`. Its fonts are subset first, as for a save (`subsetting`, `addSubset`).
    pub fn standalone(&self, page: &str, name: &str) -> Result<String, JsError> {
        self.0.standalone(page, name).map_err(js)
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
    /// `author` calls it (`agent:` and the model's name; `agent` without it) at `at` (RFC
    /// 3339): an edit it makes is theirs when a save records it in the bundle's history.
    pub fn tool(&mut self, name: &str, args: &str, author: Option<String>, at: Option<String>) -> ToolResult {
        let by = assistant::Caller {
            author: author.as_deref().unwrap_or("agent"),
            at: at.as_deref().and_then(store::seconds),
        };
        let called = match serde_json::from_str(args) {
            Ok(args) => self.0.tool(name, args, by),
            Err(e) => Err(Error::Ops(format!("{name}: the arguments are not JSON: {e}"))),
        };
        match called {
            Ok(c) => {
                ToolResult { json: c.result, error: false, edited: c.edited, frame: c.frame, rewritten: c.rewritten }
            }
            Err(e) => {
                let json = assistant::failure(&e).to_string();
                ToolResult { json, error: true, edited: false, frame: None, rewritten: Vec::new() }
            }
        }
    }
}

/// A data source from the editor (PLAN 2.55, SPEC §3.10): its sheet, its edits, and their undo.
#[cfg(feature = "editor")]
#[wasm_bindgen]
impl Player {
    /// The bundle's images, fonts, and data, as JSON (PLAN 2.59): `[{ path, type, bytes, named,
    /// used }]`, as `scaena files --json` lists them, each with what in the deck names it, and
    /// the nodes drawn from it in the states that show them so.
    #[wasm_bindgen(js_name = bundleFiles)]
    pub fn bundle_files(&self) -> Result<String, JsError> {
        serde_json::to_string(&self.0.bundle_files().map_err(js)?).map_err(js)
    }

    /// `path` taken out of the bundle (PLAN 2.59): one of its images, fonts, or data that nothing
    /// names. `dataUndo` puts it back; the next save takes it out where the bundle is kept. An
    /// error says why, where something names it.
    #[wasm_bindgen(js_name = removeFile)]
    pub fn remove_file(&mut self, path: &str) -> Result<(), JsError> {
        self.0.remove_file(path).map_err(js)
    }

    /// The deck's data sources, as JSON: `[{ name, file? }]`, in its order, `file` the file each
    /// is (none for rows written inline).
    #[wasm_bindgen(js_name = dataSources)]
    pub fn data_sources(&self) -> String {
        let sources = self.0.data_sources().into_iter().map(|(name, file)| match file {
            Some(file) => serde_json::json!({ "name": name, "file": file }),
            None => serde_json::json!({ "name": name }),
        });
        serde_json::Value::Array(sources.collect()).to_string()
    }

    /// Data source `name` as a sheet, as `scaena data` reads it: as JSON, `{ sheet, file? }`.
    #[wasm_bindgen(js_name = dataSheet)]
    pub fn data_sheet(&self, name: &str) -> Result<String, JsError> {
        let (sheet, file) = self.0.data_sheet(name).map_err(js)?;
        serde_json::to_string(&serde_json::json!({ "sheet": sheet, "file": file })).map_err(js)
    }

    /// Where a data file `name` dropped on the canvas goes in the bundle: as `place` puts it,
    /// unless the bundle holds other bytes there, when a number goes before its extension.
    pub fn placing(&self, name: &str, bytes: &[u8]) -> String {
        self.0.placing(name, bytes)
    }

    /// `path`, a data file the bundle holds, as the source a chart of it reads, as JSON:
    /// `{ path, data, attached, patch }`. A source the deck declares for it already has no
    /// `attached` and no patch; a new one has what `data_attach` says of it and the patch that
    /// declares it, which `make` applies, empty where it is refused.
    /// `schema` (JSON: `{ column: type }`), where given, types its columns, as a pasted sheet's
    /// cells say they read (PLAN 2.96).
    pub fn attaching(&self, path: &str, schema: Option<String>) -> Result<String, JsError> {
        let schema = schema.as_deref().map(serde_json::from_str).transpose().map_err(js)?;
        serde_json::to_string(&self.0.attaching(path, schema).map_err(js)?).map_err(js)
    }

    /// `text` pasted on the canvas as a sheet's cells, read in the deck's language (PLAN 2.96),
    /// as JSON: `{ name, columns, schema, formats, rows, csv }`, the source they would be and the
    /// format that prints each column as it was copied; `null` where `text` is not cells.
    pub fn cells(&self, text: &str) -> String {
        serde_json::to_string(&self.0.cells(text)).unwrap_or_else(|_| "null".into())
    }

    /// `req` (JSON: `{ source, edits }`, as `data_edit` takes them) made by `author` (`user`
    /// without one) at `at` (RFC 3339): validated, as a patch is, but not linted, as for a value
    /// typed in a cell. As JSON, `{ result, wrote }`: what `data_edit` says it did, and whether it
    /// wrote the file or the deck.
    #[wasm_bindgen(js_name = dataEdit)]
    pub fn data_edit(&mut self, req: &str, author: Option<String>, at: Option<String>) -> Result<String, JsError> {
        let req: scaena_ops::data::DataEdit = serde_json::from_str(req).map_err(js)?;
        let by = assistant::Caller {
            author: author.as_deref().unwrap_or("user"),
            at: at.as_deref().and_then(store::seconds),
        };
        let (result, wrote) = self.0.data_edit(&req, false, by).map_err(js)?;
        serde_json::to_string(&serde_json::json!({ "result": result, "wrote": wrote })).map_err(js)
    }

    /// The file the last edit wrote, or the Files panel took out, put back as it was, by `author`
    /// (`user` without one) at `at`: the source it is, by name, or its path; none where there was
    /// nothing to undo.
    #[wasm_bindgen(js_name = dataUndo)]
    pub fn data_undo(&mut self, author: Option<String>, at: Option<String>) -> Result<Option<String>, JsError> {
        let by = assistant::Caller {
            author: author.as_deref().unwrap_or("user"),
            at: at.as_deref().and_then(store::seconds),
        };
        self.0.data_undo(false, by).map_err(js)
    }

    /// The file the last undo put back written, or taken out, again, as [`Player::data_undo`]
    /// says.
    #[wasm_bindgen(js_name = dataRedo)]
    pub fn data_redo(&mut self, author: Option<String>, at: Option<String>) -> Result<Option<String>, JsError> {
        let by = assistant::Caller {
            author: author.as_deref().unwrap_or("user"),
            at: at.as_deref().and_then(store::seconds),
        };
        self.0.data_undo(true, by).map_err(js)
    }

    /// Show a version of the deck read-only (PLAN 2.60): `held`, JSON `{ deck, files }` as the
    /// history's module reads it (`at`), in a session of its own. Its states' ids, as JSON.
    #[wasm_bindgen(js_name = viewVersion)]
    pub fn view_version(&mut self, held: &str) -> Result<String, JsError> {
        let held: versions::Held = serde_json::from_str(held).map_err(js)?;
        serde_json::to_string(&self.0.view_version(&held).map_err(js)?).map_err(js)
    }

    /// The version shown's `state` at rest, as a PNG `width` pixels wide.
    #[wasm_bindgen(js_name = versionPng)]
    pub fn version_png(&mut self, state: &str, width: u32) -> Result<Vec<u8>, JsError> {
        self.0.version_png(state, width).map_err(js)
    }

    /// What changed from version `from` to version `to`, each `{ deck, files }`, or without `to`,
    /// to the deck and its data files as they are now, as JSON `{ states, deck, files }`, as
    /// `scaena history --diff` says it.
    #[wasm_bindgen(js_name = compareVersions)]
    pub fn compare_versions(&self, from: &str, to: Option<String>) -> Result<String, JsError> {
        let from: versions::Held = serde_json::from_str(from).map_err(js)?;
        let to: Option<versions::Held> = to.map(|to| serde_json::from_str(&to)).transpose().map_err(js)?;
        serde_json::to_string(&self.0.compare_versions(&from, to.as_ref()).map_err(js)?).map_err(js)
    }

    /// Make version `held` (`{ deck, files }`), `version` as listed, the deck again, with its data
    /// files, by `author` (`user` without one) at `at`: one change, refused as a patch is. JSON
    /// `{ restored, files }`: what `scaena history --restore` says, and each data file written,
    /// `{ path, before, after }`, for the editor's undo to write back.
    #[wasm_bindgen(js_name = restoreVersion)]
    pub fn restore_version(
        &mut self,
        held: &str,
        version: &str,
        author: Option<String>,
        at: Option<String>,
    ) -> Result<String, JsError> {
        let held: versions::Held = serde_json::from_str(held).map_err(js)?;
        let version: scaena_ops::history::Version = serde_json::from_str(version).map_err(js)?;
        let by = assistant::Caller {
            author: author.as_deref().unwrap_or("user"),
            at: at.as_deref().and_then(store::seconds),
        };
        let (restored, files) = self.0.restore_version(&held, version, by).map_err(js)?;
        serde_json::to_string(&serde_json::json!({ "restored": restored, "files": files })).map_err(js)
    }

    /// Edit the theme the deck names by `edit` (JSON `{ ops }`, RFC 6902 operations on it,
    /// ADR-0016), by `author` (`user` without one) at `at`, unless `dry_run`: refused as `scaena
    /// theme --edit` refuses one. JSON `{ edited, files }`: what it did, and the theme file it
    /// wrote, `{ path, before, after }`, for the editor's undo to write back.
    #[wasm_bindgen(js_name = themeEdit)]
    pub fn theme_edit(
        &mut self,
        edit: &str,
        dry_run: bool,
        author: Option<String>,
        at: Option<String>,
    ) -> Result<String, JsError> {
        let edit: scaena_ops::theme::ThemeEdit = serde_json::from_str(edit).map_err(js)?;
        let by = assistant::Caller {
            author: author.as_deref().unwrap_or("user"),
            at: at.as_deref().and_then(store::seconds),
        };
        let (edited, files) = self.0.theme_edit(&edit, dry_run, by).map_err(js)?;
        serde_json::to_string(&serde_json::json!({ "edited": edited, "files": files })).map_err(js)
    }

    /// Files written back, as an undo or a redo of a restore or a theme edit has them: JSON
    /// `[{ path, text }]`, `text` null for a file to take out.
    #[wasm_bindgen(js_name = writeFiles)]
    pub fn write_files(&mut self, files: &str) -> Result<(), JsError> {
        let files: Vec<versions::Written> = serde_json::from_str(files).map_err(js)?;
        self.0.write_files(files);
        Ok(())
    }
}

#[cfg(feature = "editor")]
#[wasm_bindgen]
extern "C" {
    /// The module that keeps a bundle's history (`scaena-history`), as the page loaded it
    /// (PLAN 2.9): it records changes in a history's bytes.
    #[wasm_bindgen(typescript_type = "{ record(history: Uint8Array, changes: string): Uint8Array }")]
    pub type History;
    #[wasm_bindgen(method, catch)]
    fn record(this: &History, history: &[u8], changes: &str) -> Result<Vec<u8>, JsValue>;

    /// What a module throws: an `Error`, with its message.
    type Thrown;
    #[wasm_bindgen(method, getter)]
    fn message(this: &Thrown) -> Option<String>;
}

/// What `thrown` says: its message, or itself as text.
#[cfg(feature = "editor")]
fn said(thrown: &JsValue) -> String {
    let message = thrown.is_object().then(|| thrown.unchecked_ref::<Thrown>().message()).flatten();
    thrown.as_string().or(message).unwrap_or_else(|| format!("{thrown:?}"))
}

/// What a tool returned (PLAN 2.6).
#[cfg(feature = "editor")]
#[wasm_bindgen]
pub struct ToolResult {
    json: String,
    error: bool,
    edited: bool,
    frame: Option<scaena_paint::Raster>,
    rewritten: Vec<versions::Rewritten>,
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

    /// The files it wrote beside the deck that the editor's undo writes back, as JSON `[{ path,
    /// before, after }]`: the theme `theme_edit` edited (ADR-0016); empty from any other tool.
    #[wasm_bindgen(getter)]
    pub fn rewritten(&self) -> String {
        serde_json::to_string(&self.rewritten).unwrap_or_else(|_| "[]".into())
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
            let dl = self.0.viewed(state, t_ms).map_err(js)?;
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
