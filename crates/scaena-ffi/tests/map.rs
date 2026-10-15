//! A map put on a slide through the C ABI, step by step as ScaenaKit's `Maps` puts one (ADR-0030,
//! PLAN O.1d): the generator's `map` and `mapAnswered` steps answered here with Geoapify's recorded
//! answers, the key in each request's query alone, and the session's `mapTarget` and `mapping`
//! making the ops the browser's canvas makes one patch of. Each step out of order is an error that
//! says why.

use scaena_ffi::*;
use serde_json::{Value, json};
use std::ffi::{CStr, CString, c_char};
use std::path::Path;
use std::ptr::{null, null_mut};

const B1: &str = "../../tests/bench/b1.scaena";
const MAPS: &str = "../../tests/fixtures/maps";

fn c(s: &str) -> CString {
    CString::new(s).unwrap()
}

/// A string the library returned, taken and freed.
fn took(p: *mut c_char) -> String {
    assert!(!p.is_null(), "a string was returned");
    let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap().to_string();
    unsafe { scaena_string_free(p) };
    s
}

/// Every file under `dir`, by its path from it.
fn files_of(dir: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let name = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                out.push((name, std::fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out
}

fn open(dir: &str) -> *mut ScaenaSession {
    let files = scaena_files_new();
    for (path, bytes) in files_of(Path::new(dir)) {
        assert!(unsafe { scaena_files_add(files, c(&path).as_ptr(), bytes.as_ptr(), bytes.len()) });
    }
    let mut error = null_mut();
    let session = unsafe { scaena_open(files, &mut error) };
    assert!(!session.is_null(), "{dir} opens: {}", took(error));
    session
}

/// One step of the generator as it answers: `{"ok"}` or `{"error"}`.
fn answered(g: *mut ScaenaGenerator, method: &str, args: &Value, bytes: &[u8]) -> Value {
    let (method, args) = (c(method), c(&args.to_string()));
    let at = if bytes.is_empty() { null() } else { bytes.as_ptr() };
    let answer = took(unsafe { scaena_generator_call(g, method.as_ptr(), args.as_ptr(), at, bytes.len()) });
    serde_json::from_str(&answer).unwrap()
}

fn step(g: *mut ScaenaGenerator, method: &str, args: Value, bytes: &[u8]) -> Value {
    let answer = answered(g, method, &args, bytes);
    answer.get("ok").cloned().unwrap_or_else(|| panic!("{method}: {answer}"))
}

/// The error a step answers with.
fn refused(g: *mut ScaenaGenerator, method: &str, args: Value) -> String {
    let answer = answered(g, method, &args, &[]);
    answer["error"]["message"].as_str().unwrap_or_else(|| panic!("{method} is refused: {answer}")).to_string()
}

/// A session's call answered: its `ok`, or a panic with its error.
fn call(s: *mut ScaenaSession, method: &str, args: Value) -> Value {
    let answer: Value =
        serde_json::from_str(&took(unsafe { scaena_call(s, c(method).as_ptr(), c(&args.to_string()).as_ptr()) }))
            .unwrap();
    answer.get("ok").cloned().unwrap_or_else(|| panic!("{method}: {answer}"))
}

fn tool(s: *mut ScaenaSession, name: &str, args: Value) -> Value {
    let (author, at) = (c("user"), c("2026-10-15T20:12:44Z"));
    let called =
        unsafe { scaena_tool(s, c(name).as_ptr(), c(&args.to_string()).as_ptr(), author.as_ptr(), at.as_ptr()) };
    serde_json::from_str(&took(called)).unwrap()
}

/// What Geoapify answers `url` with, recorded: the geocoder's places, or the static map's picture.
fn geoapify(url: &str) -> (Vec<u8>, &'static str) {
    if url.starts_with("https://maps.geoapify.com/") {
        (std::fs::read(format!("{MAPS}/map.png")).unwrap(), "image/png")
    } else {
        (std::fs::read(format!("{MAPS}/places-several.json")).unwrap(), "application/json")
    }
}

/// `request` made as Swift makes it: the key in its query alone, no header holding it.
fn sent(request: &Value, asked: &mut Vec<String>) -> (Vec<u8>, &'static str) {
    let url = request["url"].as_str().unwrap();
    assert_eq!(request["method"], "GET");
    assert!(url.ends_with("apiKey=geo-key"), "the key in the query: {url}");
    assert!(!request["headers"].to_string().contains("geo-key"), "no header holds the key: {request}");
    assert!(request["headers"]["user-agent"].as_str().unwrap().starts_with("Scaena/"), "{request}");
    asked.push(url.to_string());
    geoapify(url)
}

#[test]
fn a_map_is_put_on_a_slide_as_swift_puts_one() {
    let s = open(B1);
    let g = scaena_generator_new();
    let states = call(s, "states", Value::Null);
    let state = states[0]["id"].as_str().or(states[0].as_str()).unwrap().to_string();
    let target = call(s, "mapTarget", json!({ "state": state, "rect": [96.0, 96.0, 640.0, 360.0] }));
    let style = target["style"].as_str().unwrap().to_string();

    // A name that matches three places: they are offered, and nothing is fetched past them.
    let mut asked = Vec::new();
    let ask = json!({ "place": "Springfield", "show": "city", "style": style, "size": [640.0, 360.0] });
    let first = step(g, "map", json!({ "ask": ask, "key": "geo-key" }), &[]);
    let (body, kind) = sent(&first["request"], &mut asked);
    let offered = step(g, "mapAnswered", json!({ "status": 200, "kind": kind, "key": "geo-key" }), &body);
    let places = offered["choose"].as_array().unwrap_or_else(|| panic!("{offered}"));
    assert_eq!(places.len(), 3);
    assert_eq!(places[1]["address"], "Springfield, MA, United States of America");

    // The second picked: asked again, the static map of it fetched.
    let mut ask = ask;
    ask["pick"] = json!(1);
    let mut next = step(g, "map", json!({ "ask": ask, "key": "geo-key" }), &[]);
    let map = loop {
        let (body, kind) = sent(&next["request"], &mut asked);
        let args = json!({ "status": 200, "kind": kind, "key": "geo-key", "at": "2026-10-15T20:12:44Z" });
        next = step(g, "mapAnswered", args, &body);
        if let Some(map) = next.get("map") {
            break map.clone();
        }
        assert!(next.get("request").is_some(), "{next}");
    };
    let statics: Vec<&String> = asked.iter().filter(|u| u.starts_with("https://maps.geoapify.com/")).collect();
    assert_eq!(statics.len(), 1, "{asked:?}");
    assert!(statics[0].contains("lonlat:-72.5898,42.1015"), "{}", statics[0]);
    assert_eq!(map["fetched"]["from"], "geoapify");
    assert_eq!(map["fetched"]["at"], "2026-10-15T20:12:44Z");
    assert!(!map.to_string().contains("geo-key"), "no key in the record");

    // Put on the slide: the picture joins the bundle, and the ops are one patch that applies.
    let label = map["place"].clone();
    let mapped = call(
        s,
        "mapping",
        json!({ "target": target, "fetched": map["fetched"], "pin": map["pin"], "png": map["png"], "label": label }),
    );
    assert_eq!(mapped["said"], "Map of Springfield from Geoapify added");
    let patched = tool(s, "deck_patch", json!({ "ops": mapped["ops"] }));
    assert_eq!(patched["ok"]["applied"], true, "{patched}");
    let node = mapped["node"].as_str().unwrap();
    let files = call(s, "imageFiles", Value::Null);
    assert!(files.as_array().unwrap().iter().any(|f| f == &mapped["src"]), "{files}");

    // Fetched again a step nearer: the same view asked for, one zoom in.
    let again = call(s, "mapRefreshing", json!({ "state": state, "node": node, "zoom": 13.0 }));
    assert_eq!((again["zoom"].as_f64(), again["target"]["node"].as_str()), (Some(13.0), Some(node)));
    unsafe {
        scaena_generator_free(g);
        scaena_session_free(s);
    }
}

#[test]
fn each_map_step_out_of_order_says_why() {
    let g = scaena_generator_new();
    let answered_first = refused(g, "mapAnswered", json!({ "status": 200, "key": "geo-key" }));
    assert!(answered_first.contains("`map` first"), "{answered_first}");
    let ask = json!({ "place": "Ferry Building", "style": "positron", "size": [640.0, 360.0] });
    let keyless = refused(g, "map", json!({ "ask": ask, "key": " " }));
    assert!(keyless.contains("no Geoapify key"), "{keyless}");
    let odd = refused(
        g,
        "map",
        json!({ "ask": { "place": "Ferry Building", "style": "positron", "size": [1, 1], "colour": "red" }, "key": "k" }),
    );
    assert!(odd.contains("colour"), "{odd}");
    let styled =
        refused(g, "map", json!({ "ask": { "place": "Ferry Building", "style": "neon", "size": [1, 1] }, "key": "k" }));
    assert!(styled.contains("neon"), "{styled}");

    // Geoapify's refusal, said in its words; and then no map is asked.
    step(g, "map", json!({ "ask": ask, "key": "geo-key" }), &[]);
    let body = std::fs::read(format!("{MAPS}/unauthorized.json")).unwrap();
    let args = json!({ "status": 401, "kind": "application/json", "key": "geo-key" }).to_string();
    let answer: Value = serde_json::from_str(&took(unsafe {
        scaena_generator_call(g, c("mapAnswered").as_ptr(), c(&args).as_ptr(), body.as_ptr(), body.len())
    }))
    .unwrap();
    assert!(answer["error"]["message"].as_str().unwrap().contains("Geoapify"), "{answer}");
    let after = refused(g, "mapAnswered", json!({ "status": 200, "key": "geo-key" }));
    assert!(after.contains("`map` first"), "{after}");
    unsafe { scaena_generator_free(g) };
}
