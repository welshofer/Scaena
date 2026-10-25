//! `scaena mcp` (PLAN 1.17): the MCP server on stdio, as an agent's client starts it.

use rmcp::RoleClient;
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult, ClientConfig, ContentBlock};
use rmcp::service::RunningService;
use serde_json::{Value, json};
use std::path::Path;
use std::process::Stdio;

type Client = RunningService<RoleClient, ClientConfig>;

/// `scaena mcp` as a child process started in the repository, as an agent's client starts it
/// in a project (a tool reads a theme only under it, SPEC §7.2), and a client talking to it
/// on its stdio.
async fn serve() -> (Client, tokio::process::Child) {
    serve_in(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).await
}

/// `scaena mcp` started in `dir`, the project a test makes its bundles in: the server writes
/// only under the folder it works in, which `CARGO_TARGET_TMPDIR` is not, wherever the target
/// is kept (SPEC §7.2).
async fn serve_in(dir: &Path) -> (Client, tokio::process::Child) {
    std::fs::create_dir_all(dir).unwrap();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_scaena"))
        .arg("mcp")
        .current_dir(dir)
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
    assert_eq!(client.list_all_tools().await.unwrap().len(), 24);
    let spine = ok(&client, "spine_read", json!({ "bundle": "docs/examples/revenue.deck.json" })).await;
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
    let (client, mut child) = serve_in(&dir).await;

    let made = ok(&client, "deck_create", json!({ "bundle": bundle, "theme": "dusk", "title": "Loop" })).await;
    assert_eq!(made["created"], true, "{made:#}");
    let ops = json!([
        { "op": "add", "path": "/states/0/layout", "value": "agenda-photo" },
        { "op": "add_node", "id": "headline", "state": "start",
          "node": { "type": "text", "role": "headline", "semantic": "claim", "at": { "in": "header" },
                    "text": "Volunteers rebuilt fifty-two miles of trail across the state park" } },
    ]);
    let patched = ok(&client, "deck_patch", json!({ "bundle": bundle, "ops": ops })).await;
    assert_eq!(patched["applied"], true, "{patched:#}");

    // The headline needs three lines, and its slot holds two: shrunk, it takes two.
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

/// `deck_lint` with a `state` lays out that state alone and fixes only what it finds there, its
/// dates judged at `now` with `fix` too; `deck_health` reads what lint last found in the whole
/// deck, and lints again once the deck has changed (lift-up 20261025b PERF-3).
#[tokio::test(flavor = "multi_thread")]
async fn deck_lint_scoped_to_a_state_and_health_reading_what_it_found() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("mcp-scoped-lint");
    let _ = std::fs::remove_dir_all(&dir);
    let bundle = dir.join("deck").to_str().unwrap().to_string();
    let (client, mut child) = serve_in(&dir).await;

    let made = ok(&client, "deck_create", json!({ "bundle": bundle, "theme": "dusk", "title": "Loop" })).await;
    assert_eq!(made["created"], true, "{made:#}");
    let ops = json!([
        { "op": "add", "path": "/states/0/layout", "value": "agenda-photo" },
        { "op": "add_node", "id": "headline", "state": "start",
          "node": { "type": "text", "role": "headline", "semantic": "claim", "at": { "in": "header" },
                    "text": "Volunteers rebuilt fifty-two miles of trail across the state park" } },
        { "op": "add_state", "after": "start", "state": { "id": "two", "layout": "agenda-photo" } },
    ]);
    let patched = ok(&client, "deck_patch", json!({ "bundle": bundle, "ops": ops })).await;
    assert_eq!(patched["applied"], true, "{patched:#}");

    let overflows = |found: &Value| -> Vec<String> {
        let findings = found["findings"].as_array().unwrap();
        (findings.iter().filter(|f| f["code"] == "E100" && f["node"] == "headline"))
            .map(|f| f["state"].as_str().unwrap_or_default().to_string())
            .collect()
    };
    let whole = ok(&client, "deck_lint", json!({ "bundle": bundle, "severity": "error" })).await;
    eprintln!("whole: {:?}", overflows(&whole));
    let health = ok(&client, "deck_health", json!({ "bundle": bundle })).await;
    assert_eq!(health["findings"][0]["kind"], "layout", "{health:#}");
    // Scoped to `two`, laid out alone: the overflow that holds there from `start`, said at it.
    let two = ok(&client, "deck_lint", json!({ "bundle": bundle, "severity": "error", "state": "two" })).await;
    assert_eq!(overflows(&two), ["two"], "{two:#}");
    assert!(two["findings"].as_array().unwrap().iter().all(|f| f["state"] == "two"), "{two:#}");

    // A fix scoped to a state, its date judged too, fixes only what it finds there.
    let fixed = ok(
        &client,
        "deck_lint",
        json!({ "bundle": bundle, "fix": true, "state": "start", "now": "2026-10-15" }),
    )
    .await;
    let fixed_in: Vec<&str> = fixed["fixed"].as_array().unwrap().iter().filter_map(|f| f["state"].as_str()).collect();
    assert!(!fixed_in.is_empty() && fixed_in.iter().all(|s| *s == "start"), "{fixed:#}");
    assert_eq!(fixed["errors"], 0, "{fixed:#}");
    // The deck changed, so health lints it again, and finds no slide broken.
    let health = ok(&client, "deck_health", json!({ "bundle": bundle })).await;
    let broken = health["findings"].as_array().unwrap().iter().filter(|f| f["kind"] == "layout").count();
    let again = ok(&client, "deck_lint", json!({ "bundle": bundle, "severity": "error" })).await;
    assert_eq!((broken, again["errors"].clone()), (0, json!(0)), "{health:#}\n{again:#}");

    // A data file changed on disk, and the deck not: lint reads the bundle again, not what it
    // kept of it.
    let csv = dir.join("facts.csv");
    std::fs::write(&csv, "quarter,revenue\nQ1,10\nQ2,12\n").unwrap();
    let attached = ok(&client, "data_attach", json!({ "bundle": bundle, "id": "facts", "file": csv })).await;
    assert_eq!(attached["schema"]["revenue"], "number", "{attached:#}");
    let before = ok(&client, "deck_lint", json!({ "bundle": bundle })).await;
    let deck = std::fs::read(Path::new(&bundle).join("deck.json")).unwrap();
    let file = attached["source"].as_str().unwrap();
    std::fs::write(Path::new(&bundle).join(file), "quarter,revenue\nQ1,ten\nQ2,12\n").unwrap();
    let after = ok(&client, "deck_lint", json!({ "bundle": bundle })).await;
    assert_eq!(std::fs::read(Path::new(&bundle).join("deck.json")).unwrap(), deck);
    eprintln!("before: {before:#}\nafter: {after:#}");
    assert_ne!(after["findings"], before["findings"], "{after:#}");

    client.cancel().await.unwrap();
    assert!(child.wait().await.unwrap().success());
}

/// `deck_create` takes every theme in the registry by its name, offline, with its fonts alone
/// (PLAN T.0c): the server carries every family in its binary.
#[tokio::test(flavor = "multi_thread")]
async fn deck_create_takes_every_theme_in_the_registry_by_its_name() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let registry: Value =
        serde_json::from_slice(&std::fs::read(root.join("docs/themes/registry.json")).unwrap()).unwrap();
    let made_in = Path::new(env!("CARGO_TARGET_TMPDIR")).join("mcp-registry");
    let _ = std::fs::remove_dir_all(&made_in);
    let (client, _child) = serve_in(&made_in).await;
    for theme in registry["themes"].as_array().unwrap() {
        let slug = theme["slug"].as_str().unwrap();
        let dir = made_in.join(slug);
        let _ = std::fs::remove_dir_all(&dir);
        let made = ok(&client, "deck_create", json!({ "bundle": dir, "theme": slug, "title": "Field notes" })).await;
        assert_eq!(made["errors"], 0, "{slug}: {made:#}");
        let file = Path::new(theme["files"][0].as_str().unwrap()).file_name().unwrap();
        let named = std::fs::read_to_string(dir.join("themes").join(file)).unwrap();
        let named: Value = serde_json::from_str(&named).unwrap();
        for family in named["type"]["families"].as_object().unwrap().values() {
            for file in [&family["file"], &family["italic"]["file"]].into_iter().filter_map(Value::as_str) {
                assert!(dir.join(file).is_file(), "{slug}: {file}");
            }
        }
    }
    client.cancel().await.unwrap();
}
