//! `scaena haiku` (PLAN O.4e): against an Anthropic on this machine, an answer of 5, 8, and 5 is
//! asked once more and the second, of 5, 7, and 5, placed on a new Haiku slide: two requests.
//! `--lines` asks no model, `--dry-run` writes nothing, and no key exits 2 naming the variable, as `generate` does.

use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const KEYS: [&str; 4] = ["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "GEMINI_API_KEY", "GOOGLE_API_KEY"];
const PROXIES: [&str; 6] = ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"];

fn scaena(args: &[&str], key: bool) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_scaena"));
    for k in KEYS.into_iter().chain(["SCAENA_MODEL"]).chain(PROXIES) {
        c.env_remove(k);
    }
    if key {
        c.env("ANTHROPIC_API_KEY", "sk-test");
    }
    c.args(args).output().unwrap()
}

/// A new Dusk bundle at a place of its own.
fn bundle(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("haiku-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    let out = scaena(&["new", dir.to_str().unwrap(), "--theme", "dusk", "--title", "Field notes"], false);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    dir
}

fn deck(dir: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("deck.json")).unwrap()).unwrap()
}

struct Asked {
    body: String,
}

/// Anthropic on this machine: the first answer counts 5, 8, and 5; every other, 5, 7, and 5.
fn serve() -> (String, Arc<Mutex<Vec<Asked>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let asked = Arc::new(Mutex::new(Vec::new()));
    let kept = asked.clone();
    let n = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() || line.is_empty() {
                continue;
            }
            let mut length = 0;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header.trim().is_empty() {
                    break;
                }
                if let Some((k, v)) = header.split_once(':')
                    && k.eq_ignore_ascii_case("content-length")
                {
                    length = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            kept.lock().unwrap().push(Asked { body: String::from_utf8_lossy(&body).into_owned() });
            let text = if n.fetch_add(1, Ordering::SeqCst) == 0 {
                "An old silent pond\nA green frog jumps into the pond\nSplash! Silence again"
            } else {
                "An old silent pond\nA frog jumps into the pond\nSplash! Silence again"
            };
            let reply = serde_json::json!({ "content": [{ "type": "text", "text": text }], "stop_reason": "end_turn" })
                .to_string();
            let head = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                reply.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(reply.as_bytes()).unwrap();
        }
    });
    (base, asked)
}

#[test]
fn a_near_miss_is_asked_once_more_and_the_second_placed() {
    let dir = bundle("asked");
    let states = deck(&dir)["states"].as_array().unwrap().len();
    let (base, asked) = serve();
    let out = scaena(
        &[
            "--json",
            "haiku",
            dir.to_str().unwrap(),
            "a field at dawn",
            "--provider",
            "anthropic",
            "--model",
            "test-model-1",
            "--base",
            &base,
        ],
        true,
    );
    let said = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.status.code(), Some(0), "{said}");
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 2, "two requests");
    assert!(asked[0].body.contains("Write a haiku about: a field at dawn"), "{}", asked[0].body);
    assert!(asked[1].body.contains("Its lines count 5, 8, and 5 syllables"), "{}", asked[1].body);
    let written: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(written["lines"][1], "A frog jumps into the pond");
    assert_eq!(written["kept"], "kept");
    assert_eq!(written["patched"]["applied"], true);
    let d = deck(&dir);
    assert_eq!(d["states"].as_array().unwrap().len(), states + 1);
    let slide = d["states"].as_array().unwrap().iter().find(|s| s["id"] == written["state"]).unwrap();
    assert_eq!(slide["layout"], "haiku");
    let notes = slide["notes"].as_str().unwrap();
    assert!(notes.starts_with("Haiku written by test-model-1 with your Anthropic key on "), "{notes}");
    assert!(notes.ends_with(", about a field at dawn."), "{notes}");
    assert!(!d["nodes"].to_string().contains("test-model-1"), "nothing on the slide names the model");
    assert_eq!(scaena(&["validate", dir.to_str().unwrap()], false).status.code(), Some(0));
}

#[test]
fn lines_given_ask_no_model_and_a_dry_run_writes_nothing() {
    let dir = bundle("lines");
    let before = deck(&dir);
    let lines = "Winter rain on glass\nthe projector hums along\nslides wait in the dark";
    let out = scaena(&["--json", "haiku", dir.to_str().unwrap(), "--lines", lines, "--dry-run"], false);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let written: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(written["patched"]["applied"], false);
    assert_eq!(deck(&dir), before, "a dry run writes nothing");
    let out = scaena(&["haiku", dir.to_str().unwrap(), "--lines", lines, "--attribution", "after a long day"], false);
    let said = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(out.status.code(), Some(0), "{said}");
    assert!(said.contains("Winter rain on glass    (5)") && said.contains("Placed it on"), "{said}");
    let d = deck(&dir);
    assert!(d["nodes"].to_string().contains("after a long day"));
    let notes: Vec<&str> = d["states"].as_array().unwrap().iter().filter_map(|s| s["notes"].as_str()).collect();
    assert!(
        notes.iter().any(|n| n.starts_with("Haiku given on ") && n.ends_with(", about the whole talk.")),
        "{notes:?}"
    );
}

#[test]
fn no_key_exits_2_naming_the_variable() {
    let dir = bundle("nokey");
    let out = scaena(&["haiku", dir.to_str().unwrap(), "rain", "--model", "test-model-1"], false);
    // Input the command cannot act on, as `generate` says it (SPEC §7.1): no request is made.
    assert_eq!(out.status.code(), Some(2));
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("ANTHROPIC_API_KEY"), "{said}");
}
