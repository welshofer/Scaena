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

/// A client of a server working in `dir`, as an agent's is started in its project's folder: a
/// new bundle goes only where the server writes.
async fn connect_in(dir: &Path) -> Client {
    serve(
        scaena_mcp::Scaena::default().with_dir(dir.to_path_buf()),
        ClientConfig::default(),
        ClientLifecycleMode::Initialize,
    )
    .await
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
            "background_set",
            "data_attach",
            "data_edit",
            "deck_create",
            "deck_diff",
            "deck_export",
            "deck_find",
            "deck_generate",
            "deck_health",
            "deck_history",
            "deck_inspect",
            "deck_lint",
            "deck_patch",
            "deck_read",
            "deck_render",
            "icon_find",
            "map_fetch",
            "spine_read",
            "spine_update",
            "state_start",
            "theme_apply",
            "theme_edit"
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
    let root = scratch("build");
    let client = connect_in(&root).await;
    let dir = root.join("q3");
    let bundle = path(&dir);
    // The data file beside the bundle: a tool reads only there or under its own folder.
    let csv = root.join("q3-revenue.csv");
    std::fs::copy(Path::new(EXAMPLES).join("data/q3-revenue.csv"), &csv).unwrap();
    let csv = path(&csv);

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
    // At half the canvas, for a model to see; the whole canvas only where `size` asks for it.
    assert_eq!((facts["size"].clone(), facts["digest"].as_str().map(str::len)), (json!([960, 540]), Some(16)));
    let whole =
        call(&client, "deck_render", json!({ "bundle": bundle, "state": "revenue", "size": "1920x1080" })).await;
    let whole: Value = serde_json::from_str(&text(&whole)).unwrap();
    assert_eq!((whole["size"].clone(), &whole["digest"]), (json!([1920, 1080]), &facts["digest"]));
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
    // Its story (PLAN G.9): two slides, each its beat's claim; coherence read by words, unscored.
    let health = ok(&client, "deck_health", json!({ "bundle": bundle })).await;
    let titles: Vec<&str> = health["slides"].as_array().unwrap().iter().filter_map(|s| s["title"].as_str()).collect();
    assert_eq!(titles, ["Q3 in review.", "Revenue doubled."], "{health:#}");
    assert!(health["coherence"]["score"].is_null() && health["score"].as_u64().is_some(), "{health:#}");
    // The icons that ship (PLAN 2.104): found by words, each its path and the view it is drawn on.
    let icons = ok(&client, "icon_find", json!({ "query": "rocket", "weight": "fill", "limit": 2 })).await;
    assert_eq!(icons["icons"][0]["name"], "rocket", "{icons:#}");
    assert_eq!(icons["icons"][0]["icon"]["view"], json!([0.0, 0.0, 256.0, 256.0]), "{icons:#}");
    assert!(icons["icons"].as_array().unwrap().len() == 2 && icons["found"].as_u64() >= Some(2), "{icons:#}");

    // Found across the deck's words, and replaced where each is written (PLAN 2.47, 2.83): the
    // beat's claim in the spine, and the title in `revenue`'s delta.
    let found = ok(&client, "deck_find", json!({ "bundle": bundle, "find": "doubled" })).await;
    assert_eq!(found["matches"], 2, "{found:#}");
    assert_eq!(found["found"][0]["lives"], "/spine/sections/0/beats/1/claim", "{found:#}");
    assert_eq!(found["found"][1]["lives"], "/states/1/props/title/text", "{found:#}");
    let replaced = ok(&client, "deck_find", json!({ "bundle": bundle, "find": "doubled", "replace": "grew" })).await;
    assert_eq!(replaced["replaced"]["applied"], true, "{replaced:#}");
    let read = ok(&client, "deck_read", json!({ "bundle": bundle, "scn": true })).await;
    assert!(read["scn"].as_str().unwrap().contains("Revenue grew"), "{}", read["scn"]);
    let spine = ok(&client, "spine_read", json!({ "bundle": bundle })).await;
    assert_eq!(spine["spine"]["sections"][0]["beats"][1]["claim"], "Revenue grew.");

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

    // The theme, edited (ADR-0016): the bundle's copy of Dusk, its paper and its body size, and
    // the slide drawn in it; a role the deck uses taken out is refused, and nothing is written.
    let before = call(&client, "deck_render", json!({ "bundle": bundle, "state": "revenue" })).await;
    let before: Value = serde_json::from_str(&text(&before)).unwrap();
    let ops = json!([
        { "op": "replace", "path": "/tokens/color/paper", "value": "#0B0B10" },
        { "op": "replace", "path": "/type/roles/body/size", "value": 30 },
    ]);
    let edited = ok(&client, "theme_edit", json!({ "bundle": bundle, "ops": ops })).await;
    assert_eq!(edited["theme"], "themes/dusk.theme.json", "{edited:#}");
    assert_eq!(edited["paths"], json!(["/tokens/color/paper", "/type/roles/body/size"]), "{edited:#}");
    assert_eq!((edited["applied"].clone(), edited["refused"].clone()), (json!(true), json!(false)), "{edited:#}");
    let held = || std::fs::read_to_string(dir.join("themes/dusk.theme.json")).unwrap();
    let theme: Value = serde_json::from_str(&held()).unwrap();
    assert_eq!(theme["tokens"]["color"]["paper"], "#0B0B10");
    assert_eq!(theme["type"]["roles"]["body"]["size"], 30);
    let after = call(&client, "deck_render", json!({ "bundle": bundle, "state": "revenue" })).await;
    let after: Value = serde_json::from_str(&text(&after)).unwrap();
    assert_ne!(after["digest"], before["digest"], "the deck is drawn in the theme as edited");
    let kept = held();
    let gone = json!([{ "op": "remove", "path": "/type/roles/headline" }]);
    let refused = ok(&client, "theme_edit", json!({ "bundle": bundle, "ops": gone })).await;
    assert_eq!((refused["refused"].clone(), refused["applied"].clone()), (json!(true), json!(false)), "{refused:#}");
    assert!(refused["added"].as_array().unwrap().iter().any(|f| f["code"] == "E102"), "{refused:#}");
    assert_eq!(held(), kept, "a refused edit writes nothing");
    client.cancel().await.unwrap();
}

/// The operations the editors have, as tools (narrative FUNC-5): a slide started in a theme
/// layout as Add Slide starts one, a slide's background set and taken away as the Format tab
/// does, and a deck made from slides written in Narrato's JSON, as `scaena generate --slides`
/// makes one, with no model called.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_starts_a_slide_sets_a_background_and_builds_from_written_slides() {
    let root = scratch("editors-ops");
    let client = connect_in(&root).await;
    let dir = root.join("q3");
    let bundle = path(&dir);
    ok(&client, "deck_create", json!({ "bundle": bundle, "theme": "dusk", "title": "Q3" })).await;

    // A slide started in Bullets after the first slide: a new absolute state in the layout, a
    // text in each slot that holds words, its words the slot's prompt; a dry run writes nothing.
    let dry =
        ok(&client, "state_start", json!({ "bundle": bundle, "state": "start", "layout": "bullets", "dry_run": true }))
            .await;
    assert_eq!((dry["applied"].clone(), dry["id"].clone()), (json!(false), json!("bullets")), "{dry:#}");
    let deck = || -> Value { serde_json::from_str(&std::fs::read_to_string(dir.join("deck.json")).unwrap()).unwrap() };
    assert_eq!(deck()["states"].as_array().unwrap().len(), 1, "a dry run writes nothing");
    let started = ok(&client, "state_start", json!({ "bundle": bundle, "state": "start", "layout": "bullets" })).await;
    assert_eq!((started["applied"].clone(), started["id"].clone()), (json!(true), json!("bullets")), "{started:#}");
    assert_eq!(started["errors"], 0, "{started:#}");
    let d = deck();
    assert_eq!(d["states"][1]["id"], "bullets");
    assert_eq!(
        (d["states"][1]["layout"].clone(), d["states"][1]["mode"].clone()),
        (json!("bullets"), json!("absolute"))
    );
    let texts = d["nodes"].as_object().unwrap().values().filter(|n| n["type"] == "text").count();
    assert!(texts >= 2, "a text in each slot that holds words: {d:#}");
    // A layout the theme lacks stops it, saying so.
    let lacks = call(&client, "state_start", json!({ "bundle": bundle, "state": "start", "layout": "nowhere" })).await;
    assert_eq!(lacks.is_error, Some(true));
    assert!(text(&lacks).contains("no layout `nowhere`"), "{}", text(&lacks));

    // The new slide's background: Texture, kept to its slide; then taken away.
    let set = ok(&client, "background_set", json!({ "bundle": bundle, "state": "bullets", "preset": "texture" })).await;
    assert_eq!((set["applied"].clone(), set["errors"].clone()), (json!(true), json!(0)), "{set:#}");
    let shaders = |d: &Value, state: usize| -> Vec<String> {
        let ids: Vec<String> = d["nodes"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(_, n)| n["type"] == "shader")
            .map(|(id, _)| id.clone())
            .collect();
        let snaps = scaena_core::resolve_states(&scaena_core::Deck::from_value(d).unwrap()).unwrap();
        ids.into_iter().filter(|id| snaps[state].nodes.contains_key(id)).collect()
    };
    let d = deck();
    assert_eq!(shaders(&d, 1), ["bullets-background"], "{d:#}");
    assert!(shaders(&d, 0).is_empty(), "it stays on its slide");
    assert_eq!(d["nodes"]["bullets-background"]["preset"], "texture");
    let gone = ok(&client, "background_set", json!({ "bundle": bundle, "state": "bullets" })).await;
    assert_eq!(gone["applied"], true, "{gone:#}");
    assert!(shaders(&deck(), 1).is_empty());
    let lacks =
        call(&client, "background_set", json!({ "bundle": bundle, "state": "bullets", "preset": "nowhere" })).await;
    assert_eq!(lacks.is_error, Some(true));
    assert!(text(&lacks).contains("no shader preset `nowhere`"), "{}", text(&lacks));

    // A deck from slides written elsewhere, with no model called and no key: the arc chosen for
    // the brief, each slide in a theme layout, and the fields health reads.
    let slides: Value =
        serde_json::from_str(&std::fs::read_to_string("../scaena-narrato/tests/answers/catalog.json").unwrap())
            .unwrap();
    let made_dir = scratch("editors-ops").join("tidewater");
    let made = ok(
        &client,
        "deck_generate",
        json!({ "bundle": path(&made_dir), "slides": slides, "theme": "ember", "topic": "Tidewater's Series A",
                "audience": "Investors", "goal": "Persuade", "arc": "Problem–Solution" }),
    )
    .await;
    assert_eq!(
        (made["created"]["created"].clone(), made["created"]["errors"].clone()),
        (json!(true), json!(0)),
        "{made:#}"
    );
    assert_eq!(made["arc"], "Problem–Solution", "{made:#}");
    assert_eq!(made["layouts"].as_array().unwrap().len(), 56, "{made:#}");
    let d: Value = serde_json::from_str(&std::fs::read_to_string(made_dir.join("deck.json")).unwrap()).unwrap();
    assert_eq!(d["theme"], "themes/ember.theme.json");
    assert_eq!(d["meta"]["narrative"]["generator"], "narrato");
    let health = ok(&client, "deck_health", json!({ "bundle": path(&made_dir) })).await;
    assert_eq!(health["arc"], "Problem–Solution", "{health:#}");
    assert_eq!(health["slides"].as_array().unwrap().len(), 56, "{health:#}");
    // A place taken, and an arc Narrato lacks, are refused before anything is made.
    let taken = call(&client, "deck_generate", json!({ "bundle": path(&made_dir), "slides": slides })).await;
    assert!(text(&taken).contains("not an empty directory"), "{}", text(&taken));
    let no_arc = call(
        &client,
        "deck_generate",
        json!({ "bundle": path(&scratch("editors-ops").join("none")), "slides": slides, "arc": "Spiral" }),
    )
    .await;
    assert!(text(&no_arc).contains("no arc of Narrato's"), "{}", text(&no_arc));
    client.cancel().await.unwrap();
}

/// A theme that ships, by its name, as deck_create takes one (PLAN T.0d): copied into the bundle
/// with its fonts, and Salon by `gallery`, the name it shipped under, too.
#[tokio::test(flavor = "multi_thread")]
async fn a_theme_that_ships_applies_by_its_name() {
    let root = scratch("theme-by-name");
    let client = connect_in(&root).await;
    let dir = root.join("deck");
    let bundle = path(&dir);
    ok(&client, "deck_create", json!({ "bundle": bundle, "theme": "dusk", "title": "Q3" })).await;
    let named = |dir: &Path| -> Value {
        serde_json::from_str::<Value>(&std::fs::read_to_string(dir.join("deck.json")).unwrap()).unwrap()["theme"]
            .clone()
    };
    for (name, file) in [
        ("Salon", "themes/salon.theme.json"),
        ("ember", "themes/ember.theme.json"),
        ("gallery", "themes/salon.theme.json"),
    ] {
        let themed = ok(&client, "theme_apply", json!({ "bundle": bundle, "theme": name })).await;
        assert_eq!(themed["applied"], true, "{name}: {themed:#}");
        assert_eq!(named(&dir), json!(file), "{name}");
    }
    assert!(dir.join("fonts/InstrumentSerif-Regular.ttf").is_file(), "Salon's display family, copied in");
    let none = call(&client, "theme_apply", json!({ "bundle": bundle, "theme": "sepia" })).await;
    assert_eq!(none.is_error, Some(true));
    assert!(text(&none).contains("nor is it a theme that ships: dusk, daybreak, ember, salon"), "{}", text(&none));
    client.cancel().await.unwrap();
}

/// A theme's colors from a photo the bundle holds (PLAN 2.94): what the photo's colors are, and
/// each of the theme's as it would be, with nothing written under a dry run.
#[tokio::test(flavor = "multi_thread")]
async fn a_theme_edit_takes_a_photos_colors() {
    let client = connect().await;
    let example = path(&Path::new(EXAMPLES).join("revenue.deck.json"));
    let args = json!({ "bundle": example, "photo": "assets/trails-ridge.png", "dry_run": true });
    let edited = ok(&client, "theme_edit", args).await;
    assert_eq!((edited["applied"].clone(), edited["refused"].clone()), (json!(false), json!(false)), "{edited:#}");
    let hue = edited["photo"]["read"]["hues"][0]["degrees"].as_f64().unwrap();
    assert!((hue - 303.4).abs() < 0.1, "the ridge's violet: {edited:#}");
    let accent =
        edited["photo"]["set"].as_array().unwrap().iter().find(|s| s["path"] == "/tokens/color/accent").cloned();
    assert_eq!(accent.map(|s| s["now"].clone()), Some(json!("#B97CFF")), "{edited:#}");
    assert!(edited["paths"].as_array().unwrap().contains(&json!("/tokens/color/accent")), "{edited:#}");
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
        // A theme edit whose op does not apply says which.
        (
            "theme_edit",
            json!({ "bundle": example, "dry_run": true, "ops": [{ "op": "replace", "path": "/no/such", "value": 1 }] }),
            json!({ "op": 0 }),
        ),
        // A theme edit is ops or a photo, not both (PLAN 2.94).
        (
            "theme_edit",
            json!({ "bundle": example, "dry_run": true, "photo": "assets/trails-ridge.png",
                    "ops": [{ "op": "replace", "path": "/grid/gutter", "value": 32 }] }),
            json!({}),
        ),
        // A bundle that keeps no history, and one thing asked of a history at a time.
        ("deck_history", json!({ "bundle": example }), json!({})),
        ("deck_history", json!({ "bundle": example, "at": "1", "restore": "1" }), json!({})),
    ] {
        let result = call(&client, tool, args).await;
        assert_eq!(result.is_error, Some(true), "{tool}");
        let failure: Value = serde_json::from_str(&text(&result)).unwrap();
        assert!(failure["message"].as_str().is_some_and(|m| !m.is_empty()), "{failure:#}");
        if tool == "deck_history" {
            let message = failure["message"].as_str().unwrap();
            // It names what an agent can call to begin one (dogfood pass 3, item 4).
            assert!(message.contains("`deck_history` with `begin`") || message.contains("one of"), "{message}");
        }
        for (k, v) in says.as_object().unwrap() {
            assert_eq!(&failure[k], v, "{tool}: {failure:#}");
        }
    }
    client.cancel().await.unwrap();
}

/// A tool writes `out` only under the server's folder or the bundle's (lift-up SEC-1).
#[tokio::test(flavor = "multi_thread")]
async fn a_tool_writes_only_under_its_folder_or_beside_the_bundle() {
    // What an agent read may tell it where to write: the server writes `out` under its own
    // folder or the bundle's, read through every link, and nowhere else.
    let here = scratch("confined");
    let elsewhere = scratch("confined-elsewhere");
    let server = scaena_mcp::Scaena::default().with_dir(here.clone());
    let client = serve(server, ClientConfig::default(), ClientLifecycleMode::Initialize).await;
    let deck = path(&Path::new(EXAMPLES).join("revenue.deck.json"));
    let outside = path(&elsewhere.join("keys"));
    #[cfg(unix)]
    std::os::unix::fs::symlink(&elsewhere, here.join("link")).unwrap();
    let mut refused = vec![outside.clone(), path(&here.join("../confined-elsewhere/keys")), "/".into()];
    if cfg!(unix) {
        refused.push(path(&here.join("link/keys")));
    }
    for out in &refused {
        let render = call(&client, "deck_render", json!({ "bundle": deck, "state": "revenue", "out": out })).await;
        assert_eq!(render.is_error, Some(true), "{out}");
        assert!(text(&render).contains("is outside where this server writes"), "{}", text(&render));
        let export = call(&client, "deck_export", json!({ "bundle": deck, "format": "pdf", "out": out })).await;
        assert_eq!(export.is_error, Some(true), "{out}");
        assert!(text(&export).contains("is outside where this server writes"), "{}", text(&export));
    }
    assert_eq!(elsewhere.read_dir().unwrap().count(), 0, "nothing was written outside");
    // Nor is a file outside them read into a bundle, whatever it is.
    let bundle = path(&here.join("q3"));
    ok(&client, "deck_create", json!({ "bundle": bundle, "theme": "dusk", "title": "Q3" })).await;
    std::fs::write(elsewhere.join("credentials"), "aws_secret_access_key\nAKIA0000\n").unwrap();
    let read = call(
        &client,
        "data_attach",
        json!({ "bundle": bundle, "id": "keys", "file": path(&elsewhere.join("credentials")) }),
    )
    .await;
    assert_eq!(read.is_error, Some(true));
    assert!(text(&read).contains("is outside where this server reads"), "{}", text(&read));
    assert!(!here.join("q3/data/credentials").exists());
    // Under its folder, nothing hidden: no key, no cloud credentials, no git hook.
    std::fs::create_dir_all(here.join(".ssh")).unwrap();
    std::fs::create_dir_all(here.join(".aws")).unwrap();
    for out in [".ssh/authorized_keys", ".ssh/frame.png", ".git/hooks/pre-commit.png"] {
        let render = call(&client, "deck_render", json!({ "bundle": deck, "state": "revenue", "out": out })).await;
        assert_eq!(render.is_error, Some(true), "{out}");
        assert!(text(&render).contains("in no hidden folder"), "{out}: {}", text(&render));
    }
    std::fs::write(here.join(".aws/credentials.csv"), "key,secret\nAKIA0000,abc\n").unwrap();
    let hidden = json!({ "bundle": bundle, "id": "aws", "file": path(&here.join(".aws/credentials.csv")) });
    let read = call(&client, "data_attach", hidden).await;
    assert_eq!(read.is_error, Some(true));
    assert!(text(&read).contains("is outside where this server reads"), "{}", text(&read));
    // And only what the tool writes, by its name: no notes, no shell's start-up file.
    let notes = call(&client, "deck_render", json!({ "bundle": deck, "state": "revenue", "out": "notes.txt" })).await;
    assert_eq!(notes.is_error, Some(true));
    assert!(text(&notes).contains("is no .png file"), "{}", text(&notes));
    let pdf = call(&client, "deck_export", json!({ "bundle": deck, "format": "pdf", "out": "profile" })).await;
    assert_eq!(pdf.is_error, Some(true));
    assert!(text(&pdf).contains("is no .pdf file"), "{}", text(&pdf));
    let images =
        call(&client, "deck_export", json!({ "bundle": deck, "format": "png", "out": "LaunchAgent.plist" })).await;
    assert_eq!(images.is_error, Some(true));
    assert!(text(&images).contains("this writes a folder of images there"), "{}", text(&images));
    assert!(
        !here.join("notes.txt").exists()
            && !here.join("profile").exists()
            && !here.join(".ssh/authorized_keys").exists()
    );
    // Under its folder, by a path relative to it, it writes.
    let rendered =
        call(&client, "deck_render", json!({ "bundle": deck, "state": "revenue", "out": "revenue.png" })).await;
    assert_ne!(rendered.is_error, Some(true), "{}", text(&rendered));
    assert!(here.join("revenue.png").is_file());
    client.cancel().await.unwrap();
}

/// A theme a tool reads (deck_create, theme_apply, deck_generate) is held as a data file is: a
/// theme that ships by its name, or one under the server's folder, and nothing else the user
/// can read (lift-up Security 2).
#[tokio::test(flavor = "multi_thread")]
async fn a_theme_is_read_only_from_the_servers_folder_or_by_its_name() {
    let here = scratch("theme-confined");
    let elsewhere = scratch("theme-confined-elsewhere");
    let server = scaena_mcp::Scaena::default().with_dir(here.clone());
    let client = serve(server, ClientConfig::default(), ClientLifecycleMode::Initialize).await;
    // A theme that ships, by its name: nothing is read from the disk.
    let made =
        ok(&client, "deck_create", json!({ "bundle": path(&here.join("a")), "theme": "dusk", "title": "Q3" })).await;
    assert_eq!(made["created"], true, "{made:#}");
    // A theme file under the server's folder, its fonts above it in the bundle it belongs to.
    let inside = here.join("a/themes/dusk.theme.json");
    let made = ok(&client, "deck_create", json!({ "bundle": path(&here.join("b")), "theme": path(&inside) })).await;
    assert_eq!(made["created"], true, "{made:#}");
    // The same theme anywhere else is refused, by deck_create and by theme_apply, and no bundle is made.
    copy_dir(&here.join("a"), &elsewhere.join("a"));
    let outside = path(&elsewhere.join("a/themes/dusk.theme.json"));
    let refused = call(&client, "deck_create", json!({ "bundle": path(&here.join("c")), "theme": outside })).await;
    assert_eq!(refused.is_error, Some(true));
    assert!(text(&refused).contains("is outside where this server reads"), "{}", text(&refused));
    assert!(!here.join("c").exists());
    let applied = call(&client, "theme_apply", json!({ "bundle": path(&here.join("a")), "theme": outside })).await;
    assert_eq!(applied.is_error, Some(true));
    assert!(text(&applied).contains("is outside where this server reads"), "{}", text(&applied));
    let slides: Value =
        serde_json::from_str(&std::fs::read_to_string("../scaena-narrato/tests/answers/catalog.json").unwrap())
            .unwrap();
    let generated =
        call(&client, "deck_generate", json!({ "bundle": path(&here.join("d")), "slides": slides, "theme": outside }))
            .await;
    assert_eq!(generated.is_error, Some(true));
    assert!(text(&generated).contains("is outside where this server reads"), "{}", text(&generated));
    assert!(!here.join("d").exists());
    client.cancel().await.unwrap();
}

/// Over MCP a theme's fonts are looked for no higher than the folder it was read under, and a
/// new bundle goes only where the server writes: a font one folder above the server's is not
/// read, and no bundle is made outside its folder or in a hidden one (SPEC §7.2).
#[tokio::test(flavor = "multi_thread")]
async fn a_themes_fonts_and_a_new_bundle_stay_in_the_servers_folder() {
    let root = scratch("font-walk");
    let here = root.join("server");
    std::fs::create_dir_all(&here).unwrap();
    let server = scaena_mcp::Scaena::default().with_dir(here.clone());
    let client = serve(server, ClientConfig::default(), ClientLifecycleMode::Initialize).await;
    let made = ok(&client, "deck_create", json!({ "bundle": path(&here.join("a")), "theme": "dusk" })).await;
    assert_eq!(made["created"], true, "{made:#}");
    // The theme in the server's folder, its fonts one folder above it.
    std::fs::copy(here.join("a/themes/dusk.theme.json"), here.join("loose.theme.json")).unwrap();
    copy_dir(&here.join("a/fonts"), &root.join("fonts"));
    let loose = path(&here.join("loose.theme.json"));
    let refused = call(&client, "deck_create", json!({ "bundle": path(&here.join("b")), "theme": loose })).await;
    assert_eq!(refused.is_error, Some(true));
    assert!(text(&refused).contains("is not beside"), "{}", text(&refused));
    assert!(!here.join("b").exists());
    let slides: Value =
        serde_json::from_str(&std::fs::read_to_string("../scaena-narrato/tests/answers/catalog.json").unwrap())
            .unwrap();
    let generated =
        call(&client, "deck_generate", json!({ "bundle": path(&here.join("c")), "slides": slides, "theme": loose }))
            .await;
    assert_eq!(generated.is_error, Some(true));
    assert!(text(&generated).contains("is not beside"), "{}", text(&generated));
    // With its fonts beside it, the same theme makes a bundle.
    copy_dir(&root.join("fonts"), &here.join("fonts"));
    let made = ok(&client, "deck_create", json!({ "bundle": path(&here.join("b")), "theme": loose })).await;
    assert_eq!(made["created"], true, "{made:#}");
    // A new bundle outside the server's folder, or in a hidden one, is not made.
    for (bundle, tool) in [(root.join("outside"), "deck_create"), (here.join(".hidden/x"), "deck_create")]
        .into_iter()
        .chain([(root.join("outside"), "deck_generate"), (here.join(".hidden/x"), "deck_generate")])
    {
        let args = json!({ "bundle": path(&bundle), "theme": "dusk", "slides": slides });
        let args = if tool == "deck_create" { json!({ "bundle": path(&bundle), "theme": "dusk" }) } else { args };
        let refused = call(&client, tool, args).await;
        assert_eq!(refused.is_error, Some(true), "{tool} {}", bundle.display());
        assert!(text(&refused).contains("is outside where this server writes"), "{}", text(&refused));
        assert!(!bundle.exists(), "{}", bundle.display());
    }
    client.cancel().await.unwrap();
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// An export that outlives the client's wait keeps going (SPEC §7.2): the call answers
/// that it is running and how far it has got, the same call again waits for the rest, and
/// a different export of the same file is refused until it is done.
#[tokio::test(flavor = "multi_thread")]
async fn a_long_export_keeps_going_while_the_client_asks_after_it() {
    // A server that waits for nothing, so every export outlives its first call.
    let scratched = scratch("long-export");
    let server = scaena_mcp::Scaena::default().with_export_wait(Duration::ZERO).with_dir(scratched.clone());
    let client = serve(server, ClientConfig::default(), ClientLifecycleMode::Initialize).await;
    let dir = scratched.join("images");
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
async fn an_agent_makes_a_deck_edits_it_twice_and_restores_the_first_edit() {
    let dir = scratch("history-loop");
    let bundle = path(&dir.join("q3"));
    let client = connect().await;
    // Made with its history begun: every write after is a version.
    ok(&client, "deck_create", json!({ "bundle": bundle, "theme": "dusk", "title": "Q3" })).await;
    let ops = json!([
        { "op": "add_node", "id": "title", "state": "start",
          "node": { "type": "text", "role": "display", "text": "Q3 Review", "at": { "in": "title" } } },
        { "op": "add", "path": "/states/0/layout", "value": "title" }
    ]);
    ok(&client, "deck_patch", json!({ "bundle": bundle, "ops": ops })).await;
    let ops = json!([{ "op": "set_text", "node": "title", "text": "Q3, in full" }]);
    ok(&client, "deck_patch", json!({ "bundle": bundle, "ops": ops })).await;

    let listed = ok(&client, "deck_history", json!({ "bundle": bundle })).await;
    let versions = listed["versions"].as_array().expect("the versions");
    assert_eq!(versions.len(), 3, "{listed:#}");
    for told in versions {
        assert!(told["author"].as_str().is_some_and(|by| by.starts_with("agent:")), "{told:#}");
        assert!(told["by"].as_str().is_some_and(|by| by.starts_with("the assistant")), "{told:#}");
        assert!(told["said"].as_str().is_some_and(|said| !said.is_empty()), "{told:#}");
    }
    assert!(versions[2]["changed"].to_string().contains("Slide 1"), "{listed:#}");

    // What the second edit changed, and what it was before.
    let compared = ok(&client, "deck_history", json!({ "bundle": bundle, "compare": ["2", "3"] })).await;
    let start = &compared["compared"]["states"]["start"]["changed"];
    assert_eq!(start["before"]["nodes"]["title"]["text"], "Q3 Review", "{compared:#}");

    let restored = ok(&client, "deck_history", json!({ "bundle": bundle, "restore": "2" })).await;
    assert_eq!(restored["restored"]["applied"], true, "{restored:#}");
    let deck = scaena_ops::open(&dir.join("q3")).unwrap().deck;
    assert_eq!(deck.nodes["title"].props.get("text"), Some(&json!("Q3 Review")));
    let listed = ok(&client, "deck_history", json!({ "bundle": bundle })).await;
    assert_eq!(listed["versions"].as_array().map(Vec::len), Some(4), "the restore is a change of its own");

    // A deck made without one begins it once.
    let plain = path(&dir.join("plain"));
    ok(&client, "deck_create", json!({ "bundle": plain, "theme": "dusk", "history": false })).await;
    let begun = ok(&client, "deck_history", json!({ "bundle": plain, "begin": true })).await;
    assert_eq!(begun["begun"], true, "{begun:#}");
    let again = ok(&client, "deck_history", json!({ "bundle": plain, "begin": true })).await;
    assert_eq!(again["begun"], false, "{again:#}");
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
