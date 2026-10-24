//! # scaena-mcp
//!
//! The MCP server (SPEC §7.2, ADR-0003): the operations every client exposes
//! ([`scaena_ops`]), as tools an agent calls over stdio, and the format's schemas, lint
//! catalog, and examples as resources, so an agent can learn the format without the docs.
//!
//! Tool names mirror the CLI. Each tool's input and output schemas are generated from the
//! Rust types and shipped in `docs/schema/mcp/`. A tool that stops returns an error result
//! whose text is `{ "message", "plan"?, "op"? }`: what stopped it, the PLAN task that builds
//! what it needs, the op of a patch that does not apply. Paths are on this machine,
//! relative to the server's working directory: the server runs where the agent does, and
//! assumes no other server (ADR-0006).

use base64::Engine as _;
use indexmap::IndexMap;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CacheScope, CallToolResult, ContentBlock, ErrorData, Implementation, ListResourcesResult, PaginatedRequestParams,
    ProtocolVersion, ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource, ResourceContents,
    ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{Json, RoleServer, ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use scaena_core::Finding;
use scaena_core::patch::Query;
use scaena_ops::OpsError;
use scaena_ops::export::{Exported, Progress, Running};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::watch;

use scaena_resources as resources;
pub use scaena_resources::{LIMIT, resource};

pub mod maps;

/// What makes the requests a tool sends outside this machine (ADR-0030): the client's, which the
/// CLI hands the server, so the server links no HTTP client. None, and no tool reaches out.
#[derive(Clone, Default)]
pub struct Reach(Option<Arc<dyn scaena_objects::Transport + Send + Sync>>);

impl std::fmt::Debug for Reach {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() { "Reach(a transport)" } else { "Reach(none)" })
    }
}

/// The server: its tools, and the resources it serves.
#[derive(Debug, Clone)]
pub struct Scaena {
    tool_router: ToolRouter<Self>,
    /// The exports this server is running, or has run and not yet handed back, by the
    /// file each writes (SPEC §7.2).
    exports: Arc<Mutex<HashMap<PathBuf, Export>>>,
    /// How long `deck_export` waits for an export before it answers that it is running.
    wait: Duration,
    /// What makes `map_fetch`'s requests.
    reach: Reach,
    /// Where `out` may be written besides beside a bundle; the working directory without it.
    dir: Option<PathBuf>,
    /// The home folder, which is never such a place; `$HOME` without it.
    home: Option<PathBuf>,
}

impl Default for Scaena {
    fn default() -> Self {
        let mut tool_router = Self::tool_router();
        for route in tool_router.map.values_mut() {
            route.attr.input_schema = standard(&route.attr.input_schema);
            route.attr.output_schema = route.attr.output_schema.as_deref().map(standard);
        }
        Scaena {
            tool_router,
            exports: Arc::default(),
            wait: EXPORT_WAIT,
            reach: Reach::default(),
            dir: None,
            home: None,
        }
    }
}

impl Scaena {
    /// The same server, its exports answering after `wait` at most.
    pub fn with_export_wait(mut self, wait: Duration) -> Self {
        self.wait = wait;
        self
    }

    /// The same server, its requests outside this machine made by `transport` (ADR-0030).
    pub fn with_transport(mut self, transport: Arc<dyn scaena_objects::Transport + Send + Sync>) -> Self {
        self.reach = Reach(Some(transport));
        self
    }

    /// The same server, writing `out` under `dir` in place of its working directory.
    pub fn with_dir(mut self, dir: PathBuf) -> Self {
        self.dir = Some(dir);
        self
    }

    /// Where a tool writes `out`, a file named as `wrote` says or a folder: under this server's
    /// working directory or the folder that holds `bundle` (see [`Self::within`]). So a path an
    /// agent was told by what it read (a deck, a data file, a page) reaches nothing else the user
    /// can write, and no tool writes a key, a shell's start-up file, or a launch agent.
    fn writable(&self, out: &str, bundle: &str, wrote: Wrote) -> Result<PathBuf, OpsError> {
        let path = self.within(out, bundle, "writes")?;
        let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        match wrote {
            Wrote::File(want) if ext.as_deref() != Some(want) => {
                Err(OpsError::new(format!("{out} is no .{want} file, which is what this writes there")))
            }
            Wrote::Folder if ext.is_some() && !path.is_dir() => {
                Err(OpsError::new(format!("{out} is a file's name: this writes a folder of images there")))
            }
            _ => Ok(path),
        }
    }

    /// The file a tool copies into a bundle, held as [`Self::writable`] holds what it writes: so a
    /// path an agent was told reads nothing else the user can, into a bundle that is shared.
    fn readable(&self, file: &Path, bundle: &str) -> Result<PathBuf, OpsError> {
        self.within(&file.to_string_lossy(), bundle, "reads")
    }

    /// The theme file a tool reads, held as [`Self::readable`] holds a data file. A theme that
    /// ships, by its name, where no file of that name is, passes as it is: nothing is read from
    /// the disk for it.
    fn theme_readable(&self, theme: &str, bundle: &str) -> Result<PathBuf, OpsError> {
        let asked = Path::new(theme);
        let there = |p: &Path| std::fs::symlink_metadata(p).is_ok();
        if !there(asked) && !there(&self.here()?.join(asked)) {
            return Ok(asked.to_path_buf());
        }
        self.readable(asked, bundle)
    }

    /// This server's working directory, or the folder it was given in its place.
    fn here(&self) -> Result<PathBuf, OpsError> {
        match &self.dir {
            Some(dir) => Ok(dir.clone()),
            None => std::env::current_dir().map_err(|e| OpsError::new(format!("this server has no folder: {e}"))),
        }
    }

    /// `out` read through every link, where it is under this server's working directory or the
    /// folder that holds `bundle`, and nothing under that folder it passes through is hidden
    /// (`.ssh`, `.aws`, `.config`, `.git`, a shell's start-up files); else why not, saying where
    /// this server `does` its work. The root of the disk and the home folder are no such folder:
    /// a server started there, or a bundle kept straight in either, would reach all of it.
    fn within(&self, out: &str, bundle: &str, does: &str) -> Result<PathBuf, OpsError> {
        let here = self.here()?;
        let home = (self.home.clone()).or_else(|| std::env::var_os("HOME").map(PathBuf::from));
        let home = home.and_then(|h| std::fs::canonicalize(h).ok());
        let beside = std::fs::canonicalize(bundle).ok().and_then(|b| b.parent().map(Path::to_path_buf));
        let roots: Vec<PathBuf> = [Some(here.clone()), beside]
            .into_iter()
            .flatten()
            .filter_map(|r| std::fs::canonicalize(r).ok())
            .filter(|r| r.parent().is_some() && Some(r) != home.as_ref())
            .collect();
        if roots.is_empty() {
            return Err(OpsError::new(format!(
                "{out}: this server {does} only under its own folder or the bundle's, and neither is one of its \
                 own (the disk's root or the home folder): keep the bundle in a folder of its own"
            )));
        }
        let refused = || {
            let places: Vec<String> = roots.iter().map(|r| r.display().to_string()).collect();
            OpsError::new(format!(
                "{out} is outside where this server {does}: under {}, in no hidden folder",
                places.join(" or ")
            ))
        };
        let asked = Path::new(out);
        if out.is_empty() || asked.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err(refused());
        }
        let asked = here.join(asked);
        // The nearest part of it that is there, read through its links; the rest is made new.
        let mut there = asked.as_path();
        let mut made = Vec::new();
        while std::fs::symlink_metadata(there).is_err() {
            let (Some(name), Some(parent)) = (there.file_name(), there.parent()) else { return Err(refused()) };
            made.push(name);
            there = parent;
        }
        let mut real = std::fs::canonicalize(there).map_err(|_| refused())?;
        real.extend(made.iter().rev());
        let hidden = |below: &Path| below.components().any(|c| c.as_os_str().to_string_lossy().starts_with('.'));
        let held = roots.iter().any(|r| real.strip_prefix(r).is_ok_and(|below| !hidden(below)));
        if held { Ok(real) } else { Err(refused()) }
    }
}

/// What a tool writes at a path: a file of this extension, or a folder.
#[derive(Debug, Clone, Copy)]
enum Wrote {
    File(&'static str),
    Folder,
}

impl Wrote {
    /// What `deck_export` writes in `format`, named as `scaena_export::Format` names it; none for
    /// a format it does not know, which the export refuses before it writes.
    fn exported(format: &str) -> Option<Wrote> {
        Some(match format {
            "pdf" => Wrote::File("pdf"),
            "mp4" => Wrote::File("mp4"),
            "webm" => Wrote::File("webm"),
            "prores" => Wrote::File("mov"),
            "html" => Wrote::File("html"),
            "spine" => Wrote::File("json"),
            "png" | "svg" => Wrote::Folder,
            _ => return None,
        })
    }
}

/// How long `deck_export` waits for an export before it answers that the export is still
/// running: under the minute a client commonly gives a tool call (Claude Code's limit, and
/// the TypeScript SDK's default), with room to answer.
pub const EXPORT_WAIT: Duration = Duration::from_secs(40);

/// An export the server runs beyond the call that asked for it.
#[derive(Debug)]
struct Export {
    /// What was asked: the arguments, and the deck and theme they were asked of.
    asked: u64,
    format: String,
    out: String,
    started: Instant,
    progress: Arc<Progress>,
    /// What it wrote, or why it stopped, once it is done.
    done: watch::Receiver<Option<Result<Exported, String>>>,
}

impl Export {
    /// How far it has got, and what to do while it runs.
    fn running(&self) -> Running {
        let (done, of) = self.progress.get();
        Running {
            done,
            of,
            unit: scaena_ops::export::unit(&self.format).into(),
            elapsed_ms: self.started.elapsed().as_millis() as u64,
            next: "It is still being written. Call deck_export again with the same arguments to wait for the rest; \
                   it then returns what it wrote."
                .into(),
        }
    }
}

/// What a call to `deck_export` asks: its arguments, and the deck and theme it reads.
fn asked(a: &DeckExport, b: &scaena_ops::Bundle) -> u64 {
    let mut hasher = DefaultHasher::new();
    (&a.bundle, &a.format, &a.states, &a.out, &a.size, &a.fps, &a.audio, &a.painter).hash(&mut hasher);
    serde_json::to_string(&b.deck).unwrap_or_default().hash(&mut hasher);
    b.theme_json.hash(&mut hasher);
    hasher.finish()
}

/// The formats JSON Schema defines (2020-12, §7.3).
const FORMATS: &[&str] = &[
    "date-time",
    "date",
    "time",
    "duration",
    "email",
    "idn-email",
    "hostname",
    "idn-hostname",
    "ipv4",
    "ipv6",
    "uri",
    "uri-reference",
    "iri",
    "iri-reference",
    "uuid",
    "uri-template",
    "json-pointer",
    "relative-json-pointer",
    "regex",
];

/// A tool's schema with only the formats JSON Schema defines. schemars also marks a number
/// with the width Rust gives it (`uint32`, `double`, …), which a client's validator does not
/// know and warns of; the type and its `minimum` say what the width meant.
fn standard(schema: &Map<String, Value>) -> Arc<Map<String, Value>> {
    fn object(o: &mut Map<String, Value>) {
        if o.get("format").and_then(Value::as_str).is_some_and(|f| !FORMATS.contains(&f)) {
            o.shift_remove("format");
        }
        // Values, not schemas: what a `default` or an `enum` holds is left as it is.
        for (key, v) in o.iter_mut() {
            if !matches!(key.as_str(), "default" | "examples" | "const" | "enum") {
                value(v);
            }
        }
    }
    fn value(v: &mut Value) {
        match v {
            Value::Object(o) => object(o),
            Value::Array(a) => a.iter_mut().for_each(value),
            _ => {}
        }
    }
    let mut schema = schema.clone();
    object(&mut schema);
    Arc::new(schema)
}

/// Who the client is, as the history of a bundle it edits names it: `agent:<name>`, by the
/// name it gives (SPEC §8.2). A client on protocol 2026-07-28 gives it with every request;
/// an older one, once, when it connects.
fn author(context: &RequestContext<RoleServer>) -> String {
    context.client_info().map_or_else(|| "agent".into(), |client| format!("agent:{}", client.name))
}

/// Serve on stdin and stdout until the client goes.
pub fn stdio() -> std::io::Result<()> {
    serve_stdio(Scaena::default())
}

/// [`stdio`], `map_fetch`'s requests made by `transport`: the CLI's (ADR-0030).
pub fn stdio_with(transport: Arc<dyn scaena_objects::Transport + Send + Sync>) -> std::io::Result<()> {
    serve_stdio(Scaena::default().with_transport(transport))
}

fn serve_stdio(scaena: Scaena) -> std::io::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(async {
        let server = scaena.serve(rmcp::transport::stdio()).await.map_err(std::io::Error::other)?;
        server.waiting().await.map_err(std::io::Error::other)?;
        Ok(())
    })
}

/// Every tool, as `tools/list` gives it: what `docs/schema/mcp/` holds.
pub fn tools() -> Vec<Tool> {
    Scaena::default().tool_router.list_all()
}

/// A tool's failure: the text of its error result.
#[derive(Debug, Serialize)]
struct Failure {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    op: Option<usize>,
}

fn failure(e: OpsError) -> String {
    let OpsError { message, plan, op } = e;
    serde_json::to_string(&Failure { message, plan, op }).unwrap_or_default()
}

/// Waits until an export has counted what it writes, for [`COUNTING`] at most; whether it
/// finished meanwhile.
async fn counted(progress: &Progress, done: &watch::Receiver<Option<Result<Exported, String>>>) -> bool {
    let until = Instant::now() + COUNTING;
    while progress.get().1 == 0 && done.borrow().is_none() && Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    done.borrow().is_some()
}

/// How long `deck_export` waits for an export to count what it writes before it says how far
/// it has got. An export counts its states or pages as it starts, and a video its frames once
/// it has its timeline, in well under this.
const COUNTING: Duration = Duration::from_secs(10);

/// `f`, off the async runtime: the operations lay out and paint, and block.
/// Starts the export `req` of `b` on a thread of its own, which runs to the end whoever is
/// still waiting for it.
fn start(b: scaena_ops::Bundle, req: scaena_ops::export::Request, a: &DeckExport, asked: u64) -> Export {
    let progress = Arc::new(Progress::default());
    let (tell, done) = watch::channel(None);
    let watched = progress.clone();
    tokio::task::spawn_blocking(move || {
        let result = scaena_ops::export::export_watched(&b, &req, &watched).map_err(failure);
        let _ = tell.send(Some(result));
    });
    Export {
        asked,
        format: a.format.clone(),
        out: a.out.clone().unwrap_or_default(),
        started: Instant::now(),
        progress,
        done,
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, OpsError> + Send + 'static) -> Result<T, String> {
    match tokio::task::spawn_blocking(f).await {
        Ok(result) => result.map_err(failure),
        Err(e) => Err(failure(OpsError::new(format!("the operation stopped: {e}")))),
    }
}

fn open(bundle: &str) -> Result<scaena_ops::Bundle, OpsError> {
    scaena_ops::open(Path::new(bundle))
}

/// The bundle, edited by `author`: what it writes is theirs in its history.
fn open_by(bundle: &str, author: String) -> Result<scaena_ops::Bundle, OpsError> {
    let mut b = open(bundle)?;
    b.author = author;
    Ok(b)
}

// --- inputs ---------------------------------------------------------------------------

/// A bundle: a directory, a `.scaena` zip, or a deck file.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BundleArg {
    /// The bundle's path: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckCreate {
    /// Where the bundle goes: a directory that is not there yet, or is empty.
    pub bundle: String,
    /// The theme to start from: one that ships, by its name (`dusk`, `daybreak`, `ember`, or `salon`),
    /// which comes with its fonts; or a theme file, copied in with the fonts its families
    /// name (from beside it, or above it).
    pub theme: String,
    /// The deck, as `deck.json` holds it (`scaena://schema/deck`). Its `theme` and `fonts`
    /// are set to the bundle's. Without it and `scn`, one state with nothing on screen.
    #[serde(default)]
    pub deck: Option<Map<String, Value>>,
    /// The deck as `.scn` source (SPEC §4), instead of `deck`.
    #[serde(default)]
    pub scn: Option<String>,
    /// The title of a deck made here.
    #[serde(default)]
    pub title: Option<String>,
    /// Data files to attach, as `data_attach` attaches them.
    #[serde(default)]
    pub data: Vec<scaena_ops::create::Attach>,
    /// Keep a history (`history/deck.loro`, SPEC §8), as the editors' Keep a History does: the
    /// deck made is its first version, every write after is recorded by its author, and
    /// `deck_history` lists, compares, and restores them. On unless `false`.
    #[serde(default = "yes")]
    pub history: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckRead {
    /// The bundle: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
    /// The deck as `.scn` source rather than JSON.
    #[serde(default)]
    pub scn: bool,
    /// The bundle's images, fonts, and data too, each with what in the deck names it and the
    /// nodes drawn from it in the states that show them so; a file nothing names has no
    /// `named`, and may be taken out (`scaena files --remove`).
    #[serde(default)]
    pub files: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckPatch {
    /// The bundle: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
    /// The ops, in order: JSON Patch (RFC 6902) and semantic ops (SPEC §7.3,
    /// `scaena://schema/patch`). All apply, or none.
    pub ops: Vec<Map<String, Value>>,
    /// Say what would change, and write nothing.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IconFind {
    /// The words to find: each begins a word of an icon's name, its tags, or its categories.
    /// None finds every icon.
    #[serde(default)]
    pub query: String,
    /// `regular`, drawn in outline (the default), or `fill`, drawn solid.
    #[serde(default)]
    pub weight: scaena_ops::icons::Weight,
    /// At most this many; 20 when not given.
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckFind {
    /// The bundle: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
    /// The characters sought, as typed.
    pub find: String,
    /// Upper and lower case apart; alike without it.
    #[serde(default)]
    pub case: bool,
    /// Whole words only: a match that neither begins nor ends inside a word.
    #[serde(default)]
    pub words: bool,
    /// Replace every match with this, in one patch: a `replace_text` where each text lives.
    #[serde(default)]
    pub replace: Option<String>,
    /// With `replace`: say what would change, and write nothing.
    #[serde(default)]
    pub dry_run: bool,
}

/// The least severity to report.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SeverityArg {
    Error,
    Warning,
    #[default]
    Info,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckLint {
    /// The bundle: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
    /// Only the findings in this state.
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub severity: SeverityArg,
    /// Apply the fixes lint offers (each checked by laying its state out again; never a
    /// change of content), write the deck, and lint again.
    #[serde(default)]
    pub fix: bool,
    /// The moment to judge dates at (W428: a forecast that has passed): an instant, as
    /// 2026-10-15T20:00:00Z, or a day, as 2026-10-15. Without it, no date is judged.
    #[serde(default)]
    pub now: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckInspect {
    /// The bundle: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
    /// One state; every state without it.
    #[serde(default)]
    pub state: Option<String>,
    #[serde(flatten)]
    pub views: scaena_ops::inspect::Views,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckRender {
    /// The bundle: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
    pub state: String,
    /// Ms into the state's cue (its transition, then its motions); at rest without it.
    #[serde(default)]
    pub t: Option<f64>,
    /// One of the deck's `formats` (`9:16`), laid out again with its template set.
    #[serde(default)]
    pub format: Option<String>,
    /// `WxH` pixels, with the canvas's aspect ratio; half the canvas's size without it.
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub painter: scaena_ops::render::Painter,
    /// Also write the PNG here: under the server's working directory or the bundle's folder.
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckExport {
    /// The bundle: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
    /// `spine`, `pdf`, `png`, `svg`, `mp4`, `webm`, `prores`, or `html`.
    pub format: String,
    /// The states a frame export draws, in this order: an image each (png, svg), a page
    /// each (pdf), each one's part of the timeline (video), or the states it plays (html).
    /// Without it: every state; a PDF's slides, each at its last state, in spine order; a
    /// video's whole timeline.
    #[serde(default)]
    pub states: Option<Vec<String>>,
    /// Where to write it: a file (pdf, video, html, spine), or a directory that gets an
    /// image per state (png, svg), under the server's working directory or the bundle's
    /// folder. Only the spine needs none.
    #[serde(default)]
    pub out: Option<String>,
    /// `WxH` pixels for png, svg, and video, in the canvas's aspect ratio; the canvas's
    /// size without it.
    #[serde(default)]
    pub size: Option<String>,
    /// A video's frames a second; 60 without it.
    #[serde(default)]
    pub fps: Option<u32>,
    /// A video's sound track, any file ffmpeg reads, from the first frame: cut where the
    /// video ends, or carried on in silence until it does.
    #[serde(default)]
    pub audio: Option<String>,
    /// What paints a video's frames: `gpu` is vello on the GPU, in a server built with the
    /// `gpu` feature. Every other export paints with the CPU painter.
    #[serde(default)]
    pub painter: scaena_ops::render::Painter,
    /// How a video and the spine pace the timeline: `beats` (the default) gives each state
    /// with no `hold` its share of its beat's `duration`, as the talk is given; `holds` plays
    /// each state's hold alone, as a player auto-advances.
    #[serde(default)]
    pub pace: scaena_core::spine::Pace,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckDiff {
    /// The bundle: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThemeApply {
    /// The bundle: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
    /// The theme file; one outside the bundle is copied to `themes/`. Where no such file is, a
    /// theme that ships, by its name (`dusk`, `daybreak`, `ember`, or `salon`), copied in with its
    /// fonts.
    pub theme: String,
    #[serde(default)]
    pub dry_run: bool,
    /// Apply a theme that leaves the deck invalid. Without it, the deck keeps its theme, and
    /// the new one is copied in for a `deck_patch` with the `retheme` op and the fixes.
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThemeEdit {
    /// The bundle: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
    #[serde(flatten)]
    pub edit: scaena_ops::theme::ThemeEdit,
    /// Say what would change, and write nothing.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataAttach {
    /// The bundle: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
    #[serde(flatten)]
    pub data: scaena_ops::create::Attach,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataEdit {
    /// The bundle: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
    #[serde(flatten)]
    pub data: scaena_ops::data::DataEdit,
    /// Say what would change, and write nothing.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckHistory {
    /// The bundle: a directory, a `.scaena` zip, or a `deck.json`. Its history is
    /// `history/deck.loro`, which `begin` here, `deck_create`, or `scaena save --history` begins.
    pub bundle: String,
    #[serde(flatten)]
    pub ask: scaena_ops::history::Ask,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpineUpdate {
    /// The bundle: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
    /// The spine: its sections, each with its beats (SPEC §3.11, `scaena://schema/deck`'s
    /// `Spine`).
    pub spine: Map<String, Value>,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StateStart {
    /// The bundle: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
    /// The state the slide goes after: after the last step of its slide, in its beat.
    pub state: String,
    /// One of the theme's layouts (`deck_inspect` with `layouts` lists what a state may take,
    /// each by its name; `bundle://theme`'s `layouts` holds them all). Without it, a blank slide.
    #[serde(default)]
    pub layout: Option<String>,
    /// Say what would change, and write nothing.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BackgroundSet {
    /// The bundle: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
    /// A state of the slide: the background enters in the slide's first state and stays on it.
    pub state: String,
    /// One of the theme's shader presets (`shaders.presets`: `backdrop`, `aurora`, `texture`,
    /// `haze`, `lagoon`, `sweep`, `dust` in the themes that ship). Without it, the slide's
    /// background is taken away.
    #[serde(default)]
    pub preset: Option<String>,
    /// Say what would change, and write nothing.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckGenerate {
    /// Where the bundle goes: a directory that is not there yet, or is empty.
    pub bundle: String,
    /// The slides, as Narrato writes them: `executiveSummary`, and `slides`, each with
    /// `slideTitle`, `subtitle`, `layoutSuggestion` (`Title`, `Bullet List`, `Big Number`,
    /// `Chart`, `Two Column`, `Quote`, `Section Divider`, `Closing`, …), `keyBullets`,
    /// `speakerNotes`, `beatIndex` and `beatName` (the arc's beat it belongs to), `intensity`
    /// (0–1), `tone` (−1–1), and `visualDescription`; `chartData` where a chart is drawn.
    pub slides: Map<String, Value>,
    /// The theme to make it in: one that ships, by name (`dusk`, `daybreak`, `ember`, `salon`),
    /// or a theme file. `dusk` without it.
    #[serde(default)]
    pub theme: Option<String>,
    /// What the deck is about, which health reads back; the executive summary without it.
    #[serde(default)]
    pub topic: Option<String>,
    /// Who it is for ("Investors", "a school board").
    #[serde(default)]
    pub audience: Option<String>,
    /// `Inform`, `Persuade`, `Inspire`, `Sell`, `Educate`, or `Update Status`.
    #[serde(default)]
    pub goal: Option<String>,
    /// `5-10 slides`, `11-15 slides`, `16-25 slides`, `26-40 slides`, or `41+ slides`.
    #[serde(default)]
    pub length: Option<String>,
    /// The language the slides are written in, by its English name; English without it.
    #[serde(default)]
    pub language: Option<String>,
    /// One of Narrato's arcs by name (`Problem-Solution`, `Hero's Journey`, …); without it,
    /// chosen from the brief.
    #[serde(default)]
    pub arc: Option<String>,
    /// Keep a history, as `deck_create` does: on unless `false`.
    #[serde(default = "yes")]
    pub history: bool,
}

// --- outputs --------------------------------------------------------------------------

#[derive(Debug, Serialize, JsonSchema)]
pub struct Linted {
    /// What lint finds, errors first (SPEC §7.4).
    pub findings: Vec<Finding>,
    /// The findings whose fixes were applied, under `fix`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixed: Option<Vec<Finding>>,
    /// The findings that are errors.
    pub errors: usize,
    /// Whether the layout rules ran: they run once validation and the document rules
    /// find no error.
    pub laid: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Inspected {
    pub states: Vec<scaena_ops::inspect::Inspected>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Diffed {
    /// By node: it enters, exits, or changes these props.
    pub changes: IndexMap<String, scaena_ops::inspect::Change>,
}

/// A rendered frame's facts; the PNG is the result's image.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Rendered {
    pub state: String,
    /// Pixels: width, height.
    pub size: [u32; 2],
    /// The state's span: its transition and motions, ms.
    pub span_ms: f64,
    /// The display list's digest: one digest, one drawing.
    pub digest: String,
    pub painter: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out: Option<String>,
    pub ms: scaena_ops::render::Timings,
}

/// A deck made from written slides (PLAN G.2): the bundle, as `deck_create` reports one, and
/// how it was built.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DeckGenerated {
    pub created: scaena_ops::create::Created,
    /// The arc the deck follows.
    pub arc: String,
    /// Why the heuristic chose the arc, where none was named.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
    /// Each slide's layout, in order.
    pub layouts: Vec<String>,
    /// The findings whose fixes were applied, each checked by laying its state out again.
    pub fixed: Vec<Finding>,
}

// --- tools ----------------------------------------------------------------------------

#[tool_router]
impl Scaena {
    #[tool(description = "Make a bundle: a theme and its fonts, data files, and a deck pointed at them \
        (or one state with nothing on screen). The theme is one that ships, by name (dusk, daybreak, ember, salon), or a \
        theme file. Checked as `deck_lint` checks a bundle, and written only if it validates. It keeps a history \
        unless `history` is false: the deck made is its first version, and every write after is recorded \
        (`deck_history`).")]
    async fn deck_create(
        &self,
        Parameters(a): Parameters<DeckCreate>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<scaena_ops::create::Created>, String> {
        let author = author(&context);
        // A new bundle has no folder yet: the theme is read under this server's own.
        let theme = self.theme_readable(&a.theme, &a.bundle).map_err(failure)?;
        let req = scaena_ops::create::Create {
            theme,
            deck: a.deck.map(Value::Object),
            scn: a.scn,
            title: a.title,
            data: a.data,
            ..Default::default()
        };
        blocking(move || {
            let created = scaena_ops::create::create(Path::new(&a.bundle), &req)?;
            if created.created && a.history {
                scaena_ops::history::begin(&open_by(&a.bundle, author)?, "deck_create")?;
            }
            Ok(created)
        })
        .await
        .map(Json)
    }

    #[tool(description = "The bundle's deck, as `deck.json` holds it, or as `.scn` source; with `files`, the \
        bundle's images, fonts, and data too, and what uses each.")]
    async fn deck_read(&self, Parameters(a): Parameters<DeckRead>) -> Result<Json<scaena_ops::read::Read>, String> {
        blocking(move || scaena_ops::read::read(&open(&a.bundle)?, a.scn, a.files)).await.map(Json)
    }

    #[tool(description = "Apply a patch: JSON Patch and semantic ops (add_node, rename_node, set_prop, set_text, \
        show_node, hide_node, add_state, …; SPEC §7.3), all or none. A patch that would make the deck invalid is refused. \
        Reports the patch as JSON Patch, and what lint finds that it did not (`added`) and finds no more (`removed`).")]
    async fn deck_patch(
        &self,
        Parameters(a): Parameters<DeckPatch>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<scaena_ops::patch::Patched>, String> {
        let ops = Value::Array(a.ops.into_iter().map(Value::Object).collect());
        let author = author(&context);
        blocking(move || scaena_ops::patch::patch(&open_by(&a.bundle, author)?, &ops, a.dry_run)).await.map(Json)
    }

    #[tool(description = "Find text across the deck's words, in every state (PLAN 2.47, 2.83): its texts, each \
        node's description (`alt`), each state's notes, and each beat's claim and notes (`kind`), once for each \
        place they are written (the node's own, a state's delta, the deck's overrides, a state, or a beat), with the \
        states that show them from there and each match, in characters. With `replace`, every match is replaced in \
        one patch, a `replace_text` where each text lives and a JSON Patch `replace` of the rest where they are \
        written, reported as `deck_patch` reports a patch (`replaced`).")]
    async fn deck_find(
        &self,
        Parameters(a): Parameters<DeckFind>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<scaena_ops::find::Searched>, String> {
        let query = Query { find: a.find, case: a.case, words: a.words };
        let author = author(&context);
        blocking(move || {
            scaena_ops::find::search(&open_by(&a.bundle, author)?, &query, a.replace.as_deref(), a.dry_run)
        })
        .await
        .map(Json)
    }

    #[tool(description = "Lint the bundle (SPEC §7.5): validation, the document rules, then layout, contrast, \
        motion, and narrative in every format it lists. Findings carry a JSON pointer, and a fix when one is safe; \
        `fix` applies them.")]
    async fn deck_lint(
        &self,
        Parameters(a): Parameters<DeckLint>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<Linted>, String> {
        let min = match a.severity {
            SeverityArg::Error => scaena_core::Severity::Error,
            SeverityArg::Warning => scaena_core::Severity::Warning,
            SeverityArg::Info => scaena_core::Severity::Info,
        };
        let author = author(&context);
        blocking(move || {
            let now = match a.now.as_deref() {
                Some(now) => Some(scaena_core::fetched::moment(now).ok_or_else(|| {
                    OpsError::new(format!(
                        "`now` `{now}` is not an instant or a day: write 2026-10-15T20:00:00Z or 2026-10-15"
                    ))
                })?),
                None => None,
            };
            let b = open_by(&a.bundle, author)?;
            let (findings, fixed, laid) = match a.fix {
                true => {
                    let f = scaena_ops::lint::lint_fix(&b)?;
                    (f.findings, Some(f.fixed), true)
                }
                false => {
                    let l = scaena_ops::lint::lint_at(&b, now)?;
                    (l.findings, None, l.laid)
                }
            };
            let findings: Vec<Finding> = findings
                .into_iter()
                .filter(|f| f.severity >= min && a.state.as_ref().is_none_or(|s| f.state.as_deref() == Some(s)))
                .collect();
            let errors = scaena_ops::lint::errors(&findings);
            Ok(Linted { findings, fixed, errors, laid })
        })
        .await
        .map(Json)
    }

    #[tool(description = "Each state as it resolves (SPEC §2.2): its nodes with their props, what enters and exits. \
        `resolved` adds each text node's look through the theme, `timeline` each motion as placed on the state's \
        clock, `data` the rows each chart and table reads. With `state`: `layouts` the layouts the state may take, \
        best first, each with the `set_state` that gives it; `state_choices` what an inspector offers for the state \
        itself (its layout, transition, hold, notes, and slide number and footer); `choices` (a node) what the theme \
        offers for each of its properties and where each lives, which `choose` writes; `inserts` what may be \
        inserted, each as `add_node` adds it; `layers` the nodes as the editor's layers list them; `boxes` where \
        each node stands and `at` what draws at a point; `targets` (a node) where it may go, with `snap` and `to` \
        the `place` that puts it there; `look` (a node) its look and, with `onto`, the patch that pastes it on \
        others; `arrange` (nodes) with `align`, `spread`, `order`, `before`, `after`, `into`, or `by` the patch \
        that arranges them.")]
    async fn deck_inspect(&self, Parameters(a): Parameters<DeckInspect>) -> Result<Json<Inspected>, String> {
        blocking(move || scaena_ops::inspect::inspect(&open(&a.bundle)?, a.state.as_deref(), a.views))
            .await
            .map(|states| Json(Inspected { states }))
    }

    #[tool(description = "Render a state to a PNG, returned as an image: at rest, or `t` ms into its cue. \
        At half the canvas's size (960 \u{d7} 540 for a 1920 \u{d7} 1080 deck) unless `size` asks for another. \
        With the display list's digest: one digest, one drawing.")]
    async fn deck_render(&self, Parameters(a): Parameters<DeckRender>) -> Result<CallToolResult, String> {
        let req = scaena_ops::render::Request {
            state: a.state.clone(),
            t: a.t,
            format: a.format,
            size: a.size,
            scale: Some(scaena_ops::render::FOR_A_MODEL),
            painter: a.painter,
        };
        let out = a
            .out
            .as_deref()
            .map(|out| self.writable(out, &a.bundle, Wrote::File("png")))
            .transpose()
            .map_err(failure)?;
        let frame = blocking(move || {
            let frame = scaena_ops::render::render(Path::new(&a.bundle), &req)?;
            if let Some(out) = &out {
                std::fs::write(out, &frame.png)
                    .map_err(|e| OpsError::new(format!("writing {}: {e}", out.display())))?;
            }
            Ok(frame)
        })
        .await?;
        let facts = Rendered {
            state: a.state,
            size: frame.size,
            span_ms: frame.span_ms,
            digest: frame.digest,
            painter: frame.painter.to_string(),
            out: a.out,
            ms: frame.ms,
        };
        let png = base64::engine::general_purpose::STANDARD.encode(&frame.png);
        let facts = serde_json::to_string(&facts).map_err(|e| failure(OpsError::new(e.to_string())))?;
        Ok(CallToolResult::success(vec![ContentBlock::image(png, "image/png"), ContentBlock::text(facts)]))
    }

    #[tool(description = "Export a projection to `out`: `pdf` (each slide at its last state, or `states`, a page \
        each), `png` or `svg` (an image per state, into a directory), `mp4`, `webm`, or `prores` (the timeline, \
        each state's cue then its hold, or where it has none its share of its beat's `duration` (`pace`), at \
        `fps`; needs ffmpeg), `html` (one file that plays the deck in a \
        browser, offline: the player, the engine, and the bundle), or `spine`, which is also returned. An export \
        that takes longer than 40 s keeps going: the call returns \
        `running`, how far it has got, and the same call again waits for the rest, then returns what it wrote.")]
    async fn deck_export(&self, Parameters(a): Parameters<DeckExport>) -> Result<Json<Exported>, String> {
        let req = scaena_ops::export::Request {
            format: a.format.clone(),
            states: a.states.clone(),
            out: match (a.out.as_deref(), Wrote::exported(&a.format)) {
                (Some(out), Some(wrote)) => Some(self.writable(out, &a.bundle, wrote).map_err(failure)?),
                // No format of its own: the export refuses it before anything is written.
                (_, None) => None,
                (None, Some(_)) => None,
            },
            size: a.size.clone(),
            fps: a.fps,
            audio: a.audio.clone().map(Into::into),
            painter: a.painter,
            pace: a.pace,
        };
        let bundle = a.bundle.clone();
        let b = blocking(move || open(&bundle)).await?;
        // The spine without `out` comes back in the result: there is nothing to wait for.
        let Some(out) = req.out.clone() else {
            return blocking(move || scaena_ops::export::export(&b, &req)).await.map(Json);
        };
        let file = std::path::absolute(&out).unwrap_or(out);
        let asked = asked(&a, &b);
        let (mut done, progress) = {
            let mut exports = self.exports.lock().unwrap_or_else(|e| e.into_inner());
            match exports.get(&file) {
                // Asked again: wait for it.
                Some(export) if export.asked == asked => (export.done.clone(), export.progress.clone()),
                Some(export) if export.done.borrow().is_none() => {
                    let Running { done, of, unit, elapsed_ms, .. } = export.running();
                    return Err(failure(OpsError::new(format!(
                        "an export to {} is running, asked with other arguments or of another version of the deck: \
                         {done} of {of} {unit} after {} s. Export to another file, or again when it is done.",
                        export.out,
                        elapsed_ms / 1000
                    ))));
                }
                // Not running, or done and never handed back: start it.
                _ => {
                    let export = start(b, req, &a, asked);
                    let started = (export.done.clone(), export.progress.clone());
                    exports.insert(file.clone(), export);
                    started
                }
            }
        };
        let waited = tokio::time::timeout(self.wait, done.wait_for(Option::is_some)).await.map(|r| r.is_ok());
        let finished = match waited {
            Ok(true) => true,
            Ok(false) => return Err(failure(OpsError::new("the export stopped before it was done"))),
            // Still running. How far it has got means nothing until it has counted what it
            // writes, which takes it a moment: wait for that, or for the end.
            Err(_) => counted(&progress, &done).await,
        };
        let mut exports = self.exports.lock().unwrap_or_else(|e| e.into_inner());
        let result = done.borrow().clone();
        match result.filter(|_| finished) {
            Some(result) => {
                // Handed back: the next call for this file starts again.
                if exports.get(&file).is_some_and(|e| e.asked == asked) {
                    exports.remove(&file);
                }
                result.map(Json)
            }
            None => {
                let running = exports.get(&file).map(Export::running);
                Ok(Json(Exported { format: a.format, out: a.out, running, ..Exported::default() }))
            }
        }
    }

    #[tool(
        description = "What changes between two states, resolved: by node, it enters, exits, or changes these props."
    )]
    async fn deck_diff(&self, Parameters(a): Parameters<DeckDiff>) -> Result<Json<Diffed>, String> {
        blocking(move || scaena_ops::inspect::diff(&open(&a.bundle)?, &a.from, &a.to))
            .await
            .map(|changes| Json(Diffed { changes }))
    }

    #[tool(description = "Re-theme: point the deck at another theme, a file or one that ships by name (dusk, \
        daybreak, ember, salon), copied into the bundle with its fonts, and say what that changes in what lint finds. A text the theme leaves too faint to read where it stands is set in the \
        theme's color that reads there, as its contrast error's fix offers (`recolored`); the deck is not otherwise \
        touched. A theme that \
        would leave the deck invalid, as one that lacks a name it uses does, is refused (`refused`, and its \
        errors in `added`) unless `force`: the deck keeps its theme, and the new one is copied in, so one \
        `deck_patch` with the `retheme` op and the fixes swaps it with the deck valid throughout.")]
    async fn theme_apply(
        &self,
        Parameters(a): Parameters<ThemeApply>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<scaena_ops::theme::Themed>, String> {
        let author = author(&context);
        let theme = self.theme_readable(&a.theme, &a.bundle).map_err(failure)?;
        blocking(move || scaena_ops::theme::theme_apply(&open_by(&a.bundle, author)?, &theme, a.dry_run, a.force))
            .await
            .map(Json)
    }

    #[tool(description = "Edit the theme the deck names (ADR-0016): its colors, type roles, spacing, and the rest, \
        by RFC 6902 operations on its JSON (`ops`, each path a JSON Pointer into the theme: \
        `/tokens/color/accent`, `/type/roles/body/size`, `/grid/gutter`). Every node that names what the edit \
        changes changes with it, so a look asked of the whole deck is an edit here, not a literal in each node. \
        Checked as a re-theme is: an edit that would leave the deck invalid, as one that takes out a name it uses \
        does, is refused (`refused`, and its errors in `added`), and nothing is written. Otherwise the theme file \
        is written in canonical form (an inline theme, in the deck), and the delta in what lint finds is \
        reported. A theme that ships is edited in the bundle's copy. Or, in place of `ops`, a `photo`: an image \
        the bundle holds, whose colors the theme's take (PLAN 2.94). Its best hue goes to the accent, and the best \
        far enough from that to the next chromatic color; the hue it leans to tints the neutrals. Each color keeps \
        the theme's lightness and chroma, and each color text is set in moves until it reads on the surfaces. \
        `photo` in the result says what the photo's colors are and what each of the theme's became.")]
    async fn theme_edit(
        &self,
        Parameters(a): Parameters<ThemeEdit>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<scaena_ops::theme::ThemeEdited>, String> {
        let author = author(&context);
        blocking(move || scaena_ops::theme::theme_edit(&open_by(&a.bundle, author)?, &a.edit, a.dry_run))
            .await
            .map(Json)
    }

    #[tool(description = "Attach a CSV or JSON data file: copy it into the bundle's `data/` and declare it as a \
        source charts and tables name as `@id`, each column typed (inferred from its values without `schema`).")]
    async fn data_attach(
        &self,
        Parameters(a): Parameters<DataAttach>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<scaena_ops::create::Attached>, String> {
        let author = author(&context);
        let mut data = a.data;
        data.file = self.readable(&data.file, &a.bundle).map_err(failure)?;
        blocking(move || scaena_ops::create::attach(&open_by(&a.bundle, author)?, &data)).await.map(Json)
    }

    #[tool(description = "Put a map on a slide (ADR-0030): a picture of `place` (an address, a place, or `lat, lon`) \
        fetched once from Geoapify with the user's own key ($GEOAPIFY_API_KEY), kept in the bundle, and drawn with a \
        pin, its label, and the credit Geoapify's terms ask. It sends the place's words to Geoapify's geocoder \
        (api.geoapify.com) and the map's middle, size, and zoom to its static map (maps.geoapify.com), under \
        Geoapify's terms; coordinates skip the geocoder. It goes in the image `into`, else the slide's first empty \
        place for a picture, else a new image at `at`, in one patch by its author, refused as a patch is. A name that \
        matches several places answers them in `places`: ask again with `pick`, from 1. `refresh`, an image's id, \
        fetches its map again, the same place and view or at `zoom` or `center`, one patch, both pictures kept.")]
    async fn map_fetch(
        &self,
        Parameters(a): Parameters<maps::MapFetch>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<maps::MapFetched>, String> {
        let author = author(&context);
        let reach = self.reach.0.clone();
        blocking(move || {
            let reach = reach.ok_or_else(|| OpsError::new("this server reaches no service outside this machine"))?;
            maps::fetch(&open_by(&a.bundle, author)?, &a, reach.as_ref())
        })
        .await
        .map(Json)
    }

    #[tool(description = "A data source's rows (SPEC §3.10). With no `edits`, the source as a sheet: its columns, \
        each one's type, its rows as written, by index from 0, and the cells a column does not read. With them, \
        cells set (`set`), rows added (`add`, at `row` or the end) and taken away (`remove`), in order, all or none: \
        one write of the source's file that keeps every other byte, or a patch of rows written inline, and what \
        that changes in what lint finds. A value its column's type refuses stops them, by index (`op`); edits that \
        would leave the deck invalid are refused (`refused`, and why in `added`). Every chart and table that reads \
        the source reads it changed.")]
    async fn data_edit(
        &self,
        Parameters(a): Parameters<DataEdit>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<scaena_ops::data::DataEdited>, String> {
        let author = author(&context);
        blocking(move || scaena_ops::data::data_edit(&open_by(&a.bundle, author)?, &a.data, a.dry_run)).await.map(Json)
    }

    #[tool(description = "A bundle's versions (SPEC §8): each change its history keeps, by author and time, \
        oldest first, each named by its number as listed or by its id, and told as the editors tell it: what it \
        did (`said`), by whom (`by`), and what it changed from the version before, slide by slide, each object by \
        what it shows (`changed`). With `begin`, a history begun where the bundle keeps none, the deck as it is \
        its first version. With `at`, the deck as it was just after \
        one (`scn` for source); with `compare`, what changed from one version to another, or, with one, to the \
        deck as it is now: each state added, removed, or changed (its own fields, and its nodes as `deck_diff` \
        says them), the deck's own fields, and the files it is drawn from, data files and the theme, whose bytes \
        changed, each with what it was before (`before`); with `restore`, that version made the deck again, with its data files and its theme as they \
        were: one change, refused as a patch is where the deck would not validate in the bundle as it is \
        (`dry_run` writes nothing).")]
    async fn deck_history(
        &self,
        Parameters(a): Parameters<DeckHistory>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<scaena_ops::history::History>, String> {
        let author = author(&context);
        blocking(move || scaena_ops::history::history(&open_by(&a.bundle, author)?, &a.ask)).await.map(Json)
    }

    #[tool(description = "The deck's spine (SPEC §2.6, §10): its sections and beats, each beat's claim, evidence, \
        and states, and the state that shows it. `deck_export` with `spine` also places it on the timeline and \
        draws each beat.")]
    async fn spine_read(
        &self,
        Parameters(a): Parameters<BundleArg>,
    ) -> Result<Json<scaena_core::spine::SpineProjection>, String> {
        blocking(move || Ok(scaena_ops::read::spine(&open(&a.bundle)?))).await.map(Json)
    }

    #[tool(description = "The deck's narrative health (PLAN G.9): Vzestup's measures of a talk, over its slides as \
        its spine tells them, each 0\u{2013}100 with what it says: pacing against the length its brief asked, the points \
        each slide makes and what its notes give the speaker, how each slide follows from the one before (by their words, \
        and so unscored), how the intensity each declares follows the curve of the arc it was written to, and whether its \
        words carry the intensity and the tone it declares. Each slide as read: where its arc's curve stands, what it declares, and what its words \
        read. Findings slide by slide, each with what to ask about it and, where one is safe, a patch for `deck_patch`: \
        a slide's declared intensity or tone set to what its words read, or a moved slide made part of the beat it now \
        stands in.")]
    async fn deck_health(
        &self,
        Parameters(a): Parameters<BundleArg>,
    ) -> Result<Json<scaena_ops::health::Health>, String> {
        blocking(move || scaena_ops::health::health(&open(&a.bundle)?.deck)).await.map(Json)
    }

    #[tool(description = "Find the icons that ship (Phosphor, MIT) by words: each word begins a word of an icon's \
        name, its tags, or its categories, and its name asked whole comes first. Each icon is its path data on a view \
        256 square, which deck_patch's add_node adds as a shape: {\"type\": \"shape\", \"path\": …, \
        \"view\": [0, 0, 256, 256], \"fill\": \"accent\"}, so icons beside each other keep one weight.")]
    async fn icon_find(&self, Parameters(a): Parameters<IconFind>) -> Result<Json<scaena_ops::icons::Icons>, String> {
        let limit = a.limit.unwrap_or(20);
        blocking(move || Ok(scaena_ops::icons::find(&a.query, a.weight, limit))).await.map(Json)
    }

    #[tool(description = "Replace the deck's spine, as a patch: checked, refused if it makes the deck invalid, \
        with what lint finds differently. A beat's `notes` are what a presenter reads for each state it names \
        that has none of its own; its `duration`, in seconds, paces a video and the spine where its states have \
        no `hold`.")]
    async fn spine_update(
        &self,
        Parameters(a): Parameters<SpineUpdate>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<scaena_ops::patch::Patched>, String> {
        let author = author(&context);
        blocking(move || {
            scaena_ops::read::spine_update(&open_by(&a.bundle, author)?, Value::Object(a.spine), a.dry_run)
        })
        .await
        .map(Json)
    }

    #[tool(description = "Start a slide after `state`'s slide, as Add Slide starts one (PLAN 3.30), in one of the \
        theme's layouts: a new state, absolute, in the layout, with a text in each slot that says what goes there, in \
        the slot's role, its words to type over (`replace_text` or `set_text` put yours), a shape in each slot that \
        holds one, and the slots that wait for a picture or a figure left empty; it stays on its slide. One patch, \
        all or none, refused as `deck_patch` refuses one, reported as `deck_patch` reports one, with `id`, the new \
        state's. `deck_inspect` with `layouts` lists the layouts a state may take.")]
    async fn state_start(
        &self,
        Parameters(a): Parameters<StateStart>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<scaena_ops::states::Started>, String> {
        let author = author(&context);
        blocking(move || {
            scaena_ops::states::start(&open_by(&a.bundle, author)?, &a.state, a.layout.as_deref(), a.dry_run)
        })
        .await
        .map(Json)
    }

    #[tool(description = "Set a slide's background (PLAN 2.148): one of the theme's shader presets filling the \
        canvas behind everything on `state`'s slide, entering in the slide's first state and kept to its slide, each \
        shader that filled the canvas before taken away, and each text too faint to read over it set in the theme's \
        color that reads there, as a re-theme sets it. Without `preset`, the slide's background is taken away. One \
        patch, refused as `deck_patch` refuses one, reported as `deck_patch` reports one. Backgrounds stay off a slide \
        that shows a chart or a table (W311): data reads against a plain surface.")]
    async fn background_set(
        &self,
        Parameters(a): Parameters<BackgroundSet>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<scaena_ops::patch::Patched>, String> {
        let author = author(&context);
        blocking(move || {
            let b = open_by(&a.bundle, author)?;
            match a.dry_run {
                true => scaena_ops::backgrounds::backgrounding(&b, &a.state, a.preset.as_deref()).map(|(mut p, _)| {
                    p.applied = false;
                    p
                }),
                false => scaena_ops::backgrounds::background(&b, &a.state, a.preset.as_deref()),
            }
        })
        .await
        .map(Json)
    }

    #[tool(description = "Make a deck from slides written in the JSON Narrato writes them in (`scaena generate \
        --slides`, ADR-0026), with no model called: the spine, a section for each run of slides in one beat of the \
        arc and a beat for each slide, each slide an absolute state in the theme layout its `layoutSuggestion` comes \
        to, its builds, and the fields `deck_health` reads (`media.narrato`), lint's fixes applied. A new bundle at \
        `bundle`, in `theme`, as `deck_create` makes one; a deck in a script the theme's fonts do not draw is \
        refused, since the CLI alone fetches a script's font.")]
    async fn deck_generate(
        &self,
        Parameters(a): Parameters<DeckGenerate>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<DeckGenerated>, String> {
        let author = author(&context);
        let slides = Value::Object(a.slides);
        let summary = slides["executiveSummary"].as_str().unwrap_or_default().to_string();
        let brief = scaena_ops::narrative::Brief {
            topic: a.topic.unwrap_or(summary),
            audience: a.audience.unwrap_or_default(),
            goal: a.goal.unwrap_or_default(),
            length: a.length.unwrap_or_default(),
            language: a.language,
            ..scaena_ops::narrative::Brief::default()
        };
        // A new bundle has no folder yet: the theme is read under this server's own, as
        // deck_create reads one.
        let theme = self.theme_readable(a.theme.as_deref().unwrap_or("dusk"), &a.bundle).map_err(failure)?;
        blocking(move || {
            let made = scaena_ops::narrative::from_slides(
                Path::new(&a.bundle),
                &theme,
                &slides.to_string(),
                brief,
                a.arc.as_deref(),
            )?;
            if made.created.created && a.history {
                scaena_ops::history::begin(&open_by(&a.bundle, author)?, "deck_create")?;
            }
            Ok(DeckGenerated {
                created: made.created,
                arc: made.arc,
                fallback: made.fallback,
                layouts: made.layouts,
                fixed: made.fixed,
            })
        })
        .await
        .map(Json)
    }
}

/// What the server tells a client as it connects: the model in a sentence, the loop, and where
/// to start. The spine and the deck's health come first: a deck is a story, and every edit serves
/// a beat's claim (SPEC §2.6, §7.7).
pub const INSTRUCTIONS: &str = "Scaena decks are states over one scene graph: nodes exist for the whole deck, each \
    state says what changes, and the theme owns type and layout, so a deck names roles, slots, and presets, never \
    pixels. A deck is a story: its spine's beats each carry one claim, and every slide serves one. On a deck that \
    exists, read the spine first (spine_read) and its narrative health (deck_health: pacing, balance, how the \
    slides follow the arc, and a finding on each slide with what to ask of it), then edit toward the claims, and \
    change the spine (spine_update) when the claims are wrong. Make a bundle with deck_create (a theme that ships, \
    dusk, daybreak, ember, or salon, needs no file), attach data with data_attach, edit with deck_patch, check with \
    deck_lint, read the story again with deck_health, and look with deck_render. The resources hold the schemas, the lint \
    catalog, the specification by section (scaena://spec is its index), the skills (procedures to follow: \
    scaena://skills/author-deck first), and examples.";

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Scaena {
    fn get_info(&self) -> ServerConfig {
        let capabilities = ServerCapabilities::builder().enable_tools().enable_resources().build();
        let server = Implementation::new("scaena", env!("CARGO_PKG_VERSION"));
        ServerConfig::new(capabilities).with_server_info(server).with_instructions(INSTRUCTIONS)
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let listed = resources::all()
            .iter()
            .filter(|r| r.listed)
            .map(|r| Resource::new(&r.uri, &r.name).with_mime_type(r.mime).with_size(r.text.len() as u64))
            .collect();
        let mut list = ListResourcesResult::with_all_items(listed);
        if hints(&context) {
            list = list.with_ttl_ms(RESOURCE_TTL_MS).with_cache_scope(CacheScope::Public);
        }
        Ok(list)
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let served = resources::all()
            .iter()
            .find(|r| r.uri == request.uri)
            .ok_or_else(|| ErrorData::resource_not_found(format!("no resource `{}`", request.uri), None))?;
        let contents = ResourceContents::TextResourceContents {
            uri: request.uri,
            mime_type: Some(served.mime.into()),
            text: served.text.clone(),
            meta: None,
        };
        let mut read = ReadResourceResult::new(vec![contents]);
        if hints(&context) {
            read = read.with_ttl_ms(RESOURCE_TTL_MS).with_cache_scope(CacheScope::Public);
        }
        Ok(read.into())
    }
}

/// How long a client may keep what `resources/list` and `resources/read` return. The
/// resources are built into the server, the same for everyone, and never change while it
/// runs; an hour bounds how stale a client's copy can be across a rebuild.
const RESOURCE_TTL_MS: u64 = 3_600_000;

/// Whether the client negotiated a protocol (2026-07-28 on) whose list and read results
/// carry cache hints: there `ttlMs` and `cacheScope` are required, and a client rejects a
/// result without them (SEP-2549). Older clients get the shape they know, as rmcp's own
/// `tools/list` does.
fn hints(context: &RequestContext<RoleServer>) -> bool {
    context.protocol_version().is_some_and(|v| v >= ProtocolVersion::V_2026_07_28)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own under the system's temporary folder, empty.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("scaena-mcp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_disks_root_and_the_home_folder_are_no_folder_of_a_servers_own() {
        let home = scratch("home");
        let nowhere = home.join("no-bundle");
        let at = |dir: &Path| Scaena { dir: Some(dir.to_path_buf()), home: Some(home.clone()), ..Scaena::default() };
        // Started at the disk's root, or at home, with no bundle beside anything: nothing is written.
        for dir in [Path::new("/"), home.as_path()] {
            let err = at(dir).within(&path(&home.join(".ssh/authorized_keys")), &path(&nowhere), "writes").unwrap_err();
            assert!(err.message.contains("keep the bundle in a folder of its own"), "{}", err.message);
            let err = at(dir).within("notes/frame.png", &path(&nowhere), "writes").unwrap_err();
            assert!(err.message.contains("keep the bundle in a folder of its own"), "{}", err.message);
        }
        // A bundle kept straight in the home folder makes it no root either.
        let bundle = home.join("deck.json");
        std::fs::write(&bundle, "{}").unwrap();
        let err = at(Path::new("/")).within(&path(&home.join("frame.png")), &path(&bundle), "writes").unwrap_err();
        assert!(err.message.contains("keep the bundle in a folder of its own"), "{}", err.message);
        // In a folder of its own, under home, it is one.
        let own = home.join("talks");
        std::fs::create_dir_all(&own).unwrap();
        let bundle = own.join("deck.json");
        std::fs::write(&bundle, "{}").unwrap();
        let ok = at(Path::new("/")).within(&path(&own.join("frame.png")), &path(&bundle), "writes").unwrap();
        assert_eq!(ok, std::fs::canonicalize(&own).unwrap().join("frame.png"));
        let _ = std::fs::remove_dir_all(&home);
    }

    fn path(p: &Path) -> String {
        p.to_string_lossy().into_owned()
    }
}
