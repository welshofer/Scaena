#ifndef SCAENA_H
#define SCAENA_H

/* Made by cbindgen from crates/scaena-ffi (`just bless`): edit the Rust, not this. */

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

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

// Answer `method` with `args` (a JSON object, or null for none), as a page's `Player` does:
// `{"ok": value}` or `{"error": {"message"}}`, a string to free. The calls ([`call`]):
// `states`, `formats`, `setFormat {format?}`, `canvasSize`, `duration {state}`, `timeline`,
// `files`, `imageFiles`, `digest {state}`, `reading {state}`, `source`, `compiledFrom
// {source}`, `compile {source}`, `lint {state?}`, `fix {patch}`, `inspect {state}`, `boxes
// {state}`, `hit {state, x, y}`, `layers {state}`, `choices {state, node}`, `stateChoices
// {state}`, `inserts`, `themes`, `themeText`, `keepHistory`, `keepsHistory`.
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

// Paint on `layer`, a `CAMetalLayer`, `width` × `height` device pixels, with the adapter that
// presents to it; frames are presented at the display's refresh. The layer keeps the deck's
// aspect, and outlives the surface. Null where none can be made, as on any machine but a Mac,
// `*error` then saying why.
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
