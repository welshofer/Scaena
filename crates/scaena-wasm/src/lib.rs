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
use std::sync::Arc;
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
    /// A call about a frame held for its shaders that no frame is, or that is not in it.
    #[error("{0}")]
    Shading(String),
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
    /// Shared, so an operation reads the bundle without copying its fonts.
    files: Arc<BTreeMap<String, Vec<u8>>>,
    /// The data files the deck names, as the engine reads them.
    data: DataFiles,
    /// The layout engine, built on the first frame from the fonts and images the deck
    /// names, and again on the first frame after it names others.
    engine: Option<Engine>,
    /// The transition last sampled, so the frames of one transition lay out once.
    transition: Option<(String, Transition)>,
    /// The state last asked what stands where, laid out at rest, so a pointer's every move
    /// lays out nothing (ADR-0013).
    rest: Option<(String, scaena_engine::sample::Scene)>,
    /// Where the node last asked about may go, by its state and its id: a drag asks again
    /// with every move, and lays nothing out (ADR-0013).
    targets: Option<(String, String, scaena_engine::geometry::Targets)>,
    /// The node a drag moves, and how far, canvas units: frames at rest draw it there, from
    /// the state as laid out at rest, laying nothing out (ADR-0013).
    moving: Option<(String, [f32; 2])>,
    /// The deck a patch would make, shown before it is made: frames at rest draw it, laid out
    /// once a frame, as a resize does when it pauses (ADR-0013).
    #[cfg(feature = "editor")]
    previewing: Option<Deck>,
    /// The format frames are laid out in (SPEC §3.4); `None` for the deck's own canvas.
    format: Option<String>,
    /// The fonts and images the engine was built from, as painters read them.
    store: Assets,
    /// The frame held while its shaders' rows are worked out (PLAN 2.28).
    #[cfg(feature = "cpu")]
    shading: Option<Shading>,
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
    /// What the next save records in the bundle's history, if it keeps one, besides the save:
    /// each edit an operation made since the bundle was opened or saved, after the deck
    /// before it (PLAN 2.9).
    #[cfg(feature = "editor")]
    recorded: Vec<scaena_store::crdt::Recorded>,
}

/// A frame held to be painted once its shaders' pixels are in (PLAN 2.28): its display
/// list, its scale, and each shader it draws.
#[cfg(feature = "cpu")]
struct Shading {
    dl: DisplayList,
    scale: f32,
    shaders: Vec<Shader>,
}

/// A shader a held frame draws: the spec its job is made from, the job, and its pixels,
/// filled in a band of rows at a time.
#[cfg(feature = "cpu")]
struct Shader {
    spec: scaena_core::shader::Spec,
    job: scaena_core::shader::Job,
    pixels: Vec<u8>,
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
            files: Arc::new(files),
            data: DataFiles::new(),
            engine: None,
            transition: None,
            rest: None,
            targets: None,
            moving: None,
            #[cfg(feature = "editor")]
            previewing: None,
            format: None,
            store: Assets::new(),
            #[cfg(feature = "cpu")]
            shading: None,
            #[cfg(feature = "editor")]
            theme_json: theme_json.to_string(),
            #[cfg(feature = "editor")]
            edit: None,
            #[cfg(feature = "editor")]
            laid: Vec::new(),
            #[cfg(feature = "editor")]
            subsets: BTreeMap::new(),
            #[cfg(feature = "editor")]
            recorded: Vec::new(),
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
        self.forget();
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
            self.forget();
        }
        Arc::make_mut(&mut self.files).insert(path.to_string(), bytes);
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

    /// Let go of what was laid out: the deck, its files, or its format changed. A drag, or a
    /// patch previewed, ends with it.
    fn forget(&mut self) {
        self.transition = None;
        self.rest = None;
        self.targets = None;
        self.moving = None;
        #[cfg(feature = "editor")]
        {
            self.previewing = None;
        }
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
            self.forget();
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
            self.forget();
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
    /// rest, the state laid out alone, once, and kept until the deck, its files, or the format
    /// change: what stands where is read from the same layout. Inside its cue, a sample of
    /// it, laid out on the first such frame and kept: the frames of one cue lay out once
    /// (SPEC §5).
    pub fn frame(&mut self, state: &str, t_ms: f64) -> Result<DisplayList, Error> {
        let timeline = self.timeline()?;
        let slot = timeline.slot(state).ok_or_else(|| EngineError::UnknownState(state.to_string()))?;
        let (start, span) = (slot.start, slot.span);
        // The engine is built by now: the timeline took it.
        let format = self.format.as_deref();
        if t_ms.is_nan() || t_ms >= span {
            #[cfg(feature = "editor")]
            if let Some(deck) = &self.previewing {
                let engine = self.engine.as_mut().expect("built for the timeline");
                let req = FrameRequest { deck, theme: &self.theme, data: &self.data, state, t_ms, format };
                return Ok(engine.frame(&req)?.display_list);
            }
            // Its shaders at the time it comes to rest, or `t_ms` into its hold.
            let time = (start + if t_ms.is_finite() { t_ms } else { span }) / 1000.0;
            let moving = self.moving.clone();
            let scene = self.at_rest(state)?;
            return Ok(match moving {
                Some((node, by)) => scene.moved(time, &node, by),
                None => scene.draw_at(time),
            });
        }
        let engine = self.engine.as_mut().expect("built for the timeline");
        let cached = self.transition.as_ref().is_some_and(|(s, _)| s == state);
        if !cached {
            let (deck, theme) = project(&self.deck, &self.theme, format)?;
            let transition = engine.transition(&deck, &theme, &self.data, state)?;
            self.transition = Some((state.to_string(), transition));
        }
        Ok(self.transition.as_ref().expect("set above").1.frame(t_ms))
    }

    /// `state` at rest in the format shown, laid out once and kept until the deck, its
    /// files, or the format change.
    fn at_rest(&mut self, state: &str) -> Result<&scaena_engine::sample::Scene, Error> {
        if self.rest.as_ref().is_none_or(|(s, _)| s != state) {
            // The engine is built by now: the span took it.
            self.duration(state)?;
            let engine = self.engine.as_mut().expect("built for the span");
            let format = self.format.as_deref();
            let req = FrameRequest {
                deck: &self.deck,
                theme: &self.theme,
                data: &self.data,
                state,
                t_ms: f64::INFINITY,
                format,
            };
            self.rest = Some((state.to_string(), engine.at_rest(&req)?));
        }
        Ok(&self.rest.as_ref().expect("laid out above").1)
    }

    /// Each visible node's box in `state` at rest, in the format shown (ADR-0013): those that
    /// draw, in paint order, then the containers and groups that only hold others.
    pub fn boxes(&mut self, state: &str) -> Result<Vec<scaena_engine::geometry::NodeBox>, Error> {
        Ok(self.at_rest(state)?.boxes())
    }

    /// The nodes that draw at `point` (canvas units) in `state` at rest, in the format shown,
    /// topmost first, each with the containers it sits in (ADR-0013).
    pub fn hit(&mut self, state: &str, point: [f32; 2]) -> Result<Vec<scaena_engine::geometry::Hit>, Error> {
        Ok(self.at_rest(state)?.hit(point))
    }

    /// Where `node` may go in `state` at rest, in the format shown (ADR-0013): what holds
    /// it, its cell, and the tracks, slots, or order a drag snaps it to.
    pub fn targets(&mut self, state: &str, node: &str) -> Result<&scaena_engine::geometry::Targets, Error> {
        if self.targets.as_ref().is_none_or(|(s, n, _)| s != state || n != node) {
            self.duration(state)?;
            let engine = self.engine.as_mut().expect("built for the span");
            let format = self.format.as_deref();
            let req = FrameRequest {
                deck: &self.deck,
                theme: &self.theme,
                data: &self.data,
                state,
                t_ms: f64::INFINITY,
                format,
            };
            let found = engine.targets(&req, node)?;
            self.targets = Some((state.to_string(), node.to_string(), found));
        }
        Ok(&self.targets.as_ref().expect("found above").2)
    }

    /// Where the box `to` (`node`'s cell as a drag left it) lands in `state` when it snaps
    /// `how`, with the patch that puts the node there, or, to `fork` it, keeps it to `state`;
    /// `None` where nothing places the node that way.
    #[cfg(feature = "editor")]
    pub fn snap(
        &mut self,
        state: &str,
        node: &str,
        how: scaena_ops::inspect::SnapMode,
        to: [f32; 4],
        fork: bool,
    ) -> Result<Option<scaena_ops::inspect::Snapped>, Error> {
        let found = self.targets(state, node)?.clone();
        scaena_ops::inspect::snap(&found, how, to, state, fork).map_err(|e| Error::Deck(e.to_string()))
    }

    /// Draw a node, and what it holds, `by` canvas units from where it stands in the frames at
    /// rest that follow, from the state as laid out at rest: what a drag shows as it moves,
    /// laying nothing out (ADR-0013). `None` puts it back.
    pub fn set_moving(&mut self, moving: Option<(String, [f32; 2])>) {
        self.moving = moving;
    }

    /// Show `ops` (a patch) as if it were made, without making it: frames at rest draw the
    /// deck it would make, laid out once a frame, until it is let go (`None`) or the deck
    /// changes. A resize shows its text reflowed this way when it pauses (ADR-0013).
    #[cfg(feature = "editor")]
    pub fn preview(&mut self, ops: Option<&[serde_json::Value]>) -> Result<(), Error> {
        self.previewing = match ops {
            None => None,
            Some(ops) => {
                let doc = self.deck.to_value().map_err(|e| Error::Deck(e.to_string()))?;
                let compiled = scaena_core::patch::compile(&doc, ops, &editor::Handed(&self.files))
                    .map_err(|e| Error::Ops(e.to_string()))?;
                Some(Deck::from_value(&compiled.doc).map_err(Error::Deck)?)
            }
        };
        Ok(())
    }

    /// The states `ops` (a patch) would change what shows in, by id, with nothing made,
    /// validated, or linted: what the editor says of a drag before it is dropped ("in 3
    /// states", ADR-0013).
    #[cfg(feature = "editor")]
    pub fn reach(&self, ops: &[serde_json::Value]) -> Result<Vec<String>, Error> {
        scaena_ops::patch::reach(&self.deck, &editor::Handed(&self.files), ops).map_err(|e| Error::Ops(e.to_string()))
    }

    /// Where a caret stands in `node`'s text in `state` at rest, in the format shown
    /// (ADR-0013, PLAN 2.32): each character as written, on its line, from the layout the
    /// frames at rest draw. `None` for a node that is no text there.
    pub fn carets(&mut self, state: &str, node: &str) -> Result<Option<scaena_engine::carets::Carets>, Error> {
        Ok(self.at_rest(state)?.carets(node))
    }

    /// What an inspector offers for `node` as `state` shows it (ADR-0013, PLAN 2.33): each
    /// property it edits, with the theme's names for it or what the schema allows, the value
    /// shown, and where that value lives, which is where a `choose` patch writes.
    pub fn choices(&self, state: &str, node: &str) -> Result<scaena_core::choices::Choices, Error> {
        scaena_core::choices::choices(&self.deck, &self.theme, state, node).map_err(Error::Ops)
    }

    /// What may be inserted in the deck (PLAN 2.34): a text in each of the theme's roles, each
    /// kind of shape, each image in the bundle, and each shader preset, each as `add_node`
    /// adds it, with the box it takes at first.
    pub fn inserts(&self) -> Vec<scaena_core::inserts::Insert> {
        let paths: Vec<String> = self.files.keys().cloned().collect();
        scaena_core::inserts::inserts(&self.deck, &self.theme, &paths)
    }

    /// The patch that inserts what [`Session::inserts`] offers `n`th in `state`, the box it
    /// starts as about `at` (canvas units, in the format shown), snapped to the theme's grid
    /// as a drop snaps; or into the slot it fills (PLAN 2.34). Its id is new to the deck.
    #[cfg(feature = "editor")]
    pub fn inserting(&mut self, state: &str, n: usize, at: [f32; 2]) -> Result<scaena_ops::inspect::Added, Error> {
        use scaena_core::inserts::{Start, fresh};
        let offered = self.inserts().into_iter().nth(n);
        let insert = offered.ok_or_else(|| Error::Ops(format!("nothing is offered at {n}")))?;
        let id = fresh(&self.deck, &insert.id);
        let share = match insert.start {
            Start::Box { w, h } => [w, h],
            Start::Slot(_) => [1.0, 1.0],
        };
        self.duration(state)?;
        let engine = self.engine.as_mut().expect("built for the span");
        let format = self.format.as_deref();
        let req =
            FrameRequest { deck: &self.deck, theme: &self.theme, data: &self.data, state, t_ms: f64::INFINITY, format };
        let room = engine.room(&req, &id, share)?;
        scaena_ops::inspect::inserting(&self.deck, &room, &insert, state, at).map_err(|e| Error::Deck(e.to_string()))
    }

    /// The patch that copies `node`, as `state` shows it, with what it holds there, beside it
    /// under an id new to the deck, clear of the rest where there is room (PLAN 2.34).
    #[cfg(feature = "editor")]
    pub fn duplicating(&mut self, state: &str, node: &str) -> Result<scaena_ops::inspect::Added, Error> {
        let mut found = self.targets(state, node)?.clone();
        found.node = scaena_core::inserts::fresh(&self.deck, node);
        let boxes = self.boxes(state)?;
        scaena_ops::inspect::duplicating(&self.deck, &found, node, state, &boxes)
            .map_err(|e| Error::Deck(e.to_string()))
    }

    /// The patch that deletes `node` from `state`, with what it holds there (PLAN 2.34): each
    /// leaves there and in the states after it, and one no state shows then goes from the deck;
    /// or, `everywhere`, each goes from the deck.
    #[cfg(feature = "editor")]
    pub fn deleting(&self, state: &str, node: &str, everywhere: bool) -> Result<Vec<serde_json::Value>, Error> {
        scaena_ops::inspect::deleting(&self.deck, &editor::Handed(&self.files), node, state, everywhere)
            .map_err(|e| Error::Ops(e.to_string()))
    }

    /// The patch that adds a state after `state`, the state shown (PLAN 2.35): a step of its
    /// slide, tracking from it, or a slide of its own, empty, after the slide's last step.
    #[cfg(feature = "editor")]
    pub fn adding_state(
        &self,
        state: &str,
        what: scaena_ops::states::Adding,
    ) -> Result<scaena_ops::states::AddedState, Error> {
        scaena_ops::states::adding(&self.deck, state, what).map_err(|e| Error::Ops(e.to_string()))
    }

    /// Text typed on the canvas (ADR-0013, PLAN 2.32): `ops` (a `replace_text`) made by
    /// `user` at `at` (seconds since the epoch), validated and refused as a patch is but not
    /// linted: the page lints the state it shows after, as it does after a keystroke in the
    /// source. A bundle's history records a run of it as one change, `type`. Whether it
    /// changed the deck.
    #[cfg(feature = "editor")]
    pub fn typed(&mut self, ops: &serde_json::Value, at: Option<i64>) -> Result<bool, Error> {
        let (patched, write) = scaena_ops::patch::typing(&self.bundle(), ops, Some(store::TYPED))?;
        if patched.refused {
            let why = patched.added.iter().find(|f| f.severity == scaena_core::lint::Severity::Error);
            return Err(Error::Ops(format!("the deck refuses it: {}", why.map_or("", |f| f.message.as_str()))));
        }
        self.write(write, assistant::Caller { author: "user", at })
    }

    /// How `state` reads at rest, in the format shown, as HTML (SPEC §3.12): what the page
    /// shows a screen reader, unseen, in a live region (PLAN 2.8), and what a single-file
    /// export carries for each state it plays (PLAN 2.5).
    pub fn reading(&mut self, state: &str) -> Result<String, Error> {
        let list = self.frame(state, f64::INFINITY)?;
        let snapshots = scaena_core::resolve_states(&self.deck).map_err(|e| Error::Deck(e.to_string()))?;
        let snap = (snapshots.iter().find(|s| s.state_id == state))
            .ok_or_else(|| EngineError::UnknownState(state.to_string()))?;
        Ok(scaena_core::reading::html(&self.deck, snap, &list))
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

    /// Hold [`Session::pixels`]' frame until its shaders' pixels are in, and say how many
    /// shaders it draws: each one's rows are worked out in bands, here ([`Session::shade`])
    /// or on another worker from its spec ([`Session::shader_spec`], [`shader_rows`]), then
    /// [`Session::shaded`] paints the frame (PLAN 2.28). A frame held before is let go.
    #[cfg(feature = "cpu")]
    pub fn shading(&mut self, state: &str, t_ms: f64, width: u32) -> Result<usize, Error> {
        self.shading = None;
        let dl = self.frame(state, t_ms)?;
        let scale = width as f32 / dl.viewport[0];
        let mut shaders = Vec::new();
        for spec in scaena_paint::shader_specs(&dl, scale)? {
            if let Some(job) = spec.job().map_err(PaintError::from)? {
                let [_, _, w, h] = job.bbox();
                shaders.push(Shader { spec, job, pixels: vec![0; w as usize * h as usize * 4] });
            }
        }
        let count = shaders.len();
        self.shading = Some(Shading { dl, scale, shaders });
        Ok(count)
    }

    /// The held frame's shader `i`.
    #[cfg(feature = "cpu")]
    fn shader(&mut self, i: usize) -> Result<&mut Shader, Error> {
        let shading = self.shading.as_mut().ok_or_else(|| Error::Shading("no frame is held for its shaders".into()))?;
        let count = shading.shaders.len();
        let none = || Error::Shading(format!("the frame held draws {count} shaders; there is no shader {i}"));
        shading.shaders.get_mut(i).ok_or_else(none)
    }

    /// The spec shader `i` of the held frame is made from: what [`shader_rows`] takes.
    #[cfg(feature = "cpu")]
    pub fn shader_spec(&mut self, i: usize) -> Result<Vec<u8>, Error> {
        Ok(self.shader(i)?.spec.to_bytes().map_err(PaintError::from)?)
    }

    /// How shader `i`'s rows split among `workers`: each band's first row and its rows, as
    /// `Job::render_on` splits them among threads (`scaena_core::shader::bands`).
    #[cfg(feature = "cpu")]
    pub fn shader_bands(&mut self, i: usize, workers: usize) -> Result<Vec<[u32; 2]>, Error> {
        let [_, _, _, h] = self.shader(i)?.job.bbox();
        Ok(scaena_core::shader::bands(h, workers))
    }

    /// The bytes of shader `i`'s rows `first..first + rows` in the held frame, to fill.
    #[cfg(feature = "cpu")]
    pub fn shader_band(&mut self, i: usize, first: u32, rows: u32) -> Result<&mut [u8], Error> {
        let Shader { job, pixels, .. } = self.shader(i)?;
        let [_, _, w, h] = job.bbox();
        let row = w as usize * 4;
        match first.checked_add(rows).is_some_and(|end| end <= h) {
            true => Ok(&mut pixels[first as usize * row..(first + rows) as usize * row]),
            false => Err(Error::Shading(format!("shader {i} has {h} rows; {rows} from row {first} are not all in it"))),
        }
    }

    /// Work out shader `i`'s rows `first..first + rows` here, into the held frame.
    #[cfg(feature = "cpu")]
    pub fn shade(&mut self, i: usize, first: u32, rows: u32) -> Result<(), Error> {
        let job = self.shader(i)?.job.clone();
        job.render_rows(first, self.shader_band(i, first, rows)?);
        Ok(())
    }

    /// The held frame painted with its shaders' pixels, as [`Session::pixels`] paints it;
    /// the frame is let go.
    #[cfg(feature = "cpu")]
    pub fn shaded(&mut self) -> Result<scaena_paint::Raster, Error> {
        let Shading { dl, scale, shaders } =
            self.shading.take().ok_or_else(|| Error::Shading("no frame is held for its shaders".into()))?;
        let pixels = shaders.into_iter().map(|shader| shader.pixels).collect();
        Ok(scaena_paint::cpu::CpuPainter::default().paint_shaded(&dl, &self.store, scale, pixels)?)
    }
}

/// Rows `first..first + rows` of the shader whose spec is `spec` ([`Session::shader_spec`]):
/// what a worker that holds no deck works out for one that does (PLAN 2.28). The same job,
/// made again from the spec's bytes, gives the same bytes.
#[cfg(feature = "cpu")]
pub fn shader_rows(spec: &[u8], first: u32, rows: u32) -> Result<Vec<u8>, Error> {
    let spec = scaena_core::shader::Spec::from_bytes(spec).map_err(PaintError::from)?;
    let job =
        spec.job().map_err(PaintError::from)?.ok_or_else(|| Error::Shading("the shader covers no pixel".into()))?;
    let [_, _, w, h] = job.bbox();
    if first.checked_add(rows).is_none_or(|end| end > h) {
        return Err(Error::Shading(format!("the shader has {h} rows; {rows} from row {first} are not all in it")));
    }
    let mut out = vec![0; rows as usize * w as usize * 4];
    job.render_rows(first, &mut out);
    Ok(out)
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

    /// What identifies `state`'s drawing at rest, in the format shown: its display list's
    /// digest (FNV-1a over its postcard bytes, as the goldens keep it). One digest, one
    /// drawing, so a thumbnail painted from it stands until it changes (PLAN 2.35).
    pub fn digest(&mut self, state: &str) -> Result<String, JsError> {
        self.0.frame(state, f64::INFINITY).map_err(js)?.digest().map_err(js)
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
        let row = (self.0.shader(i as usize).map_err(js)?.job.bbox()[2] as usize * 4).max(1);
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

/// Direct manipulation (ADR-0013): what stands where in a state at rest, and where a node may
/// go, for the editor's canvas. The player's module leaves it out.
#[cfg(feature = "editor")]
#[wasm_bindgen]
impl Player {
    /// Each visible node's box in `state` at rest, in the format shown, as JSON (ADR-0013):
    /// `[{ "node", "rect": [x, y, w, h], "parent"?, "draws" }]`, canvas units, those that draw
    /// in paint order, then the containers and groups that only hold others.
    pub fn boxes(&mut self, state: &str) -> Result<String, JsError> {
        let boxes: Vec<serde_json::Value> = (self.0.boxes(state).map_err(js)?.into_iter())
            .map(|b| serde_json::json!({ "node": b.node, "rect": b.rect, "parent": b.parent, "draws": b.draws }))
            .collect();
        serde_json::to_string(&boxes).map_err(js)
    }

    /// The nodes that draw at `x`, `y` (canvas units) in `state` at rest, in the format shown,
    /// topmost first, as JSON (ADR-0013): `[{ "node", "rect", "containers" }]`, each node's
    /// containers innermost first.
    pub fn hit(&mut self, state: &str, x: f32, y: f32) -> Result<String, JsError> {
        let hits: Vec<serde_json::Value> = (self.0.hit(state, [x, y]).map_err(js)?.into_iter())
            .map(|h| serde_json::json!({ "node": h.node, "rect": h.rect, "containers": h.containers }))
            .collect();
        serde_json::to_string(&hits).map_err(js)
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
    /// "patch" }`, the patch the place ops that put the node there, kept to `state` when they
    /// `fork`; `null` where nothing places the node that way. Asked with each move of a drag,
    /// it lays nothing out.
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
    ) -> Result<String, JsError> {
        let how: scaena_ops::inspect::SnapMode = how.parse().map_err(|e: String| JsError::new(&e))?;
        let snapped = self.0.snap(state, node, how, [x, y, w, h], fork).map_err(js)?;
        serde_json::to_string(&snapped).map_err(js)
    }

    /// Draw `node`, and what it holds, `dx`, `dy` canvas units from where it stands in the
    /// frames at rest that follow, laying nothing out: what a drag shows as it moves. Without
    /// `node`, every node stands where it is.
    #[wasm_bindgen(js_name = setMoving)]
    pub fn set_moving(&mut self, node: Option<String>, dx: f32, dy: f32) {
        self.0.set_moving(node.map(|node| (node, [dx, dy])));
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
    /// lead, trail]] }] }`, canvas units; `null` for a node that is no text there.
    pub fn carets(&mut self, state: &str, node: &str) -> Result<String, JsError> {
        let Some(c) = self.0.carets(state, node).map_err(js)? else { return Ok("null".into()) };
        let units = utf16(&c.text);
        let lines: Vec<serde_json::Value> = (c.lines.iter())
            .map(|l| {
                let chars: Vec<_> =
                    l.chars.iter().map(|ch| serde_json::json!([units(ch.offset), ch.lead, ch.trail])).collect();
                serde_json::json!({
                    "top": l.top, "bottom": l.bottom, "x": l.x, "start": units(l.start), "end": units(l.end),
                    "broken": l.broken, "chars": chars,
                })
            })
            .collect();
        serde_json::to_string(&serde_json::json!({ "text": c.text, "lines": lines })).map_err(js)
    }

    /// What an inspector offers for `node` as `state` shows it, as JSON (ADR-0013, PLAN 2.33):
    /// `{ node, type, state, fields }`, as `scaena inspect --choices` says it.
    pub fn choices(&self, state: &str, node: &str) -> Result<String, JsError> {
        serde_json::to_string(&self.0.choices(state, node).map_err(js)?).map_err(js)
    }

    /// What may be inserted, as JSON (PLAN 2.34): `[{ label, node, id, start }]`, as `scaena
    /// inspect --inserts` says it.
    pub fn inserts(&self) -> Result<String, JsError> {
        serde_json::to_string(&self.0.inserts()).map_err(js)
    }

    /// The patch that inserts what `inserts` offers `n`th in `state`, about `x`, `y` (canvas
    /// units), as JSON: `{ id, cell, patch }` (PLAN 2.34).
    pub fn inserting(&mut self, state: &str, n: usize, x: f32, y: f32) -> Result<String, JsError> {
        serde_json::to_string(&self.0.inserting(state, n, [x, y]).map_err(js)?).map_err(js)
    }

    /// The patch that copies `node` beside it in `state`, as JSON: `{ id, cell, patch }`
    /// (PLAN 2.34).
    pub fn duplicating(&mut self, state: &str, node: &str) -> Result<String, JsError> {
        serde_json::to_string(&self.0.duplicating(state, node).map_err(js)?).map_err(js)
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

    /// Make `ops` (JSON: a `replace_text`, typed on the canvas) as `user` at `at` (RFC 3339),
    /// validated and refused as a patch is, but not linted (ADR-0013, PLAN 2.32). Whether it
    /// changed the deck.
    pub fn typed(&mut self, ops: &str, at: Option<String>) -> Result<bool, JsError> {
        let ops: serde_json::Value = serde_json::from_str(ops).map_err(js)?;
        self.0.typed(&ops, at.as_deref().and_then(store::seconds)).map_err(js)
    }
}

/// Byte offsets in `text` as UTF-16 code units, as the page counts a string.
#[cfg(feature = "editor")]
fn utf16(text: &str) -> impl Fn(usize) -> usize + '_ {
    let mut at: Vec<(usize, usize)> = Vec::with_capacity(text.len() + 1);
    let mut units = 0;
    for (byte, c) in text.char_indices() {
        at.push((byte, units));
        units += c.len_utf16();
    }
    at.push((text.len(), units));
    move |byte| at[at.partition_point(|&(b, _)| b < byte).min(at.len() - 1)].1
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
        let mut given = BTreeMap::new();
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
            Ok(c) => ToolResult { json: c.result, error: false, edited: c.edited, frame: c.frame },
            Err(e) => {
                let json = assistant::failure(&e).to_string();
                ToolResult { json, error: true, edited: false, frame: None }
            }
        }
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

    /// A frame held for its shaders, its rows worked out in bands, some here and the rest
    /// from their spec as another worker would, paints what `pixels` paints (PLAN 2.28); a
    /// call about a frame that is not held, or rows not in it, is an error.
    #[cfg(feature = "cpu")]
    #[test]
    fn shaders_worked_out_in_bands_paint_what_pixels_paints() {
        let mut s = torture();
        for (state, t) in [("shaders", f64::INFINITY), ("shaders", 300.0), ("images", f64::INFINITY)] {
            let width = 1280;
            let want = s.pixels(state, t, width).unwrap();
            let count = s.shading(state, t, width).unwrap();
            assert_eq!(count > 0, state == "shaders", "{state}");
            for i in 0..count {
                let spec = s.shader_spec(i).unwrap();
                for (n, [first, rows]) in s.shader_bands(i, 3).unwrap().into_iter().enumerate() {
                    match n {
                        0 => s.shade(i, first, rows).unwrap(),
                        _ => s
                            .shader_band(i, first, rows)
                            .unwrap()
                            .copy_from_slice(&shader_rows(&spec, first, rows).unwrap()),
                    }
                }
            }
            assert!(s.shaded().unwrap() == want, "{state} at {t}");
        }
        assert!(matches!(s.shaded(), Err(Error::Shading(_))));
        assert!(matches!(s.shade(0, 0, 1), Err(Error::Shading(_))));
        let count = s.shading("shaders", f64::INFINITY, 640).unwrap();
        assert!(matches!(s.shader_spec(count), Err(Error::Shading(_))));
        let [first, rows] = *s.shader_bands(0, 1).unwrap().last().unwrap();
        assert!(matches!(s.shade(0, first, rows + 1), Err(Error::Shading(_))));
        assert!(matches!(s.shader_band(0, u32::MAX, 2), Err(Error::Shading(_))));
        let spec = s.shader_spec(0).unwrap();
        assert!(matches!(shader_rows(&spec, first + rows, 1), Err(Error::Shading(_))));
        assert!(matches!(shader_rows(&spec[..spec.len() / 2], 0, 1), Err(Error::Paint(PaintError::Shader(_)))));
    }

    /// What stands where comes from the state at rest in the format shown, laid out once and
    /// kept until the deck or the format changes (ADR-0013).
    #[test]
    fn a_state_at_rest_says_what_stands_where() {
        let mut s = torture();
        let boxes = s.boxes("containers").unwrap();
        let label = boxes.iter().find(|b| b.node == "card-tag-label").unwrap();
        let [x, y, w, h] = label.rect;
        let hits = s.hit("containers", [x + w / 2.0, y + h / 2.0]).unwrap();
        assert_eq!(hits[0].node, "card-tag-label");
        assert_eq!(hits[0].containers, ["card"]);
        // Kept: asking again lays out nothing, and answers the same.
        assert!(s.rest.as_ref().is_some_and(|(state, _)| state == "containers"));
        assert_eq!(s.boxes("containers").unwrap(), boxes);
        // Another format lays the deck out again, and the boxes are that layout's.
        s.set_format(Some("9:16")).unwrap();
        assert!(s.rest.is_none());
        let tall = s.boxes("formats").unwrap();
        assert!(tall.iter().all(|b| b.rect[0] + b.rect[2] <= 1080.01), "{tall:?}");
        assert!(matches!(s.boxes("nowhere"), Err(Error::Engine(EngineError::UnknownState(_)))));
    }

    /// A drag asks where its node may go once, and each move after snaps with what it was
    /// told, laying nothing out. The patch it ends with applies as any edit does, and the
    /// node stands where the drag said (ADR-0013).
    #[cfg(feature = "editor")]
    #[test]
    fn a_drag_snaps_its_node_and_the_patch_puts_it_there() {
        use scaena_ops::inspect::SnapMode;
        let mut s = torture();
        let found = s.targets("containers", "tally").unwrap().clone();
        let (cell, pitch) = (found.cell, found.columns[1][0] - found.columns[0][0]);
        let mut snapped = None;
        for dx in [10.0, 0.6 * pitch, 1.1 * pitch] {
            snapped = s
                .snap("containers", "tally", SnapMode::Move, [cell[0] + dx, cell[1], cell[2], cell[3]], false)
                .unwrap();
            assert!(
                s.targets
                    .as_ref()
                    .is_some_and(|(state, node, t)| state == "containers" && node == "tally" && *t == found)
            );
        }
        let snapped = snapped.unwrap();
        assert_eq!(
            snapped.patch,
            [
                serde_json::json!({ "op": "place", "node": "tally", "at": { "col": [2, 12], "row": 5 }, "state": "containers" })
            ]
        );
        let by = assistant::Caller { author: "user", at: None };
        s.tool("deck_patch", serde_json::json!({ "ops": snapped.patch }), by).unwrap();
        assert!(s.targets.is_none(), "the deck changed, and with it where things stand");
        assert_eq!(s.targets("containers", "tally").unwrap().cell, snapped.cell);
        // A way that does not place a node is no target, and a node not on screen is an error.
        assert!(s.snap("containers", "stat-a", SnapMode::Move, cell, false).unwrap().is_none());
        assert!(s.targets("containers", "title").is_err());
    }

    /// A drag shows its node moved in the frames at rest, from the layout the session keeps,
    /// and nothing is laid out while it moves. Before a drop, the editor hears which states
    /// the patch changes, as the patch itself says once made; a resize previews the frame its
    /// patch makes, and a drop makes it (ADR-0013).
    #[cfg(feature = "editor")]
    #[test]
    fn a_drag_shows_its_node_moved_and_a_resize_its_patch() {
        use scaena_ops::inspect::SnapMode;
        let mut s = torture();
        let still = s.frame("containers", f64::INFINITY).unwrap();
        assert!(s.rest.as_ref().is_some_and(|(state, _)| state == "containers"), "laid out once, and kept");
        assert_eq!(s.boxes("containers").unwrap().len(), s.rest.as_ref().unwrap().1.boxes().len());

        s.set_moving(Some(("tally".into(), [30.0, 0.0])));
        let moved = s.frame("containers", f64::INFINITY).unwrap();
        assert_ne!(moved, still);
        assert!(s.rest.as_ref().is_some_and(|(state, _)| state == "containers"));
        s.set_moving(None);
        assert_eq!(s.frame("containers", f64::INFINITY).unwrap(), still);

        // Which states a move changes, said before it is made, and by the patch once made.
        let found = s.targets("containers", "tally").unwrap().clone();
        let (cell, pitch) = (found.cell, found.columns[1][0] - found.columns[0][0]);
        let to = [cell[0] + pitch, cell[1], cell[2], cell[3]];
        let snapped = s.snap("containers", "tally", SnapMode::Move, to, false).unwrap().unwrap();
        let reach = s.reach(&snapped.patch).unwrap();
        assert!(reach.contains(&"containers".to_string()), "{reach:?}");
        let by = assistant::Caller { author: "user", at: None };
        let dry = s.tool("deck_patch", serde_json::json!({ "ops": snapped.patch, "dry_run": true }), by).unwrap();
        let said: serde_json::Value = serde_json::from_str(&dry.result).unwrap();
        assert_eq!(said["states"], serde_json::json!(reach));
        // Kept to the state, it changes no state it did not.
        let forked = s.snap("containers", "tally", SnapMode::Move, to, true).unwrap().unwrap();
        assert_eq!(forked.patch[0]["fork"], true);
        let kept = s.reach(&forked.patch).unwrap();
        assert!(kept.contains(&"containers".to_string()) && kept.iter().all(|state| reach.contains(state)));

        // A resize, previewed: the frame its patch makes, with nothing made until it is. The
        // board's areas fill it, so a narrower board draws them narrower.
        let board = s.targets("containers", "board").unwrap().cell;
        let narrower = [board[0], board[1], board[2] - pitch, board[3]];
        let resize = s.snap("containers", "board", SnapMode::Resize, narrower, false).unwrap().unwrap();
        assert!(!resize.patch.is_empty());
        s.preview(Some(&resize.patch)).unwrap();
        let previewed = s.frame("containers", f64::INFINITY).unwrap();
        assert_ne!(previewed, still);
        s.preview(None).unwrap();
        assert_eq!(s.frame("containers", f64::INFINITY).unwrap(), still);
        s.preview(Some(&resize.patch)).unwrap();
        s.set_moving(Some(("tally".into(), [30.0, 0.0])));
        s.tool("deck_patch", serde_json::json!({ "ops": resize.patch }), by).unwrap();
        assert!(s.moving.is_none() && s.previewing.is_none(), "the deck changed: the drag is over");
        assert_eq!(s.frame("containers", f64::INFINITY).unwrap(), previewed);
    }

    /// Text on the canvas (ADR-0013, PLAN 2.32): where a caret stands in a text, from the
    /// layout the frames at rest draw; and typing, a `replace_text` written where the text
    /// lives, which the next frame shows. A keystroke the deck cannot take says why.
    #[cfg(feature = "editor")]
    #[test]
    fn typing_on_the_canvas_writes_where_the_text_lives() {
        let mut s = revenue();
        let before = s.frame("revenue", f64::INFINITY).unwrap();
        let carets = s.carets("revenue", "title").unwrap().unwrap();
        assert_eq!(carets.text, "Revenue doubled", "as `revenue` shows it");
        assert!(s.rest.as_ref().is_some_and(|(state, _)| state == "revenue"), "from the layout kept at rest");
        assert!(s.carets("revenue", "rev").unwrap().is_none(), "a chart is no text");

        let ops = serde_json::json!([
            { "op": "replace_text", "node": "title", "state": "revenue", "from": 8, "to": 15, "text": "tripled" }
        ]);
        assert!(s.typed(&ops, None).unwrap());
        let deck = s.deck.to_value().unwrap();
        assert_eq!(deck["states"][1]["props"]["title"]["text"], "Revenue tripled", "where `revenue` sets it");
        assert_eq!(deck["nodes"]["title"]["text"], "Q3 Review");
        assert_eq!(s.carets("revenue", "title").unwrap().unwrap().text, "Revenue tripled");
        assert_ne!(s.frame("revenue", f64::INFINITY).unwrap(), before, "the frame shows it");

        let past = serde_json::json!([{ "op": "replace_text", "node": "title", "state": "revenue", "from": 99, "to": 99, "text": "!" }]);
        let e = s.typed(&past, None).unwrap_err();
        assert!(e.to_string().contains("15 characters"), "{e}");
    }

    /// A page reads a caret from the deck its source says (PLAN 2.32): the session says
    /// whether that deck is the one shown, so the page compiles the source first when it is not.
    #[test]
    fn the_session_says_whether_the_deck_shown_is_compiled_from_a_source() {
        let mut s = revenue();
        let source = s.source();
        assert!(!s.compiled_from(&source), "nothing is compiled yet");
        assert!(s.compile(&source).valid);
        assert!(s.compiled_from(&source));
        assert!(!s.compiled_from(&source.replace("Revenue doubled", "Revenue tripled")));

        let ops = serde_json::json!([
            { "op": "replace_text", "node": "title", "state": "revenue", "from": 0, "to": 0, "text": "Net " }
        ]);
        assert!(s.typed(&ops, None).unwrap());
        assert!(!s.compiled_from(&source), "typing wrote over it");
        let typed = s.source();
        assert!(s.compile(&typed).valid);
        assert!(s.compiled_from(&typed));

        // A source that compiles but does not validate leaves the deck shown as it was.
        let invalid = typed.replacen("role:display", "role:nowhere", 1);
        assert!(!s.compile(&invalid).valid);
        assert!(!s.compiled_from(&invalid));
        assert!(!s.compiled_from(&typed), "the deck shown is from it, but it is not the source compiled last");
    }

    /// The revenue example, its files handed over as a page hands them.
    fn revenue() -> Session {
        let dir = "../../docs/examples";
        let read = |p: &str| std::fs::read_to_string(format!("{dir}/{p}")).unwrap();
        let mut s = Session::new(&read("revenue.deck.json"), &read("themes/dusk.theme.json")).unwrap();
        for path in ["fonts/Fraunces-VF.ttf", "fonts/Inter-VF.ttf", "fonts/JetBrainsMono-VF.ttf", "data/q3-revenue.csv"]
        {
            s.add_file(path, std::fs::read(format!("{dir}/{path}")).unwrap());
        }
        s
    }

    /// What may be inserted goes where the pointer is, and a copy of a node beside it (PLAN
    /// 2.34): each patch adds a node new to the deck, entering in the state shown, placed on
    /// the theme's grid as a drop places it; a shader fills the canvas under the rest.
    #[cfg(feature = "editor")]
    #[test]
    fn an_insert_lands_where_the_pointer_is_and_a_copy_beside_its_node() {
        let mut s = revenue();
        let by = assistant::Caller { author: "user", at: None };
        let offered = s.inserts();
        let of = |kind: &str| offered.iter().filter(|i| i.node["type"] == kind).count();
        assert_eq!(of("text"), s.theme.typography.roles.len(), "a text in each of Dusk's roles");
        assert_eq!((of("shape"), of("image"), of("shader")), (4, 0, 2), "the revenue bundle holds no PNG");
        let n = |label: &str| offered.iter().position(|i| i.label == label).unwrap();
        let within =
            |c: [f32; 4], p: [f32; 2]| c[0] <= p[0] && p[0] <= c[0] + c[2] && c[1] <= p[1] && p[1] <= c[1] + c[3];
        let stands = |s: &mut Session, state: &str, node: &str| {
            s.boxes(state).unwrap().into_iter().find(|b| b.node == node).map(|b| b.rect)
        };

        // A headline about the middle of the canvas, in `revenue`.
        let added = s.inserting("revenue", n("Text · headline"), [960.0, 540.0]).unwrap();
        assert_eq!(added.id, "headline");
        let ops: Vec<&str> = added.patch.iter().map(|op| op["op"].as_str().unwrap()).collect();
        assert_eq!(ops, ["add_node", "place"]);
        assert_eq!(added.patch[0]["state"], "revenue", "it enters in the state shown");
        assert!(within(added.cell, [960.0, 540.0]), "{:?}", added.cell);
        s.tool("deck_patch", serde_json::json!({ "ops": added.patch }), by).unwrap();
        assert_eq!(stands(&mut s, "revenue", "headline"), Some(added.cell));
        assert_eq!(stands(&mut s, "intro", "headline"), None, "nor before it");
        assert!(stands(&mut s, "mix", "headline").is_some(), "the states after it track it");

        // A copy beside it, under the next free id, clear of it.
        let copy = s.duplicating("revenue", "headline").unwrap();
        assert_eq!(copy.id, "headline-2");
        let [x, y, w, h] = added.cell;
        let c = copy.cell;
        assert!(
            c[0] >= x + w || c[0] + c[2] <= x || c[1] >= y + h || c[1] + c[3] <= y,
            "{c:?} clear of {:?}",
            added.cell
        );
        assert_eq!((c[2], c[3]), (w, h), "the same span");
        s.tool("deck_patch", serde_json::json!({ "ops": copy.patch }), by).unwrap();
        assert_eq!(stands(&mut s, "revenue", "headline-2"), Some(copy.cell));
        assert_eq!(s.deck.nodes["headline-2"].props.get("text"), s.deck.nodes["headline"].props.get("text"));

        // In a slot nothing fills, a text fills it: `close` shows nothing in `subtitle`.
        let slots = s.targets("close", "title").unwrap().slots.clone();
        let (_, sub) = slots.iter().find(|(name, _)| name == "subtitle").unwrap();
        let middle = [sub[0] + sub[2] / 2.0, sub[1] + sub[3] / 2.0];
        let lede = s.inserting("close", n("Text · lede"), middle).unwrap();
        assert_eq!(lede.cell, *sub);
        assert_eq!(lede.patch[1]["at"], serde_json::json!({ "in": "subtitle" }), "{:?}", lede.patch);
        // Where a slot is filled, it takes the grid's cells: `title` fills `close`'s title slot.
        let (_, title) = slots.iter().find(|(name, _)| name == "title").unwrap();
        let over =
            s.inserting("close", n("Text · lede"), [title[0] + title[2] / 2.0, title[1] + title[3] / 2.0]).unwrap();
        assert!(over.patch[1]["at"].get("in").is_none(), "{:?}", over.patch);

        // A shader preset fills the canvas, under what is there.
        let shader = s.inserting("revenue", n("Shader · texture"), [10.0, 10.0]).unwrap();
        assert_eq!(shader.cell, [0.0, 0.0, 1920.0, 1080.0]);
        assert_eq!(shader.patch[0]["node"]["z"], -1);
        s.tool("deck_patch", serde_json::json!({ "ops": shader.patch }), by).unwrap();
        let topmost = s.hit("revenue", [960.0, 540.0]).unwrap();
        assert_ne!(topmost.first().map(|h| h.node.as_str()), Some("texture"), "it draws under the rest");
    }

    /// Delete takes a node out of the state shown and the states that track it from there; one
    /// that no state shows then goes from the deck, so a node inserted and deleted leaves
    /// nothing behind. Shift+Delete takes it from the deck. A container goes with what it holds,
    /// what it holds first (PLAN 2.34).
    #[cfg(feature = "editor")]
    #[test]
    fn a_node_deleted_leaves_the_state_shown_and_one_inserted_leaves_nothing() {
        let mut s = revenue();
        let by = assistant::Caller { author: "user", at: None };
        let ops = |patch: &[serde_json::Value]| -> Vec<String> {
            patch
                .iter()
                .map(|op| format!("{} {}", op["op"].as_str().unwrap(), op.get("node").or(op.get("id")).unwrap()))
                .collect()
        };
        let shows = |s: &Session, node: &str| -> Vec<String> {
            let snaps = scaena_core::resolve_states(&s.deck).unwrap();
            snaps.iter().filter(|snap| snap.nodes.contains_key(node)).map(|snap| snap.state_id.clone()).collect()
        };

        // The title leaves `mix`; `close` shows it again, by its own delta.
        assert_eq!(shows(&s, "title"), ["intro", "revenue", "mix", "close"]);
        let delete = s.deleting("mix", "title", false).unwrap();
        assert_eq!(ops(&delete), [r#"hide_node "title""#]);
        s.tool("deck_patch", serde_json::json!({ "ops": delete }), by).unwrap();
        assert_eq!(shows(&s, "title"), ["intro", "revenue", "close"]);

        // A headline inserted in `revenue`, deleted there: no state shows it, so it goes.
        let n = s.inserts().iter().position(|i| i.label == "Text · headline").unwrap();
        let added = s.inserting("revenue", n, [960.0, 540.0]).unwrap();
        s.tool("deck_patch", serde_json::json!({ "ops": added.patch }), by).unwrap();
        let delete = s.deleting("revenue", "headline", false).unwrap();
        assert_eq!(ops(&delete), [r#"remove_node "headline""#]);
        s.tool("deck_patch", serde_json::json!({ "ops": delete }), by).unwrap();
        assert!(!s.deck.nodes.contains_key("headline"));

        // Shift+Delete: the chart goes from the deck.
        let delete = s.deleting("revenue", "rev", true).unwrap();
        assert_eq!(ops(&delete), [r#"remove_node "rev""#]);
        s.tool("deck_patch", serde_json::json!({ "ops": delete }), by).unwrap();
        assert!(!s.deck.nodes.contains_key("rev") && shows(&s, "rev").is_empty());
        assert!(s.deleting("revenue", "rev", false).is_err(), "nothing is there to delete");

        // A copy of a container holds copies of what it holds.
        let mut t = torture();
        let copy = t.duplicating("containers", "card").unwrap();
        assert!(t.typed(&serde_json::json!(copy.patch), None).unwrap(), "the deck takes {:?}", copy.patch);
        let snaps = scaena_core::resolve_states(&t.deck).unwrap();
        let snap = snaps.iter().find(|snap| snap.state_id == "containers").unwrap();
        let parent = |id: &str| {
            snap.nodes[id].get("at").and_then(|a| a.get("parent")).and_then(|p| p.as_str()).map(String::from)
        };
        let held_by = |p: &str| snap.nodes.keys().filter(|id| parent(id).as_deref() == Some(p)).count();
        assert!(held_by("card") > 0);
        assert_eq!(held_by(&copy.id), held_by("card"), "the copy holds what the card holds");

        // A container goes with what it holds, what it holds first.
        let s = torture();
        let card: Vec<String> = {
            let snaps = scaena_core::resolve_states(&s.deck).unwrap();
            let snap = snaps.iter().find(|snap| snap.state_id == "containers").unwrap();
            let parent = |id: &str| {
                snap.nodes[id].get("at").and_then(|a| a.get("parent")).and_then(|p| p.as_str()).map(String::from)
            };
            snap.nodes
                .keys()
                .filter(|id| parent(id).is_some_and(|p| p == "card" || parent(&p).as_deref() == Some("card")))
                .cloned()
                .collect()
        };
        assert!(!card.is_empty(), "the card holds nodes");
        for everywhere in [false, true] {
            let delete = s.deleting("containers", "card", everywhere).unwrap();
            let named: Vec<&str> =
                delete.iter().map(|op| op.get("node").or(op.get("id")).unwrap().as_str().unwrap()).collect();
            assert_eq!(named.last(), Some(&"card"), "{named:?}");
            assert!(card.iter().all(|id| named.contains(&id.as_str())), "{named:?} has {card:?}");
            let mut tried = torture();
            assert!(tried.typed(&serde_json::json!(delete), None).unwrap(), "the deck takes {delete:?}");
            assert!(shows(&tried, "card").iter().all(|state| state != "containers"));
        }
    }

    /// The state strip adds a step after the state shown, which shows what it shows, and a
    /// slide after the shown state's slide, empty, in its layout (PLAN 2.35).
    #[cfg(feature = "editor")]
    #[test]
    fn a_step_shows_what_the_state_before_it_shows_and_a_slide_starts_empty() {
        use scaena_ops::states::Adding;
        let mut s = revenue();
        let by = assistant::Caller { author: "user", at: None };
        let ids = |s: &Session| s.deck.states.iter().map(|st| st.id.clone()).collect::<Vec<_>>();
        let shows = |s: &Session, state: &str| {
            let snaps = scaena_core::resolve_states(&s.deck).unwrap();
            snaps.into_iter().find(|snap| snap.state_id == state).unwrap()
        };

        let step = s.adding_state("revenue", Adding::Step).unwrap();
        s.tool("deck_patch", serde_json::json!({ "ops": step.patch }), by).unwrap();
        assert_eq!(ids(&s), ["intro", "revenue", "revenue-2", "mix", "close"]);
        assert_eq!(shows(&s, "revenue-2").nodes, shows(&s, "revenue").nodes, "a step shows what it follows");
        assert_eq!(
            s.frame("revenue-2", f64::INFINITY).unwrap().digest().unwrap(),
            s.frame("revenue", f64::INFINITY).unwrap().digest().unwrap()
        );

        let slide = s.adding_state("revenue-2", Adding::Slide).unwrap();
        s.tool("deck_patch", serde_json::json!({ "ops": slide.patch }), by).unwrap();
        assert_eq!(ids(&s), ["intro", "revenue", "revenue-2", "mix", "slide", "close"], "after the slide's last step");
        let empty = shows(&s, "slide");
        assert!(empty.nodes.is_empty(), "{:?}", empty.nodes.keys());
        assert_eq!(empty.layout.as_deref(), Some("figure"));
        // `close` tracks from the empty slide now: its own props, as before, and its title.
        assert!(shows(&s, "close").nodes.contains_key("title"));
    }

    /// Every insert the torture deck's theme and bundle offer makes a patch the deck takes, one
    /// after another, in a state laid out by the theme's grid (PLAN 2.34): each node is valid
    /// as `add_node` adds it, and lands on the grid or fills its slot. Each is validated as
    /// `deck_patch` validates a patch, which refuses one only for what validation finds, but
    /// not linted: linting the whole torture deck for each would take minutes.
    #[cfg(feature = "editor")]
    #[test]
    fn every_insert_offered_is_a_patch_the_deck_takes() {
        let mut s = torture();
        let offered = s.inserts();
        assert!(offered.iter().any(|i| i.node["type"] == "image"), "the torture bundle's PNG is offered");
        for (n, insert) in offered.iter().enumerate() {
            let added = s.inserting("axes", n, [700.0, 400.0]).unwrap_or_else(|e| panic!("{}: {e}", insert.label));
            let made = s.typed(&serde_json::json!(added.patch), None);
            assert!(made.unwrap_or_else(|e| panic!("{}: {e}", insert.label)), "{}: the deck took it", insert.label);
            assert!(s.deck.nodes.contains_key(&added.id), "{}", insert.label);
        }
    }

    /// A state reads as a single-file export reads it (PLAN 2.5, 2.8): these are what `scaena
    /// export --format html` writes for the revenue example's states. In another format the
    /// same nodes read, in that format's paint order.
    #[test]
    fn each_state_reads_as_a_single_file_reads_it() {
        let mut s = revenue();
        let chart = r#"<div role="img" data-node="rev" aria-label="Quarterly revenue by product, Q4 2025 through Q3 2026."></div>"#;
        let note = r#"<p data-node="note">Revenue in $M. Enterprise recognized on delivery.</p>"#;
        let read = [
            (
                "intro",
                r#"<h1 data-node="title">Q3 Review</h1><h2 data-node="subtitle">This quarter changed the shape of the business.</h2>"#.to_string(),
            ),
            ("revenue", format!(r#"<h1 data-node="title">Revenue doubled</h1>{chart}{note}"#)),
            ("mix", format!(r#"<h1 data-node="title">…and the mix shifted</h1>{chart}{note}"#)),
            ("close", r#"<h1 data-node="title">Thank you</h1>"#.to_string()),
        ];
        for (state, expected) in &read {
            assert_eq!(&s.reading(state).unwrap(), expected, "{state}");
        }
        s.set_format(Some("9:16")).unwrap();
        let tall = s.reading("revenue").unwrap();
        for node in ["title", "rev", "note"] {
            assert!(tall.contains(&format!(r#"data-node="{node}""#)), "{node} reads in 9:16: {tall}");
        }
        assert!(matches!(s.reading("nowhere"), Err(Error::Engine(EngineError::UnknownState(_)))));
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
        assert_eq!(s.states().len(), 49);
    }
}
