//! The MCP server, end to end (PLAN 1.17, SPEC §7.2): a client over an in-process pipe
//! builds a deck from a theme and a CSV with the tools alone, checks it, and looks at it;
//! reads the resources; and is told why a tool stopped.

use base64::Engine as _;
use rmcp::model::{
    CacheScope, CallToolRequestParams, CallToolResult, ClientConfig, ContentBlock, ProtocolVersion,
    ReadResourceRequestParams, ResourceContents,
};
use rmcp::service::RunningService;
use rmcp::{ClientLifecycleMode, ClientServiceExt, RoleClient, ServiceExt};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;

const EXAMPLES: &str = "../../docs/examples";

type Client = RunningService<RoleClient, ClientConfig>;

/// A client talking to a server over an in-process pipe.
async fn connect() -> Client {
    connect_as(None).await
}

/// The same, the client giving `name` as its own when it connects.
async fn connect_as(name: Option<&str>) -> Client {
    let mut config = ClientConfig::default();
    if let Some(name) = name {
        config.client_info.name = name.into();
    }
    connect_with(config, ClientLifecycleMode::Initialize).await
}

/// The same, the client connecting as `config` says by `lifecycle`: the `initialize`
/// handshake, or protocol 2026-07-28's discovery, after which it names itself and its
/// protocol on every request.
async fn connect_with(config: ClientConfig, lifecycle: ClientLifecycleMode) -> Client {
    serve(scaena_mcp::Scaena::default(), config, lifecycle).await
}

/// `server`, and a client of it.
async fn serve(server: scaena_mcp::Scaena, config: ClientConfig, lifecycle: ClientLifecycleMode) -> Client {
    let (server_io, client_io) = tokio::io::duplex(1 << 22);
    tokio::spawn(async move {
        let server = server.serve(server_io).await.unwrap();
        server.waiting().await.unwrap();
    });
    config.serve_with_lifecycle(client_io, lifecycle).await.unwrap()
}

/// Discovery at protocol 2026-07-28, with no handshake.
fn discover() -> ClientLifecycleMode {
    ClientLifecycleMode::Discover { preferred_versions: vec![ProtocolVersion::V_2026_07_28] }
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
            "data_edit",
            "deck_create",
            "deck_diff",
            "deck_export",
            "deck_find",
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

/// From protocol 2026-07-28, which a client reaches by discovery (a handshake settles on
/// 2025-11-25 at most), what `resources/list` and `resources/read` return says how long a
/// client may keep it and who may share it, and a client rejects a result that does not
/// (SEP-2549). A client that shook hands gets the shape it knows.
#[tokio::test(flavor = "multi_thread")]
async fn the_resources_say_how_long_to_keep_them() {
    for (lifecycle, hinted) in [(ClientLifecycleMode::Initialize, false), (discover(), true)] {
        let client = connect_with(ClientConfig::default(), lifecycle.clone()).await;
        let server = client.peer_info().and_then(|info| info.server_info.clone()).expect("the server names itself");
        assert_eq!((server.name.as_str(), server.version.as_str()), ("scaena", env!("CARGO_PKG_VERSION")));
        let hints = hinted.then_some((3_600_000, CacheScope::Public));
        let list = client.list_resources(None).await.unwrap();
        assert_eq!(list.ttl_ms.zip(list.cache_scope), hints, "{lifecycle:?}: resources/list");
        let read = client.read_resource(ReadResourceRequestParams::new("scaena://lint/catalog")).await.unwrap();
        assert_eq!(read.ttl_ms.zip(read.cache_scope), hints, "{lifecycle:?}: resources/read");
        client.cancel().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_builds_a_deck_with_the_tools_alone() {
    let client = connect().await;
    let dir = scratch("build").join("q3");
    let bundle = path(&dir);
    let csv = path(&Path::new(EXAMPLES).join("data/q3-revenue.csv"));

    // A theme that ships, by name, with its fonts: the agent needs no theme file (PLAN 2.13).
    let made = ok(&client, "deck_create", json!({ "bundle": bundle, "theme": "dusk", "title": "Q3" })).await;
    assert_eq!(made["created"], true, "{made:#}");
    assert!(made["files"].as_array().unwrap().iter().any(|f| f == "themes/dusk.theme.json"), "{made:#}");
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

    // Found across the deck's texts, and replaced where each is written (PLAN 2.47): the title
    // in `revenue`'s delta. The spine's claim is no text on a slide, and stays.
    let found = ok(&client, "deck_find", json!({ "bundle": bundle, "find": "doubled" })).await;
    assert_eq!(found["matches"], 1, "{found:#}");
    assert_eq!(found["found"][0]["lives"], "/states/1/props/title/text", "{found:#}");
    let replaced = ok(&client, "deck_find", json!({ "bundle": bundle, "find": "doubled", "replace": "grew" })).await;
    assert_eq!(replaced["replaced"]["applied"], true, "{replaced:#}");
    let read = ok(&client, "deck_read", json!({ "bundle": bundle, "scn": true })).await;
    assert!(read["scn"].as_str().unwrap().contains("Revenue grew"), "{}", read["scn"]);
    let spine = ok(&client, "spine_read", json!({ "bundle": bundle })).await;
    assert_eq!(spine["spine"]["sections"][0]["beats"][1]["claim"], "Revenue doubled.");

    // The data, edited in place (PLAN 2.55): the source read as a sheet, a cell set, and the
    // chart reads it so: 2025-Q4's sum is 20.2 + 6.1 + 4.4.
    let read = ok(&client, "data_edit", json!({ "bundle": bundle, "source": "q3" })).await;
    assert_eq!(read["sheet"]["rows"][0], json!(["2025-Q4", "Core", "18.2", "1210"]), "{read:#}");
    let edits = json!([{ "op": "set", "row": 0, "column": "revenue", "value": 20.2 }]);
    let set = ok(&client, "data_edit", json!({ "bundle": bundle, "source": "q3", "edits": edits })).await;
    assert_eq!((set["edited"].clone(), set["file"].clone()), (json!(true), json!("data/q3-revenue.csv")), "{set:#}");
    let inspected = ok(&client, "deck_inspect", json!({ "bundle": bundle, "state": "revenue", "data": true })).await;
    let sum = inspected["states"][0]["data"]["rev"]["rows"][0][1].as_f64().unwrap();
    assert!((sum - 30.7).abs() < 1e-9, "{inspected:#}");
    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tool_that_stops_says_why() {
    let client = connect().await;
    let example = path(&Path::new(EXAMPLES).join("revenue.deck.json"));
    for (tool, args, says) in [
        // A PDF, a video, and a single-file page are files, and PNGs go in a directory: each
        // needs `out`.
        ("deck_export", json!({ "bundle": example, "format": "html" }), json!({})),
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

/// An export that outlives the client's wait keeps going (SPEC §7.2): the call answers
/// that it is running and how far it has got, the same call again waits for the rest, and
/// a different export of the same file is refused until it is done.
#[tokio::test(flavor = "multi_thread")]
async fn a_long_export_keeps_going_while_the_client_asks_after_it() {
    // A server that waits for nothing, so every export outlives its first call.
    let server = scaena_mcp::Scaena::default().with_export_wait(Duration::ZERO);
    let client = serve(server, ClientConfig::default(), ClientLifecycleMode::Initialize).await;
    let dir = scratch("long-export").join("images");
    let (images, deck) = (path(&dir), path(&Path::new(EXAMPLES).join("revenue.deck.json")));
    let png = json!({ "bundle": deck, "format": "png", "out": images });
    let first = ok(&client, "deck_export", png.clone()).await;
    let running = &first["running"];
    assert_eq!((first["out"].as_str(), running["unit"].as_str()), (Some(images.as_str()), Some("images")), "{first:#}");
    assert!(running["of"].as_u64().is_some_and(|of| of > 0) && running["next"].is_string(), "{first:#}");
    assert!(first.get("files").is_none(), "nothing else is said until it is done: {first:#}");
    // Another export of the same file waits its turn.
    let refused = call(&client, "deck_export", json!({ "bundle": deck, "format": "svg", "out": images })).await;
    assert_eq!(refused.is_error, Some(true));
    assert!(text(&refused).contains("is running"), "{}", text(&refused));
    // Asked again, it says how it is going until it is done, then gives what it wrote.
    let mut done = None;
    for _ in 0..2400 {
        let again = ok(&client, "deck_export", png.clone()).await;
        if again.get("running").is_none() {
            done = Some(again);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let done = done.expect("the export finishes");
    let files = done["files"].as_array().expect("the images it wrote");
    assert_eq!(Some(files.len()), done["pages"].as_array().map(Vec::len));
    assert_eq!(dir.read_dir().unwrap().count(), files.len());
    // Handed back, it is done with: the next export of the file starts afresh.
    let svg = ok(&client, "deck_export", json!({ "bundle": deck, "format": "svg", "out": images })).await;
    assert!(svg["running"]["of"].as_u64().is_some_and(|of| of as usize == files.len()), "{svg:#}");
    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agents_edits_are_its_own_in_a_bundles_history() {
    let bundle = scratch("history").join("q3");
    let opts = scaena_store::SaveOptions { subset_fonts: false, now: "2026-10-03T00:00:00Z".into(), history: true };
    scaena_ops::open(&Path::new(EXAMPLES).join("revenue.deck.json")).unwrap().save(&bundle, &opts).unwrap();
    // A client that shakes hands names itself once; one that discovers, on every request.
    for (name, lifecycle, title) in
        [("claude-test", ClientLifecycleMode::Initialize, "Q3, in full"), ("claude-next", discover(), "Q3, again")]
    {
        let mut config = ClientConfig::default();
        config.client_info.name = name.into();
        let client = connect_with(config, lifecycle).await;
        let ops = json!([{ "op": "set_text", "node": "title", "text": title }]);
        ok(&client, "deck_patch", json!({ "bundle": path(&bundle), "ops": ops })).await;
        let doc = scaena_ops::open(&bundle).unwrap().history().unwrap().expect("it keeps history");
        let last = doc.changes().pop().unwrap();
        let author = format!("agent:{name}");
        assert_eq!((last.author.as_deref(), last.message.as_deref()), (Some(&*author), Some("patch: set_text")));
        client.cancel().await.unwrap();
    }
}
