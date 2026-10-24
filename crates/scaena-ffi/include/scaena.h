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

// A deck being made from a prompt (PLAN G.6, ADR-0026), a step at a time: Swift makes each
// request and hands each answer back. Free with [`scaena_generator_free`].
typedef struct ScaenaGenerator ScaenaGenerator;

// The deck a session compiled last, taken to be laid out on every state for lint on a thread
// of its own (PLAN 4.13): [`scaena_laying_new`].
typedef struct ScaenaLaying ScaenaLaying;

// A save: every file of the bundle as saved, for the caller to write where it keeps the
// bundle, then to [adopt](scaena_adopt).
typedef struct ScaenaSaved ScaenaSaved;

// One bundle open for editing.
typedef struct ScaenaSession ScaenaSession;

// What a save reads of a session, taken in a moment to be saved on another thread while the
// session goes on (lift-up PERF-5): [`scaena_snapshot_new`].
typedef struct ScaenaSnapshot ScaenaSnapshot;

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
// it names, and every other file by its path; an older copy of a theme that ships takes its
// layout library (PLAN 3.31). Takes `files`, opened or not. Null where the bundle cannot be
// opened, `*error` then saying why (JSON `{"message"}`; free it).
//
// # Safety
// `files` is a live handle from [`scaena_files_new`]; `error` is null or writable.
struct ScaenaSession *scaena_open(struct ScaenaFiles *files, char **error);

// Open a `.scaena` zip, its `len` bytes, as [`scaena_open`] opens a folder. Null where it cannot
// be opened, `*error` then saying why.
//
// # Safety
// `bytes` is `len` readable bytes; `error` is null or writable.
struct ScaenaSession *scaena_open_zip(const uint8_t *bytes, size_t len, char **error);

// A new bundle (PLAN 2.12, 3.3), as New makes one in the browser and `deck_create` does: the
// theme that ships as `theme` (`dusk`, `daybreak`, `ember`, or `salon`), the fonts it names,
// and one state with nothing on it, titled `title`. Kept nowhere until it is saved. Null where
// no such theme ships, `*error` then saying why.
//
// # Safety
// `theme` and `title` are NUL-terminated strings; `error` is null or writable.
struct ScaenaSession *scaena_create(const char *theme, const char *title, char **error);

// Close a session.
//
// # Safety
// `session` is a live handle from [`scaena_open`] or [`scaena_open_zip`], or null.
void scaena_session_free(struct ScaenaSession *session);

// The deck `session` compiled last, taken to be linted on every state away from the session
// (PLAN 4.13): [`scaena_laying_lay`] lays it out on whatever thread calls it, while `session`
// answers calls on its own thread, and [`scaena_lint_laid`] hands what it found back. Null
// where no source compiled to a deck that validates. Free with [`scaena_laying_free`].
//
// # Safety
// `session` is a live handle.
struct ScaenaLaying *scaena_laying_new(struct ScaenaSession *session);

// Lay `laying`'s deck out on every state and run lint's layout rules on it, with an engine of
// its own: the slow part of `lint`, on the thread that calls it, which touches no session.
// False where `laying` is null or the engine stopped.
//
// # Safety
// `laying` is a live handle from [`scaena_laying_new`], used by one thread at a time.
bool scaena_laying_lay(struct ScaenaLaying *laying);

// What lint finds on every state of the deck `session` compiled last, as `lint` with no state
// says it, from what `laying` found, laying nothing out, dates judged at `now` (RFC 3339, or
// null for none), as `lint` judges them: `{"ok": {"linting"}}`, `linting` null where the deck
// changed after `laying` was taken from it, or it was not laid out.
//
// # Safety
// `session` and `laying` are live handles, on the session's thread; `now` a NUL-terminated
// string, or null.
char *scaena_lint_laid(struct ScaenaSession *session, struct ScaenaLaying *laying, const char *now);

// The narrative health of `laying`'s deck (PLAN G.9), as `health` gives the session's:
// `{"ok": health}`. Read on the thread that calls it, which touches no session, so the Story
// panel's reading of a deck as it is typed leaves the window's thread free (lift-up narrative
// PERF-1). It lays nothing out, and may be read before or after [`scaena_laying_lay`].
//
// # Safety
// `laying` is a live handle from [`scaena_laying_new`], used by one thread at a time.
char *scaena_laying_health(struct ScaenaLaying *laying);

// `state` at rest in `format`, one of the deck's formats, or its own canvas where null, from
// `laying`'s deck, painted by the CPU painter `width` pixels wide, the height keeping the canvas's
// aspect: the pixels [`scaena_pixels`] gives at rest, with nothing dragged or previewed, drawn
// on the thread that calls it, which touches no session, so the Mac's thumbnails and a remote's
// slides are drawn while the window answers (lift-up PERF-2). The engine it draws with is
// built at the first and kept for the next. Null bytes where it cannot be painted, `*error`
// then saying why.
//
// # Safety
// `laying` is a live handle from [`scaena_laying_new`], used by one thread at a time; `format`
// null or a NUL-terminated string; `state` one; `error` null or writable.
struct ScaenaPixels scaena_laying_pixels(struct ScaenaLaying *laying,
                                         const char *format,
                                         const char *state,
                                         uint32_t width,
                                         char **error);

// Whether `laying` was taken from the deck `session` holds as it stands: false once another
// source compiled, a file was handed over, or the theme was edited, so what it draws is of an
// older deck.
//
// # Safety
// `session` and `laying` are live handles, on the session's thread.
bool scaena_laying_holds(struct ScaenaSession *session, struct ScaenaLaying *laying);

// Free a laying.
//
// # Safety
// `laying` is a live handle from [`scaena_laying_new`], or null.
void scaena_laying_free(struct ScaenaLaying *laying);

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
// named by its SHA-256 as a save names it. A picture an image could not show, one too large
// among them ([`scaena_picture`] says so first), is refused and kept out of the bundle.
// `{"ok": path}` or `{"error": {"message"}}`, a string to free.
//
// # Safety
// `session` is a live handle; `name` a NUL-terminated string; `bytes` `len` readable bytes.
char *scaena_drop(struct ScaenaSession *session,
                  const char *name,
                  const uint8_t *bytes,
                  size_t len);

// What the PNG or JPEG `bytes` are as a picture (SPEC §3.3): `{"ok": {"width", "height",
// "largest"}}`, its size as it is seen and the longest side an image may have, read before it
// is dropped so one too large is offered as a smaller copy; `{"error": {"message"}}` where it
// is no picture an image shows. A string to free.
//
// # Safety
// `bytes` is `len` readable bytes.
char *scaena_picture(const uint8_t *bytes, size_t len);

// Answer `method` with `args` (a JSON object, or null for none), as a page's `Player` does:
// `{"ok": value}` or `{"error": {"message"}}`, a string to free. The calls ([`call`]):
// `states`, `formats`, `sizes` (`{own, anew}`: the deck's own canvas named by its shape, and the
// formats it lays out anew), `setFormat {format?}`, `canvasSize`, `duration {state}`, `timeline`,
// `files`, `imageFiles`, `digest {state}`, `reading {state}`, `reads {state}` (each node read,
// in turn: its role and its words, PLAN 3.17), `notes {state}` (what a presenter reads: the
// state's own, else its first beat's, SPEC §2.6, `{text, blocks}`, the blocks lines, a list, or
// a table), `source`, `compiledFrom
// {source}`, `compile {source}`, `lint {state?}`, `fix {patch}`, `inspect {state}`, `boxes
// {state}`, `hit {state, x, y}`, `layers {state}`, `carets {state, node}`, `choices {state,
// node}`, `stateChoices {state}`, `words {state}` (its objects by what they show, and the
// theme's names said, PLAN 2.99), `inserts`, `themes`, `themeText`, `keepHistory`,
// `keepsHistory`, and `writeFiles {files: [{path, text}]}`, which writes files back as an undo
// has them (`text` null: taken out). A drag (PLAN 3.7, ADR-0013): `targets {state, node}`,
// where a node may go; `snap {state, node, how, x, y, w, h, fork?, reach?}`, where a box left
// there lands and the patch that puts it there; `setMoving {nodes, dx, dy}`, the nodes drawn
// moved in frames at rest, laying nothing out; `preview {ops?}`, frames at rest drawn as a
// patch would make them; and `reach {ops}`, the states a patch changes. Text typed in place
// (PLAN 3.9): `typed {ops, at?}`, `replace_text`, `style_text`, and `list` ops by the user,
// validated but not linted, and whether the deck changed; and `bolding` and `italicizing
// {state, node, from, to}`, the look ⌘B and ⌘I give characters; and `codeWords {state, node}`,
// a code text's words as the state shows them, which Copy Code copies, and `tidied {text,
// language}`, code pasted into code, tidied (PLAN 2.133). A text's
// characters (PLAN
// 3.10): `characterChoices {state, node, from, to}`, what an inspector offers for them; and
// `linkAt {state, x, y}`, the link drawn there at rest, where a click goes. Nodes added and
// taken away (PLAN 3.11): `inserting {state, n, x, y, named?, with?}`, the patch that inserts
// what `inserts` offers `n`th about a point, or in the room nearest it, `with` properties of its
// own set on it; `duplicating {state, node}`,
// a copy beside it; each `{id, cell, patch}`; `duplicatingAll {state, nodes}`, a copy of each
// and of the connectors that join two of them, `{ids, patch}`; and `deleting {state, nodes |
// node, everywhere?}`, the ops that take them out of the state and those after, or out of the
// deck, a connector going with an end that goes. The clipboard (PLAN
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
// files}`, the theme file written before and after for the undo; `codeColors`, each kind of code
// token as the theme draws it (PLAN 2.144); `bundleFiles` and `removeFile
// {path}`; `dataSources`, `dataSheet {name}`, `dataEdit {source, edits, at?}`, `{result,
// wrote}`, and `dataUndo {redo?, at?}`, the file an edit wrote or a removal took out put back or
// written again; and, from the bundle's history, `versions`, each with what its change did
// and who made it in a person's words (`said`, `by`), `viewVersion {version}`, its states, and
// `versionSlides`, each as its slide list numbers it, `compareVersions {from, to?}`, and
// `restoreVersion {version, at?}`, `{restored, files}`. Handles and views (PLAN 3.16):
// `outline {state, node}`, a shape's points and corner;
// `framing {state, node}`, an image's crop and focal point; `focalAt {state, node, x, y}`, the
// point of an image under a press; `grid`, the theme's tracks and baselines; `setView {view?}`,
// the part of the canvas frames are painted through; and `find {query}` and `replacing {query,
// with, one?}`, the deck's texts found and the patch that replaces them. The layouts a state may
// take (PLAN 2.92, 3.26): `layoutsBegin {state}`, how many are to be judged; `layoutsStep`, the
// next judged, and whether any is left; and `layoutSuggestions {height}`, those judged, best
// first, each with the `set_state` that gives it and its picture's size, painted `height`
// pixels high, which [`scaena_layout_pixels`] takes. A slide's background (PLAN 2.148, 3.32):
// `backgroundsPainted {state, height}`, `{first, held?, options}`, None and each of the theme's
// shader presets, each `{preset?, kind?, label, said?, current?, patch, width, height}`, the slide
// drawn with it `height` pixels high, which [`scaena_background_pixels`] paints at a time; and
// `backgroundPatch {state, preset?}`, the patch one chosen makes, each text too faint over it set in
// a color that reads there. The slides a person may start (PLAN 3.30):
// `starters`, the slides a person may start as a picker lists them before any is drawn, each
// `{layout, name, description, group}`, then a blank one; `startersPainted {state, height,
// layouts?}`, one in each of the theme's layouts, then a blank
// one, or those `layouts` names, a null the blank one, each `{layout, name, description, group,
// width, height}`, painted `height` pixels high, which
// [`scaena_starter_pixels`] takes; `starting {state, layout?}`, the patch that starts one after
// the slide of `state`, `{id, patch}`; `titling {state}`, the patch that makes a new deck's
// title slide after it, its places for a picture given the theme's example, `{id, patch}`;
// `waiting {state}`, the slots of its layout that wait
// for what their prompts say goes there, each `{slot, rect, words, typed, role?}`; and
// `filling {state, slot}`, the patch that fills one that waits for words as a new slide fills
// it, `{id, patch}`; and `prompted {state, node}`, whether a text still reads as its slot's
// prompt, which typing selects whole. Writing (PLAN 2.101): `writing {kind, state, node?,
// finding?}`, what to ask a model, `{task, title, instructions, prompt}`; and `written {ask,
// answer}`, an answer as it is offered and the patch that takes it, `{text, patch}`.
//
// # Safety
// `session` is a live handle; `method` a NUL-terminated string; `args` one, or null.
char *scaena_call(struct ScaenaSession *session,
                  const char *method,
                  const char *args);

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

// `state` at `t_ms` (infinity: at rest) in `format`, one of the deck's formats, or its own canvas
// where null, painted by the CPU painter `height` pixels high, the width keeping the format's
// aspect: what the editor paints beside the canvas, a format at a time (PLAN 2.62, 3.16). Each
// format lays each state out once. Null bytes where it cannot be painted, `*error` then saying why.
//
// # Safety
// `session` is a live handle; `format` null or a NUL-terminated string; `state` one; `error`
// null or writable.
struct ScaenaPixels scaena_pixels_in(struct ScaenaSession *session,
                                     const char *format,
                                     const char *state,
                                     double t_ms,
                                     uint32_t height,
                                     char **error);

// The `i`th picture `layoutSuggestions` painted last (PLAN 2.92, 3.26), taken: the state laid out
// in that layout, at rest, as [`scaena_pixels`] gives a frame. Null bytes for one taken already,
// or past the last, `*error` then saying why.
//
// # Safety
// `session` is a live handle; `error` null or writable.
struct ScaenaPixels scaena_layout_pixels(struct ScaenaSession *session, size_t i, char **error);

// The `i`th background `backgroundsPainted` drew last (PLAN 2.148, 3.32): the slide with it, at
// rest, its effect's clock at `t` seconds, as [`scaena_pixels`] gives a frame. Null bytes past the last,
// `*error` then saying why.
//
// # Safety
// `session` is a live handle; `error` null or writable.
struct ScaenaPixels scaena_background_pixels(struct ScaenaSession *session,
                                             size_t i,
                                             double t,
                                             char **error);

// The `i`th picture `startersPainted` painted last (PLAN 3.30), taken: a slide started in that
// layout, at rest, as [`scaena_pixels`] gives a frame. Null bytes for one taken already, or past
// the last, `*error` then saying why.
//
// # Safety
// `session` is a live handle; `error` null or writable.
struct ScaenaPixels scaena_starter_pixels(struct ScaenaSession *session, size_t i, char **error);

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

// What a save of `session` reads, taken now to be saved away from the session's thread (lift-up
// PERF-5): [`scaena_snapshot_save`] saves it on whatever thread holds it, and
// [`scaena_adopt_since`] goes on from the save. Free with [`scaena_snapshot_free`].
//
// # Safety
// `session` is a live handle.
struct ScaenaSnapshot *scaena_snapshot_new(struct ScaenaSession *session);

// Save `snapshot` at `now` as [`scaena_save`] saves the session: fonts subset if `subset`, and
// the edits since the last save recorded, on the thread that calls it, which touches no session.
// Null where it cannot be saved, `*error` then saying why.
//
// # Safety
// `snapshot` is a live handle from [`scaena_snapshot_new`], used by one thread at a time; `now` a
// NUL-terminated string; `error` null or writable.
struct ScaenaSaved *scaena_snapshot_save(struct ScaenaSnapshot *snapshot,
                                         const char *now,
                                         bool subset,
                                         char **error);

// A session of its own on `snapshot`'s deck and files, to run a call that only reads on the thread
// that holds it (narrative lift-up PERF-2, [`scaena_chat_run_apart`]): it answers as the session
// it was taken from did. Null where it cannot be opened, `*error` then saying why. Free with
// [`scaena_session_free`].
//
// # Safety
// `snapshot` is a live handle from [`scaena_snapshot_new`]; `error` null or writable.
struct ScaenaSession *scaena_snapshot_open(const struct ScaenaSnapshot *snapshot, char **error);

// Let go of a snapshot.
//
// # Safety
// `snapshot` is a live handle from [`scaena_snapshot_new`], or null.
void scaena_snapshot_free(struct ScaenaSnapshot *snapshot);

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

// Go on from `saved`, a save of `snapshot`, once the caller has written it where it keeps the
// bundle, as [`scaena_adopt`] does, where the session is still as `snapshot` was taken (lift-up
// PERF-5): its files are taken out of `saved`, which holds none after. Where the session changed
// since, it goes on as it is and adopts nothing. `{"ok": {"adopted": bool}}` or `{"error"}`.
//
// # Safety
// `session` is a live handle, on its own thread; `snapshot` one from [`scaena_snapshot_new`];
// `saved` one from [`scaena_snapshot_save`].
char *scaena_adopt_since(struct ScaenaSession *session,
                         const struct ScaenaSnapshot *snapshot,
                         struct ScaenaSaved *saved);

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

// The adapter that paints the surface, and what paints on it (`vello`, or `cpu` where the GPU runs no
// vello), as JSON: `{"ok": {"name", "backend", "device", "painter"}}`.
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
// painter that wide, as the editor's PNG export paints it; or `slides`, `args` `{width, name}`,
// every slide once its steps have played, as PNGs that wide in one zip, each `{name} - Slide
// 3.png` (PLAN 2.122); or `version`, the same as `png` of the version `viewVersion` shows (PLAN
// 3.15). Null bytes where it cannot be made, `*error` then saying why.
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
//   model's name, at `at` (RFC 3339): `{id, name, error, summary, made, json, png?, edited,
//   rewritten}`, `made` whether it did what its words say. A `call` `{id, name, args}` may be
//   given in its place.
// - `took {index, ran}`: the last answer's call `index`, run apart on a snapshot's session by
//   [`scaena_chat_run_apart`], taken as `run` takes what it ran: `ran` back.
// - `next`: the calls' results handed to the model, every call answered (those not run, as
//   stopped): whether it may be asked again, or the question has taken its rounds.
// - `use {provider, model, base?}`: another model from the next request on, the conversation
//   kept. `shorter`: the provider found the conversation too long; whether to ask again with
//   half as much of tools' results, once a question (lift-up PERF-3). `forget`: the next
//   question is the first. `conversation`: the conversation as kept.
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

// Whether the call `name` with `args` (JSON, or null) only reads the deck, and may run apart from
// the session on a snapshot's ([`scaena_snapshot_open`], [`scaena_chat_run_apart`], narrative
// lift-up PERF-2): `deck_render`, `deck_health`, and `deck_lint` without `fix`.
//
// # Safety
// `name` is a NUL-terminated string; `args` one, or null.
bool scaena_chat_reads_alone(const char *name, const char *args);

// Run `call` (`{id, name, args}`, a call of a conversation's last answer) on `session`, a
// snapshot's ([`scaena_snapshot_open`]), by `agent:` and `model`, at `at` (RFC 3339, or null), on
// the thread that calls it (narrative lift-up PERF-2): `{"ok": ran}`, what the conversation's
// `run` gives, for its `took` on the conversation's thread; or `{"error"}`. Only for a call that
// reads alone ([`scaena_chat_reads_alone`]).
//
// # Safety
// `session` is a live handle, used by this thread alone; `model` and `call` NUL-terminated
// strings; `at` one, or null.
char *scaena_chat_run_apart(struct ScaenaSession *session,
                            const char *model,
                            const char *call,
                            const char *at);

// Free a conversation.
//
// # Safety
// `chat` is a handle this library made and has not freed, or null.
void scaena_chat_free(struct ScaenaChat *chat);

// A new generation, with no source read yet.
struct ScaenaGenerator *scaena_generator_new(void);

// What a brief may ask, as `{"ok": value}`: Narrato's goals, lengths, and ways a deck is
// delivered (`{id, label}`), the audiences its personas name, its arcs, and the defaults; and the
// themes that ship, as `shippedThemes` lists them, to make the deck in.
char *scaena_generator_choices(void);

// One step of the generation `generator`, as `{"ok": value}` or `{"error": {"message"}}`, with
// `bytes` where the step reads some. Each request a step gives is `{method, url, headers, body?,
// most?}`, the body the text to send:
//
// - `add {name}` with the file's bytes: read by its extension, `{name, kind}`.
// - `fetch {spec, contact?, token?}`: what a link names, planned as GETs, `{get}`: SEC's
//   filings with the user's `contact`, a GitHub repository with their `token` where it is
//   private, a Google file shared by link, or a page. `fetched {status, kind}` with the answer's
//   bytes: `{get}`, the next, or `{read: {name, kind}}`.
// - `research {brief, provider, model, base?, key}`: deep research on the brief's topic,
//   `{send}`; `researched {key, status, statusText?, body}`: `{send}`, `{wait, send}` (milliseconds
//   to wait first), or `{done: {sources, cut}}`, the report then a source; each with `progress`.
// - `begin {brief, provider, model, base?, arc?}`: the sources packed for the brief, `{given}`.
// - `request {key}`: `{step, slides, request}`, or `{step: "done"}`; `answer {status,
//   statusText?, body}`: `{step, slides, arc?, fallback?, written?, truncated?}`. `written
//   {text}`: slides written elsewhere, with no call.
// - `imageModels {provider, key, base?}`: the request that lists the models a key can use,
//   `{request}`; `imageModelsRead {provider, status, statusText?, body}`: the image models it
//   lists, best first, `{models}` (PLAN G.10).
// - `pictures {theme, look, provider, model, base?, quality?}`: the pictures the slides ask for
//   in the theme that ships as `theme`, as `illustration` or `photo`, by that image model,
//   `{pictures}`. Then each round: `paint {key}`, `{requests}`, every picture not yet made, none
//   once each is made or given up; `painted {status, statusText?, after?}` with each answer's
//   body as its bytes, in order; and `round`, `{wait}`, the seconds to wait before the next.
//   `finish`: `{pictures, unmade}`, the pictures kept for `make`.
// - `make {theme, key?}`: the deck built in the theme that ships as `theme`, each slide with a
//   picture made shown with it, and made a bundle, `{fit}`: with a key, the request that asks
//   for each text that runs past its place shorter (`{request, texts}`), or null; `fitted
//   {status, statusText?, body}`: `{fitted}` or `{unfitted}`.
// - `report`: what was made, as `scaena generate --json` says it.
//
// # Safety
// `generator` is a live handle; `method` a NUL-terminated string; `args` one, or null; `bytes`
// `len` readable bytes, or null with `len` 0.
char *scaena_generator_call(struct ScaenaGenerator *generator,
                            const char *method,
                            const char *args,
                            const uint8_t *bytes,
                            size_t len);

// The bundle the generation made, each sheet given kept as a data source, opened as a session:
// kept nowhere until it is saved. Null where it would not validate, or nothing is made yet,
// `*error` then saying why.
//
// # Safety
// `generator` is a live handle; `error` null or writable.
struct ScaenaSession *scaena_generator_open(struct ScaenaGenerator *generator, char **error);

// Free a generation.
//
// # Safety
// `generator` is a handle this library made and has not freed, or null.
void scaena_generator_free(struct ScaenaGenerator *generator);

// The models' providers, as `{"ok": value}` or `{"error": {"message"}}`: `list`, each provider
// `{id, name, base}`; `models {provider, key, base?}`, the request that lists the models a key
// can use, as `request` gives one; `readModels {provider, status, statusText?, body}`, the
// models its answer lists; and, for a writing tool (PLAN 2.101), `once {provider, model, key,
// base?, instructions, prompt}`, the request that asks a model once with nothing to call, and
// `answered {provider, status, statusText?, body}`, the words of its answer.
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
