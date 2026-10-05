//! The resources reach an agent whole (PLAN 1.33, SPEC §7.2). Claude Code keeps an MCP result
//! over its limit in a file, which an agent with no tools for files cannot open; so every
//! resource weighs under `scaena_mcp::LIMIT` as `resources/read` returns it. The schemas are
//! served in parts and SPEC by section, and nothing is lost on the way.

use rmcp::model::{ClientConfig, ProtocolVersion, ReadResourceRequestParams, ResourceContents};
use rmcp::service::RunningService;
use rmcp::{ClientLifecycleMode, ClientServiceExt, RoleClient, ServiceExt};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

type Client = RunningService<RoleClient, ClientConfig>;

/// A client of the server, at protocol 2026-07-28: its results carry the cache hints too.
async fn connect() -> Client {
    let (server_io, client_io) = tokio::io::duplex(1 << 22);
    tokio::spawn(async move {
        let server = scaena_mcp::Scaena::default().serve(server_io).await.unwrap();
        server.waiting().await.unwrap();
    });
    let lifecycle = ClientLifecycleMode::Discover { preferred_versions: vec![ProtocolVersion::V_2026_07_28] };
    ClientConfig::default().serve_with_lifecycle(client_io, lifecycle).await.unwrap()
}

async fn read(client: &Client, uri: &str) -> String {
    let result = client.read_resource(ReadResourceRequestParams::new(uri)).await.unwrap();
    match &result.contents[0] {
        ResourceContents::TextResourceContents { text, .. } => text.clone(),
        other => panic!("{uri}: {other:?}"),
    }
}

/// The `scaena://` uris a text names in backticks, in order.
fn uris(text: &str) -> Vec<String> {
    text.split('`').skip(1).step_by(2).filter(|s| s.starts_with("scaena://")).map(String::from).collect()
}

/// What a section too large to arrive whole says before the uris of its subsections.
const BY_SUBSECTION: &str = "This section is served by subsection:\n\n";

#[tokio::test(flavor = "multi_thread")]
async fn every_resource_arrives_whole() {
    let client = connect().await;
    let list = client.list_resources(None).await.unwrap();
    let listed = serde_json::to_string(&list).unwrap().len();
    assert!(listed <= scaena_mcp::LIMIT, "resources/list weighs {listed} bytes");
    let mut all: Vec<String> = list.resources.iter().map(|r| r.uri.clone()).collect();
    // The subsections SPEC's index names, listed or not.
    all.extend(uris(&read(&client, "scaena://spec").await));
    all.sort();
    all.dedup();
    let mut heavy = vec![];
    for uri in &all {
        let result = client.read_resource(ReadResourceRequestParams::new(uri.as_str())).await.unwrap();
        let bytes = serde_json::to_string(&result).unwrap().len();
        if bytes > scaena_mcp::LIMIT {
            heavy.push(format!("{uri}: {bytes} bytes"));
        }
    }
    assert!(heavy.is_empty(), "over {} bytes as read: {heavy:#?}", scaena_mcp::LIMIT);
    assert!(all.len() > 60, "{all:?}");
    client.cancel().await.unwrap();
}

/// A schema's definitions as its file writes them: every `$ref` local again.
fn unlinked(v: &Value) -> Value {
    match v {
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| match (k.as_str(), v) {
                    ("$ref", Value::String(r)) => {
                        (k.clone(), Value::String(format!("#{}", r.split_once('#').unwrap().1)))
                    }
                    _ => (k.clone(), unlinked(v)),
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(unlinked).collect()),
        other => other.clone(),
    }
}

/// Every `$ref` in a value.
fn refs(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(o) => {
            if let Some(Value::String(r)) = o.get("$ref") {
                out.push(r.clone());
            }
            o.values().for_each(|v| refs(v, out));
        }
        Value::Array(a) => a.iter().for_each(|v| refs(v, out)),
        _ => {}
    }
}

fn file(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("../../docs/schema/{name}.schema.json")).unwrap()).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_schemas_are_served_in_parts_that_are_the_schemas() {
    let client = connect().await;
    let mut parts: BTreeMap<String, Value> = BTreeMap::new();
    for r in client.list_all_resources().await.unwrap() {
        if r.uri.starts_with("scaena://schema/") {
            parts.insert(r.uri.clone(), serde_json::from_str(&read(&client, &r.uri).await).unwrap());
        }
    }
    // Each part is a JSON Schema whose `$id` is its uri, and each `$ref` names a definition
    // that the part it names holds.
    for (uri, part) in &parts {
        assert_eq!(part["$id"], Value::String(uri.clone()), "{uri}");
        let mut all = vec![];
        refs(part, &mut all);
        for r in all {
            let (at, pointer) = r.split_once('#').unwrap();
            let holder = if at.is_empty() { part } else { parts.get(at).unwrap_or_else(|| panic!("{uri}: {r}")) };
            assert!(holder.pointer(pointer).is_some(), "{uri}: {r} names nothing");
        }
    }
    // The deck's and the theme's parts hold their file's definitions, each once, and the
    // root holds the rest of it.
    for name in ["deck", "theme"] {
        let root = format!("scaena://schema/{name}");
        let mut defs = Map::new();
        for (uri, part) in parts.iter().filter(|(u, _)| **u == root || u.starts_with(&format!("{root}/"))) {
            for (def, v) in part["$defs"].as_object().unwrap() {
                assert!(defs.insert(def.clone(), unlinked(v)).is_none(), "{def} is in two parts, one of them {uri}");
            }
        }
        let mut whole = file(name);
        assert_eq!(Value::Object(defs), whole["$defs"], "{name}'s definitions");
        let mut served = parts[&root].clone();
        for key in ["$id", "$comment", "$defs"] {
            served.as_object_mut().unwrap().remove(key);
        }
        for key in ["$id", "$defs"] {
            whole.as_object_mut().unwrap().remove(key);
        }
        assert_eq!(unlinked(&served), whole, "{name}'s root");
    }
    // The patch's ops reach, through the deck's parts, the definitions its file holds.
    let mut reached = Map::new();
    let mut todo = vec![("scaena://schema/patch".to_string(), None::<String>)];
    while let Some((uri, def)) = todo.pop() {
        let part = &parts[&uri];
        let at = match &def {
            Some(d) => &part["$defs"][d],
            None => part,
        };
        if let Some(d) = &def
            && reached.insert(d.clone(), unlinked(at)).is_some()
        {
            continue;
        }
        let mut all = vec![];
        refs(at, &mut all);
        for r in all {
            let (doc, pointer) = r.split_once('#').unwrap();
            let next = pointer.strip_prefix("/$defs/").unwrap().to_string();
            todo.push((if doc.is_empty() { uri.clone() } else { doc.to_string() }, Some(next)));
        }
    }
    assert_eq!(Value::Object(reached), file("patch")["$defs"], "the patch's definitions");
    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn spec_is_served_by_section_and_nothing_is_lost() {
    let client = connect().await;
    let spec = std::fs::read_to_string("../../docs/SPEC.md").unwrap();
    let index = read(&client, "scaena://spec").await;
    assert!(index.starts_with("# Scaena"), "{index}");
    let mut rebuilt = String::new();
    for line in index.lines().filter(|l| l.starts_with("- §")) {
        let text = read(&client, &uris(line)[0]).await;
        match text.split_once(BY_SUBSECTION) {
            None => rebuilt.push_str(&text),
            Some((intro, subsections)) => {
                rebuilt.push_str(intro);
                for uri in uris(subsections) {
                    rebuilt.push_str(&read(&client, &uri).await);
                }
            }
        }
    }
    let start = spec.find("\n## 0. ").unwrap() + 1;
    assert!(rebuilt == spec[start..], "the sections, in order, are SPEC after its preamble");
    // A subsection is its own resource whether its section is served whole or not; it is
    // listed when its section is served by subsection, as §3 and §7 are, and §8 is not.
    assert!(read(&client, "scaena://spec/3.7").await.starts_with("### 3.7 Charts\n"));
    assert!(read(&client, "scaena://spec/7.2").await.starts_with("### 7.2 "));
    assert!(read(&client, "scaena://spec/8.2").await.starts_with("### 8.2 "));
    let listed: Vec<String> = client.list_all_resources().await.unwrap().into_iter().map(|r| r.uri).collect();
    let has = |uri: &str| listed.iter().any(|u| u == uri);
    assert!(has("scaena://spec/3.7") && has("scaena://spec/7.2") && !has("scaena://spec/8.2"));
    client.cancel().await.unwrap();
}
