//! The assistant's tools (PLAN 2.6, SPEC §11): the operations an agent calls over MCP
//! (ADR-0009), run on the bundle the page holds. The page's assistant calls one by its MCP
//! tool's name, with the arguments that tool takes but for where the bundle is (`bundle`),
//! where a frame is written (`out`), and which painter draws it (here, the CPU painter):
//! the session is the bundle. Each returns what its MCP tool returns. An edit is written
//! into the session, and the page shows it as source.
//!
//! Each edit is computed by the operation's twin that writes nothing (`patching`, `fixing`,
//! `attaching`), so the CRDT, which writing reaches, stays out of the module. The session
//! keeps each edit, by whoever called the tool and with what the operation says it did, for
//! the next save to record in the bundle's history (PLAN 2.9, `store`).

use crate::{Error, Session};
use scaena_core::{Finding, Severity};
use scaena_ops::OpsError;
use scaena_ops::inspect::Views;
use scaena_ops::lint::Write;
use scaena_paint::Raster;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// The tools a page's assistant has, by their MCP names.
pub const TOOLS: &[&str] = &[
    "deck_read",
    "deck_patch",
    "deck_lint",
    "deck_inspect",
    "deck_diff",
    "deck_render",
    "spine_read",
    "spine_update",
    "data_attach",
];

/// Who calls a tool, and when, in seconds since 1970: an edit it makes is theirs in the
/// bundle's history (SPEC §8.2). The page's assistant is `agent:` and its model's name.
#[derive(Debug, Clone, Copy)]
pub struct Caller<'a> {
    pub author: &'a str,
    pub at: Option<i64>,
}

/// What a tool returned.
#[derive(Debug)]
pub struct Called {
    /// What its MCP tool returns, as JSON: the result's structured content, or for
    /// `deck_render`, the facts its text holds beside the image. Written as text, with the
    /// serializers the engine's module carries already (SPEC §15).
    pub result: String,
    /// `deck_render`'s frame, painted by the CPU painter.
    pub frame: Option<Raster>,
    /// Whether it changed the deck: the page takes the source again.
    pub edited: bool,
}

impl Called {
    fn of(result: impl Serialize) -> Result<Called, Error> {
        Ok(Called { result: serde_json::to_string(&result).map_err(ops)?, frame: None, edited: false })
    }
}

/// `deck_inspect`'s result, as its MCP tool's.
#[derive(Serialize)]
struct Inspected {
    states: Vec<scaena_ops::inspect::Inspected>,
}

/// `deck_diff`'s result, as its MCP tool's.
#[derive(Serialize)]
struct Diffed {
    changes: indexmap::IndexMap<String, scaena_ops::inspect::Change>,
}

/// Why a tool stopped, as an MCP tool's error result says it: `{ message, plan?, op? }`.
pub fn failure(e: &Error) -> Value {
    match e {
        Error::Tool(OpsError { message, plan, op }) => {
            let mut out = json!({ "message": message });
            if let Some(plan) = plan {
                out["plan"] = json!(plan);
            }
            if let Some(op) = op {
                out["op"] = json!(op);
            }
            out
        }
        other => json!({ "message": other.to_string() }),
    }
}

fn ops(e: impl std::fmt::Display) -> Error {
    Error::Tool(OpsError::new(e.to_string()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeckRead {
    #[serde(default)]
    scn: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeckPatch {
    ops: Vec<Map<String, Value>>,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SeverityArg {
    Error,
    Warning,
    #[default]
    Info,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeckLint {
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    severity: SeverityArg,
    #[serde(default)]
    fix: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeckInspect {
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    resolved: bool,
    #[serde(default)]
    timeline: bool,
    #[serde(default)]
    data: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeckDiff {
    from: String,
    to: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeckRender {
    state: String,
    #[serde(default)]
    t: Option<f64>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    size: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpineRead {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpineUpdate {
    spine: Map<String, Value>,
    #[serde(default)]
    dry_run: bool,
}

/// What `deck_lint` returns, as its MCP tool does.
#[derive(Serialize)]
struct Linted {
    findings: Vec<Finding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fixed: Option<Vec<Finding>>,
    errors: usize,
    laid: bool,
}

/// What `deck_render` says of its frame, as its MCP tool's text does.
#[derive(Serialize)]
struct Rendered {
    state: String,
    size: [u32; 2],
    span_ms: f64,
    digest: String,
    painter: &'static str,
}

fn args<T: DeserializeOwned>(tool: &str, args: Value) -> Result<T, Error> {
    // A tool called with no arguments may be sent `null`.
    let args = if args.is_null() { json!({}) } else { args };
    serde_json::from_value(args).map_err(|e| ops(format!("{tool}: {e}")))
}

impl Session {
    /// Call the assistant's tool `name` with `args`, as its MCP tool takes them less
    /// `bundle`, `out`, and `painter`, as `by`.
    pub fn tool(&mut self, name: &str, a: Value, by: Caller) -> Result<Called, Error> {
        let b = self.bundle();
        match name {
            "deck_read" => {
                let a: DeckRead = args(name, a)?;
                Called::of(scaena_ops::read::read(&b, a.scn)?)
            }
            "deck_patch" => {
                let a: DeckPatch = args(name, a)?;
                let ops = Value::Array(a.ops.into_iter().map(Value::Object).collect());
                let (mut patched, deck) = scaena_ops::patch::patching(&b, &ops, None)?;
                patched.applied &= !a.dry_run;
                let edited = self.write(deck.filter(|_| !a.dry_run), by)?;
                Ok(Called { edited, ..Called::of(patched)? })
            }
            "deck_lint" => {
                let a: DeckLint = args(name, a)?;
                let min = match a.severity {
                    SeverityArg::Error => Severity::Error,
                    SeverityArg::Warning => Severity::Warning,
                    SeverityArg::Info => Severity::Info,
                };
                let (findings, fixed, laid, deck) = match a.fix {
                    true => {
                        let (f, deck) = scaena_ops::lint::fixing(&b)?;
                        (f.findings, Some(f.fixed), true, deck)
                    }
                    false => {
                        let l = scaena_ops::lint::lint(&b)?;
                        (l.findings, None, l.laid, None)
                    }
                };
                let findings: Vec<Finding> = (findings.into_iter())
                    .filter(|f| f.severity >= min && a.state.as_ref().is_none_or(|s| f.state.as_deref() == Some(s)))
                    .collect();
                let errors = scaena_ops::lint::errors(&findings);
                let edited = self.write(deck, by)?;
                Ok(Called { edited, ..Called::of(Linted { findings, fixed, errors, laid })? })
            }
            "deck_inspect" => {
                let a: DeckInspect = args(name, a)?;
                let views = Views { resolved: a.resolved, timeline: a.timeline, data: a.data };
                Called::of(Inspected { states: scaena_ops::inspect::inspect(&b, a.state.as_deref(), views)? })
            }
            "deck_diff" => {
                let a: DeckDiff = args(name, a)?;
                Called::of(Diffed { changes: scaena_ops::inspect::diff(&b, &a.from, &a.to)? })
            }
            "deck_render" => {
                let a: DeckRender = args(name, a)?;
                self.render(a)
            }
            "spine_read" => {
                let _: SpineRead = args(name, a)?;
                Called::of(scaena_ops::read::spine(&b))
            }
            "spine_update" => {
                let a: SpineUpdate = args(name, a)?;
                let (mut patched, deck) = scaena_ops::read::spine_updating(&b, Value::Object(a.spine))?;
                patched.applied &= !a.dry_run;
                let edited = self.write(deck.filter(|_| !a.dry_run), by)?;
                Ok(Called { edited, ..Called::of(patched)? })
            }
            "data_attach" => {
                let a: scaena_ops::create::Attach = args(name, a)?;
                let path = a.file.to_string_lossy().into_owned();
                let bytes = (self.files.get(&path).cloned()).ok_or_else(|| {
                    ops(format!("data_attach: the bundle holds no `{path}`; drop the file on the page"))
                })?;
                let (attached, deck) = scaena_ops::create::attaching(&b, &a, bytes)?;
                let edited = self.write(deck, by)?;
                Ok(Called { edited, ..Called::of(attached)? })
            }
            _ => Err(ops(format!("no tool `{name}`: the tools are {}", TOOLS.join(", ")))),
        }
    }

    /// Write what an operation `by` called computed into the session: its files, then its
    /// deck, which frames show from now on, kept for the next save to record. Whether there
    /// was anything to write.
    fn write(&mut self, w: Option<Write>, by: Caller) -> Result<bool, Error> {
        let Some(w) = w else { return Ok(false) };
        self.keep(&w.deck, &w.why, by)?;
        for (path, bytes) in w.files {
            self.add_file(&path, bytes);
        }
        self.set_deck(w.deck);
        // The page compiles the deck's source again, as it does after a fix.
        self.edit = None;
        Ok(true)
    }

    /// `deck_render`: the state at rest, or `t` ms into its cue, painted by the CPU painter.
    fn render(&mut self, a: DeckRender) -> Result<Called, Error> {
        if let Some(t) = a.t.filter(|t| !t.is_finite() || *t < 0.0) {
            return Err(ops(format!("t {t}: expected a finite, non-negative number of milliseconds")));
        }
        let shown = self.format.clone();
        self.set_format(a.format.as_deref())?;
        let painted = self.paint(&a);
        self.set_format(shown.as_deref())?;
        let (rendered, raster) = painted?;
        Ok(Called { frame: Some(raster), ..Called::of(rendered)? })
    }

    fn paint(&mut self, a: &DeckRender) -> Result<(Rendered, Raster), Error> {
        use scaena_paint::Painter;
        let span_ms = self.duration(&a.state)?;
        let dl = self.frame(&a.state, a.t.unwrap_or(f64::INFINITY))?;
        let digest = dl.digest().map_err(ops)?;
        let scale = match a.size.as_deref() {
            Some(size) => scaena_ops::render::scale_for(size, dl.viewport)?,
            None => 1.0,
        };
        let raster = scaena_paint::cpu::CpuPainter::default().paint(&dl, &self.store, scale)?;
        let size = [raster.width, raster.height];
        Ok((Rendered { state: a.state.clone(), size, span_ms, digest, painter: "cpu" }, raster))
    }
}

impl From<OpsError> for Error {
    fn from(e: OpsError) -> Error {
        Error::Tool(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::Path;

    /// A new deck from Dusk, as `deck_create` makes one and the editor's New does (PLAN 2.12):
    /// the theme and its fonts, and one state with nothing on it.
    fn dusk() -> Session {
        let dir = Path::new("../../docs/examples");
        let theme = std::fs::read_to_string(dir.join("themes/dusk.theme.json")).unwrap();
        let fonts: BTreeMap<String, Vec<u8>> = ["Fraunces-VF.ttf", "Inter-VF.ttf", "JetBrainsMono-VF.ttf"]
            .iter()
            .map(|f| (format!("fonts/{f}"), std::fs::read(dir.join("fonts").join(f)).unwrap()))
            .collect();
        Session::create("dusk.theme.json", &theme, &fonts, "Agent loop").unwrap()
    }

    /// A tool's result, read back, and the rest of what it returned.
    struct Got {
        result: Value,
        frame: Option<Raster>,
        edited: bool,
    }

    /// The assistant, as the page's tests name it.
    pub(crate) const AGENT: Caller = Caller { author: "agent:scripted", at: None };

    fn call(s: &mut Session, name: &str, a: Value) -> Got {
        let c = s.tool(name, a, AGENT).unwrap_or_else(|e| panic!("{name}: {}", failure(&e)));
        Got { result: serde_json::from_str(&c.result).unwrap(), frame: c.frame, edited: c.edited }
    }

    /// PLAN 1.19's agent loop, through the tools a page's assistant calls: a headline too
    /// long for its one-row slot, the E100 lint finds with its fix, the fix, a clean lint,
    /// and the state rendered.
    #[test]
    fn the_agent_loop_runs_on_the_bundle_in_memory() {
        let mut s = dusk();
        let long = "Volunteers rebuilt fifty-two miles of trail";
        let patch = json!({ "ops": [
            { "op": "add", "path": "/states/0/layout", "value": "figure" },
            { "op": "add_node", "id": "headline", "state": "start",
              "node": { "type": "text", "role": "headline", "semantic": "claim", "at": { "in": "header" }, "text": long } },
        ] });
        let dry = call(&mut s, "deck_patch", json!({ "dry_run": true, "ops": patch["ops"] }));
        assert_eq!((dry.result["applied"].as_bool(), dry.edited), (Some(false), false), "{}", dry.result);
        assert!(!s.source().contains(long));

        let patched = call(&mut s, "deck_patch", patch);
        assert_eq!((patched.result["applied"].as_bool(), patched.edited), (Some(true), true), "{}", patched.result);
        assert!(s.source().contains(long), "the deck shown is the patched one");
        let added: Vec<&str> =
            (patched.result["added"].as_array().unwrap().iter()).map(|f| f["code"].as_str().unwrap()).collect();
        assert!(added.contains(&"E100"), "{added:?}");

        // The headline needs two lines, and its slot is one row tall.
        let linted = call(&mut s, "deck_lint", json!({ "severity": "error" }));
        let e100 = &linted.result["findings"].as_array().unwrap()[0];
        assert_eq!((e100["code"].as_str(), e100["node"].as_str()), (Some("E100"), Some("headline")));
        assert!(e100["fix"].is_array(), "{e100}");
        assert!(!linted.edited);

        let fixed = call(&mut s, "deck_lint", json!({ "fix": true }));
        assert!(fixed.edited);
        assert_eq!(fixed.result["fixed"][0]["code"], "E100");
        assert_eq!(fixed.result["errors"], 0, "{}", fixed.result);
        let again = call(&mut s, "deck_lint", json!({}));
        assert_eq!((again.result["errors"].as_u64(), again.result["laid"].as_bool()), (Some(0), Some(true)));

        let rendered = call(&mut s, "deck_render", json!({ "state": "start", "size": "960x540" }));
        let frame = rendered.frame.expect("a frame");
        assert_eq!([frame.width, frame.height], [960, 540]);
        assert_eq!(rendered.result["size"], json!([960, 540]));
        assert_eq!(rendered.result["digest"].as_str().map(str::len), Some(16), "{}", rendered.result);
        // Something is drawn: the background is not all there is.
        let first = &frame.rgba[..4];
        assert!(frame.rgba.chunks(4).any(|p| p != first));
    }

    #[test]
    fn a_refused_patch_changes_nothing_and_says_why() {
        let mut s = dusk();
        let before = s.source();
        let refused = call(
            &mut s,
            "deck_patch",
            json!({ "ops": [{ "op": "add_node", "id": "photo", "state": "start",
                "node": { "type": "image", "src": "assets/none.png", "alt": "Nothing" } }] }),
        );
        assert_eq!((refused.result["applied"].as_bool(), refused.edited), (Some(false), false), "{}", refused.result);
        assert!(refused.result["added"].as_array().unwrap().iter().any(|f| f["code"] == "E102"));
        assert_eq!(s.source(), before);
    }

    #[test]
    fn a_tool_says_what_it_was_given_wrong() {
        let mut s = dusk();
        let unknown = s.tool("deck_paint", json!({}), AGENT).unwrap_err();
        assert!(failure(&unknown)["message"].as_str().unwrap().contains("deck_patch"), "{}", failure(&unknown));
        let wrong = s.tool("deck_read", json!({ "bundle": "deck.json" }), AGENT).unwrap_err();
        assert!(failure(&wrong)["message"].as_str().unwrap().contains("bundle"), "{}", failure(&wrong));
        let op = s.tool("deck_patch", json!({ "ops": [{ "op": "remove_node", "id": "nothing" }] }), AGENT).unwrap_err();
        assert_eq!(failure(&op)["op"], 0, "{}", failure(&op));
    }

    #[test]
    fn reads_inspects_diffs_and_the_spine() {
        let mut s = dusk();
        call(
            &mut s,
            "deck_patch",
            json!({ "ops": [
                { "op": "add", "path": "/states/0/layout", "value": "figure" },
                { "op": "add_node", "id": "title", "state": "start",
                  "node": { "type": "text", "role": "headline", "at": { "in": "header" }, "text": "Hello" } },
                { "op": "add_state", "after": "start", "state": { "id": "next" } },
                { "op": "set_text", "node": "title", "state": "next", "text": "Hello again" },
            ] }),
        );
        let scn = call(&mut s, "deck_read", json!({ "scn": true }));
        assert!(scn.result["scn"].as_str().unwrap().contains("Hello again"));
        let json = call(&mut s, "deck_read", Value::Null);
        assert_eq!(json.result["deck"]["states"][1]["id"], "next");
        let inspected = call(&mut s, "deck_inspect", json!({ "state": "next", "resolved": true }));
        assert_eq!(inspected.result["states"][0]["looks"]["title"]["role"], "headline", "{}", inspected.result);
        let diff = call(&mut s, "deck_diff", json!({ "from": "start", "to": "next" }));
        assert!(diff.result["changes"]["title"].is_object(), "{}", diff.result);
        let spine = call(&mut s, "spine_read", json!({}));
        assert!(spine.result.is_object());
        let spine = json!({ "sections": [{ "id": "only", "title": "Only", "beats": [
            { "id": "hello", "claim": "We say hello.", "states": ["start", "next"] },
        ] }] });
        let updated = call(&mut s, "spine_update", json!({ "spine": spine }));
        assert!(updated.edited, "{}", updated.result);
        let read = call(&mut s, "spine_read", json!({}));
        assert_eq!(read.result["spine"]["sections"][0]["beats"][0]["id"], "hello", "{}", read.result);
    }

    #[test]
    fn data_attaches_from_a_file_the_bundle_holds() {
        let mut s = dusk();
        s.add_file("data/q.csv", b"quarter,revenue\nQ1,10\nQ2,14\n".to_vec());
        let attached = call(&mut s, "data_attach", json!({ "id": "q", "file": "data/q.csv" }));
        assert!(attached.edited && attached.result["attached"] == true, "{}", attached.result);
        assert_eq!(attached.result["schema"], json!({ "quarter": "string", "revenue": "number" }));
        assert!(s.source().contains("data/q.csv"));
        let missing = s.tool("data_attach", json!({ "id": "r", "file": "data/r.csv" }), AGENT).unwrap_err();
        assert!(failure(&missing)["message"].as_str().unwrap().contains("drop"), "{}", failure(&missing));
    }

    /// Each tool takes what its MCP tool takes (`docs/schema/mcp/`), less where the bundle
    /// is, where a frame goes, and the painter: an argument the MCP tool takes is one these
    /// take, and none other.
    #[test]
    fn the_tools_take_what_their_mcp_tools_take() {
        let mut s = dusk();
        for name in TOOLS {
            let path = format!("../../docs/schema/mcp/{name}.json");
            let schema: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let props = schema["inputSchema"]["properties"].as_object().unwrap();
            for prop in props.keys().filter(|p| !["bundle", "out", "painter"].contains(&p.as_str())) {
                // Taken, or refused for what it holds: never an unknown field.
                if let Err(e) = s.tool(name, json!({ prop.as_str(): { "not": "this" } }), AGENT) {
                    let message = failure(&e)["message"].as_str().unwrap().to_string();
                    assert!(!message.contains("unknown field"), "{name} takes no `{prop}`: {message}");
                }
            }
            let e = s.tool(name, json!({ "nothing_takes_this": 1 }), AGENT).unwrap_err();
            let message = failure(&e)["message"].as_str().unwrap().to_string();
            assert!(message.contains("unknown field") || message.contains("missing field"), "{name}: {message}");
        }
    }
}
