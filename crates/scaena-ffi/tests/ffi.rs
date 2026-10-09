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

/// `method`'s answer as it comes: `{"ok"}` or `{"error"}`.
fn answered(s: *mut ScaenaSession, method: &str, args: &Value) -> Value {
    serde_json::from_str(&took(unsafe { scaena_call(s, c(method).as_ptr(), c(&args.to_string()).as_ptr()) })).unwrap()
}

/// `method` answered: its `ok`, or a panic with its error.
fn call(s: *mut ScaenaSession, method: &str, args: Value) -> Value {
    let answer = answered(s, method, &args);
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
fn a_findings_fix_is_one_click_as_in_the_browser() {
    // The editor's loop (PLAN 2.3, 3.4): a choice in the inspector is a patch, the deck's source
    // compiled again, and the state shown linted.
    let s = open(B1);
    let source = call(s, "source", Value::Null);
    assert_eq!(call(s, "compile", json!({ "source": source }))["valid"], true);
    // The title in the color of what lies behind it: it does not read.
    let ops = json!([{ "op": "choose", "node": "title", "prop": "style/color", "value": "surface", "state": "cover" }]);
    let chosen = tool(s, "deck_patch", json!({ "ops": ops }));
    assert_eq!(chosen["edited"], true, "{chosen}");
    let source = call(s, "source", Value::Null);
    assert_eq!(call(s, "compile", json!({ "source": source }))["valid"], true);
    let linted = call(s, "lint", json!({ "state": "cover" }));
    let faint = (linted["findings"].as_array().unwrap().iter())
        .find(|f| f["node"] == "title" && f["fixable"] == true)
        .unwrap_or_else(|| panic!("the title reads too faintly, with a fix: {linted}"))
        .clone();
    assert!(["E110", "E111"].contains(&faint["code"].as_str().unwrap()), "{faint}");

    // Its fix, applied to the source compiled last, is the deck's source fixed (PLAN 2.82):
    // compiled, the title reads.
    let fixed = call(s, "fix", json!({ "patch": faint["fix"] }));
    let compiled = call(s, "compile", json!({ "source": fixed }));
    assert_eq!(compiled["valid"], true, "{compiled}");
    let linted = call(s, "lint", json!({ "state": "cover" }));
    let left =
        linted["findings"].as_array().unwrap().iter().filter(|f| f["node"] == "title" && f["code"] == faint["code"]);
    assert_eq!(left.count(), 0, "{linted}");
    unsafe { scaena_session_free(s) };
}

#[test]
fn the_canvas_reads_what_stands_where_as_the_browser_does() {
    let s = open(B1);
    let states = call(s, "states", Value::Null);
    let state = states[0].as_str().unwrap();
    // Each node's box at rest, canvas units: one that draws, hit at its middle, is there.
    let boxes = call(s, "boxes", json!({ "state": state }));
    let drawn = (boxes.as_array().unwrap().iter())
        .rfind(|b| b["draws"] == true)
        .unwrap_or_else(|| panic!("a node draws in {state}: {boxes}"))
        .clone();
    let node = drawn["node"].as_str().unwrap();
    let rect: Vec<f64> = drawn["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    let (x, y) = (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);
    let hits = call(s, "hit", json!({ "state": state, "x": x, "y": y }));
    assert!(hits.as_array().unwrap().iter().any(|h| h["node"] == node), "{node} at {x}, {y}: {hits}");
    assert!(hits.as_array().unwrap().iter().all(|h| h.get("locked").is_none()), "{hits}");

    // Locked by its own `locked`, as the canvas locks it (PLAN 2.95): a pointer passes over it.
    let locked = tool(
        s,
        "deck_patch",
        json!({ "ops": [{ "op": "add", "path": format!("/nodes/{node}/locked"), "value": true }] }),
    );
    assert_eq!(locked["edited"], true, "{locked}");
    let hits = call(s, "hit", json!({ "state": state, "x": x, "y": y }));
    let hit = hits.as_array().unwrap().iter().find(|h| h["node"] == node).unwrap();
    assert_eq!(hit["locked"], node, "{hits}");
    let boxes = call(s, "boxes", json!({ "state": state }));
    let shown = boxes.as_array().unwrap().iter().find(|b| b["node"] == node).unwrap();
    assert_eq!(shown["locked"], node, "{boxes}");

    // A point that is no number is an error that says so.
    let answer = took(unsafe {
        scaena_call(s, c("hit").as_ptr(), c(&json!({ "state": state, "x": "left" }).to_string()).as_ptr())
    });
    assert!(answer.contains("`x` is a number"), "{answer}");
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
fn a_new_deck_starts_from_a_theme_that_ships() {
    let mut error = null_mut();
    let s = unsafe { scaena_create(c("Dusk").as_ptr(), c("Trail report").as_ptr(), &mut error) };
    assert!(!s.is_null(), "made: {}", took(error));
    let states = call(s, "states", Value::Null);
    assert_eq!(states.as_array().map(Vec::len), Some(1), "{states}");
    let files = call(s, "files", Value::Null);
    let files = files.as_array().unwrap();
    assert!(files.contains(&json!("themes/dusk.theme.json")), "{files:?}");
    assert!(files.iter().any(|f| f.as_str().is_some_and(|f| f.starts_with("fonts/"))), "{files:?}");
    let source = call(s, "source", Value::Null);
    assert!(source.as_str().unwrap().contains("Trail report"), "titled");
    let state = c(states[0].as_str().unwrap());
    let pixels = unsafe { scaena_pixels(s, state.as_ptr(), f64::INFINITY, 64, &mut error) };
    assert_eq!((pixels.width, pixels.height), (64, 36));
    unsafe { scaena_bytes_free(pixels.bytes) };
    unsafe { scaena_session_free(s) };

    let mut error = null_mut();
    assert!(unsafe { scaena_create(c("sepia").as_ptr(), c("x").as_ptr(), &mut error) }.is_null());
    let said = took(error);
    assert!(said.contains("not a theme that ships") && said.contains("dusk"), "{said}");
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

    // A surface needs a layer, and off the Mac there is none to paint on: an error, said.
    let mut said = null_mut();
    assert!(unsafe { scaena_surface_new(null_mut(), 320, 180, &mut said) }.is_null());
    assert!(took(said).contains("layer is null"));
    #[cfg(not(target_vendor = "apple"))]
    {
        let mut not_a_layer = 0_u64;
        let mut said = null_mut();
        let surface = unsafe { scaena_surface_new((&raw mut not_a_layer).cast(), 320, 180, &mut said) };
        assert!(surface.is_null());
        assert!(took(said).contains("on the Mac"));
        // Nor a GPU for the surfaces to share.
        let mut said = null_mut();
        assert!(!unsafe { scaena_gpu_warm(&mut said) });
        assert!(took(said).contains("on the Mac"));
        assert!(!unsafe { scaena_gpu_warm(null_mut()) }, "an error with nowhere to say it is still one");
    }
    let mut said = null_mut();
    assert_eq!(unsafe { scaena_surface_paint(null_mut(), s, c("cover").as_ptr(), 0.0, &mut said) }, -1);
    assert!(took(said).contains("surface is null"));

    // Freeing nothing is nothing.
    unsafe {
        scaena_string_free(null_mut());
        scaena_bytes_free(ScaenaBytes { data: null_mut(), len: 0 });
        scaena_session_free(null_mut());
        scaena_saved_free(null_mut());
        scaena_files_free(null_mut());
        scaena_surface_free(null_mut());
        scaena_session_free(s);
    }
}

/// A step of the conversation `chat`: its `ok`, or a panic with its error.
fn step(chat: *mut ScaenaChat, s: *mut ScaenaSession, method: &str, args: Value) -> Value {
    let answer = stepped(chat, s, method, args);
    assert!(answer.get("error").is_none(), "{method}: {answer}");
    answer["ok"].clone()
}

/// A step of the conversation `chat`, as the envelope it returns.
fn stepped(chat: *mut ScaenaChat, s: *mut ScaenaSession, method: &str, args: Value) -> Value {
    let called = unsafe { scaena_chat_call(chat, s, c(method).as_ptr(), c(&args.to_string()).as_ptr()) };
    serde_json::from_str(&took(called)).unwrap()
}

fn providers(method: &str, args: Value) -> Value {
    let answer: Value =
        serde_json::from_str(&took(unsafe { scaena_providers(c(method).as_ptr(), c(&args.to_string()).as_ptr()) }))
            .unwrap();
    assert!(answer.get("error").is_none(), "{method}: {answer}");
    answer["ok"].clone()
}

/// An answer of Anthropic's that calls `calls`, each `(id, name, input)`.
fn calling(calls: &[(&str, &str, Value)]) -> String {
    let mut content = vec![json!({ "type": "text", "text": "Looking." })];
    for (id, name, input) in calls {
        content.push(json!({ "type": "tool_use", "id": id, "name": name, "input": input }));
    }
    json!({ "content": content, "stop_reason": "tool_use" }).to_string()
}

/// The request's body, read back.
fn body_of(request: &Value) -> Value {
    serde_json::from_str(request["body"].as_str().unwrap()).unwrap()
}

#[test]
fn the_assistant_asks_with_the_users_key_and_runs_each_call_on_the_bundle() {
    // The providers, outside a conversation: the models a key can use.
    let listed = providers("list", Value::Null);
    let ids: Vec<&str> = listed.as_array().unwrap().iter().map(|p| p["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["anthropic", "openai", "gemini"]);
    let models = providers("models", json!({ "provider": "openai", "key": "sk-test" }));
    assert_eq!(models["url"], "https://api.openai.com/v1/models");
    assert_eq!(models["headers"]["authorization"], "Bearer sk-test");
    let read = providers(
        "readModels",
        json!({ "provider": "openai", "status": 200, "body": r#"{"data":[{"id":"b"},{"id":"a"}]}"# }),
    );
    assert_eq!(read, json!(["a", "b"]));

    let s = open(B1);
    let mut error = null_mut();
    let chat = unsafe { scaena_chat_new(c(r#"{"provider":"anthropic","model":"a-model"}"#).as_ptr(), &mut error) };
    assert!(!chat.is_null(), "a conversation begins: {}", took(error));

    // The question, begun with what the window shows; the model told the deck as it is.
    let seeing = json!({ "state": "cover", "nodes": [{ "node": "title", "type": "text" }] });
    step(chat, s, "ask", json!({ "text": "Is the cover clean?", "seeing": seeing }));
    let request = step(chat, null_mut(), "request", json!({ "key": "sk-test" }));
    assert_eq!(request["url"], "https://api.anthropic.com/v1/messages");
    assert_eq!(request["headers"]["x-api-key"], "sk-test");
    assert!(request["headers"].get("anthropic-dangerous-direct-browser-access").is_none(), "the Mac is no browser");
    let body = body_of(&request);
    let system = body["system"][0]["text"].as_str().unwrap();
    assert!(system.starts_with("You are the assistant in Scaena's Mac app"), "{system}");
    assert!(system.contains("The deck: \"B1: the manifesto as a talk\", 40 states (cover, goal, goal-why"), "{system}");
    assert!(system.contains("--- scaena://skills/author-deck ---"));
    let question = body["messages"][0]["content"][0]["text"].as_str().unwrap();
    assert_eq!(question, "[In the window: state cover shown; selected: title (text).]\n\nIs the cover clean?");
    let tools: Vec<&str> = body["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(tools.len(), 13, "the page's twelve and resource_read: {tools:?}");
    assert_eq!(tools.last(), Some(&"resource_read"));

    // The model calls three tools: each runs on the bundle, as the browser runs it.
    let answer = calling(&[
        ("t1", "deck_lint", json!({ "state": "cover" })),
        ("t2", "deck_render", json!({ "state": "cover" })),
        ("t3", "resource_read", json!({ "uri": "scaena://spec" })),
    ]);
    let next = step(chat, null_mut(), "answer", json!({ "status": 200, "body": answer }));
    assert_eq!(next["next"], "calls", "{next}");
    assert_eq!(next["text"], "Looking.");
    let at = "2026-10-08T12:00:00Z";
    let mut ran = Vec::new();
    for index in 0..next["calls"].as_array().unwrap().len() {
        ran.push(step(chat, s, "run", json!({ "index": index, "at": at })));
    }
    assert!(stepped(chat, s, "run", json!({ "index": 3 }))["error"]["message"].is_string(), "three calls, not four");
    assert_eq!(ran[0]["error"], false, "{}", ran[0]);
    assert!(ran[0]["summary"].as_str().unwrap().contains("found") || ran[0]["summary"] == "nothing found");
    assert!(ran[1]["png"].as_str().unwrap().starts_with("iVBORw0KGgo"), "deck_render's frame, a PNG in base64");
    assert!(ran[1]["summary"].as_str().unwrap().starts_with("cover, "), "{}", ran[1]["summary"]);
    assert!(ran[2]["summary"].as_str().unwrap().ends_with(" characters"), "{}", ran[2]);
    assert!(ran.iter().all(|r| r["edited"] == false));
    assert_eq!(step(chat, null_mut(), "next", Value::Null), json!(true), "one round taken of 32");

    // The next request carries each result, the frame as an image.
    let body = body_of(&step(chat, null_mut(), "request", json!({ "key": "sk-test" })));
    let results = body["messages"].as_array().unwrap().last().unwrap()["content"].clone();
    assert_eq!(results.as_array().unwrap().len(), 3, "{results}");
    assert_eq!(results[1]["tool_use_id"], "t2");
    assert_eq!(results[1]["content"][1]["type"], "image");
    let done = json!({ "content": [{ "type": "text", "text": "Clean." }], "stop_reason": "end_turn" });
    let next = step(chat, null_mut(), "answer", json!({ "status": 200, "body": done.to_string() }));
    assert_eq!(next, json!({ "next": "done", "text": "Clean.", "stop": "end", "usage": null }));

    // A text's words as written, which the on-device model tightens (PLAN 3.6).
    let carets = call(s, "carets", json!({ "state": "cover", "node": "title" }));
    assert!(!carets["text"].as_str().unwrap().is_empty(), "{carets}");
    assert_eq!(call(s, "carets", json!({ "state": "cover", "node": "nothing-here" })), Value::Null);

    // An edit the model makes is the agent's, and changes the deck; another model goes on with
    // the conversation.
    step(chat, null_mut(), "use", json!({ "provider": "openai", "model": "another" }));
    let request = step(chat, null_mut(), "request", json!({ "key": "sk-test" }));
    assert_eq!(request["url"], "https://api.openai.com/v1/chat/completions");
    let messages = body_of(&request)["messages"].as_array().unwrap().len();
    assert_eq!(messages, 8, "the system, the question, the calls, a result each, the frame as an image, the answer");
    step(chat, null_mut(), "use", json!({ "provider": "anthropic", "model": "a-model" }));
    step(chat, s, "ask", json!({ "text": "Say it louder." }));
    let inspected = call(s, "inspect", json!({ "state": "cover" }));
    let node = inspected["looks"].as_object().unwrap().keys().next().unwrap().clone();
    let op = json!({ "op": "replace_text", "state": "cover", "node": node, "from": 0, "to": 0, "text": "Loudly: " });
    let answer = calling(&[("t4", "deck_patch", json!({ "ops": [op] }))]);
    let next = step(chat, null_mut(), "answer", json!({ "status": 200, "body": answer }));
    let patched = step(chat, s, "run", json!({ "call": next["calls"][0], "at": at }));
    assert_eq!(patched["edited"], true, "{patched}");
    assert_eq!(patched["summary"], "applied");
    assert!(call(s, "source", Value::Null).as_str().unwrap().contains("Loudly: "));
    step(chat, null_mut(), "next", Value::Null);

    // The user stops it: the call never run is answered as stopped, so every call has an answer.
    step(chat, s, "ask", json!({ "text": "Two things." }));
    let answer = calling(&[("t5", "deck_lint", json!({})), ("t6", "deck_render", json!({ "state": "cover" }))]);
    let next = step(chat, null_mut(), "answer", json!({ "status": 200, "body": answer }));
    step(chat, s, "run", json!({ "call": next["calls"][0], "at": at }));
    step(chat, null_mut(), "next", Value::Null);
    let conversation = step(chat, null_mut(), "conversation", Value::Null);
    let last = conversation.as_array().unwrap().last().unwrap();
    assert_eq!(last["role"], "tool");
    assert_eq!(last["results"][0]["id"], "t5");
    assert_eq!(last["results"][0]["error"], false);
    assert_eq!(last["results"][1]["id"], "t6");
    assert_eq!(last["results"][1]["error"], true);
    assert!(last["results"][1]["json"].as_str().unwrap().contains("the user stopped the assistant"));

    // A provider's refusal is an error, and the conversation is as it was.
    let refused = stepped(
        chat,
        null_mut(),
        "answer",
        json!({ "status": 401, "statusText": "Unauthorized", "body": r#"{"error":{"message":"invalid x-api-key"}}"# }),
    );
    assert_eq!(refused["error"]["message"], "401 Unauthorized: invalid x-api-key");
    let kept = step(chat, null_mut(), "conversation", Value::Null);
    assert_eq!(kept, conversation);

    // Misuse is an error, never a crash.
    assert!(stepped(chat, null_mut(), "next", Value::Null)["error"]["message"].is_string(), "no calls wait");
    assert!(stepped(chat, null_mut(), "ask", json!({ "text": "Hi" }))["error"]["message"].is_string(), "no session");
    assert!(stepped(chat, s, "dance", Value::Null)["error"]["message"].is_string());
    let mut error = null_mut();
    let none = unsafe { scaena_chat_new(c(r#"{"provider":"acme","model":"m"}"#).as_ptr(), &mut error) };
    assert!(none.is_null());
    assert!(took(error).contains("anthropic, openai, or gemini"));
    step(chat, null_mut(), "forget", Value::Null);
    assert_eq!(step(chat, null_mut(), "conversation", Value::Null), json!([]));
    unsafe {
        scaena_chat_free(chat);
        scaena_chat_free(null_mut());
        scaena_session_free(s);
    }
}

#[test]
fn an_assistants_theme_edit_is_written_back_by_an_undo() {
    let s = open(B1);
    let held = call(s, "themeText", Value::Null);
    let before = held["text"].as_str().unwrap().to_string();
    let edited =
        tool(s, "theme_edit", json!({ "ops": [{ "op": "replace", "path": "/description", "value": "Louder" }] }));
    assert!(edited.get("error").is_none(), "{edited}");
    let after = call(s, "themeText", Value::Null)["text"].as_str().unwrap().to_string();
    assert_ne!(after, before, "the theme was edited");
    call(s, "writeFiles", json!({ "files": [{ "path": held["theme"].as_str().unwrap(), "text": before }] }));
    assert_eq!(call(s, "themeText", Value::Null)["text"].as_str().unwrap(), before);
    unsafe { scaena_session_free(s) };
}

#[test]
fn a_drag_on_the_canvas_ends_in_the_patch_the_browsers_does() {
    let s = open(B1);
    // Where the title may go: on the theme's grid, in its slot, or off the grid as a `rect`.
    let targets = call(s, "targets", json!({ "state": "cover", "node": "title" }));
    assert_eq!(targets["by"], "grid", "{targets}");
    let slots = targets["slots"].as_object().unwrap();
    assert!(slots.contains_key("title") && slots.contains_key("subtitle"), "{targets}");
    let cell: Vec<f64> = serde_json::from_value(targets["cell"].clone()).unwrap();

    // As it moves, the title is drawn moved, laying nothing out, and drawn as it was after.
    let rest = call(s, "digest", json!({ "state": "cover" }));
    call(s, "setMoving", json!({ "nodes": ["title"], "dx": 40, "dy": 30 }));
    let moving = call(s, "digest", json!({ "state": "cover" }));
    assert_ne!(moving, rest, "the moved title draws elsewhere");
    call(s, "setMoving", json!({ "nodes": [] }));
    assert_eq!(call(s, "digest", json!({ "state": "cover" })), rest);

    // Into the slot it covers most: one `place`, in the state shown.
    let over = slots["subtitle"].as_array().unwrap();
    let to = json!({ "state": "cover", "node": "title", "how": "slot", "x": over[0], "y": over[1], "w": over[2], "h": over[3] });
    let slotted = call(s, "snap", to);
    assert_eq!(slotted["patch"][0]["op"], "place", "{slotted}");
    assert_eq!(slotted["patch"][0]["at"]["in"], "subtitle", "{slotted}");

    // Off the grid (Shift): a `rect` where it was left, an override lint flags (W301), kept to
    // the state shown with Alt (`fork`).
    let left = json!({
        "state": "cover", "node": "title", "how": "free", "x": cell[0] + 40.0, "y": cell[1] + 30.0,
        "w": cell[2], "h": cell[3], "fork": true, "reach": 6,
    });
    let freed = call(s, "snap", left);
    let patch = freed["patch"].clone();
    assert!(patch[0]["at"]["rect"].is_array(), "{freed}");
    let reach = call(s, "reach", json!({ "ops": patch }));
    assert!(reach.as_array().unwrap().iter().any(|st| st == "cover"), "{reach}");

    // Shown before it is made: frames at rest draw the patch, until it is let go.
    call(s, "preview", json!({ "ops": patch }));
    let previewed = call(s, "digest", json!({ "state": "cover" }));
    assert_ne!(previewed, rest, "the preview draws the title where the patch puts it");
    call(s, "preview", json!({ "ops": null }));
    assert_eq!(call(s, "digest", json!({ "state": "cover" })), rest);

    // Made: the title is off the grid, and lint says so.
    let made = tool(s, "deck_patch", json!({ "ops": patch }));
    assert_eq!(made["edited"], true, "{made}");
    assert_eq!(call(s, "digest", json!({ "state": "cover" })), previewed, "made as previewed");
    // The editor compiles the deck's source after a patch, then lints it.
    let source = call(s, "source", Value::Null);
    call(s, "compile", json!({ "source": source }));
    let linted = call(s, "lint", json!({ "state": "cover" }));
    let flagged = linted["findings"].as_array().unwrap().iter().any(|f| f["code"] == "W301" && f["node"] == "title");
    assert!(flagged, "an off-grid placement is flagged: {linted}");

    // Misuse is said.
    let answer: Value = serde_json::from_str(&took(unsafe {
        scaena_call(
            s,
            c("snap").as_ptr(),
            c(r#"{"state":"cover","node":"title","how":"sideways","x":0,"y":0,"w":1,"h":1}"#).as_ptr(),
        )
    }))
    .unwrap();
    assert!(answer["error"]["message"].is_string(), "{answer}");
    unsafe { scaena_session_free(s) };
}

/// Text typed in place on the canvas (PLAN 3.9), as the browser's typing makes it (PLAN 2.32):
/// each change a `replace_text` by the user, validated but not linted; ⌘B's and ⌘I's looks given
/// as a `style_text`; what reads so already, or what the deck refuses, changes nothing.
#[test]
fn text_typed_in_place_is_the_patch_the_browsers_typing_makes() {
    let s = open(B1);
    let carets = call(s, "carets", json!({ "state": "cover", "node": "title" }));
    assert_eq!(carets["text"], "Scaena", "{carets}");

    // Words typed after it: one `replace_text`, its offsets in characters.
    let op =
        json!({ "op": "replace_text", "node": "title", "state": "cover", "from": 6, "to": 6, "text": " on the Mac" });
    assert_eq!(call(s, "typed", json!({ "ops": [op], "at": "2026-10-08T12:00:00Z" })), true);
    let typed = call(s, "carets", json!({ "state": "cover", "node": "title" }));
    assert_eq!(typed["text"], "Scaena on the Mac", "{typed}");
    let source = call(s, "source", Value::Null);
    assert!(source.as_str().unwrap().contains("Scaena on the Mac"), "the source says it: {source}");

    // The same characters typed over themselves: it reads so already.
    let same = json!({ "op": "replace_text", "node": "title", "state": "cover", "from": 0, "to": 6, "text": "Scaena" });
    assert_eq!(call(s, "typed", json!({ "ops": [same] })), false);

    // ⌘B and ⌘I: the look each gives "on", given as one `style_text`.
    let bolding = call(s, "bolding", json!({ "state": "cover", "node": "title", "from": 7, "to": 9 }));
    assert!(bolding.get("style/weight").is_some(), "{bolding}");
    let bold = json!({ "op": "style_text", "node": "title", "state": "cover", "from": 7, "to": 9, "look": bolding });
    assert_eq!(call(s, "typed", json!({ "ops": [bold] })), true);
    let italicizing = call(s, "italicizing", json!({ "state": "cover", "node": "title", "from": 7, "to": 9 }));
    assert!(italicizing.get("style/italic").is_some(), "{italicizing}");
    let styled = call(s, "carets", json!({ "state": "cover", "node": "title" }));
    assert_eq!(styled["text"], "Scaena on the Mac", "a look leaves the characters as they are");

    // What the deck cannot take is said, and the deck stays as it was.
    let nowhere = json!({ "ops": [{ "op": "replace_text", "node": "nobody", "state": "cover", "from": 0, "to": 0, "text": "x" }] });
    let refused: Value =
        serde_json::from_str(&took(unsafe { scaena_call(s, c("typed").as_ptr(), c(&nowhere.to_string()).as_ptr()) }))
            .unwrap();
    assert!(refused["error"]["message"].is_string(), "{refused}");
    let missing: Value =
        serde_json::from_str(&took(unsafe { scaena_call(s, c("typed").as_ptr(), c("{}").as_ptr()) })).unwrap();
    assert!(missing["error"]["message"].as_str().unwrap().contains("ops"), "{missing}");
    assert_eq!(call(s, "carets", json!({ "state": "cover", "node": "title" }))["text"], "Scaena on the Mac");
    unsafe { scaena_session_free(s) };
}

/// A text's characters (PLAN 3.10), as the browser's editor gives them (PLAN 2.38, 2.69, 2.70):
/// what the inspector offers for them, a link given them and found where it is drawn, and their
/// paragraphs made a list's items.
#[test]
fn a_texts_characters_take_a_look_a_link_and_a_list() {
    let s = open(B1);
    let offered = call(s, "characterChoices", json!({ "state": "cover", "node": "title", "from": 0, "to": 6 }));
    assert_eq!(offered["node"], "title", "{offered}");
    let props: Vec<&str> = offered["fields"].as_array().unwrap().iter().filter_map(|f| f["prop"].as_str()).collect();
    assert!(props.contains(&"role") && props.contains(&"style/color"), "{props:?}");

    // ⌘K: the title links to a state, one `style_text`; a click on it goes there.
    let link = json!({ "op": "style_text", "node": "title", "state": "cover", "from": 0, "to": 6, "look": { "link": { "state": "goal" } } });
    assert_eq!(call(s, "typed", json!({ "ops": [link] })), true);
    assert_eq!(call(s, "linkAt", json!({ "state": "cover", "x": 300, "y": 300 })), json!({ "state": "goal" }));
    assert_eq!(call(s, "linkAt", json!({ "state": "cover", "x": 10, "y": 10 })), Value::Null, "off every link");

    // ⌘⇧8: the subtitle's paragraph a bullet, one `list`.
    let list = json!({ "op": "list", "node": "subtitle", "state": "cover", "from": 0, "to": 0, "kind": "bullet" });
    assert_eq!(call(s, "typed", json!({ "ops": [list] })), true);
    let carets = call(s, "carets", json!({ "state": "cover", "node": "subtitle" }));
    assert_eq!(carets["items"][0]["kind"], "bullet", "{carets}");

    // No characters selected, or no text: said.
    for (node, from, to) in [("title", 3, 3), ("nobody", 0, 1)] {
        let args = json!({ "state": "cover", "node": node, "from": from, "to": to }).to_string();
        let answer: Value =
            serde_json::from_str(&took(unsafe { scaena_call(s, c("characterChoices").as_ptr(), c(&args).as_ptr()) }))
                .unwrap();
        assert!(answer["error"]["message"].is_string(), "{answer}");
    }
    unsafe { scaena_session_free(s) };
}

/// Nodes added and taken away (PLAN 3.11), as the browser's Insert menu, ⌘D, and Delete make them
/// (PLAN 2.34, 2.79): what is offered, inserted about a point and entering in the state shown;
/// a copy beside a node; and a node taken out of the state and those after, or out of the deck.
#[test]
fn a_node_is_inserted_copied_and_taken_away_as_the_browser_does() {
    let s = open(B1);
    let offered = call(s, "inserts", Value::Null);
    let n = offered.as_array().unwrap().iter().position(|i| i["label"] == "Shape · rect").expect("a rect is offered");

    let added = call(s, "inserting", json!({ "state": "cover", "n": n, "x": 1500, "y": 900 }));
    let id = added["id"].as_str().unwrap().to_string();
    assert_eq!(added["patch"][0]["op"], "add_node", "{added}");
    assert_eq!(added["cell"].as_array().map(Vec::len), Some(4), "{added}");
    let made = tool(s, "deck_patch", json!({ "ops": added["patch"] }));
    assert_eq!(made["edited"], true, "{made}");
    let shown = call(s, "boxes", json!({ "state": "cover" }));
    assert!(shown.as_array().unwrap().iter().any(|b| b["node"] == id.as_str()), "{shown}");

    // ⌘D: a copy of it beside it.
    let copy = call(s, "duplicating", json!({ "state": "cover", "node": id }));
    assert_ne!(copy["id"], id.as_str(), "{copy}");
    assert_eq!(tool(s, "deck_patch", json!({ "ops": copy["patch"] }))["edited"], true);

    // Delete: shown in no state after this one, it goes from the deck; a headline the states
    // after show goes from this one on, and with Shift from the deck.
    let gone = call(s, "deleting", json!({ "state": "cover", "node": id }));
    assert!(gone.as_array().unwrap().iter().all(|op| op["op"] == "remove_node"), "{gone}");
    let hidden = call(s, "deleting", json!({ "state": "goal-why", "node": "headline" }));
    assert!(hidden.as_array().unwrap().iter().all(|op| op["op"] != "remove_node"), "{hidden}");
    let everywhere = call(s, "deleting", json!({ "state": "goal-why", "node": "headline", "everywhere": true }));
    assert!(everywhere.as_array().unwrap().iter().all(|op| op["op"] == "remove_node"), "{everywhere}");

    // Nothing offered there, or no such node: said.
    for (method, args) in [
        ("inserting", json!({ "state": "cover", "n": 999, "x": 0, "y": 0 })),
        ("duplicating", json!({ "state": "cover", "node": "nobody" })),
        ("deleting", json!({ "state": "cover", "node": "nobody" })),
    ] {
        let answer: Value =
            serde_json::from_str(&took(unsafe { scaena_call(s, c(method).as_ptr(), c(&args.to_string()).as_ptr()) }))
                .unwrap();
        assert!(answer["error"]["message"].is_string(), "{method}: {answer}");
    }
    unsafe { scaena_session_free(s) };
}

/// The clipboard (PLAN 3.12), as the browser's (PLAN 2.37, 2.58, 2.96): a node copied as a clip
/// and pasted in another state under an id new to the deck; words from another app pasted as a
/// text; a look copied and put on another node; a sheet's cells read as a source, its file
/// dropped and attached; and a picture dropped, kept by its content.
#[test]
fn the_clipboard_copies_and_pastes_as_the_browsers_does() {
    let s = open(B1);
    let clip = call(s, "copying", json!({ "state": "cover", "nodes": ["title"] }));
    let clip = clip.as_str().expect("a clip is the text the clipboard holds");
    assert_eq!(serde_json::from_str::<Value>(clip).unwrap()["kind"], "scaena/clip", "{clip}");
    let pasted = call(s, "pasting", json!({ "text": clip, "state": "goal", "x": 960, "y": 540 }));
    let id = pasted["id"].as_str().unwrap().to_string();
    assert_ne!(id, "title");
    assert_eq!(tool(s, "deck_patch", json!({ "ops": pasted["patch"] }))["edited"], true);
    let shown = call(s, "boxes", json!({ "state": "goal" }));
    assert!(shown.as_array().unwrap().iter().any(|b| b["node"] == id.as_str()), "{shown}");

    let words = call(s, "pasting", json!({ "text": "Words from elsewhere", "state": "goal", "x": 100, "y": 100 }));
    assert_eq!(words["patch"][0]["node"]["type"], "text", "{words}");

    // ⌥⌘C on the title, ⌥⌘V on the subtitle: one patch of `choose`s.
    let look = call(s, "look", json!({ "state": "cover", "node": "title" }));
    let put = call(s, "putting", json!({ "state": "cover", "look": look, "nodes": ["subtitle"] }));
    assert!(put["patch"].as_array().is_some_and(|p| !p.is_empty()), "{put}");

    // A sheet's cells: the source they would be; words are none.
    let cells = call(s, "cells", json!({ "text": "Quarter\tSales\nQ1\t1,200\nQ2\t1,450\n" }));
    assert_eq!(cells["rows"], 2, "{cells}");
    assert_eq!(call(s, "cells", json!({ "text": "just words" })), Value::Null);
    let name = format!("{}.csv", cells["name"].as_str().unwrap());
    let csv = cells["csv"].as_str().unwrap().as_bytes().to_vec();
    let dropped: Value =
        serde_json::from_str(&took(unsafe { scaena_drop(s, c(&name).as_ptr(), csv.as_ptr(), csv.len()) })).unwrap();
    let path = dropped["ok"].as_str().unwrap().to_string();
    assert!(path.starts_with("data/"), "{dropped}");
    let attaching = call(s, "attaching", json!({ "path": path, "schema": cells["schema"] }));
    assert!(attaching["patch"].as_array().is_some_and(|p| !p.is_empty()), "{attaching}");
    assert_eq!(tool(s, "deck_patch", json!({ "ops": attaching["patch"] }))["edited"], true);

    // A picture, kept under its content's name.
    let picture = std::fs::read(format!("{TORTURE}/assets/test-card.png")).unwrap();
    let kept: Value = serde_json::from_str(&took(unsafe {
        scaena_drop(s, c("Screenshot.png").as_ptr(), picture.as_ptr(), picture.len())
    }))
    .unwrap();
    let image = kept["ok"].as_str().unwrap();
    assert!(image.starts_with("assets/") && image.ends_with(".png"), "{kept}");
    let offered = call(s, "inserts", Value::Null);
    let insertable =
        offered.as_array().unwrap().iter().any(|i| i["node"]["type"] == "image" && i["node"]["src"] == image);
    assert!(insertable, "Insert offers it as an image: {offered}");

    // What is not a clip's, a look's, or a list of nodes: said.
    for (method, args) in [
        ("copying", json!({ "state": "cover", "nodes": "title" })),
        ("putting", json!({ "state": "cover", "look": 3, "nodes": ["subtitle"] })),
    ] {
        let answer: Value =
            serde_json::from_str(&took(unsafe { scaena_call(s, c(method).as_ptr(), c(&args.to_string()).as_ptr()) }))
                .unwrap();
        assert!(answer["error"]["message"].is_string(), "{method}: {answer}");
    }
    unsafe { scaena_session_free(s) };
}

/// Several selected (PLAN 3.13), as the browser's canvas and inspector take them (PLAN 2.42,
/// 2.43): moved together as a drag moves the first; aligned, spread, and ordered; and grouped
/// where they stand, then taken apart.
#[test]
fn several_nodes_move_arrange_and_group_together() {
    let s = open(B1);
    let nodes = json!(["kicker", "headline", "body"]);
    let moved = call(s, "together", json!({ "state": "goal-bar", "nodes": nodes, "dx": 200, "dy": 0, "free": true }));
    assert!(moved["patch"].as_array().is_some_and(|p| !p.is_empty()), "{moved}");
    assert_eq!(moved["landed"].as_array().map(Vec::len), Some(3), "{moved}");
    for how in [json!({ "align": "left" }), json!({ "spread": "down" }), json!({ "order": "front" })] {
        let arranged = call(s, "arranging", json!({ "state": "goal-bar", "nodes": nodes, "how": how }));
        assert!(arranged.is_null() || arranged["patch"].is_array(), "{how}: {arranged}");
    }

    let grouped = call(s, "grouping", json!({ "state": "goal-bar", "nodes": nodes }));
    let id = grouped["id"].as_str().unwrap().to_string();
    assert_eq!(tool(s, "deck_patch", json!({ "ops": grouped["patch"] }))["edited"], true);
    let boxes = call(s, "boxes", json!({ "state": "goal-bar" }));
    assert!(boxes.as_array().unwrap().iter().any(|b| b["node"] == "kicker" && b["parent"] == id.as_str()), "{boxes}");
    let ungrouped = tool(s, "deck_patch", json!({ "ops": [{ "op": "ungroup", "group": id }] }));
    assert_eq!(ungrouped["edited"], true, "{ungrouped}");

    // A way to arrange them that is none: said.
    let args = json!({ "state": "goal-bar", "nodes": nodes, "how": {} }).to_string();
    let answer: Value =
        serde_json::from_str(&took(unsafe { scaena_call(s, c("arranging").as_ptr(), c(&args).as_ptr()) })).unwrap();
    assert!(answer["error"]["message"].is_string(), "{answer}");
    unsafe { scaena_session_free(s) };
}

/// The states in the order the timeline plays them, each with its slide: `state/slide`.
fn playing(s: *mut ScaenaSession) -> Vec<String> {
    let slots = call(s, "timeline", Value::Null);
    let slot = |x: &Value| format!("{}/{}", x["state"].as_str().unwrap(), x["slide"].as_str().unwrap());
    slots.as_array().unwrap().iter().map(slot).collect()
}

/// States and slides (PLAN 3.14), as the browser's strip, light table, rehearsal, and cue make
/// them (PLAN 2.35, 2.97, 2.63, 2.44): a step and a slide added after the state shown, a state
/// renamed, moved, and removed; a slide moved, copied, and taken out; a hold kept; and a motion
/// timed, the transition's end moved, and a preset added. Each is a patch.
#[test]
fn states_and_slides_are_added_moved_and_timed_as_the_browsers_are() {
    let s = open(B1);
    let patch = |s, ops: &Value| {
        let made = tool(s, "deck_patch", json!({ "ops": ops }));
        assert_eq!(made["edited"], true, "{ops}: {made}");
    };
    let step = call(s, "addingState", json!({ "state": "cover", "what": "step" }));
    assert_eq!(step["id"], "cover-2", "{step}");
    assert_eq!(step["patch"][0]["op"], "add_state", "{step}");
    patch(s, &step["patch"]);
    let slide = call(s, "addingState", json!({ "state": "goal", "what": "slide" }));
    assert_eq!(slide["id"], "slide", "{slide}");
    patch(s, &slide["patch"]);
    assert_eq!(
        playing(s)[..6],
        ["cover/cover", "cover-2/cover", "goal/goal", "goal-why/goal", "goal-bar/goal", "slide/slide"]
    );
    for (args, why) in [
        (json!({ "state": "cover", "what": "chapter" }), "step"),
        (json!({ "state": "nowhere", "what": "step" }), "nowhere"),
    ] {
        let said = answered(s, "addingState", &args);
        assert!(said["error"]["message"].as_str().is_some_and(|m| m.contains(why)), "{args}: {said}");
    }

    // The strip: renamed, moved before the cover, and removed.
    patch(s, &json!([{ "op": "rename_state", "id": "cover-2", "to": "opening" }]));
    patch(s, &json!([{ "op": "move_state", "id": "opening", "before": "cover" }]));
    assert_eq!(playing(s)[..2], ["opening/cover", "cover/cover"]);
    patch(s, &json!([{ "op": "remove_state", "id": "opening" }]));
    assert_eq!(playing(s)[0], "cover/cover");

    // The light table: the new slide moved before `goal`, `goal` copied just after itself, and
    // the new slide taken out.
    patch(s, &json!([{ "op": "move_slide", "slide": "slide", "before": "goal" }]));
    assert_eq!(playing(s)[1], "slide/slide");
    patch(s, &json!([{ "op": "duplicate_slide", "slide": "goal" }]));
    assert_eq!(playing(s)[5..8], ["goal-2/goal-2", "goal-why-2/goal-2", "goal-bar-2/goal-2"]);
    patch(s, &json!([{ "op": "remove_slide", "slide": "slide" }]));
    assert_eq!(playing(s)[1], "goal/goal");

    // A rehearsal kept: the cover holds a second and a half.
    patch(s, &json!([{ "op": "set_state", "id": "cover", "prop": "hold", "value": 1500 }]));
    assert_eq!(call(s, "timeline", Value::Null)[0]["hold"], 1500.0);
    unsafe { scaena_session_free(s) };

    // The cue: the badge's flash waits 100 ms and lasts 600, the transition lasts 500, and the
    // title pulses for emphasis, a preset the theme offers.
    let t = open(TORTURE);
    let motion = json!({ "op": "time_motion", "node": "mf-badge", "motion": "emphasis", "state": "morph" });
    let timed = |part: &str, ms: u32| {
        let mut op = motion.clone();
        op[part] = json!(ms);
        json!([op])
    };
    patch(t, &timed("delay", 100));
    patch(t, &timed("duration", 600));
    patch(t, &json!([{ "op": "set_state", "id": "morph", "prop": "transition/duration", "value": 500 }]));
    let choices = call(t, "choices", json!({ "state": "morph", "node": "mf-title" }));
    let presets =
        choices["fields"].as_array().unwrap().iter().find(|f| f["prop"] == "enter").unwrap()["takes"]["names"].clone();
    assert!(presets.as_array().unwrap().contains(&json!("pulse")), "{presets}");
    patch(
        t,
        &json!([{ "op": "apply_preset", "node": "mf-title", "preset": "pulse", "motion": "emphasis", "state": "morph" }]),
    );
    let cue = call(t, "inspect", json!({ "state": "morph" }))["timeline"].clone();
    assert_eq!(cue["transition"]["duration"], 500.0, "{cue}");
    let badge = cue["motions"].as_array().unwrap().iter().find(|m| m["node"] == "mf-badge").unwrap().clone();
    assert_eq!(
        (&badge["delay"], &badge["duration"], &badge["start"]),
        (&json!(100.0), &json!(600.0), &json!(600.0)),
        "{badge}"
    );
    assert!(
        cue["motions"].as_array().unwrap().iter().any(|m| m["node"] == "mf-title" && m["motion"] == "emphasis"),
        "{cue}"
    );
    unsafe { scaena_session_free(t) };
}

/// The revenue example as a bundle: its deck, and the fonts, themes, data, and images the examples
/// share.
fn revenue() -> *mut ScaenaSession {
    let examples = Path::new("../../docs/examples");
    let files = scaena_files_new();
    let add = |path: &str, bytes: &[u8]| {
        assert!(unsafe { scaena_files_add(files, c(path).as_ptr(), bytes.as_ptr(), bytes.len()) });
    };
    add("deck.json", &std::fs::read(examples.join("revenue.deck.json")).unwrap());
    for dir in ["fonts", "themes", "data", "assets"] {
        for (path, bytes) in files_of(&examples.join(dir)) {
            add(&format!("{dir}/{path}"), &bytes);
        }
    }
    let mut error = null_mut();
    let session = unsafe { scaena_open(files, &mut error) };
    assert!(!session.is_null(), "the revenue example opens: {}", took(error));
    session
}

/// The theme panel (PLAN 3.15, as the browser's, PLAN 2.39, 2.61): the themes that ship, the deck
/// put in one, and the theme edited, with the file it wrote for the undo, and a dry run that
/// writes nothing.
#[test]
fn the_theme_is_edited_and_another_taken_as_the_browsers_theme_tab_does() {
    let shipped = call(revenue(), "shippedThemes", Value::Null);
    let names: Vec<&str> = shipped.as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
    assert_eq!(names, ["dusk", "daybreak", "ember"], "{shipped}");

    let s = revenue();
    let themed = call(s, "retheme", json!({ "ships": "daybreak", "at": "2026-10-08T12:00:00Z" }));
    assert_eq!(
        (&themed["applied"], &themed["theme"]),
        (&json!(true), &json!("themes/daybreak.theme.json")),
        "{themed}"
    );
    assert_eq!(call(s, "themes", Value::Null)["current"], "themes/daybreak.theme.json");
    let again = answered(s, "retheme", &json!({ "path": "themes/dusk.theme.json" }));
    assert_eq!(again["ok"]["applied"], true, "{again}");
    let nowhere = answered(s, "retheme", &json!({ "ships": "noon" }));
    assert!(nowhere["error"]["message"].as_str().is_some_and(|m| m.contains("ships")), "{nowhere}");

    let paper = json!([{ "op": "replace", "path": "/tokens/color/paper", "value": "#0B0B10" }]);
    let dry = call(s, "themeEdit", json!({ "ops": paper, "dryRun": true }));
    assert_eq!((&dry["edited"]["applied"], &dry["files"]), (&json!(false), &json!([])), "{dry}");
    let edited = call(s, "themeEdit", json!({ "ops": paper }));
    assert_eq!(edited["edited"]["applied"], true, "{edited}");
    let file = &edited["files"][0];
    assert_eq!(file["path"], "themes/dusk.theme.json", "{edited}");
    assert!(file["after"].as_str().is_some_and(|t| t.contains("#0B0B10")) && file["before"] != file["after"]);
    let text = call(s, "themeText", Value::Null);
    assert!(text["text"].as_str().is_some_and(|t| t.contains("#0B0B10")), "{text}");
    unsafe { scaena_session_free(s) };
}

/// The data and files panels (PLAN 3.15, as the browser's, PLAN 2.55, 2.59): a source as a sheet,
/// a cell set, undone and redone; a file nothing names taken out and put back.
#[test]
fn data_is_edited_and_files_taken_out_as_the_browsers_panels_do() {
    let s = open(TORTURE);
    let sources = call(s, "dataSources", Value::Null);
    assert!(sources.as_array().unwrap().contains(&json!({ "name": "bars", "file": "data/bars.csv" })), "{sources}");
    assert!(sources.as_array().unwrap().contains(&json!({ "name": "rev" })), "{sources}");
    let sheet = call(s, "dataSheet", json!({ "name": "bars" }));
    assert_eq!(sheet["file"], "data/bars.csv");
    assert_eq!(sheet["sheet"]["columns"][1]["name"], "value", "{sheet}");
    assert_eq!(sheet["sheet"]["rows"][0], json!(["2025-Q1", "12"]), "{sheet}");

    let set = json!([{ "op": "set", "row": 0, "column": "value", "value": "99" }]);
    let edited = call(s, "dataEdit", json!({ "source": "bars", "edits": set, "at": "2026-10-08T12:00:00Z" }));
    assert_eq!((&edited["result"]["edited"], &edited["wrote"]), (&json!(true), &json!(true)), "{edited}");
    let first = |s| call(s, "dataSheet", json!({ "name": "bars" }))["sheet"]["rows"][0][1].clone();
    assert_eq!(first(s), "99");
    assert_eq!(call(s, "dataUndo", Value::Null), "bars");
    assert_eq!(first(s), "12");
    assert_eq!(call(s, "dataUndo", json!({ "redo": true })), "bars");
    assert_eq!(first(s), "99");
    let refused = answered(
        s,
        "dataEdit",
        &json!({ "source": "bars", "edits": [{ "op": "set", "row": 0, "column": "value", "value": "lots" }] }),
    );
    assert!(refused.get("error").is_some() || refused["ok"]["result"]["edited"] == false, "{refused}");

    // A file nothing names, dropped in, is taken out and put back; one the deck names stays.
    let csv = b"a,b\n1,2\n";
    let path = took(unsafe { scaena_drop(s, c("extra.csv").as_ptr(), csv.as_ptr(), csv.len()) });
    let path = serde_json::from_str::<Value>(&path).unwrap()["ok"].as_str().unwrap().to_string();
    let listed = |s| call(s, "bundleFiles", Value::Null).as_array().unwrap().iter().any(|f| f["path"] == path.as_str());
    assert!(listed(s));
    assert_eq!(call(s, "removeFile", json!({ "path": path })), Value::Null);
    assert!(!listed(s));
    assert_eq!(call(s, "dataUndo", Value::Null), path.as_str());
    assert!(listed(s));
    let named = answered(s, "removeFile", &json!({ "path": "data/bars.csv" }));
    assert!(named["error"]["message"].is_string(), "{named}");
    unsafe { scaena_session_free(s) };
}

/// The versions panel (PLAN 3.15, as the browser's, PLAN 2.60): two saves keep two versions, read
/// from the bundle's history; the first shown and drawn, compared with the deck now, and made the
/// deck again.
#[test]
fn versions_are_listed_shown_compared_and_restored_from_the_history() {
    let s = open(B1);
    let none = answered(s, "versions", &Value::Null);
    assert!(none["error"]["message"].as_str().is_some_and(|m| m.contains("no history")), "{none}");
    call(s, "keepHistory", Value::Null);
    let save = |s, at: &str| {
        let mut error = null_mut();
        let saved = unsafe { scaena_save(s, c(at).as_ptr(), false, &mut error) };
        assert!(!saved.is_null(), "saved: {}", took(error));
        assert_eq!(took(unsafe { scaena_adopt(s, saved) }), r#"{"ok":null}"#);
        unsafe { scaena_saved_free(saved) };
    };
    save(s, "2026-10-08T12:00:00Z");
    let typed =
        json!([{ "op": "replace_text", "node": "title", "state": "cover", "from": 6, "to": 6, "text": " anew" }]);
    assert_eq!(call(s, "typed", json!({ "ops": typed })), true);
    save(s, "2026-10-08T12:05:00Z");
    let versions = call(s, "versions", Value::Null);
    assert_eq!(versions.as_array().map(Vec::len), Some(2), "{versions}");
    assert_eq!((&versions[0]["n"], &versions[1]["n"]), (&json!(1), &json!(2)));

    let states = call(s, "viewVersion", json!({ "version": "1" }));
    assert_eq!(states[0], "cover", "{states}");
    let mut error = null_mut();
    let png = bytes_of(unsafe {
        scaena_export(s, c("version").as_ptr(), c(r#"{"state":"cover","width":320}"#).as_ptr(), &mut error)
    })
    .expect("the version drawn");
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));

    let compared = call(s, "compareVersions", json!({ "from": "1" }));
    assert!(compared["states"]["cover"].is_object(), "the cover's title changed since: {compared}");
    let restored = call(s, "restoreVersion", json!({ "version": "1", "at": "2026-10-08T12:10:00Z" }));
    assert_eq!(restored["restored"]["applied"], true, "{restored}");
    assert!(!call(s, "source", Value::Null).as_str().unwrap().contains("anew"));
    let unknown = answered(s, "viewVersion", &json!({ "version": "9" }));
    assert!(unknown["error"]["message"].as_str().is_some_and(|m| m.contains("no version")), "{unknown}");
    unsafe { scaena_session_free(s) };
}

/// Handles and views (PLAN 3.16), as the browser's canvas reads them (PLAN 2.45, 2.46, 2.47, 2.57,
/// 2.62, 2.68, 2.74): a shape's points and a rect's corner, an image's framing and the point of it
/// under a press, the theme's grid, frames painted through a view, a format painted beside the
/// canvas, and texts found and replaced, every match or one.
#[test]
fn handles_and_views_read_as_the_browsers_canvas_reads_them() {
    let s = open(TORTURE);
    let rect = call(s, "outline", json!({ "state": "shapes", "node": "shape-panel" }));
    assert_eq!((&rect["kind"], &rect["radius"]), (&json!("rect"), &json!(16.0)), "{rect}");
    assert!(rect["radii"].as_array().is_some_and(|r| r.contains(&json!(32.0))), "the theme's steps: {rect}");
    let tri = call(s, "outline", json!({ "state": "shapes", "node": "shape-tri" }));
    assert_eq!((&tri["kind"], &tri["fewest"]), (&json!("polygon"), &json!(3)), "{tri}");
    assert_eq!(tri["points"], json!([[0.5, 0.0], [1.0, 1.0], [0.0, 1.0]]));
    assert!(call(s, "outline", json!({ "state": "shapes", "node": "nobody" })).is_null());

    let framing = call(s, "framing", json!({ "state": "images", "node": "image-focal" }));
    assert_eq!((&framing["crop"], &framing["fit"]), (&json!([0.0, 0.0, 1.0, 1.0]), &json!("cover")), "{framing}");
    let point = call(s, "focalAt", json!({ "state": "images", "node": "image-focal", "x": 741, "y": 426 }));
    let near = |v: &Value, to: f64| v.as_f64().is_some_and(|v| (v - to).abs() < 1e-6);
    assert!(near(&point[0], 0.24) && near(&point[1], 0.5), "the point of the image under the press: {point}");
    assert!(call(s, "focalAt", json!({ "state": "images", "node": "image-focal", "x": 10, "y": 10 })).is_null());

    let grid = call(s, "grid", Value::Null);
    assert_eq!(grid["canvas"], json!([1920.0, 1080.0]));
    assert_eq!(grid["columns"].as_array().map(Vec::len), Some(12), "{grid}");

    // The part of the canvas the Mac's surface paints through (`scaena_surface_paint`), a part of
    // some size; none, the whole.
    assert_eq!(call(s, "setView", json!({ "view": [0, 0, 960, 540] })), Value::Null);
    assert_eq!(call(s, "setView", json!({})), Value::Null);
    let refused = answered(s, "setView", &json!({ "view": [0, 0, 0, 10] }));
    assert!(refused["error"]["message"].as_str().is_some_and(|m| m.contains("size")), "{refused}");

    // A format beside the canvas, the deck's own where none is named.
    let mut error = null_mut();
    let tall = unsafe { scaena_pixels_in(s, c("9:16").as_ptr(), c("shapes").as_ptr(), f64::INFINITY, 320, &mut error) };
    assert_eq!((tall.width, tall.height), (180, 320));
    assert_eq!(bytes_of(tall.bytes).map(|b| b.len()), Some(180 * 320 * 4));
    let own = unsafe { scaena_pixels_in(s, null(), c("shapes").as_ptr(), f64::INFINITY, 320, &mut error) };
    assert_eq!((own.width, own.height), (569, 320));
    unsafe { scaena_bytes_free(own.bytes) };
    let none = unsafe { scaena_pixels_in(s, c("4:3").as_ptr(), c("shapes").as_ptr(), f64::INFINITY, 320, &mut error) };
    assert!(none.bytes.data.is_null() && took(error).contains("not one of the deck's formats"));
    unsafe { scaena_session_free(s) };

    // Find and replace on B1: every match, each where its text lives, or one.
    let b = open(B1);
    let query = json!({ "find": "Scaena" });
    let found = call(b, "find", json!({ "query": query }));
    assert!(found.as_array().is_some_and(|f| f.iter().any(|t| t["node"] == "title")), "{found}");
    let one = call(b, "replacing", json!({ "query": query, "with": "Stage", "one": [0, 0] }));
    assert_eq!(one.as_array().map(Vec::len), Some(1), "{one}");
    let every = call(b, "replacing", json!({ "query": query, "with": "Stage" }));
    assert_eq!(tool(b, "deck_patch", json!({ "ops": every }))["edited"], true);
    assert_eq!(call(b, "find", json!({ "query": query })), json!([]));
    unsafe { scaena_session_free(b) };
}

/// Bytes the library returned, taken and freed: none where it returned none.
fn bytes_of(made: ScaenaBytes) -> Option<Vec<u8>> {
    if made.data.is_null() {
        return None;
    }
    let out = unsafe { std::slice::from_raw_parts(made.data, made.len) }.to_vec();
    unsafe { scaena_bytes_free(made) };
    Some(out)
}

#[test]
fn an_export_is_the_bytes_scaena_export_writes() {
    let s = open(B1);
    let mut error = null_mut();
    let pdf = bytes_of(unsafe { scaena_export(s, c("pdf").as_ptr(), null(), &mut error) }).expect("a PDF");
    assert!(pdf.starts_with(b"%PDF-"), "a PDF");
    // The CLI's, for the bundle on disk.
    let disk = scaena_store::Bundle::open(Path::new(B1)).unwrap();
    let (cli, _) = scaena_ops::export::pdf_document(&disk, None, &scaena_export::pdf::PdfSettings::default()).unwrap();
    assert!(pdf == cli, "the Mac's PDF is the CLI's, byte for byte");

    let png = bytes_of(unsafe {
        scaena_export(s, c("png").as_ptr(), c(r#"{"state":"cover","width":640}"#).as_ptr(), &mut error)
    })
    .expect("a PNG");
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']), "a PNG");
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 640, "640 pixels wide");

    for (format, args) in [("png", r#"{"state":"cover","width":0}"#), ("png", r#"{"width":10}"#), ("docx", "{}")] {
        let mut error = null_mut();
        let none = bytes_of(unsafe { scaena_export(s, c(format).as_ptr(), c(args).as_ptr(), &mut error) });
        assert!(none.is_none(), "{format} {args}");
        assert!(took(error).contains("message"), "{format} {args}: said why");
    }
    unsafe { scaena_session_free(s) };
}

/// How a state reads, node by node (PLAN 3.17): each node a reader hears, in turn, its role and its
/// words or what it shows, as the reading HTML reads it; decoration and shapes with no alt text
/// read nothing.
#[test]
fn a_state_reads_node_by_node_as_a_screen_reader_hears_it() {
    let b = open(B1);
    assert_eq!(
        call(b, "reads", json!({ "state": "cover" })),
        json!([
            { "node": "title", "role": "heading", "level": 1, "text": "Scaena" },
            { "node": "subtitle", "role": "heading", "level": 2, "text": "Why this exists, what we believe, and what we refuse to build." },
        ])
    );
    let refused = answered(b, "reads", &json!({ "state": "nowhere" }));
    assert!(refused.get("error").is_some(), "{refused}");
    unsafe { scaena_session_free(b) };

    let t = open(TORTURE);
    let shapes = call(t, "reads", json!({ "state": "shapes" }));
    assert_eq!(shapes.as_array().map(Vec::len), Some(1), "the shapes are decoration: {shapes}");
    let images = call(t, "reads", json!({ "state": "images" }));
    let focal = images.as_array().and_then(|i| i.iter().find(|p| p["node"] == "image-focal")).cloned();
    assert_eq!(focal.as_ref().map(|p| &p["role"]), Some(&json!("figure")), "{images}");
    assert!(focal.is_some_and(|p| p["text"].as_str().is_some_and(|t| t.starts_with("The test card, cover"))));
    let table = call(t, "reads", json!({ "state": "chart-kinds-2" }));
    assert!(table.as_array().is_some_and(|p| p.iter().any(|p| p["node"] == "k-table" && p["role"] == "table")));
    unsafe { scaena_session_free(t) };
}

/// The layouts a state may take (PLAN 2.92, 3.26), as the browser's inspector shows them: judged
/// a step at a time, best first, each painted small and taken once, and one chosen by its patch.
#[test]
fn a_states_layouts_are_judged_a_step_at_a_time_and_drawn_small() {
    let s = revenue();
    let count = call(s, "layoutsBegin", json!({ "state": "revenue" })).as_u64().unwrap_or(0);
    let mut steps = 1;
    while call(s, "layoutsStep", Value::Null) == json!(true) {
        steps += 1;
    }
    assert_eq!(steps, count, "each step judges one layout");
    let suggested = call(s, "layoutSuggestions", json!({ "height": 90 }));
    let list = suggested.as_array().cloned().unwrap_or_default();
    let judged: Vec<(&str, bool, u64)> = list
        .iter()
        .map(|x| (x["layout"].as_str().unwrap_or(""), x["current"] == true, x["errors"].as_u64().unwrap_or(0)))
        .collect();
    // As the session judges them on its own: the layout it takes now first, the others with errors.
    assert_eq!(judged.len(), 3, "{suggested}");
    assert_eq!(judged[0], ("figure", true, 0), "{suggested}");
    assert!(judged[1..].iter().all(|&(_, current, errors)| !current && errors > 0), "{suggested}");
    for (i, x) in list.iter().enumerate() {
        let mut error = null_mut();
        let picture = unsafe { scaena_layout_pixels(s, i, &mut error) };
        assert_eq!((picture.width, picture.height), (160, 90), "{}", x["layout"]);
        assert_eq!((x["width"].as_u64(), x["height"].as_u64()), (Some(160), Some(90)));
        assert_eq!(bytes_of(picture.bytes).map(|b| b.len()), Some(160 * 90 * 4));
    }
    // Each is taken once, and none is past the last.
    for i in [0, list.len()] {
        let mut error = null_mut();
        let none = unsafe { scaena_layout_pixels(s, i, &mut error) };
        assert!(none.bytes.data.is_null() && took(error).contains("taken once"));
    }
    // A step with no round begun says so.
    let refused = answered(s, "layoutsStep", &Value::Null);
    assert!(refused["error"]["message"].as_str().is_some_and(|m| m.contains("no layouts")), "{refused}");
    // Another chosen by its patch: the state takes it.
    let other = &list[1];
    assert_eq!(tool(s, "deck_patch", json!({ "ops": other["patch"] }))["edited"], true);
    let now = call(s, "stateChoices", json!({ "state": "revenue" }));
    let layout = now["fields"].as_array().and_then(|f| f.iter().find(|f| f["prop"] == "layout")).cloned();
    assert_eq!(layout.map(|f| f["value"].clone()), Some(other["layout"].clone()), "{now}");
    unsafe { scaena_session_free(s) };
}
