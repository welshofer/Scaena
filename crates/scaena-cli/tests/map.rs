//! `scaena map` (ADR-0030, PLAN O.1b) against a stand-in Geoapify on this machine, serving the
//! recorded answers in `tests/fixtures/maps`: a map put in a slide's empty place for a picture, a
//! name that matches several places and one picked, a map fetched again, and no key exiting 2
//! before any call. `map_fetch` over MCP, through the CLI's transport, on the same stand-in.

use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, ClientConfig};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/maps");
const EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/examples");

/// Geoapify on this machine: its geocoder answers by the place asked, its static map with the
/// recorded picture. Each request's path is kept.
fn geoapify() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let asked = Arc::new(Mutex::new(Vec::new()));
    let kept = asked.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() || line.is_empty() {
                continue;
            }
            let path = line.split_whitespace().nth(1).unwrap_or_default().to_string();
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
                    break;
                }
            }
            let read = |name: &str| std::fs::read(format!("{FIXTURES}/{name}")).unwrap();
            let (status, kind, body) = if path.starts_with("/v1/staticmap?") {
                (200, "image/png", read("map.png"))
            } else if path.contains("text=Springfield") {
                (200, "application/json", read("places-several.json"))
            } else if path.starts_with("/v1/geocode/search?") {
                (200, "application/json", read("places-one.json"))
            } else {
                (404, "text/plain", b"no such page".to_vec())
            };
            kept.lock().unwrap().push(path);
            let head = format!(
                "HTTP/1.1 {status} OK\r\ncontent-type: {kind}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(&body).unwrap();
        }
    });
    (base, asked)
}

/// The revenue example in a bundle of its own.
fn bundle(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("map-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    let deck: Value = serde_json::from_slice(&std::fs::read(format!("{EXAMPLES}/revenue.deck.json")).unwrap()).unwrap();
    let mut files: Vec<String> =
        deck["fonts"].as_array().unwrap().iter().map(|f| f["file"].as_str().unwrap().into()).collect();
    files.push(deck["theme"].as_str().unwrap().into());
    files.extend(deck["data"].as_object().unwrap().values().filter_map(|s| s["source"].as_str().map(String::from)));
    for f in files {
        let to = dir.join(&f);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(format!("{EXAMPLES}/{f}"), to).unwrap();
    }
    std::fs::write(dir.join("deck.json"), serde_json::to_vec_pretty(&deck).unwrap()).unwrap();
    dir
}

fn scaena(base: &str, key: Option<&str>, args: &[&str]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_scaena"));
    c.args(args)
        .env("SCAENA_GEOAPIFY_BASE", base)
        .env("SOURCE_DATE_EPOCH", "1792094000")
        .env_remove("GEOAPIFY_API_KEY");
    if let Some(key) = key {
        c.env("GEOAPIFY_API_KEY", key);
    }
    c.output().unwrap()
}

fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!("{e}: {}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
    })
}

#[test]
fn a_map_is_put_on_a_slide_picked_and_fetched_again() {
    let (base, asked) = geoapify();
    let dir = bundle("cli");
    let b = dir.to_str().unwrap();

    // No key: said, before any call.
    let out = scaena(&base, None, &["--json", "map", b, "Ferry Building"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(json(&out)["error"]["message"].as_str().unwrap().contains("GEOAPIFY_API_KEY"));
    assert!(asked.lock().unwrap().is_empty(), "nothing asked without a key");

    // Into the title slide's empty place for a picture.
    let out = scaena(&base, Some("TEST-KEY"), &["--json", "map", b, "Ferry Building, San Francisco"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let done = json(&out);
    assert_eq!(done["target"]["slot"], "art");
    assert_eq!(done["placed"]["node"], "map");
    assert_eq!(done["placed"]["applied"], true);
    let paths = asked.lock().unwrap().clone();
    assert_eq!(paths.len(), 2);
    assert!(
        paths[0]
            .starts_with("/v1/geocode/search?text=Ferry%20Building%2C%20San%20Francisco&lang=en&limit=5&format=json")
    );
    assert!(paths[1].starts_with("/v1/staticmap?style=dark-matter&"), "Dusk's surface is dark: {}", paths[1]);
    assert!(paths.iter().all(|p| p.ends_with("&apiKey=TEST-KEY")), "the key rides in its slot: {paths:?}");
    let deck: Value = serde_json::from_slice(&std::fs::read(dir.join("deck.json")).unwrap()).unwrap();
    let map = &deck["nodes"]["map"];
    assert_eq!(map["fetched"]["at"], "2026-10-15T19:53:20Z", "when its answer came, by the client's clock");
    assert!(!deck.to_string().contains("TEST-KEY"), "no key in the bundle");
    assert!(dir.join(map["src"].as_str().unwrap()).is_file());

    // Several places: listed, exit 2, then one picked.
    let out = scaena(
        &base,
        Some("TEST-KEY"),
        &["--json", "map", b, "Springfield", "--state", "revenue", "--at", "100,100,400,225"],
    );
    assert_eq!(out.status.code(), Some(2));
    let said = json(&out);
    assert_eq!(said["error"]["places"].as_array().unwrap_or_else(|| panic!("{said:#}")).len(), 3);
    let out = scaena(&base, Some("TEST-KEY"), &["map", b, "Springfield", "--places"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("2. Springfield, MA, United States of America"));
    let picked = ["--json", "map", b, "Springfield", "--state", "revenue", "--at", "100,100,400,225", "--pick", "2"];
    let done = json(&scaena(&base, Some("TEST-KEY"), &picked));
    assert_eq!(done["placed"]["node"], "map-2");

    // Fetched again, nearer: the same image, a new picture, both kept.
    let before = deck["nodes"]["map"]["src"].as_str().unwrap().to_string();
    let n = asked.lock().unwrap().len();
    let done = json(&scaena(&base, Some("TEST-KEY"), &["--json", "map", b, "--refresh", "map", "--zoom", "17"]));
    assert_eq!(done["placed"]["node"], "map");
    let again = asked.lock().unwrap()[n..].to_vec();
    assert_eq!(again.len(), 1, "a refresh asks no geocoder: {again:?}");
    assert!(again[0].contains("&zoom=17&"), "{again:?}");
    let deck: Value = serde_json::from_slice(&std::fs::read(dir.join("deck.json")).unwrap()).unwrap();
    assert_eq!(deck["nodes"]["map"]["fetched"]["map"]["zoom"], 17.0);
    assert!(dir.join(&before).is_file(), "the first picture stays in the bundle");
    assert_eq!(scaena(&base, None, &["validate", b]).status.code(), Some(0));
}

#[tokio::test(flavor = "multi_thread")]
async fn map_fetch_reaches_geoapify_through_the_clis_transport() {
    let (base, asked) = geoapify();
    let dir = bundle("mcp");
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_scaena"))
        .arg("mcp")
        .env("SCAENA_GEOAPIFY_BASE", &base)
        .env("GEOAPIFY_API_KEY", "TEST-KEY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let io = (child.stdout.take().unwrap(), child.stdin.take().unwrap());
    let client = ClientConfig::default().serve(io).await.unwrap();
    let tools = client.list_all_tools().await.unwrap();
    let tool = tools.iter().find(|t| t.name == "map_fetch").expect("map_fetch is listed");
    assert!(
        tool.description.as_deref().unwrap_or_default().contains("api.geoapify.com"),
        "it says what it sends where"
    );
    let args = json!({ "bundle": dir.to_str().unwrap(), "place": "Ferry Building" });
    let request = CallToolRequestParams::new("map_fetch".to_string()).with_arguments(args.as_object().unwrap().clone());
    let result = client.call_tool(request).await.unwrap();
    assert_ne!(result.is_error, Some(true), "{:?}", result.content);
    let done = result.structured_content.unwrap();
    assert_eq!(done["placed"]["node"], "map");
    assert_eq!(asked.lock().unwrap().len(), 2);
    client.cancel().await.unwrap();
    assert!(child.wait().await.unwrap().success());
}
