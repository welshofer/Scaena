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
    CallToolResult, ContentBlock, ErrorData, ListResourcesResult, PaginatedRequestParams, ReadResourceRequestParams,
    ReadResourceResponse, ReadResourceResult, Resource, ResourceContents, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{Json, RoleServer, ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use scaena_core::Finding;
use scaena_ops::OpsError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// The server: its tools, and the resources it serves.
#[derive(Debug, Clone)]
pub struct Scaena {
    tool_router: ToolRouter<Self>,
}

impl Default for Scaena {
    fn default() -> Self {
        Scaena { tool_router: Self::tool_router() }
    }
}

/// Serve on stdin and stdout until the client goes.
pub fn stdio() -> std::io::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(async {
        let server = Scaena::default().serve(rmcp::transport::stdio()).await.map_err(std::io::Error::other)?;
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

/// `f`, off the async runtime: the operations lay out and paint, and block.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, OpsError> + Send + 'static) -> Result<T, String> {
    match tokio::task::spawn_blocking(f).await {
        Ok(result) => result.map_err(failure),
        Err(e) => Err(failure(OpsError::new(format!("the operation stopped: {e}")))),
    }
}

fn open(bundle: &str) -> Result<scaena_ops::Bundle, OpsError> {
    scaena_ops::open(Path::new(bundle))
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
    /// The theme file to start from, copied in with the fonts its families name (from
    /// beside it, or above it): `docs/examples/themes/dusk.theme.json` is one.
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
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckRead {
    /// The bundle: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
    /// The deck as `.scn` source rather than JSON.
    #[serde(default)]
    pub scn: bool,
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
    /// `WxH` pixels, with the canvas's aspect ratio; the canvas's size without it.
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub painter: scaena_ops::render::Painter,
    /// Also write the PNG here.
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckExport {
    /// The bundle: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
    /// `spine` or `pdf`; `png`, `svg`, `mp4`, `webm`, and `html` name the PLAN tasks that
    /// build them.
    pub format: String,
    /// The states a frame export draws; every state without it.
    #[serde(default)]
    pub states: Option<Vec<String>>,
    /// Also write the export here; a PDF is written only here.
    #[serde(default)]
    pub out: Option<String>,
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
    /// The theme file; one outside the bundle is copied to `themes/`.
    pub theme: String,
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
pub struct SpineUpdate {
    /// The bundle: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
    /// The spine: its sections, each with its beats (SPEC §3.11, `scaena://schema/deck`'s
    /// `Spine`).
    pub spine: Map<String, Value>,
    #[serde(default)]
    pub dry_run: bool,
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

#[derive(Debug, Serialize, JsonSchema)]
pub struct Exported {
    pub format: String,
    /// Where it was written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out: Option<String>,
    /// The spine, for `spine`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spine: Option<Map<String, Value>>,
    /// The state each page draws, in order, for `pdf`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<Vec<String>>,
    /// The document's size in bytes, for `pdf`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<usize>,
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

fn object(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(map) => map,
        other => Map::from_iter([("value".to_string(), other)]),
    }
}

// --- tools ----------------------------------------------------------------------------

#[tool_router]
impl Scaena {
    #[tool(description = "Make a bundle: a theme and its fonts, data files, and a deck pointed at them \
        (or one state with nothing on screen). Checked as `deck_lint` checks a bundle, and written only if it validates.")]
    async fn deck_create(
        &self,
        Parameters(a): Parameters<DeckCreate>,
    ) -> Result<Json<scaena_ops::create::Created>, String> {
        let req = scaena_ops::create::Create {
            theme: PathBuf::from(a.theme),
            deck: a.deck.map(Value::Object),
            scn: a.scn,
            title: a.title,
            data: a.data,
        };
        blocking(move || scaena_ops::create::create(Path::new(&a.bundle), &req)).await.map(Json)
    }

    #[tool(description = "The bundle's deck, as `deck.json` holds it, or as `.scn` source.")]
    async fn deck_read(&self, Parameters(a): Parameters<DeckRead>) -> Result<Json<scaena_ops::read::Read>, String> {
        blocking(move || scaena_ops::read::read(&open(&a.bundle)?, a.scn)).await.map(Json)
    }

    #[tool(description = "Apply a patch: JSON Patch and semantic ops (add_node, rename_node, set_prop, set_text, \
        show_node, hide_node, add_state, …; SPEC §7.3), all or none. A patch that would make the deck invalid is refused. \
        Reports the patch as JSON Patch, and what lint finds that it did not (`added`) and finds no more (`removed`).")]
    async fn deck_patch(
        &self,
        Parameters(a): Parameters<DeckPatch>,
    ) -> Result<Json<scaena_ops::patch::Patched>, String> {
        let ops = Value::Array(a.ops.into_iter().map(Value::Object).collect());
        blocking(move || scaena_ops::patch::patch(&open(&a.bundle)?, &ops, a.dry_run)).await.map(Json)
    }

    #[tool(description = "Lint the bundle (SPEC §7.5): validation, the document rules, then layout, contrast, \
        motion, and narrative in every format it lists. Findings carry a JSON pointer, and a fix when one is safe; \
        `fix` applies them.")]
    async fn deck_lint(&self, Parameters(a): Parameters<DeckLint>) -> Result<Json<Linted>, String> {
        let min = match a.severity {
            SeverityArg::Error => scaena_core::Severity::Error,
            SeverityArg::Warning => scaena_core::Severity::Warning,
            SeverityArg::Info => scaena_core::Severity::Info,
        };
        blocking(move || {
            let b = open(&a.bundle)?;
            let (findings, fixed, laid) = match a.fix {
                true => {
                    let f = scaena_ops::lint::lint_fix(&b)?;
                    (f.findings, Some(f.fixed), true)
                }
                false => {
                    let l = scaena_ops::lint::lint(&b)?;
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
        clock, `data` the rows each chart and table reads.")]
    async fn deck_inspect(&self, Parameters(a): Parameters<DeckInspect>) -> Result<Json<Inspected>, String> {
        blocking(move || scaena_ops::inspect::inspect(&open(&a.bundle)?, a.state.as_deref(), a.views))
            .await
            .map(|states| Json(Inspected { states }))
    }

    #[tool(description = "Render a state to a PNG, returned as an image: at rest, or `t` ms into its cue. \
        With the display list's digest: one digest, one drawing.")]
    async fn deck_render(&self, Parameters(a): Parameters<DeckRender>) -> Result<CallToolResult, String> {
        let req = scaena_ops::render::Request {
            state: a.state.clone(),
            t: a.t,
            format: a.format,
            size: a.size,
            painter: a.painter,
        };
        let out = a.out.clone();
        let frame = blocking(move || {
            let frame = scaena_ops::render::render(Path::new(&a.bundle), &req)?;
            if let Some(out) = &out {
                std::fs::write(out, &frame.png).map_err(|e| OpsError::new(format!("writing {out}: {e}")))?;
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

    #[tool(description = "Export a projection: `spine`, or `pdf` written to `out` (each slide at its last state, \
        or `states`, one page each). Frames and video (PLAN 1.21) and HTML (2.5) say which task builds them.")]
    async fn deck_export(&self, Parameters(a): Parameters<DeckExport>) -> Result<Json<Exported>, String> {
        blocking(move || {
            if a.format == "pdf" && a.out.is_none() {
                return Err(OpsError::new("`pdf` writes a file: give `out`"));
            }
            match scaena_ops::export::export(&open(&a.bundle)?, &a.format, a.states.as_deref())? {
                scaena_ops::export::Export::Spine(v) => {
                    if let Some(out) = &a.out {
                        let text = serde_json::to_string_pretty(&v)? + "\n";
                        std::fs::write(out, text).map_err(|e| OpsError::new(format!("writing {out}: {e}")))?;
                    }
                    Ok(Exported { format: a.format, out: a.out, spine: Some(object(v)), pages: None, bytes: None })
                }
                scaena_ops::export::Export::Pdf { bytes, pages } => {
                    let out = a.out.clone().expect("checked above");
                    std::fs::write(&out, &bytes).map_err(|e| OpsError::new(format!("writing {out}: {e}")))?;
                    let size = bytes.len();
                    Ok(Exported { format: a.format, out: a.out, spine: None, pages: Some(pages), bytes: Some(size) })
                }
            }
        })
        .await
        .map(Json)
    }

    #[tool(
        description = "What changes between two states, resolved: by node, it enters, exits, or changes these props."
    )]
    async fn deck_diff(&self, Parameters(a): Parameters<DeckDiff>) -> Result<Json<Diffed>, String> {
        blocking(move || scaena_ops::inspect::diff(&open(&a.bundle)?, &a.from, &a.to))
            .await
            .map(|changes| Json(Diffed { changes }))
    }

    #[tool(description = "Re-theme: point the deck at another theme, copied into the bundle, and say what that \
        changes in what lint finds. A theme change is a re-render; the deck is not otherwise touched.")]
    async fn theme_apply(
        &self,
        Parameters(a): Parameters<ThemeApply>,
    ) -> Result<Json<scaena_ops::theme::Themed>, String> {
        blocking(move || scaena_ops::theme::theme_apply(&open(&a.bundle)?, Path::new(&a.theme), a.dry_run))
            .await
            .map(Json)
    }

    #[tool(description = "Attach a CSV or JSON data file: copy it into the bundle's `data/` and declare it as a \
        source charts and tables name as `@id`, each column typed (inferred from its values without `schema`).")]
    async fn data_attach(
        &self,
        Parameters(a): Parameters<DataAttach>,
    ) -> Result<Json<scaena_ops::create::Attached>, String> {
        blocking(move || scaena_ops::create::attach(&open(&a.bundle)?, &a.data)).await.map(Json)
    }

    #[tool(
        description = "The deck's spine (SPEC §2.6): its sections and beats, each beat's claim, evidence, and states."
    )]
    async fn spine_read(&self, Parameters(a): Parameters<BundleArg>) -> Result<Json<Map<String, Value>>, String> {
        blocking(move || Ok(object(scaena_ops::read::spine(&open(&a.bundle)?)))).await.map(Json)
    }

    #[tool(description = "Replace the deck's spine, as a patch: checked, refused if it makes the deck invalid, \
        with what lint finds differently.")]
    async fn spine_update(
        &self,
        Parameters(a): Parameters<SpineUpdate>,
    ) -> Result<Json<scaena_ops::patch::Patched>, String> {
        blocking(move || scaena_ops::read::spine_update(&open(&a.bundle)?, Value::Object(a.spine), a.dry_run))
            .await
            .map(Json)
    }
}

// --- resources ------------------------------------------------------------------------

/// What the server serves as resources: (uri, name, MIME type, text).
const RESOURCES: &[(&str, &str, &str, &str)] = &[
    (
        "scaena://schema/deck",
        "The deck format",
        "application/schema+json",
        include_str!("../../../docs/schema/deck.schema.json"),
    ),
    (
        "scaena://schema/theme",
        "The theme format",
        "application/schema+json",
        include_str!("../../../docs/schema/theme.schema.json"),
    ),
    (
        "scaena://schema/patch",
        "A patch's ops",
        "application/schema+json",
        include_str!("../../../docs/schema/patch.schema.json"),
    ),
    ("scaena://lint/catalog", "The lint catalog", "text/markdown", ""),
    ("scaena://spec", "The specification", "text/markdown", include_str!("../../../docs/SPEC.md")),
    (
        "scaena://skills/author-deck",
        "How to author a deck",
        "text/markdown",
        include_str!("../../../skills/author-deck/SKILL.md"),
    ),
    (
        "scaena://skills/chart-from-data",
        "How to make a chart or table from data",
        "text/markdown",
        include_str!("../../../skills/chart-from-data/SKILL.md"),
    ),
    (
        "scaena://skills/motion-pass",
        "How to set a deck's motion",
        "text/markdown",
        include_str!("../../../skills/motion-pass/SKILL.md"),
    ),
    (
        "scaena://skills/retheme",
        "How to apply another theme",
        "text/markdown",
        include_str!("../../../skills/retheme/SKILL.md"),
    ),
    (
        "scaena://skills/tighten-copy",
        "How to tighten a deck's words",
        "text/markdown",
        include_str!("../../../skills/tighten-copy/SKILL.md"),
    ),
    (
        "scaena://examples/revenue.deck.json",
        "An example deck",
        "application/json",
        include_str!("../../../docs/examples/revenue.deck.json"),
    ),
    (
        "scaena://examples/trails.deck.json",
        "A fifteen-slide example: text, a stat, a photograph, five kinds of chart, a table, cards, and a quote",
        "application/json",
        include_str!("../../../docs/examples/trails.deck.json"),
    ),
    (
        "scaena://examples/revenue.deck.scn",
        "The example deck as .scn",
        "text/plain",
        include_str!("../../../docs/examples/revenue.deck.scn"),
    ),
    (
        "scaena://examples/revenue.patch.json",
        "An example patch",
        "application/json",
        include_str!("../../../docs/examples/revenue.patch.json"),
    ),
    (
        "scaena://examples/dusk.theme.json",
        "An example theme",
        "application/json",
        include_str!("../../../docs/examples/themes/dusk.theme.json"),
    ),
];

/// The lint catalog: SPEC §7.5, as SPEC writes it.
fn catalog() -> &'static str {
    let spec = include_str!("../../../docs/SPEC.md");
    let start = spec.find("### 7.5").unwrap_or(0);
    let end = spec[start..].find("### 7.6").map_or(spec.len(), |i| start + i);
    &spec[start..end]
}

/// A resource's text, by its uri.
pub fn resource(uri: &str) -> Option<&'static str> {
    match uri {
        "scaena://lint/catalog" => Some(catalog()),
        _ => RESOURCES.iter().find(|r| r.0 == uri).map(|r| r.3),
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Scaena {
    fn get_info(&self) -> ServerConfig {
        let capabilities = ServerCapabilities::builder().enable_tools().enable_resources().build();
        ServerConfig::new(capabilities).with_instructions(
            "Scaena decks are states over one scene graph: nodes exist for the whole deck, each state says what changes, \
             and the theme owns type and layout, so a deck names roles, slots, and presets, never pixels. Make a bundle \
             with deck_create, attach data with data_attach, edit with deck_patch, check with deck_lint, and look with \
             deck_render. The resources hold the schemas, the lint catalog, the specification, the skills (procedures to \
             follow: scaena://skills/author-deck first), and examples.",
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let resources = RESOURCES
            .iter()
            .map(|(uri, name, mime, _)| {
                let text = resource(uri).unwrap_or_default();
                Resource::new(*uri, *name).with_mime_type(*mime).with_size(text.len() as u64)
            })
            .collect();
        Ok(ListResourcesResult::with_all_items(resources))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let text = resource(&request.uri)
            .ok_or_else(|| ErrorData::resource_not_found(format!("no resource `{}`", request.uri), None))?;
        Ok(ReadResourceResult::new(vec![ResourceContents::text(text, request.uri)]).into())
    }
}
