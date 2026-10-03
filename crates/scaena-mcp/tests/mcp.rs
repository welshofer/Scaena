//! The MCP server, end to end (PLAN 1.17, SPEC §7.2): a client over an in-process pipe
//! builds a deck from a theme and a CSV with the tools alone, checks it, and looks at it;
//! reads the resources; and is told why a tool stopped.

use base64::Engine as _;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ClientConfig, ContentBlock, ReadResourceRequestParams, ResourceContents,
};
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const EXAMPLES: &str = "../../docs/examples";

type Client = RunningService<RoleClient, ClientConfig>;

/// A client talking to a server over an in-process pipe.
async fn connect() -> Client {
    let (server_io, client_io) = tokio::io::duplex(1 << 22);
    tokio::spawn(async move {
        let server = scaena_mcp::Scaena::default().serve(server_io).await.unwrap();
        server.waiting().await.unwrap();
    });
    ClientConfig::default().serve(client_io).await.unwrap()
}

async fn call(client: &Client, tool: &str, args: Value) -> CallToolResult {
    let args = args.as_object().unwrap().clone();
    client.call_tool(CallToolRequestParams::new(tool.to_string()).with_arguments(args)).await.unwrap()
}

/// A tool's structured result, which must not be an error.
async fn ok(client: &Client, tool: &str, args: Value) -> Value {
    let result = call(client, tool, args.clone()).await;
    assert_ne!(result.is_error, Some(true), "{tool} {args}: {:?}", text(&result));
    result.structured_content.unwrap_or_else(|| panic!("{tool}: no structured content"))
}

fn text(result: &CallToolResult) -> String {
    result.content.iter().filter_map(|c| c.as_text().map(|t| t.text.clone())).collect()
}

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("mcp-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn path(p: &Path) -> String {
    p.to_str().unwrap().to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_tools_and_resources_are_listed() {
    let client = connect().await;
    let mut tools: Vec<String> = client.list_all_tools().await.unwrap().iter().map(|t| t.name.to_string()).collect();
    tools.sort();
    assert_eq!(
        tools,
        [
            "data_attach",
            "deck_create",
            "deck_diff",
            "deck_export",
            "deck_inspect",
            "deck_lint",
            "deck_patch",
            "deck_read",
            "deck_render",
            "spine_read",
            "spine_update",
            "theme_apply"
        ]
    );
    let resources: Vec<String> = client.list_all_resources().await.unwrap().iter().map(|r| r.uri.clone()).collect();
    for uri in [
        "scaena://schema/deck",
        "scaena://schema/patch",
        "scaena://schema/spine",
        "scaena://lint/catalog",
        "scaena://examples/revenue.deck.json",
        "scaena://examples/trails.deck.json",
    ] {
        assert!(resources.iter().any(|r| r == uri), "{uri}: {resources:?}");
    }
    let read = |uri: &str| {
        let client = &client;
        let uri = uri.to_string();
        async move {
            let result = client.read_resource(ReadResourceRequestParams::new(uri)).await.unwrap();
            match &result.contents[0] {
                ResourceContents::TextResourceContents { text, .. } => text.clone(),
                other => panic!("{other:?}"),
            }
        }
    };
    let catalog = read("scaena://lint/catalog").await;
    assert!(catalog.starts_with("### 7.5") && catalog.contains("| E100 |") && !catalog.contains("### 7.6"));
    let patch: Value = serde_json::from_str(&read("scaena://schema/patch").await).unwrap();
    assert_eq!(patch["title"], "Scaena patch");
    let spine: Value = serde_json::from_str(&read("scaena://schema/spine").await).unwrap();
    assert_eq!(spine["title"], "Scaena spine projection");
    assert!(client.read_resource(ReadResourceRequestParams::new("scaena://nothing")).await.is_err());
    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_builds_a_deck_with_the_tools_alone() {
    let client = connect().await;
    let dir = scratch("build").join("q3");
    let bundle = path(&dir);
    let theme = path(&Path::new(EXAMPLES).join("themes/dusk.theme.json"));
    let csv = path(&Path::new(EXAMPLES).join("data/q3-revenue.csv"));

    let made = ok(&client, "deck_create", json!({ "bundle": bundle, "theme": theme, "title": "Q3" })).await;
    assert_eq!(made["created"], true, "{made:#}");
    let attached = ok(&client, "data_attach", json!({ "bundle": bundle, "id": "q3", "file": csv })).await;
    assert_eq!(attached["schema"]["revenue"], "number");

    // A title slide, then the chart, then the claim: one patch.
    let ops = json!([
        { "op": "add_node", "id": "title", "state": "start",
          "node": { "type": "text", "role": "display", "text": "Q3 Review", "semantic": "navigation", "at": { "in": "title" } } },
        { "op": "set_prop", "node": "title", "prop": "z", "value": 1 },
        { "op": "add", "path": "/states/0/layout", "value": "title" },
        { "op": "add_state", "after": "start", "state": {
            "id": "revenue", "layout": "figure", "transition": { "duration": "standard" },
            "props": { "title": { "text": "Revenue doubled", "role": "headline", "semantic": "claim", "at": { "in": "header" } } } } },
        { "op": "add_node", "id": "rev", "state": "revenue",
          "node": { "type": "chart", "kind": "bar", "data": "@q3", "key": "quarter",
                    "dataTransform": [{ "aggregate": { "revenue": "sum(revenue)" }, "groupby": ["quarter"] }],
                    "x": { "field": "quarter", "type": "ordinal" },
                    "y": { "field": "revenue", "type": "quantitative", "title": "Revenue ($M)" },
                    "alt": "Revenue by quarter, Q4 2025 through Q3 2026.", "semantic": "evidence", "at": { "in": "main" } } },
    ]);
    let patched = ok(&client, "deck_patch", json!({ "bundle": bundle, "ops": ops })).await;
    assert_eq!(patched["applied"], true, "{patched:#}");
    let spine = json!({ "sections": [{ "id": "q3", "beats": [
        { "id": "opening", "claim": "Q3 in review.", "states": ["start"] },
        { "id": "doubled", "claim": "Revenue doubled.", "evidence": ["@q3"], "states": ["revenue"] }] }] });
    let updated = ok(&client, "spine_update", json!({ "bundle": bundle, "spine": spine })).await;
    assert_eq!(updated["applied"], true, "{updated:#}");

    // Checked, then seen.
    let lint = ok(&client, "deck_lint", json!({ "bundle": bundle, "severity": "error" })).await;
    assert_eq!(lint["errors"], 0, "{lint:#}");
    let rendered = call(&client, "deck_render", json!({ "bundle": bundle, "state": "revenue" })).await;
    assert_ne!(rendered.is_error, Some(true), "{}", text(&rendered));
    let ContentBlock::Image(image) = &rendered.content[0] else { panic!("{:?}", rendered.content) };
    assert_eq!(image.mime_type, "image/png");
    let png = base64::engine::general_purpose::STANDARD.decode(&image.data).unwrap();
    assert!(png.starts_with(b"\x89PNG"));
    let facts: Value = serde_json::from_str(&text(&rendered)).unwrap();
    assert_eq!((facts["size"].clone(), facts["digest"].as_str().map(str::len)), (json!([1920, 1080]), Some(16)));
    // The same state renders to the same drawing.
    let again = call(&client, "deck_render", json!({ "bundle": bundle, "state": "revenue" })).await;
    let again: Value = serde_json::from_str(&text(&again)).unwrap();
    assert_eq!(again["digest"], facts["digest"]);

    let inspected = ok(&client, "deck_inspect", json!({ "bundle": bundle, "state": "revenue", "data": true })).await;
    assert_eq!(inspected["states"][0]["data"]["rev"]["rows"].as_array().unwrap().len(), 4, "{inspected:#}");
    let diff = ok(&client, "deck_diff", json!({ "bundle": bundle, "from": "start", "to": "revenue" })).await;
    assert!(diff["changes"]["rev"].get("enter").is_some(), "{diff:#}");
    let spine = ok(&client, "spine_read", json!({ "bundle": bundle })).await;
    assert_eq!(spine["spine"]["sections"][0]["beats"][1]["id"], "doubled");
    let read = ok(&client, "deck_read", json!({ "bundle": bundle, "scn": true })).await;
    assert!(read["scn"].as_str().unwrap().contains("Revenue doubled"));
    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tool_that_stops_says_why() {
    let client = connect().await;
    let example = path(&Path::new(EXAMPLES).join("revenue.deck.json"));
    for (tool, args, says) in [
        ("deck_export", json!({ "bundle": example, "format": "html" }), json!({ "plan": "2.5" })),
        // A PDF and a video are files, and PNGs go in a directory: each needs `out`.
        ("deck_export", json!({ "bundle": example, "format": "pdf" }), json!({})),
        ("deck_export", json!({ "bundle": example, "format": "mp4" }), json!({})),
        ("deck_export", json!({ "bundle": example, "format": "png" }), json!({})),
        (
            "deck_patch",
            json!({ "bundle": example, "dry_run": true, "ops": [{ "op": "remove_node", "id": "nobody" }] }),
            json!({ "op": 0 }),
        ),
        ("deck_lint", json!({ "bundle": "no/such/bundle" }), json!({})),
    ] {
        let result = call(&client, tool, args).await;
        assert_eq!(result.is_error, Some(true), "{tool}");
        let failure: Value = serde_json::from_str(&text(&result)).unwrap();
        assert!(failure["message"].as_str().is_some_and(|m| !m.is_empty()), "{failure:#}");
        for (k, v) in says.as_object().unwrap() {
            assert_eq!(&failure[k], v, "{tool}: {failure:#}");
        }
    }
    client.cancel().await.unwrap();
}
