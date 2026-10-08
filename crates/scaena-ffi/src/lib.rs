//! # scaena-ffi
//!
//! The session the browser edits, for Swift (PLAN 3.1, ADR-0021): a C ABI over
//! `scaena-session`, its header made by cbindgen (`include/scaena.h`, held to the code by the
//! `header` test), which the Swift package `apps/mac/ScaenaKit` imports.
//!
//! - **Files in, as a page hands them over.** Swift reads a bundle and hands each file over by
//!   its path in it ([`scaena_files_add`], then [`scaena_open`]), or a `.scaena` zip whole
//!   ([`scaena_open_zip`]). Rust reads no filesystem: a sandboxed app keeps its own access.
//! - **JSON out, as `Player` gives a page.** [`scaena_call`] answers the session's calls by the
//!   names a page uses (`states`, `timeline`, `compile`, `lint`, …), and [`scaena_tool`] runs any
//!   of the MCP server's operations on the bundle, as the browser's editor and assistant do
//!   (`deck_patch`, `deck_lint`, `deck_inspect`, …; ADR-0011). Each returns `{"ok": value}` or
//!   `{"error": {"message", …}}`.
//! - **Bytes for frames.** [`scaena_frame`] gives a state's display list, postcard-encoded (SPEC
//!   §6), and [`scaena_pixels`] paints it with the CPU painter. A save gives the saved bundle's
//!   files ([`scaena_save`]), fonts subset and the history recorded in it, as `scaena save` does.
//!
//! **Memory.** A string or bytes this library returns are the caller's, to free with
//! [`scaena_string_free`] or [`scaena_bytes_free`]; a handle, with its own `_free`. Every pointer
//! the caller passes is borrowed for the call alone.
//!
//! **Failure.** A null where a value is needed, text that is not UTF-8, and arguments that are
//! not JSON are each an error the call returns. A panic is caught at the boundary and is an
//! error that says so: nothing unwinds into Swift.

use scaena_session::Session;
use scaena_session::assistant::{Caller, failure};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::{CStr, CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};

/// One bundle open for editing.
pub struct ScaenaSession(Session);

/// A bundle's files, gathered to open it.
pub struct ScaenaFiles(BTreeMap<String, Vec<u8>>);

/// A save: every file of the bundle as saved, for the caller to write where it keeps the
/// bundle, then to [adopt](scaena_adopt).
pub struct ScaenaSaved(scaena_store::Saving);

/// Bytes this library made, the caller's to free with [`scaena_bytes_free`]. `data` is null
/// where there are none, as where a call failed.
#[repr(C)]
pub struct ScaenaBytes {
    pub data: *mut u8,
    pub len: usize,
}

/// A frame painted: straight-alpha sRGB, four bytes a pixel, row by row, `width` × `height`.
#[repr(C)]
pub struct ScaenaPixels {
    pub bytes: ScaenaBytes,
    pub width: u32,
    pub height: u32,
}

impl ScaenaBytes {
    const NONE: ScaenaBytes = ScaenaBytes { data: std::ptr::null_mut(), len: 0 };

    fn of(bytes: Vec<u8>) -> ScaenaBytes {
        let len = bytes.len();
        let data = Box::into_raw(bytes.into_boxed_slice()).cast::<u8>();
        ScaenaBytes { data, len }
    }
}

/// Why a call stopped, as the envelope carries it.
type Failure = Value;

fn said(message: impl std::fmt::Display) -> Failure {
    json!({ "message": message.to_string() })
}

/// `f`'s result, with a panic caught and said.
fn guarded<T>(f: impl FnOnce() -> Result<T, Failure>) -> Result<T, Failure> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(panic) => {
            let why = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "a panic".to_string());
            Err(said(format!("the engine stopped: {why}")))
        }
    }
}

/// `value` as a C string the caller frees. JSON holds no NUL.
fn c_string(value: String) -> *mut c_char {
    CString::new(value).map_or(std::ptr::null_mut(), CString::into_raw)
}

/// `{"ok": value}` or `{"error": failure}`, as a C string.
fn envelope(result: Result<Value, Failure>) -> *mut c_char {
    c_string(match result {
        Ok(value) => json!({ "ok": value }).to_string(),
        Err(failure) => json!({ "error": failure }).to_string(),
    })
}

/// Set `*error`, where the caller gave one, to `failure` as JSON.
unsafe fn report(error: *mut *mut c_char, failure: Failure) {
    if !error.is_null() {
        // SAFETY: the caller gives a pointer it can write a pointer through, or null.
        unsafe { *error = c_string(failure.to_string()) };
    }
}

/// The text at `p`, which must be there.
unsafe fn text<'a>(p: *const c_char, what: &str) -> Result<&'a str, Failure> {
    if p.is_null() {
        return Err(said(format!("{what} is null")));
    }
    // SAFETY: the caller gives a NUL-terminated string that lives through the call.
    unsafe { CStr::from_ptr(p) }.to_str().map_err(|_| said(format!("{what} is not UTF-8")))
}

/// The text at `p`, if there is any.
unsafe fn maybe_text<'a>(p: *const c_char, what: &str) -> Result<Option<&'a str>, Failure> {
    if p.is_null() { Ok(None) } else { unsafe { text(p, what) }.map(Some) }
}

/// The `len` bytes at `p`; none where `len` is 0.
unsafe fn bytes<'a>(p: *const u8, len: usize, what: &str) -> Result<&'a [u8], Failure> {
    match (p.is_null(), len) {
        (_, 0) => Ok(&[]),
        (true, _) => Err(said(format!("{what} is null"))),
        // SAFETY: the caller gives `len` readable bytes that live through the call.
        (false, _) => Ok(unsafe { std::slice::from_raw_parts(p, len) }),
    }
}

/// The handle at `p`, which must be one this library made and has not freed.
unsafe fn handle<'a, T>(p: *mut T, what: &str) -> Result<&'a mut T, Failure> {
    // SAFETY: the caller gives a live handle, used by one thread at a time, or null.
    unsafe { p.as_mut() }.ok_or_else(|| said(format!("{what} is null")))
}

/// Begin gathering a bundle's files. Free with [`scaena_files_free`], or hand to [`scaena_open`],
/// which takes them.
#[unsafe(no_mangle)]
pub extern "C" fn scaena_files_new() -> *mut ScaenaFiles {
    Box::into_raw(Box::new(ScaenaFiles(BTreeMap::new())))
}

/// Add the file at `path` in the bundle (`deck.json`, `fonts/Inter-VF.ttf`, …), its `len`
/// bytes copied. False where `files` or `path` is null or `path` is not UTF-8.
///
/// # Safety
/// `files` is a live handle from [`scaena_files_new`]; `path` a NUL-terminated string; `bytes`
/// `len` readable bytes, or null where `len` is 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_files_add(
    files: *mut ScaenaFiles,
    path: *const c_char,
    bytes: *const u8,
    len: usize,
) -> bool {
    guarded(|| {
        let files = unsafe { handle(files, "files") }?;
        let path = unsafe { text(path, "path") }?;
        files.0.insert(path.to_string(), unsafe { self::bytes(bytes, len, "bytes") }?.to_vec());
        Ok(())
    })
    .is_ok()
}

/// Let go of files not opened.
///
/// # Safety
/// `files` is a live handle from [`scaena_files_new`], or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_files_free(files: *mut ScaenaFiles) {
    if !files.is_null() {
        // SAFETY: the caller gives a handle this library made and has not freed.
        drop(unsafe { Box::from_raw(files) });
    }
}

/// Open the bundle `files` hold, as a page opens one from a folder: its `deck.json`, the theme
/// it names, and every other file by its path. Takes `files`, opened or not. Null where the
/// bundle cannot be opened, `*error` then saying why (JSON `{"message"}`; free it).
///
/// # Safety
/// `files` is a live handle from [`scaena_files_new`]; `error` is null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_open(files: *mut ScaenaFiles, error: *mut *mut c_char) -> *mut ScaenaSession {
    let opened = guarded(|| {
        if files.is_null() {
            return Err(said("files is null"));
        }
        // SAFETY: the caller gives a handle this library made, which it takes.
        let files = unsafe { Box::from_raw(files) };
        Session::open(files.0).map_err(said)
    });
    match opened {
        Ok(session) => Box::into_raw(Box::new(ScaenaSession(session))),
        Err(failure) => {
            unsafe { report(error, failure) };
            std::ptr::null_mut()
        }
    }
}

/// Open a `.scaena` zip, its `len` bytes. Null where it cannot be opened, `*error` then saying
/// why.
///
/// # Safety
/// `bytes` is `len` readable bytes; `error` is null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_open_zip(bytes: *const u8, len: usize, error: *mut *mut c_char) -> *mut ScaenaSession {
    match guarded(|| Session::from_zip(unsafe { self::bytes(bytes, len, "bytes") }?).map_err(said)) {
        Ok(session) => Box::into_raw(Box::new(ScaenaSession(session))),
        Err(failure) => {
            unsafe { report(error, failure) };
            std::ptr::null_mut()
        }
    }
}

/// Close a session.
///
/// # Safety
/// `session` is a live handle from [`scaena_open`] or [`scaena_open_zip`], or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_session_free(session: *mut ScaenaSession) {
    if !session.is_null() {
        // SAFETY: the caller gives a handle this library made and has not freed.
        drop(unsafe { Box::from_raw(session) });
    }
}

/// Hand over a file at any time, by its path in the bundle: an image dropped, a font, data. One
/// the deck draws with builds the engine again on the next frame. False where an argument is
/// null or `path` is not UTF-8.
///
/// # Safety
/// `session` is a live handle; `path` a NUL-terminated string; `bytes` `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_add_file(
    session: *mut ScaenaSession,
    path: *const c_char,
    bytes: *const u8,
    len: usize,
) -> bool {
    guarded(|| {
        let session = unsafe { handle(session, "session") }?;
        let path = unsafe { text(path, "path") }?;
        session.0.add_file(path, unsafe { self::bytes(bytes, len, "bytes") }?.to_vec());
        Ok(())
    })
    .is_ok()
}

/// Answer `method` with `args` (a JSON object, or null for none), as a page's `Player` does:
/// `{"ok": value}` or `{"error": {"message"}}`, a string to free. The calls ([`call`]):
/// `states`, `formats`, `setFormat {format?}`, `canvasSize`, `duration {state}`, `timeline`,
/// `files`, `imageFiles`, `digest {state}`, `reading {state}`, `source`, `compiledFrom
/// {source}`, `compile {source}`, `lint {state?}`, `fix {patch}`, `inspect {state}`, `layers
/// {state}`, `choices {state, node}`, `stateChoices {state}`, `inserts`, `themes`,
/// `themeText`, `keepHistory`, `keepsHistory`.
///
/// # Safety
/// `session` is a live handle; `method` a NUL-terminated string; `args` one, or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_call(
    session: *mut ScaenaSession,
    method: *const c_char,
    args: *const c_char,
) -> *mut c_char {
    envelope(guarded(|| {
        let session = unsafe { handle(session, "session") }?;
        let method = unsafe { text(method, "method") }?;
        let args = match unsafe { maybe_text(args, "args") }? {
            None => json!({}),
            Some(a) => {
                serde_json::from_str(a).map_err(|e| said(format!("{method}: the arguments are not JSON: {e}")))?
            }
        };
        call(&mut session.0, method, &args)
    }))
}

/// Run the MCP server's operation `name` on the bundle with `args` (its tool's arguments, less
/// `bundle`, `out`, and `painter`), as `author` (null: `agent`) at `at` (RFC 3339, or null):
/// `deck_patch`, `deck_lint`, `deck_inspect`, `deck_find`, `theme_edit`, `data_edit`, and the
/// rest (SPEC §7.2). `{"ok": result, "edited": bool}`, `edited` where it changed the deck, or
/// `{"error": {"message", "plan"?, "op"?}}`, as the tool says it.
///
/// # Safety
/// `session` is a live handle; `name` a NUL-terminated string; `args`, `author`, and `at` one
/// each, or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_tool(
    session: *mut ScaenaSession,
    name: *const c_char,
    args: *const c_char,
    author: *const c_char,
    at: *const c_char,
) -> *mut c_char {
    let called = guarded(|| {
        let session = unsafe { handle(session, "session") }?;
        let name = unsafe { text(name, "name") }?;
        let args = match unsafe { maybe_text(args, "args") }? {
            None => Value::Null,
            Some(a) => serde_json::from_str(a).map_err(|e| said(format!("{name}: the arguments are not JSON: {e}")))?,
        };
        let author = unsafe { maybe_text(author, "author") }?.unwrap_or("agent");
        let at = unsafe { maybe_text(at, "at") }?.and_then(scaena_session::store::seconds);
        match session.0.tool(name, args, Caller { author, at }) {
            Ok(called) => {
                let result: Value = serde_json::from_str(&called.result).map_err(said)?;
                Ok(json!({ "ok": result, "edited": called.edited }))
            }
            Err(e) => Ok(json!({ "error": failure(&e) })),
        }
    });
    c_string(match called {
        Ok(value) => value.to_string(),
        Err(failure) => json!({ "error": failure }).to_string(),
    })
}

/// `state`'s display list at `t_ms` (infinity: at rest), postcard-encoded (SPEC §6): the bytes
/// the browser's module gives for it. Null bytes where it cannot be made, `*error` then saying
/// why.
///
/// # Safety
/// `session` is a live handle; `state` a NUL-terminated string; `error` null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_frame(
    session: *mut ScaenaSession,
    state: *const c_char,
    t_ms: f64,
    error: *mut *mut c_char,
) -> ScaenaBytes {
    let made = guarded(|| {
        let session = unsafe { handle(session, "session") }?;
        let state = unsafe { text(state, "state") }?;
        session.0.frame(state, t_ms).map_err(said)?.to_postcard().map_err(said)
    });
    match made {
        Ok(bytes) => ScaenaBytes::of(bytes),
        Err(failure) => {
            unsafe { report(error, failure) };
            ScaenaBytes::NONE
        }
    }
}

/// `state` at `t_ms` (infinity: at rest), painted by the CPU painter `width` pixels wide, the
/// height keeping the canvas's aspect: the pixels the goldens hold (SPEC §13.5). Null bytes
/// where it cannot be painted, `*error` then saying why.
///
/// # Safety
/// `session` is a live handle; `state` a NUL-terminated string; `error` null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_pixels(
    session: *mut ScaenaSession,
    state: *const c_char,
    t_ms: f64,
    width: u32,
    error: *mut *mut c_char,
) -> ScaenaPixels {
    let painted = guarded(|| {
        let session = unsafe { handle(session, "session") }?;
        let state = unsafe { text(state, "state") }?;
        session.0.pixels(state, t_ms, width).map_err(said)
    });
    match painted {
        Ok(raster) => ScaenaPixels { bytes: ScaenaBytes::of(raster.rgba), width: raster.width, height: raster.height },
        Err(failure) => {
            unsafe { report(error, failure) };
            ScaenaPixels { bytes: ScaenaBytes::NONE, width: 0, height: 0 }
        }
    }
}

/// Save the bundle with the deck shown at `now` (RFC 3339; the engine reads no clock), as
/// `scaena save` lays one out: files named by their content, fonts subset to what the deck can
/// draw if `subset`, and every edit since the last save recorded in its history, where it keeps
/// one (or [`keepHistory`](call) began one). The session keeps the bundle as it was until it
/// [adopts](scaena_adopt) the save. Null where it cannot be saved, `*error` then saying why.
///
/// # Safety
/// `session` is a live handle; `now` a NUL-terminated string; `error` null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_save(
    session: *mut ScaenaSession,
    now: *const c_char,
    subset: bool,
    error: *mut *mut c_char,
) -> *mut ScaenaSaved {
    let saved = guarded(|| {
        let session = unsafe { handle(session, "session") }?;
        let now = unsafe { text(now, "now") }?;
        if subset {
            // What the browser hands its subsetter's module, done here: the same bytes.
            let (chars, fonts) = session.0.subsetting().map_err(said)?;
            let wanted = chars.chars().collect();
            for font in fonts {
                let bytes = session.0.file(&font).ok_or_else(|| said(format!("{font} is not in the bundle")))?;
                let small = scaena_store::subset::subset(bytes, &wanted).map_err(said)?;
                session.0.add_subset(&font, &chars, small);
            }
        }
        let record = |held: &[u8], changes: &str| scaena_store::crdt::recorded(held, changes);
        session.0.save(now, subset, Some(&record as &scaena_session::store::Recorder)).map_err(said)
    });
    match saved {
        Ok(saving) => Box::into_raw(Box::new(ScaenaSaved(saving))),
        Err(failure) => {
            unsafe { report(error, failure) };
            std::ptr::null_mut()
        }
    }
}

/// The saved bundle's files and what the save did, as JSON: `{"files": [path], "replaced":
/// [path], "summary": …}`. `replaced` are the files of the bundle as it was that the save
/// renamed or rewrote: one saved in place drops those `files` does not hold.
///
/// # Safety
/// `saved` is a live handle from [`scaena_save`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_saved_list(saved: *const ScaenaSaved) -> *mut c_char {
    envelope(guarded(|| {
        // SAFETY: the caller gives a live handle, or null.
        let saved = unsafe { saved.as_ref() }.ok_or_else(|| said("saved is null"))?;
        Ok(json!({
            "files": saved.0.files.keys().collect::<Vec<_>>(),
            "replaced": saved.0.replaced,
            "summary": saved.0.saved,
        }))
    }))
}

/// The bytes of the saved file at `path`, to write where the bundle is kept; null bytes where
/// the save holds no such file.
///
/// # Safety
/// `saved` is a live handle from [`scaena_save`]; `path` a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_saved_file(saved: *const ScaenaSaved, path: *const c_char) -> ScaenaBytes {
    guarded(|| {
        // SAFETY: the caller gives a live handle, or null.
        let saved = unsafe { saved.as_ref() }.ok_or_else(|| said("saved is null"))?;
        let path = unsafe { text(path, "path") }?;
        Ok(saved.0.files.get(path).cloned())
    })
    .ok()
    .flatten()
    .map_or(ScaenaBytes::NONE, ScaenaBytes::of)
}

/// The saved bundle as one `.scaena` zip.
///
/// # Safety
/// `saved` is a live handle from [`scaena_save`]; `error` null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_saved_zip(saved: *const ScaenaSaved, error: *mut *mut c_char) -> ScaenaBytes {
    let zipped = guarded(|| {
        // SAFETY: the caller gives a live handle, or null.
        let saved = unsafe { saved.as_ref() }.ok_or_else(|| said("saved is null"))?;
        scaena_store::zip(&saved.0.files).map_err(said)
    });
    match zipped {
        Ok(bytes) => ScaenaBytes::of(bytes),
        Err(failure) => {
            unsafe { report(error, failure) };
            ScaenaBytes::NONE
        }
    }
}

/// Go on from `saved`, once the caller has written it where it keeps the bundle: its files are
/// the session's from now on, and its deck, which names files by their content, is shown; the
/// source is the saved deck's. `{"ok": null}` or `{"error"}`.
///
/// # Safety
/// `session` is a live handle; `saved` one from [`scaena_save`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_adopt(session: *mut ScaenaSession, saved: *const ScaenaSaved) -> *mut c_char {
    envelope(guarded(|| {
        let session = unsafe { handle(session, "session") }?;
        // SAFETY: the caller gives a live handle, or null.
        let saved = unsafe { saved.as_ref() }.ok_or_else(|| said("saved is null"))?;
        session.0.adopt(&saved.0).map_err(said)?;
        Ok(Value::Null)
    }))
}

/// Let go of a save.
///
/// # Safety
/// `saved` is a live handle from [`scaena_save`], or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_saved_free(saved: *mut ScaenaSaved) {
    if !saved.is_null() {
        // SAFETY: the caller gives a handle this library made and has not freed.
        drop(unsafe { Box::from_raw(saved) });
    }
}

/// Free a string this library returned.
///
/// # Safety
/// `s` is a string this library returned and has not been freed, or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_string_free(s: *mut c_char) {
    if !s.is_null() {
        // SAFETY: the caller gives a string this library made with `CString::into_raw`.
        drop(unsafe { CString::from_raw(s) });
    }
}

/// Free bytes this library returned.
///
/// # Safety
/// `bytes` were returned by this library and have not been freed; null `data` is nothing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_bytes_free(bytes: ScaenaBytes) {
    if !bytes.data.is_null() {
        // SAFETY: `data` and `len` are a boxed slice this library made.
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(bytes.data, bytes.len)) });
    }
}

/// The session's answer to `method`, as `Player` gives it a page.
fn call(s: &mut Session, method: &str, args: &Value) -> Result<Value, Failure> {
    let arg = |key: &str| {
        args.get(key).and_then(Value::as_str).ok_or_else(|| said(format!("{method}: `{key}` is a string it needs")))
    };
    let optional = |key: &str| args.get(key).and_then(Value::as_str);
    let value = |v: Result<Value, serde_json::Error>| v.map_err(said);
    Ok(match method {
        "states" => json!(s.states()),
        "formats" => json!(s.formats()),
        "setFormat" => {
            s.set_format(optional("format")).map_err(said)?;
            Value::Null
        }
        "canvasSize" => json!(s.canvas_size().map_err(said)?),
        "duration" => json!(s.duration(arg("state")?).map_err(said)?),
        "timeline" => s.slots().map_err(said)?,
        "files" => json!(s.files()),
        "imageFiles" => json!(s.image_files()),
        "digest" => json!(s.frame(arg("state")?, f64::INFINITY).map_err(said)?.digest().map_err(said)?),
        "reading" => json!(s.reading(arg("state")?).map_err(said)?),
        "source" => json!(s.source()),
        "compiledFrom" => json!(s.compiled_from(arg("source")?)),
        "compile" => value(serde_json::to_value(s.compile(arg("source")?)))?,
        "lint" => value(serde_json::to_value(s.lint(optional("state")).map_err(said)?))?,
        "fix" => {
            let patch: Vec<Value> = args
                .get("patch")
                .and_then(Value::as_array)
                .cloned()
                .ok_or_else(|| said(format!("{method}: `patch` is a list it needs")))?;
            json!(s.fix(&patch).map_err(said)?)
        }
        "inspect" => value(serde_json::to_value(s.inspect(arg("state")?).map_err(said)?))?,
        "layers" => value(serde_json::to_value(s.layers(arg("state")?).map_err(said)?))?,
        "choices" => value(serde_json::to_value(s.choices(arg("state")?, arg("node")?).map_err(said)?))?,
        "stateChoices" => value(serde_json::to_value(s.state_choices(arg("state")?).map_err(said)?))?,
        "inserts" => value(serde_json::to_value(s.inserts()))?,
        "themes" => {
            let (current, files) = s.themes();
            json!({ "current": current, "files": files })
        }
        "themeText" => s.theme_text().unwrap_or(Value::Null),
        "keepHistory" => {
            s.keep_history();
            Value::Null
        }
        "keepsHistory" => json!(s.keeps_history()),
        _ => return Err(said(format!("`{method}` is not a call this library answers"))),
    })
}
