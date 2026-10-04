//! `scaena serve` from the command line (PLAN 2.11, ADR-0012). Built after `just web`, it serves a
//! bundle's folder and says where, under `--json`, as one JSON value; a folder that is no bundle
//! exits 2. Built before, it exits 3 and names its PLAN task. `scaena-serve`'s own tests hold
//! the server to what it serves.

use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Command, Stdio};

const TORTURE: &str = "../../tests/fixtures/torture.scaena";

fn scaena() -> Command {
    Command::new(env!("CARGO_BIN_EXE_scaena"))
}

#[test]
fn serve_says_where_it_serves_or_what_it_needs() {
    if !scaena_serve::pages_built() {
        let out = scaena().args(["--json", "serve", TORTURE]).output().unwrap();
        assert_eq!(out.status.code(), Some(3));
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["error"]["plan"], "2.11");
        assert!(v["error"]["message"].as_str().unwrap().contains("just web"), "{v:#}");
        return;
    }
    let out = scaena().args(["--json", "serve", "no/such/bundle", "--port", "0"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["error"]["message"].as_str().unwrap().contains("not a bundle's folder"), "{v:#}");

    // The torture bundle keeps no deck.scn, so serving it writes nothing.
    let mut child = scaena()
        .args(["--json", "serve", TORTURE, "--port", "0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut said = Vec::new();
    let mut chunk = [0; 512];
    let served: Value = loop {
        let n = stdout.read(&mut chunk).unwrap();
        assert!(n > 0, "scaena serve stopped: {}", String::from_utf8_lossy(&said));
        said.extend_from_slice(&chunk[..n]);
        if let Ok(v) = serde_json::from_slice(&said) {
            break v;
        }
    };
    let player = served["player"].as_str().unwrap();
    assert_eq!(served["editor"].as_str(), Some(format!("{player}edit").as_str()));
    let port: u16 = player.trim_start_matches("http://localhost:").trim_end_matches('/').parse().unwrap();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(stream, "GET /bundle/deck.json HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\n\r\n").unwrap();
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).unwrap();
    let _ = child.kill();
    let _ = child.wait();
    let answer = String::from_utf8_lossy(&answer);
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    let deck = std::fs::read_to_string(format!("{TORTURE}/deck.json")).unwrap();
    assert!(answer.ends_with(&deck), "it serves the folder's deck.json");
}
