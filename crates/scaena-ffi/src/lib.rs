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
//! - **Exports** (PLAN 3.8). [`scaena_export`] makes the PDF and PNGs `scaena export` writes,
//!   which the app shares and previews.
//! - **The assistant's conversation** (PLAN 3.6, ADR-0022). [`scaena_chat_new`] begins one with
//!   the model the user chose, and [`scaena_chat_call`] takes it a step at a time: the question,
//!   each request for Swift to make with the user's key, the answer read, and each call the model
//!   makes run on the session. [`scaena_providers`] lists the models a key can use.
//!
//! **Memory.** A string or bytes this library returns are the caller's, to free with
//! [`scaena_string_free`] or [`scaena_bytes_free`]; a handle, with its own `_free`. Every pointer
//! the caller passes is borrowed for the call alone.
//!
//! **Failure.** A null where a value is needed, text that is not UTF-8, and arguments that are
//! not JSON are each an error the call returns. A panic is caught at the boundary and is an
//! error that says so: nothing unwinds into Swift.

mod chat;

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

/// A new bundle (PLAN 2.12, 3.3), as New makes one in the browser and `deck_create` does: the
/// theme that ships as `theme` (`dusk`, `daybreak`, or `ember`), the fonts it names, and one
/// state with nothing on it, titled `title`. Kept nowhere until it is saved. Null where no such
/// theme ships, `*error` then saying why.
///
/// # Safety
/// `theme` and `title` are NUL-terminated strings; `error` is null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_create(
    theme: *const c_char,
    title: *const c_char,
    error: *mut *mut c_char,
) -> *mut ScaenaSession {
    let made = guarded(|| {
        let name = unsafe { text(theme, "theme") }?;
        let title = unsafe { text(title, "title") }?;
        let shipped = scaena_ops::shipped::theme(name)
            .ok_or_else(|| said(format!("`{name}` is not a theme that ships: {}", scaena_ops::shipped::names())))?;
        let mut fonts = BTreeMap::new();
        for (path, bytes) in scaena_ops::shipped::fonts() {
            fonts.insert(path.to_string(), bytes.to_vec());
        }
        Session::create(shipped.file, shipped.text, &fonts, title).map_err(said)
    });
    match made {
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

/// A file dropped or pasted on the canvas (PLAN 2.45, 2.76, 2.96, 3.12), handed over where the
/// browser's canvas keeps one: a data file (`.csv`, `.json`) under `data/` by its name, numbered
/// where the bundle holds other bytes there; anything else, an image above all, under `assets/`,
/// named by its SHA-256 as a save names it. `{"ok": path}` or `{"error": {"message"}}`, a
/// string to free.
///
/// # Safety
/// `session` is a live handle; `name` a NUL-terminated string; `bytes` `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_drop(
    session: *mut ScaenaSession,
    name: *const c_char,
    bytes: *const u8,
    len: usize,
) -> *mut c_char {
    envelope(guarded(|| {
        let session = unsafe { handle(session, "session") }?;
        let name = unsafe { text(name, "name") }?;
        let bytes = unsafe { self::bytes(bytes, len, "bytes") }?.to_vec();
        let ext = name.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase());
        let path = match ext.as_deref() {
            Some("csv" | "json") => session.0.placing(name, &bytes),
            _ => scaena_store::place(name, &bytes),
        };
        session.0.add_file(&path, bytes);
        Ok(json!(path))
    }))
}

/// Answer `method` with `args` (a JSON object, or null for none), as a page's `Player` does:
/// `{"ok": value}` or `{"error": {"message"}}`, a string to free. The calls ([`call`]):
/// `states`, `formats`, `setFormat {format?}`, `canvasSize`, `duration {state}`, `timeline`,
/// `files`, `imageFiles`, `digest {state}`, `reading {state}`, `reads {state}` (each node read,
/// in turn: its role and its words, PLAN 3.17), `source`, `compiledFrom
/// {source}`, `compile {source}`, `lint {state?}`, `fix {patch}`, `inspect {state}`, `boxes
/// {state}`, `hit {state, x, y}`, `layers {state}`, `carets {state, node}`, `choices {state,
/// node}`, `stateChoices {state}`, `inserts`, `themes`, `themeText`, `keepHistory`,
/// `keepsHistory`, and `writeFiles {files: [{path, text}]}`, which writes files back as an undo
/// has them (`text` null: taken out). A drag (PLAN 3.7, ADR-0013): `targets {state, node}`,
/// where a node may go; `snap {state, node, how, x, y, w, h, fork?, reach?}`, where a box left
/// there lands and the patch that puts it there; `setMoving {nodes, dx, dy}`, the nodes drawn
/// moved in frames at rest, laying nothing out; `preview {ops?}`, frames at rest drawn as a
/// patch would make them; and `reach {ops}`, the states a patch changes. Text typed in place
/// (PLAN 3.9): `typed {ops, at?}`, `replace_text`, `style_text`, and `list` ops by the user,
/// validated but not linted, and whether the deck changed; and `bolding` and `italicizing
/// {state, node, from, to}`, the look ⌘B and ⌘I give characters. A text's characters (PLAN
/// 3.10): `characterChoices {state, node, from, to}`, what an inspector offers for them; and
/// `linkAt {state, x, y}`, the link drawn there at rest, where a click goes. Nodes added and
/// taken away (PLAN 3.11): `inserting {state, n, x, y, named?, with?}`, the patch that inserts
/// what `inserts` offers `n`th about a point, or in the room nearest it, `with` properties of its
/// own set on it; `duplicating {state, node}`,
/// a copy beside it; each `{id, cell, patch}`; and `deleting {state, node, everywhere?}`, the
/// ops that take it out of the state and those after, or out of the deck. The clipboard (PLAN
/// 3.12): `copying {state, nodes}`, the clip as text; `pasting {text, state, x, y}`, the patch that
/// pastes a clip, or other text as a text, about a point, the files it carries handed over;
/// `look {state, node}` and `putting {state, look, nodes}`, a look copied and the patch that
/// puts it on others; `cells {text}`, a sheet's cells as the source they would be, or null; and
/// `attaching {path, schema?}`, a data file the bundle holds as the source a chart of it reads.
/// Several selected (PLAN 3.13): `together {state, nodes, dx, dy, free?, fork?, reach?}`,
/// children of one container moved together as a drag moves the first, with the guides they
/// meet; `arranging {state, nodes, how, fork?}`, them aligned, spread, or ordered (`how`: one of
/// `align`, `spread`, `order`, `before`, `after`, `into`); and `grouping {state, nodes}`, the
/// patch that puts them in a new group, `{id, patch}`. States (PLAN 3.14): `addingState {state,
/// what}`, the patch that adds a state after it, a `step` of its slide or a `slide` of its own,
/// `{id, patch}`. The panels (PLAN 3.15), each edit by the user at `at` (RFC 3339) where given:
/// `shippedThemes`; `retheme {path | ships, at?}`, the deck in a theme the bundle holds or one
/// that ships, as `theme --apply` says it; `themeEdit {ops | photo, dryRun?, at?}`, `{edited,
/// files}`, the theme file written before and after for the undo; `bundleFiles` and `removeFile
/// {path}`; `dataSources`, `dataSheet {name}`, `dataEdit {source, edits, at?}`, `{result,
/// wrote}`, and `dataUndo {redo?, at?}`, the file an edit wrote or a removal took out put back or
/// written again; and, from the bundle's history, `versions`, `viewVersion {version}`, its
/// states, `compareVersions {from, to?}`, and `restoreVersion {version, at?}`, `{restored,
/// files}`. Handles and views (PLAN 3.16): `outline {state, node}`, a shape's points and corner;
/// `framing {state, node}`, an image's crop and focal point; `focalAt {state, node, x, y}`, the
/// point of an image under a press; `grid`, the theme's tracks and baselines; `setView {view?}`,
/// the part of the canvas frames are painted through; and `find {query}` and `replacing {query,
/// with, one?}`, the deck's texts found and the patch that replaces them. The layouts a state may
/// take (PLAN 2.92, 3.26): `layoutsBegin {state}`, how many are to be judged; `layoutsStep`, the
/// next judged, and whether any is left; and `layoutSuggestions {height}`, those judged, best
/// first, each with the `set_state` that gives it and its picture's size, painted `height`
/// pixels high, which [`scaena_layout_pixels`] takes.
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

/// `state` at `t_ms` (infinity: at rest) in `format`, one of the deck's formats, or its own canvas
/// where null, painted by the CPU painter `height` pixels high, the width keeping the format's
/// aspect: what the editor paints beside the canvas, a format at a time (PLAN 2.62, 3.16). Each
/// format lays each state out once. Null bytes where it cannot be painted, `*error` then saying why.
///
/// # Safety
/// `session` is a live handle; `format` null or a NUL-terminated string; `state` one; `error`
/// null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_pixels_in(
    session: *mut ScaenaSession,
    format: *const c_char,
    state: *const c_char,
    t_ms: f64,
    height: u32,
    error: *mut *mut c_char,
) -> ScaenaPixels {
    let painted = guarded(|| {
        let session = unsafe { handle(session, "session") }?;
        let format = unsafe { maybe_text(format, "format") }?;
        let state = unsafe { text(state, "state") }?;
        session.0.pixels_in(format, state, t_ms, height).map_err(said)
    });
    match painted {
        Ok(raster) => ScaenaPixels { bytes: ScaenaBytes::of(raster.rgba), width: raster.width, height: raster.height },
        Err(failure) => {
            unsafe { report(error, failure) };
            ScaenaPixels { bytes: ScaenaBytes::NONE, width: 0, height: 0 }
        }
    }
}

/// The `i`th picture `layoutSuggestions` painted last (PLAN 2.92, 3.26), taken: the state laid out
/// in that layout, at rest, as [`scaena_pixels`] gives a frame. Null bytes for one taken already,
/// or past the last, `*error` then saying why.
///
/// # Safety
/// `session` is a live handle; `error` null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_layout_pixels(
    session: *mut ScaenaSession,
    i: usize,
    error: *mut *mut c_char,
) -> ScaenaPixels {
    let painted = guarded(|| {
        let session = unsafe { handle(session, "session") }?;
        session.0.layout_picture(i).ok_or_else(|| {
            said(format!("no layout's picture {i} is kept: `layoutSuggestions` paints them, and each is taken once"))
        })
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

/// Frames painted onto a `CAMetalLayer` by vello on Metal (PLAN 3.2): what the Mac's view
/// shows. Made by [`scaena_surface_new`], on the Mac alone.
pub struct ScaenaSurface {
    #[cfg(target_vendor = "apple")]
    painter: scaena_paint::gpu::LayerPainter,
}

/// Make the GPU every surface paints with, if it is not made yet: what the app calls as it starts,
/// off its main thread, so that the first deck it opens shows its first frame without waiting for
/// it (gate 3). A surface made meanwhile waits for it. False where none can be made, as on any
/// machine but a Mac, `*error` then saying why.
///
/// # Safety
/// `error` is null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_gpu_warm(error: *mut *mut c_char) -> bool {
    let warmed = guarded(|| {
        #[cfg(target_vendor = "apple")]
        return scaena_paint::gpu::warm().map_err(said);
        #[cfg(not(target_vendor = "apple"))]
        Err(said("a layer is painted on the Mac, by Metal: there is none here"))
    });
    match warmed {
        Ok(()) => true,
        Err(failure) => {
            unsafe { report(error, failure) };
            false
        }
    }
}

/// Paint on `layer`, a `CAMetalLayer`, `width` × `height` device pixels, on the GPU every surface
/// shares, made first if [`scaena_gpu_warm`] has not made it; frames are presented at the
/// display's refresh. The layer keeps the deck's aspect, and outlives the surface. Null where
/// none can be made, as on any machine but a Mac, `*error` then saying why.
///
/// # Safety
/// `layer` is a live `CAMetalLayer` that outlives the surface; `error` is null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_surface_new(
    layer: *mut std::ffi::c_void,
    width: u32,
    height: u32,
    error: *mut *mut c_char,
) -> *mut ScaenaSurface {
    let made = guarded(|| {
        if layer.is_null() {
            return Err(said("layer is null"));
        }
        #[cfg(target_vendor = "apple")]
        {
            // SAFETY: the caller gives a live CAMetalLayer that outlives the surface.
            let painter = unsafe { scaena_paint::gpu::LayerPainter::new(layer, width, height) }.map_err(said)?;
            Ok(ScaenaSurface { painter })
        }
        #[cfg(not(target_vendor = "apple"))]
        {
            let _ = (width, height);
            Err(said("a layer is painted on the Mac, by Metal: there is none here"))
        }
    });
    match made {
        Ok(surface) => Box::into_raw(Box::new(surface)),
        Err(failure) => {
            unsafe { report(error, failure) };
            std::ptr::null_mut()
        }
    }
}

/// Paint at `width` × `height` device pixels from now on: the view's size, or its screen's
/// scale, changed.
///
/// # Safety
/// `surface` is a live handle from [`scaena_surface_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_surface_resize(surface: *mut ScaenaSurface, width: u32, height: u32) -> bool {
    guarded(|| {
        let surface = unsafe { handle(surface, "surface") }?;
        #[cfg(target_vendor = "apple")]
        surface.painter.resize(width, height);
        #[cfg(not(target_vendor = "apple"))]
        let _ = (surface, width, height);
        Ok(())
    })
    .is_ok()
}

/// Paint `state` at `t_ms` into its cue (infinity: at rest) from `session` onto the layer, as
/// the canvas shows it (zoomed, where it is), and present it at the next refresh: 1. 0 where the
/// layer had no drawable to give, hidden or resized meanwhile: the frame is skipped. -1 where
/// it could not be painted, `*error` then saying why.
///
/// # Safety
/// `surface` and `session` are live handles; `state` a NUL-terminated string; `error` null or
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_surface_paint(
    surface: *mut ScaenaSurface,
    session: *mut ScaenaSession,
    state: *const c_char,
    t_ms: f64,
    error: *mut *mut c_char,
) -> i32 {
    let painted = guarded(|| {
        let surface = unsafe { handle(surface, "surface") }?;
        let session = unsafe { handle(session, "session") }?;
        let state = unsafe { text(state, "state") }?;
        let dl = session.0.viewed(state, t_ms).map_err(said)?;
        #[cfg(target_vendor = "apple")]
        return surface.painter.paint(&dl, session.0.assets()).map_err(said);
        #[cfg(not(target_vendor = "apple"))]
        {
            let _ = (surface, dl);
            Err(said("a layer is painted on the Mac, by Metal: there is none here"))
        }
    });
    match painted {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(failure) => {
            unsafe { report(error, failure) };
            -1
        }
    }
}

/// The last frame the surface painted, read back as the layer was given it: what a test holds
/// to [`scaena_pixels`] (SPEC §13.5). Null bytes before any frame, `*error` then saying why.
///
/// # Safety
/// `surface` is a live handle; `error` null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_surface_pixels(surface: *mut ScaenaSurface, error: *mut *mut c_char) -> ScaenaPixels {
    let read = guarded(|| {
        let surface = unsafe { handle(surface, "surface") }?;
        #[cfg(target_vendor = "apple")]
        return surface.painter.last().map_err(said);
        #[cfg(not(target_vendor = "apple"))]
        {
            let _ = surface;
            Err::<scaena_paint::Raster, _>(said("a layer is painted on the Mac, by Metal: there is none here"))
        }
    });
    match read {
        Ok(raster) => ScaenaPixels { bytes: ScaenaBytes::of(raster.rgba), width: raster.width, height: raster.height },
        Err(failure) => {
            unsafe { report(error, failure) };
            ScaenaPixels { bytes: ScaenaBytes::NONE, width: 0, height: 0 }
        }
    }
}

/// The adapter that paints the surface, and what paints on it (`vello`, or `cpu` where the GPU runs no
/// vello), as JSON: `{"ok": {"name", "backend", "device", "painter"}}`.
///
/// # Safety
/// `surface` is a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_surface_adapter(surface: *mut ScaenaSurface) -> *mut c_char {
    envelope(guarded(|| {
        let surface = unsafe { handle(surface, "surface") }?;
        #[cfg(target_vendor = "apple")]
        {
            let info = surface.painter.adapter();
            Ok(json!({
                "name": info.name,
                "backend": format!("{:?}", info.backend),
                "device": format!("{:?}", info.device_type),
                "painter": surface.painter.painter(),
            }))
        }
        #[cfg(not(target_vendor = "apple"))]
        {
            let _ = surface;
            Err(said("a layer is painted on the Mac, by Metal: there is none here"))
        }
    }))
}

/// Let go of a surface; the layer stays the caller's.
///
/// # Safety
/// `surface` is a live handle from [`scaena_surface_new`], or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_surface_free(surface: *mut ScaenaSurface) {
    if !surface.is_null() {
        // SAFETY: the caller gives a handle this library made and has not freed.
        drop(unsafe { Box::from_raw(surface) });
    }
}

/// The deck exported as `format` (PLAN 3.8), the bytes `scaena export` writes for it: `pdf`, a page
/// for each slide at its last state, as the browser's PDF module draws it from the pages the
/// session lays out; or `png`, `args` `{state, width}`, the state at rest painted by the CPU
/// painter that wide, as the editor's PNG export paints it; or `version`, the same of the version
/// `viewVersion` shows (PLAN 3.15). Null bytes where it cannot be made, `*error` then saying why.
///
/// # Safety
/// `session` is a live handle; `format` a NUL-terminated string; `args` one, or null; `error`
/// null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_export(
    session: *mut ScaenaSession,
    format: *const c_char,
    args: *const c_char,
    error: *mut *mut c_char,
) -> ScaenaBytes {
    let made = guarded(|| {
        let session = unsafe { handle(session, "session") }?;
        let format = unsafe { text(format, "format") }?;
        let args: Value = match unsafe { maybe_text(args, "args") }? {
            None => json!({}),
            Some(a) => {
                serde_json::from_str(a).map_err(|e| said(format!("{format}: the arguments are not JSON: {e}")))?
            }
        };
        match format {
            "pdf" => {
                use scaena_export::pdf::{PdfSettings, Prepared, prepared};
                let laid = session.0.pdf_laid_out().map_err(said)?;
                let laid = Prepared::from_bytes(&laid).map_err(said)?;
                prepared(&laid, &PdfSettings::default()).map_err(said)
            }
            "png" => {
                let state = args
                    .get("state")
                    .and_then(Value::as_str)
                    .ok_or_else(|| said("png: `state` is a string it needs"))?;
                let width = (args.get("width").and_then(Value::as_u64))
                    .and_then(|w| u32::try_from(w).ok())
                    .filter(|w| (1..=16384).contains(w))
                    .ok_or_else(|| said("png: `width` is a number of pixels, 1 to 16384"))?;
                session.0.png(state, width).map_err(said)
            }
            // The version shown (`viewVersion`, PLAN 2.60): `args` `{state, width}`, as `png`.
            "version" => {
                let state = args
                    .get("state")
                    .and_then(Value::as_str)
                    .ok_or_else(|| said("version: `state` is a string it needs"))?;
                let width = (args.get("width").and_then(Value::as_u64))
                    .and_then(|w| u32::try_from(w).ok())
                    .filter(|w| (1..=16384).contains(w))
                    .ok_or_else(|| said("version: `width` is a number of pixels, 1 to 16384"))?;
                session.0.version_png(state, width).map_err(said)
            }
            _ => Err(said(format!("`{format}` is not an export this library makes: pdf, png, or version"))),
        }
    });
    match made {
        Ok(bytes) => ScaenaBytes::of(bytes),
        Err(failure) => {
            unsafe { report(error, failure) };
            ScaenaBytes::NONE
        }
    }
}

/// A conversation with a model (PLAN 3.6, ADR-0022), kept between questions until it is
/// forgotten: the user's own key, and the tools the browser's assistant has. Free with
/// [`scaena_chat_free`].
pub struct ScaenaChat(chat::Asking);

/// Begin a conversation with `args`' model, `{"provider": "anthropic" | "openai" | "gemini",
/// "model", "base"?}`, which calls the tools a page's assistant has (`deck_read`, `deck_patch`,
/// `deck_lint`, `deck_render`, …) and `resource_read`. Null where it cannot begin, `*error` then
/// saying why.
///
/// # Safety
/// `args` is a NUL-terminated string; `error` null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_chat_new(args: *const c_char, error: *mut *mut c_char) -> *mut ScaenaChat {
    let made = guarded(|| {
        let args = unsafe { text(args, "args") }?;
        let args: Value = serde_json::from_str(args).map_err(|e| said(format!("the arguments are not JSON: {e}")))?;
        chat::Asking::new(&args)
    });
    match made {
        Ok(asking) => Box::into_raw(Box::new(ScaenaChat(asking))),
        Err(failure) => {
            unsafe { report(error, failure) };
            std::ptr::null_mut()
        }
    }
}

/// One step of the conversation `chat`, on `session`'s bundle, as `{"ok": value}` or
/// `{"error": {"message"}}`. The client makes each request, and hands its answer back:
///
/// - `ask {text, seeing?}`: the user's question, begun with what the window shows (`seeing`:
///   `{state, format?, nodes: [{node, type?}], characters?: {node, from, to, text}}`), the model
///   told the deck as it is now.
/// - `request {key}`: the request for the model's next turn, `{method, url, headers, body}`, the
///   body the text to send; the key goes into its headers and is kept nowhere.
/// - `answer {status, statusText?, body}`: the answer read, `{"next": "calls", text, calls,
///   usage}`, the calls to run, or `{"next": "done", text, stop, usage}`; a provider's refusal is
///   an error, and the conversation is as it was.
/// - `run {index, at?}`: the last answer's call `index` run on the session by `agent:` and the
///   model's name, at `at` (RFC 3339): `{id, name, error, summary, json, png?, edited,
///   rewritten}`. A `call` `{id, name, args}` may be given in its place.
/// - `next`: the calls' results handed to the model, every call answered (those not run, as
///   stopped): whether it may be asked again, or the question has taken its rounds.
/// - `use {provider, model, base?}`: another model from the next request on, the conversation
///   kept. `forget`: the next question is the first. `conversation`: the conversation as kept.
///
/// `session` may be null for a step that runs nothing on it: all but `ask` and `run`.
///
/// # Safety
/// `chat` is a live handle; `session` one, or null; `method` a NUL-terminated string; `args` one,
/// or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_chat_call(
    chat: *mut ScaenaChat,
    session: *mut ScaenaSession,
    method: *const c_char,
    args: *const c_char,
) -> *mut c_char {
    envelope(guarded(|| {
        let chat = unsafe { handle(chat, "chat") }?;
        // SAFETY: the caller gives a live handle, used by one thread at a time, or null.
        let session = unsafe { session.as_mut() }.map(|s| &mut s.0);
        let method = unsafe { text(method, "method") }?;
        let args = match unsafe { maybe_text(args, "args") }? {
            None => json!({}),
            Some(a) => {
                serde_json::from_str(a).map_err(|e| said(format!("{method}: the arguments are not JSON: {e}")))?
            }
        };
        chat.0.call(session, method, &args)
    }))
}

/// Free a conversation.
///
/// # Safety
/// `chat` is a handle this library made and has not freed, or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_chat_free(chat: *mut ScaenaChat) {
    if !chat.is_null() {
        // SAFETY: the caller gives a handle this library made and has not freed.
        drop(unsafe { Box::from_raw(chat) });
    }
}

/// The models' providers, as `{"ok": value}` or `{"error": {"message"}}`: `list`, each provider
/// `{id, name, base}`; `models {provider, key, base?}`, the request that lists the models a key
/// can use, as `request` gives one; and `readModels {provider, status, statusText?, body}`, the
/// models its answer lists.
///
/// # Safety
/// `method` is a NUL-terminated string; `args` one, or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scaena_providers(method: *const c_char, args: *const c_char) -> *mut c_char {
    envelope(guarded(|| {
        let method = unsafe { text(method, "method") }?;
        let args = match unsafe { maybe_text(args, "args") }? {
            None => json!({}),
            Some(a) => {
                serde_json::from_str(a).map_err(|e| said(format!("{method}: the arguments are not JSON: {e}")))?
            }
        };
        chat::providers(method, &args)
    }))
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

/// A patch handed in as `ops`, a list of operations; none where none is.
fn patch_of(ops: Option<&Value>, method: &str) -> Result<Option<Vec<Value>>, Failure> {
    match ops {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(ops)) => Ok(Some(ops.clone())),
        Some(_) => Err(said(format!("{method}: `ops` is a list of operations"))),
    }
}

/// The bundle's history, and the versions it keeps, oldest first; an error that says how to begin
/// one where it keeps none.
fn history_of(s: &Session) -> Result<(scaena_store::crdt::DeckDoc, Vec<scaena_ops::history::Version>), Failure> {
    let bytes = s
        .file(scaena_store::HISTORY)
        .ok_or_else(|| said("the bundle keeps no history: Keep a History begins one with the next save"))?;
    let doc = scaena_store::crdt::DeckDoc::load(bytes).map_err(said)?;
    let versions = scaena_ops::history::listed(&doc);
    Ok((doc, versions))
}

/// Version `name` (its number as listed, or its id) as a session takes one, as the history's own
/// module hands a page one: the deck as `deck.json`'s text, and the files it is drawn from, its
/// data files and its theme, as their text.
fn held(s: &Session, name: &str) -> Result<scaena_session::versions::Held, Failure> {
    let (doc, versions) = history_of(s)?;
    let version = scaena_ops::history::named(&versions, name).map_err(said)?;
    let (deck, files) = scaena_ops::history::then(&doc, version).map_err(said)?;
    let mut held = scaena_session::versions::Held { deck: deck.to_json().map_err(said)?, files: BTreeMap::new() };
    for (path, bytes) in files {
        held.files.insert(path, String::from_utf8_lossy(&bytes).into_owned());
    }
    Ok(held)
}

/// The session's answer to `method`, as `Player` gives it a page.
fn call(s: &mut Session, method: &str, args: &Value) -> Result<Value, Failure> {
    let arg = |key: &str| {
        args.get(key).and_then(Value::as_str).ok_or_else(|| said(format!("{method}: `{key}` is a string it needs")))
    };
    let optional = |key: &str| args.get(key).and_then(Value::as_str);
    let number = |key: &str| {
        args.get(key).and_then(Value::as_f64).ok_or_else(|| said(format!("{method}: `{key}` is a number it needs")))
    };
    let value = |v: Result<Value, serde_json::Error>| v.map_err(said);
    // An edit the panels make: by the user, at the time the call gives (RFC 3339), if it gives one.
    let at = || optional("at").and_then(scaena_session::store::seconds);
    let user = || Caller { author: "user", at: at() };
    let strings = |key: &str| -> Result<Vec<String>, Failure> {
        let list = args.get(key).and_then(Value::as_array);
        let all = list.map(|l| l.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>());
        all.filter(|all| Some(all.len()) == list.map(Vec::len))
            .ok_or_else(|| said(format!("{method}: `{key}` is a list of strings it needs")))
    };
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
        "reads" => value(serde_json::to_value(s.reads(arg("state")?).map_err(said)?))?,
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
        "boxes" => json!(s.boxes_json(arg("state")?).map_err(said)?),
        "hit" => json!(s.hits_json(arg("state")?, [number("x")? as f32, number("y")? as f32]).map_err(said)?),
        // The link drawn at a point at rest (PLAN 2.70): `{"href"}` or `{"state"}`, or null.
        "linkAt" => json!(s.link_at(arg("state")?, [number("x")? as f32, number("y")? as f32]).map_err(said)?),
        "layers" => value(serde_json::to_value(s.layers(arg("state")?).map_err(said)?))?,
        "carets" => s.carets_json(arg("state")?, arg("node")?).map_err(said)?,
        // Text typed in place (PLAN 2.32, 3.9): `replace_text`, `style_text`, and `list` ops by
        // the user at `at` (RFC 3339), validated as a patch is but not linted; whether the deck
        // changed.
        "typed" => {
            let ops = patch_of(args.get("ops"), method)?.ok_or_else(|| said("typed: `ops` is a list it needs"))?;
            let at = optional("at").and_then(scaena_session::store::seconds);
            json!(s.typed(&Value::Array(ops), at).map_err(said)?)
        }
        // What ⌘B and ⌘I give the characters `from` to `to` (Unicode scalar values): `style_text`'s
        // `look` (PLAN 2.38, 2.40).
        "bolding" | "italicizing" => {
            let (state, node) = (arg("state")?, arg("node")?);
            let (from, to) = (number("from")? as usize, number("to")? as usize);
            match method {
                "bolding" => s.bolding(state, node, from, to),
                _ => s.italicizing(state, node, from, to),
            }
            .map_err(said)?
        }
        "targets" => {
            let found = s.targets(arg("state")?, arg("node")?).map_err(said)?.clone();
            value(serde_json::to_value(scaena_ops::inspect::Targets::from(found)))?
        }
        "snap" => {
            let how: scaena_ops::inspect::SnapMode = arg("how")?.parse().map_err(said)?;
            let to = [number("x")? as f32, number("y")? as f32, number("w")? as f32, number("h")? as f32];
            let fork = args.get("fork").and_then(Value::as_bool).unwrap_or(false);
            let reach = args.get("reach").and_then(Value::as_f64).unwrap_or(0.0) as f32;
            // `grid`: a presentation app's drag, where it is let go, the grid's tracks drawing it
            // in (PLAN 3.19, ADR-0024).
            let grid = args.get("grid").and_then(Value::as_bool).unwrap_or(false);
            let (state, node) = (arg("state")?, arg("node")?);
            let guided = match grid && how == scaena_ops::inspect::SnapMode::Free {
                true => s.guided_freely(state, node, to, fork, reach),
                false => s.guided(state, node, how, to, fork, reach),
            };
            match guided.map_err(said)? {
                None => Value::Null,
                Some((snapped, guides)) => scaena_session::with_guides(value(serde_json::to_value(snapped))?, &guides),
            }
        }
        "setMoving" => {
            let nodes: Vec<String> = serde_json::from_value(args.get("nodes").cloned().unwrap_or_else(|| json!([])))
                .map_err(|e| said(format!("{method}: `nodes` is a list of node ids: {e}")))?;
            let by = [number("dx").unwrap_or(0.0) as f32, number("dy").unwrap_or(0.0) as f32];
            s.set_moving((!nodes.is_empty()).then_some((nodes, by)));
            Value::Null
        }
        "preview" => {
            let ops = patch_of(args.get("ops"), method)?;
            s.preview(ops.as_deref()).map_err(said)?;
            Value::Null
        }
        "reach" => {
            let ops = patch_of(args.get("ops"), method)?.ok_or_else(|| said(format!("{method}: `ops` is a patch")))?;
            json!(s.reach(&ops).map_err(said)?)
        }
        "choices" => value(serde_json::to_value(s.choices(arg("state")?, arg("node")?).map_err(said)?))?,
        // What an inspector offers for the characters `from` to `to` (characters, as
        // `replace_text` counts them) of a text typed in (PLAN 2.38, 3.10).
        "characterChoices" => {
            let (state, node) = (arg("state")?, arg("node")?);
            let (from, to) = (number("from")? as usize, number("to")? as usize);
            value(serde_json::to_value(s.character_choices(state, node, from, to).map_err(said)?))?
        }
        "stateChoices" => value(serde_json::to_value(s.state_choices(arg("state")?).map_err(said)?))?,
        "layoutsBegin" => json!(s.layouts_begin(arg("state")?).map_err(said)?),
        "layoutsStep" => json!(s.layouts_step().map_err(said)?),
        "layoutSuggestions" => json!(s.layouts_painted(number("height")?.max(1.0) as u32).map_err(said)?),
        "inserts" => value(serde_json::to_value(s.inserts()))?,
        // What Insert, ⌘D, and Delete make (PLAN 2.34, 3.11): `{id, cell, patch}`, or the ops.
        "inserting" => {
            let (state, n) = (arg("state")?, number("n")? as usize);
            let at = [number("x")? as f32, number("y")? as f32];
            // `with`: properties of the node's own on what is offered, a pasted sheet's columns.
            let with = args.get("with").and_then(Value::as_object);
            value(serde_json::to_value(s.inserting_with(state, n, at, optional("named"), with).map_err(said)?))?
        }
        "duplicating" => value(serde_json::to_value(s.duplicating(arg("state")?, arg("node")?).map_err(said)?))?,
        // A state added after the state shown (PLAN 2.35, 3.14): a `step` of its slide, or a
        // `slide` of its own, `{id, patch}`.
        "addingState" => {
            let what = serde_json::from_value(Value::String(arg("what")?.to_string())).map_err(said)?;
            value(serde_json::to_value(s.adding_state(arg("state")?, what).map_err(said)?))?
        }
        // The panels (PLAN 3.15). The theme (PLAN 2.39, 2.61, 2.94): the themes that ship; the deck
        // put in another, one the bundle holds by its path or one that ships by its name, with its
        // fonts; and the theme edited by RFC 6902 operations, or to a photo's colors, with the file
        // it wrote, before and after, for the undo.
        "shippedThemes" => {
            let shipped = scaena_ops::shipped::THEMES.iter().map(|t| json!({ "name": t.name, "file": t.file }));
            Value::Array(shipped.collect())
        }
        "retheme" => {
            let themed = match optional("ships") {
                Some(name) => {
                    let shipped = scaena_ops::shipped::theme(name).ok_or_else(|| {
                        said(format!("`{name}` is not a theme that ships: {}", scaena_ops::shipped::names()))
                    })?;
                    let mut fonts = BTreeMap::new();
                    for (path, bytes) in scaena_ops::shipped::fonts() {
                        fonts.insert(path.to_string(), bytes.to_vec());
                    }
                    s.retheme(&format!("themes/{}", shipped.file), Some(shipped.text), fonts, at())
                }
                None => s.retheme(arg("path")?, None, BTreeMap::new(), at()),
            };
            value(serde_json::to_value(themed.map_err(said)?))?
        }
        "themeEdit" => {
            let asked = json!({ "ops": args.get("ops").cloned().unwrap_or(json!([])), "photo": args.get("photo") });
            let edit: scaena_ops::theme::ThemeEdit = serde_json::from_value(asked).map_err(said)?;
            let dry = args.get("dryRun").and_then(Value::as_bool).unwrap_or(false);
            let (edited, files) = s.theme_edit(&edit, dry, user()).map_err(said)?;
            json!({ "edited": edited, "files": files })
        }
        // The bundle's files (PLAN 2.59): each with what names it and the nodes drawn from it; and
        // one nothing names taken out, which `dataUndo` puts back.
        "bundleFiles" => value(serde_json::to_value(s.bundle_files().map_err(said)?))?,
        "removeFile" => {
            s.remove_file(arg("path")?).map_err(said)?;
            Value::Null
        }
        // The data (PLAN 2.55): the deck's sources, one as a sheet, its rows edited as `data_edit`
        // edits them, and the file an edit wrote, or the Files panel took out, put back, or with
        // `redo` written again: the source it is, or null where there was nothing to undo.
        "dataSources" => {
            let source = |(name, file): (String, Option<String>)| match file {
                Some(file) => json!({ "name": name, "file": file }),
                None => json!({ "name": name }),
            };
            Value::Array(s.data_sources().into_iter().map(source).collect())
        }
        "dataSheet" => {
            let (sheet, file) = s.data_sheet(arg("name")?).map_err(said)?;
            json!({ "sheet": sheet, "file": file })
        }
        "dataEdit" => {
            let asked = json!({ "source": arg("source")?, "edits": args.get("edits").cloned().unwrap_or(json!([])) });
            let req: scaena_ops::data::DataEdit = serde_json::from_value(asked).map_err(said)?;
            let (result, wrote) = s.data_edit(&req, false, user()).map_err(said)?;
            json!({ "result": result, "wrote": wrote })
        }
        "dataUndo" => {
            let redo = args.get("redo").and_then(Value::as_bool).unwrap_or(false);
            json!(s.data_undo(redo, user()).map_err(said)?)
        }
        // Handles and views (PLAN 3.16): a shape's outline (PLAN 2.68), an image's framing and the
        // point of it under a press (PLAN 2.45, 2.74), the theme's grid (PLAN 2.57), the part of the
        // canvas frames are painted through (PLAN 2.46), and the deck's texts found and replaced
        // (PLAN 2.47).
        "outline" => value(serde_json::to_value(s.outline(arg("state")?, arg("node")?).map_err(said)?))?,
        "framing" => value(serde_json::to_value(s.framing(arg("state")?, arg("node")?).map_err(said)?))?,
        "focalAt" => {
            let at = [number("x")? as f32, number("y")? as f32];
            // To a thousandth, as a person would write it.
            let point = s.focal_at(arg("state")?, arg("node")?, at).map_err(said)?;
            json!(point.map(|p| p.map(|v| (v * 1000.0).round() / 1000.0)))
        }
        "grid" => {
            let g = s.grid().map_err(said)?;
            json!({ "canvas": g.canvas, "columns": g.columns, "rows": g.rows, "baselines": g.baselines, "safe": g.safe })
        }
        "setView" => {
            let view = match args.get("view").and_then(Value::as_array) {
                None => None,
                Some(v) => match v.iter().map(Value::as_f64).collect::<Option<Vec<f64>>>().as_deref() {
                    Some(&[x, y, w, h]) => Some([x as f32, y as f32, w as f32, h as f32]),
                    _ => return Err(said("setView: a view is [x, y, w, h], canvas units")),
                },
            };
            s.set_view(view).map_err(said)?;
            Value::Null
        }
        "find" => {
            let query = serde_json::from_value(args.get("query").cloned().unwrap_or(Value::Null)).map_err(said)?;
            value(serde_json::to_value(s.find(&query).map_err(said)?))?
        }
        "replacing" => {
            let query = serde_json::from_value(args.get("query").cloned().unwrap_or(Value::Null)).map_err(said)?;
            let one = match args.get("one").and_then(Value::as_array) {
                None => None,
                Some(one) => match one.iter().map(Value::as_u64).collect::<Option<Vec<u64>>>().as_deref() {
                    Some(&[i, k]) => Some([i as usize, k as usize]),
                    _ => return Err(said("replacing: one match is [text, match]")),
                },
            };
            json!(s.replacing(&query, arg("with")?, one).map_err(said)?)
        }
        // The versions the bundle's history keeps (PLAN 2.60), read from it as `scaena history`
        // reads them: listed; one shown read only in a session of its own, its states' ids (its
        // states drawn by `scaena_export`'s `version`); one compared with another or with the deck
        // now; and one made the deck again, with each file it wrote for the undo. A version is
        // named by its number as listed, or its id.
        "versions" => json!(history_of(s)?.1),
        "viewVersion" => {
            let held = held(s, arg("version")?)?;
            json!(s.view_version(&held).map_err(said)?)
        }
        "compareVersions" => {
            let from = held(s, arg("from")?)?;
            let to = optional("to").map(|to| held(s, to)).transpose()?;
            s.compare_versions(&from, to.as_ref()).map_err(said)?
        }
        "restoreVersion" => {
            let name = arg("version")?;
            let (_, versions) = history_of(s)?;
            let version = scaena_ops::history::named(&versions, name).map_err(said)?.clone();
            let held = held(s, name)?;
            let (restored, files) = s.restore_version(&held, version, user()).map_err(said)?;
            json!({ "restored": restored, "files": files })
        }
        "deleting" => {
            let everywhere = args.get("everywhere").and_then(Value::as_bool).unwrap_or(false);
            json!(s.deleting(arg("state")?, arg("node")?, everywhere).map_err(said)?)
        }
        // Several selected (PLAN 2.42, 2.43, 3.13): moved together as a drag moves the first, the
        // guides the box around them meets; arranged; grouped.
        "together" => {
            let by = [number("dx")? as f32, number("dy")? as f32];
            let free = args.get("free").and_then(Value::as_bool).unwrap_or(false);
            let fork = args.get("fork").and_then(Value::as_bool).unwrap_or(false);
            let reach = args.get("reach").and_then(Value::as_f64).unwrap_or(0.0) as f32;
            match s.together(arg("state")?, &strings("nodes")?, by, free, fork, reach).map_err(said)? {
                None => Value::Null,
                Some((arranged, guides)) => {
                    scaena_session::with_guides(value(serde_json::to_value(arranged))?, &guides)
                }
            }
        }
        "arranging" => {
            let asked: scaena_ops::arrange::Asked =
                serde_json::from_value(args.get("how").cloned().unwrap_or_default())
                    .map_err(|e| said(format!("{method}: `how` is one way to arrange them: {e}")))?;
            let how = asked.how().map_err(said)?;
            let fork = args.get("fork").and_then(Value::as_bool).unwrap_or(false);
            value(serde_json::to_value(s.arranging(arg("state")?, &strings("nodes")?, how, fork).map_err(said)?))?
        }
        "grouping" => value(serde_json::to_value(s.grouping(arg("state")?, &strings("nodes")?)))?,
        // The clipboard (PLAN 2.37, 2.58, 2.96, 3.12): a clip of nodes, pasted; a node's look,
        // put on others; a sheet's cells pasted, as a data source.
        // The clip as the text the clipboard holds, as the browser writes it.
        "copying" => {
            let nodes = strings("nodes")?;
            let nodes: Vec<&str> = nodes.iter().map(String::as_str).collect();
            json!(serde_json::to_string(&s.copying(arg("state")?, &nodes).map_err(said)?).map_err(said)?)
        }
        "pasting" => {
            let text = arg("text")?;
            // A clip pastes what it holds; other text, a text in the theme's body role.
            let clip = match scaena_ops::clipboard::read(text).map_err(said)? {
                Some(clip) => clip,
                None => scaena_ops::clipboard::of_text(s.deck(), s.theme(), text).map_err(said)?,
            };
            let at = [number("x")? as f32, number("y")? as f32];
            value(serde_json::to_value(s.pasting(&clip, arg("state")?, at).map_err(said)?))?
        }
        "look" => value(serde_json::to_value(s.look(arg("state")?, arg("node")?).map_err(said)?))?,
        "putting" => {
            let look = serde_json::from_value(args.get("look").cloned().unwrap_or_default())
                .map_err(|e| said(format!("{method}: `look` is a look as `look` gives it: {e}")))?;
            value(serde_json::to_value(s.putting(arg("state")?, &look, &strings("nodes")?).map_err(said)?))?
        }
        "cells" => value(serde_json::to_value(s.cells(arg("text")?)))?,
        "attaching" => {
            let schema = (args.get("schema").filter(|v| !v.is_null()).cloned())
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| said(format!("{method}: `schema` is each column's type by its name: {e}")))?;
            value(serde_json::to_value(s.attaching(arg("path")?, schema).map_err(said)?))?
        }
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
        "writeFiles" => {
            let files = serde_json::from_value(args.get("files").cloned().unwrap_or_default())
                .map_err(|e| said(format!("{method}: `files` is a list of {{path, text}}: {e}")))?;
            s.write_files(files);
            Value::Null
        }
        _ => return Err(said(format!("`{method}` is not a call this library answers"))),
    })
}
