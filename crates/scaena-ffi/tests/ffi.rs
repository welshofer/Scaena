//! The C ABI as Swift calls it, from Rust: a bundle opened from its files and from a zip, the
//! session's calls and the MCP operations as JSON, frames that are the goldens', pixels, a save
//! with its history, and every misuse an error, never a crash (PLAN 3.1).

use scaena_ffi::*;
use serde_json::{Value, json};
use std::ffi::{CStr, CString, c_char};
use std::path::Path;
use std::ptr::{null, null_mut};

const TORTURE: &str = "../../tests/fixtures/torture.scaena";
const B1: &str = "../../tests/bench/b1.scaena";

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

/// `method` answered: its `ok`, or a panic with its error.
fn call(s: *mut ScaenaSession, method: &str, args: Value) -> Value {
    let answer: Value =
        serde_json::from_str(&took(unsafe { scaena_call(s, c(method).as_ptr(), c(&args.to_string()).as_ptr()) }))
            .unwrap();
    assert!(answer.get("error").is_none(), "{method}: {answer}");
    answer["ok"].clone()
}

fn tool(s: *mut ScaenaSession, name: &str, args: Value) -> Value {
    let author = c("user");
    let at = c("2026-10-08T12:00:00Z");
    let called =
        unsafe { scaena_tool(s, c(name).as_ptr(), c(&args.to_string()).as_ptr(), author.as_ptr(), at.as_ptr()) };
    serde_json::from_str(&took(called)).unwrap()
}

#[test]
fn a_bundle_opened_from_its_files_draws_its_goldens() {
    let s = open(TORTURE);
    let states = call(s, "states", Value::Null);
    assert!(states.as_array().unwrap().len() > 50, "{states}");

    // Each state at rest in the deck's own canvas: its display list is the goldens' (raw.fnv1a).
    let goldens = std::fs::read_to_string("../../tests/golden/torture/raw.fnv1a").unwrap();
    let mut checked = 0;
    for (name, digest) in goldens.lines().filter_map(|l| l.split_once(' ')) {
        if name.contains(['@', '~']) {
            continue;
        }
        assert_eq!(call(s, "digest", json!({ "state": name })), json!(digest), "{name}");
        checked += 1;
    }
    assert!(checked > 40, "{checked} states checked");

    // The display list's bytes are the digest's.
    let state = c("images");
    let mut error = null_mut();
    let frame = unsafe { scaena_frame(s, state.as_ptr(), f64::INFINITY, &mut error) };
    assert!(!frame.data.is_null() && frame.len > 0 && error.is_null());
    let bytes = unsafe { std::slice::from_raw_parts(frame.data, frame.len) };
    let list = scaena_core::displaylist::DisplayList::from_postcard(bytes).unwrap();
    assert_eq!(json!(list.digest().unwrap()), call(s, "digest", json!({ "state": "images" })));
    unsafe { scaena_bytes_free(frame) };

    // Painted 320 pixels wide on a 16:9 canvas.
    let pixels = unsafe { scaena_pixels(s, state.as_ptr(), f64::INFINITY, 320, &mut error) };
    assert_eq!((pixels.width, pixels.height, pixels.bytes.len), (320, 180, 320 * 180 * 4));
    unsafe { scaena_bytes_free(pixels.bytes) };

    // The timeline names each state's slide.
    let timeline = call(s, "timeline", Value::Null);
    let first = &timeline[0];
    assert!(first["state"].is_string() && first["slide"].is_string() && first["start"].is_number(), "{first}");
    unsafe { scaena_session_free(s) };
}

#[test]
fn the_editor_compiles_lints_and_patches_as_the_browser_does() {
    let s = open(B1);
    let source = call(s, "source", Value::Null);
    let source = source.as_str().unwrap();
    let compiled = call(s, "compile", json!({ "source": source }));
    assert_eq!(compiled["valid"], true, "{compiled}");
    let linted = call(s, "lint", Value::Null);
    assert!(linted["findings"].is_array() && linted["whole"] == true, "{linted}");

    // A patch by the user, as the canvas makes one: the deck, and so its source, change.
    let states = call(s, "states", Value::Null);
    let state = states[0].as_str().unwrap();
    // Each text node's look, through the cascade, by its id.
    let inspected = call(s, "inspect", json!({ "state": state }));
    let node = (inspected["looks"].as_object().and_then(|looks| looks.keys().next().cloned()))
        .unwrap_or_else(|| panic!("a text in {state}: {inspected}"));
    let patched = tool(
        s,
        "deck_patch",
        json!({ "ops": [{ "op": "replace_text", "state": state, "node": node, "from": 0, "to": 0, "text": "Rewritten from Swift. " }] }),
    );
    assert_eq!(patched["edited"], true, "{patched}");
    let after = call(s, "source", Value::Null);
    assert!(after.as_str().unwrap().contains("Rewritten from Swift"), "the source shows the patch");

    // A patch the deck refuses is the tool's error, as the assistant reads it.
    let refused = tool(s, "deck_patch", json!({ "ops": [{ "op": "remove", "path": "/states/9999" }] }));
    assert!(refused["error"]["message"].is_string(), "{refused}");
    unsafe { scaena_session_free(s) };
}

#[test]
fn a_save_writes_the_bundle_records_its_history_and_reopens() {
    let s = open(B1);
    call(s, "keepHistory", Value::Null);
    assert_eq!(call(s, "keepsHistory", Value::Null), true);
    let now = c("2026-10-08T12:00:00Z");
    let mut error = null_mut();
    let saved = unsafe { scaena_save(s, now.as_ptr(), true, &mut error) };
    assert!(!saved.is_null(), "saved: {}", took(error));
    let listed: Value = serde_json::from_str(&took(unsafe { scaena_saved_list(saved) })).unwrap();
    let files = listed["ok"]["files"].as_array().unwrap();
    assert!(files.contains(&json!("deck.json")) && files.contains(&json!("history/deck.loro")), "{listed}");
    let subset = &listed["ok"]["summary"]["subset"];
    assert!(subset.as_array().is_some_and(|s| !s.is_empty()), "fonts subset: {listed}");

    let deck = unsafe { scaena_saved_file(saved, c("deck.json").as_ptr()) };
    assert!(deck.len > 0);
    unsafe { scaena_bytes_free(deck) };
    let none = unsafe { scaena_saved_file(saved, c("nothing/here").as_ptr()) };
    assert!(none.data.is_null() && none.len == 0);

    // The save as a zip opens as the bundle it is.
    let zip = unsafe { scaena_saved_zip(saved, &mut error) };
    assert!(!zip.data.is_null());
    let reopened = unsafe { scaena_open_zip(zip.data, zip.len, &mut error) };
    assert!(!reopened.is_null());
    assert_eq!(call(reopened, "states", Value::Null), call(s, "states", Value::Null));
    unsafe { scaena_bytes_free(zip) };
    unsafe { scaena_session_free(reopened) };

    // Adopted, the session's files are the save's.
    let adopted: Value = serde_json::from_str(&took(unsafe { scaena_adopt(s, saved) })).unwrap();
    assert_eq!(adopted, json!({ "ok": null }));
    assert!(call(s, "files", Value::Null).as_array().unwrap().contains(&json!("history/deck.loro")));
    unsafe { scaena_saved_free(saved) };
    unsafe { scaena_session_free(s) };
}

#[test]
fn every_misuse_is_an_error_never_a_crash() {
    let s = open(B1);
    let answer = |method: *const c_char, args: *const c_char| -> Value {
        serde_json::from_str(&took(unsafe { scaena_call(s, method, args) })).unwrap()
    };
    let error = |v: &Value| v["error"]["message"].as_str().unwrap_or_default().to_string();
    assert!(error(&answer(c("nonsense").as_ptr(), null())).contains("not a call"));
    assert!(error(&answer(c("states").as_ptr(), c("{not json").as_ptr())).contains("not JSON"));
    assert!(error(&answer(null(), null())).contains("method is null"));
    assert!(error(&answer(c("duration").as_ptr(), null())).contains("`state`"));
    assert!(
        error(&answer(c("duration").as_ptr(), c(r#"{"state":"no-such-state"}"#).as_ptr())).contains("no-such-state")
    );
    let nul: Value =
        serde_json::from_str(&took(unsafe { scaena_call(null_mut(), c("states").as_ptr(), null()) })).unwrap();
    assert!(error(&nul).contains("session is null"));

    // Bytes and handles: null in, null out, the error said.
    let mut said = null_mut();
    let frame = unsafe { scaena_frame(s, c("no-such-state").as_ptr(), 0.0, &mut said) };
    assert!(frame.data.is_null() && !said.is_null());
    assert!(took(said).contains("no-such-state"));
    let mut said = null_mut();
    assert!(unsafe { scaena_open(null_mut(), &mut said) }.is_null());
    assert!(took(said).contains("files is null"));
    let mut said = null_mut();
    assert!(unsafe { scaena_open_zip(b"not a zip".as_ptr(), 9, &mut said) }.is_null());
    assert!(!took(said).is_empty());
    assert!(!unsafe { scaena_files_add(null_mut(), c("deck.json").as_ptr(), null(), 0) });
    let files = scaena_files_new();
    assert!(!unsafe { scaena_files_add(files, null(), null(), 0) });
    assert!(!unsafe { scaena_files_add(files, c("deck.json").as_ptr(), null(), 4) });
    let mut said = null_mut();
    assert!(unsafe { scaena_open(files, &mut said) }.is_null(), "a bundle with no deck does not open");
    assert!(!took(said).is_empty());

    // Freeing nothing is nothing.
    unsafe {
        scaena_string_free(null_mut());
        scaena_bytes_free(ScaenaBytes { data: null_mut(), len: 0 });
        scaena_session_free(null_mut());
        scaena_saved_free(null_mut());
        scaena_files_free(null_mut());
        scaena_session_free(s);
    }
}
