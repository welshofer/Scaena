//! # scaena-session
//!
//! One bundle open for editing, in memory (ADR-0021): the deck, its theme, every file of the
//! bundle handed over, and the engine built from the fonts and images the deck names. A client
//! hands the files over by their paths in the bundle, then asks for frames and edits:
//!
//! - [`Session::frame`] gives a state's display list (SPEC §6), the same bytes natively and in
//!   the browser; [`Session::pixels`] paints it with `vello_cpu` (feature `cpu`), the painter
//!   the goldens hold. A frame can also be held while its shaders' rows are worked out in bands
//!   elsewhere ([`Session::shading`], [`shader_rows`]).
//! - With the `editor` feature, a session compiles `.scn` as it is typed, lints it with its
//!   engine, applies a finding's fix, and inspects a state (PLAN 2.3, [`editor`]). It says what
//!   stands where and where a node may go, makes each gesture's patch, opens a bundle from its
//!   files or a `.scaena` zip, and saves it as `scaena save` does (PLAN 2.4, `store`).
//!
//! The browser's module wraps it for JavaScript (`scaena-wasm`'s `Player`), and the Mac
//! client's C ABI wraps it for Swift (`scaena-ffi`, PLAN 3.1): each turns arguments and
//! results into JSON, and neither holds an editing rule of its own.

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

#[cfg(feature = "editor")]
pub mod assistant;
#[cfg(feature = "editor")]
mod data;
#[cfg(feature = "editor")]
pub mod editor;
#[cfg(feature = "editor")]
mod formats;
#[cfg(feature = "editor")]
pub mod store;
#[cfg(feature = "editor")]
mod theme;
#[cfg(feature = "editor")]
pub mod versions;

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
    targets: Vec<(String, String, scaena_engine::geometry::Targets)>,
    /// The node a drag moves, and how far, canvas units: frames at rest draw it there, from
    /// the state as laid out at rest, laying nothing out (ADR-0013).
    moving: Option<(Vec<String>, [f32; 2])>,
    /// The part of the canvas the editor's preview shows, `[x, y, w, h]` canvas units, when it
    /// is zoomed in (PLAN 2.46): its frames are painted through it, at the size shown.
    view: Option<[f32; 4]>,
    /// The deck a patch would make, shown before it is made: frames at rest draw it, laid out
    /// once a frame, as a resize does when it pauses (ADR-0013).
    #[cfg(feature = "editor")]
    previewing: Option<Deck>,
    /// The format frames are laid out in (SPEC §3.4); `None` for the deck's own canvas.
    format: Option<String>,
    /// What each format painted beside the canvas laid out, by the format (PLAN 2.62).
    #[cfg(feature = "editor")]
    besides: BTreeMap<Option<String>, formats::Beside>,
    /// The fonts and images the engine was built from, as painters read them.
    store: Assets,
    /// The frame held while its shaders' rows are worked out (PLAN 2.28).
    #[cfg(feature = "cpu")]
    shading: Option<Shading>,
    /// The CPU painter, kept from frame to frame with its render context.
    #[cfg(feature = "cpu")]
    painter: scaena_paint::cpu::CpuPainter,
    /// The theme's JSON, as handed over: what lint and a save read (PLAN 2.3–2.4).
    #[cfg(feature = "editor")]
    theme_json: String,
    /// The source the editor compiled last (PLAN 2.3).
    #[cfg(feature = "editor")]
    edit: Option<editor::Edit>,
    /// What the layout rules found in every state the last time they ran on all of them:
    /// kept for the states a lint of one state does not lay out.
    #[cfg(feature = "editor")]
    laid: editor::Laid,
    /// Fonts subset by the page's subsetter for a save, by path: the characters each keeps,
    /// and its bytes (PLAN 2.4).
    #[cfg(feature = "editor")]
    subsets: BTreeMap<String, (String, Vec<u8>)>,
    /// The layouts a state may take, being judged a step at a time (PLAN 2.92): let go with
    /// what was laid out.
    #[cfg(feature = "editor")]
    suggesting: Option<editor::Suggesting>,
    /// The pictures `layoutSuggestions` painted last, each until `layoutPixels` takes it
    /// (PLAN 2.92, 3.26).
    #[cfg(feature = "editor")]
    suggested: Vec<scaena_paint::Raster>,
    /// What the next save records in the bundle's history, if it keeps one, besides the save:
    /// each edit an operation made since the bundle was opened or saved, after the deck
    /// before it (PLAN 2.9).
    #[cfg(feature = "editor")]
    recorded: Vec<scaena_store::crdt::Recorded>,
    /// The data files edits wrote, each as it was before and after (PLAN 2.55): the Data panel
    /// undoes the last, and redoes the last undone.
    #[cfg(feature = "editor")]
    done: Vec<data::Written>,
    #[cfg(feature = "editor")]
    undone: Vec<data::Written>,
    /// Each data file an edit wrote, as the bundle held it when it was opened or last saved:
    /// what the next save records as the bundle's own, before the edits (PLAN 2.55).
    #[cfg(feature = "editor")]
    held: BTreeMap<String, Vec<u8>>,
    /// The files taken out of the bundle since it was opened or last saved (PLAN 2.59): what the
    /// next save takes out where the bundle is kept.
    #[cfg(feature = "editor")]
    removed: std::collections::BTreeSet<String>,
    /// Whether the next save begins a history, where the bundle keeps none (PLAN 2.87): as
    /// `scaena save --history` begins one.
    #[cfg(feature = "editor")]
    begins: bool,
    /// A version from the bundle's history, shown read-only (PLAN 2.60): a session of its own.
    #[cfg(feature = "editor")]
    viewing: Option<Box<Session>>,
}

/// What grouping makes (PLAN 2.43): the new group's id, and the patch that makes it.
#[cfg(feature = "editor")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct Grouping {
    pub id: String,
    pub patch: Vec<serde_json::Value>,
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
    /// The deck as the session holds it: as opened, or as compiled or patched last.
    pub fn deck(&self) -> &Deck {
        &self.deck
    }

    /// The theme frames are drawn in.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

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
            targets: Vec::new(),
            moving: None,
            view: None,
            #[cfg(feature = "editor")]
            previewing: None,
            format: None,
            #[cfg(feature = "editor")]
            besides: BTreeMap::new(),
            store: Assets::new(),
            #[cfg(feature = "cpu")]
            shading: None,
            #[cfg(feature = "cpu")]
            painter: scaena_paint::cpu::CpuPainter::default(),
            #[cfg(feature = "editor")]
            theme_json: theme_json.to_string(),
            #[cfg(feature = "editor")]
            edit: None,
            #[cfg(feature = "editor")]
            laid: editor::Laid::default(),
            #[cfg(feature = "editor")]
            subsets: BTreeMap::new(),
            #[cfg(feature = "editor")]
            suggesting: None,
            #[cfg(feature = "editor")]
            suggested: Vec::new(),
            #[cfg(feature = "editor")]
            recorded: Vec::new(),
            #[cfg(feature = "editor")]
            done: Vec::new(),
            #[cfg(feature = "editor")]
            undone: Vec::new(),
            #[cfg(feature = "editor")]
            held: BTreeMap::new(),
            #[cfg(feature = "editor")]
            removed: Default::default(),
            #[cfg(feature = "editor")]
            begins: false,
            #[cfg(feature = "editor")]
            viewing: None,
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
        if deck.theme != self.deck.theme {
            self.follow_theme(&deck);
        }
        self.deck = deck;
        self.forget();
    }

    /// Draw in the theme `deck` names (PLAN 2.39): a theme file the bundle holds, or one set
    /// inline, as a re-theme or an edit of the deck's source names it. One that does not read
    /// as a theme leaves the theme shown, as validation will have refused the deck.
    fn follow_theme(&mut self, deck: &Deck) {
        let text = match &deck.theme {
            Some(serde_json::Value::String(path)) => {
                self.files.get(path).and_then(|bytes| String::from_utf8(bytes.clone()).ok())
            }
            Some(inline @ serde_json::Value::Object(_)) => Some(inline.to_string()),
            _ => None,
        };
        let Some(theme) = text.as_deref().and_then(|text| Theme::from_json(text).ok()) else { return };
        self.theme = theme;
        // The engine checks the theme against the fonts it is built from.
        self.engine = None;
        #[cfg(feature = "editor")]
        if let Some(text) = text {
            self.theme_json = text;
        }
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
        // The theme the deck names, edited (ADR-0016): frames are drawn in it from now on.
        if self.deck.theme.as_ref().and_then(|t| t.as_str()) == Some(path) {
            self.follow_theme(&self.deck.clone());
            self.forget();
        }
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

    /// Let go of what was laid out: the deck or its files changed. A drag, or a patch
    /// previewed, ends with it.
    fn forget(&mut self) {
        self.forget_shown();
        #[cfg(feature = "editor")]
        self.besides.clear();
    }

    /// Let go of what the canvas laid out: the format it shows changed, or the deck did. What
    /// each format beside it laid out stands (PLAN 2.62).
    fn forget_shown(&mut self) {
        self.transition = None;
        self.rest = None;
        self.targets.clear();
        self.moving = None;
        #[cfg(feature = "editor")]
        {
            self.previewing = None;
            self.suggesting = None;
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
            self.forget_shown();
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

    /// The deck's states end to end, as a client lists them: `[{ "state", "slide", "start",
    /// "span", "hold" }]`, ms (SPEC §2.4), `slide` the slide each builds on. What a player
    /// auto-advances by, and a video samples.
    pub fn slots(&mut self) -> Result<serde_json::Value, Error> {
        let timeline = self.timeline()?;
        let deck = &self.deck;
        let slide = |id: &str| deck.states.iter().find(|s| s.id == id).map_or(id, |s| deck.slide_of(s)).to_string();
        let slots: Vec<serde_json::Value> = (timeline.slots.iter())
            .map(|s| {
                let slide = slide(&s.state);
                serde_json::json!({ "state": s.state, "slide": slide, "start": s.start, "span": s.span, "hold": s.hold })
            })
            .collect();
        Ok(serde_json::Value::Array(slots))
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
                Some((nodes, by)) => scene.moved(time, &nodes.iter().map(String::as_str).collect::<Vec<_>>(), by),
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

    /// [`Session::boxes`] as a client lists them (ADR-0013): `[{ "node", "rect": [x, y, w, h],
    /// "parent"?, "draws", "transform"?, "locked"? }]`, canvas units. `transform` is where its
    /// own and its containers' draw it from `rect` (SPEC §3.3), `[a, b, c, d, e, f]`, where
    /// something moves it; `locked`, where the node is locked (PLAN 2.95), the node whose lock
    /// holds it: itself, or what holds it.
    pub fn boxes_json(&mut self, state: &str) -> Result<Vec<serde_json::Value>, Error> {
        let boxes = self.boxes(state)?;
        // Locked, or held by what is, however deep (PLAN 2.95).
        let parents: std::collections::HashMap<&str, &str> =
            boxes.iter().filter_map(|b| Some((b.node.as_str(), b.parent.as_deref()?))).collect();
        let held = |node: &str| {
            let mut at = Some(node);
            let mut seen = 0;
            while let Some(n) = at.filter(|_| seen <= parents.len()) {
                if self.locked(n) {
                    return Some(n.to_string());
                }
                (at, seen) = (parents.get(n).copied(), seen + 1);
            }
            None
        };
        let locked: Vec<Option<String>> = boxes.iter().map(|b| held(&b.node)).collect();
        Ok((boxes.into_iter().zip(locked))
            .map(|(b, locked)| {
                let mut out =
                    serde_json::json!({ "node": b.node, "rect": b.rect, "parent": b.parent, "draws": b.draws });
                if let Some(map) = b.transform {
                    out["transform"] = serde_json::json!(map);
                }
                if let Some(by) = locked {
                    out["locked"] = serde_json::json!(by);
                }
                out
            })
            .collect())
    }

    /// [`Session::hit`] as a client lists them (ADR-0013): `[{ "node", "rect", "containers",
    /// "transform"?, "locked"? }]`, each node's containers innermost first, read through its
    /// transform as a box's is; `locked` where the node is locked (PLAN 2.95), which a pointer
    /// passes over: the node whose lock holds it, itself or the innermost container locked.
    pub fn hits_json(&mut self, state: &str, point: [f32; 2]) -> Result<Vec<serde_json::Value>, Error> {
        Ok((self.hit(state, point)?.into_iter())
            .map(|h| {
                let locked = std::iter::once(&h.node).chain(&h.containers).find(|n| self.locked(n)).cloned();
                let mut out = serde_json::json!({ "node": h.node, "rect": h.rect, "containers": h.containers });
                if let Some(map) = h.transform {
                    out["transform"] = serde_json::json!(map);
                }
                if let Some(by) = locked {
                    out["locked"] = serde_json::json!(by);
                }
                out
            })
            .collect())
    }

    /// Whether `node` is locked (PLAN 2.95): its own `locked`, which holds in every state. What a
    /// locked container or group holds is locked with it, as the canvas reads it.
    pub fn locked(&self, node: &str) -> bool {
        self.deck.nodes.get(node).and_then(|n| n.props.get("locked")).and_then(serde_json::Value::as_bool) == Some(true)
    }

    /// The chart mark or table row drawn at `point` (canvas units) in `state` at rest, in the
    /// format shown, with the rows of its source it was made from (PLAN 2.64). `None` where the
    /// topmost node there is no chart or table, or the point falls between its marks.
    pub fn mark_at(&mut self, state: &str, point: [f32; 2]) -> Result<Option<scaena_engine::marks::DataMark>, Error> {
        Ok(self.at_rest(state)?.mark_at(point))
    }

    /// What `rows` of data source `source` draw in `state` at rest, in the format shown: each
    /// chart mark and table row made from any of them, in paint order (PLAN 2.64).
    pub fn marks_of(
        &mut self,
        state: &str,
        source: &str,
        rows: &[usize],
    ) -> Result<Vec<scaena_engine::marks::DataMark>, Error> {
        Ok(self.at_rest(state)?.marks_of(source, rows))
    }

    /// The chart annotation drawn at `point` (canvas units) in `state` at rest, in the format
    /// shown (PLAN 2.67): its chart and its place among the chart's `annotations`. `None`
    /// where the topmost node there is no chart, or the point is on none of its annotations.
    pub fn note_at(&mut self, state: &str, point: [f32; 2]) -> Result<Option<scaena_engine::marks::NoteMark>, Error> {
        Ok(self.at_rest(state)?.note_at(point))
    }

    /// Chart `node`'s marks and annotations in `state` at rest, in the format shown (PLAN 2.75):
    /// what the keys step through. `None` for a node the state does not draw, or one that is no
    /// chart.
    #[allow(clippy::type_complexity)]
    pub fn marks_in(
        &mut self,
        state: &str,
        node: &str,
    ) -> Result<Option<(Vec<scaena_engine::marks::DataMark>, Vec<scaena_engine::marks::NoteMark>)>, Error> {
        Ok(self.at_rest(state)?.marks_in(node))
    }

    /// Where a callout of chart `node` dropped at `point` (canvas units) in `state` at rest
    /// would stand, in the format shown (PLAN 2.67): on the mark there, or at the category or
    /// the x nearest across and the value there. `None` for a node that is no chart, or a
    /// donut.
    pub fn callout_at(
        &mut self,
        state: &str,
        node: &str,
        point: [f32; 2],
    ) -> Result<Option<scaena_core::model::values::AnnotationAt>, Error> {
        Ok(self.at_rest(state)?.callout_at(node, point))
    }

    /// The layout `state` uses and its slots in the format shown (PLAN 2.71): what the canvas
    /// draws to edit it. None for a state that names no layout.
    pub fn layout(&mut self, state: &str) -> Result<Option<(String, Vec<scaena_engine::guides::SlotBox>)>, Error> {
        Ok(scaena_engine::guides::layout(&self.deck, &self.theme, state, self.format.as_deref())?)
    }

    /// Shape `node`'s outline in `state` at rest, in the format shown (PLAN 2.68): its points,
    /// a rect's corner radius, and the theme's radius steps. `None` for a node the state does
    /// not draw, or one that is no shape.
    pub fn outline(&mut self, state: &str, node: &str) -> Result<Option<scaena_engine::geometry::Outline>, Error> {
        self.at_rest(state)?;
        let scene = &self.rest.as_ref().expect("laid out above").1;
        Ok(scene.outline(node, &self.theme))
    }

    /// Image `node`'s framing in `state` at rest, in the format shown (PLAN 2.74): where its whole
    /// is drawn, the part that shows, its crop, and its focal point. `None` for a node the state
    /// does not draw, or one that is no image.
    pub fn framing(&mut self, state: &str, node: &str) -> Result<Option<scaena_engine::geometry::Framing>, Error> {
        Ok(self.at_rest(state)?.framing(node))
    }

    /// The point of image `node` drawn under `point` in `state` at rest, in fractions of the
    /// part its crop keeps: what a focal point picked there is (PLAN 2.45). `None` off the image,
    /// or for a node that is no image.
    pub fn focal_at(&mut self, state: &str, node: &str, point: [f32; 2]) -> Result<Option<[f32; 2]>, Error> {
        Ok(self.at_rest(state)?.image_point(node, point))
    }

    /// Where `node` may go in `state` at rest, in the format shown (ADR-0013): what holds
    /// it, its cell, and the tracks, slots, or order a drag snaps it to.
    pub fn targets(&mut self, state: &str, node: &str) -> Result<&scaena_engine::geometry::Targets, Error> {
        // Kept for the nodes a gesture moves, until the deck, its files, or the format change.
        let kept = self.targets.iter().position(|(s, n, _)| s == state && n == node);
        if kept.is_none() {
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
            if self.targets.len() >= 64 {
                self.targets.remove(0);
            }
            self.targets.push((state.to_string(), node.to_string(), found));
        }
        let at = kept.unwrap_or(self.targets.len() - 1);
        Ok(&self.targets[at].2)
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

    /// The theme's grid in the format shown, as the editor's guides draw it (PLAN 2.57).
    #[cfg(feature = "editor")]
    pub fn grid(&self) -> Result<scaena_engine::guides::Guides, Error> {
        Ok(scaena_engine::guides::grid(&self.deck, &self.theme, self.format.as_deref())?)
    }

    /// What a box `moving` move in `state` at rest, in the format shown, may meet (PLAN 2.57):
    /// what else draws, and the canvas, as [`scaena_engine::guides::around`] says. Nothing where
    /// one of them is drawn elsewhere than its placement puts it, by its transform or what holds
    /// it: a drag of it shows no guides.
    #[cfg(feature = "editor")]
    fn around(&mut self, state: &str, moving: &[&str]) -> Result<Vec<[f32; 4]>, Error> {
        let scene = self.at_rest(state)?;
        let boxes = scene.boxes();
        let plain = moving.iter().all(|n| boxes.iter().any(|b| b.node == *n && b.transform.is_none()));
        Ok(match plain {
            true => scaena_engine::guides::around(&boxes, scene.canvas, moving),
            false => Vec::new(),
        })
    }

    /// Where the box `to` lands, as [`Session::snap`] says, and the guides it meets there (PLAN
    /// 2.57): a line wherever one of its edges, or its middle, meets another box's or the
    /// canvas's. Off the grid (`free`), it goes first the least way that brings one onto another
    /// within `reach` canvas units. Into a slot or among a stack's children, it meets none.
    #[cfg(feature = "editor")]
    #[allow(clippy::type_complexity)]
    pub fn guided(
        &mut self,
        state: &str,
        node: &str,
        how: scaena_ops::inspect::SnapMode,
        to: [f32; 4],
        fork: bool,
        reach: f32,
    ) -> Result<Option<(scaena_ops::inspect::Snapped, Vec<[f32; 4]>)>, Error> {
        use scaena_engine::guides;
        use scaena_ops::inspect::SnapMode;
        let cell = self.targets(state, node)?.cell;
        let around = self.around(state, &[node])?;
        let to = match how {
            SnapMode::Free => guides::align(to, cell, &around, reach),
            _ => to,
        };
        let Some(snapped) = self.snap(state, node, how, to, fork)? else { return Ok(None) };
        let lines = match how {
            SnapMode::Move | SnapMode::Resize | SnapMode::Free => guides::meets(snapped.cell, &around),
            SnapMode::Slot | SnapMode::Order => Vec::new(),
        };
        Ok(Some((snapped, lines)))
    }

    /// Where the box `to` lands as a presentation app's drag puts it (PLAN 3.19, ADR-0024): where
    /// it was let go, after it goes the least way, within `reach` canvas units, that brings an edge
    /// of it, or its middle, onto another box's, the canvas's, or a track of the theme's grid; on
    /// the grid's tracks all round, by those cells, as a drag on the grid places it, else by a
    /// `rect`. With the guides it meets there, the grid's tracks among them. None where nothing
    /// places the node so: a stack's child, or a grid container's.
    #[cfg(feature = "editor")]
    #[allow(clippy::type_complexity)]
    pub fn guided_freely(
        &mut self,
        state: &str,
        node: &str,
        to: [f32; 4],
        fork: bool,
        reach: f32,
    ) -> Result<Option<(scaena_ops::inspect::Snapped, Vec<[f32; 4]>)>, Error> {
        use scaena_engine::guides;
        use scaena_ops::inspect::SnapMode;
        let found = self.targets(state, node)?.clone();
        let mut magnets = self.around(state, &[node])?;
        // The theme's grid draws it in too, each column down the canvas and each row across it;
        // nothing does where it is drawn turned, as `around` says.
        if !magnets.is_empty() {
            let canvas = self.at_rest(state)?.canvas;
            magnets.extend(found.columns.iter().map(|c| [c[0], 0.0, c[1] - c[0], canvas[1]]));
            magnets.extend(found.rows.iter().map(|r| [0.0, r[0], canvas[0], r[1] - r[0]]));
        }
        let to = guides::align(to, found.cell, &magnets, reach);
        let on = |tracks: &[[f32; 2]], start: f32, end: f32| {
            tracks.iter().any(|t| (t[0] - start).abs() <= 0.5) && tracks.iter().any(|t| (t[1] - end).abs() <= 0.5)
        };
        let cells = on(&found.columns, to[0], to[0] + to[2]) && on(&found.rows, to[1], to[1] + to[3]);
        let how = if cells { SnapMode::Resize } else { SnapMode::Free };
        let Some(snapped) = self.snap(state, node, how, to, fork)? else { return Ok(None) };
        let lines = guides::meets(snapped.cell, &magnets);
        Ok(Some((snapped, lines)))
    }

    /// `nodes`, children of one container, moved together `by` (PLAN 2.42) as
    /// [`Session::arranging`] moves them, and the guides the box around them meets where they
    /// land (PLAN 2.57). Off the grid (`free`), that box goes first the least way that brings an
    /// edge, or its middle, onto another's within `reach` canvas units.
    #[cfg(feature = "editor")]
    #[allow(clippy::type_complexity, clippy::too_many_arguments)]
    pub fn together(
        &mut self,
        state: &str,
        nodes: &[String],
        by: [f32; 2],
        free: bool,
        fork: bool,
        reach: f32,
    ) -> Result<Option<(scaena_ops::arrange::Arranged, Vec<[f32; 4]>)>, Error> {
        use scaena_engine::guides;
        let cells = nodes.iter().map(|n| self.targets(state, n).map(|t| t.cell)).collect::<Result<Vec<_>, _>>()?;
        let moving: Vec<&str> = nodes.iter().map(String::as_str).collect();
        let around = self.around(state, &moving)?;
        let by = match (free, guides::union(&cells)) {
            (true, Some(from)) => {
                let to = guides::align([from[0] + by[0], from[1] + by[1], from[2], from[3]], from, &around, reach);
                [to[0] - from[0], to[1] - from[1]]
            }
            _ => by,
        };
        let how = scaena_ops::arrange::How::Together { by, free };
        let Some(arranged) = self.arranging(state, nodes, how, fork)? else { return Ok(None) };
        let landed: Vec<[f32; 4]> = arranged.landed.iter().map(|l| l.cell).collect();
        let lines = guides::union(&landed).map_or_else(Vec::new, |cell| guides::meets(cell, &around));
        Ok(Some((arranged, lines)))
    }

    /// Draw nodes, and what they hold, `by` canvas units from where they stand in the frames at
    /// rest that follow, from the state as laid out at rest: what a drag shows as it moves,
    /// laying nothing out (ADR-0013). `None` puts them back.
    pub fn set_moving(&mut self, moving: Option<(Vec<String>, [f32; 2])>) {
        self.moving = moving;
    }

    /// Paint the preview's frames through `view`, `[x, y, w, h]` canvas units: the part of
    /// the canvas a zoomed editor shows, painted at the size shown (PLAN 2.46). `None` shows
    /// the whole canvas. A view that is not a part of some size is refused.
    pub fn set_view(&mut self, view: Option<[f32; 4]>) -> Result<(), Error> {
        if let Some([x, y, w, h]) = view
            && !([x, y, w, h].iter().all(|v| v.is_finite()) && w > 0.0 && h > 0.0)
        {
            return Err(Error::Deck(format!("a view is a part of the canvas with a size: [{x}, {y}, {w}, {h}]")));
        }
        self.view = view;
        Ok(())
    }

    /// `state` at `t_ms`, as the preview shows it: through the view, if it is zoomed in.
    pub fn viewed(&mut self, state: &str, t_ms: f64) -> Result<DisplayList, Error> {
        let dl = self.frame(state, t_ms)?;
        Ok(match self.view {
            Some(view) => dl.viewed(view),
            None => dl,
        })
    }

    /// `nodes`, children of one container, arranged `how` in `state` at rest, in the format
    /// shown (PLAN 2.42): where each lands and the patch that puts them there, made in `state`
    /// or, to `fork` it, kept there; `None` where nothing moves them that way.
    #[cfg(feature = "editor")]
    pub fn arranging(
        &mut self,
        state: &str,
        nodes: &[String],
        how: scaena_ops::arrange::How,
        fork: bool,
    ) -> Result<Option<scaena_ops::arrange::Arranged>, Error> {
        let found = nodes.iter().map(|n| self.targets(state, n).cloned()).collect::<Result<Vec<_>, _>>()?;
        let boxes = self.boxes(state)?;
        let (deck, _) = project(&self.deck, &self.theme, self.format.as_deref())?;
        let deck = deck.into_owned();
        let snaps = scaena_core::resolve_states(&deck).map_err(|e| Error::Deck(e.to_string()))?;
        let snap =
            snaps.iter().find(|s| s.state_id == state).ok_or_else(|| Error::Ops(format!("no state `{state}`")))?;
        let shown = scaena_engine::cascade::with_overrides(&deck, snap);
        // Where a node moved in may go in another container, asked of the engine only then.
        let mut into = |node: &str, holder: Option<&str>| {
            self.targets_into(state, node, holder).map_err(|e| scaena_ops::OpsError::new(e.to_string()))
        };
        scaena_ops::arrange::arrange(&deck, &shown, &boxes, nodes, found, how, fork, &mut into)
            .map_err(|e| Error::Ops(e.message))
    }

    /// Where `node` may go in `state` at rest, in the format shown, `into` another container,
    /// or onto the canvas for `None` (PLAN 2.50).
    #[cfg(feature = "editor")]
    pub fn targets_into(
        &mut self,
        state: &str,
        node: &str,
        into: Option<&str>,
    ) -> Result<scaena_engine::geometry::Targets, Error> {
        self.duration(state)?;
        let engine = self.engine.as_mut().expect("built for the span");
        let format = self.format.as_deref();
        let req =
            FrameRequest { deck: &self.deck, theme: &self.theme, data: &self.data, state, t_ms: f64::INFINITY, format };
        Ok(engine.targets_into(&req, node, into)?)
    }

    /// Each text of the deck that `query` matches, once for each place it is written, with
    /// the states that show it from there (PLAN 2.47).
    #[cfg(feature = "editor")]
    pub fn find(&self, query: &scaena_core::patch::Query) -> Result<Vec<scaena_core::patch::Found>, Error> {
        let doc = self.deck.to_value().map_err(|e| Error::Deck(e.to_string()))?;
        scaena_core::patch::find(&doc, query).map_err(Error::Ops)
    }

    /// The patch that replaces what `query` matches with `with` (PLAN 2.47): every match, a
    /// `replace_text` where each text lives, or, with `one`, `[text, match]` into what
    /// [`Session::find`] gives, that match alone.
    #[cfg(feature = "editor")]
    pub fn replacing(
        &self,
        query: &scaena_core::patch::Query,
        with: &str,
        one: Option<[usize; 2]>,
    ) -> Result<Vec<serde_json::Value>, Error> {
        let found = self.find(query)?;
        let found = match one {
            None => found,
            Some([i, k]) => {
                let mut text = found.get(i).filter(|f| k < f.matches.len()).cloned().ok_or_else(|| {
                    Error::Ops(format!(
                        "there is no match {k} of text {i}: `{}` is in {} texts",
                        query.find,
                        found.len()
                    ))
                })?;
                text.matches = vec![text.matches[k]];
                vec![text]
            }
        };
        Ok(scaena_core::patch::replacing(&found, with))
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

    /// [`Session::carets`] as a page and the Mac take it (PLAN 2.32, 3.6): `{ "text", "lines":
    /// [{ "top", "bottom", "x", "start", "end", "broken", "chars": [[offset, lead, trail]] }],
    /// "items": [{ "kind", "level", "marker" } | null] }`, canvas units, its offsets in UTF-16
    /// code units, as JavaScript and Swift count a string; `items` each paragraph as a list's
    /// item (ADR-0018). Null for a node that is no text there.
    pub fn carets_json(&mut self, state: &str, node: &str) -> Result<serde_json::Value, Error> {
        let Some(c) = self.carets(state, node)? else { return Ok(serde_json::Value::Null) };
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
        Ok(serde_json::json!({ "text": c.text, "lines": lines, "items": c.items }))
    }

    /// What an inspector offers for `node` as `state` shows it (ADR-0013, PLAN 2.33): each
    /// property it edits, with the theme's names for it or what the schema allows, the value
    /// shown, and where that value lives, which is where a `choose` patch writes. A chart's or a
    /// table's fields are the columns of the data handed over (PLAN 2.41).
    pub fn choices(&self, state: &str, node: &str) -> Result<scaena_core::choices::Choices, Error> {
        scaena_core::choices::choices(&self.deck, &self.theme, state, node, &*self.files).map_err(Error::Ops)
    }

    /// `node`'s look as `state` shows it (PLAN 2.58): each property of its type's look and the
    /// value shown, which ⌥⌘C picks up.
    pub fn look(&self, state: &str, node: &str) -> Result<scaena_core::looks::Look, Error> {
        scaena_core::looks::look(&self.deck, &self.theme, state, node, &*self.files).map_err(Error::Ops)
    }

    /// `look` put on `nodes` in `state` (PLAN 2.58): the patch of `choose`s that ⌥⌘V makes, each
    /// written where that node's own value lives, and the nodes that look so already or take
    /// none of it, with why.
    pub fn putting(
        &self,
        state: &str,
        look: &scaena_core::looks::Look,
        nodes: &[String],
    ) -> Result<scaena_core::looks::Put, Error> {
        scaena_core::looks::putting(&self.deck, &self.theme, state, look, nodes, &*self.files).map_err(Error::Ops)
    }

    /// What an inspector offers for the characters `from` to `to` (Unicode scalar values) of
    /// `node`'s text as `state` shows it (PLAN 2.38): each look a run of its own takes, with
    /// the first character's, which a `style_text` patch sets.
    pub fn character_choices(
        &self,
        state: &str,
        node: &str,
        from: usize,
        to: usize,
    ) -> Result<scaena_core::choices::Choices, Error> {
        scaena_core::choices::characters(&self.deck, &self.theme, state, node, (from, to)).map_err(Error::Ops)
    }

    /// The deck in another theme (PLAN 2.39), as `theme --apply` re-themes a bundle: the theme
    /// file at `path` in the bundle, or `text` written there first, a theme that ships, which
    /// the page carries with its fonts (`fonts`, by the paths its families give them). A path
    /// the bundle holds with other bytes takes the next name free. A theme that would leave
    /// the deck invalid is refused, and the deck keeps its own; else the deck names it, by
    /// `user` at `at` (seconds since the epoch), and frames are drawn in it from now on.
    #[cfg(feature = "editor")]
    pub fn retheme(
        &mut self,
        path: &str,
        text: Option<&str>,
        fonts: BTreeMap<String, Vec<u8>>,
        at: Option<i64>,
    ) -> Result<scaena_ops::theme::Themed, Error> {
        let (rel, text) = match text {
            Some(text) => (self.free_path(path, text.as_bytes()), text.to_string()),
            None => {
                let bytes = self.files.get(path).ok_or_else(|| Error::Ops(format!("the bundle holds no `{path}`")))?;
                let text = String::from_utf8(bytes.clone()).map_err(|_| Error::Ops(format!("`{path}` is not text")))?;
                (path.to_string(), text)
            }
        };
        if self.deck.theme.as_ref().and_then(|t| t.as_str()) == Some(&rel) {
            return Err(Error::Ops(format!("the deck is in `{rel}` already")));
        }
        let copy = self.files.get(&rel).is_none_or(|bytes| bytes != text.as_bytes());
        let (themed, write) = scaena_ops::theme::theming(&self.bundle(), &rel, &text, &fonts, copy, false)?;
        if !themed.refused {
            self.write(Some(write), assistant::Caller { author: "user", at })?;
        }
        Ok(themed)
    }

    /// `path`, or where the bundle holds other bytes there, the first of `name-2.theme.json`,
    /// `name-3.theme.json`, … it holds nothing at, or these bytes.
    #[cfg(feature = "editor")]
    fn free_path(&self, path: &str, bytes: &[u8]) -> String {
        let fits = |p: &str| self.files.get(p).is_none_or(|held| held == bytes);
        if fits(path) {
            return path.to_string();
        }
        let (stem, ext) = path.strip_suffix(".theme.json").map_or((path, ""), |stem| (stem, ".theme.json"));
        (2..).map(|n| format!("{stem}-{n}{ext}")).find(|p| fits(p)).expect("a name is free")
    }

    /// The theme the deck names, a path in the bundle (`(inline)` for one set in the deck),
    /// and the theme files the bundle holds (PLAN 2.39): what the editor offers beside the
    /// themes that ship.
    pub fn themes(&self) -> (Option<String>, Vec<String>) {
        let current = match &self.deck.theme {
            Some(serde_json::Value::String(path)) => Some(path.clone()),
            Some(_) => Some("(inline)".to_string()),
            None => None,
        };
        let files = (self.files.keys())
            .filter(|p| p.ends_with(".theme.json") || current.as_deref() == Some(p.as_str()))
            .cloned()
            .collect();
        (current, files)
    }

    /// The theme frames are drawn in, as `{ theme, text }` (PLAN 2.61): the theme file the deck
    /// names, by its path, or `(inline)` for one written in the deck, and its JSON as text; none
    /// where the deck names no theme.
    #[cfg(feature = "editor")]
    pub fn theme_text(&self) -> Option<serde_json::Value> {
        let (current, _) = self.themes();
        Some(serde_json::json!({ "theme": current?, "text": self.theme_json }))
    }

    /// What ⌘B gives the characters `from` to `to` (Unicode scalar values) of `node`'s text
    /// in `state` at rest, in the format shown (PLAN 2.38): `style_text`'s `look`, from the
    /// weight the engine sets each of them in.
    pub fn bolding(&mut self, state: &str, node: &str, from: usize, to: usize) -> Result<serde_json::Value, Error> {
        let carets =
            self.carets(state, node)?.ok_or_else(|| Error::Ops(format!("`{node}` is no text in `{state}`")))?;
        let byte = |chars: usize| carets.text.char_indices().nth(chars).map_or(carets.text.len(), |(i, _)| i);
        Ok(carets.bolding(byte(from), byte(to)))
    }

    /// What ⌘I gives the characters `from` to `to` (Unicode scalar values) of `node`'s text
    /// in `state` at rest, in the format shown (PLAN 2.40): `style_text`'s `look`, from
    /// whether each of them asks for italic.
    pub fn italicizing(&mut self, state: &str, node: &str, from: usize, to: usize) -> Result<serde_json::Value, Error> {
        let carets =
            self.carets(state, node)?.ok_or_else(|| Error::Ops(format!("`{node}` is no text in `{state}`")))?;
        let byte = |chars: usize| carets.text.char_indices().nth(chars).map_or(carets.text.len(), |(i, _)| i);
        Ok(carets.italicizing(byte(from), byte(to)))
    }

    /// What an inspector offers for `state` itself (PLAN 2.36): its layout, each key of its
    /// transition, its hold, and its notes, each with its value and where it lives, which is
    /// where a `set_state` patch writes.
    pub fn state_choices(&self, state: &str) -> Result<scaena_core::choices::StateChoices, Error> {
        scaena_core::choices::state_choices(&self.deck, &self.theme, state).map_err(Error::Ops)
    }

    /// `state`'s layers (PLAN 2.50): its nodes nested as their containers and groups hold them,
    /// topmost first, with those that leave in it and those another state of its slide shows,
    /// hidden, as `scaena inspect --layers` says them.
    pub fn layers(&self, state: &str) -> Result<Vec<scaena_core::layers::Layer>, Error> {
        let snapshots = scaena_core::resolve_states(&self.deck).map_err(|e| Error::Deck(e.to_string()))?;
        let at = (snapshots.iter().position(|s| s.state_id == state))
            .ok_or_else(|| EngineError::UnknownState(state.to_string()))?;
        Ok(scaena_core::layers::layers(&self.deck, &snapshots, at))
    }

    /// What may be inserted in the deck (PLAN 2.34): a text in each of the theme's roles, each
    /// kind of shape, each image in the bundle, a chart and a table of each data source (PLAN
    /// 2.41), and each shader preset, each as `add_node` adds it, with the box it takes at first.
    pub fn inserts(&self) -> Vec<scaena_core::inserts::Insert> {
        let paths: Vec<String> = self.files.keys().cloned().collect();
        scaena_core::inserts::inserts(&self.deck, &self.theme, &paths, &*self.files)
    }

    /// The patch that inserts what [`Session::inserts`] offers `n`th in `state`, the box it
    /// starts as about `at` (canvas units, in the format shown), snapped to the theme's grid
    /// as a drop snaps; or into the slot it fills (PLAN 2.34). Content that would overlap what
    /// draws there goes to the room on the grid nearest `at` (PLAN 2.79). Its id is new to the
    /// deck, made from `named`, a dropped file's name, where there is one: `Trailhead.jpg` is
    /// `trailhead`.
    #[cfg(feature = "editor")]
    pub fn inserting(
        &mut self,
        state: &str,
        n: usize,
        at: [f32; 2],
        named: Option<&str>,
    ) -> Result<scaena_ops::inspect::Added, Error> {
        self.inserting_with(state, n, at, named, None)
    }

    /// [`Session::inserting`], with `with`, properties of the node's own, set on what is
    /// offered: a pasted sheet's table its columns, each printed as it was copied (PLAN 2.96).
    #[cfg(feature = "editor")]
    pub fn inserting_with(
        &mut self,
        state: &str,
        n: usize,
        at: [f32; 2],
        named: Option<&str>,
        with: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Result<scaena_ops::inspect::Added, Error> {
        let (mut insert, room) = self.room(state, n, named)?;
        if let (Some(with), Some(node)) = (with, insert.node.as_object_mut()) {
            node.extend(with.clone());
        }
        // What lint E101 would judge it against: a decoration lies anywhere.
        let snaps = scaena_core::resolve_states(&self.deck).map_err(|e| Error::Deck(e.to_string()))?;
        let decorations: Vec<String> = (snaps.iter().find(|s| s.state_id == state).into_iter())
            .flat_map(|s| &s.nodes)
            .filter(|(_, props)| props.get("semantic").and_then(serde_json::Value::as_str) == Some("decoration"))
            .map(|(id, _)| id.clone())
            .collect();
        let crowded = self.at_rest(state)?.crowded(|id| decorations.iter().any(|d| d == id));
        let added = scaena_ops::inspect::inserting(&self.deck, &room, &insert, state, at, &crowded)
            .map_err(|e| Error::Deck(e.to_string()))?;
        // A table is as wide as its columns: where the cell offered is too narrow for them in a
        // format the deck lists, it takes the grid's width (PLAN 2.96).
        if insert.node["type"] != "table" || self.lays_out_with(&added.patch, state)? {
            return Ok(added);
        }
        let (_, room) = self.room_at(state, n, named, Some(1.0))?;
        scaena_ops::inspect::inserting(&self.deck, &room, &insert, state, at, &crowded)
            .map_err(|e| Error::Deck(e.to_string()))
    }

    /// The patch that draws what [`Session::inserts`] offers `n`th in `state`, in the box a drag
    /// from one point to another covers (canvas units, in the format shown): each edge snapped
    /// to the theme's grid as a resize snaps, or, `free`, where it was drawn (PLAN 2.48). A line
    /// or an arrow runs the way the drag went. Its id is new to the deck.
    #[cfg(feature = "editor")]
    pub fn drawing(
        &mut self,
        state: &str,
        n: usize,
        drag: [[f32; 2]; 2],
        free: bool,
    ) -> Result<scaena_ops::inspect::Added, Error> {
        let (insert, room) = self.room(state, n, None)?;
        scaena_ops::inspect::drawing(&self.deck, &room, &insert, state, drag, free)
            .map_err(|e| Error::Deck(e.to_string()))
    }

    /// What is offered `n`th, and where it may go in `state` under an id new to the deck, the
    /// box it starts as its cell: the id made from `named`, a file's name less its extension,
    /// where there is one, else the one offered.
    #[cfg(feature = "editor")]
    fn room(
        &mut self,
        state: &str,
        n: usize,
        named: Option<&str>,
    ) -> Result<(scaena_core::inserts::Insert, scaena_engine::geometry::Targets), Error> {
        self.room_at(state, n, named, None)
    }

    /// [`Session::room`], its cell `wide`, a share of the canvas's width, where given.
    #[cfg(feature = "editor")]
    fn room_at(
        &mut self,
        state: &str,
        n: usize,
        named: Option<&str>,
        wide: Option<f32>,
    ) -> Result<(scaena_core::inserts::Insert, scaena_engine::geometry::Targets), Error> {
        use scaena_core::inserts::{Start, fresh, slug};
        let offered = self.inserts().into_iter().nth(n);
        let insert = offered.ok_or_else(|| Error::Ops(format!("nothing is offered at {n}")))?;
        let kind = insert.node["type"].as_str().unwrap_or("node");
        let stem = named.map(|name| std::path::Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or(name));
        let id = fresh(&self.deck, &stem.map_or_else(|| insert.id.clone(), |stem| slug(stem, kind)));
        let share = match insert.start {
            Start::Box { w, h } => [wide.unwrap_or(w), h],
            Start::Slot(_) => [1.0, 1.0],
        };
        self.duration(state)?;
        let engine = self.engine.as_mut().expect("built for the span");
        let format = self.format.as_deref();
        let req =
            FrameRequest { deck: &self.deck, theme: &self.theme, data: &self.data, state, t_ms: f64::INFINITY, format };
        Ok((insert, engine.room(&req, &id, share)?))
    }

    /// The patch that copies `node`, as `state` shows it, with what it holds there, beside it
    /// under an id new to the deck, clear of the rest where there is room (PLAN 2.34): in the
    /// format shown, and in each other the node lays out in, its own canvas and each format
    /// where it has a layout of its own (ADR-0020), so the copy stands on it in none.
    #[cfg(feature = "editor")]
    pub fn duplicating(&mut self, state: &str, node: &str) -> Result<scaena_ops::inspect::Added, Error> {
        let mut added = self.duplicating_here(state, node)?;
        let shown = self.format.clone();
        let anew: Vec<Option<String>> = (self.deck.nodes.get(node))
            .and_then(|n| n.props.get("formats"))
            .and_then(serde_json::Value::as_object)
            .into_iter()
            .flat_map(|f| f.keys())
            .filter(|f| self.deck.anew().contains(&f.as_str()) && Some(*f) != shown.as_ref())
            .map(|f| Some(f.clone()))
            .collect();
        let elsewhere = shown.is_some().then_some(None).into_iter().chain(anew);
        let there = || -> Result<(), Error> {
            for format in elsewhere {
                self.set_format(format.as_deref())?;
                let placed = self.duplicating_here(state, node)?.patch.into_iter();
                added.patch.extend(placed.filter(|op| op["op"] == "place" && op["node"] == added.id.as_str()));
            }
            Ok(())
        };
        let done = there();
        self.set_format(shown.as_deref())?;
        done.map(|()| added)
    }

    /// [`Session::duplicating`] in the format shown alone.
    #[cfg(feature = "editor")]
    fn duplicating_here(&mut self, state: &str, node: &str) -> Result<scaena_ops::inspect::Added, Error> {
        let mut found = self.targets(state, node)?.clone();
        found.node = scaena_core::inserts::fresh(&self.deck, node);
        let boxes = self.boxes(state)?;
        scaena_ops::inspect::duplicating(&self.deck, &found, node, state, &boxes)
            .map_err(|e| Error::Deck(e.to_string()))
    }

    /// The patch that puts `nodes`, children of one container as `state` shows them, in a new
    /// group where they stand (PLAN 2.43): a `group` op, the group under the first id free from
    /// `group`. The patch says why where the deck refuses it.
    #[cfg(feature = "editor")]
    pub fn grouping(&self, state: &str, nodes: &[String]) -> Grouping {
        let id = scaena_core::inserts::fresh(&self.deck, "group");
        let patch = vec![serde_json::json!({ "op": "group", "id": id, "nodes": nodes, "state": state })];
        Grouping { id, patch }
    }

    /// The patch that deletes `node` from `state`, with what it holds there (PLAN 2.34): each
    /// leaves there and in the states after it, and one no state shows then goes from the deck;
    /// or, `everywhere`, each goes from the deck.
    #[cfg(feature = "editor")]
    pub fn deleting(&self, state: &str, node: &str, everywhere: bool) -> Result<Vec<serde_json::Value>, Error> {
        scaena_ops::inspect::deleting(&self.deck, &editor::Handed(&self.files), node, state, everywhere)
            .map_err(|e| Error::Ops(e.to_string()))
    }

    /// What a copy of `nodes`, as `state` shows them, holds (PLAN 2.37, 2.42): each and what
    /// it holds there, their overrides, the data sources they read, and the files those and
    /// their images read, with each one's box as a share of the canvas in the format shown.
    #[cfg(feature = "editor")]
    pub fn copying(&mut self, state: &str, nodes: &[&str]) -> Result<scaena_ops::clipboard::Clip, Error> {
        let boxes = self.boxes(state)?;
        let rect = |node: &str| boxes.iter().find(|b| b.node == node).map(|b| b.rect);
        let copied = nodes
            .iter()
            .map(|&node| {
                Ok((node, rect(node).ok_or_else(|| Error::Ops(format!("`{node}` stands nowhere in `{state}`")))?))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let [w, h] = self.canvas_size()?;
        let files = Arc::clone(&self.files);
        let read = |p: &str| files.get(p).cloned();
        scaena_ops::clipboard::copying(&self.deck, &self.theme, state, &copied, [w as f32, h as f32], &read)
            .map_err(|e| Error::Ops(e.to_string()))
    }

    /// The patch that pastes `clip` in `state` (PLAN 2.37): each node it holds under an id
    /// new to the deck, the copy of the node copied placed about `at` (canvas units, in the
    /// format shown) as Insert places a node, its box the clip's share of the canvas. The
    /// files it carries that the bundle lacks are handed over here; what the theme lacks is
    /// taken out of the copies and said in its findings.
    #[cfg(feature = "editor")]
    pub fn pasting(
        &mut self,
        clip: &scaena_ops::clipboard::Clip,
        state: &str,
        at: [f32; 2],
    ) -> Result<scaena_ops::clipboard::Pasted, Error> {
        let id = scaena_core::inserts::fresh(&self.deck, &clip.node);
        self.duration(state)?;
        let engine = self.engine.as_mut().expect("built for the span");
        let format = self.format.as_deref();
        let req =
            FrameRequest { deck: &self.deck, theme: &self.theme, data: &self.data, state, t_ms: f64::INFINITY, format };
        let room = engine.room(&req, &id, [clip.share[2], clip.share[3]])?;
        let files = Arc::clone(&self.files);
        let read = |p: &str| files.get(p).cloned();
        let pasted = scaena_ops::clipboard::pasting(&self.deck, &editor::Handed(&files), &read, &room, clip, state, at)
            .map_err(|e| Error::Ops(e.to_string()))?;
        for (path, bytes) in pasted.files.iter().zip(&pasted.bytes) {
            self.add_file(path, bytes.clone());
        }
        Ok(pasted)
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

    /// Text typed on the canvas (ADR-0013, PLAN 2.32): `ops` (a `replace_text`, or the
    /// `style_text` that gives characters selected there a look, PLAN 2.38) made by `user`
    /// at `at` (seconds since the epoch), validated and refused as a patch is but not
    /// linted: the page lints the state it shows after, as it does after a keystroke in the
    /// source. A bundle's history records a run of typing as one change, `type`, and a look
    /// given as a change of its own. Whether it changed the deck.
    #[cfg(feature = "editor")]
    pub fn typed(&mut self, ops: &serde_json::Value, at: Option<i64>) -> Result<bool, Error> {
        let typing = ops.as_array().is_some_and(|ops| ops.iter().all(|op| op["op"] == "replace_text"));
        let (patched, write) = scaena_ops::patch::typing(&self.bundle(), ops, typing.then_some(store::TYPED))?;
        if patched.refused {
            let why = patched.added.iter().find(|f| f.severity == scaena_core::lint::Severity::Error);
            return Err(Error::Ops(format!("the deck refuses it: {}", why.map_or("", |f| f.message.as_str()))));
        }
        self.write(write, assistant::Caller { author: "user", at })
    }

    /// The link drawn at `point` (canvas units) in `state` at rest, in the format shown (PLAN
    /// 2.70): where a click there goes. None off every link.
    pub fn link_at(
        &mut self,
        state: &str,
        point: [f32; 2],
    ) -> Result<Option<scaena_core::displaylist::LinkTarget>, Error> {
        Ok(self.frame(state, f64::INFINITY)?.link_at(point))
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

    /// How `state` reads at rest, in the format shown, a part for each node read, in turn (SPEC
    /// §3.12): what a screen reader speaks for each node on the Mac's canvas (PLAN 3.17).
    pub fn reads(&mut self, state: &str) -> Result<Vec<scaena_core::reading::Part>, Error> {
        let list = self.frame(state, f64::INFINITY)?;
        let snapshots = scaena_core::resolve_states(&self.deck).map_err(|e| Error::Deck(e.to_string()))?;
        let snap = (snapshots.iter().find(|s| s.state_id == state))
            .ok_or_else(|| EngineError::UnknownState(state.to_string()))?;
        Ok(scaena_core::reading::parts(&self.deck, snap, &list))
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
        Ok(self.painter.paint(&dl, &self.store, scale)?)
    }

    /// `state` at rest in the format shown, as a PNG `width` pixels wide, painted by the CPU
    /// painter, the height keeping the canvas's aspect (PLAN 2.54): what
    /// `scaena export --format png --size` writes for it on the deck's canvas. Neither a drag
    /// nor a preview shown draws in it.
    #[cfg(feature = "editor")]
    pub fn png(&mut self, state: &str, width: u32) -> Result<Vec<u8>, Error> {
        use scaena_paint::Painter;
        let timeline = self.timeline()?;
        let slot = timeline.slot(state).ok_or_else(|| EngineError::UnknownState(state.to_string()))?;
        // Its shaders at the time it comes to rest, as `frame` draws them there.
        let time = (slot.start + slot.span) / 1000.0;
        let dl = self.at_rest(state)?.draw_at(time);
        let scale = width as f32 / dl.viewport[0];
        Ok(self.painter.paint(&dl, &self.store, scale)?.to_png()?)
    }

    /// Hold [`Session::pixels`]' frame until its shaders' pixels are in, and say how many
    /// shaders it draws: each one's rows are worked out in bands, here ([`Session::shade`])
    /// or on another worker from its spec ([`Session::shader_spec`], [`shader_rows`]), then
    /// [`Session::shaded`] paints the frame (PLAN 2.28). A frame held before is let go.
    #[cfg(feature = "cpu")]
    pub fn shading(&mut self, state: &str, t_ms: f64, width: u32) -> Result<usize, Error> {
        self.shading = None;
        let dl = self.viewed(state, t_ms)?;
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

    /// How many bytes a row of the held frame's shader `i` takes: its width, four bytes each.
    #[cfg(feature = "cpu")]
    pub fn shader_row(&mut self, i: usize) -> Result<usize, Error> {
        Ok((self.shader(i)?.job.bbox()[2] as usize * 4).max(1))
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
        Ok(self.painter.paint_shaded(&dl, &self.store, scale, pixels)?)
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

/// `out` with `guides` as its `guides`, where there are any (PLAN 2.57).
#[cfg(feature = "editor")]
pub fn with_guides(mut out: serde_json::Value, guides: &[[f32; 4]]) -> serde_json::Value {
    if !guides.is_empty() {
        out["guides"] = serde_json::json!(guides);
    }
    out
}

/// A chart mark or table row as `Player.markAt` gives it (PLAN 2.64), with a chart mark's
/// annotations (PLAN 2.67).
#[cfg(feature = "editor")]
pub fn mark_json(m: scaena_engine::marks::DataMark) -> serde_json::Value {
    let mut out = serde_json::json!({
        "node": m.node, "source": m.source, "key": m.key, "rows": m.rows, "outline": m.outline, "rect": m.rect,
    });
    if let Some(map) = m.transform {
        out["transform"] = serde_json::json!(map);
    }
    if let Some(n) = m.notes {
        out["notes"] = serde_json::json!({
            "x": n.x, "value": n.value, "series": n.series, "axes": n.axes, "callout": n.callout,
            "highlight": n.highlight, "highlighted": n.highlighted,
        });
    }
    out
}

/// A chart annotation as `Player.noteAt` gives it (PLAN 2.67).
#[cfg(feature = "editor")]
pub fn note_json(n: scaena_engine::marks::NoteMark) -> serde_json::Value {
    let mut out = serde_json::json!({
        "node": n.node, "index": n.index, "kind": n.kind, "text": n.text, "outline": n.outline, "rect": n.rect,
    });
    if let Some(map) = n.transform {
        out["transform"] = serde_json::json!(map);
    }
    out
}
/// Each byte offset into `text` as UTF-16 code units, as JavaScript and Swift count a string.
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

    /// A state's layers (PLAN 2.50), as `scaena inspect --layers` says them: nested as their
    /// containers hold them, a stack's children in the order it lays them out, a frame's
    /// topmost first.
    #[test]
    fn a_states_layers_nest_as_their_containers_hold_them() {
        let s = torture();
        let layers = s.layers("containers").unwrap();
        let held = |node: &str| {
            let layer = layers.iter().find(|l| l.node == node).unwrap();
            layer.children.iter().map(|l| l.node.as_str()).collect::<Vec<_>>()
        };
        assert_eq!(held("stats"), ["stat-a", "stat-b", "stat-c"]);
        assert_eq!(held("card"), ["card-tag-label", "card-tag", "card-photo"]);
        assert!(layers.iter().all(|l| l.shown || l.node.starts_with("image-")), "{layers:?}");
        assert!(matches!(s.layers("nowhere"), Err(Error::Engine(EngineError::UnknownState(_)))));
    }

    /// A shape's outline is what a pointer edits it by (PLAN 2.68): a polygon's points, a
    /// line's two where it gives none, a rect's radius among the theme's steps; in 9:16 too,
    /// in the box that format gives it. What is no shape has none.
    #[test]
    fn a_shapes_outline_is_where_the_state_draws_it() {
        let mut s = torture();
        let tri = s.outline("shapes", "shape-tri").unwrap().unwrap();
        assert_eq!((tri.kind, tri.points.len(), tri.fewest), ("polygon", 3, 3));
        let rule = s.outline("shapes", "shape-rule").unwrap().unwrap();
        assert_eq!(rule.points, [[0.0, 0.5], [1.0, 0.5]]);
        let panel = s.outline("shapes", "shape-panel").unwrap().unwrap();
        assert_eq!((panel.radius, panel.radii.len()), (Some(panel.radii[3]), 6));
        let boxed =
            |s: &mut Session, node: &str| s.boxes("shapes").unwrap().into_iter().find(|b| b.node == node).unwrap();
        assert_eq!(tri.rect, boxed(&mut s, "shape-tri").rect);
        assert!(s.outline("shapes", "nowhere").unwrap().is_none());
        if let Some(format) = s.formats().first().cloned() {
            s.set_format(Some(&format)).unwrap();
            let there = s.outline("shapes", "shape-tri").unwrap().unwrap();
            assert_eq!(there.rect, boxed(&mut s, "shape-tri").rect);
        }
    }

    /// The point of an image under a press is the image's own, in fractions of its crop: what
    /// a focal point picked there names (PLAN 2.45). Off the image, or on a node that is no
    /// image, there is none.
    #[test]
    fn a_press_on_an_image_says_the_point_of_it_drawn_there() {
        let mut s = torture();
        let boxes = s.boxes("images").unwrap();
        let rect = |node: &str| boxes.iter().find(|b| b.node == node).unwrap().rect;
        // Cover, about its middle: the box's middle is the image's.
        let [x, y, w, h] = rect("image-cover");
        assert_eq!(s.focal_at("images", "image-cover", [x + w / 2.0, y + h / 2.0]).unwrap(), Some([0.5, 0.5]));
        // Filled from the middle half of the card: the box's left edge is the crop's.
        let [x, y, _, h] = rect("image-fill");
        let [fx, fy] = s.focal_at("images", "image-fill", [x, y + h / 2.0]).unwrap().unwrap();
        assert!(fx.abs() < 1e-3 && (fy - 0.5).abs() < 1e-3, "{fx}, {fy}");
        // Contained in its box, the card leaves bands above and below it, where it is not.
        let [x, y, w, _] = rect("image-contain");
        assert_eq!(s.focal_at("images", "image-contain", [x + w / 2.0, y + 1.0]).unwrap(), None);
        let [x, y, w, h] = rect("case");
        assert_eq!(s.focal_at("images", "case", [x + w / 2.0, y + h / 2.0]).unwrap(), None);
    }

    /// A view paints the part of the canvas it shows at the size shown: what a frame twice
    /// the size shows there, text, shapes, and images to within a level's rounding (PLAN 2.46).
    /// A shader's grain is per device pixel, counted from the corner of what it covers on the
    /// raster, so a view grains its own; its smooth part lies where the larger frame has it.
    /// The strip's thumbnails and exports never see the view.
    #[test]
    fn a_view_paints_what_a_larger_frame_shows_there() {
        use scaena_paint::Painter;
        let mut s = torture();
        for state in ["mesh", "containers", "images"] {
            let whole = s.pixels(state, f64::INFINITY, 3840).unwrap();
            s.set_view(Some([960.0, 540.0, 960.0, 540.0])).unwrap();
            let dl = s.viewed(state, f64::INFINITY).unwrap();
            assert_eq!(dl.viewport, [960.0, 540.0]);
            let part = s.painter.paint(&dl, &s.store, 1920.0 / dl.viewport[0]).unwrap();
            assert_eq!((part.width, part.height), (1920, 1080));
            // The part's pixel (x, y) is the whole's (1920 + x, 1080 + y).
            let ours = |x: usize, y: usize, c: usize| part.rgba[(y * 1920 + x) * 4 + c];
            let theirs = |x: usize, y: usize, c: usize| whole.rgba[((1080 + y) * 3840 + 1920 + x) * 4 + c];
            if state == "mesh" {
                // Each 20 × 20 block's mean, grain and all, within two levels.
                for (bx, by, c) in (0..54).flat_map(|by| (0..96).flat_map(move |bx| (0..4).map(move |c| (bx, by, c)))) {
                    let sum = |f: &dyn Fn(usize, usize, usize) -> u8| {
                        (0..400).map(|i| u32::from(f(bx * 20 + i % 20, by * 20 + i / 20, c))).sum::<u32>()
                    };
                    assert!(sum(&ours).abs_diff(sum(&theirs)) <= 2 * 400, "{state}: block ({bx}, {by}) differs");
                }
            } else {
                for (x, y, c) in (0..1080).flat_map(|y| (0..1920).flat_map(move |x| (0..4).map(move |c| (x, y, c)))) {
                    assert!(ours(x, y, c).abs_diff(theirs(x, y, c)) <= 1, "{state}: ({x}, {y}) differs");
                }
            }
            // The thumbnails' frames are the whole canvas still.
            assert_eq!(s.pixels(state, f64::INFINITY, 480).unwrap().width, 480);
            assert_eq!(s.frame(state, f64::INFINITY).unwrap().viewport, [1920.0, 1080.0]);
            s.set_view(None).unwrap();
        }
        assert!(s.set_view(Some([0.0, 0.0, 0.0, 540.0])).is_err(), "a view has a size");
        assert!(s.set_view(Some([f32::NAN, 0.0, 960.0, 540.0])).is_err(), "and a place");
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
            let kept = |(state, node, t): &(String, String, _)| state == "containers" && node == "tally" && *t == found;
            assert!(s.targets.len() == 1 && s.targets.iter().all(kept), "kept, once");
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
        assert!(s.targets.is_empty(), "the deck changed, and with it where things stand");
        assert_eq!(s.targets("containers", "tally").unwrap().cell, snapped.cell);
        // A way that does not place a node is no target, and a node not on screen is an error.
        assert!(s.snap("containers", "stat-a", SnapMode::Move, cell, false).unwrap().is_none());
        assert!(s.targets("containers", "title").is_err());
    }

    /// The trails example, the site's demo deck, its files handed over as a page hands them.
    fn trails() -> Session {
        let dir = "../../docs/examples";
        let read = |p: &str| std::fs::read_to_string(format!("{dir}/{p}")).unwrap();
        let mut s = Session::new(&read("trails.deck.json"), &read("themes/dusk.theme.json")).unwrap();
        for f in [
            "Fraunces-VF.ttf",
            "Inter-VF.ttf",
            "JetBrainsMono-VF.ttf",
            "Fraunces-Italic-VF.ttf",
            "Inter-Italic-VF.ttf",
            "JetBrainsMono-Italic-VF.ttf",
        ] {
            s.add_file(&format!("fonts/{f}"), std::fs::read(format!("{dir}/fonts/{f}")).unwrap());
        }
        for name in ["budget", "funding", "hours", "miles", "segments", "work"] {
            let path = format!("data/trails-{name}.csv");
            s.add_file(&path, std::fs::read(format!("{dir}/{path}")).unwrap());
        }
        for path in s.image_files() {
            s.add_file(&path, std::fs::read(format!("{dir}/{path}")).unwrap());
        }
        s
    }

    /// A copy, duplicated in a format or on the deck's own canvas, stands clear of its node in
    /// each the node lays out in; a paste brings a layout only for a format the deck lays out
    /// anew, and goes where it is put in each (ADR-0020, PLAN 2.86).
    #[cfg(feature = "editor")]
    #[test]
    fn a_copy_stands_clear_of_its_node_in_every_format() {
        let by = || assistant::Caller { author: "user", at: None };
        let rect =
            |s: &mut Session, n: &str| s.boxes("budget").unwrap().into_iter().find(|b| b.node == n).unwrap().rect;
        let apart = |a: [f32; 4], b: [f32; 4]| {
            a[0] >= b[0] + b[2] || b[0] >= a[0] + a[2] || a[1] >= b[1] + b[3] || b[1] >= a[1] + a[3]
        };
        for shown in [Some("9:16"), None] {
            let mut s = trails();
            s.set_format(shown).unwrap();
            let added = s.duplicating("budget", "budget-why").unwrap();
            s.tool("deck_patch", serde_json::json!({ "ops": added.patch }), by()).unwrap();
            for format in [Some("9:16"), None] {
                s.set_format(format).unwrap();
                let (node, copy) = (rect(&mut s, "budget-why"), rect(&mut s, &added.id));
                assert!(apart(node, copy), "duplicated in {shown:?}, in {format:?}: {node:?} and {copy:?}");
            }
        }
        // Into a deck that lays nothing out anew, the copy brings no layout.
        let mut s = trails();
        let clip = s.copying("budget", &["budget-why"]).unwrap();
        let mut r = revenue();
        let pasted = r.pasting(&clip, "intro", [400.0, 400.0]).unwrap();
        assert!(!serde_json::to_string(&pasted.patch).unwrap().contains("formats"), "{:?}", pasted.patch);
        // Into its own deck, shown in 9:16, it goes where it is put there too.
        s.set_format(Some("9:16")).unwrap();
        let pasted = s.pasting(&clip, "budget", [540.0, 300.0]).unwrap();
        s.tool("deck_patch", serde_json::json!({ "ops": pasted.patch }), by()).unwrap();
        assert!(rect(&mut s, &pasted.id) != rect(&mut s, "budget-why"));
    }

    /// In a format where a node lays out anew, a drag writes its layout there, and the deck's
    /// own canvas keeps it where it stood (ADR-0020, PLAN 2.85).
    #[cfg(feature = "editor")]
    #[test]
    fn a_drag_in_a_format_moves_its_node_there_alone() {
        use scaena_ops::inspect::SnapMode;
        let mut s = trails();
        let own = s.targets("process", "process-title").unwrap().cell;
        s.set_format(Some("9:16")).unwrap();
        let found = s.targets("process", "process-title").unwrap().clone();
        let (cell, pitch) = (found.cell, found.rows[1][0] - found.rows[0][0]);
        let snapped = s
            .snap("process", "process-title", SnapMode::Move, [cell[0], cell[1] + 1.1 * pitch, cell[2], cell[3]], false)
            .unwrap()
            .unwrap();
        assert_eq!(snapped.patch[0]["format"], "9:16", "{:?}", snapped.patch);
        let by = assistant::Caller { author: "user", at: None };
        s.tool("deck_patch", serde_json::json!({ "ops": snapped.patch }), by).unwrap();
        assert!(s.targets("process", "process-title").unwrap().cell[1] > cell[1], "moved down in 9:16");
        s.set_format(None).unwrap();
        assert_eq!(s.targets("process", "process-title").unwrap().cell, own, "and where it stood in its own");
    }

    /// A drag as a presentation app's (PLAN 3.19, ADR-0024): the card let go a column to the left
    /// and 3 units off goes onto the grid's tracks, which draw it in, and takes their cells, its
    /// guides down the column it lands on; let go with nothing to draw it in, it lands where it
    /// was let go, by a `rect`.
    #[cfg(feature = "editor")]
    #[test]
    fn a_drag_lands_where_it_is_let_go_the_grid_drawing_it_in() {
        let mut s = torture();
        let found = s.targets("containers", "card").unwrap().clone();
        let card = found.cell;
        let step = found.columns[1][0] - found.columns[0][0];
        let to = [card[0] - step + 3.0, card[1], card[2], card[3]];
        let (landed, guides) = s.guided_freely("containers", "card", to, false, 6.0).unwrap().unwrap();
        assert_eq!(landed.cell[0], card[0] - step, "{landed:?}");
        assert_eq!(landed.patch[0]["at"], serde_json::json!({ "col": [8, 11], "row": [6, 8] }), "{landed:?}");
        assert!(guides.iter().any(|g| g[0] == g[2] && (g[0] - (card[0] - step)).abs() <= 0.5), "{guides:?}");

        let away = [card[0] + 40.0, card[1] + 17.0, card[2], card[3]];
        let (left, _) = s.guided_freely("containers", "card", away, false, 0.0).unwrap().unwrap();
        assert_eq!(left.cell, away.map(f32::round), "{left:?}");
        let rect = &left.patch[0]["at"]["rect"];
        assert_eq!(rect[0], serde_json::json!(away[0].round()), "{left:?}");
        assert_eq!(rect[1], serde_json::json!(away[1].round()), "{left:?}");

        // A stack's child goes by its order, which nothing lands it out of.
        let flow = s.targets("containers", "stat-b").unwrap().cell;
        assert!(s.guided_freely("containers", "stat-b", flow, false, 6.0).unwrap().is_none());
    }

    /// Guides (PLAN 2.57): the theme's grid in the format shown, and where a box a drag moves
    /// meets what else draws. Off the grid, a box within reach of another's edge goes onto it,
    /// and its patch puts it there; on the grid, it lands on the tracks alone. Several moved
    /// together off the grid go as one box.
    #[cfg(feature = "editor")]
    #[test]
    fn a_drag_meets_what_else_draws_and_off_the_grid_goes_onto_it() {
        use scaena_ops::inspect::SnapMode;
        let mut s = torture();
        let wide = s.grid().unwrap();
        assert_eq!(
            (wide.canvas, wide.columns.len(), wide.rows.len(), wide.baselines.len()),
            ([1920.0, 1080.0], 12, 8, 112)
        );
        s.set_format(Some("9:16")).unwrap();
        assert_eq!(s.grid().unwrap().canvas, [1080.0, 1920.0], "the format shown's grid");
        s.set_format(None).unwrap();

        let boxes = s.boxes("containers").unwrap();
        let rect = |node: &str| boxes.iter().find(|b| b.node == node).unwrap().rect;
        let (card, photo) = (s.targets("containers", "card").unwrap().cell, rect("board-photo"));
        // Off the grid, its left edge 3 units right of the photo's: onto it, in whole units.
        let to = [photo[0] + 3.0, card[1] - 37.0, card[2], card[3]];
        let (snapped, guides) = s.guided("containers", "card", SnapMode::Free, to, false, 6.0).unwrap().unwrap();
        assert_eq!(snapped.cell[0], photo[0].round(), "{snapped:?}");
        assert_eq!(snapped.patch[0]["at"]["rect"][0], serde_json::json!(photo[0].round()));
        let down = guides.iter().find(|g| g[0] == g[2] && (g[0] - photo[0]).abs() <= 0.5);
        assert!(down.is_some_and(|g| g[1] <= photo[1] && g[3] >= snapped.cell[1] + snapped.cell[3]), "{guides:?}");
        // No reach: it stays where the drag left it.
        let (left, _) = s.guided("containers", "card", SnapMode::Free, to, false, 0.0).unwrap().unwrap();
        assert_eq!(left.cell[0], (photo[0] + 3.0).round());
        // On the grid, the tracks alone place it: a box 3 units off its cells lands in them, and
        // its edges meet the grid's other boxes where they stand on the same tracks.
        let nudged = [card[0] + 3.0, card[1], card[2], card[3]];
        let (moved, lines) = s.guided("containers", "card", SnapMode::Move, nudged, false, 6.0).unwrap().unwrap();
        assert_eq!(moved.cell, card, "{moved:?}");
        assert_eq!(moved.patch[0]["at"], serde_json::json!({ "col": [9, 12], "row": [6, 8] }));
        assert!(lines.iter().any(|g| g[1] == g[3] && g[1] == card[1]), "its top meets the board's: {lines:?}");
        // Into a slot or a stack's order, no guides.
        let flow = s.targets("containers", "stat-b").unwrap().cell;
        let (_, none) = s.guided("containers", "stat-b", SnapMode::Order, flow, false, 6.0).unwrap().unwrap();
        assert!(none.is_empty());

        // The card and the board moved together off the grid, 2 units right of where they stand:
        // the box around them goes back onto the canvas's middle and its left margin's edges.
        let both = ["board".to_string(), "card".to_string()];
        let (arranged, lines) = s.together("containers", &both, [2.0, 0.0], true, false, 6.0).unwrap().unwrap();
        let board = s.targets("containers", "board").unwrap().cell;
        assert_eq!(arranged.landed[0].cell[0], board[0], "{arranged:?}");
        assert!(lines.iter().any(|g| g[0] == g[2]), "{lines:?}");
        // Not off the grid, they go by the tracks, and nothing aligns them.
        let (on_grid, _) = s.together("containers", &both, [2.0, 0.0], false, false, 6.0).unwrap().unwrap();
        assert!(on_grid.patch.is_empty(), "two units is no track: {on_grid:?}");
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

        s.set_moving(Some((vec!["tally".into()], [30.0, 0.0])));
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
        s.set_moving(Some((vec!["tally".into()], [30.0, 0.0])));
        s.tool("deck_patch", serde_json::json!({ "ops": resize.patch }), by).unwrap();
        assert!(s.moving.is_none() && s.previewing.is_none(), "the deck changed: the drag is over");
        assert_eq!(s.frame("containers", f64::INFINITY).unwrap(), previewed);
    }

    /// Several nodes at once (PLAN 2.42): a drag of two draws both moved, over the rest, and
    /// `arranging` gives one patch that puts them in place, written where each placement
    /// lives: moved together, aligned, or ordered by `z`. Children of two containers, or of a
    /// stack, are an error that says why.
    #[cfg(feature = "editor")]
    #[test]
    fn several_nodes_move_align_and_order_at_once() {
        use scaena_ops::arrange::{Align, How, Order};
        let mut s = torture();
        let still = s.frame("containers", f64::INFINITY).unwrap();
        s.set_moving(Some((vec!["tally".into()], [0.0, -114.0])));
        let one = s.frame("containers", f64::INFINITY).unwrap();
        s.set_moving(Some((vec!["tally".into(), "card".into()], [0.0, -114.0])));
        let two = s.frame("containers", f64::INFINITY).unwrap();
        assert!(two != one && two != still, "both move, not one");
        s.set_moving(None);

        let nodes = ["tally".to_string(), "card".to_string()];
        let up = s.arranging("containers", &nodes, How::Together { by: [0.0, -114.0], free: false }, false).unwrap();
        let up = up.expect("the grid moves them");
        assert_eq!(up.landed.iter().map(|l| l.node.as_str()).collect::<Vec<_>>(), ["tally", "card"]);
        assert_eq!(up.patch.len(), 2, "{:?}", up.patch);
        let left = s.arranging("containers", &nodes, How::Align(Align::Left), false).unwrap().unwrap();
        assert_eq!(
            left.patch,
            [
                serde_json::json!({ "op": "place", "node": "card", "at": { "col": [1, 4], "row": [6, 8] }, "state": "containers" })
            ]
        );
        let front =
            s.arranging("containers", &["card-photo".into()], How::Order(Order::Front), false).unwrap().unwrap();
        assert_eq!(front.patch[0]["prop"], "z");

        // Made, the card stands at the tally's left edge.
        let by = assistant::Caller { author: "user", at: None };
        s.tool("deck_patch", serde_json::json!({ "ops": left.patch }), by).unwrap();
        assert!(s.targets.is_empty(), "the deck changed, and with it where things stand");
        let card = s.targets("containers", "card").unwrap().cell[0];
        assert_eq!(card, s.targets("containers", "tally").unwrap().cell[0]);

        let two_holders = ["tally".to_string(), "card-photo".to_string()];
        let e = s.arranging("containers", &two_holders, How::Align(Align::Left), false).unwrap_err().to_string();
        assert!(e.contains("arrange what one container holds"), "{e}");
        let stacked = ["stat-a".to_string(), "stat-b".to_string()];
        let e = s.arranging("containers", &stacked, How::Align(Align::Top), false).unwrap_err().to_string();
        assert!(e.contains("the stack `stats`"), "{e}");
    }

    /// A node listed before or after another (PLAN 2.50), as the layers panel drops it: by `z`
    /// in a frame, by the order it lays them out in a stack; into another container, or onto
    /// the canvas, placed as that one places what it holds. Each patch made, the layers list it
    /// there.
    #[cfg(feature = "editor")]
    #[test]
    fn a_node_listed_before_another_goes_there_in_the_layers() {
        use scaena_ops::arrange::How;
        let mut s = torture();
        let next = |to: &str, after: bool| How::Next { to: to.into(), after };
        let held = |s: &Session, node: &str| {
            let layers = s.layers("containers").unwrap();
            let layer = layers.iter().find(|l| l.node == node).unwrap().clone();
            layer.children.into_iter().map(|l| l.node).collect::<Vec<_>>()
        };
        let by = || assistant::Caller { author: "user", at: None };
        assert_eq!(held(&s, "card"), ["card-tag-label", "card-tag", "card-photo"]);
        // A frame's, by `z`: the photo goes over the tag's label.
        let over =
            s.arranging("containers", &["card-photo".into()], next("card-tag-label", false), false).unwrap().unwrap();
        s.tool("deck_patch", serde_json::json!({ "ops": over.patch }), by()).unwrap();
        assert_eq!(held(&s, "card"), ["card-photo", "card-tag-label", "card-tag"]);
        // A stack's, by its order: the last stat goes first.
        let first = s.arranging("containers", &["stat-c".into()], next("stat-a", false), false).unwrap().unwrap();
        s.tool("deck_patch", serde_json::json!({ "ops": first.patch }), by()).unwrap();
        assert_eq!(held(&s, "stats"), ["stat-c", "stat-a", "stat-b"]);
        // Where it is already, nothing to make.
        let there = s.arranging("containers", &["stat-a".into()], next("stat-c", true), false).unwrap().unwrap();
        assert!(there.patch.is_empty(), "{:?}", there.patch);
        // Before a child of another container: into that one, placed as it places what it
        // holds, just over it (`place` with `parent`).
        let card =
            s.arranging("containers", &["stat-a-label".into()], next("card-tag", false), false).unwrap().unwrap();
        s.tool("deck_patch", serde_json::json!({ "ops": card.patch }), by()).unwrap();
        assert_eq!(held(&s, "card"), ["card-photo", "card-tag-label", "stat-a-label", "card-tag"]);
        assert_eq!(held(&s, "stats"), ["stat-c", "stat-a", "stat-b"], "the stat keeps its figure");
        // Into a group, first among what it holds; then out again onto the canvas, just over it.
        let into = How::Into { holder: "marks".into() };
        let group = s.arranging("containers", &["tally".into()], into, false).unwrap().unwrap();
        s.tool("deck_patch", serde_json::json!({ "ops": group.patch }), by()).unwrap();
        assert_eq!(held(&s, "marks"), ["tally", "marks-dot", "marks-ring"]);
        let out = s.arranging("containers", &["tally".into()], next("marks", false), false).unwrap().unwrap();
        s.tool("deck_patch", serde_json::json!({ "ops": out.patch }), by()).unwrap();
        let roots: Vec<String> = s.layers("containers").unwrap().into_iter().map(|l| l.node).collect();
        let at = |node: &str| roots.iter().position(|r| r == node).unwrap();
        assert_eq!(at("tally") + 1, at("marks"), "{roots:?}");
        assert_eq!(held(&s, "marks"), ["marks-dot", "marks-ring"]);
        // Into what it holds, refused with why.
        let e = s.arranging("containers", &["stats".into()], How::Into { holder: "stat-a".into() }, false);
        let e = e.unwrap_err().to_string();
        assert!(e.contains("a node goes into nothing it holds"), "{e}");
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

    /// Characters selected on the canvas take a look (PLAN 2.38): ⌘B's from the weight the
    /// engine sets them in, and the inspector's from the run of the first of them.
    #[cfg(feature = "editor")]
    #[test]
    fn characters_selected_take_a_look_where_the_text_lives() {
        let mut s = revenue();
        let before = s.frame("revenue", f64::INFINITY).unwrap();
        // Dusk's headline is 550, short of bold: ⌘B makes "doubled" bold. Its display is 600,
        // bold itself, so ⌘B on the intro's title takes it to 400.
        assert_eq!(s.bolding("revenue", "title", 8, 15).unwrap(), serde_json::json!({ "style/weight": 700 }));
        assert_eq!(s.bolding("intro", "title", 0, 2).unwrap(), serde_json::json!({ "style/weight": 400 }));
        let look = serde_json::json!({ "style/weight": 700, "style/color": "accent" });
        let ops = serde_json::json!([{ "op": "style_text", "node": "title", "state": "revenue", "from": 8, "to": 15, "look": look }]);
        assert!(s.typed(&ops, None).unwrap());
        assert_eq!(
            s.deck.to_value().unwrap()["states"][1]["props"]["title"]["runs"],
            serde_json::json!([{ "text": "Revenue " }, { "text": "doubled", "style": { "weight": 700, "color": "accent" } }])
        );
        assert_ne!(s.frame("revenue", f64::INFINITY).unwrap(), before, "the frame shows it");
        assert_eq!(s.carets("revenue", "title").unwrap().unwrap().text, "Revenue doubled");
        // Bold now by a weight of its own: ⌘B takes that away.
        assert_eq!(s.bolding("revenue", "title", 8, 15).unwrap(), serde_json::json!({ "style/weight": null }));
        let offered = s.character_choices("revenue", "title", 10, 12).unwrap();
        let value = |prop: &str| offered.fields.iter().find(|f| f.prop == prop).and_then(|f| f.value.clone());
        assert_eq!((value("style/weight"), value("style/color")), (Some(700.into()), Some("accent".into())));
        assert!(s.bolding("revenue", "rev", 0, 1).is_err(), "a chart is no text");
    }

    /// A theme chosen in the editor (PLAN 2.39): one that ships, which the page carries, or one
    /// the bundle holds, re-themes the deck by `user`, and frames are drawn in it; one that
    /// would leave the deck invalid is refused, with why, and the deck keeps its theme. A
    /// theme named in an edit of the source is drawn in too.
    #[cfg(feature = "editor")]
    #[test]
    fn a_theme_chosen_in_the_editor_re_themes_the_deck_or_says_why_not() {
        let mut s = revenue();
        let dir = "../../docs/examples";
        let before = s.frame("revenue", f64::INFINITY).unwrap();
        let theme = |deck: &Deck| deck.theme.clone().and_then(|t| t.as_str().map(String::from));

        let daybreak = std::fs::read_to_string(format!("{dir}/authorability/themes/daybreak.theme.json")).unwrap();
        let themed = s.retheme("themes/daybreak.theme.json", Some(&daybreak), BTreeMap::new(), None).unwrap();
        assert!(themed.applied && !themed.refused, "{themed:?}");
        assert_eq!(theme(&s.deck).as_deref(), Some("themes/daybreak.theme.json"));
        assert_eq!(s.files.get("themes/daybreak.theme.json").map(Vec::as_slice), Some(daybreak.as_bytes()));
        assert_eq!(s.theme.name, "Daybreak");
        let day = s.frame("revenue", f64::INFINITY).unwrap();
        assert_ne!(day, before, "drawn in Daybreak");
        assert_eq!(s.themes().1, ["themes/daybreak.theme.json", "themes/dusk.theme.json"]);
        assert!(s.retheme("themes/daybreak.theme.json", None, BTreeMap::new(), None).is_err(), "in it already");

        // Back to the bundle's own: drawn as it was.
        s.retheme("themes/dusk.theme.json", None, BTreeMap::new(), None).unwrap();
        assert_eq!(s.theme.name, "Dusk");
        assert_eq!(s.frame("revenue", f64::INFINITY).unwrap(), before);

        // A theme that lacks a role the deck uses is refused, and says so.
        let mut thin: serde_json::Value = serde_json::from_slice(&s.files["themes/dusk.theme.json"]).unwrap();
        thin["type"]["roles"].as_object_mut().unwrap().remove("display");
        let thin = thin.to_string();
        let themed = s.retheme("themes/dusk.theme.json", Some(&thin), BTreeMap::new(), None).unwrap();
        assert!(themed.refused && !themed.applied);
        assert_eq!(themed.theme, "themes/dusk-2.theme.json", "the bundle's own Dusk is kept");
        assert!(themed.added.iter().any(|f| f.code == "E102" && f.message.contains("display")), "{:?}", themed.added);
        assert_eq!(theme(&s.deck).as_deref(), Some("themes/dusk.theme.json"));
        assert!(!s.files.contains_key("themes/dusk-2.theme.json"), "nothing written");

        // The theme a source names, once compiled.
        let source = s.source().replace("themes/dusk.theme.json", "themes/daybreak.theme.json");
        assert!(s.compile(&source).valid);
        assert_eq!(s.theme.name, "Daybreak");
        assert_eq!(s.frame("revenue", f64::INFINITY).unwrap(), day);
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
        for path in [
            "fonts/Fraunces-VF.ttf",
            "fonts/Inter-VF.ttf",
            "fonts/JetBrainsMono-VF.ttf",
            "fonts/Fraunces-Italic-VF.ttf",
            "fonts/Inter-Italic-VF.ttf",
            "fonts/JetBrainsMono-Italic-VF.ttf",
            "data/q3-revenue.csv",
        ] {
            s.add_file(path, std::fs::read(format!("{dir}/{path}")).unwrap());
        }
        s
    }

    /// Find and replace across the deck's words (PLAN 2.47, 2.83): each text, and the beat's
    /// claim and the chart's description, once for each place it is written; one match replaced
    /// alone, where its text lives; and every match by the assistant's `deck_find`, one patch.
    #[cfg(feature = "editor")]
    #[test]
    fn find_and_replace_write_each_text_where_it_lives() {
        use scaena_core::patch::Query;
        let mut s = revenue();
        let query = Query { find: "rev".into(), ..Query::default() };
        let found = s.find(&query).unwrap();
        let places: Vec<(&str, &str, usize)> =
            found.iter().map(|f| (f.node.as_deref().unwrap_or_default(), f.lives.as_str(), f.matches.len())).collect();
        assert_eq!(
            places,
            [
                ("title", "/nodes/title/text", 1),
                ("", "/spine/sections/1/beats/0/claim", 1),
                ("title", "/states/1/props/title/text", 1),
                ("rev", "/nodes/rev/alt", 1),
                ("note", "/nodes/note/text", 1),
            ],
            "Review, the claim's Revenue, Revenue doubled, the description's revenue, and Revenue in $M"
        );
        // One match, alone: the third place's first.
        let one = s.replacing(&query, "Inc", Some([2, 0])).unwrap();
        assert_eq!(
            one,
            [
                serde_json::json!({ "op": "replace_text", "node": "title", "from": 0, "to": 3, "text": "Inc", "state": "revenue" })
            ]
        );
        assert!(s.replacing(&query, "Inc", Some([2, 1])).is_err(), "no such match");
        assert!(s.replacing(&query, "Inc", Some([9, 0])).is_err(), "no such text");
        let by = assistant::Caller { author: "user", at: None };
        s.tool("deck_patch", serde_json::json!({ "ops": one }), by).unwrap();
        assert!(s.source().contains("Incenue doubled"), "{}", s.source());
        // Every match left, by the assistant, as an MCP client calls `deck_find`.
        let called = s.tool("deck_find", serde_json::json!({ "find": "rev", "replace": "REV" }), by).unwrap();
        let result: serde_json::Value = serde_json::from_str(&called.result).unwrap();
        assert!(called.edited, "{result}");
        assert_eq!(result["matches"], 4, "{result}");
        assert_eq!(result["replaced"]["applied"], true, "{result}");
        assert!(s.source().contains("Q3 REView"), "{}", s.source());
        assert!(
            s.find(&Query { case: true, find: "Rev".into(), ..query.clone() }).unwrap().is_empty(),
            "every `Rev` is `REV` now"
        );
        // A dry run finds, and writes nothing.
        let source = s.source();
        let dry =
            s.tool("deck_find", serde_json::json!({ "find": "rev", "replace": "x", "dry_run": true }), by).unwrap();
        let result: serde_json::Value = serde_json::from_str(&dry.result).unwrap();
        assert!(!dry.edited && result["replaced"]["applied"] == false, "{result}");
        assert_eq!(s.source(), source);
    }

    /// What may be inserted goes where the pointer is, and a copy of a node beside it (PLAN
    /// 2.34): each patch adds a node new to the deck, entering in the state shown, placed on
    /// the theme's grid as a drop places it, and leaving at the next slide (PLAN 3.24); a
    /// shader fills the canvas under the rest.
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

        // On free ground, a headline lands where the pointer is: under `close`'s title.
        let free = s.inserting("close", n("Text · headline"), [960.0, 900.0], None).unwrap();
        assert!(within(free.cell, [960.0, 900.0]), "{:?}", free.cell);
        assert!(
            free.patch.iter().all(|op| op["op"] != "hide_node"),
            "the last slide's leave nowhere: {:?}",
            free.patch
        );

        // On the chart in the middle of `revenue`, it goes to the room on the grid nearest the
        // pointer, clear of what draws there as lint E101 judges it (PLAN 2.79).
        let crowded = s.at_rest("revenue").unwrap().crowded(|_| false);
        let added = s.inserting("revenue", n("Text · headline"), [960.0, 540.0], None).unwrap();
        assert_eq!(added.id, "headline");
        let ops: Vec<&str> = added.patch.iter().map(|op| op["op"].as_str().unwrap()).collect();
        assert_eq!(ops, ["add_node", "place", "hide_node"]);
        assert_eq!(added.patch[0]["state"], "revenue", "it enters in the state shown");
        assert_eq!(added.patch[2]["state"], "close", "and leaves at the next slide (PLAN 3.24)");
        let apart = |c: [f32; 4], o: [f32; 4]| {
            (c[0] + c[2]).min(o[0] + o[2]) - c[0].max(o[0]) <= 2.0
                || (c[1] + c[3]).min(o[1] + o[3]) - c[1].max(o[1]) <= 2.0
        };
        assert!(crowded.iter().all(|o| apart(added.cell, *o)), "{:?} clear of {crowded:?}", added.cell);
        assert!(!within(added.cell, [960.0, 540.0]), "off the chart: {:?}", added.cell);
        s.tool("deck_patch", serde_json::json!({ "ops": added.patch }), by).unwrap();
        assert_eq!(stands(&mut s, "revenue", "headline"), Some(added.cell));
        assert_eq!(stands(&mut s, "intro", "headline"), None, "nor before it");
        assert!(stands(&mut s, "mix", "headline").is_some(), "the slide's step after it tracks it");
        assert_eq!(stands(&mut s, "close", "headline"), None, "the next slide does not (PLAN 3.24)");

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
        assert!(stands(&mut s, "mix", "headline-2").is_some(), "the copy shows on its slide");
        assert_eq!(stands(&mut s, "close", "headline-2"), None, "and stays there (PLAN 3.24)");
        assert_eq!(s.deck.nodes["headline-2"].props.get("text"), s.deck.nodes["headline"].props.get("text"));

        // In a slot nothing fills, a text fills it: `close` shows nothing in `subtitle`.
        let slots = s.targets("close", "title").unwrap().slots.clone();
        let (_, sub) = slots.iter().find(|(name, _)| name == "subtitle").unwrap();
        let middle = [sub[0] + sub[2] / 2.0, sub[1] + sub[3] / 2.0];
        let lede = s.inserting("close", n("Text · lede"), middle, None).unwrap();
        assert_eq!(lede.cell, *sub);
        assert_eq!(lede.patch[1]["at"], serde_json::json!({ "in": "subtitle" }), "{:?}", lede.patch);
        // Where a slot is filled, it takes the grid's cells: `title` fills `close`'s title slot.
        let (_, title) = slots.iter().find(|(name, _)| name == "title").unwrap();
        let over = s
            .inserting("close", n("Text · lede"), [title[0] + title[2] / 2.0, title[1] + title[3] / 2.0], None)
            .unwrap();
        assert!(over.patch[1]["at"].get("in").is_none(), "{:?}", over.patch);

        // A shader preset fills the canvas, under what is there.
        let shader = s.inserting("revenue", n("Shader · texture"), [10.0, 10.0], None).unwrap();
        assert_eq!(shader.cell, [0.0, 0.0, 1920.0, 1080.0]);
        assert_eq!(shader.patch[0]["node"]["z"], -1);
        s.tool("deck_patch", serde_json::json!({ "ops": shader.patch }), by).unwrap();
        let topmost = s.hit("revenue", [960.0, 540.0]).unwrap();
        assert_ne!(topmost.first().map(|h| h.node.as_str()), Some("texture"), "it draws under the rest");

        // A photo dropped from the desktop is named after its file; its file keeps the name its
        // bytes give it (PLAN 2.79).
        let photo = std::fs::read("../../docs/examples/agent-run/dusk.jpg").unwrap();
        let path = scaena_store::place("Trail Head.jpg", &photo);
        s.add_file(&path, photo);
        let offered = s.inserts();
        let image = offered.iter().position(|i| i.node["src"] == path.as_str()).expect("the photo offered");
        let dropped = s.inserting("close", image, [960.0, 900.0], Some("Trail Head.jpg")).unwrap();
        assert_eq!(dropped.id, "trail-head");
        assert_eq!(dropped.patch[0]["node"]["src"], path.as_str());
        assert_eq!(
            s.inserting("close", image, [960.0, 900.0], None).unwrap().id,
            offered[image].id,
            "a click names it as offered"
        );
    }

    /// A drag draws what is offered over the cells it covers, each edge on the nearest track's as
    /// a resize snaps, or, off the grid, where it was drawn; a line or an arrow runs the way the
    /// drag went, straight across where it was nearly level or upright (PLAN 2.48).
    #[cfg(feature = "editor")]
    #[test]
    fn a_drag_draws_over_the_cells_it_covers_and_a_line_the_way_it_went() {
        let mut s = revenue();
        let by = assistant::Caller { author: "user", at: None };
        let offered = s.inserts();
        let n = |label: &str| offered.iter().position(|i| i.label == label).unwrap();
        let grid = s.targets("revenue", "title").unwrap().clone();
        let step = grid.columns[1][0] - grid.columns[0][0];
        let on = |tracks: &[[f32; 2]], at: f32, side: usize| tracks.iter().any(|t| (t[side] - at).abs() < 0.01);
        let near = |a: f32, b: f32| (a - b).abs() <= step / 2.0 + 0.01;
        let points = |added: &scaena_ops::inspect::Added| -> Option<Vec<[f64; 2]>> {
            let p = added.patch[0]["node"].get("points")?.as_array()?.clone();
            Some(p.iter().map(|xy| [xy[0].as_f64().unwrap(), xy[1].as_f64().unwrap()]).collect())
        };

        // A rectangle drawn from one point to another: each edge a track's, the nearest the drag's.
        let drag = [[300.0, 200.0], [1100.0, 700.0]];
        let rect = s.drawing("revenue", n("Shape · rect"), drag, false).unwrap();
        assert_eq!(rect.id, "rect");
        let ops: Vec<&str> = rect.patch.iter().map(|op| op["op"].as_str().unwrap()).collect();
        assert_eq!(ops, ["add_node", "place", "hide_node"]);
        assert_eq!(rect.patch[0]["state"], "revenue", "it enters in the state shown");
        assert_eq!(rect.patch[2]["state"], "close", "and leaves at the next slide (PLAN 3.24)");
        let [x, y, w, h] = rect.cell;
        let (cols, rows) = (&grid.columns, &grid.rows);
        assert!(on(cols, x, 0) && on(cols, x + w, 1) && on(rows, y, 0) && on(rows, y + h, 1), "{:?}", rect.cell);
        assert!(near(x, 300.0) && near(x + w, 1100.0) && near(y, 200.0) && near(y + h, 700.0), "{:?}", rect.cell);
        // Drawn the other way, from the far corner, it lands the same.
        let back = s.drawing("revenue", n("Shape · rect"), [drag[1], drag[0]], false).unwrap();
        assert_eq!(back.cell, rect.cell);
        s.tool("deck_patch", serde_json::json!({ "ops": rect.patch }), by).unwrap();
        let stands = s.boxes("revenue").unwrap().into_iter().find(|b| b.node == "rect").map(|b| b.rect);
        assert_eq!(stands, Some(rect.cell), "it stands where it was drawn");

        // Off the grid, where it was drawn, in whole canvas units: a `rect`.
        let free = s.drawing("revenue", n("Shape · ellipse"), [[301.4, 199.6], [700.2, 520.0]], true).unwrap();
        assert_eq!(free.cell, [301.0, 200.0, 399.0, 320.0]);
        assert_eq!(free.patch[1]["at"], serde_json::json!({ "rect": [301.0, 200.0, 399.0, 320.0] }));

        // A line runs the way the drag went: up and to the right, from the bottom left corner.
        let line = s.drawing("revenue", n("Shape · line"), [[400.0, 900.0], [1200.0, 300.0]], false).unwrap();
        assert_eq!(points(&line), Some(vec![[0.0, 1.0], [1.0, 0.0]]));
        // Nearly level, it is level: from left to right, a line's own way, nothing written.
        let level = s.drawing("revenue", n("Shape · line"), [[400.0, 500.0], [1200.0, 560.0]], false).unwrap();
        assert_eq!(points(&level), None);
        // An arrow drawn from right to left points left, and one drawn up, up.
        let left = s.drawing("revenue", n("Shape · arrow"), [[1200.0, 500.0], [400.0, 480.0]], false).unwrap();
        assert_eq!(points(&left), Some(vec![[1.0, 0.5], [0.0, 0.5]]));
        let up = s.drawing("revenue", n("Shape · arrow"), [[600.0, 900.0], [640.0, 200.0]], false).unwrap();
        assert_eq!(points(&up), Some(vec![[0.5, 1.0], [0.5, 0.0]]));
        s.tool("deck_patch", serde_json::json!({ "ops": up.patch }), by).unwrap();
        assert!(s.boxes("revenue").unwrap().iter().any(|b| b.node == "arrow"), "the deck takes it");

        // A shader fills its slot, as Insert puts it, however it was drawn.
        let shader = s.drawing("revenue", n("Shader · texture"), drag, false).unwrap();
        assert_eq!(shader.cell, [0.0, 0.0, 1920.0, 1080.0]);
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
        let added = s.inserting("revenue", n, [960.0, 540.0], None).unwrap();
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

    /// A copy goes on the clipboard as JSON, with what it reads, and pastes where the pointer
    /// pressed under an id new to the deck, as Insert places a node: one patch (PLAN 2.37).
    #[cfg(feature = "editor")]
    #[test]
    fn a_copy_pastes_where_the_pointer_pressed_with_what_it_reads() {
        use scaena_ops::clipboard::read;
        let mut s = revenue();
        let by = assistant::Caller { author: "user", at: None };
        let stands = |s: &mut Session, state: &str, node: &str| {
            s.boxes(state).unwrap().into_iter().find(|b| b.node == node).map(|b| b.rect)
        };
        let keys = |m: Vec<&String>| m.into_iter().cloned().collect::<Vec<String>>();

        // The chart, with its data source and the file it reads.
        let clip = s.copying("revenue", &["rev"]).unwrap();
        assert_eq!((clip.node.as_str(), keys(clip.nodes.keys().collect())), ("rev", vec!["rev".to_string()]));
        assert_eq!(clip.nodes["rev"]["type"], "chart");
        assert_eq!(clip.nodes["rev"]["data"], "@q3");
        assert_eq!(keys(clip.data.keys().collect()), ["q3"]);
        assert_eq!(keys(clip.files.keys().collect()), ["data/q3-revenue.csv"]);
        let rect = stands(&mut s, "revenue", "rev").unwrap();
        assert_eq!(clip.share, [rect[0] / 1920.0, rect[1] / 1080.0, rect[2] / 1920.0, rect[3] / 1080.0]);
        // As the clipboard holds it, and back.
        assert_eq!(read(&serde_json::to_string(&clip).unwrap()).unwrap(), Some(clip.clone()));

        // Pasted in `close`, about a point: a chart of its own there, reading the same source.
        let pasted = s.pasting(&clip, "close", [700.0, 600.0]).unwrap();
        assert_eq!(pasted.id, "rev-2");
        assert!(pasted.findings.is_empty() && pasted.files.is_empty(), "{pasted:?}");
        let ops: Vec<&str> = pasted.patch.iter().map(|op| op["op"].as_str().unwrap()).collect();
        assert_eq!(ops, ["add_node", "place"]);
        assert_eq!(pasted.patch[0]["state"], "close", "it enters in the state shown");
        assert_eq!((pasted.cell[2], pasted.cell[3]), (rect[2], rect[3]), "the box it was copied with");
        let [x, y, w, h] = pasted.cell;
        assert!(x <= 700.0 && 700.0 <= x + w && y <= 600.0 && 600.0 <= y + h, "{:?}", pasted.cell);
        s.tool("deck_patch", serde_json::json!({ "ops": pasted.patch }), by).unwrap();
        assert_eq!(stands(&mut s, "close", "rev-2"), Some(pasted.cell));
        assert_eq!(s.deck.nodes["rev-2"].props.get("data"), Some(&serde_json::json!("@q3")));
        assert_eq!(s.deck.data.len(), 1, "no source is declared twice");

        // Pasted on the first slide, it stays there (PLAN 3.24).
        let early = s.pasting(&clip, "intro", [700.0, 600.0]).unwrap();
        let ops: Vec<&str> = early.patch.iter().map(|op| op["op"].as_str().unwrap()).collect();
        assert_eq!(ops, ["add_node", "place", "hide_node"]);
        s.tool("deck_patch", serde_json::json!({ "ops": early.patch }), by).unwrap();
        assert!(stands(&mut s, "intro", &early.id).is_some());
        assert_eq!(stands(&mut s, "revenue", &early.id), None, "the next slide does not show it");
        s.tool("deck_patch", serde_json::json!({ "ops": [{ "op": "remove_node", "id": early.id }] }), by).unwrap();

        // From a bundle whose file of that name holds other rows: the file comes in under a
        // name of its own, and so the source that reads it.
        let mut other = revenue();
        let csv = "quarter,product,revenue,customers\nQ3,Core,12,40\n";
        other.add_file("data/q3-revenue.csv", csv.as_bytes().to_vec());
        let theirs = other.copying("revenue", &["rev"]).unwrap();
        let pasted = s.pasting(&theirs, "close", [1400.0, 600.0]).unwrap();
        assert_eq!(pasted.id, "rev-3");
        assert_eq!(pasted.files, ["data/q3-revenue-2.csv"]);
        assert_eq!(s.files.get("data/q3-revenue-2.csv").map(Vec::as_slice), Some(csv.as_bytes()), "handed over");
        let ops: Vec<&str> = pasted.patch.iter().map(|op| op["op"].as_str().unwrap()).collect();
        assert_eq!(ops, ["add", "add_node", "place"]);
        assert_eq!(pasted.patch[0]["path"], "/data/q3-2");
        assert_eq!(pasted.patch[0]["value"]["source"], "data/q3-revenue-2.csv");
        assert_eq!(pasted.patch[1]["node"]["data"], "@q3-2");
        s.tool("deck_patch", serde_json::json!({ "ops": pasted.patch }), by).unwrap();
        assert_eq!(s.deck.data.len(), 2);

        // Text from anywhere else is no clip, and pastes as a text in the body role; a clip
        // that is damaged or newer is refused, with why.
        assert_eq!(read("Q3 Review").unwrap(), None);
        assert_eq!(read(r#"{ "kind": "scaena/deck" }"#).unwrap(), None);
        let refused = |text: &str| read(text).unwrap_err().message;
        let newer = serde_json::to_string(&scaena_ops::clipboard::Clip {
            version: scaena_ops::clipboard::VERSION + 1,
            ..clip.clone()
        })
        .unwrap();
        assert!(refused(&newer).contains("newer"), "{}", refused(&newer));
        assert!(refused(r#"{ "kind": "scaena/clip", "version": 1 }"#).contains("damaged"));
        let text = scaena_ops::clipboard::of_text(&s.deck, &s.theme, "Margins held.\n").unwrap();
        let pasted = s.pasting(&text, "close", [960.0, 900.0]).unwrap();
        assert_eq!(pasted.id, "body");
        assert_eq!(
            pasted.patch[0]["node"],
            serde_json::json!({ "type": "text", "role": "body", "text": "Margins held." })
        );
        s.tool("deck_patch", serde_json::json!({ "ops": pasted.patch }), by).unwrap();
        assert!(stands(&mut s, "close", "body").is_some());
        assert!(scaena_ops::clipboard::of_text(&s.deck, &s.theme, " \n").is_err(), "nothing to paste");
    }

    /// A CSV dropped on the canvas becomes a data source (the first-deck walk): declared as
    /// `data_attach` declares it, under an id made from its name and new to the deck, each column
    /// typed. Its patch adds it beside the deck's sources, or makes `data` in a deck with none, and
    /// Insert then offers a chart of it. A file that does not read is refused, with why.
    #[test]
    fn a_data_file_dropped_on_the_canvas_is_attached_as_a_source() {
        let by = || assistant::Caller { author: "user", at: None };
        let csv = "month,visits\nApril,1200\nMay,1850\nJune,2900\n";

        // Beside the source the deck has.
        let mut s = revenue();
        let path = s.placing("visits.csv", csv.as_bytes());
        assert_eq!(path, "data/visits.csv");
        s.add_file(&path, csv.as_bytes().to_vec());
        let a = s.attaching(&path, None).unwrap();
        let attached = a.attached.as_ref().expect("a source new to the deck");
        assert!(attached.attached, "{attached:?}");
        assert_eq!((a.data.as_str(), attached.rows), ("visits", 3));
        assert_eq!(a.patch.len(), 1, "{:?}", a.patch);
        assert_eq!(a.patch[0]["path"], "/data/visits");
        assert_eq!(a.patch[0]["value"]["source"], "data/visits.csv");
        assert_eq!(a.patch[0]["value"]["schema"], serde_json::json!({ "month": "string", "visits": "number" }));
        assert!(s.inserts().iter().all(|i| i.label != "Chart · visits"), "nothing is declared before the patch");
        s.tool("deck_patch", serde_json::json!({ "ops": a.patch }), by()).unwrap();
        assert_eq!(s.deck.data.keys().collect::<Vec<_>>(), ["q3", "visits"]);
        let chart = s.inserts().into_iter().find(|i| i.label == "Chart · visits").expect("a chart of it");
        assert_eq!((chart.node["kind"].as_str(), chart.node["data"].as_str()), (Some("bar"), Some("@visits")));

        // The same file again is where it was, and a chart of it reads the source that reads it.
        assert_eq!(s.placing("visits.csv", csv.as_bytes()), path);
        let again = s.attaching(&path, None).unwrap();
        assert_eq!((again.data.as_str(), again.attached.is_none(), again.patch.len()), ("visits", true, 0));
        // Other rows under the same name go beside it: `visits` reads the rows it read.
        let more = "month,visits\nJuly,3400\n";
        let other = s.placing("visits.csv", more.as_bytes());
        assert_eq!(other, "data/visits-2.csv");
        s.add_file(&other, more.as_bytes().to_vec());
        let b = s.attaching(&other, None).unwrap();
        assert_eq!((b.data.as_str(), b.attached.as_ref().map(|a| a.rows)), ("visits-2", Some(1)));
        assert_eq!(s.files["data/visits.csv"], csv.as_bytes());

        // A new deck has no sources: the patch makes `data`.
        let examples = std::path::Path::new("../../docs/examples");
        let fonts: BTreeMap<String, Vec<u8>> = ["Fraunces-VF.ttf", "Inter-VF.ttf", "JetBrainsMono-VF.ttf"]
            .iter()
            .chain(&["Fraunces-Italic-VF.ttf", "Inter-Italic-VF.ttf", "JetBrainsMono-Italic-VF.ttf"])
            .map(|f| (format!("fonts/{f}"), std::fs::read(examples.join("fonts").join(f)).unwrap()))
            .collect();
        let dusk = std::fs::read_to_string(examples.join("themes/dusk.theme.json")).unwrap();
        let mut made = Session::create("dusk.theme.json", &dusk, &fonts, "Trail report").unwrap();
        made.add_file(&path, csv.as_bytes().to_vec());
        let a = made.attaching(&path, None).unwrap();
        assert_eq!(a.patch[0]["path"], "/data", "{:?}", a.patch);
        assert_eq!(a.patch[0]["value"]["visits"]["source"], "data/visits.csv");
        made.tool("deck_patch", serde_json::json!({ "ops": a.patch }), by()).unwrap();
        assert_eq!(made.deck.data.len(), 1);

        // A file that does not read as rows is refused, with why; one the bundle lacks, too.
        made.add_file("data/broken.json", b"{ not json".to_vec());
        let broken = made.attaching("data/broken.json", None).unwrap_err().to_string();
        assert!(!broken.is_empty(), "says why");
        let missing = made.attaching("data/nowhere.csv", None).unwrap_err().to_string();
        assert!(missing.contains("data/nowhere.csv"), "{missing}");
    }

    /// Cells pasted from a sheet (PLAN 2.96) become a data source, typed as the cells read (a
    /// code with its zeros stays text), and a table of it whose columns print each figure as it
    /// was copied. Too wide for the cell offered in the deck's 9:16 format, the table takes the
    /// grid's width.
    #[test]
    fn cells_pasted_from_a_sheet_become_a_source_and_a_table() {
        let by = || assistant::Caller { author: "user", at: None };
        let mut s = revenue();
        assert!(s.cells("Margins held.").is_none(), "words are no sheet");
        let sheet = "Trail\tVisitors\tRevenue\tShare\tCode\r\nRidge\t1,200\t$1,234.50\t12.5%\t007\r\nCreek\t950\t$987.00\t8%\t012\r\n";
        let cells = s.cells(sheet).expect("a sheet's cells");
        let path = s.placing(&format!("{}.csv", cells.name), cells.csv.as_bytes());
        assert_eq!(path, "data/trail-visitors-revenue.csv");
        s.add_file(&path, cells.csv.as_bytes().to_vec());
        let a = s.attaching(&path, Some(cells.schema.clone())).unwrap();
        let schema = serde_json::json!({
            "Trail": "string", "Visitors": "number", "Revenue": "number", "Share": "number", "Code": "string"
        });
        assert_eq!(a.patch[0]["value"]["schema"], schema);
        s.tool("deck_patch", serde_json::json!({ "ops": a.patch }), by()).unwrap();
        let table =
            |i: &scaena_core::inserts::Insert| i.node["type"] == "table" && i.node["data"] == format!("@{}", a.data);
        let n = s.inserts().iter().position(table).expect("a table of it");
        let columns: Vec<serde_json::Value> = (cells.columns.iter().zip(&cells.formats))
            .map(|(field, format)| match format {
                Some(format) => serde_json::json!({ "field": field, "format": format }),
                None => serde_json::json!({ "field": field }),
            })
            .collect();
        let with = serde_json::json!({ "columns": columns });
        let added = s.inserting_with("close", n, [960.0, 760.0], None, with.as_object()).unwrap();
        assert_eq!(added.patch[0]["node"]["columns"][1], serde_json::json!({ "field": "Visitors", "format": ",d" }));
        assert_eq!(added.patch[0]["node"]["columns"][2], serde_json::json!({ "field": "Revenue", "format": "$,.2f" }));
        assert_eq!(added.patch[0]["node"]["columns"][3], serde_json::json!({ "field": "Share", "format": ".1~%" }));
        // Half the canvas holds its five columns in 16:9, but not in 9:16: it takes the grid's width.
        assert_eq!(added.patch[1]["at"]["col"], serde_json::json!([1, 12]), "{:?}", added.patch);
        s.tool("deck_patch", serde_json::json!({ "ops": added.patch }), by()).unwrap();
        let read = s.reading("close").unwrap();
        for figure in ["1,200", "950", "$1,234.50", "$987.00", "12.5%", "8%", "007"] {
            assert!(read.contains(figure), "{figure} as copied: {read}");
        }
    }

    /// A paste into a deck whose theme lacks what the copy names takes it out of the copy, and
    /// says so in findings; the deck takes the rest. A container comes with copies of what it
    /// holds, each held by the copy of what held it (PLAN 2.37).
    #[cfg(feature = "editor")]
    #[test]
    fn a_paste_takes_out_what_the_theme_lacks_and_a_container_brings_what_it_holds() {
        let mut s = revenue();
        let mut t = torture();

        // Dusk's `title` role is not the torture theme's.
        let clip = s.copying("intro", &["subtitle"]).unwrap();
        assert_eq!(clip.nodes["subtitle"]["role"], "title");
        let pasted = t.pasting(&clip, "shapes", [960.0, 900.0]).unwrap();
        assert_eq!(pasted.id, "subtitle");
        let found: Vec<(&str, Option<&str>)> =
            pasted.findings.iter().map(|f| (f.code.as_str(), f.path.as_deref())).collect();
        assert_eq!(found, [("E102", Some("/nodes/subtitle/role"))], "{:?}", pasted.findings);
        assert!(pasted.findings[0].message.contains("role `title`"), "{:?}", pasted.findings);
        // Dusk's title, 48 cu, is nearest its body, 32, of the roles every theme has.
        assert_eq!(clip.roles.get("title").map(String::as_str), Some("body"));
        assert_eq!(pasted.patch[0]["node"]["role"], "body", "in its place: {:?}", pasted.patch);
        assert!(t.typed(&serde_json::json!(pasted.patch), None).unwrap(), "the deck takes {:?}", pasted.patch);
        assert_eq!(t.deck.nodes["subtitle"].props.get("text"), s.deck.nodes["subtitle"].props.get("text"));

        // The chart brings its source and its file, which the torture bundle lacks.
        let clip = s.copying("revenue", &["rev"]).unwrap();
        let pasted = t.pasting(&clip, "shapes", [960.0, 500.0]).unwrap();
        assert_eq!(pasted.files, ["data/q3-revenue.csv"]);
        assert!(pasted.findings.is_empty(), "the torture theme has every name the chart takes: {:?}", pasted.findings);
        assert!(t.typed(&serde_json::json!(pasted.patch), None).unwrap(), "the deck takes {:?}", pasted);
        let declared = |s: &Session| serde_json::to_value(&s.deck.data["q3"]).unwrap();
        assert_eq!(declared(&t), declared(&s));
        assert!(t.boxes("shapes").unwrap().iter().any(|b| b.node == pasted.id && b.draws));

        // The card holds a photo, a tag, and the tag's label.
        let clip = t.copying("containers", &["card"]).unwrap();
        let ids: Vec<&str> = clip.nodes.keys().map(String::as_str).collect();
        assert_eq!(ids[0], "card");
        assert!(ids.contains(&"card-photo") && ids.contains(&"card-tag-label"), "{ids:?}");
        assert!(clip.files.keys().any(|p| p.starts_with("assets/")), "the photo's file: {:?}", clip.files.keys());
        let pasted = t.pasting(&clip, "containers", [300.0, 300.0]).unwrap();
        assert_eq!(pasted.id, "card-2");
        assert!(pasted.files.is_empty(), "the bundle holds the photo's bytes already");
        assert!(t.typed(&serde_json::json!(pasted.patch), None).unwrap(), "the deck takes {:?}", pasted.patch);
        let snaps = scaena_core::resolve_states(&t.deck).unwrap();
        let snap = snaps.iter().find(|snap| snap.state_id == "containers").unwrap();
        let parent = |id: &str| snap.nodes[id].get("at").and_then(|a| a.get("parent")).and_then(|p| p.as_str());
        for held in ["card-photo-2", "card-tag-2", "card-tag-label-2"] {
            assert_eq!(parent(held), Some("card-2"), "{held} is held by the copy of the card");
        }
        assert_eq!(parent("card-2"), None, "pasted at the root, where the pointer pressed");
    }

    /// ⌘G's patch groups the nodes selected under an id new to the deck, where they stand,
    /// and `ungroup` gives the deck back as it was (PLAN 2.43).
    #[cfg(feature = "editor")]
    #[test]
    fn grouping_makes_a_group_where_they_stand_and_ungroup_gives_the_deck_back() {
        let mut t = torture();
        let before = serde_json::to_string(&t.deck).unwrap();
        let boxes = t.boxes("containers").unwrap();
        let grouped = t.grouping("containers", &["tally".to_string(), "card".to_string()]);
        assert_eq!(grouped.id, "group");
        assert!(t.typed(&serde_json::json!(grouped.patch), None).unwrap(), "the deck takes {:?}", grouped.patch);
        let after = t.boxes("containers").unwrap();
        let rect =
            |boxes: &[scaena_engine::geometry::NodeBox], n: &str| boxes.iter().find(|b| b.node == n).map(|b| b.rect);
        for node in ["tally", "card", "card-photo"] {
            assert_eq!(rect(&after, node), rect(&boxes, node), "{node} stands where it stood");
        }
        let parent = |n: &str| after.iter().find(|b| b.node == n).and_then(|b| b.parent.clone());
        assert_eq!((parent("tally"), parent("card")), (Some("group".into()), Some("group".into())));
        assert_eq!(t.grouping("containers", &["marks".to_string()]).id, "group-2", "the next is free");
        assert!(t.typed(&serde_json::json!([{ "op": "ungroup", "group": "group" }]), None).unwrap());
        assert_eq!(serde_json::to_string(&t.deck).unwrap(), before, "ungrouped, the deck is as it was");
    }

    /// Nodes copied together paste where they stood about each other, each under an id new to
    /// the deck, in one patch; one that another of them holds comes with that one (PLAN 2.42).
    #[cfg(feature = "editor")]
    #[test]
    fn several_nodes_copied_together_paste_as_they_stood() {
        let mut t = torture();
        let before = t.boxes("containers").unwrap();
        let clip = t.copying("containers", &["tally", "card", "card-photo"]).unwrap();
        assert_eq!((clip.version, clip.node.as_str()), (2, "tally"), "a clip of several is version 2");
        let more: Vec<&str> = clip.more.iter().map(|m| m.node.as_str()).collect();
        assert_eq!(more, ["card"], "the photo comes with the card that holds it");
        let ids: Vec<&str> = clip.nodes.keys().map(String::as_str).collect();
        assert_eq!(ids.iter().filter(|id| **id == "card-photo").count(), 1, "once: {ids:?}");
        let one = t.copying("containers", &["card"]).unwrap();
        assert_eq!(
            (one.version, one.more.len()),
            (1, 0),
            "a clip of one stays version 1, which Scaena before 2.42 reads"
        );

        let text = serde_json::to_string(&clip).unwrap();
        let clip = scaena_ops::clipboard::read(&text).unwrap().expect("a clip");
        let rect = |boxes: &[scaena_engine::geometry::NodeBox], node: &str| {
            boxes.iter().find(|b| b.node == node).map(|b| b.rect).unwrap_or_else(|| panic!("{node} stands nowhere"))
        };
        let (tally, card) = (rect(&before, "tally"), rect(&before, "card"));
        // In the middle, and pressed by the grid's bottom edge and its corner: the edge stops
        // all of them, never one alone.
        for (n, at) in [(2, [960.0, 540.0]), (3, [960.0, 1060.0]), (4, [1910.0, 1070.0]), (5, [10.0, 10.0])] {
            let pasted = t.pasting(&clip, "containers", at).unwrap();
            let (copy, beside) = (format!("tally-{n}"), format!("card-{n}"));
            assert_eq!((&pasted.id, pasted.also.as_slice()), (&copy, [beside.clone()].as_slice()));
            assert!(t.typed(&serde_json::json!(pasted.patch), None).unwrap(), "the deck takes {:?}", pasted.patch);
            let after = t.boxes("containers").unwrap();
            let (tally2, card2) = (rect(&after, &copy), rect(&after, &beside));
            for i in 0..2 {
                let (was, is) = (card[i] - tally[i], card2[i] - tally2[i]);
                assert!(
                    (was - is).abs() < 0.5,
                    "pasted about {at:?}, the card stands where it stood about the tally: {was} then, {is} now"
                );
            }
            assert!(after.iter().any(|b| b.node == format!("card-photo-{n}")), "with what the card holds");
        }
        let newer =
            serde_json::to_string(&scaena_ops::clipboard::Clip { version: scaena_ops::clipboard::VERSION + 1, ..clip })
                .unwrap();
        assert!(scaena_ops::clipboard::read(&newer).is_err(), "a clip newer than this Scaena reads is refused");
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

    /// The inspector edits the state shown (PLAN 2.36): a layout chosen is written where it
    /// lives, and each state that takes it from there is laid out in it; a transition, a hold,
    /// and notes are the state's own. What a choice reaches is what the editor says of it.
    #[cfg(feature = "editor")]
    #[test]
    fn a_state_chosen_in_the_inspector_changes_where_it_lives() {
        use serde_json::json;
        let mut s = revenue();
        let by = assistant::Caller { author: "user", at: None };
        let field = |s: &Session, state: &str, prop: &str| {
            let offered = s.state_choices(state).unwrap();
            offered.fields.into_iter().find(|f| f.prop == prop).unwrap()
        };
        assert_eq!(field(&s, "mix", "layout").value, Some(json!("figure")));
        let at_rest = |s: &mut Session, state: &str| s.frame(state, f64::INFINITY).unwrap().digest().unwrap();
        let (revenue, mix) = (at_rest(&mut s, "revenue"), at_rest(&mut s, "mix"));

        // `mix` takes its layout from `revenue`: a layout chosen there reaches both, and kept
        // to `mix`, it reaches `mix` alone. Its hold is its own.
        let full = json!([{ "op": "set_state", "id": "mix", "prop": "layout", "value": "full" }]);
        assert_eq!(s.reach(full.as_array().unwrap()).unwrap(), ["revenue", "mix"]);
        let kept = json!([{ "op": "set_state", "id": "mix", "prop": "layout", "value": "full", "fork": true }]);
        assert_eq!(s.reach(kept.as_array().unwrap()).unwrap(), ["mix"]);
        let hold = json!([{ "op": "set_state", "id": "mix", "prop": "hold", "value": 2500 }]);
        assert_eq!(s.reach(hold.as_array().unwrap()).unwrap(), ["mix"]);

        s.tool("deck_patch", json!({ "ops": full }), by).unwrap();
        assert_eq!(field(&s, "mix", "layout").lives, Some(scaena_core::choices::Where::State("revenue".into())));
        assert_ne!(at_rest(&mut s, "revenue"), revenue, "laid out again in `full`");
        assert_ne!(at_rest(&mut s, "mix"), mix);

        // A transition chosen changes the state's cue: `slow` to `fast`, and a cut has none.
        let slow = s.duration("mix").unwrap();
        let fast = json!([{ "op": "set_state", "id": "mix", "prop": "transition/duration", "value": "fast" }]);
        s.tool("deck_patch", json!({ "ops": fast }), by).unwrap();
        assert!(s.duration("mix").unwrap() < slow);
        let cut = json!([{ "op": "set_state", "id": "mix", "prop": "transition", "value": null }]);
        s.tool("deck_patch", json!({ "ops": cut }), by).unwrap();
        assert_eq!(s.duration("mix").unwrap(), 0.0, "a state with no transition and no motions cuts in");
        assert!(
            s.state_choices("mix")
                .unwrap()
                .fields
                .iter()
                .all(|f| !f.prop.starts_with("transition/") || f.value.is_none())
        );

        // A layout without a slot for each node is not offered, and a patch that asks for one
        // anyway is refused, as validation finds it.
        let scaena_core::choices::Takes::Name { names, .. } = field(&s, "mix", "layout").takes else { panic!() };
        assert!(!names.contains(&"statement".to_string()), "{names:?}");
        let statement = json!([{ "op": "set_state", "id": "mix", "prop": "layout", "value": "statement" }]);
        let refused = s.tool("deck_patch", json!({ "ops": statement }), by).unwrap();
        let said: serde_json::Value = serde_json::from_str(&refused.result).unwrap();
        assert_eq!((said["applied"].as_bool(), refused.edited), (Some(false), false), "{said}");
    }

    /// A chart inserted from a data source reads what the inspector chooses (PLAN 2.41): a field
    /// from the columns of its data that its channel can read, and another source, which points
    /// what that source cannot serve again; each one patch by `user`, validated and linted as
    /// the editor's are, and drawn.
    #[cfg(feature = "editor")]
    #[test]
    fn a_chart_inserted_from_its_data_reads_what_the_inspector_chooses() {
        use scaena_core::choices::Takes;
        use serde_json::json;
        let mut s = revenue();
        let by = assistant::Caller { author: "user", at: None };
        let applied = |s: &mut Session, ops: serde_json::Value| {
            let called = s.tool("deck_patch", json!({ "ops": ops }), by).unwrap();
            let said: serde_json::Value = serde_json::from_str(&called.result).unwrap();
            assert_eq!(said["applied"], true, "{said}");
        };
        let field = |s: &Session, prop: &str| {
            let offered = s.choices("revenue", "q3-chart").unwrap();
            offered.fields.into_iter().find(|f| f.prop == prop).unwrap_or_else(|| panic!("no {prop}"))
        };
        let words = |s: &Session, prop: &str| match field(s, prop).takes {
            Takes::Word { words } => words,
            takes => panic!("{prop}: {takes:?}"),
        };
        let drawn = |s: &mut Session| s.frame("revenue", f64::INFINITY).unwrap().digest().unwrap();

        // The chart of `q3`, its quarters grouped by product, about the middle of the canvas.
        let offered = s.inserts();
        let n = offered.iter().position(|i| i.label == "Chart · q3").expect("a chart of each source");
        assert!(offered.iter().any(|i| i.label == "Table · q3"));
        let added = s.inserting("revenue", n, [960.0, 540.0], None).unwrap();
        assert_eq!(added.id, "q3-chart");
        applied(&mut s, json!(added.patch));
        assert_eq!(field(&s, "series/field").value, Some(json!("product")));
        assert_eq!(words(&s, "y/field"), ["revenue", "customers"], "a y reads numbers");

        // Customers, not revenue: drawn otherwise.
        let before = drawn(&mut s);
        let customers = json!([{ "op": "choose", "node": "q3-chart", "prop": "y/field", "value": "customers", "state": "revenue" }]);
        applied(&mut s, customers);
        assert_eq!(field(&s, "y/field").value, Some(json!("customers")));
        assert_ne!(drawn(&mut s), before);

        // Another source, of other columns: the chart reads its segments and their sales, and
        // its series, which that source has no column for, goes.
        let source = json!([{ "op": "add", "path": "/data/segments", "value": {
            "source": { "inline": [
                { "segment": "Core", "sales": 30.1 },
                { "segment": "Pro", "sales": 21.4 },
                { "segment": "Enterprise", "sales": 15.2 },
            ] },
            "schema": { "segment": "string", "sales": "number" },
        } }]);
        applied(&mut s, source);
        assert_eq!(words(&s, "data"), ["@q3", "@segments"]);
        let segments =
            json!([{ "op": "choose", "node": "q3-chart", "prop": "data", "value": "@segments", "state": "revenue" }]);
        applied(&mut s, segments);
        assert_eq!(
            (field(&s, "x/field").value, field(&s, "y/field").value),
            (Some(json!("segment")), Some(json!("sales")))
        );
        assert!(field(&s, "series/field").value.is_none());
        assert_eq!(words(&s, "x/field"), ["segment", "sales"]);
        drawn(&mut s);
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
        assert!(offered.iter().any(|i| i.node["type"] == "chart"), "a chart of a data source (PLAN 2.41)");
        assert!(offered.iter().any(|i| i.node["type"] == "table"), "and a table");
        for (n, insert) in offered.iter().enumerate() {
            let added =
                s.inserting("axes", n, [700.0, 400.0], None).unwrap_or_else(|e| panic!("{}: {e}", insert.label));
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

    /// A click on a link at rest goes where it says (PLAN 2.70): the torture deck's `links`
    /// case, its web address and its state; nothing off the link.
    #[test]
    fn a_link_is_found_where_it_is_drawn() {
        use scaena_core::displaylist::LinkTarget;
        let mut s = torture();
        let links = s.frame("links", f64::INFINITY).unwrap().links();
        assert_eq!(links.len(), 3);
        let middle = |l: &scaena_core::displaylist::LinkArea| {
            let [x, y, w, h] = l.bounds();
            [x + w / 2.0, y + h / 2.0]
        };
        assert_eq!(
            s.link_at("links", middle(&links[0])).unwrap(),
            Some(LinkTarget::Href("https://example.com/scaena/method".into()))
        );
        assert_eq!(s.link_at("links", middle(&links[2])).unwrap(), Some(LinkTarget::State("shapes".into())));
        assert_eq!(s.link_at("links", [5.0, 5.0]).unwrap(), None);
        assert_eq!(serde_json::to_string(&LinkTarget::State("shapes".into())).unwrap(), r#"{"state":"shapes"}"#);
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
        assert_eq!(s.states().len(), 60);
    }

    /// A chart's marks and the rows of its source (PLAN 2.64): each bar of the revenue chart is
    /// the row of `q3` its sheet shows as that quarter and product; a row marks its bar, and a
    /// point on the bar names the row, in the format shown.
    #[cfg(feature = "editor")]
    #[test]
    fn each_mark_is_the_row_of_its_source_it_was_made_from() {
        let mut s = revenue();
        let sheet = s.data_sheet("q3").unwrap().0;
        let mut rects = Vec::new();
        for (row, cells) in sheet.rows.iter().enumerate() {
            let marks = s.marks_of("revenue", "q3", &[row]).unwrap();
            let [mark] = &marks[..] else { panic!("row {row}: {marks:?}") };
            assert_eq!((mark.node.as_str(), mark.rows.as_slice()), ("rev", [row].as_slice()));
            assert_eq!(mark.key, format!("{}\u{1f}{}", cells[0], cells[1]), "quarter and product");
            let [x, y, w, h] = mark.rect;
            let found = s.mark_at("revenue", [x + w / 2.0, y + h / 2.0]).unwrap();
            assert_eq!(found.as_ref(), Some(mark), "the bar's middle names its row");
            rects.push(mark.rect);
        }
        // Nothing reads `q3` in the intro, and no row is past its last.
        assert!(s.marks_of("intro", "q3", &[0]).unwrap().is_empty());
        assert!(s.marks_of("revenue", "q3", &[sheet.rows.len()]).unwrap().is_empty());
        // In 9:16 the chart is laid out again: each row's bar stands elsewhere, made from the same row.
        s.set_format(Some("9:16")).unwrap();
        for (row, rect) in rects.iter().enumerate() {
            let marks = s.marks_of("revenue", "q3", &[row]).unwrap();
            assert_eq!(marks.len(), 1);
            assert_ne!(&marks[0].rect, rect, "row {row}");
            let [x, y, w, h] = marks[0].rect;
            let found = s.mark_at("revenue", [x + w / 2.0, y + h / 2.0]).unwrap().unwrap();
            assert_eq!(found.rows, [row]);
        }
    }
}
