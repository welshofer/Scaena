# Lift-Up Plan: Scaena

> Platform: mixed — web (Vite + TS player/editor over a WASM engine), macOS (SwiftUI over a C ABI), iPadOS (the Mac app's sources), and the Rust core, CLI, and MCP server behind all three
> Surveyed: 2026-10-31
> Baseline: claude/modest-tesla-bednfm@d392d2761 — up to date with origin/main (0 ahead, 0 behind after `git fetch origin`); clean tree; no drift: `git rev-parse --short HEAD` read d392d2761 again before write-up
> Scope: full repo
> Coverage: full — eight read-only survey agents mapped core/pixels, engine/paint/export, store/ops/session/mcp/cli/serve, the AI crates, the web editor/player, the web assistant and generator, the Mac app, and iPad/FFI/WASM/CI; every item below was then re-read and grepped inline
> Attractiveness anchor: inferred — Keynote (the Mac and iPad window, the player's chrome), Pitch / Figma Slides (the web player's controls), Cursor (the assistant panel)

Previous plans (20261025, 20261025b, 20261025c) audited the narrative and AI surfaces closely; this plan audits the whole product. Prior open items were re-verified: where one is still open and still in a top five it is marked *(open since 10-25c)*; the rest are in Deferred.

## Performance

### PERF-1. The video export throws a fresh `CpuPainter` away every two frames
- **Location:** `crates/scaena-export/src/video.rs:448`, `:355`
- **Proof:**
  ```rust
  fn batch(&self) -> usize {
      match self {
          Painting::Cpu(threads) => threads * 2,
  ...
              s.spawn(|| {
                  // Frames are already painted on every core, so a frame's shaders take one.
                  let mut painter = CpuPainter { threads: 1, ..CpuPainter::default() };
  ```
- **Verified:** `grep -n -E 'CpuPainter' crates/scaena-export/src/video.rs` → `28:use scaena_paint::cpu::CpuPainter;` / `448:                    let mut painter = CpuPainter { threads: 1, ..CpuPainter::default() };` — one construction, inside the per-batch `paint()`; `grep -n -E 'struct Kept|pub mod cpu' crates/scaena-paint/src/lib.rs` → `889:pub mod cpu {` / `943:    pub struct Kept {`; `grep -n -E 'enum Painting|Cpu\(' crates/scaena-export/src/video.rs` → `306:enum Painting {` / `308:    Cpu(usize),`.
- **Do:** Make `Painting::Cpu` hold `Vec<CpuPainter>` (new field), one per thread, built once in `Painting`'s constructor at `:329`, and hand `painters.iter_mut()` into the scoped spawns in `paint()` so each worker keeps its `Kept` render context and glyph cache across batches. The same `CpuPainter::default()`-per-op pattern in `svg.rs:490` and `pdf.rs:852` (shadows, shaders, color glyphs) should keep one painter on their `Cx`.
- **Why:** A batch is `threads * 2` frames, so every worker rebuilds its tile buffers and re-outlines every glyph about every two frames — in the one path (a 60 fps movie) where painter reuse pays most.
- **Effort:** S · **Impact:** M

### PERF-2. `BundleFonts::reach` allocates a key for every glyph of every text, and its caches never shrink
- **Location:** `crates/scaena-engine/src/fonts.rs:228-229`, `:242`; called from `text.rs:2205`
- **Proof:**
  ```rust
  pub fn reach(&mut self, font: &FontData, coords: &[i16], gid: u32) -> Option<(f32, f32)> {
      let key = (font.data.id(), font.index, coords.to_vec(), gid);
      if let Some(&r) = self.reaches.get(&key) {
  ...
      self.reaches.insert(key, reach);
  ```
- **Verified:** `grep -n -E 'reaches|drawable|SPANS_KEPT|\.clear\(\)' crates/scaena-engine/src/fonts.rs` → `55:    drawable: HashMap<(u64, u32, Vec<i16>), HashSet<u32>>,` / `58:    reaches: HashMap<GlyphKey, (f32, f32)>,` / `72:            drawable: HashMap::new(),` / `73:            reaches: HashMap::new(),` / `194:        let done = self.drawable.get(&key);` / `219:        self.drawable.entry(key).or_default().extend(todo);` / `229:        if let Some(&r) = self.reaches.get(&key) {` / `242:        self.reaches.insert(key, reach);` — no `clear`, no cap; `grep -n -E '\.reach\(' crates/scaena-engine/src/text.rs` → `2205:                if let Some((up, down)) = fonts.reach(run.font(), run.normalized_coords(), g.id) {`.
- **Do:** Key `reaches` as `drawable` is keyed, `HashMap<(u64, u32, Vec<i16>), HashMap<u32, (f32, f32)>>`: look the per-instance map up once per run in `text.rs` (a `reaches_of(font, coords)` (new) returning `&mut HashMap<u32, (f32, f32)>`) and index it by glyph id, so no `to_vec` per glyph. Cap both maps (drop them past a count, as a long editor session with animated or dragged axes grows them without bound).
- **Why:** Every layout of every text heap-allocates once per glyph just to consult a cache; on the editor's worker that is the hottest per-keystroke path after shaping.
- **Effort:** S · **Impact:** M

### PERF-3. The worker mirrors every file of the bundle after every message, on the player's pages too
- **Location:** `web/src/worker.ts:133-146`, `:944-947`
- **Proof:**
  ```ts
  function mirrorFiles() {
    if (!player || stopped !== undefined) return;
    const edition = player.filesEdition();
    if (mirrored?.of === player && mirrored.edition === edition) return;
    const next = new Map<string, Uint8Array>();
    for (const path of player.files()) {
      const bytes = player.file(path);
      if (bytes) next.set(path, bytes);
    }
    mirror = next;
    mirrored = { of: player, edition };
  }
  ```
- **Verified:** `grep -n -E 'recoverable|onCrashed|mirrorFiles|filesEdition' web/src/worker.ts web/src/stage.ts web/src/editor.ts` → `web/src/worker.ts:133:function mirrorFiles() {` / `web/src/worker.ts:135:  const edition = player.filesEdition();` / `web/src/worker.ts:946:      mirrorFiles();` / `web/src/stage.ts:256:  onCrashed: (crash: ...` / `web/src/stage.ts:1570:        this.onCrashed({ ... });` / `web/src/editor.ts:583:  stage.onCrashed = (crash) => void restarted(crash);` — only the editor sets `onCrashed`, nothing gates the mirror; `grep -n -E 'type: "open"' web/src/protocol.ts` → `1037:  | { type: "open"; source: Source; painter: Painter; canvas: OffscreenCanvas; engine?: WebAssembly.Module }`.
- **Do:** Add `recoverable?: boolean` (new) to the `open` message in `protocol.ts`, set only by the editor's `Stage.open`, and return from `mirrorFiles()` unless it was set. Then mirror incrementally: keep the old `mirror` entries and re-copy only the paths an edit `written`/`removed` (expose a per-path edition from `Session`, or compare `player.files()` against the kept keys and bytes' lengths first).
- **Why:** A theme tweak or a data edit copies every image and font twice (Rust `to_vec`, then wasm-bindgen into the JS heap); the player and the single file pay for a crash copy nothing ever reads.
- **Effort:** S · **Impact:** M

### PERF-4. Every save in the browser rewrites every file of the bundle
- **Location:** `web/src/worker.ts:1449-1453`
- **Proof:**
  ```ts
    const paths = saved.paths();
    const last = ["deck.json", "manifest.json"];
    for (const path of [...paths.filter((p) => !last.includes(p)), ...last.filter((p) => paths.includes(p))])
      await home.write(path, saved.file(path)!);
  ```
- **Verified:** `grep -n -E 'savedDigest|sha256|unchanged' web/src/worker.ts` → `# (no matches)`; `grep -n -E 'sha256' web/src/recovery.ts` → `38:  return sha256(new TextEncoder().encode(key));` / `42:export async function sha256(bytes: ArrayBuffer | Uint8Array<ArrayBuffer>): Promise<string> {`; `grep -n -E 'interface Home|write\(path' web/src/worker.ts` → `1095:interface Home {` / `1097:  write(path: string, bytes: Uint8Array): Promise<void>;`.
- **Do:** Keep a `savedDigest: Map<string, string>` (new) in the worker, filled by `readAll`, `fetched`, and each `save` from `recovery.ts`'s `sha256`, and skip `home.write(path, …)` when the digest of `saved.file(path)` matches. Clear an entry when `home.remove` runs or the home changes.
- **Why:** Each ⌘S rewrites every image, font, and history file through OPFS `createWritable` or one HTTP PUT per file under `scaena serve`, which makes saves slow on real decks and widens the window a closed tab leaves a half-written folder in.
- **Effort:** M · **Impact:** M

### PERF-5. The Mac's inspector paints every background preset and every layout suggestion on the main actor after each edit *(open since 10-25c)*
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/Inspector.swift:1181-1186`
- **Proof:**
  ```swift
          .task(id: "\(state)\u{1f}\(editor.revision)\u{1f}\(editor.format ?? "")") {
              try? await Task.sleep(for: .milliseconds(150))
              guard !Task.isCancelled, let drawn = try? editor.session.backgroundsDrawn(state: state, height: Self.painted)
              else { return }
              offered = drawn.0
              images = drawn.1
          }
  ```
- **Verified:** `grep -n -E 'offMain|snapshot\(\)|OffMain' apps/mac/ScaenaKit/Sources/Scaena/Inspector.swift` → `# (no matches)`; `grep -rn -E 'func apart|func backgroundsDrawn|func layoutsDrawn|final class OffMain' apps/mac/ScaenaKit/Sources` → `ScaenaKit/Backgrounds.swift:83:    public func backgroundsDrawn(state: String, height: Int) throws -> (Backgrounds, [CGImage?]) {` / `ScaenaKit/Layouts.swift:80:    public func layoutsDrawn(height: Int) throws -> [(suggestion: LayoutSuggestion, image: CGImage)] {` / `ScaenaKit/Assistant.swift:308:    private func apart(_ index: Int, call: ToolCall, chat: ScaenaChat) async throws -> Ran {` / `ScaenaKit/DeckEditor.swift:600:final class OffMain: Sendable {`.
- **Do:** Take `let snap = try editor.session.snapshot()` on the main actor and run `backgroundsDrawn` and `layoutsDrawn` on a session of it off main, as `Assistant.apart` does, through a public `DeckEditor.offMain(_:)` (new) wrapper over `OffMain`; drop the result when `editor.revision` moved meanwhile. The layouts row (`judged()` at `:1047-1056`) takes the same path.
- **Why:** Seven shader presets and a dozen layouts are laid out and CPU-painted on the main actor 150 ms after every keystroke, which is where the Mac window stutters while typing.
- **Effort:** M · **Impact:** M

## Functionality

### FUNC-1. SVG export drops every link, every alt text, and every layer label the PDF carries
- **Location:** `crates/scaena-export/src/svg.rs:228-229`, `:139-143`
- **Proof:**
  ```rust
                  Op::Image { asset, src, dst, quality } => self.image(asset, *src, *dst, *quality)?,
                  // A link's area draws nothing (PLAN 2.70).
                  Op::Link { .. } => {}
  ```
- **Verified:** `grep -n -E 'links\(\)|<a ' crates/scaena-export/src/svg.rs` → `# (no matches)`; `grep -n 'links()' crates/scaena-export/src/pdf.rs` → `255:        for link in dl.links() {`; `grep -n -E 'pub fn links|pub struct LinkArea|enum LinkTarget' crates/scaena-core/src/displaylist.rs` → `186:pub enum LinkTarget {` / `194:pub struct LinkArea {` / `537:    pub fn links(&self) -> Vec<LinkArea> {`; `grep -n 'fn escape' crates/scaena-export/src/svg.rs` → `688:fn escape(s: &str) -> String {`; `grep -n -E 'label: Option' crates/scaena-core/src/displaylist.rs` → `101:        label: Option<String>,`.
- **Do:** After the ops, walk `list.links()` and write, for each `LinkArea` with a `LinkTarget::Url`, `<a href="…"><polygon points="…" fill="transparent"/></a>` from its corners, the href through `escape()`; skip or `#state-id` a `LinkTarget::State`. Write `role="img" aria-label="…"` on a `<g>` whose `Op::Layer` has a `label`, and put `role="img"` and `aria-labelledby` of the `<title>` on the root. Add an optional `readings` (new) to `SvgSettings`, the same `Reading`s `pdf.rs` tags figures from, so an image's `alt` reaches its `<g>`.
- **Why:** A deck exported to SVG for the web silently loses its links and reads as nothing to a screen reader, while the PDF of the same deck carries both.
- **Effort:** S · **Impact:** M

### FUNC-2. The browser's Export dialog has no movie, though the engine, the CLI, and MCP export mp4, webm, and ProRes
- **Location:** `web/src/protocol.ts:56-60`; `web/editor.html` `#exporting`
- **Proof:**
  ```ts
  export type Export =
    | { kind: "png"; ... } | { kind: "slides"; ... } | { kind: "pdf"; ... } | { kind: "html"; ... };
  ```
- **Verified:** `grep -n -E 'Vp9|Prores' crates/scaena-export/src/video.rs` → `45:    Vp9,` / `55:            Codec::Vp9 => "webm",` / `533:        Codec::Vp9 => (`; `grep -n -E 'kind: "(video|mp4|webm|movie)"' web/src/protocol.ts web/src/editor.ts` → `# (no matches)`.
- **Do:** Add `{ kind: "video"; fps: number; codec: "webm" | "mp4" }` (new) to `Export`: the worker samples the slots at the deck's pace with the CPU painter as `thumbnails` already does, and encodes with WebCodecs `VideoEncoder` plus a small WebM (or MP4) muxer; where `VideoEncoder` is absent, say so in the dialog. Fall back to a `.zip` of frames at `fps` where encoding is not available.
- **Why:** The deck's whole point is its motion, and in the browser — the only client most people will use — the deck cannot leave as a movie.
- **Effort:** L · **Impact:** L

### FUNC-3. The Mac's Export To offers PDF and Images alone; the Notes PDF, the movie, and the single HTML file are reachable only elsewhere or not at all
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/Menus.swift:62-68`
- **Proof:**
  ```swift
          CommandGroup(replacing: .importExport) {
              Menu("Export To") {
                  Button("PDF…") { panes?.savePDF?() }
                      .disabled(panes?.savePDF == nil)
                  Button("Images…") { panes?.saveImages?() }
                      .disabled(panes?.saveImages == nil)
              }
          }
  ```
- **Verified:** `grep -n -E 'Movie|HTML|Notes' apps/mac/ScaenaKit/Sources/Scaena/Menus.swift` → `# (no matches)`; the Notes PDF exists only under the toolbar's Share (`DeckView.swift:615-616`, `Exporting.Kind.notes`); `grep -n -E 'Vp9|Prores' crates/scaena-export/src/video.rs` → as above.
- **Do:** Add "PDF with Notes…", "Movie…", and "HTML…" to the menu, each a new `Exporting.Kind` case (`movie`, `html`) backed by `scaena_export`'s formats through a snapshot's session (`Snapshot.exported`), the movie piped to `ffmpeg` only where one is found on the PATH, else disabled with a tooltip saying so. Give `saveImages` an accessory for scale (1×, 2×) in place of "a pixel to the unit".
- **Why:** Keynote's Export To has Movie, HTML, and Images at a size; a Mac user cannot find what the engine already does.
- **Effort:** M · **Impact:** M

### FUNC-4. The Mac's assistant never re-reads the Keychain after Settings saves a key
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/AssistantPanel.swift:49`, `:57`
- **Proof:**
  ```swift
          .task(id: "\(provider.rawValue)\u{1f}\(keysChanged)") { await list() }
  ...
          #if !os(macOS)
          .sheet(isPresented: $keying, onDismiss: { keysChanged += 1 }) {
  ```
- **Verified:** `grep -n -E 'appearsActive|keysChanged' apps/mac/ScaenaKit/Sources/Scaena/AssistantPanel.swift apps/mac/ScaenaKit/Sources/Scaena/PromptWindow.swift` → `AssistantPanel.swift:29:    @State private var keysChanged = 0` / `AssistantPanel.swift:49:        .task(id: "\(provider.rawValue)\u{1f}\(keysChanged)") { await list() }` / `AssistantPanel.swift:57:        .sheet(isPresented: $keying, onDismiss: { keysChanged += 1 }) {` / `PromptWindow.swift:184:    @Environment(\.appearsActive) private var appearsActive` / `PromptWindow.swift:526:        .task(id: appearsActive) { readKeys() }` — the panel bumps `keysChanged` only from the iPad's sheet; the Mac opens Settings instead.
- **Do:** Add `@Environment(\.appearsActive)` to `AssistantPanel` and fold it into the task id, as `PromptWindow` does at `:526`, so coming back from Settings lists the models again; or post a notification from `KeysSettings` after `Keychain.set` and observe it in both.
- **Why:** On the Mac the panel says "Add a Key…", the person pastes one in Settings, returns, and Ask stays disabled with no models until the panel is reopened.
- **Effort:** S · **Impact:** M

### FUNC-5. E102's "add the font" fix can never apply on a deck that lists no fonts — the case it exists for
- **Location:** `crates/scaena-core/src/validate.rs:924`; `crates/scaena-core/src/document.rs:49-50`; `crates/scaena-ops/src/lint.rs:280-287`
- **Proof:**
  ```rust
                          .fix(vec![json!({ "op": "add", "path": "/fonts/-", "value": font })]),
  ...
      #[serde(default, skip_serializing_if = "Vec::is_empty")]
      pub fonts: Vec<FontRef>,
  ...
      let mut doc = b.deck.to_value()?;
      ...
          let mut next = doc.clone();
          if scaena_core::patch::apply(&mut next, patch).is_ok() {
  ```
- **Verified:** `grep -n '"/fonts"' crates/scaena-core/src/validate.rs` → `922:                            .at("/fonts")` — no fix writes `/fonts` whole; `grep -n -E 'pub fn apply\(' crates/scaena-core/src/patch/mod.rs` → `821:pub fn apply(doc: &mut Value, ops: &[Value]) -> Result<(), PatchError> {`; RFC 6902 `add` to `/fonts/-` fails when `/fonts` is absent, and `lint --fix` skips a fix that fails without a word.
- **Do:** When `deck.fonts.is_empty()`, emit `{"op":"add","path":"/fonts","value":[<every missing font>]}` on each such finding (applying it twice is harmless: `add` replaces the member). Add a fixture under `tests/lint/E102/` whose deck has no `fonts` key, and a `--fix` test over it.
- **Why:** A fresh deck whose theme names families is exactly the deck this fix is for, and today `lint --fix` leaves it broken and silent.
- **Effort:** S · **Impact:** M

## Stability

### STAB-1. The assistant's tool calls run through the C ABI without the deep stack every other call gets
- **Location:** `crates/scaena-ffi/src/chat.rs:288`; compare `lib.rs:727`
- **Proof:**
  ```rust
          let author = format!("agent:{model}");
          match guarded(|| s.tool(&call.name, args, Caller { author: &author, at }).map_err(|e| failure(&e))) {
              Ok(called) => {
  ```
- **Verified:** `grep -n -E 'deep\(|fn deep|stacker' crates/scaena-ffi/src/*.rs` → `lib.rs:120:fn deep<T>(f: impl FnOnce() -> T) -> T {` / `lib.rs:121:    stacker::maybe_grow(ROOM, DEEP, f)` / `lib.rs:427:        deep(|| laying.0.pixels(format, state, width)).map_err(said)` / `lib.rs:458:        deep(|| laying.0.cue(format, state)).map_err(said)` / `lib.rs:697:        deep(|| call(&mut session.0, method, &args))` / `lib.rs:727:        match deep(|| session.0.tool(name, args, Caller { author, at })) {` — nothing in `chat.rs`; `grep -n -E 'chat\.run\(' apps/mac/ScaenaKit/Sources/ScaenaKit/Assistant.swift` → `281:                            ran = try chat.run(index, on: editor.session)` (on the main actor); `grep -n -E 'const (DEEP|ROOM)' crates/scaena-ffi/src/lib.rs` → `113:const ROOM: usize = 2 << 20;` / `114:const DEEP: usize = 16 << 20;`.
- **Do:** Wrap the call as `scaena_tool` does: `deep(|| guarded(|| s.tool(…)))` in `chat::run`, with `deep` made `pub(crate)`. Give `scaena_laying_lay` (`lib.rs:356`) the same, since the header promises room "whichever thread Swift calls from".
- **Why:** A `deck_patch` that re-themes, or a `deck_lint` with `fix`, lays the deck out before and after on the iPad's 1 MB main-thread stack; a stack overflow is not a panic `catch_unwind` catches, so the app dies (the same cause as #533).
- **Effort:** S · **Impact:** L

### STAB-2. A shader whose `kind` starts with a non-ASCII letter panics the inspector
- **Location:** `crates/scaena-core/src/choices.rs:506-507`
- **Proof:**
  ```rust
      let Some(kind) = shown.get("kind").and_then(Value::as_str).filter(|k| !k.is_empty()) else { return Vec::new() };
      let def = format!("{}{}Params", kind[..1].to_ascii_uppercase(), &kind[1..]);
  ```
- **Verified:** `grep -n 'is_ascii' crates/scaena-core/src/choices.rs` → `# (no matches)`; a node's props are untyped, `Session::new` opens a deck validation would flag, and `inspect --choices` does not validate first, so `"kind": "ñoise"` reaches this slice at byte 1, inside a two-byte character.
- **Do:** Replace the slice with a check against the six kinds `scaena_core::shader` knows (return `Vec::new()` for any other), or at least add `.filter(|k| k.is_ascii())` to the existing filter. Add the case to `crates/scaena-session/tests/edits.rs`'s random props.
- **Why:** In the browser a panic stops the editor's worker; CLAUDE.md names that a bug, not a style call.
- **Effort:** S · **Impact:** M

### STAB-3. W322's fixes remove motions by index, so `lint --fix` with two idle targets removes the wrong one
- **Location:** `crates/scaena-core/src/lint/document.rs:534`, `:545`; applied in order at `crates/scaena-ops/src/lint.rs:282-288`
- **Proof:**
  ```rust
          return vec![serde_json::json!({ "op": "remove", "path": format!("{pointer}/target/{k}") })];
  ...
      vec![serde_json::json!({ "op": "remove", "path": format!("{base}/{}", steps.join("/")) })]
  ```
- **Verified:** `grep -n '"test"' crates/scaena-core/src/lint/document.rs` → `# (no matches)`; `ls tests/lint/W322/` → `clean.deck.json  trigger.deck.json` (one idle target); `grep -n -E 'JsonOp::Test' crates/scaena-core/src/patch/mod.rs` → `861:        JsonOp::Test { path, value } => match doc.pointer(path) {`.
- **Do:** Put a `{"op":"test","path":…,"value":<the target id or the item>}` before each `remove`, so a fix whose index has shifted fails and is skipped by `lint --fix`'s loop rather than removing a neighbour; or emit one fix per state that removes indices highest-first. Add a W322 fixture with two idle targets in one `target` array and one with two idle choreography items.
- **Why:** With `target: ["a","b","c"]` and `a`, `b` idle, fix 1 removes `/target/0` and fix 2 removes `/target/1`, which is now `c` — a motion that was fine goes, silently.
- **Effort:** S · **Impact:** M

### STAB-4. After Stop, a failure, or New conversation, the next answer lands in the old entry — or in a detached one
- **Location:** `web/src/assistant/panel.ts:284-287`, `:340-344`, `:245-250`
- **Proof:**
  ```ts
        case "saying": {
          saying ??= entry("said");
          saying.append(event.text);
  ...
        case "done":
          if (event.stop !== "end" && event.stop !== "tools") entry("end", STOPS[event.stop] ?? STOPS.other);
          return;
        case "failed":
          return void entry("failed", event.message);
      }
    }
  ```
- **Verified:** `grep -n -E 'saying' web/src/assistant/panel.ts` → `281:  let saying: HTMLElement | undefined;` / `285:        saying ??= entry("said");` / `286:        saying.append(event.text);` / `287:        saying.scrollIntoView({ block: "end" });` / `293:        saying?.remove();` / `294:        saying = undefined;` / `302:        const li = saying ?? entry("said");` / `303:        saying = undefined;` — cleared only in `waiting` and `text`; `#new`'s handler (`:245-250`) calls `transcript.replaceChildren()` and never touches it; `grep -n -E 'async function ask\(' web/src/assistant/panel.ts` → `397:  async function ask(text: string): Promise<AssistantEvent> {`.
- **Do:** Set `saying = undefined` in the `done` and `failed` cases, at the top of `ask()`, and in the `#new` handler; disable `#new` while a question is working. In `converse.ts`, when a turn is stopped or fails before any assistant message, pop the orphaned user message from `o.conversation`, else the next question is merged with it.
- **Why:** The next question's streamed words go into the half-written entry above it, and after New conversation into an element no longer in the page, so the answer never shows at all.
- **Effort:** S · **Impact:** M

### STAB-5. Per-bundle listeners on the page's permanent `#look` pile up each time a deck is opened, and old editors keep running
- **Location:** `web/src/backgrounds.ts:209-231`; wired from `web/src/look.ts:313-314`
- **Proof:**
  ```ts
    function listen(into: HTMLElement) {
      into.addEventListener("keydown", keys);
      into.addEventListener("click", (e) => {
        const on = option(e.target);
        if (on) void choose(Number(on.dataset.background));
      });
  ...
      still.addEventListener("change", tick);
      document.addEventListener("visibilitychange", tick);
    }
  ```
- **Verified:** `grep -n -E 'import.*life|heed|listen\(' web/src/backgrounds.ts` → `209:  function listen(into: HTMLElement) {` — no `life.ts`; `grep -n -E 'backgrounds\(|\.attach\(|listen\(' web/src/look.ts` → `313:  const backdrops = backgrounds(stage, around);` / `314:  backdrops.listen(into);` / `874:  for (const panel of panels) listen(panel, "change", (e) => {` … `975:  listen(into, "pointerover", (e) => {` — `look.ts` itself uses `life.ts`'s `listen` for its own handlers on the same element; `grep -n -E 'export function listen|export function ended|observing' web/src/life.ts` → `10:export function listen<K extends keyof HTMLElementEventMap>(` / `24:export function observing<T extends MutationObserver | ResizeObserver>(observer: T): T {` / `30:export function ended() {`.
- **Do:** Switch `backgrounds.listen` to `life.ts`'s `listen` (which `ended()` drops with the bundle), widen `observing()` to take an `IntersectionObserver` and wrap `sight` in it, and do the same for `#keep-claim`/`#restate` (`editor.ts:1052-1053`), `#desk`'s pointerdown (`canvas.ts:3999`), and `rehearse.ts:172`'s document keydown (call `rehearsing.stop()` from `current.close`).
- **Why:** Every New or Open adds another set of handlers that call `choose()` and `preview()` on a terminated worker's Stage, whose promises never settle, while the old editor writes to the shared status line and stays reachable.
- **Effort:** S · **Impact:** M

## Reliability

### REL-1. A dash pattern of zeros, or of tiny lengths, hangs every painter and the PDF
- **Location:** `crates/scaena-paint/src/lib.rs:810-814`; `crates/scaena-export/src/pdf.rs:692`
- **Proof:**
  ```rust
          Stroke::new(f64::from(width))
              .with_caps(cap)
              .with_join(join)
              .with_miter_limit(f64::from(miter_limit))
              .with_dashes(f64::from(dash_offset), dash.iter().map(|d| f64::from(*d)))
      }
  ```
- **Verified:** `grep -n -E 'dash' crates/scaena-paint/src/lib.rs` → `799:    pub fn stroke(width: f32, cap: Cap, join: Join, miter_limit: f32, dash: &[f32], dash_offset: f32) -> Stroke {` / `814:            .with_dashes(f64::from(dash_offset), dash.iter().map(|d| f64::from(*d)))` / `1092:…Op::Stroke { … dash, dash_offset } => {` / `1094:                        self.ctx.set_stroke(stroke(*width, *cap, *join, *miter_limit, dash, *dash_offset));` / `1676:…Op::Stroke {…}` / `1677:                        let style = stroke(*width, *cap, *join, *miter_limit, dash, *dash_offset);` / `1911:                dash: vec![],` / `1912:                dash_offset: 0.0,` — no guard; the SVG writer alone has one: `sed -n 208p crates/scaena-export/src/svg.rs` → `if !dash.is_empty() && dash.iter().all(|d| *d >= 0.0) && dash.iter().any(|d| *d > 0.0) {`; `grep -n 'Layout(' crates/scaena-engine/src/error.rs` → `16:    Layout(String),`; `grep -n -E 'pub fn stroke\(' crates/scaena-engine/src/shapes.rs` → `# (no matches)` (the engine's stroke is built at `shapes.rs:452` from `s.dash.clone()`).
- **Do:** In `scaena_paint::stroke()`, drop the dashes when their period (`dash.iter().sum()`) is not positive or is under about `0.05 * width.max(1.0)`; refuse the same in the engine where `shapes.rs:452` builds the op (an `EngineError::Layout` naming the node), and in `pdf.rs:692`'s `StrokeDash`. Add a torture case and a lint fixture for `dash: [0, 0]`.
- **Why:** The model's `dash: Vec<NonNegative>` admits `[0, 0]` and `[1e-6, 1e-6]`; kurbo expands one segment per period, so a 1920-unit line with a 2e-6 period is about a billion segments — the browser's worker, the Mac's layer, and the PDF export all hang or run out of memory on one untrusted deck.
- **Effort:** S · **Impact:** M

### REL-2. Kept folders are keyed by their bare name, so a reload or a recovery copy can land on a different folder of the same name
- **Location:** `web/src/editor.ts:416`, `:2190`, `:627-633`
- **Proof:**
  ```ts
      await folders("readwrite", (store) => store.put(handle, handle.name));
  ...
        await folders("readwrite", (store) => store.put(to.folder, to.folder.name));
  ```
- **Verified:** `grep -n -E 'randomUUID|isSameEntry|db\.result\.close|function folders|function copyKey|function address' web/src/editor.ts web/src/folders.ts` → `web/src/editor.ts:211:function folders<T>(mode: IDBTransactionMode, act: (store: IDBObjectStore) => IDBRequest<T>): Promise<T> {` / `web/src/editor.ts:284:function address(where: Where | undefined) {` — no UUID, no `isSameEntry`, no `close`; `grep -n -E 'const copyKey' web/src/editor.ts` → `633:  const copyKey = () => {` (keyed `${where.kind}:${where.name}`); `grep -n -E 'async function discard' web/src/recovery.ts` → `99:export async function discard(key: string) {`.
- **Do:** Store each handle under a `crypto.randomUUID()` key (reusing an existing key only where `handle.isSameEntry(old)` is true), put that id in `address()` (`folder:<id>`) and in `copyKey()`, and close the database in `folders()` (`db.result.close()`) so each call stops leaving a connection open.
- **Why:** Two folders called `talk` overwrite each other in IndexedDB: `?bundle=folder:talk` reopens the other, and a recovery copy made in folder A is offered on folder B — and `recover()` would write A's source and files into B — or is silently discarded because B saved later.
- **Effort:** S · **Impact:** M

### REL-3. A save can race an in-flight recovery copy and leave a stale "not saved" copy behind
- **Location:** `web/src/editor.ts:2199-2216`; compare `:3124`
- **Proof:**
  ```ts
      const was = copyKey();
      clearTimeout(copyTimer);
      copied = new Map();
  ...
      void Promise.all([discard(was), discard(copyKey())]).catch(() => {});
  ```
- **Verified:** `grep -n -E 'let copying|copying =' web/src/editor.ts` → `645:  const copying: Promise<void> = Promise.resolve();` / `667:    copying = copying.then(run, run).catch((e: unknown) => console.warn(\`The recovery copy was not kept: ${said(e)}\`));`; `sed -n 3124p web/src/editor.ts` → `gone = copying.then(() => discard(key)).catch(() => {});` — `close()` chains the discard behind `copying`; `took()` does not.
- **Do:** In `took()`, chain as `close()` does: `copying = copying.then(() => Promise.all([discard(was), discard(copyKey())])).catch(() => {})`, so a `keepCopy` already awaiting `stage.file()` cannot write `copy.json` after the discard.
- **Why:** When it loses the race, `copy.at` is newer than the save and `copy.files` is not empty, so the next open says "Changes … were not saved" for work that was saved, and Recover writes stale files back over it.
- **Effort:** S · **Impact:** M

### REL-4. `save` renames an image file but not an image `src` set in `deck.overrides`, so the picture breaks after saving
- **Location:** `crates/scaena-store/src/save.rs:199-206`
- **Proof:**
  ```rust
          for (_, node) in deck.nodes.iter_mut().filter(|(id, _)| images.contains(*id)) {
              sources.extend(node.props.get_mut("src"));
          }
          for state in &mut deck.states {
              for (_, delta) in state.props.iter_mut().filter(|(id, _)| images.contains(*id)) {
                  sources.extend(delta.get_mut("src"));
              }
          }
  ```
- **Verified:** `grep -n 'overrides' crates/scaena-store/src/save.rs` → `# (no matches)`; `grep -n -A3 'fn image_files' crates/scaena-core/src/document.rs` → `397:    pub fn image_files(&self) -> Vec<String> {` / `398-        let images = |id: &String| self.nodes.get(id).is_some_and(|n| n.node_type == NodeType::Image);` … — the deck counts an override's `src` as a file the bundle holds, and `saving_with` (`save.rs:143`) renames the file to `assets/<sha>` and lists the old path in `replaced`.
- **Do:** Extend `sources` with `deck.overrides.iter_mut().filter(|(id, _)| images.contains(*id)).filter_map(|(_, p)| p.get_mut("src"))` beside the nodes' and states'. Add a save test whose override sets an image's `src`, checking the file and the override agree after an in-place save.
- **Why:** An in-place `write_dir` deletes the old file while the override still names it: `scaena save`, the browser's ⌘S, and the Mac's autosave each turn that image into an empty box with E107.
- **Effort:** S · **Impact:** M

### REL-5. The writing tools take a cut-off or refused answer as whole, in the browser and on the Mac
- **Location:** `web/src/assistant/index.ts:125-136`; `crates/scaena-chat/src/lib.rs:147-149`
- **Proof:**
  ```ts
  export async function write(writing: Writing, signal?: AbortSignal): Promise<{ text: string; usage?: { input: number; output: number } }> {
    const turn = await providers[writing.provider].turn({
  ...
    return { text: turn.text, usage: turn.usage };
  }
  ```
  ```rust
  pub fn answered(provider: Provider, status: u16, status_text: &str, body: &str) -> Result<String, ChatError> {
      Ok(provider.read_turn(status, status_text, body)?.text)
  }
  ```
- **Verified:** `grep -n -E 'stop' web/src/assistant/index.ts` (the `write` function) → no read of `turn.stop`; `grep -n -E 'function turn\(' web/src/assistant/converse.ts` → `72:async function turn(o: Options, messages: Message[]): Promise<Turn> {` (the retrying wrapper, not exported, not used by `write`); `sed -n 408,412p crates/scaena-chat/src/providers.rs` → `Some("tool_use") => Stop::Tools, Some("end_turn") => Stop::End, Some("max_tokens") => Stop::Length, _ => Stop::Other,`; `grep -n -E 'Refus|content_filter|SAFETY' crates/scaena-chat/src/providers.rs crates/scaena-chat/src/lib.rs` → `# (no matches)` (narrato's `providers.rs:349-383` maps all three providers' refusals; chat's does not).
- **Do:** Return `stop` from `write` and from `answered` (an `Err(ChatError::Provider("the answer ran out of room: ask again, or pick a larger model"))` for `Stop::Length`), and in `writing.ts`'s `asking()` and `slide-ask.ts` keep Use it hidden and say so. Export `converse.ts`'s `turn` and call it from `write` so a 429 is asked again as a question is. Add `Stop::Refused` (new) to both the Rust `Stop` and the TS `Turn.stop`, mapped from Anthropic `refusal`, OpenAI `content_filter`, and Gemini `SAFETY`/`RECITATION`/`promptFeedback.blockReason`, reusing narrato's matching.
- **Why:** A Rewrite or Draft Notes cut off by `max_tokens` replaces the slide's words with a fragment, and a refusal reads as "The model stopped." with nothing to act on.
- **Effort:** S · **Impact:** M

## Security

### SEC-1. The web loader fetches any URL a deck names — another origin included — and sends the serve token with it
- **Location:** `web/src/worker.ts:1177-1182`
- **Proof:**
  ```ts
  async function fetched(deck: string, serve?: Serving): Promise<Player> {
    const headers: Record<string, string> = serve ? { "X-Scaena-Token": serve.token } : {};
    const get = async (path: string) => {
      const response = await fetch(new URL(path, deck), { headers });
      if (!response.ok) throw new Error(`${path}: HTTP ${response.status}`);
      return response;
    };
  ```
- **Verified:** `grep -n -E 'origin|startsWith\("http|new URL\(path' web/src/worker.ts` → `1115:    const url = new URL(path.split("/").map(encodeURIComponent).join("/"), deck);` / `1179:    const response = await fetch(new URL(path, deck), { headers });` — no origin check anywhere in the file; the paths come from `deck.json`'s `theme`, `fonts[].file`, data `source`, `imageFiles()`, and `manifest.json`'s `files`, each unchecked; the store refuses them natively: `grep -n -E '^(pub )?fn inside' crates/scaena-store/src/lib.rs` → `583:fn inside(root: &Path, rel: &str) -> Result<PathBuf, StoreError> {`.
- **Do:** In `get`, resolve `new URL(path, deck)` and refuse it when its `origin` differs from the deck's or its path is not under the deck's directory — the rule `scaena_store`'s `inside` applies — and attach `X-Scaena-Token` only to same-origin requests. Say which path was refused in the error. Add a `web/*.mjs` walk with a deck naming `https://` and `//host/` files.
- **Why:** Opening a deck from any URL beacons to a third party today, and under `scaena serve` a deck.json naming `"https://evil.example/x.png"` hands that host the token that writes the bundle.
- **Effort:** S · **Impact:** L

### SEC-2. A bare `deck.json` path turns off the store's link-out guard
- **Location:** `crates/scaena-store/src/lib.rs:417`, `:648`
- **Proof:**
  ```rust
          let root = path.parent().unwrap_or(Path::new(".")).to_path_buf();
  ...
  fn resolved(root: &Path, path: PathBuf, rel: &str) -> Result<PathBuf, StoreError> {
      let Ok(real_root) = std::fs::canonicalize(root) else { return Ok(path) };
  ```
- **Verified:** `grep -n -E 'is_empty\(\)|Path::new\("\."\)' crates/scaena-store/src/lib.rs` → `417:        let root = path.parent().unwrap_or(Path::new(".")).to_path_buf();` / `506:…` / `507:…` / `541:        let made = path.parent().filter(|p| !p.as_os_str().is_empty()).map_or(Ok(()), std::fs::create_dir_all);` / `619:    if rel.is_empty() || !normal {` — line 541 knows `parent()` of a bare name is `Some("")`, line 417 does not; `grep -n -E 'resolved\(' crates/scaena-store/src/lib.rs` → `604:            Err(e) if e.raw_os_error() == Some(libc::ELOOP) => read(resolved(root, path, rel)?),` / `637:            Ok(meta) if meta.file_type().is_symlink() => return resolved(root, path, rel),`.
- **Do:** In `locate`, treat an empty parent as `"."` (`Some(p) if !p.as_os_str().is_empty()`), and make `resolved` fail closed: when `canonicalize(root)` fails, return `Err(StoreError::LinkedOut(rel.to_string()))`. Add a test that opens `Path::new("deck.json")` with the current directory set, a linked asset leading out.
- **Why:** `scaena export deck.json`, `scaena save deck.json`, or an MCP call with `bundle: "deck.json"` follows a link out of the bundle and puts the outside file's bytes into a save or an HTML export, against the guard CLAUDE.md promises (`StoreError::LinkedOut`).
- **Effort:** S · **Impact:** M

### SEC-3. An existing folder inside a hidden folder becomes a root the MCP server reads and writes under
- **Location:** `crates/scaena-mcp/src/lib.rs:208-214`, `:243-248`
- **Proof:**
  ```rust
          let beside = std::fs::canonicalize(bundle).ok().and_then(|b| b.parent().map(Path::to_path_buf));
          let roots: Vec<PathBuf> = [Some(here.clone()), beside]
  ...
          let hidden = |below: &Path| below.components().any(|c| c.as_os_str().to_string_lossy().starts_with('.'));
          let root = roots.iter().find(|r| real.strip_prefix(r).is_ok_and(|below| !hidden(below)));
  ```
- **Verified:** `grep -n -E 'fn new_bundle' crates/scaena-mcp/src/lib.rs` → `182:    fn new_bundle(&self, bundle: &str) -> Result<PathBuf, OpsError> {` which is `self.within(bundle, bundle, "writes")` — the bundle supplies its own root; `grep -n -E '\.hidden|\.aws|\.git/' crates/scaena-mcp/tests/mcp.rs` → `814:…create_dir_all(here.join(".aws"))…` / `821:    let hidden = json!({ "bundle": bundle, "id": "aws", "file": path(&here.join(".aws/credentials.csv")) });` / `928:    for (bundle, tool) in [(root.join("outside"), "deck_create"), (here.join(".hidden/x"), "deck_create")]` — the test's `.hidden/x` does not yet exist, so `beside` is `None`; an existing `.git/branches` or `~/.config/<app>` as `bundle` makes its hidden parent a root, below which nothing is hidden.
- **Do:** In `held`, keep `beside` only where the canonical bundle path itself passes `here`'s rule (no dot-component below `here` or `home`); or require `bundle` to pass `self.within(bundle, "", …)` against `here` before it may supply a root. Add the existing-hidden-folder case to `mcp.rs:928`.
- **Why:** `deck_create` with `bundle: "<here>/.aws/empty"` then `data: [{file: "<here>/.aws/credentials.csv"}]` passes `readable()`, and the bundle is written in a hidden folder — the two things SPEC §7.2's sandbox exists to refuse.
- **Effort:** S · **Impact:** M

### SEC-4. A JSON Patch `copy` can grow the document exponentially before anything validates it
- **Location:** `crates/scaena-core/src/patch/mod.rs:857-860`
- **Proof:**
  ```rust
          JsonOp::Copy { from, path } => {
              let v = doc.pointer(from).cloned().ok_or_else(|| format!("`{from}` is not there"))?;
              add(doc, path, v)
          }
  ```
- **Verified:** `grep -n -E 'fn weight|budget|starts_with\(&format' crates/scaena-core/src/patch/mod.rs` → `845:            if path.starts_with(&format!("{from}/")) {` (the `Move` arm's guard; `Copy` has none, and no size budget exists); `grep -n -E 'pub fn apply\(|pub fn compile\(|pub fn compile_alone' crates/scaena-core/src/patch/mod.rs` → `760:pub fn compile(…)` / `792:pub fn compile_alone(…)` / `821:pub fn apply(doc: &mut Value, ops: &[Value]) -> Result<(), PatchError> {`.
- **Do:** Give `one()` a size budget: a recursive `weight(&Value) -> usize` (new); in `apply`, `compile`, and `compile_alone` set `budget = 4 * weight(doc) + 64 KiB`, charge each `Copy` and `Add` value against it, and fail with "the patch would grow the deck past …" when it is spent. Mirror `Move`'s self-containment check for `Copy` (and `from == ""`). Add a root-pointer `copy` case to `crates/scaena-session/tests/edits.rs`.
- **Why:** `copy /a → /a/x1`, `/a → /a/x2`, … doubles `/a` each op; thirty ops in one `deck_patch` or theme edit from a model exhaust memory and kill the editor's worker before validation runs.
- **Effort:** S · **Impact:** M

### SEC-5. CI runs pull-request code with a write token in `.git/config`, and `mac-app` imports the Developer ID on pull requests
- **Location:** `.github/workflows/ci.yml:133-134`, `:306-307`; `.github/workflows/mac-app.yml:10`, `:41-47`
- **Proof:**
  ```yaml
      # The Mac app's window is pushed to a branch of the pull request's own (scripts/shots.sh).
      permissions:
        contents: write
  ```
  ```yaml
    pull_request:
      paths:
        - .github/workflows/mac-app.yml
        - apps/mac/build-app.sh
  ...
            security create-keychain -p "$secret" "$keychain"
            security set-keychain-settings -lut 3600 "$keychain"
  ```
- **Verified:** `grep -n -E 'permissions:|contents:|persist-credentials|uses: actions/checkout' .github/workflows/ci.yml` → `43:    permissions:` / `44:      contents: read` / `53:      - uses: actions/checkout@v4` / `133:    permissions:` / `134:      contents: write` / `142:      - uses: actions/checkout@v4` / `306:    permissions:` / `307:      contents: write` / `310:      - uses: actions/checkout@v4` / `365:…` / `648:…` — no `persist-credentials: false`; `grep -n -E "event_name|if:|delete-keychain|always\(\)" .github/workflows/mac-app.yml` → `# (no matches)` beyond the `on:` block — the Developer ID step has no `if`, and no step deletes the keychain.
- **Do:** Give the `rust` and `ipad` jobs `contents: read` and `persist-credentials: false`, and move the push into a `shots` job (new) with `contents: write` that `needs` them, downloads their artifacts, and runs `scripts/shots.sh` with nothing of the PR's built. In `mac-app.yml`, add `if: github.event_name != 'pull_request'` to the Developer ID and notarization steps (PRs build ad hoc), pin `dtolnay/rust-toolchain` and `Swatinem/rust-cache` by commit SHA, and add an `if: always()` step that deletes the keychain and the `.p12`.
- **Why:** A same-repo pull request's `Cargo.lock`, `engine.sh`, or `build-app.sh` runs with a token that pushes to any branch protection does not cover, and on `mac-app` with an unlocked signing keychain for an hour.
- **Effort:** M · **Impact:** M

## Usability

### USE-1. The Mac and the iPad never speak progress, results, or failures to VoiceOver *(open since 10-25c)*
- **Location:** `apps/mac/ScaenaKit/Sources/ScaenaKit/Assistant.swift:257-258`, `:525`; `apps/mac/ScaenaKit/Sources/Scaena/PromptWindow.swift:858`
- **Proof:**
  ```swift
      private func add(_ kind: Entry.Kind) -> Int {
  ```
  ```swift
      private func say(_ words: String, failed: Bool = false) {
  ```
- **Verified:** `grep -rn -E 'AccessibilityNotification|NSAccessibility\.post|announcement' apps/mac/ScaenaKit/Sources` → `# (no matches)`; `grep -n -E 'func say\(|func add\(' apps/mac/ScaenaKit/Sources/Scaena/PromptWindow.swift apps/mac/ScaenaKit/Sources/ScaenaKit/Assistant.swift` → `PromptWindow.swift:858:    private func say(_ words: String, failed: Bool = false) {` / `Assistant.swift:525:    private func add(_ kind: Entry.Kind) -> Int {`.
- **Do:** Post `AccessibilityNotification.Announcement(text).post()` in `Assistant.add` for `.failed` and `.done` (and when a `.call` finishes, with `ToolWords.ran`), in `PromptView.say` (always for `failed:`, else only on a step change so progress is not a torrent), and when an export finishes or fails in `DeckView`. The browser's panel and dialog already use live regions; this brings the native clients level.
- **Why:** A VoiceOver user asks the assistant or generates a deck and hears nothing until they go looking — the browser tells them, the Mac does not.
- **Effort:** S · **Impact:** M

### USE-2. The Mac's Format menu lacks a presentation app's standard keys, and Lock sits on the wrong one
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/NodeCommands.swift:145-162`; `apps/mac/ScaenaKit/Sources/ScaenaKit/MenuKeys.swift:8`
- **Proof:**
  ```swift
  public enum MenuKey: CaseIterable, Sendable {
  ```
- **Verified:** `grep -n -E 'Underline|alignLeft|alignCenter|Bigger|Smaller' apps/mac/ScaenaKit/Sources/Scaena/NodeCommands.swift apps/mac/ScaenaKit/Sources/ScaenaKit/MenuKeys.swift` → `# (no matches)`; `grep -n -E '"align/x"' apps/mac/ScaenaKit/Sources/Scaena/Inspector.swift` → `263:            case "align/x":` / `973:…accessibilityLabel(Words.word(way.word, of: "align/x", type: "text"))` (alignment exists only as the inspector's `choose`); `grep -rn -E 'func chooseAll' apps/mac/ScaenaKit/Sources` → `Scaena/Inspector.swift:525:    private func chooseAll(_ prop: String, _ value: JSONValue, in state: String) {`; `grep -n -E 'style_text' apps/mac/ScaenaKit/Sources/ScaenaKit/Typing.swift` → `20:…one \`style_text\`…` / `587:            "op": "style_text", "node": .string(node), "state": .string(state),`.
- **Do:** Add `MenuKey` cases for Align Left/Center/Right (⌘{ ⌘| ⌘}) wired to `chooseAll("align/x", …)` (moved to a shared place both the inspector and the menu call), Bigger/Smaller (⌘+ ⌘−) as a `choose` of the text's size step, and Underline (⌘U) as a `style_text`; give Delete Slide and Duplicate Slide keys. Reconsider Lock: Keynote uses ⌘L (Lock) and ⌥⌘L (Unlock), where ScaenaKit puts Light Table on ⌥⌘L and Lock on ⇧⌘L.
- **Why:** The window is meant to feel like Keynote (PLAN 3.18), and a Keynote user's fingers find nothing under ⌘{, ⌘}, ⌘+, or ⌘L.
- **Effort:** M · **Impact:** M

### USE-3. The player's live status says every advance twice, and writes `title` and `aria-valuetext` every frame
- **Location:** `web/src/player.ts:424-431`
- **Proof:**
  ```ts
      const line = notice?.words ?? [place(slots, at.index), format(), at.playing && !screen ? "playing" : "", screen].filter(Boolean).join(" · ");
      // The status is a live region: it changes when the deck's place does, not every frame.
      if (status.textContent !== line) status.textContent = line;
      status.title = stage.opened.painter === "webgpu" ? "Painted by WebGPU" : "Painted by the CPU";
      if (statesPicker.value !== String(at.index)) statesPicker.value = String(at.index);
      scrub.value = String(scrubbed(at));
      scrub.setAttribute("aria-valuetext", slots[at.index].said ?? `${at.index + 1} of ${slots.length}`);
  ```
- **Verified:** `grep -n 'role="status"' web/index.html` → `17:      <span id="status" role="status">Loading…</span>`; the `· playing` part comes and goes with each cue, so the line changes twice per advance; `title` and `aria-valuetext` are set unconditionally while `statesPicker.value` is guarded two lines up.
- **Do:** Take "playing" out of the live text (keep it in `title` or a non-live span), and guard `status.title` and `scrub`'s `aria-valuetext` with the same "only if changed" test `statesPicker.value` has.
- **Why:** A screen reader hears "Slide 3 of 12 · playing" and then "Slide 3 of 12" on top of the `#reading` region, and the accessibility tree churns at 60 Hz.
- **Effort:** S · **Impact:** S

### USE-4. Menu buttons made by `opens()` never set `aria-expanded`, and a second click reopens the menu instead of closing it
- **Location:** `web/src/commands.ts:259-272`
- **Proof:**
  ```ts
  export function opens(button: HTMLButtonElement, items: () => Command[]) {
    const open = () => {
      const at = button.getBoundingClientRect();
      return menu(at.left, at.bottom + 2, items(), button);
    };
    button.setAttribute("aria-haspopup", "menu");
    button.onclick = open;
  ```
- **Verified:** `grep -n 'aria-expanded' web/src/commands.ts web/src/toolbar.ts web/src/editor.ts web/src/cue.ts` → `web/src/toolbar.ts:83:      opener.setAttribute("aria-expanded", "false");` / `web/src/toolbar.ts:92:      opener.setAttribute("aria-expanded", "true");` — the File menu sets it; Insert (`editor.ts:1686`) and the cue's Add (`cue.ts:445`) through `opens()` do not; `grep -n -E 'export function menu' web/src/commands.ts` → `193:export function menu(x: number, y: number, items: Command[], back: HTMLElement | null) {`.
- **Do:** In `opens`, set `aria-expanded="true"` on open and `"false"` when the menu closes — `menu()` needs an `onClose` callback (new) or to return its close promise — and make a click while open close it (ignore `menu()`'s capturing `pointerdown` when `e.target === back`, toggle in `onclick`).
- **Why:** The toolbar's CSS styles `[aria-expanded="true"]`, so these buttons never show open, assistive tech is not told, and the menu cannot be closed from its own button.
- **Effort:** S · **Impact:** S

### USE-5. The player's `?` keys list is a non-modal `show()` that never takes focus
- **Location:** `web/src/player.ts:280`
- **Proof:**
  ```ts
      key(key: string): boolean {
        if (key === "?") dialog.open ? dialog.close() : dialog.show();
        else if (key === "Escape" && dialog.open) dialog.close();
        else return false;
        return true;
      },
  ```
- **Verified:** `grep -n -E 'showModal|\.show\(\)' web/src/player.ts` → `280:      if (key === "?") dialog.open ? dialog.close() : dialog.show();` / `934:    dialog.showModal();` — the slide navigator uses `showModal`; the keys list does not.
- **Do:** Use `dialog.showModal()` and focus the `#keylist-title` heading (`tabindex="-1"`), letting the dialog return focus on close as the navigator at `:934` does.
- **Why:** `show()` leaves focus behind the dialog with no dialog semantics: Tab walks past it, and closing it restores nothing.
- **Effort:** S · **Impact:** S

## Attractiveness / Sexiness

### ATTR-1. The presenter's slide navigator stretches every slide to 16:9
- **Location:** `web/src/player.css:108`
- **Proof:**
  ```css
  .navigator li canvas { width: 100%; height: auto; aspect-ratio: 16 / 9; border-radius: 4px; background: #33312d; }
  ```
- **Verified:** `grep -rn 'aspect-ratio' web/src/player.css` → `97:.laser { … aspect-ratio: 1; … }` / `108:.navigator li canvas { width: 100%; height: auto; aspect-ratio: 16 / 9; … }` — no `auto` keyword; the editor avoids it with `--aspect` (`editor.html:319/346`) and `aspect-ratio: auto` for drawn canvases.
- **Do:** Write `aspect-ratio: auto 16 / 9` so the canvas's own ratio wins once it has one, or set `--aspect` from `shown.size` in the navigator's `build()` as the editor's strip does.
- **Why:** A 4:3, a square, or a 9:16 deck's slides are drawn distorted in the one view a presenter uses to find a slide mid-talk.
- **Effort:** S · **Impact:** S

### ATTR-2. The player's chrome is OS-default buttons with Unicode glyphs, and fullscreen hides every control
- **Location:** `web/index.html:10-22`; `web/src/player.css:23`
- **Proof:**
  ```html
      <button id="back" aria-label="Back" title="Back (←)">◀</button>
      <button id="on" aria-label="Next" title="Next (→ or Space)">▶</button>
  ...
      <button id="full" aria-label="Fullscreen" title="Fullscreen (F)">⛶</button>
  ```
  ```css
  :fullscreen .bar { display: none; }
  ```
- **Verified:** `grep -n -E ':fullscreen|\.bar button' web/src/player.css` → `23::fullscreen .bar { display: none; }` / `24::fullscreen body:not(.presenter) { background: #000; }` / `101::fullscreen .typed, .presenter .typed { bottom: 24px; }` — no rule styles the bar's buttons; `grep -n -E 'export function icon\(' web/src/toolbar.ts` → `40:export function icon(name: string): SVGSVGElement {` (the Phosphor set the editor's toolbar draws); `grep -n -E 'classList.*idle' web/src/player.ts` → `624:    document.body.classList.remove("idle");` / `627:        document.body.classList.add("idle");` (the two-second rest timer).
- **Do:** Draw the bar's buttons with `toolbar.ts`'s `icon()` from the shipped Phosphor set and give `.bar` one style (a dark translucent strip, the editor's tokens); in fullscreen show the bar as a floating overlay that the existing `idle` class hides after the pointer rests, as Keynote, Pitch, and Figma Slides do; replace "Loading…" over a blank canvas with a centred spinner until `Stage.open` resolves.
- **Why:** The player is the first thing a viewer of a shared deck sees, and today it looks like a form; a touch user in fullscreen has no visible way forward, back, or out.
- **Effort:** M · **Impact:** M

### ATTR-3. Unsaved changes are guarded by native `confirm()`s whose OK means "lose", with no Save
- **Location:** `web/src/editor.ts:264`, `:2948`, `:2974`, `:3177`
- **Proof:**
  ```ts
    if (lost && !asked && !confirm("The deck open is not saved. Open another and lose what is not saved?")) return;
  ```
- **Verified:** `grep -n 'confirm(' web/src/editor.ts` → `264:…` / `387:  if (!confirm(\`Delete “${name}” from this browser?…\`)) return;` / `2948:        if (confirm("Take the deck as it is on disk, and lose the changes here not saved?")) …` / `2974:      if (!confirm(\`${folder.name} is not empty. Save the bundle into it…\`)) return;` / `3177:    if (!fresh && !confirm("The deck has changes not saved, which the player would not play. Save it first?")) return;`; `grep -n -E 'id="(unsaved|recovering)"' web/editor.html` → `1234:    <dialog id="recovering" aria-labelledby="recovering-title">` — a styled three-way dialog exists for recovery, none for unsaved changes.
- **Do:** Add an `#unsaved` `<dialog>` (new) with Save, Don't Save, and Cancel as `value` buttons, as `#recovering` is built (`editor.ts:916`), await its `returnValue`, and run `save()` before `open()` on Save; use it at `:264`, `:2948`, and `:3177`.
- **Why:** Every presentation app offers Save at this moment; a browser `confirm` blocks the page, cannot be styled or walked by the `.mjs` tests, and makes the person choose between losing work and going back to save by hand.
- **Effort:** S · **Impact:** M

### ATTR-4. A morphing shadow mixes its color in sRGB while every other paint mixes in Oklab
- **Location:** `crates/scaena-engine/src/shadows.rs:116`
- **Proof:**
  ```rust
              (Some(a), Some(b)) => {
                  let [ca, cb] = [a.color.0, b.color.0];
                  let mix = |k: usize| lerp(f32::from(ca[k]), f32::from(cb[k])).round() as u8;
                  Some(Shadow {
                      offset: [lerp(a.offset[0], b.offset[0]), lerp(a.offset[1], b.offset[1])],
                      blur: lerp(a.blur, b.blur),
                      color: Color([mix(0), mix(1), mix(2), mix(3)]),
  ```
- **Verified:** `grep -n -E 'pub fn mix' crates/scaena-engine/src/sample.rs` → `3336:pub fn mix(a: Color, b: Color, p: f32) -> Color {` (Oklab, premultiplied, which shape paints, shader palettes, and chart colors use); `grep -n -E 'mix\(|oklab' crates/scaena-engine/src/shadows.rs` → only the local sRGB closure at `116`.
- **Do:** Replace the closure with `color: crate::sample::mix(a.color, b.color, p)` in `Shadow::between` (`shadows.rs:107`), and bless the shadow goldens the change moves.
- **Why:** A black shadow morphing to a tinted one goes muddy and grey midway, the one interpolation in the engine that does not match the others (SPEC §6).
- **Effort:** S · **Impact:** S

### ATTR-5. H.264 and VP9 exports smear coloured text on dark grounds
- **Location:** `crates/scaena-export/src/video.rs:530`, `:561`
- **Proof:**
  ```rust
              &["-c:v", "libx264", "-preset", "medium", "-crf", "18", "-movflags", "+faststart", "-f", "mp4"],
  ...
      let filter = format!("scale=out_color_matrix=bt709:out_range=tv,format={pixels}");
  ```
- **Verified:** `grep -n -E 'tune|lanczos|accurate_rnd|full_chroma' crates/scaena-export/src/video.rs` → `# (no matches)`.
- **Do:** Use `scale=…:flags=lanczos+accurate_rnd+full_chroma_int+full_chroma_inp` in the filter and add `-tune animation` (flat colour, hard edges) to the libx264 arguments; offer `VideoSettings::chroma444` (new) that writes `yuv444p` with `-profile:v high444` for a master.
- **Why:** Slides are flat fields and thin type; swscale's default bicubic 4:2:0 conversion without full-chroma interpolation bleeds red and blue text into their ground, which is the first thing a viewer sees in an exported movie.
- **Effort:** S · **Impact:** M

---

## First move

**SEC-1. The web loader fetches any URL a deck names — another origin included — and sends the serve token with it** (from Security)

It is the only item here where opening a file is enough to be exploited: a deck shared by URL, a `.scaena` dropped on the page, or a folder under `scaena serve` names its own files, and the worker fetches whatever they say — from any host, with the token that writes the bundle when served. The Rust store refuses exactly this (`inside`, `LinkedOut`), so the web loader is the one client out of step with CLAUDE.md's promise that a bundle is self-contained. The fix is a few lines in one function (`fetched`'s `get`), it costs no feature, a `.mjs` walk pins it, and it clears the table for the two store-side guards beside it (SEC-2, SEC-3), which close the same class of hole natively.

## Dropped during verification

- **`scaena serve`'s HTML pages carry no Content-Security-Policy** — already in place: `web/vite.config.ts:73-112` writes a meta CSP into every built page (`default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; … frame-src 'none'; base-uri 'none'`), which serve's gzipped pages carry; only `frame-ancestors` (header-only) is missing, noted in Deferred.
- **The web loader resolves paths without URL-encoding them** — cited code does something else: `worker.ts:1115` already encodes each path component (`path.split("/").map(encodeURIComponent).join("/")`) on the folder-read path; the origin gap is SEC-1.
- **`scaena_sources::Fetch::answer` does not enforce `most` itself** — cited as a gap, but `crates/scaena-objects/src/lib.rs::run` and the browser worker both enforce it before `answer`, and no caller passes an unbounded body; a defence-in-depth note at most.
- **The CPU painter rebuilds its render context every frame** — already in place: `scaena-paint/src/lib.rs:943 pub struct Kept` keeps the context and glyph cache between frames; only the video path (PERF-1) discards it.
- **MCP's `data_edit` with no `edits` waits on the write lock** — real but the `held_if` pattern (`mcp/lib.rs:483-494`) already exists for `deck_lint` and `deck_find`; a one-line sibling, not top-five, moved to Deferred.
- **Gradients default to no dither and band** — mislocated as a shipped-theme problem: `grep -c '"grain"' docs/examples/themes/dusk.theme.json` → `4`, the shipped presets do set grain; only the `Params::default()` at `gradient.rs:64` is 0, which a theme-less gradient alone sees.
- **`scaena save deck.json` fails with an OS error** — plausible from `save.rs:316-319` (`read_dir(to)` on a file), but not reproduced in this run; a wording fix if confirmed.
- **The JPEG decoder lets a 2 KB file cost ~100 full decodes** — `MAX_SCANS = 100` is real (`jpeg.rs:258`), but each scan is bounded by the side limit and `MAX_PIXELS`, and no measurement of the cost was made; kept in Deferred rather than claimed as a DoS.

## Deferred

- **`Reported::of` should stand in for the whole compiled patch** in `spine_update`, `background_set`, `state_start`, `image_add`, `map_fetch`, `slide_write`, and `deck_find --replace` (`mcp/lib.rs:1524`, `:1571`; `patch.rs:25`) *(open since 10-25c)*: the model pays for every op again.
- **MCP's write tools lint the "before" deck again** though `Lints` holds it by digest (`patch.rs:341`, `mcp/lib.rs:470-491`): pass the cached findings in.
- **`deck_export` asked again ignores `pace`, `notes`, and the data files** (`mcp/lib.rs:313`): hash `lint::digest(b)` and the two fields.
- **A streamed answer that ends without `message_stop`/`finish_reason` is taken as whole, and a half-received `tool_use` runs with `{}`** (`web/src/assistant/stream.ts:63-69`; `owing()` guards `length` alone).
- **A pasted clip's `files` paths are never checked** (`ops/clipboard.rs:475-494`, `session/lib.rs:2369`): a `../x.png` makes every later save fail; `deck.scn` lands in the folder.
- **The outline prompt and block are unfenced** (`narrato/outline.rs:56-82`, `:295`) *(open since 10-25c)*: Scaena's own prompt, so `scaena_fence` applies.
- **`research.rs` ends a 30-minute deep research on one 429** (`:277-289`): put the `Polling` state back and `Wait`.
- **A picture request with status 0 or 408 is skipped for good** (`pictures.rs:430`) *(open since 10-25c)*.
- **`fit::read` never checks a list kept its lines** (`fit.rs:76-83`), and **`same_figures` ignores sign and magnitude** (`narrato/lib.rs:64-70`): `figures_in` in `ops/narrative.rs:2869` already keeps both.
- **Health's "Make it part of" fix uses `replace` on a `beatName` that may not exist** (`health.rs:1152-1157`) *(open since 10-25c)*: `add` would do.
- **`slide_write` with `beat` and no `after` lands at the end** (`one.rs:113`) *(open since 10-25c)*.
- **The Story never coaches W422/W423** (`health.rs:1254`) *(open since 10-25c)*.
- **`icon_find`'s `limit` has no ceiling** (`mcp/lib.rs:1512`; `docs/schema/mcp/icon_find.json:27-31`) *(open since 10-25c)*.
- **`image_add`, `map_fetch`, and stock say a bare "no state"** where 21 siblings use `OpsError::unknown_state` (`pictures.rs:66`, `maps.rs:126`, `stock.rs:201`; `session/lib.rs:1471`, `writing.rs:151`, `:184`).
- **`data_attach` reads with no size bound** (`create.rs:56`) where `pictures::read` checks `metadata` first (`pictures.rs:130`).
- **Exports and the CLI's outputs are not written whole** (`ops/export.rs:406`; `cli/main.rs:846`, `:1048`, `:1192`, `:2343`): route through `write_whole`; and **`write_whole` never syncs the directory** after the rename (`store/lib.rs:519-524`).
- **Notes pages scale the slide to the page's width**, so a 9:16 deck's slide overruns its page (`engine/notes.rs:50-61`).
- **`tracking::lives` lacks the `k < j` guard its siblings have** (`tracking.rs:122` vs `:165`), and **`apply_state` finds entering nodes in quadratic time** (`:245-249`).
- **A chart coloured by a number gets no ramp legend** (`charts/compile.rs:764-771`).
- **The PDF re-splits a PNG's pixels on every page** it appears on and never downsamples (`pdf.rs:805-808`; `photos` caches JPEGs alone).
- **Video batches hold two batches of RGB frames** (`video.rs:355`): cap the batch by bytes.
- **Image adjustment quantizes twice** and bands at high contrast (`adjust.rs:96-101`).
- **The Mac's `Data(bytes:count:)` copies every frame and export once more** (`ScaenaSession.swift:299-300`, `:402-405`): `bytesNoCopy` with `scaena_bytes_free` as deallocator.
- **Mac: the assistant doesn't follow its answer as it streams** (`AssistantPanel.swift:162-164`) *(open since 10-25c)*; **each streamed line rebuilds the whole answer and calls `session.states()`** (`:301`, `:303`, `:348`).
- **Mac: a whole-deck delete is simulated on the main actor after each edit to title a menu item** (`DeckView.swift:797`, `:1749-1759`) *(open since 10-25c)*.
- **Mac: a source that fails to compile clears the reason** (`DeckEditor.swift:530-533`) *(open since 10-25c)*; **inspector Arrange and Group are silent when the engine refuses** (`Inspector.swift:504-521`) *(open since 10-25c)*.
- **Mac: Show Other Sizes re-lays out and paints every format on main after each edit** (`CanvasViews.swift:378`, `:416-422`).
- **Mac: a Metal paint failure is swallowed** (`ScaenaView.swift:273`; `failure` is read only by `TickingTests`).
- **Mac: the assistant retries timeouts alone, not dropped connections** (`Assistant.swift:436-437`).
- **Mac: paste reads a whole file on the main actor with no size gate** (`Clipboard.swift:300`).
- **iPad: no privacy manifest** though `UserDefaults` and `.creationDateKey` are read (`ScaenaApp.swift:23`, `ExportFolders.swift:38`): App Store Connect refuses the upload.
- **iPad: the remote's idle-timer switch skips `Awake`** (`RemoteView.swift:151-152`), and **no scene handles background/foreground for the Bonjour link** (`RemoteHosting.swift:69`).
- **iPad: Play and Share pick the first foreground scene** (`Presenting.swift:179-182`), which under Stage Manager may be another window's.
- **iPad: issue badges are ~21 pt tap targets** (`IssueBadges.swift:73-77`) and **both launch screens are blank** (`Info.plist:98-99`, `Remote/Info.plist:40-41`).
- **A session a panic left half-mutated is still saved** (`ffi/lib.rs:124-125`): a `poisoned` flag that refuses `scaena_save`.
- **The WASM sort guard skips `scaena-narrato` and `scaena-sources`**, which the editor's module carries (`wasm/tests/sorts.rs:13-14`).
- **The data sheet renders every row as inputs** with no windowing (`data.ts:212-225`).
- **The editor silences uncaught rejections without showing them** (`editor.ts:128-130`) where the player says them (`saysRejections`).
- **Switching provider while a key loads can send one provider's key to another** (`panel.ts:169-175`; `listing` guards models, nothing guards `choose`).
- **From a Prompt: choices failing to load say nothing, Enter twice submits twice, focus drops after moving a beat, the brief is forgotten between visits, and a generated deck leaves focus on the button** (`prompt.ts:189-191`, `:387-389`, `:498-501`; `editor.ts:451`) *(open since 10-25c)*.
- **Closing Make a Picture doesn't abort the paid request** (`picturing.ts:114-122`; `stage.makePicture` takes no signal) *(open since 10-25c)*.
- **The Story graph is mouse-only and a fix button drops focus** (`story.ts:252-254`, `:322-324`).
- **`recent()` rewrites earlier messages every round past 60 KB**, breaking Anthropic's cache (`converse.ts:141-148`); **narrato caches only the system rider** (`narrato/providers.rs:256`).
- **Patch errors say "is not there" without the nearest key** (`patch/mod.rs:840`, `:928`): `check::distance` exists.
- **`frame-ancestors` cannot be set by meta CSP**: add it as a header in `scaena-serve`'s `page()`.

Run `/burn-down` to execute this plan.
