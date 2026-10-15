//! The assistant's tools (PLAN 2.6, 3.6): the MCP server's, as `docs/schema/mcp/` says each one
//! is called (ADR-0009), less what names a place on disk (`bundle`, `out`) and the painter, since
//! they work on the bundle the client holds; and `resource_read`, which reads what the server
//! serves as resources. As `web/src/assistant/tools.ts` gives them.

use crate::providers::first;
use crate::{ChatError, Tool};
use serde_json::{Value, json};

/// Each MCP tool's schema, as `just bless` writes it from the server's code.
const MCP: &[(&str, &str)] = &[
    ("data_attach", include_str!("../../../docs/schema/mcp/data_attach.json")),
    ("data_edit", include_str!("../../../docs/schema/mcp/data_edit.json")),
    ("deck_create", include_str!("../../../docs/schema/mcp/deck_create.json")),
    ("deck_diff", include_str!("../../../docs/schema/mcp/deck_diff.json")),
    ("deck_export", include_str!("../../../docs/schema/mcp/deck_export.json")),
    ("deck_find", include_str!("../../../docs/schema/mcp/deck_find.json")),
    ("deck_health", include_str!("../../../docs/schema/mcp/deck_health.json")),
    ("deck_history", include_str!("../../../docs/schema/mcp/deck_history.json")),
    ("deck_inspect", include_str!("../../../docs/schema/mcp/deck_inspect.json")),
    ("deck_lint", include_str!("../../../docs/schema/mcp/deck_lint.json")),
    ("deck_patch", include_str!("../../../docs/schema/mcp/deck_patch.json")),
    ("deck_read", include_str!("../../../docs/schema/mcp/deck_read.json")),
    ("deck_render", include_str!("../../../docs/schema/mcp/deck_render.json")),
    ("icon_find", include_str!("../../../docs/schema/mcp/icon_find.json")),
    ("map_fetch", include_str!("../../../docs/schema/mcp/map_fetch.json")),
    ("spine_read", include_str!("../../../docs/schema/mcp/spine_read.json")),
    ("spine_update", include_str!("../../../docs/schema/mcp/spine_update.json")),
    ("theme_apply", include_str!("../../../docs/schema/mcp/theme_apply.json")),
    ("theme_edit", include_str!("../../../docs/schema/mcp/theme_edit.json")),
];

/// What a client's tool does not take: the session is the bundle, and the CPU painter draws.
const DROPPED: [&str; 3] = ["bundle", "out", "painter"];

/// The tool that reads a resource.
pub const RESOURCE_READ: &str = "resource_read";

/// `resource_read`: the MCP server's resources, the skills a bundle carries, and its theme.
pub fn resource_read() -> Tool {
    Tool {
        name: RESOURCE_READ.to_string(),
        description: "Read a resource, as the MCP server serves it: the schemas (scaena://schema/deck, scaena://schema/patch, …), \
            the lint catalog (scaena://lint/catalog), the specification by section (scaena://spec is its index), \
            the skills (scaena://skills/author-deck, …), and examples; a skill the bundle carries (bundle://skills/NAME); \
            and the theme the deck is drawn in, as theme_edit edits it (bundle://theme)."
            .to_string(),
        schema: json!({
            "type": "object",
            "properties": { "uri": { "type": "string", "description": "The resource's uri." } },
            "required": ["uri"],
            "additionalProperties": false,
        }),
    }
}

/// The tools named `names`, as their MCP tools take them, less what a client does not take;
/// then `resource_read`.
pub fn tools(names: &[&str]) -> Result<Vec<Tool>, ChatError> {
    let mut out = Vec::with_capacity(names.len() + 1);
    for name in names {
        let Some((_, text)) = MCP.iter().find(|(n, _)| n == name) else {
            return Err(ChatError::Provider(format!("docs/schema/mcp has no {name}")));
        };
        let tool: Value = serde_json::from_str(text).map_err(|e| ChatError::Provider(format!("{name}: {e}")))?;
        let mut schema = tool["inputSchema"].clone();
        if let Some(props) = schema.get_mut("properties").and_then(Value::as_object_mut) {
            for drop in DROPPED {
                props.shift_remove(drop);
            }
        }
        if let Some(required) = schema.get_mut("required").and_then(Value::as_array_mut) {
            required.retain(|r| !r.as_str().is_some_and(|r| DROPPED.contains(&r)));
        }
        let description = tool["description"].as_str().unwrap_or_default().to_string();
        out.push(Tool { name: name.to_string(), description, schema });
    }
    out.push(resource_read());
    Ok(out)
}

/// A line saying what a tool's result came to, for the client to show beside the call, in a
/// person's words (PLAN 2.99): what lint finds counted, never its codes, as the browser's
/// `summary` says it (`web/src/assistant/tools.ts`).
pub fn summary(name: &str, json: &str, error: bool) -> String {
    // A resource is its text, not JSON.
    if name == RESOURCE_READ && !error {
        return format!("{} characters", json.encode_utf16().count());
    }
    let Ok(r) = serde_json::from_str::<Value>(json) else {
        return first(json, 200);
    };
    if error {
        return match &r["message"] {
            Value::Null => format!("Stopped: {json}"),
            message => format!("Stopped: {}", shown(message)),
        };
    }
    match name {
        "deck_patch" | "spine_update" => {
            let done = if truthy(&r["applied"]) {
                "Done"
            } else if any(&r["added"]) {
                "Refused"
            } else {
                "Tried, not made"
            };
            format!("{done}{}", delta(&r))
        }
        "deck_find" => {
            let texts = r["found"].as_array().map_or(0, Vec::len);
            let matches = if r["matches"].is_null() { "0".to_string() } else { shown(&r["matches"]) };
            let es = if r["matches"].as_f64() == Some(1.0) { "" } else { "es" };
            let s = if texts == 1 { "" } else { "s" };
            let found = format!("{matches} match{es} in {texts} text{s}");
            let p = &r["replaced"];
            if !truthy(p) {
                return found;
            }
            let done = if truthy(&p["applied"]) {
                "replaced"
            } else if any(&p["added"]) {
                "refused"
            } else {
                "tried, not replaced"
            };
            format!("{found}; {done}{}", delta(p))
        }
        "data_attach" => {
            let done = if truthy(&r["attached"]) {
                format!("Added {}: {} rows", shown(&r["source"]), shown(&r["rows"]))
            } else {
                "Refused".to_string()
            };
            format!("{done}{}", delta(&r))
        }
        "data_edit" => {
            let rows = r.pointer("/sheet/rows").and_then(Value::as_array).map_or(0, Vec::len);
            let sheet = format!("{}, {rows} row{}", shown(&r["source"]), if rows == 1 { "" } else { "s" });
            if truthy(&r["edited"]) {
                format!("Changed {sheet}{}", delta(&r))
            } else if truthy(&r["refused"]) {
                format!("Refused{}", delta(&r))
            } else if any(&r["added"]) || any(&r["removed"]) {
                format!("Tried, not written{}", delta(&r))
            } else {
                format!("Read {sheet}")
            }
        }
        "theme_edit" => {
            if truthy(&r["refused"]) {
                return format!("Refused{}", delta(&r));
            }
            let done = if truthy(&r["applied"]) { "Changed" } else { "Tried changing" };
            let values = many(r["paths"].as_array().map_or(0, Vec::len), "value");
            format!("{done} {}, {values}{}", shown(&r["theme"]), delta(&r))
        }
        "deck_lint" => {
            let fixed = match r["fixed"].as_array() {
                Some(fixed) if !fixed.is_empty() => format!("Fixed {}; ", many(fixed.len(), "issue")),
                _ => String::new(),
            };
            let findings = r["findings"].as_array().map(Vec::as_slice).unwrap_or_default();
            let found: Vec<String> = [("error", "problem"), ("warning", "warning"), ("info", "note")]
                .iter()
                .filter_map(|(severity, said)| {
                    let n = findings.iter().filter(|f| f["severity"].as_str() == Some(severity)).count();
                    (n > 0).then(|| format!("{n} {said}{}", if n > 1 { "s" } else { "" }))
                })
                .collect();
            let none = if fixed.is_empty() { "No issues" } else { "no issues" };
            let found = if found.is_empty() { none.to_string() } else { found.join(", ") };
            format!("{fixed}{found}")
        }
        "deck_render" => {
            let size = match r["size"].as_array() {
                Some(size) => size.iter().map(shown).collect::<Vec<_>>().join("×"),
                None => "undefined".to_string(),
            };
            format!("{size} px")
        }
        "deck_read" => match r["scn"].as_str() {
            Some(scn) => format!("The deck, {} lines", scn.split('\n').count()),
            None => "The deck".to_string(),
        },
        "deck_inspect" => match r["states"].as_array().map_or(0, Vec::len) {
            1 => "1 slide".to_string(),
            n => format!("{n} slides and steps"),
        },
        "deck_diff" => match r["changes"].as_object().map_or(0, |c| c.len()) {
            1 => "1 object changes".to_string(),
            n => format!("{n} objects change"),
        },
        _ => "Done".to_string(),
    }
}

/// Whether a call did what the window heads it as having done (the browser's `done`, the Mac's
/// `ToolWords.done`): not where it stopped, nor where an edit was refused or only tried, which
/// the window heads as not done, as the browser's `made` says it (`web/src/assistant/tools.ts`).
pub fn made(name: &str, json: &str, error: bool) -> bool {
    if error {
        return false;
    }
    let Ok(r) = serde_json::from_str::<Value>(json) else { return true };
    match name {
        "deck_patch" | "spine_update" | "theme_apply" => truthy(&r["applied"]),
        "theme_edit" => truthy(&r["applied"]) && !truthy(&r["refused"]),
        "data_attach" => truthy(&r["attached"]),
        "data_edit" => truthy(&r["edited"]),
        _ => true,
    }
}

/// `n` of `one`, made plural where it is not one.
fn many(n: usize, one: &str) -> String {
    format!("{n} {one}{}", if n == 1 { "" } else { "s" })
}

/// `value` as a page prints it into text.
fn shown(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => "null".to_string(),
        Value::Array(items) => {
            let items: Vec<String> =
                (items.iter()).map(|v| if v.is_null() { String::new() } else { shown(v) }).collect();
            items.join(",")
        }
        Value::Object(_) => "[object Object]".to_string(),
        other => other.to_string(),
    }
}

/// Whether a page counts `value` as true.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// Whether `value` is a list with something in it.
fn any(value: &Value) -> bool {
    value.as_array().is_some_and(|a| !a.is_empty())
}

/// What a result's lint delta came to, as a person counts it: the issues it added, and those it
/// took away.
fn delta(p: &Value) -> String {
    let added = p["added"].as_array().map_or(0, Vec::len);
    let removed = p["removed"].as_array().map_or(0, Vec::len);
    let mut parts = Vec::new();
    if added > 0 {
        parts.push(format!("{added} new {}", if added == 1 { "issue" } else { "issues" }));
    }
    if removed > 0 {
        parts.push(if added > 0 { format!("{removed} gone") } else { format!("{} gone", many(removed, "issue")) });
    }
    if parts.is_empty() { String::new() } else { format!(" ({})", parts.join(", ")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mcp_tool_is_known() {
        let mut files: Vec<String> = std::fs::read_dir("../../docs/schema/mcp")
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().trim_end_matches(".json").to_string())
            .collect();
        files.sort();
        let known: Vec<&str> = MCP.iter().map(|(n, _)| *n).collect();
        assert_eq!(files, known, "docs/schema/mcp's tools, each in MCP");
    }

    #[test]
    fn a_tool_takes_no_bundle_and_reads_resources_last() {
        let given = tools(&["deck_render", "deck_lint"]).unwrap();
        let names: Vec<&str> = given.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["deck_render", "deck_lint", "resource_read"]);
        for tool in &given {
            let props = tool.schema["properties"].as_object().unwrap();
            for drop in DROPPED {
                assert!(!props.contains_key(drop), "{} takes {drop}", tool.name);
            }
            let required = tool.schema["required"].as_array().cloned().unwrap_or_default();
            assert!(required.iter().all(|r| !DROPPED.contains(&r.as_str().unwrap())), "{}", tool.name);
        }
        assert!(given[0].description.starts_with("Render a state to a PNG"));
        assert_eq!(
            tools(&["deck_dance"]).unwrap_err(),
            ChatError::Provider("docs/schema/mcp has no deck_dance".into())
        );
    }

    #[test]
    fn a_summary_says_what_a_call_came_to() {
        let lint = r#"{"findings":[{"code":"E100","severity":"error"},{"code":"W300","severity":"warning"},{"code":"E101","severity":"error"}],"fixed":[{"code":"E110"},{"code":"E110"}]}"#;
        assert_eq!(summary("deck_lint", lint, false), "Fixed 2 issues; 2 problems, 1 warning");
        assert_eq!(summary("deck_lint", r#"{"findings":[]}"#, false), "No issues");
        let patch = r#"{"applied":false,"added":[{"code":"E101"},{"code":"E101"}],"removed":[{"code":"W300"}]}"#;
        assert_eq!(summary("deck_patch", patch, false), "Refused (2 new issues, 1 gone)");
        assert_eq!(
            summary("deck_patch", r#"{"applied":true,"removed":[{"code":"W300"}]}"#, false),
            "Done (1 issue gone)"
        );
        assert_eq!(summary("deck_patch", r#"{"applied":true}"#, false), "Done");
        assert_eq!(summary("deck_patch", r#"{"applied":false}"#, false), "Tried, not made");
        assert_eq!(summary("deck_find", r#"{"matches":1,"found":[{}]}"#, false), "1 match in 1 text");
        assert_eq!(summary("deck_find", r#"{"found":[]}"#, false), "0 matches in 0 texts");
        assert_eq!(
            summary("deck_find", r#"{"matches":3,"found":[{},{}],"replaced":{"applied":true}}"#, false),
            "3 matches in 2 texts; replaced"
        );
        assert_eq!(summary("deck_render", r#"{"state":"cover","size":[1920,1080]}"#, false), "1920×1080 px");
        assert_eq!(summary("deck_read", r#"{"scn":"a\nb\nc"}"#, false), "The deck, 3 lines");
        assert_eq!(summary("deck_read", r#"{"deck":{}}"#, false), "The deck");
        assert_eq!(summary("data_edit", r#"{"source":"sales","sheet":{"rows":[[1]]}}"#, false), "Read sales, 1 row");
        assert_eq!(
            summary("data_attach", r#"{"attached":true,"source":"sales","rows":12}"#, false),
            "Added sales: 12 rows"
        );
        assert_eq!(
            summary("theme_edit", r#"{"applied":true,"theme":"dusk","paths":["/colors/accent"]}"#, false),
            "Changed dusk, 1 value"
        );
        assert_eq!(summary("deck_inspect", r#"{"states":[{},{},{}]}"#, false), "3 slides and steps");
        assert_eq!(summary("deck_diff", r#"{"changes":{"title":{}}}"#, false), "1 object changes");
        assert_eq!(summary("deck_patch", r#"{"message":"no such op"}"#, true), "Stopped: no such op");
        assert_eq!(summary("resource_read", "é𝄞", false), "3 characters");
        assert_eq!(summary("deck_lint", &"x".repeat(300), false), "x".repeat(200));
        assert_eq!(summary("spine_read", "{}", false), "Done");
    }

    /// A call is headed as done only where it did what its words say: an edit refused, only
    /// tried, or stopped is not.
    #[test]
    fn a_call_refused_or_stopped_is_not_made() {
        assert!(made("deck_patch", r#"{"applied":true}"#, false));
        assert!(!made("deck_patch", r#"{"applied":false,"added":[{}]}"#, false), "refused");
        assert!(!made("deck_patch", r#"{"applied":false}"#, false), "tried, not made");
        assert!(!made("deck_patch", r#"{"message":"no such op"}"#, true), "stopped");
        assert!(!made("theme_edit", r#"{"applied":false,"refused":true}"#, false));
        assert!(!made("data_edit", r#"{"edited":false,"refused":true}"#, false));
        assert!(!made("data_attach", r#"{"attached":false}"#, false));
        assert!(made("deck_lint", r#"{"findings":[]}"#, false));
        assert!(made("resource_read", "# A skill", false));
        assert!(!made("deck_render", r#"{"message":"no state"}"#, true));
    }
}
