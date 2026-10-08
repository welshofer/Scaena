// The C ABI from C, as Swift sees it through `scaena.h` (PLAN 3.1): a bundle opened from its
// files, its states and timeline, a frame's display list and pixels, an MCP operation, a call
// it does not answer, and a save. `just ffi` builds the library and runs it:
//
//   ffi-smoke BUNDLE_DIR PATH...   (each PATH a file of the bundle, from BUNDLE_DIR)
//
// It prints a line a step and exits 0 when every step does what it should.

#include "scaena.h"

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int failures = 0;

static void check(int ok, const char *what) {
  printf("%s %s\n", ok ? "ok  " : "FAIL", what);
  if (!ok) failures++;
}

// The file at `dir`/`path`, read whole into a buffer the caller frees; NULL where it cannot be.
static unsigned char *slurp(const char *dir, const char *path, size_t *len) {
  char full[4096];
  snprintf(full, sizeof full, "%s/%s", dir, path);
  FILE *f = fopen(full, "rb");
  if (!f) return NULL;
  fseek(f, 0, SEEK_END);
  long size = ftell(f);
  fseek(f, 0, SEEK_SET);
  unsigned char *bytes = malloc(size > 0 ? (size_t)size : 1);
  *len = fread(bytes, 1, (size_t)size, f);
  fclose(f);
  return bytes;
}

// The JSON a call returned, printed after `what`, and whether it is an `ok`.
static int answered(char *json, const char *what) {
  int ok = json && strncmp(json, "{\"ok\":", 6) == 0;
  printf("     %s: %.160s%s\n", what, json ? json : "(null)", json && strlen(json) > 160 ? "…" : "");
  scaena_string_free(json);
  return ok;
}

int main(int argc, char **argv) {
  if (argc < 3) {
    fprintf(stderr, "ffi-smoke BUNDLE_DIR PATH...\n");
    return 2;
  }
  ScaenaFiles *files = scaena_files_new();
  for (int i = 2; i < argc; i++) {
    size_t len = 0;
    unsigned char *bytes = slurp(argv[1], argv[i], &len);
    if (!bytes) {
      fprintf(stderr, "cannot read %s/%s\n", argv[1], argv[i]);
      return 2;
    }
    scaena_files_add(files, argv[i], bytes, len);
    free(bytes);
  }
  char *error = NULL;
  ScaenaSession *session = scaena_open(files, &error);
  check(session != NULL, "the bundle opens from its files");
  if (!session) {
    printf("     %s\n", error);
    scaena_string_free(error);
    return 1;
  }

  check(answered(scaena_call(session, "states", NULL), "states"), "its states");
  check(answered(scaena_call(session, "timeline", NULL), "timeline"), "its timeline");

  // The first state, from the timeline's JSON: `{"ok":[{"state":"…"`.
  char *timeline = scaena_call(session, "timeline", NULL);
  char state[256] = {0};
  const char *at = strstr(timeline, "\"state\":\"");
  if (at) sscanf(at + 9, "%255[^\"]", state);
  scaena_string_free(timeline);
  check(state[0] != 0, "the first state named");

  ScaenaBytes frame = scaena_frame(session, state, INFINITY, &error);
  check(frame.data != NULL && frame.len > 0, "its display list, at rest");
  printf("     %zu bytes\n", frame.len);
  scaena_bytes_free(frame);

  ScaenaPixels pixels = scaena_pixels(session, state, INFINITY, 160, &error);
  check(pixels.bytes.data != NULL && pixels.width == 160 && pixels.bytes.len == (size_t)pixels.width * pixels.height * 4,
        "painted 160 pixels wide");
  printf("     %u x %u\n", pixels.width, pixels.height);
  scaena_bytes_free(pixels.bytes);

  check(answered(scaena_tool(session, "deck_lint", "{}", "user", NULL), "deck_lint"), "an MCP operation on the bundle");
  check(!answered(scaena_call(session, "nonsense", NULL), "nonsense"), "a call it does not answer is an error");

  ScaenaSaved *saved = scaena_save(session, "2026-10-08T12:00:00Z", false, &error);
  check(saved != NULL, "a save");
  if (saved) {
    check(answered(scaena_saved_list(saved), "saved"), "what the save holds");
    ScaenaBytes deck = scaena_saved_file(saved, "deck.json");
    check(deck.data != NULL && deck.len > 0, "the saved deck.json");
    scaena_bytes_free(deck);
    scaena_saved_free(saved);
  } else {
    printf("     %s\n", error);
    scaena_string_free(error);
  }

  scaena_session_free(session);
  printf("%s\n", failures ? "the C ABI failed" : "the C ABI opens, answers, draws, and saves a bundle");
  return failures ? 1 : 0;
}
