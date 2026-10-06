//! `scaena mcp` (PLAN 1.17): the MCP server on stdio, as an agent's client starts it.

use rmcp::RoleClient;
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult, ClientConfig, ContentBlock};
use rmcp::service::RunningService;
use serde_json::{Value, json};
use std::path::Path;
use std::process::Stdio;

type Client = RunningService<RoleClient, ClientConfig>;

/// `scaena mcp` as a child process, and a client talking to it on its stdio.
async fn serve() -> (Client, tokio::process::Child) {
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_scaena"))
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let io = (child.stdout.take().unwrap(), child.stdin.take().unwrap());
    (ClientConfig::default().serve(io).await.unwrap(), child)
}

async fn call(client: &Client, tool: &str, args: Value) -> CallToolResult {
    let request = CallToolRequestParams::new(tool.to_string()).with_arguments(args.as_object().unwrap().clone());
    client.call_tool(request).await.unwrap()
}

/// A tool's structured result, which must not be an error.
async fn ok(client: &Client, tool: &str, args: Value) -> Value {
    let result = call(client, tool, args).await;
    assert_ne!(result.is_error, Some(true), "{tool}: {:?}", result.content);
    result.structured_content.unwrap_or_else(|| panic!("{tool}: no structured content"))
}

#[tokio::test(flavor = "multi_thread")]
async fn scaena_mcp_serves_on_stdio_until_the_client_goes() {
    let (client, mut child) = serve().await;
    assert_eq!(client.list_all_tools().await.unwrap().len(), 13);
    let spine = ok(&client, "spine_read", json!({ "bundle": "../../docs/examples/revenue.deck.json" })).await;
    assert_eq!(spine["title"], "Q3 Review");
    client.cancel().await.unwrap();
    assert!(child.wait().await.unwrap().success(), "the server exits cleanly when the client goes");
}

/// The agent loop (SPEC §14, PLAN 1.19), as an agent runs it against `scaena mcp`: create
/// a deck, set a headline too long for its slot, find the E100 that lint offers a fix
/// for, apply it, lint again to no errors, and look.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_fixes_what_lint_finds_then_renders() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("mcp-agent-loop");
    let _ = std::fs::remove_dir_all(&dir);
    let bundle = dir.join("deck").to_str().unwrap().to_string();
    let (client, mut child) = serve().await;

    let theme = "../../docs/examples/themes/dusk.theme.json";
    let made = ok(&client, "deck_create", json!({ "bundle": bundle, "theme": theme, "title": "Loop" })).await;
    assert_eq!(made["created"], true, "{made:#}");
    let ops = json!([
        { "op": "add", "path": "/states/0/layout", "value": "figure" },
        { "op": "add_node", "id": "headline", "state": "start",
          "node": { "type": "text", "role": "headline", "semantic": "claim", "at": { "in": "header" },
                    "text": "Volunteers rebuilt fifty-two miles of trail" } },
    ]);
    let patched = ok(&client, "deck_patch", json!({ "bundle": bundle, "ops": ops })).await;
    assert_eq!(patched["applied"], true, "{patched:#}");

    // The headline needs two lines, and its slot is one row tall.
    let found = ok(&client, "deck_lint", json!({ "bundle": bundle, "severity": "error" })).await;
    let findings = found["findings"].as_array().unwrap();
    let overflow = findings.iter().find(|f| f["code"] == "E100" && f["node"] == "headline");
    let overflow = overflow.unwrap_or_else(|| panic!("an E100 on the headline: {found:#}"));
    assert!(overflow.get("fix").is_some(), "with a fix: {overflow:#}");
    assert_eq!(found["errors"], findings.len());

    // Lint applies its fix, checked by laying the state out again, then finds nothing.
    let fixed = ok(&client, "deck_lint", json!({ "bundle": bundle, "fix": true })).await;
    assert!(fixed["fixed"].as_array().unwrap().iter().any(|f| f["code"] == "E100"), "{fixed:#}");
    assert_eq!(fixed["errors"], 0, "{fixed:#}");
    let again = ok(&client, "deck_lint", json!({ "bundle": bundle, "severity": "error" })).await;
    assert_eq!((again["errors"].clone(), again["findings"].clone()), (json!(0), json!([])), "{again:#}");

    let rendered = call(&client, "deck_render", json!({ "bundle": bundle, "state": "start" })).await;
    assert_ne!(rendered.is_error, Some(true), "{:?}", rendered.content);
    let ContentBlock::Image(image) = &rendered.content[0] else { panic!("{:?}", rendered.content) };
    assert_eq!(image.mime_type, "image/png");

    client.cancel().await.unwrap();
    assert!(child.wait().await.unwrap().success());
}
