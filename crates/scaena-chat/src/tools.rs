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
    ("deck_history", include_str!("../../../docs/schema/mcp/deck_history.json")),
    ("deck_inspect", include_str!("../../../docs/schema/mcp/deck_inspect.json")),
    ("deck_lint", include_str!("../../../docs/schema/mcp/deck_lint.json")),
    ("deck_patch", include_str!("../../../docs/schema/mcp/deck_patch.json")),
    ("deck_read", include_str!("../../../docs/schema/mcp/deck_read.json")),
    ("deck_render", include_str!("../../../docs/schema/mcp/deck_render.json")),
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

/// A line saying what a tool's result came to, for the client to show beside the call.
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
            Value::Null => format!("stopped: {json}"),
            message => format!("stopped: {}", shown(message)),
        };
    }
    match name {
        "deck_patch" | "spine_update" => {
            let done = if truthy(&r["applied"]) {
                "applied"
            } else if any(&r["added"]) {
                "refused"
            } else {
                "not applied (a dry run)"
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
                "not replaced (a dry run)"
            };
            format!("{found}; {done}{}", delta(p))
        }
        "data_attach" => {
            let done = if truthy(&r["attached"]) {
                format!("attached {}: {} rows", shown(&r["source"]), shown(&r["rows"]))
            } else {
                "refused".to_string()
            };
            format!("{done}{}", delta(&r))
        }
        "data_edit" => {
            let rows = r.pointer("/sheet/rows").and_then(Value::as_array).map_or(0, Vec::len);
            let sheet = format!("{}, {rows} row{}", shown(&r["source"]), if rows == 1 { "" } else { "s" });
            if truthy(&r["edited"]) {
                format!("edited {sheet}{}", delta(&r))
            } else if truthy(&r["refused"]) {
                format!("refused{}", delta(&r))
            } else if any(&r["added"]) || any(&r["removed"]) {
                format!("not written (a dry run){}", delta(&r))
            } else {
                format!("read {sheet}")
            }
        }
        "theme_edit" => {
            if truthy(&r["refused"]) {
                return format!("refused{}", delta(&r));
            }
            let done = if truthy(&r["applied"]) { "edited" } else { "not written (a dry run)" };
            let paths: Vec<String> = r["paths"].as_array().into_iter().flatten().map(shown).collect();
            format!("{done} {}: {}{}", shown(&r["theme"]), paths.join(", "), delta(&r))
        }
        "deck_lint" => {
            let fixed = if any(&r["fixed"]) { format!("fixed {}; ", codes(&r["fixed"])) } else { String::new() };
            let findings = r["findings"].as_array().map(Vec::as_slice).unwrap_or_default();
            let found: Vec<String> = ["error", "warning", "info"]
                .iter()
                .filter_map(|severity| {
                    let n = findings.iter().filter(|f| f["severity"].as_str() == Some(severity)).count();
                    (n > 0).then(|| format!("{n} {severity}{}", if n > 1 { "s" } else { "" }))
                })
                .collect();
            let found = if found.is_empty() { "nothing found".to_string() } else { found.join(", ") };
            format!("{fixed}{found}")
        }
        "deck_render" => {
            let size = match r["size"].as_array() {
                Some(size) => size.iter().map(shown).collect::<Vec<_>>().join("×"),
                None => "undefined".to_string(),
            };
            format!("{}, {size} px", shown(&r["state"]))
        }
        "deck_read" => match r["scn"].as_str() {
            Some(scn) => format!("{} lines of .scn", scn.split('\n').count()),
            None => "deck.json".to_string(),
        },
        "deck_inspect" => {
            let n = r["states"].as_array().map_or(0, Vec::len);
            format!("{n} state{}", if n == 1 { "" } else { "s" })
        }
        "deck_diff" => format!("{} nodes change", r["changes"].as_object().map_or(0, |c| c.len())),
        _ => "done".to_string(),
    }
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

/// The findings' codes, each once, in the order they come.
fn codes(findings: &Value) -> String {
    let mut seen: Vec<String> = Vec::new();
    for f in findings.as_array().into_iter().flatten() {
        let code = shown(&f["code"]);
        if !seen.contains(&code) {
            seen.push(code);
        }
    }
    seen.join(", ")
}

/// What a result's lint delta came to: the codes it added and removed.
fn delta(p: &Value) -> String {
    let mut parts = Vec::new();
    if any(&p["added"]) {
        parts.push(format!("+{}", codes(&p["added"])));
    }
    if any(&p["removed"]) {
        parts.push(format!("−{}", codes(&p["removed"])));
    }
    if parts.is_empty() { String::new() } else { format!(" ({})", parts.join(" ")) }
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
        assert_eq!(summary("deck_lint", lint, false), "fixed E110; 2 errors, 1 warning");
        assert_eq!(summary("deck_lint", r#"{"findings":[]}"#, false), "nothing found");
        let patch = r#"{"applied":false,"added":[{"code":"E101"},{"code":"E101"}],"removed":[{"code":"W300"}]}"#;
        assert_eq!(summary("deck_patch", patch, false), "refused (+E101 −W300)");
        assert_eq!(summary("deck_patch", r#"{"applied":true}"#, false), "applied");
        assert_eq!(summary("deck_patch", r#"{"applied":false}"#, false), "not applied (a dry run)");
        assert_eq!(summary("deck_find", r#"{"matches":1,"found":[{}]}"#, false), "1 match in 1 text");
        assert_eq!(summary("deck_find", r#"{"found":[]}"#, false), "0 matches in 0 texts");
        assert_eq!(
            summary("deck_find", r#"{"matches":3,"found":[{},{}],"replaced":{"applied":true}}"#, false),
            "3 matches in 2 texts; replaced"
        );
        assert_eq!(summary("deck_render", r#"{"state":"cover","size":[1920,1080]}"#, false), "cover, 1920×1080 px");
        assert_eq!(summary("deck_read", r#"{"scn":"a\nb\nc"}"#, false), "3 lines of .scn");
        assert_eq!(summary("deck_read", r#"{"deck":{}}"#, false), "deck.json");
        assert_eq!(summary("data_edit", r#"{"source":"sales","sheet":{"rows":[[1]]}}"#, false), "read sales, 1 row");
        assert_eq!(
            summary("data_attach", r#"{"attached":true,"source":"sales","rows":12}"#, false),
            "attached sales: 12 rows"
        );
        assert_eq!(
            summary("theme_edit", r#"{"applied":true,"theme":"dusk","paths":["/colors/accent"]}"#, false),
            "edited dusk: /colors/accent"
        );
        assert_eq!(summary("deck_patch", r#"{"message":"no such op"}"#, true), "stopped: no such op");
        assert_eq!(summary("resource_read", "é𝄞", false), "3 characters");
        assert_eq!(summary("deck_lint", &"x".repeat(300), false), "x".repeat(200));
        assert_eq!(summary("spine_read", "{}", false), "done");
    }
}
