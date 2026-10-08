#ifndef SCAENA_H
#define SCAENA_H

/* Made by cbindgen from crates/scaena-ffi (`just bless`): edit the Rust, not this. */

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

// A conversation with a model (PLAN 3.6, ADR-0022), kept between questions until it is
// forgotten: the user's own key, and the tools the browser's assistant has. Free with
// [`scaena_chat_free`].
typedef struct ScaenaChat ScaenaChat;

// A bundle's files, gathered to open it.
typedef struct ScaenaFiles ScaenaFiles;

// A save: every file of the bundle as saved, for the caller to write where it keeps the
// bundle, then to [adopt](scaena_adopt).
typedef struct ScaenaSaved ScaenaSaved;

// One bundle open for editing.
typedef struct ScaenaSession ScaenaSession;

// Frames painted onto a `CAMetalLayer` by vello on Metal (PLAN 3.2): what the Mac's view
// shows. Made by [`scaena_surface_new`], on the Mac alone.
typedef struct ScaenaSurface ScaenaSurface;

// Bytes this library made, the caller's to free with [`scaena_bytes_free`]. `data` is null
// where there are none, as where a call failed.
typedef struct ScaenaBytes {
  uint8_t *data;
  size_t len;
} ScaenaBytes;

// A frame painted: straight-alpha sRGB, four bytes a pixel, row by row, `width` × `height`.
typedef struct ScaenaPixels {
  struct ScaenaBytes bytes;
  uint32_t width;
  uint32_t height;
} ScaenaPixels;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

// Begin gathering a bundle's files. Free with [`scaena_files_free`], or hand to [`scaena_open`],
// which takes them.
struct ScaenaFiles *scaena_files_new(void);

// Add the file at `path` in the bundle (`deck.json`, `fonts/Inter-VF.ttf`, …), its `len`
// bytes copied. False where `files` or `path` is null or `path` is not UTF-8.
//
// # Safety
// `files` is a live handle from [`scaena_files_new`]; `path` a NUL-terminated string; `bytes`
// `len` readable bytes, or null where `len` is 0.
bool scaena_files_add(struct ScaenaFiles *files,
                      const char *path,
                      const uint8_t *bytes,
                      size_t len);

// Let go of files not opened.
//
// # Safety
// `files` is a live handle from [`scaena_files_new`], or null.
void scaena_files_free(struct ScaenaFiles *files);

// Open the bundle `files` hold, as a page opens one from a folder: its `deck.json`, the theme
// it names, and every other file by its path. Takes `files`, opened or not. Null where the
// bundle cannot be opened, `*error` then saying why (JSON `{"message"}`; free it).
//
// # Safety
// `files` is a live handle from [`scaena_files_new`]; `error` is null or writable.
struct ScaenaSession *scaena_open(struct ScaenaFiles *files, char **error);

// Open a `.scaena` zip, its `len` bytes. Null where it cannot be opened, `*error` then saying
// why.
//
// # Safety
// `bytes` is `len` readable bytes; `error` is null or writable.
struct ScaenaSession *scaena_open_zip(const uint8_t *bytes, size_t len, char **error);

// A new bundle (PLAN 2.12, 3.3), as New makes one in the browser and `deck_create` does: the
// theme that ships as `theme` (`dusk`, `daybreak`, or `ember`), the fonts it names, and one
// state with nothing on it, titled `title`. Kept nowhere until it is saved. Null where no such
// theme ships, `*error` then saying why.
//
// # Safety
// `theme` and `title` are NUL-terminated strings; `error` is null or writable.
struct ScaenaSession *scaena_create(const char *theme, const char *title, char **error);

// Close a session.
//
// # Safety
// `session` is a live handle from [`scaena_open`] or [`scaena_open_zip`], or null.
void scaena_session_free(struct ScaenaSession *session);

// Hand over a file at any time, by its path in the bundle: an image dropped, a font, data. One
// the deck draws with builds the engine again on the next frame. False where an argument is
// null or `path` is not UTF-8.
//
// # Safety
// `session` is a live handle; `path` a NUL-terminated string; `bytes` `len` readable bytes.
bool scaena_add_file(struct ScaenaSession *session,
                     const char *path,
                     const uint8_t *bytes,
                     size_t len);

// A file dropped or pasted on the canvas (PLAN 2.45, 2.76, 2.96, 3.12), handed over where the
// browser's canvas keeps one: a data file (`.csv`, `.json`) under `data/` by its name, numbered
// where the bundle holds other bytes there; anything else, an image above all, under `assets/`,
// named by its SHA-256 as a save names it. `{"ok": path}` or `{"error": {"message"}}`, a
// string to free.
//
// # Safety
// `session` is a live handle; `name` a NUL-terminated string; `bytes` `len` readable bytes.
char *scaena_drop(struct ScaenaSession *session,
                  const char *name,
                  const uint8_t *bytes,
                  size_t len);

// Answer `method` with `args` (a JSON object, or null for none), as a page's `Player` does:
// `{"ok": value}` or `{"error": {"message"}}`, a string to free. The calls ([`call`]):
// `states`, `formats`, `setFormat {format?}`, `canvasSize`, `duration {state}`, `timeline`,
// `files`, `imageFiles`, `digest {state}`, `reading {state}`, `source`, `compiledFrom
// {source}`, `compile {source}`, `lint {state?}`, `fix {patch}`, `inspect {state}`, `boxes
// {state}`, `hit {state, x, y}`, `layers {state}`, `carets {state, node}`, `choices {state,
// node}`, `stateChoices {state}`, `inserts`, `themes`, `themeText`, `keepHistory`,
// `keepsHistory`, and `writeFiles {files: [{path, text}]}`, which writes files back as an undo
// has them (`text` null: taken out). A drag (PLAN 3.7, ADR-0013): `targets {state, node}`,
// where a node may go; `snap {state, node, how, x, y, w, h, fork?, reach?}`, where a box left
// there lands and the patch that puts it there; `setMoving {nodes, dx, dy}`, the nodes drawn
// moved in frames at rest, laying nothing out; `preview {ops?}`, frames at rest drawn as a
// patch would make them; and `reach {ops}`, the states a patch changes. Text typed in place
// (PLAN 3.9): `typed {ops, at?}`, `replace_text`, `style_text`, and `list` ops by the user,
// validated but not linted, and whether the deck changed; and `bolding` and `italicizing
// {state, node, from, to}`, the look ⌘B and ⌘I give characters. A text's characters (PLAN
// 3.10): `characterChoices {state, node, from, to}`, what an inspector offers for them; and
// `linkAt {state, x, y}`, the link drawn there at rest, where a click goes. Nodes added and
// taken away (PLAN 3.11): `inserting {state, n, x, y, named?, with?}`, the patch that inserts
// what `inserts` offers `n`th about a point, or in the room nearest it, `with` properties of its
// own set on it; `duplicating {state, node}`,
// a copy beside it; each `{id, cell, patch}`; and `deleting {state, node, everywhere?}`, the
// ops that take it out of the state and those after, or out of the deck. The clipboard (PLAN
// 3.12): `copying {state, nodes}`, the clip as text; `pasting {text, state, x, y}`, the patch that
// pastes a clip, or other text as a text, about a point, the files it carries handed over;
// `look {state, node}` and `putting {state, look, nodes}`, a look copied and the patch that
// puts it on others; `cells {text}`, a sheet's cells as the source they would be, or null; and
// `attaching {path, schema?}`, a data file the bundle holds as the source a chart of it reads.
// Several selected (PLAN 3.13): `together {state, nodes, dx, dy, free?, fork?, reach?}`,
// children of one container moved together as a drag moves the first, with the guides they
// meet; `arranging {state, nodes, how, fork?}`, them aligned, spread, or ordered (`how`: one of
// `align`, `spread`, `order`, `before`, `after`, `into`); and `grouping {state, nodes}`, the
// patch that puts them in a new group, `{id, patch}`. States (PLAN 3.14): `addingState {state,
// what}`, the patch that adds a state after it, a `step` of its slide or a `slide` of its own,
// `{id, patch}`. The panels (PLAN 3.15), each edit by the user at `at` (RFC 3339) where given:
// `shippedThemes`; `retheme {path | ships, at?}`, the deck in a theme the bundle holds or one
// that ships, as `theme --apply` says it; `themeEdit {ops | photo, dryRun?, at?}`, `{edited,
// files}`, the theme file written before and after for the undo; `bundleFiles` and `removeFile
// {path}`; `dataSources`, `dataSheet {name}`, `dataEdit {source, edits, at?}`, `{result,
// wrote}`, and `dataUndo {redo?, at?}`, the file an edit wrote or a removal took out put back or
// written again; and, from the bundle's history, `versions`, `viewVersion {version}`, its
// states, `compareVersions {from, to?}`, and `restoreVersion {version, at?}`, `{restored,
// files}`.
//
// # Safety
// `session` is a live handle; `method` a NUL-terminated string; `args` one, or null.
char *scaena_call(struct ScaenaSession *session, const char *method, const char *args);

// Run the MCP server's operation `name` on the bundle with `args` (its tool's arguments, less
// `bundle`, `out`, and `painter`), as `author` (null: `agent`) at `at` (RFC 3339, or null):
// `deck_patch`, `deck_lint`, `deck_inspect`, `deck_find`, `theme_edit`, `data_edit`, and the
// rest (SPEC §7.2). `{"ok": result, "edited": bool}`, `edited` where it changed the deck, or
// `{"error": {"message", "plan"?, "op"?}}`, as the tool says it.
//
// # Safety
// `session` is a live handle; `name` a NUL-terminated string; `args`, `author`, and `at` one
// each, or null.
char *scaena_tool(struct ScaenaSession *session,
                  const char *name,
                  const char *args,
                  const char *author,
                  const char *at);

// `state`'s display list at `t_ms` (infinity: at rest), postcard-encoded (SPEC §6): the bytes
// the browser's module gives for it. Null bytes where it cannot be made, `*error` then saying
// why.
//
// # Safety
// `session` is a live handle; `state` a NUL-terminated string; `error` null or writable.
struct ScaenaBytes scaena_frame(struct ScaenaSession *session,
                                const char *state,
                                double t_ms,
                                char **error);

// `state` at `t_ms` (infinity: at rest), painted by the CPU painter `width` pixels wide, the
// height keeping the canvas's aspect: the pixels the goldens hold (SPEC §13.5). Null bytes
// where it cannot be painted, `*error` then saying why.
//
// # Safety
// `session` is a live handle; `state` a NUL-terminated string; `error` null or writable.
struct ScaenaPixels scaena_pixels(struct ScaenaSession *session,
                                  const char *state,
                                  double t_ms,
                                  uint32_t width,
                                  char **error);

// Save the bundle with the deck shown at `now` (RFC 3339; the engine reads no clock), as
// `scaena save` lays one out: files named by their content, fonts subset to what the deck can
// draw if `subset`, and every edit since the last save recorded in its history, where it keeps
// one (or [`keepHistory`](call) began one). The session keeps the bundle as it was until it
// [adopts](scaena_adopt) the save. Null where it cannot be saved, `*error` then saying why.
//
// # Safety
// `session` is a live handle; `now` a NUL-terminated string; `error` null or writable.
struct ScaenaSaved *scaena_save(struct ScaenaSession *session,
                                const char *now,
                                bool subset,
                                char **error);

// The saved bundle's files and what the save did, as JSON: `{"files": [path], "replaced":
// [path], "summary": …}`. `replaced` are the files of the bundle as it was that the save
// renamed or rewrote: one saved in place drops those `files` does not hold.
//
// # Safety
// `saved` is a live handle from [`scaena_save`].
char *scaena_saved_list(const struct ScaenaSaved *saved);

// The bytes of the saved file at `path`, to write where the bundle is kept; null bytes where
// the save holds no such file.
//
// # Safety
// `saved` is a live handle from [`scaena_save`]; `path` a NUL-terminated string.
struct ScaenaBytes scaena_saved_file(const struct ScaenaSaved *saved, const char *path);

// The saved bundle as one `.scaena` zip.
//
// # Safety
// `saved` is a live handle from [`scaena_save`]; `error` null or writable.
struct ScaenaBytes scaena_saved_zip(const struct ScaenaSaved *saved, char **error);

// Go on from `saved`, once the caller has written it where it keeps the bundle: its files are
// the session's from now on, and its deck, which names files by their content, is shown; the
// source is the saved deck's. `{"ok": null}` or `{"error"}`.
//
// # Safety
// `session` is a live handle; `saved` one from [`scaena_save`].
char *scaena_adopt(struct ScaenaSession *session, const struct ScaenaSaved *saved);

// Let go of a save.
//
// # Safety
// `saved` is a live handle from [`scaena_save`], or null.
void scaena_saved_free(struct ScaenaSaved *saved);

// Make the GPU every surface paints with, if it is not made yet: what the app calls as it starts,
// off its main thread, so that the first deck it opens shows its first frame without waiting for
// it (gate 3). A surface made meanwhile waits for it. False where none can be made, as on any
// machine but a Mac, `*error` then saying why.
//
// # Safety
// `error` is null or writable.
bool scaena_gpu_warm(char **error);

// Paint on `layer`, a `CAMetalLayer`, `width` × `height` device pixels, on the GPU every surface
// shares, made first if [`scaena_gpu_warm`] has not made it; frames are presented at the
// display's refresh. The layer keeps the deck's aspect, and outlives the surface. Null where
// none can be made, as on any machine but a Mac, `*error` then saying why.
//
// # Safety
// `layer` is a live `CAMetalLayer` that outlives the surface; `error` is null or writable.
struct ScaenaSurface *scaena_surface_new(void *layer,
                                         uint32_t width,
                                         uint32_t height,
                                         char **error);

// Paint at `width` × `height` device pixels from now on: the view's size, or its screen's
// scale, changed.
//
// # Safety
// `surface` is a live handle from [`scaena_surface_new`].
bool scaena_surface_resize(struct ScaenaSurface *surface, uint32_t width, uint32_t height);

// Paint `state` at `t_ms` into its cue (infinity: at rest) from `session` onto the layer, as
// the canvas shows it (zoomed, where it is), and present it at the next refresh: 1. 0 where the
// layer had no drawable to give, hidden or resized meanwhile: the frame is skipped. -1 where
// it could not be painted, `*error` then saying why.
//
// # Safety
// `surface` and `session` are live handles; `state` a NUL-terminated string; `error` null or
// writable.
int32_t scaena_surface_paint(struct ScaenaSurface *surface,
                             struct ScaenaSession *session,
                             const char *state,
                             double t_ms,
                             char **error);

// The last frame the surface painted, read back as the layer was given it: what a test holds
// to [`scaena_pixels`] (SPEC §13.5). Null bytes before any frame, `*error` then saying why.
//
// # Safety
// `surface` is a live handle; `error` null or writable.
struct ScaenaPixels scaena_surface_pixels(struct ScaenaSurface *surface, char **error);

// The adapter that paints the surface, as JSON: `{"ok": {"name", "backend", "device"}}`.
//
// # Safety
// `surface` is a live handle.
char *scaena_surface_adapter(struct ScaenaSurface *surface);

// Let go of a surface; the layer stays the caller's.
//
// # Safety
// `surface` is a live handle from [`scaena_surface_new`], or null.
void scaena_surface_free(struct ScaenaSurface *surface);

// The deck exported as `format` (PLAN 3.8), the bytes `scaena export` writes for it: `pdf`, a page
// for each slide at its last state, as the browser's PDF module draws it from the pages the
// session lays out; or `png`, `args` `{state, width}`, the state at rest painted by the CPU
// painter that wide, as the editor's PNG export paints it; or `version`, the same of the version
// `viewVersion` shows (PLAN 3.15). Null bytes where it cannot be made, `*error` then saying why.
//
// # Safety
// `session` is a live handle; `format` a NUL-terminated string; `args` one, or null; `error`
// null or writable.
struct ScaenaBytes scaena_export(struct ScaenaSession *session,
                                 const char *format,
                                 const char *args,
                                 char **error);

// Begin a conversation with `args`' model, `{"provider": "anthropic" | "openai" | "gemini",
// "model", "base"?}`, which calls the tools a page's assistant has (`deck_read`, `deck_patch`,
// `deck_lint`, `deck_render`, …) and `resource_read`. Null where it cannot begin, `*error` then
// saying why.
//
// # Safety
// `args` is a NUL-terminated string; `error` null or writable.
struct ScaenaChat *scaena_chat_new(const char *args, char **error);

// One step of the conversation `chat`, on `session`'s bundle, as `{"ok": value}` or
// `{"error": {"message"}}`. The client makes each request, and hands its answer back:
//
// - `ask {text, seeing?}`: the user's question, begun with what the window shows (`seeing`:
//   `{state, format?, nodes: [{node, type?}], characters?: {node, from, to, text}}`), the model
//   told the deck as it is now.
// - `request {key}`: the request for the model's next turn, `{method, url, headers, body}`, the
//   body the text to send; the key goes into its headers and is kept nowhere.
// - `answer {status, statusText?, body}`: the answer read, `{"next": "calls", text, calls,
//   usage}`, the calls to run, or `{"next": "done", text, stop, usage}`; a provider's refusal is
//   an error, and the conversation is as it was.
// - `run {index, at?}`: the last answer's call `index` run on the session by `agent:` and the
//   model's name, at `at` (RFC 3339): `{id, name, error, summary, json, png?, edited,
//   rewritten}`. A `call` `{id, name, args}` may be given in its place.
// - `next`: the calls' results handed to the model, every call answered (those not run, as
//   stopped): whether it may be asked again, or the question has taken its rounds.
// - `use {provider, model, base?}`: another model from the next request on, the conversation
//   kept. `forget`: the next question is the first. `conversation`: the conversation as kept.
//
// `session` may be null for a step that runs nothing on it: all but `ask` and `run`.
//
// # Safety
// `chat` is a live handle; `session` one, or null; `method` a NUL-terminated string; `args` one,
// or null.
char *scaena_chat_call(struct ScaenaChat *chat,
                       struct ScaenaSession *session,
                       const char *method,
                       const char *args);

// Free a conversation.
//
// # Safety
// `chat` is a handle this library made and has not freed, or null.
void scaena_chat_free(struct ScaenaChat *chat);

// The models' providers, as `{"ok": value}` or `{"error": {"message"}}`: `list`, each provider
// `{id, name, base}`; `models {provider, key, base?}`, the request that lists the models a key
// can use, as `request` gives one; and `readModels {provider, status, statusText?, body}`, the
// models its answer lists.
//
// # Safety
// `method` is a NUL-terminated string; `args` one, or null.
char *scaena_providers(const char *method, const char *args);

// Free a string this library returned.
//
// # Safety
// `s` is a string this library returned and has not been freed, or null.
void scaena_string_free(char *s);

// Free bytes this library returned.
//
// # Safety
// `bytes` were returned by this library and have not been freed; null `data` is nothing.
void scaena_bytes_free(struct ScaenaBytes bytes);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* SCAENA_H */
