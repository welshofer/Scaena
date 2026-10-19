# Lift-Up Plan: Scaena

> Platform: mixed. A Rust workspace (`Cargo.toml`, 21 crates: engine, painters, store, ops, session, CLI, MCP, serve, WASM, C ABI); a Vite + TypeScript web player and editor (`web/`, CodeMirror 6); SwiftUI clients (`apps/mac/ScaenaKit/Package.swift` for macOS 15 and iOS 26, the iPad app from `apps/ipad/project.yml`, and the iPhone Remote).
> Surveyed: 2026-10-19
> Baseline: origin/main@49ca37a.
> - `git fetch origin main` timed out, so freshness rests on the GitHub API. It gave main = 49ca37a5 at the start and again just before this file was written: no drift.
> - Everything was read from git objects (`git show` / `git grep origin/main`).
> - The local checkout (generation/narrato@6a51eadf, clean tree) was not used: with about 50 MB of disk free, main could not be checked out.
> Scope: full repo
> Model: below the top tier. A second pass at the top tier is recommended.
> Coverage: full.
> - Eight read-only surveys: core; engine and painters; store and ops; CLI, MCP, serve, chat and Narrato; web runtime and WASM; web editor UI; Mac app and C ABI; iPad, Remote and CI.
> - Every item below was then verified inline against 49ca37a.
> Attractiveness anchor: Keynote (inferred from CLAUDE.md's Phase 3: "a Keynote-like toolbar", PLAN 3.18).

How to read the evidence:
- Every `$ git grep …` line below is the literal command run, followed by its stdout verbatim.
- Line numbers are on 49ca37a.
- In a Proof, a line holding only `…` marks lines left out between quoted ranges, and `//` or `<!-- -->` lines are annotations. Every other line is verbatim.

## Performance

### PERF-1. Keep vello_cpu's glyph cache across frames
- **Location:** `crates/scaena-paint/src/lib.rs:866`
- **Proof:**
  ```rust
  // lib.rs:865-869: a fresh Resources for every frame
              let ctx = self.kept.context(width, height, self.level);
              let mut resources = Resources::new();
              let shaded = shaded.into_iter();
              fonts.frame();
              let mut cx = Cx { ctx: &mut *ctx, resources: &mut resources, store: fonts, fonts: &dl.fonts, shaded };
  // lib.rs:785-787: what the painter keeps between frames
      pub struct Kept {
          ctx: Option<RenderContext>,
      }
  ```
- **Verified:**
  ```
  $ git grep -n -E 'Resources' origin/main -- crates/scaena-paint/src
  origin/main:crates/scaena-paint/src/lib.rs:744:        Image, ImageSource, Pixmap, PixmapMut, RasterizerSettings, RenderContext, RenderSettings, Resources,
  origin/main:crates/scaena-paint/src/lib.rs:866:            let mut resources = Resources::new();
  origin/main:crates/scaena-paint/src/lib.rs:904:        resources: &'a mut Resources,
  $ git grep -n -E 'resources' origin/main -- crates/scaena-paint/src/lib.rs
  origin/main:crates/scaena-paint/src/lib.rs:866:            let mut resources = Resources::new();
  origin/main:crates/scaena-paint/src/lib.rs:869:            let mut cx = Cx { ctx: &mut *ctx, resources: &mut resources, store: fonts, fonts: &dl.fonts, shaded };
  origin/main:crates/scaena-paint/src/lib.rs:878:                &mut resources,
  origin/main:crates/scaena-paint/src/lib.rs:904:        resources: &'a mut Resources,
  origin/main:crates/scaena-paint/src/lib.rs:946:                            .glyph_run(self.resources, &font)
  $ git grep -n -E 'struct Kept' origin/main -- crates/scaena-paint/src
  origin/main:crates/scaena-paint/src/lib.rs:785:    pub struct Kept {
  ```
  The library's contract comes from the pinned `vello_cpu = "0.2.0"` (Cargo.toml:109), read from the local cargo registry:
  - `vello_cpu-0.2.0/src/render.rs:59-61` says "Persistent resources required by Vello CPU for rendering. … You should create one such instance per renderer."
  - Its fields include `glyph_prep_cache: GlyphPrepCache`.
  - `glyph_run` passes `atlas_cache_enabled: false` (render.rs:457), so a kept `Resources` caches outlines only, not glyph pixels.
  - `maintain_glyph_cache` (text.rs:109-110) evicts unused entries after each render.
  - Pictures are drawn as `ImageSource::Pixmap(picture.pixmap())` (paint lib.rs:968), not registered in it, so nothing accumulates.
- **Do:** Add a `resources: Resources` field `(new)` to `cpu::Kept` beside `ctx`. Hand `&mut self.kept.resources` to `Cx` and to `ctx.render_with` in place of the per-frame `Resources::new()`. The torture goldens and `just spike` must stay byte-identical.
- **Why:** Every CPU-painter frame re-prepares every glyph outline it draws. That covers each thumbnail, the Formats view, PNG and PDF rasters, every video frame, and every frame where WebGPU is missing, so text-heavy slides pay it on every paint.
- **Effort:** S · **Impact:** M

### PERF-2. Pause the Metal view's display link while nothing moves
- **Location:** `apps/mac/ScaenaKit/Sources/ScaenaKit/ScaenaView.swift:172`
- **Proof:**
  ```swift
  // ScaenaView.swift:168-175 and 207-209
      private func attach(_ next: CADisplayLink?) {
          link?.invalidate()
          link = next
          guard let next else { return }
          next.preferredFrameRateRange = CAFrameRateRange(minimum: 30, maximum: 120, preferred: 120)
          next.add(to: .main, forMode: .common)
          resized()
      }
  …
      @objc private func step(_ link: CADisplayLink) {
          guard dirty || playhead.playing, let session, let state, let layer = metalLayer, bounds.width > 0
          else { return }
  ```
- **Verified:**
  ```
  $ git grep -n -E 'isPaused|invalidate\(\)' origin/main -- apps/mac/ScaenaKit/Sources
  origin/main:apps/mac/ScaenaKit/Sources/ScaenaKit/ScaenaView.swift:169:        link?.invalidate()
  ```
  `attach(_:)` (:168), `show(_:)` (:179), `resized()` (:190), `restart()` (:199) and `step(_:)` (:207) are in the file as quoted. The one class serves the Mac and the iPad (the iPad's link is attached at :150).
- **Do:**
  - In `step(_:)`, when `!(dirty || playhead.playing)`, set `link.isPaused = true` before returning.
  - Set `link?.isPaused = false` wherever `dirty = true` is set: `show(_:)` :184, `resized()` :196, `restart()` :201, and the rest. A `didSet` on `dirty` does it in one place.
- **Why:** Every open deck, on the Mac and the iPad, wakes the main thread up to 120 times a second while idle, which costs battery on laptops and iPads. Also worth checking against the iPad UI tests' two-minute waits for the app to settle, the stall in the test plan: a main run loop that never goes quiet may be what keeps XCUITest from seeing the app idle. That link is a hypothesis, not established here.
- **Effort:** S · **Impact:** M

### PERF-3. Coalesce the canvas overlay's redraws into one per frame
- **Location:** `web/src/canvas.ts:3757`
- **Proof:**
  ```ts
  // canvas.ts:3753-3757, 3770-3775, 3784-3787: pointermove redraws on every event
      if (stroke && e.pointerId === stroke.pointer) {
        // Every point the pointer gave since the last move: a quick stroke keeps its curve.
        const given = e.getCoalescedEvents?.() ?? [];
        for (const g of given.length ? given : [e]) stroke.points.push(point(g));
        return draw();
  …
      if (drag) {
        [drag.at, drag.shift, drag.alt, drag.loose] = [at, e.shiftKey, e.altKey, e.metaKey || e.ctrlKey];
        if (drag.kind === "move") draw();
        void pump(drag);
        return;
      }
  …
      if (marquee) {
        marquee.at = at;
        return draw();
      }
  // canvas.ts:1369, 1373-1374: each draw rebuilds the overlay, then places the marks
      svg.innerHTML = parts.join("");
  …
      pins.aside(Boolean(drag || sketch || marquee));
      pins.draw();
  // marks.ts:131-132: which reads layout each time
    function placed(all: Mark[]): { mark: Mark; at: [number, number] }[] {
      const { width, height } = layer.getBoundingClientRect();
  ```
- **Verified:**
  ```
  $ git grep -n -E 'requestAnimationFrame' origin/main -- web/src/canvas.ts web/src/marks.ts web/src/editor.ts
  # (no matches)
  $ git grep -n -E 'function draw\(\)|const draw = ' origin/main -- web/src/canvas.ts
  origin/main:web/src/canvas.ts:1155:  function draw() {
  $ git grep -n -E 'const sized|sized = new ResizeObserver' origin/main -- web/src/canvas.ts
  origin/main:web/src/canvas.ts:4278:  const sized = new ResizeObserver(() => draw());
  ```
- **Do:**
  - Route the pointermove callers' `draw()` calls through `drawSoon()` `(new)`, which schedules one `requestAnimationFrame` and draws once per frame.
  - Have `placed()` (marks.ts:131) use a size kept by the existing `sized` ResizeObserver (canvas.ts:4278) instead of calling `getBoundingClientRect()` on every draw.
  - For the pen, append to one live `<polyline>` `(new)` instead of rebuilding the overlay.
- **Why:** During a drag, a marquee, a carry or a pen stroke, every pointermove rebuilds the whole overlay SVG through `innerHTML` and then forces a layout read. On a full slide that is visible jank in the editor's main gesture.
- **Effort:** S · **Impact:** M

### PERF-4. Stop thumbnails from laying out every state again and evicting the shown slide's layout
- **Location:** `crates/scaena-session/src/lib.rs:722`
- **Proof:**
  ```rust
  // scaena-session/src/lib.rs:108 and 722-723, 736: one at-rest layout, for whichever state asked last
      rest: Option<(String, scaena_engine::sample::Scene)>,
  …
      fn at_rest(&mut self, state: &str) -> Result<&scaena_engine::sample::Scene, Error> {
          if self.rest.as_ref().is_none_or(|(s, _)| s != state) {
  …
              self.rest = Some((state.to_string(), engine.at_rest(&req)?));
  // scaena-wasm/src/lib.rs:135-136 (the C ABI's "digest", scaena-ffi lib.rs:1414, is the same call)
      pub fn digest(&mut self, state: &str) -> Result<String, JsError> {
          self.0.frame(state, f64::INFINITY).map_err(js)?.digest().map_err(js)
  ```
  ```ts
  // web/src/worker.ts:1345-1349: after each compile, every state's digest, one after another
    for (const { state } of slots) {
      if (mine !== thumbing[kind]) break;
      if (only && !only.includes(state)) continue;
      try {
        const digest = player.digest(state);
  ```
  ```swift
  // apps/mac/ScaenaKit/Sources/Scaena/StateList.swift:210-211: each visible row, each revision, on the main actor
          .task(id: "\(slot.state)\u{1f}\(revision)") {
              drawing = editor.drawing(slot.state, width: Int(width * scale))
  ```
  `frame(state, ∞)` reaches `at_rest(state)` at lib.rs:704.
- **Verified:**
  ```
  $ git grep -n -E 'rest = |self\.rest|rest:' origin/main -- crates/scaena-session/src/lib.rs
  origin/main:crates/scaena-session/src/lib.rs:108:    rest: Option<(String, scaena_engine::sample::Scene)>,
  origin/main:crates/scaena-session/src/lib.rs:333:            rest: None,
  origin/main:crates/scaena-session/src/lib.rs:563:        self.rest = None;
  origin/main:crates/scaena-session/src/lib.rs:723:        if self.rest.as_ref().is_none_or(|(s, _)| s != state) {
  origin/main:crates/scaena-session/src/lib.rs:736:            self.rest = Some((state.to_string(), engine.at_rest(&req)?));
  origin/main:crates/scaena-session/src/lib.rs:738:        Ok(&self.rest.as_ref().expect("laid out above").1)
  origin/main:crates/scaena-session/src/lib.rs:998:        let scene = &self.rest.as_ref().expect("laid out above").1;
  origin/main:crates/scaena-session/src/lib.rs:1183:    /// rest that follow, from the state as laid out at rest: what a drag shows as it moves,
  origin/main:crates/scaena-session/src/lib.rs:5173:        let at_rest = |s: &mut Session, state: &str| s.frame(state, f64::INFINITY).unwrap().digest().unwrap();
  $ git grep -n -E 'digests|digest_kept|digest:' origin/main -- crates/scaena-session/src/lib.rs
  origin/main:crates/scaena-session/src/lib.rs:2643:    /// native goldens' digests (`raw.fnv1a`). The browser smoke check compares the
  origin/main:crates/scaena-session/src/lib.rs:2646:    fn frames_match_the_native_raw_digests() {
  $ git grep -n -E 'pub fn frame\(|fn forget_shown|pub fn digest' origin/main -- crates/scaena-session/src/lib.rs crates/scaena-core/src/displaylist.rs
  origin/main:crates/scaena-core/src/displaylist.rs:620:    pub fn digest(&self) -> Result<String, DlError> {
  origin/main:crates/scaena-session/src/lib.rs:560:    fn forget_shown(&mut self) {
  origin/main:crates/scaena-session/src/lib.rs:689:    pub fn frame(&mut self, state: &str, t_ms: f64) -> Result<DisplayList, Error> {
  $ git grep -n -E 'pub fn digest' origin/main -- crates/scaena-wasm/src/lib.rs crates/scaena-session/src/lib.rs
  origin/main:crates/scaena-wasm/src/lib.rs:135:    pub fn digest(&mut self, state: &str) -> Result<String, JsError> {
  ```
- **Do:**
  - Add `Session::digest(state)` `(new)`. It lays a state out at rest without replacing `rest`, so the canvas keeps the shown slide's scene.
  - Memoize it per state, keyed by a hash `(new)` of everything `frame(state, ∞)` reads: the resolved snapshot, the theme, the data files, the format, the slide's number, and its start on the timeline (shader time). Keep the memo across edits, so an edit lays out again only the states whose inputs changed.
  - Route the WASM `digest` (lib.rs:135) and the C ABI's `"digest"` (ffi lib.rs:1414) to it.
  - Add a test that edits one slide and checks the others' digests equal a fresh session's.
- **Why:**
  - Browser: after every compile, the worker lays out every state once more, and each call evicts the shown slide's layout, which the next frame lays out again.
  - Mac: each visible thumbnail row runs that layout synchronously on the main thread at every revision, so typing slows as more slides are in view.
- **Effort:** M · **Impact:** L

### PERF-5. Move the Mac save's subsetting and reopen out of `snapshot`
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/ScaenaDocument.swift:126`
- **Proof:**
  ```swift
  // ScaenaDocument.swift:126-134
      func snapshot(contentType: UTType) throws -> [String: Data] {
          let saved = try session.save(subset: true)
          var files: [String: Data] = [:]
          for path in saved.files {
              files[path] = saved.file(path)
          }
          try session.adopt(saved)
          return files
      }
  ```
  ```rust
  // scaena-ffi/src/lib.rs:730-736: every font subset on each save
          if subset {
              // What the browser hands its subsetter's module, done here: the same bytes.
              let (chars, fonts) = session.0.subsetting().map_err(said)?;
              let wanted = chars.chars().collect();
              for font in fonts {
                  let bytes = session.0.file(&font).ok_or_else(|| said(format!("{font} is not in the bundle")))?;
                  let small = scaena_store::subset::subset(bytes, &wanted).map_err(said)?;
  // scaena-session/src/store.rs:304-306: and adopt opens the whole bundle again
      pub fn adopt(&mut self, saved: &Saving) -> Result<(), Error> {
          // The theme names the fonts by their new names too.
          let mut next = Session::open(saved.files.clone())?;
  ```
- **Verified:**
  ```
  $ git grep -n -E 'Task\.detached|DispatchQueue|nonisolated|async' origin/main -- apps/mac/ScaenaKit/Sources/Scaena/ScaenaDocument.swift
  # (no matches)
  ```
  `ScaenaSession.adopt` is at ScaenaSession.swift:265. Its doc (:263-264) says "Go on from `saved`, once it is written where the bundle is kept", yet `snapshot` adopts before SwiftUI writes anything.
- **Do:**
  - Make `snapshot` cheap: take the session's files as they are (a `Sendable` copy, as `Laying` already is per the session's `Send` assertion).
  - Do the subsetting, the history recording and the wrapper in `fileWrapper(snapshot:configuration:)` (:136), which SwiftUI runs off the main thread.
  - Adopt the renames after the write instead of `Session::open` on a deep copy of every file.
- **Why:** SwiftUI calls `snapshot` on the main actor for every save. Today that is a subset of every font, a history record, and a full reopen and recompile, so a large deck hitches and briefly holds about three copies of the bundle.
- **Effort:** M · **Impact:** M

## Functionality

### FUNC-1. Duplicate a slide from the strip, and keep ⌘D away from the browser
- **Location:** `web/src/strip.ts:267`
- **Proof:**
  ```ts
  // strip.ts:267-272 and 284-291: the strip's keydown handler (273-283 are the arrow keys); no D, no ⌘D
    list.onkeydown = (e) => {
      const li = item(e);
      if (!li || (e.target as Element).tagName === "INPUT") return;
      const state = li.dataset.state!;
      const at = index(state);
      const steps: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1, Home: -at, End: slots.length - 1 - at };
  …
      if (e.key === "Delete" || e.key === "Backspace") {
        e.preventDefault();
        void remove(state);
      } else if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        editor.show(at);
      }
    };
  ```
  ```ts
  // editor.ts:1286-1289: the editor's page-wide key handler takes Z and Y alone
    heed(document, "keydown", (e) => {
      if (e.defaultPrevented || !(e.metaKey || e.ctrlKey) || e.altKey) return;
      const key = e.key.toLowerCase();
      if (key !== "z" && key !== "y") return;
  ```
- **Verified:**
  ```
  $ git grep -n -E 'duplicat' origin/main -- web/src/strip.ts web/src/slides.ts
  origin/main:web/src/slides.ts:13://   patch of `move_slide`, `duplicate_slide`, or `remove_slide`, by the user, one step to undo, and
  origin/main:web/src/slides.ts:304:      else if (op.op === "duplicate_slide") slides.splice(at + 1, 0, { id: `${op.slide} copy`, states: slides[at].states.map((s) => `${s} copy`) });
  origin/main:web/src/slides.ts:334:  function duplicate() {
  origin/main:web/src/slides.ts:341:      copying.map((slide) => ({ op: "duplicate_slide", slide })),
  origin/main:web/src/slides.ts:422:      return void duplicate();
  origin/main:web/src/slides.ts:493:    duplicate,
  $ git grep -n -E '"keydown"' origin/main -- web/src
  origin/main:web/src/canvas.ts:739:  listen(document, "keydown", unpress, { capture: true });
  origin/main:web/src/canvas.ts:4302:      document.removeEventListener("keydown", unpress, true);
  origin/main:web/src/cue.ts:369:  listen(lanes, "keydown", (e) => {
  origin/main:web/src/data.ts:327:  listen(table, "keydown", (e) => {
  origin/main:web/src/editor.ts:1286:  heed(document, "keydown", (e) => {
  origin/main:web/src/editor.ts:2034:    overlay.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...keys }));
  origin/main:web/src/files.ts:198:  listen(into, "keydown", (e) => {
  origin/main:web/src/icons.ts:65:  listen(field, "keydown", (e) => {
  origin/main:web/src/icons.ts:75:  listen(grid, "keydown", (e) => {
  origin/main:web/src/notes.ts:80:    field.addEventListener("keydown", (e) => {
  origin/main:web/src/player.ts:104:  addEventListener("keydown", (e) => {
  origin/main:web/src/rehearse.ts:117:    document.addEventListener("keydown", key, true);
  origin/main:web/src/rehearse.ts:140:    document.removeEventListener("keydown", key, true);
  origin/main:web/src/typing.ts:891:  area.addEventListener("keydown", (e) => {
  $ git grep -n -E 'where: \["state"' origin/main -- web/src/editor.ts
  origin/main:web/src/editor.ts:2344:      { label: "Add a step", where: ["state"], applies: () => now() !== undefined, run: () => states.add("step") },
  origin/main:web/src/editor.ts:2345:      { label: "Add a slide…", where: ["state"], applies: () => now() !== undefined, run: () => void states.gallery() },
  origin/main:web/src/editor.ts:2346:      { label: step() ? "Delete this step" : "Delete this slide", where: ["state"], applies: () => now() !== undefined && count() > 1, run: () => states.remove(now()!) },
  origin/main:web/src/editor.ts:2347:      { label: "Export it as a PNG…", where: ["state"], applies: () => now() !== undefined, run: () => openExport("png") },
  ```
- **Do:**
  - In `list.onkeydown` (strip.ts:267), take ⌘D/Ctrl+D with `preventDefault()`.
  - Make a `duplicate_slide` patch for the slide that holds the focused state, as the light table does (slides.ts:334-341), through the strip's existing `stage.make` path (strip.ts:193).
  - Add "Duplicate this slide" `(new)` to the `where: ["state"]` commands (editor.ts:2344-2347).
- **Why:** Copying a slide is among the most common deck edits, and today only the light table offers it. In the strip, ⌘D does nothing in the app, and the browser takes it as Add Bookmark.
- **Effort:** S · **Impact:** M

### FUNC-2. Play from the slide shown, with the edits made
- **Location:** `web/src/editor.ts:548`
- **Proof:**
  ```ts
  // editor.ts:548-555
      const playing =
        where?.kind === "serve"
          ? new URLSearchParams({ bundle: params.get("bundle") ?? "/bundle/", serve: "" })
          : new URLSearchParams({ bundle: where ? `${where.kind}:${where.name}` : (params.get("bundle") ?? fallback) });
      if (params.has("painter")) playing.set("painter", painter);
      play.href = `index.html?${playing}`;
      // Nor a new deck, until it is saved.
      play.hidden = where?.kind === "folder" || (!where && ("create" in source || "files" in source));
  ```
  editor.html:679: `<a id="play" class="button" target="_blank" title="The player, on the deck as last saved" hidden>Play</a>`
- **Verified:**
  ```
  $ git grep -n -E 'playing\.set\("state"|set\("state"' origin/main -- web/src/editor.ts
  # (no matches)
  $ git grep -n -E 'get\("state"\)|params\.get\("state"|"state"' origin/main -- web/src/main.ts
  origin/main:web/src/main.ts:30:    state: params.get("state"),
  ```
  `showing()` (editor.ts:774-777) returns `{ state, index }`, and `dirty()` is used at :544.
- **Do:**
  - Set `state` in `playing` from `showing()?.state`. The player already reads `?state=` (main.ts:30).
  - When `dirty()`, offer to save first, so the player plays what the editor shows.
  - Draft #440 (not on main) unhides Play for new decks. This item covers where Play starts and what it plays.
- **Why:** Play always opens slide 1 of the deck as last saved. Someone rehearsing slide 14 clicks through thirteen slides, and edits not yet saved silently don't play.
- **Effort:** S · **Impact:** M

### FUNC-3. Type over a selected text's words, as Keynote does
- **Location:** `apps/mac/ScaenaKit/Sources/ScaenaKit/CanvasKeys.swift:266`
- **Proof:**
  ```swift
  // CanvasKeys.swift:265-270
      public func insertText(_ string: Any, replacementRange: NSRange) {
          // Characters typed with no text typed in do nothing yet.
          guard typing.typing else { return }
          let text = Self.plain(string)
          if replacementRange.location == NSNotFound {
              typing.insert(text)
  ```
  ```ts
  // web/src/canvas.ts:4045-4046 and 4069: in the browser only Enter starts typing on a selected text
      if (e.key === "Enter" && selected !== undefined && !drag && !mod) {
        e.preventDefault();
  …
        return void type(selected, undefined, e.altKey);
  ```
- **Verified:**
  ```
  $ git grep -n -E 'key\.length === 1|printable|type over|types over' origin/main -- web/src/canvas.ts web/src/typing.ts
  origin/main:web/src/canvas.ts:2445:   * there: its prompt's words to type over, or a list's items (PLAN 3.30). One patch, selected. */
  $ git grep -n -E 'func typeIn|func selectAll' origin/main -- apps/mac/ScaenaKit/Sources
  origin/main:apps/mac/ScaenaKit/Sources/Scaena/CanvasSelection.swift:1489:    private func selectAll() -> Bool {
  origin/main:apps/mac/ScaenaKit/Sources/Scaena/CanvasSelection.swift:1559:    private func typeIn(_ node: String, at point: CGPoint?) -> Bool {
  origin/main:apps/mac/ScaenaKit/Sources/ScaenaKit/CanvasTextInput.swift:320:    public override func selectAll(_ sender: Any?) {
  origin/main:apps/mac/ScaenaKit/Sources/ScaenaKit/CanvasTextInput.swift:1069:    public override func selectAll(_ sender: Any?) {
  origin/main:apps/mac/ScaenaKit/Sources/ScaenaKit/Typing.swift:335:    public func selectAll() {
  ```
- **Do:**
  - Mac: in `insertText` (CanvasKeys.swift:265), when nothing is typed in and exactly one unlocked text (or shape with words) is selected, call the canvas's `typeIn` (CanvasSelection.swift:1559, through a callback `(new)` the canvas hands `CanvasKeys`, since it is private to the app target). Then call `Typing.selectAll()` (Typing.swift:335) and `typing.insert(text)`, so the first key replaces the words.
  - Browser: in `overlay.onkeydown` (canvas.ts:3939), a printable key with one text selected does what Enter does at :4069, with all the words selected.
  - The canvas's own letter keys (`[`, `]`, `+`, Space) keep their meaning.
- **Why:** Keynote and PowerPoint users click a box and type. Here the keys vanish, and only a double-click or Enter starts typing.
- **Effort:** M · **Impact:** M

### FUNC-4. Let touch alone select several things and keep an edit to one slide (iPad)
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/Platform.swift:17`
- **Proof:**
  ```swift
  // Platform.swift:16-32: Shift and Option are a hardware keyboard's alone on the iPad
      /// Shift: off the grid, or one more selected.
      static var shift: Bool {
          #if os(macOS)
          NSEvent.modifierFlags.contains(.shift)
          #else
          down(.leftShift) || down(.rightShift)
          #endif
      }

      /// Option: kept to the state shown.
      static var option: Bool {
          #if os(macOS)
          NSEvent.modifierFlags.contains(.option)
          #else
          down(.leftAlt) || down(.rightAlt)
          #endif
      }
  // CanvasSelection.swift:1539-1540
          if Held.shift, let hit, node != nil {
              return toggle(hit)
  // LightTable.swift:138-140
          if Held.shift, let anchor, let a = ids.firstIndex(of: anchor), let b = ids.firstIndex(of: id) {
              picked = Set(ids[min(a, b)...max(a, b)])
          } else if Held.command {
  ```
- **Verified:**
  ```
  $ git grep -n -E 'EditMode|editMode|selecting mode|Select mode' origin/main -- apps/mac/ScaenaKit/Sources/Scaena
  # (no matches)
  $ git grep -n -E 'struct UndoButton|struct CanvasMenu|func toggle\(' origin/main -- apps/mac/ScaenaKit/Sources/Scaena
  origin/main:apps/mac/ScaenaKit/Sources/Scaena/CanvasMenus.swift:174:struct CanvasMenu: NSViewRepresentable {
  origin/main:apps/mac/ScaenaKit/Sources/Scaena/CanvasSelection.swift:1652:    private func toggle(_ hit: String) {
  origin/main:apps/mac/ScaenaKit/Sources/Scaena/CanvasTouch.swift:162:struct CanvasMenu: UIViewRepresentable {
  origin/main:apps/mac/ScaenaKit/Sources/Scaena/CueBar.swift:211:    private func toggle() {
  origin/main:apps/mac/ScaenaKit/Sources/Scaena/Menus.swift:198:struct UndoButton: View {
  ```
- **Do:**
  - On the iPad, add a Select toggle `(new)` beside `UndoButton` (Menus.swift:198). While on, a canvas tap calls `toggle(hit)` (CanvasSelection.swift:1652), and a light-table tap toggles `picked` as ⌘-click does (LightTable.swift:140-142).
  - Add "Only on This Slide" `(new)` to the long press's `CanvasMenu` (CanvasTouch.swift:162), so a gesture can be kept to the slide without Option.
- **Why:** Without a keyboard, an iPad user cannot select two objects to align, group or space them, cannot pick several slides in the light table, and cannot keep an edit to one slide. Gate 4's criterion asks for each touch to make the Mac's patch.
- **Effort:** M · **Impact:** M

### FUNC-5. Open a deck whose theme is inline from a URL
- **Location:** `web/src/worker.ts:942`
- **Proof:**
  ```ts
  // worker.ts:787-789 and 940-942
  interface DeckFiles {
    meta?: { title?: string };
    theme: string;
  …
    const json = await (await get(deck)).text();
    const files = JSON.parse(json) as DeckFiles;
    const fetchedPlayer = new Player(json, await (await get(files.theme)).text());
  // worker.ts:1066-1067: the bundle path already handles both
    const theme = (JSON.parse(deck) as { theme?: unknown }).theme;
    const session = new Player(deck, typeof theme === "string" ? text(theme) : JSON.stringify(theme));
  ```
- **Verified:**
  ```
  $ git grep -n -E 'typeof theme === "string"|typeof files.theme' origin/main -- web/src/worker.ts
  origin/main:web/src/worker.ts:1067:  const session = new Player(deck, typeof theme === "string" ? text(theme) : JSON.stringify(theme));
  ```
  docs/schema/deck.schema.json:24: `"description": "Path to a theme file inside the bundle, or an inline theme object (schema: theme.schema.json).",`
- **Do:** Type `DeckFiles.theme` as `string | object`. In `fetched()` do what `bundleOf` does: fetch the theme only when it is a string, else `JSON.stringify(files.theme)`. Delete it from `paths` (:954) only in the string case.
- **Why:** A deck with its theme inline opens from a bundle but fails from a URL (`?bundle=`) with an HTTP 404 for "[object Object]". The schema allows it, and `tests/lint/W221/*.deck.json` are written that way.
- **Effort:** S · **Impact:** S

## Stability

### STAB-1. Bound nesting depth in the expression and DSL parsers
- **Location:** `crates/scaena-core/src/expr.rs:252`
- **Proof:**
  ```rust
  // expr.rs:251-253, 277-279, 312-313: each `!`, `-`, or `(` recurses, with no limit
      fn not(&mut self) -> Result<Expr, ExprError> {
          if self.eat("!") { Ok(Expr::Not(Box::new(self.not()?))) } else { self.compare() }
      }
  …
      fn unary(&mut self) -> Result<Expr, ExprError> {
          if self.eat("-") { Ok(Expr::Negate(Box::new(self.unary()?))) } else { self.atom() }
      }
  …
              Token::Op("(") => {
                  let inner = self.or()?;
  // dsl/parse.rs:291 and 297: and each `[` in a .scn value
              Tok::LBracket => {
  …
                          let item = self.value(t, &format!("{ptr}/{}", items.len()))?;
  ```
- **Verified:**
  ```
  $ git grep -n -E 'depth|DEPTH|nest' origin/main -- crates/scaena-core/src/dsl/parse.rs crates/scaena-core/src/expr.rs
  # (no matches)
  $ git grep -n -E 'pub const MAX_NESTING' origin/main -- crates
  origin/main:crates/scaena-core/src/code/mod.rs:76:pub const MAX_NESTING: usize = 8;
  origin/main:crates/scaena-core/src/document.rs:19:pub const MAX_NESTING: usize = 64;
  $ git grep -n -E 'expr::parse\(|expr::parse\b' origin/main -- crates
  origin/main:crates/scaena-core/src/transform.rs:81:        let parsed = expr::parse(src).map_err(|e| self.malformed(at, format!("`{src}` {e}")))?;
  origin/main:crates/scaena-core/src/transform.rs:213:            let call = expr::parse(src).map_err(|e| self.malformed(&at, format!("`{src}` {e}")))?;
  ```
  Everything else that nests is bounded: containers at 64 (document.rs:19, containers.rs:136), and code highlighting at 8 (code/mod.rs:76). `crates/scaena-session/tests/edits.rs` exists.
- **Do:**
  - Give expr's `Parser` and the DSL's `value()` a depth counter `(new)`, refused past `scaena_core::document::MAX_NESTING` with an `ExprError`/`DslError` that says "nested more than 64 deep".
  - Add deep brackets and long runs of `!` and `-` to the random edits in `crates/scaena-session/tests/edits.rs`.
- **Why:** A dataTransform expression, or a .scn value, nested a few thousand deep overflows the stack, and less on a background queue's smaller stack.
  - A stack overflow aborts the process: the C ABI's `catch_unwind` cannot catch it, so the Mac app closes. In the browser it kills the worker (STAB-2).
  - A shared deck can carry such an expression in deck.json; it is parsed when the chart that reads it is laid out.
- **Effort:** S · **Impact:** L

### STAB-2. Recover the browser editor from an engine panic instead of losing Save
- **Location:** `web/src/worker.ts:731`
- **Proof:**
  ```ts
  // worker.ts:731-735: the error is posted and the same player goes on being used
    } catch (e) {
      post({ type: "error", id, message: said(e) });
    } finally {
      if (!reads) worded = undefined;
    }
  // stage.ts:211, 215, 234: a worker error reaches a no-op, and requests waiting stay waiting
    onError: (error: Error) => void = () => {};
  …
    private waiting = new Map<number, { resolve: (reply: Reply) => void; reject: (error: Error) => void }>();
  …
      worker.onerror = (e) => this.onError(new Error(e.message));
  ```
- **Verified:**
  ```
  $ git grep -n -E 'RuntimeError|recursive use|crashed|onmessageerror' origin/main -- web/src
  # (no matches)
  $ git grep -n -E 'onError|private waiting|pending = new Map|replies = new Map|new Map<number' origin/main -- web/src/stage.ts
  origin/main:web/src/stage.ts:211:  onError: (error: Error) => void = () => {};
  origin/main:web/src/stage.ts:215:  private waiting = new Map<number, { resolve: (reply: Reply) => void; reject: (error: Error) => void }>();
  origin/main:web/src/stage.ts:217:  private hearing = new Map<number, (event: AssistantEvent) => void>();
  origin/main:web/src/stage.ts:219:  private thumbing = new Map<number, (thumb: Thumb) => void>();
  origin/main:web/src/stage.ts:234:    worker.onerror = (e) => this.onError(new Error(e.message));
  origin/main:web/src/stage.ts:1213:        } else this.onError(new Error(data.message));
  ```
  `Stage.open` is at stage.ts:241.
- **Do:**
  - In the worker's catch (worker.ts:731), when `e instanceof WebAssembly.RuntimeError`, post a `crashed` message `(new, protocol.ts)` and stop using `player`.
  - In `Stage`, on `crashed` or `worker.onerror`, reject every entry in `waiting`, reopen through `Stage.open` (stage.ts:241) from the editor's current source, and say what happened.
  - Test it with a debug-only trap `(new)` and assert Save works after the recovery.
- **Why:** wasm32 does not unwind. A Rust panic traps, wasm-bindgen leaves the `Player`'s borrow held, and every later call fails, Save included. So one engine bug becomes lost work, and with no recovery copy (REL-4) a reload is the only way out.
- **Effort:** M · **Impact:** L

### STAB-3. Decode pictures on demand under a memory budget
- **Location:** `crates/scaena-session/src/lib.rs:259`
- **Proof:**
  ```rust
  // scaena-session/src/lib.rs:259-264: every image the deck names, decoded when it opens
      for path in deck.image_files() {
          let bytes = file(&path)?;
          let info = images.register(&path, bytes)?;
          if !store.take_image(&info.id, kept) {
              store.insert_image(&info.id, bytes)?;
          }
  // scaena-paint/src/lib.rs:149-150 and 244-246: full RGBA kept, and the JPEG file copied again
      pub fn insert_image(&mut self, id: &str, bytes: &[u8]) -> Result<(), PaintError> {
          self.images.insert(id.to_string(), Arc::new(Picture::decode(bytes)?));
  …
              rgba: Blob::new(Arc::new(seen.rgba)),
              jpeg: Some(Jpeg {
                  bytes: Blob::new(Arc::new(bytes.to_vec())),
  ```
  The CPU painter then keeps a second, premultiplied copy per picture (`pixmap()`, paint lib.rs:274-289).
- **Verified:**
  ```
  $ git grep -n -E 'images\.(retain|remove|clear)|IMAGE_PIXELS|MAX_IMAGE' origin/main -- crates/scaena-paint/src/lib.rs
  origin/main:crates/scaena-paint/src/lib.rs:213:        if width.max(height) > scaena_core::displaylist::MAX_IMAGE_SIDE {
  $ git grep -n -E 'struct Adjusted|ADJUSTED_|const KEPT_PIXELS|MAX_KEPT|lru' origin/main -- crates/scaena-paint/src/lib.rs
  origin/main:crates/scaena-paint/src/lib.rs:82:struct Adjusted {
  origin/main:crates/scaena-paint/src/lib.rs:91:const ADJUSTED_KEPT: usize = 8;
  origin/main:crates/scaena-paint/src/lib.rs:92:const ADJUSTED_PIXELS: u64 = 64 << 20;
  origin/main:crates/scaena-paint/src/lib.rs:95:    /// Lets the least lately named go while more than [`ADJUSTED_KEPT`] are kept or their pixels
  origin/main:crates/scaena-paint/src/lib.rs:96:    /// pass [`ADJUSTED_PIXELS`], never one this frame or the last named: a frame painted again
  origin/main:crates/scaena-paint/src/lib.rs:102:        while self.kept.len() > 1 && (self.kept.len() > ADJUSTED_KEPT || total > ADJUSTED_PIXELS) {
  origin/main:crates/scaena-paint/src/lib.rs:390:        for e in 1..=(ADJUSTED_KEPT as i16 + 1) {
  origin/main:crates/scaena-paint/src/lib.rs:393:        assert_eq!(assets.adjusted.lock().unwrap().kept.len(), ADJUSTED_KEPT);
  origin/main:crates/scaena-paint/src/lib.rs:407:        let many = ADJUSTED_KEPT as i16 * 2;
  origin/main:crates/scaena-paint/src/lib.rs:424:        assert_eq!(names.len(), ADJUSTED_KEPT, "{names:?}");
  origin/main:crates/scaena-paint/src/lib.rs:426:        assert_eq!(names[ADJUSTED_KEPT - 2..], [named(1), named(-3)], "the latest last: {names:?}");
  $ git grep -n -E 'pub fn register' origin/main -- crates/scaena-engine/src/images.rs
  origin/main:crates/scaena-engine/src/images.rs:117:    pub fn register(&mut self, path: &str, bytes: &[u8]) -> Result<ImageInfo, EngineError> {
  ```
  Only adjusted pictures have a budget. The only check on base pictures is a side cap (:213).
- **Do:**
  - Decode a picture the first time it is drawn, not at open. Layout needs only the `ImageInfo` that `BundleImages::register` (images.rs:117) already reads.
  - Keep decoded pixels under a total-pixel budget `(new)`, evicting the least recently drawn as `Adjusted` does with `ADJUSTED_PIXELS` (lib.rs:92).
  - Take the JPEG bytes from the session's files instead of copying them (:246).
  - The pixels stay the same; the goldens must not move.
- **Why:** A 12-MP photo holds about 48 MB as RGBA, then a second premultiplied copy once the CPU painter draws it, plus its file twice. A deck of thirty photos is past what an iPad or a WASM heap gives, and every photo is decoded before the first frame.
- **Effort:** L · **Impact:** L

### STAB-4. Keep a large table from freezing layout before it errors
- **Location:** `crates/scaena-engine/src/tables.rs:167`
- **Proof:**
  ```rust
  // tables.rs:164-168: key uniqueness by linear search, once per row
      let mut keys: Vec<String> = Vec::with_capacity(table.rows.len());
      for row in &table.rows {
          let k = row[key].label();
          if keys.contains(&k) {
              return Err(EngineError::Data(format!(
  // tables.rs:222-223 and 233: then every cell is shaped …
      let mut body: Vec<Vec<Option<TextLayout>>> = Vec::with_capacity(table.rows.len());
      for row in &table.rows {
  …
              cells.push(if text.is_empty() { None } else { Some(set(text, &cell_role, c.numeric)?) });
  // tables.rs:290: … before `place` learns the room it has and refuses the overflow
      pub fn place(self, size: [f32; 2], lenient: bool) -> Result<TableLayout, EngineError> {
  ```
- **Verified:**
  ```
  $ git grep -n -E 'MAX_ROWS|HashSet|max_rows' origin/main -- crates/scaena-engine/src/tables.rs
  # (no matches)
  ```
- **Do:**
  - Check key uniqueness with a set filled by `insert`, not `collect` (CLAUDE.md's WASM-size rule for sets).
  - Stop typesetting body rows once their summed height passes the cell `place` will get: pass the size into the typesetting, or cap rows at what the theme's smallest row could fit `(new)`. Count the rest for E100 under `lenient`.
- **Why:** Pasting a sheet of a few thousand rows makes a table (PLAN 2.96). The key check is quadratic, and every cell is shaped by parley before the error, so the worker or the Mac's main thread freezes for seconds and then says the table doesn't fit.
- **Effort:** M · **Impact:** M

### STAB-5. Stop lint from panicking on a forecast's short first date
- **Location:** `crates/scaena-core/src/lint/mod.rs:181`
- **Proof:**
  ```rust
  // lint/mod.rs:176-181
          let first = rows.and_then(|r| r.first()).and_then(|row| row.get("date").or_else(|| row.get("time")));
          let first = first.and_then(Value::as_str);
          let message: Box<dyn Fn(&str) -> String> = if crate::fetched::passed(fetched, first, now) {
              let Some(forecast) = &fetched.forecast else { continue };
              let day: String =
                  first.map_or_else(|| crate::fetched::date(&fetched.at).unwrap_or("").into(), |d| d[..10].into());
  // fetched.rs:454-458: `passed` is true for a first date `days` refuses, by the fetch's own day
      let first = match first.and_then(days) {
          Some(day) => day,
          None => match seconds(&fetched.at) {
              Some(at) => (at + offset).div_euclid(86_400),
              None => return false,
  ```
  `days` (fetched.rs:417) refuses anything shorter than 10 bytes, which is exactly what then reaches `d[..10]`.
- **Verified:**
  ```
  $ git grep -n -E 'lint::dated' origin/main -- crates
  origin/main:crates/scaena-ops/src/lint.rs:102:        linted.findings.extend(scaena_core::lint::dated(deck, now));
  origin/main:crates/scaena-session/src/editor.rs:414:        let linted = scaena_ops::lint::dated(linted, &deck, now);
  $ git grep -n -E '\[\.\.[0-9]+\]\.into|\[\.\.[0-9]+\]\.to_|\| ?d\[\.\.' origin/main -- crates/scaena-core/src crates/scaena-ops/src crates/scaena-engine/src crates/scaena-session/src
  origin/main:crates/scaena-core/src/inserts.rs:218:            true => (format!("Image · {}…", &stem[..8]), format!("image-{}", stem[..8].to_ascii_lowercase())),
  origin/main:crates/scaena-core/src/lint/mod.rs:181:                first.map_or_else(|| crate::fetched::date(&fetched.at).unwrap_or("").into(), |d| d[..10].into());
  origin/main:crates/scaena-core/src/model/mod.rs:163:        let name = format!("{}{}Node", tag[..1].to_uppercase(), &tag[1..]);
  origin/main:crates/scaena-core/src/words.rs:969:        first[..1].to_uppercase(),
  origin/main:crates/scaena-ops/src/health.rs:453:                format!("{}{}.", said[..1].to_uppercase(), &said[1..])
  origin/main:crates/scaena-ops/src/narrative.rs:3126:                let word = format!("{}{}", word[..1].to_uppercase(), &word[1..]);
  origin/main:crates/scaena-ops/src/narrative.rs:3853:        let mut p = Painting::new(asked[..4].to_vec(), settings.clone());
  origin/main:crates/scaena-ops/src/narrative.rs:3874:        let mut p = Painting::new(asked[..3].to_vec(), settings);
  origin/main:crates/scaena-ops/src/narrative.rs:4127:            generating(&narrative(&summary, slides[..3].to_vec()), Path::new("dusk.theme.json"), dusk(), &font)
  origin/main:crates/scaena-session/src/writing.rs:554:        assert_eq!(smaller(&rgba[..64], 4, 4, SEEN), (rgba[..64].to_vec(), 4, 4), "one no larger is as it was");
  ```
  The siblings were checked: see "Dropped during verification".
- **Do:** Slice with `d.get(..10).unwrap_or(d)`, or fall back to the fetch's own date as the `None` arm does. Add a lint test with a forecast whose first `time` is `"noon"`, and a case to `crates/scaena-session/tests/edits.rs`.
- **Why:** `scaena lint`, the MCP lint tool, and the editors' lint panic on such a deck once its fetch day has passed, instead of reporting. CLAUDE.md requires an error that says why, never a panic, and in the browser the panic also kills the worker (STAB-2).
- **Effort:** S · **Impact:** S

## Reliability

### REL-1. Keep the screen awake while presenting, and follow displays that come and go
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/Presenting.swift:33`
- **Proof:**
  ```swift
  // Presenting.swift:33 and 40-43 (macOS): screens read once; no activity taken
      static func play(_ editor: DeckEditor, from state: String?, stopped: ((String) -> Void)? = nil) {
  …
          let main = NSScreen.main ?? NSScreen.screens.first
          let external = NSScreen.screens.first { $0 != main }
          guard let screen = external ?? main else { return }
          let stage = StageWindow(screen: screen, content: StageView(showing: showing))
  ```
  ```ts
  // web/src/player.ts:245-248
  const fullscreen = () =>
    (document.fullscreenElement ? document.exitFullscreen() : document.documentElement.requestFullscreen()).catch(
      () => {},
    );
  ```
- **Verified:**
  ```
  $ git grep -n -E 'wakeLock|WakeLock' origin/main -- web/src
  # (no matches)
  $ git grep -n -E 'isIdleTimerDisabled|beginActivity|idleDisplaySleepDisabled|IOPMAssertion' origin/main -- apps/mac/ScaenaKit/Sources
  origin/main:apps/mac/ScaenaKit/Sources/ScaenaRemote/RemoteView.swift:138:        .onAppear { UIApplication.shared.isIdleTimerDisabled = true }
  origin/main:apps/mac/ScaenaKit/Sources/ScaenaRemote/RemoteView.swift:139:        .onDisappear { UIApplication.shared.isIdleTimerDisabled = false }
  $ git grep -n -E 'didChangeScreenParameters|screensDidChange' origin/main -- apps/mac/ScaenaKit/Sources
  # (no matches)
  $ git grep -n -E 'static func (play|end|place)\(' origin/main -- apps/mac/ScaenaKit/Sources/Scaena/Presenting.swift
  origin/main:apps/mac/ScaenaKit/Sources/Scaena/Presenting.swift:33:    static func play(_ editor: DeckEditor, from state: String?, stopped: ((String) -> Void)? = nil) {
  origin/main:apps/mac/ScaenaKit/Sources/Scaena/Presenting.swift:71:    static func end() {
  origin/main:apps/mac/ScaenaKit/Sources/Scaena/Presenting.swift:110:    static func play(_ editor: DeckEditor, from state: String?, stopped: ((String) -> Void)? = nil) {
  origin/main:apps/mac/ScaenaKit/Sources/Scaena/Presenting.swift:140:    static func end() {
  origin/main:apps/mac/ScaenaKit/Sources/Scaena/Presenting.swift:156:    static func place() {
  $ git grep -n -E 'const fullscreen|function fullscreen|presenter\(|function present' origin/main -- web/src/player.ts
  origin/main:web/src/player.ts:245:const fullscreen = () =>
  origin/main:web/src/player.ts:698:async function presentView(deck: Source, how: Play, channel: BroadcastChannel) {
  ```
- **Do:**
  - Mac: take `ProcessInfo.processInfo.beginActivity(options: [.idleDisplaySleepDisabled, .userInitiated], reason:)` in `play` (:33) and end it in `end()` (:71). Observe `NSApplication.didChangeScreenParametersNotification` to re-place the stage when a projector comes or goes, as the iPad's `place()` (:156) does.
  - iPad: set `UIApplication.shared.isIdleTimerDisabled` in `play` (:110) and clear it in `end()` (:140), as the Remote does.
  - Browser: take `navigator.wakeLock.request("screen")` when `fullscreen()` or `presentView` (:698) starts, take it again on `visibilitychange`, and release it at the end.
- **Why:** A talk that stays on one slide past the display's sleep timer dims or blanks the projector mid-sentence, on every platform. An iPad driven from the iPhone Remote is never touched, so it locks. A projector plugged in after Play starts is ignored on the Mac.
- **Effort:** S · **Impact:** L

### REL-2. Make the offline site update safely
- **Location:** `web/vite.config.ts:124`
- **Proof:**
  ```js
  // vite.config.ts:123-125 and 149-153 (the service worker the build writes)
  self.addEventListener("install", (e) => {
    e.waitUntil(caches.open(site).then((cache) => cache.addAll(files)));
  });
  …
    const cache = await caches.open(kept);
    try {
      const response = await fetch(request);
      if (response.ok) await cache.put(request, response.clone());
      return response;
  ```
- **Verified:**
  ```
  $ git grep -n -E 'skipWaiting|clients\.claim|updatefound|controllerchange|cache: "reload"|cache: .no-cache' origin/main -- web/vite.config.ts web/src/offline.ts
  origin/main:web/vite.config.ts:132:      .then(() => self.clients.claim()),
  $ git grep -n -E 'register|export (async )?function' origin/main -- web/src/offline.ts
  origin/main:web/src/offline.ts:1:// The static site, installed (PLAN 2.73): the pages register `sw.js`, the service worker the build
  origin/main:web/src/offline.ts:10:export function offline(ready?: () => void) {
  origin/main:web/src/offline.ts:17:    .register("./sw.js")
  ```
- **Do:**
  - In `worker()` (vite.config.ts:112), precache with `cache.addAll(files.map((f) => new Request(f, { cache: "reload" })))`.
  - Give `cache.put` (:152) its own try, so a full quota leaves the network's answer standing.
  - In `offline()` (offline.ts:10), watch `updatefound`/`controllerchange` and offer "A new version is ready — reload" `(new)`, which posts a `skipWaiting` message `(new)` to the waiting worker.
- **Why:**
  - `addAll` reads through the browser's HTTP cache, so a page cached from the old build can be stored beside the new build's hashed assets, and the site breaks offline.
  - A failed `cache.put` turns a good response into an error.
  - With no update path, users keep the old build until every tab closes.
- **Effort:** S · **Impact:** M

### REL-3. Write bundles atomically
- **Location:** `crates/scaena-store/src/save.rs:69`
- **Proof:**
  ```rust
  // save.rs:68-69, 292-293 and 297, 324-325 and 329: truncate-then-write, file by file in path order
          if to.extension().and_then(|e| e.to_str()) == Some("scaena") && !to.is_dir() {
              std::fs::write(to, zip(&files)?)?;
  …
          for (rel, bytes) in out {
              let path = to.join(rel);
  …
              std::fs::write(path, bytes)?;
  …
              Files::Zip(entries) => {
                  let mut all = (**entries).clone();
  …
                  std::fs::write(&self.root, zip(&all)?)?;
  ```
- **Verified:**
  ```
  $ git grep -n -E 'fs::rename|sync_all|sync_data|NamedTempFile|tempfile|write_whole|\.partial' origin/main -- crates/scaena-store/src
  # (no matches)
  $ git grep -n -E 'fs::write' origin/main -- crates/scaena-store/src
  origin/main:crates/scaena-store/src/save.rs:69:            std::fs::write(to, zip(&files)?)?;
  origin/main:crates/scaena-store/src/save.rs:297:            std::fs::write(path, bytes)?;
  origin/main:crates/scaena-store/src/save.rs:320:                    std::fs::write(path, bytes)?;
  origin/main:crates/scaena-store/src/save.rs:329:                std::fs::write(&self.root, zip(&all)?)?;
  origin/main:crates/scaena-store/src/save.rs:355:                    std::fs::write(crate::inside(root, "manifest.json")?, bytes)?;
  origin/main:crates/scaena-store/src/save.rs:367:                std::fs::write(&self.root, zip(&all)?)?;
  $ git grep -n -E 'fn write_whole' origin/main -- crates
  origin/main:crates/scaena-serve/src/watch.rs:395:fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
  ```
- **Do:**
  - Move `write_whole` (watch.rs:395: a dot-named partial made with `create_new`, then renamed) into scaena-store as a public helper that serve keeps using.
  - Use it at all six `std::fs::write` sites above.
  - Write `deck.json` and `manifest.json` last in `write_dir` and `write`, as the browser's save already orders them (worker.ts:1195-1197).
- **Why:** `std::fs::write` truncates and then writes. A crash, a full disk, or an agent killed mid-save leaves a half-written `.scaena` zip, with the whole deck unreadable, or a `deck.json` naming files not yet written. These six lines are every write the store makes.
- **Effort:** M · **Impact:** L

### REL-4. Keep a recovery copy of what the browser editor hasn't saved
- **Location:** `web/src/editor.ts:2637`
- **Proof:**
  ```ts
  // editor.ts:2637-2644: the only guard
    onbeforeunload = (e) => {
      // Changes not saved, among them what is typed and not yet made, and a field in the inspector
      // typed in and not yet left.
      const field = document.activeElement;
      const unsettled =
        board.unsettled() || ((field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement) && $("#look").contains(field) && field.value !== field.defaultValue);
      if (dirty() || unsettled) e.preventDefault();
    };
  ```
  editor.html:881 ends: `It is kept nowhere until you save it.</p>`
- **Verified:**
  ```
  $ git grep -n -E 'visibilitychange|pagehide|setInterval|recover|autosave|Recover' origin/main -- web/src/editor.ts
  # (no matches)
  $ git grep -n -E 'navigator\.storage\.getDirectory|export (async )?function (kept|keptIn|bundles|opfs)' origin/main -- web/src
  origin/main:web/src/folders.ts:6:export async function kept(): Promise<FileSystemDirectoryHandle> {
  origin/main:web/src/folders.ts:7:  return (await navigator.storage.getDirectory()).getDirectoryHandle("bundles", { create: true });
  origin/main:web/src/folders.ts:11:export async function keptNames(): Promise<string[]> {
  origin/main:web/src/folders.ts:18:export async function keptBundle(name: string): Promise<FileSystemDirectoryHandle> {
  ```
- **Do:**
  - Write a recovery copy `(new)` of the editor's source, and where the deck lives, to OPFS: a `recovery/` directory `(new)` beside the `bundles/` that `kept()` uses.
  - Write it a second or two after each change and on `visibilitychange` to hidden; delete it after a save.
  - When a deck opens with a newer copy, offer Recover or Discard `(new)`.
  - Files dropped since the save keep their bytes too, or the offer says what the copy lacks.
- **Why:** `beforeunload` is the only protection. A tab crash, an OS kill, a discarded background tab, or an engine panic (STAB-2) loses everything since the last save, and a new deck has no saved copy at all.
- **Effort:** M · **Impact:** L

### REL-5. Make patches in flight from two panels compose instead of overwriting
- **Location:** `web/src/editor.ts:789`
- **Proof:**
  ```ts
  // editor.ts:785-791: the returned source replaces the document, whatever happened since
    /** Take `source`, a patch made on the canvas or in the inspector: one change, one step to undo. */
    const made = (source: string, edited: Edited) => {
      taken = { source, edited, showings };
      anchor();
      const changes = change(view.state.doc.toString(), source);
      view.dispatch({ changes, userEvent: "input.canvas", annotations: isolateHistory.of("full") });
    };
  // stage.ts:489: the patch is made from the source the caller read when it began
    make(source: string, ops: unknown[], index: number, format?: string): Promise<{ source: string; edited: Edited }> {
  ```
- **Verified:**
  ```
  $ git grep -n -E 'stage\.make\(' origin/main -- web/src
  origin/main:web/src/canvas.ts:2330:      const { source, edited } = await stage.make(editor.source(), ops, shown.index, editor.format());
  origin/main:web/src/canvas.ts:2347:      const { source, edited } = await stage.make(editor.source(), ops, shown.index, editor.format());
  origin/main:web/src/cue.ts:270:        const { source, edited } = await stage.make(editor.source(), [op], at.index, editor.format());
  origin/main:web/src/editor.ts:1057:      const { source, edited } = await stage.make(view.state.doc.toString(), ops, now.index, format());
  origin/main:web/src/editor.ts:1131:        const { source, edited } = await stage.make(view.state.doc.toString(), ops, shown, format());
  origin/main:web/src/editor.ts:2092:      const { source, edited } = await stage.make(view.state.doc.toString(), [op], now.index, f);
  origin/main:web/src/find.ts:133:      const { source, edited } = await stage.make(editor.source(), ops, next >= 0 ? next : shown.index, editor.format());
  origin/main:web/src/layers.ts:134:        const { source, edited } = await stage.make(editor.source(), patch, now.index, editor.format());
  origin/main:web/src/look.ts:777:      const { source, edited } = await stage.make(around.source(), ops, now.index, around.format());
  origin/main:web/src/look.ts:795:        const { source, edited } = await stage.make(around.source(), ops, now.index, around.format());
  origin/main:web/src/slides.ts:282:        const { source, edited } = await stage.make(editor.source(), ops, index, editor.format());
  origin/main:web/src/strip.ts:193:        const { source, edited } = await stage.make(editor.source(), ops, Math.max(0, to(slots)), editor.format());
  origin/main:web/src/writing.ts:194:      const { source, edited } = await stage.make(host.source(), taking.patch, now.index, host.format());
  ```
  The inspector applies results through `made` (editor.ts:811, `apply: made,`).
- **Do:** Give `Stage.make` one queue `(new)` for the whole editor. Each request waits for the previous patch to be applied, and the source is read when the request's turn comes (`make` takes the ops; the editor supplies the source then). Alternatively, `made` checks that `view.state.doc` still equals the source the patch was made from, and makes the same ops again on the current document when it doesn't.
- **Why:** Nine modules make patches from the source as they read it, each on its own queue, and `made` swaps in the result. With two in flight from different panels (an arrow nudge and an inspector choice, or a find-replace), the second silently undoes the first.
- **Effort:** M · **Impact:** M

## Security

### SEC-1. Escape theme slot names in the editor's Layout overlay
- **Location:** `web/src/canvas.ts:1352`
- **Proof:**
  ```ts
  // canvas.ts:84: the overlay's escape skips the double quote
  const escaped = (text: string) => text.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]!);
  // canvas.ts:1349-1353 and 1369: slot names go into the markup raw
        for (const slot of slotting.layout.slots) {
          const r = slotDrag?.slot.name === slot.name ? slotDrag.rect : slot.rect;
          const keyedHere = slotting.layout.slots[slotKeyed ?? -1]?.name === slot.name ? " keyed" : "";
          parts.push(rect(r, (slot.own ? "layout-slot own" : "layout-slot") + keyedHere, ` data-slot="${slot.name}"`));
          parts.push(`<text class="slot-name" x="${r[0] + 8 * u}" y="${r[1] + 18 * u}" font-size="${13 * u}">${slot.name}</text>`);
  …
      svg.innerHTML = parts.join("");
  ```
  The same happens at canvas.ts:1363 (`data-slot="${slot.name}" data-slot-edge=…`). The names come from the theme as written: `name: slot.clone(),` (scaena-engine guides.rs:133).
- **Verified:**
  ```
  $ git grep -n -E 'escaped\(|html\(' origin/main -- web/src/canvas.ts
  origin/main:web/src/canvas.ts:1201:          `<text class="waiting-words" data-wait="${i}" x="${x + width / 2}" y="${y + height / 2}" text-anchor="middle" dominant-baseline="middle" font-size="${13 * u}">${escaped(w.words)}</text>`,
  origin/main:web/src/canvas.ts:1299:      const said = escaped(coded.code.said);
  origin/main:web/src/canvas.ts:1302:        `<g class="code-chip" data-chip="${escaped(coded.node)}" role="button" aria-label="Language: ${said}"><rect x="${x + w - cw}" y="${y - ch - 4 * u}" width="${cw}" height="${ch}" rx="${4 * u}"/><text x="${x + w - cw / 2}" y="${y - ch / 2 - 4 * u}" text-anchor="middle" dominant-baseline="middle" font-size="${12 * u}">${said}</text><title>${said}: a press changes the language</title></g>`,
  $ git grep -n -E 'propertyNames' origin/main -- docs/schema/theme.schema.json
  # (no matches)
  $ git grep -n -E 'localStorage' origin/main -- web/src/assistant/keys.ts
  origin/main:web/src/assistant/keys.ts:44:  const stored = localStorage.getItem(item(provider));
  origin/main:web/src/assistant/keys.ts:52:    localStorage.removeItem(item(provider));
  origin/main:web/src/assistant/keys.ts:63:  localStorage.setItem(item(provider), JSON.stringify({ iv: base64(iv), data: base64(data) }));
  origin/main:web/src/assistant/keys.ts:69:  localStorage.removeItem(item(provider));
  ```
  The other modules' escapers include `"` (e.g. look.ts:132: `s.replace(/[&<>"]/g, …)`).
- **Do:**
  - Add `"` to `escaped` (canvas.ts:84), and use it on `slot.name` at :1352, :1353 and :1363.
  - Add a test `(new)` that opens a theme with a slot named `<image href=x onerror=alert(1)>"` and presses Layout, asserting the overlay shows it as text.
- **Why:** A theme's slot names are free text, a shared bundle carries its own theme, and the overlay writes them into `svg.innerHTML` unescaped. Opening such a deck's Layout view runs its author's script on the editor's origin, where the assistant's keys live: keys.ts:4 says "any script this page runs can still use it".
- **Effort:** S · **Impact:** L

### SEC-2. Follow only http, https, and mailto links
- **Location:** `crates/scaena-engine/src/render.rs:1407`
- **Proof:**
  ```rust
  // render.rs:1406-1408: any href becomes a link
                  let link = run.get("link").and_then(|l| match (l.get("href"), l.get("state")) {
                      (Some(Value::String(href)), _) => Some(LinkTarget::Href(href.clone())),
                      (_, Some(Value::String(state))) => Some(LinkTarget::State(state.clone())),
  // reading.rs:447, then 288-289: and the reader writes it as an anchor
                      (Some(Value::String(h)), _) => Some(LinkTarget::Href(h.clone())),
  …
                  Some(LinkTarget::Href(href)) => {
                      let _ = write!(out, r#"<a href="{}">{said}</a>"#, attr(href));
  ```
  ```ts
  // web/src/player.ts:579-580
      if ("href" in link) {
        open(link.href, "_blank", "noopener");
  ```
  ```swift
  // Presenting.swift:531-534
          case .href(let href):
              guard let url = URL(string: href) else { return false }
              #if os(macOS)
              return NSWorkspace.shared.open(url)
  ```
  The rule exists only as a schema pattern: `#[schemars(regex(pattern = "^(https?://[^\\s]+|mailto:[^\\s]+)$"))]` (model/values.rs:538). The players open decks with `read_deck` (scaena-session lib.rs:277-283), which checks the format version only.
- **Verified:**
  ```
  $ git grep -n -E 'https?:|mailto|javascript:|scheme' origin/main -- crates/scaena-core/src/validate.rs crates/scaena-core/src/reading.rs crates/scaena-engine/src/render.rs web/src/player.ts web/src/reading.ts apps/mac/ScaenaKit/Sources/Scaena/Presenting.swift
  origin/main:crates/scaena-core/src/reading.rs:661:            "credit": [{ "text": "© OpenStreetMap", "link": "https://www.openstreetmap.org/copyright" }, { "text": " · Geoapify" }],
  origin/main:crates/scaena-core/src/reading.rs:689:                r#"<p data-node="m" data-credit=""><a href="https://www.openstreetmap.org/copyright">© OpenStreetMap</a> · Geoapify</p>"#,
  origin/main:crates/scaena-core/src/reading.rs:708:                        "credit": [{ "text": "Weather data by Open-Meteo.com", "link": "https://open-meteo.com/" }],
  ```
  These are test fixtures, not checks.
- **Do:**
  - Add one allowlist `(new)`, e.g. `scaena_core::links::followable(href)`, true for `http:`, `https:` and `mailto:` only.
  - Apply it wherever a `LinkTarget::Href` is made (render.rs:1407, reading.rs:447, and the credits' links), so anything else draws and reads as plain text.
  - Check it again in the clients' follow: player.ts:579 and Presenting.swift:531.
- **Why:** A shared deck's `javascript:` link becomes a live anchor in the player's reading region, on the site's origin. On the Mac, a `file:`, `smb:` or `shortcuts:` link opens with one click mid-show.
- **Effort:** S · **Impact:** L

### SEC-3. Refuse deck ids that would write exports outside `--out`
- **Location:** `crates/scaena-ops/src/export.rs:582`
- **Proof:**
  ```rust
  // export.rs:339-340: states are checked for membership, not form
  fn named(deck: &scaena_core::Deck, states: Option<&[String]>) -> Result<Vec<String>, OpsError> {
      let Some(states) = states else { return Ok(deck.states.iter().map(|s| s.id.clone()).collect()) };
  // export.rs:582-583: and a state id names the file
          let file = dir.join(format!("{state}.{ext}"));
          write(&file, &bytes)?;
  // export.rs:396, then 407: as a beat id and a format name the spine's renders
          entry.thumbnail = Some(draw(&list, scale, format!("renders/{beat}.png"))?);
  …
              let name = format!("renders/{beat}@{}.png", format.replace(':', "x"));
  ```
  The spine's `draw` runs `create_dir_all` on the joined path's parent (export.rs:381-384).
- **Verified:**
  ```
  $ git grep -n -E 'validate|is_valid_id' origin/main -- crates/scaena-ops/src/export.rs
  # (no matches)
  $ git grep -n -E 'join\(format!\(' origin/main -- crates/scaena-ops/src crates/scaena-export/src
  origin/main:crates/scaena-export/src/video.rs:694:        let dir = std::env::temp_dir().join(format!("scaena-video-{}", std::process::id()));
  origin/main:crates/scaena-export/src/video.rs:721:        let dir = std::env::temp_dir().join(format!("scaena-video-twice-{}", std::process::id()));
  origin/main:crates/scaena-export/src/video.rs:753:        let dir = std::env::temp_dir().join(format!("scaena-video-fails-{}", std::process::id()));
  origin/main:crates/scaena-ops/src/export.rs:582:        let file = dir.join(format!("{state}.{ext}"));
  origin/main:crates/scaena-ops/src/narrative.rs:3906:            std::fs::read_to_string(root().join(format!("crates/scaena-narrato/tests/answers/{name}.json"))).unwrap()
  $ git grep -n -E 'pub fn is_valid_id' origin/main -- crates/scaena-core/src/ids.rs
  origin/main:crates/scaena-core/src/ids.rs:4:pub fn is_valid_id(s: &str) -> bool {
  ```
- **Do:** Before an export names a file after a deck id, refuse any state or beat id that fails `scaena_core::ids::is_valid_id` (ids.rs:4, the rule E105 checks), and any format the deck does not list. Do it in `images()` (export.rs:582) and in the spine's renders (:396, :407), with an error that names the id.
- **Why:** `scaena export` and the MCP server's export run on bundles nobody validated, and `Path::join` takes `../x` or an absolute id as written. A hostile deck therefore writes PNG and SVG files outside `--out`, and the spine export creates the directories on the way.
- **Effort:** S · **Impact:** M

### SEC-4. Give the site's pages a Content-Security-Policy
- **Location:** `web/editor.html:3`
- **Proof:**
  ```html
  <!-- web/editor.html:3-7: the editor's head (index.html:3-7 is the same shape); no CSP -->
    <head>
      <meta charset="utf-8" />
      <meta name="viewport" content="width=device-width, initial-scale=1" />
      <title>Scaena editor</title>
      <style>
  <!-- web/standalone.html:6-8: the single-file export has one -->
      <meta
        http-equiv="Content-Security-Policy"
        content="default-src 'none'; script-src 'unsafe-inline' 'wasm-unsafe-eval'; worker-src blob:; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'"
  ```
- **Verified:**
  ```
  $ git grep -n -E 'Content-Security-Policy' origin/main -- web
  origin/main:web/standalone.html:7:      http-equiv="Content-Security-Policy"
  $ git grep -n -E '<script|onclick=|onload=' origin/main -- web/editor.html web/index.html
  origin/main:web/editor.html:1118:    <script type="module" src="/src/editor.ts"></script>
  origin/main:web/index.html:25:    <script type="module" src="/src/main.ts"></script>
  ```
  The pages load external module scripts only, so `script-src 'self'` fits them. `scaena serve`'s `answer` (scaena-serve lib.rs:402-410) sets Content-Type, Cache-Control and nosniff, and no CSP.
- **Do:**
  - Add a CSP `<meta>` to index.html and editor.html at build time, through a Vite `transformIndexHtml` hook `(new)` in vite.config.ts beside the `offline` plugin. Along the lines of: `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self' blob:; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self' https:; object-src 'none'; base-uri 'none'`.
  - Send the same as a header from `answer`.
  - Run the a11y and smoke walks to catch anything it blocks.
- **Why:** The pages hold API keys any page script can use, take decks from any CORS host (`?bundle=`), and write engine and theme strings through `innerHTML` in many places. SEC-1 is one such slip; with a `script-src` that has no `'unsafe-inline'`, the next one is a blocked handler instead of a stolen key.
- **Effort:** S · **Impact:** M

### SEC-5. Harden Remote pairing against guessing the four-digit code
- **Location:** `apps/mac/ScaenaKit/Sources/ScaenaRemote/Remote.swift:20`
- **Proof:**
  ```swift
  // Remote.swift:19-20, 31, 35-36: the whole TLS key is HMAC of a 4-digit code, PSK only
      public static func code() -> String {
          String(format: "%04d", Int.random(in: 0..<10_000))
  …
          let mac = HMAC<SHA256>.authenticationCode(for: identity, using: SymmetricKey(data: Data(code.utf8)))
  …
          sec_protocol_options_append_tls_ciphersuite(
              tls.securityProtocolOptions, tls_ciphersuite_t(rawValue: UInt16(TLS_PSK_WITH_AES_128_GCM_SHA256))!)
  // RemoteServer.swift:69-71 and 79-81: every connection is kept, and a failed handshake is simply forgotten
      private func join(_ connection: NWConnection) {
          let id = ObjectIdentifier(connection)
          remotes[id] = connection
  …
                  case .failed, .cancelled:
                      self.remotes[id] = nil
                      self.joined(self.count)
  ```
- **Verified:**
  ```
  $ git grep -n -E 'attempt|approve|pending|failures|rotate|ecdhe|PAKE' origin/main -- apps/mac/ScaenaKit/Sources/ScaenaRemote apps/mac/ScaenaKit/Sources/Scaena/RemoteHosting.swift
  # (no matches)
  ```
  RemoteHosting.swift:49 advertises `let name = String("\(Self.device): \(deck)".prefix(60))` over Bonjour.
- **Do:**
  - In `RemoteServer.join`, count handshakes that fail before `.ready`. After about five, rotate the code (`Remote.code()`), restart the listener, and show the new code.
  - Have the presenter approve each new remote in the sheet before places and commands flow.
  - Advertise the device name only, cut to Bonjour's 63 UTF-8 bytes.
  - Longer term: a longer code, or pair once and keep a random key.
- **Why:** A PSK-only handshake over 10⁴ keys can be brute-forced offline from one recorded join, or online with unlimited tries. Anyone on the venue's Wi-Fi or AWDL could then read the speaker's notes and slides and drive or end the show. The Bonjour name also tells the room the deck's file name.
- **Effort:** M · **Impact:** M

## Usability

### USE-1. Stop the Mac and iPad apps from claiming every .zip file
- **Location:** `apps/mac/Scaena/Info.plist:57`
- **Proof:**
  ```xml
  <!-- apps/mac/Scaena/Info.plist:52-62 (apps/ipad/Info.plist:26-36 is the same) -->
  			<key>CFBundleTypeName</key>
  			<string>Zipped Scaena Deck</string>
  			<key>CFBundleTypeRole</key>
  			<string>Editor</string>
  			<key>LSHandlerRank</key>
  			<string>Owner</string>
  			<key>LSItemContentTypes</key>
  			<array>
  				<string>com.scaena.deck.zip</string>
  				<string>public.zip-archive</string>
  			</array>
  ```
- **Verified:**
  ```
  $ git grep -n -E 'public.zip-archive|LSHandlerRank' origin/main -- apps/mac/Scaena/Info.plist apps/ipad/Info.plist
  origin/main:apps/ipad/Info.plist:16:			<key>LSHandlerRank</key>
  origin/main:apps/ipad/Info.plist:30:			<key>LSHandlerRank</key>
  origin/main:apps/ipad/Info.plist:35:				<string>public.zip-archive</string>
  origin/main:apps/ipad/Info.plist:123:				<string>public.zip-archive</string>
  origin/main:apps/mac/Scaena/Info.plist:42:			<key>LSHandlerRank</key>
  origin/main:apps/mac/Scaena/Info.plist:56:			<key>LSHandlerRank</key>
  origin/main:apps/mac/Scaena/Info.plist:61:				<string>public.zip-archive</string>
  origin/main:apps/mac/Scaena/Info.plist:102:				<string>public.zip-archive</string>
  ```
  Lines 102 and 123 are `UTTypeConformsTo` for `com.scaena.deck.zip`, which is correct. Lines 61 and 35 are the claims. `ScaenaDocument.readableContentTypes` lists `.zip` (ScaenaDocument.swift:24).
- **Do:**
  - Take `public.zip-archive` out of the Owner-ranked document type in both plists, keeping `com.scaena.deck.zip`.
  - If plain zips should still open, give them their own document type with `LSHandlerRank` `Alternate`. Keep `.zip` readable for Open With and drops.
- **Why:** Ranked as Owner of every zip, the app can take over double-clicks on unrelated archives in Finder and Files. Apple reserves Owner for types the app defines.
- **Effort:** S · **Impact:** M

### USE-2. Keep a rehearsal's timings safe from a second Escape
- **Location:** `web/src/rehearse.ts:196`
- **Proof:**
  ```ts
  // rehearse.ts:178: Escape stops the run and opens the results (showModal at :155) …
      else if (e.key === "Escape") stop();
  // rehearse.ts:195-196: … where Escape again closes the dialog without "keep"
    listen(dialog, "close", async () => {
      if (dialog.returnValue !== "keep") return editor.say("Rehearsal: nothing kept");
  ```
- **Verified:**
  ```
  $ git grep -n -E 'oncancel|"cancel"' origin/main -- web/src/rehearse.ts
  # (no matches)
  $ git grep -n -E 'showModal|function stop' origin/main -- web/src/rehearse.ts
  origin/main:web/src/rehearse.ts:135:  function stop() {
  origin/main:web/src/rehearse.ts:155:    dialog.showModal();
  ```
- **Do:**
  - Listen for the dialog's `cancel` event and `preventDefault()` it, or ask "Discard the timings?".
  - Keep `kept` after the dialog closes, and add a palette command "Rehearsal results…" `(new)` that reopens the table.
- **Why:** Escape is the reflex to end a run-through. Pressed twice, it discards a whole rehearsal's timings with no way back, and the run has to be redone.
- **Effort:** S · **Impact:** M

### USE-3. Replace selects that act on every change with real menus
- **Location:** `web/src/editor.ts:1346`
- **Proof:**
  ```ts
  // editor.ts:1346-1351: Insert inserts on `change`, then resets to "Insert…"
    inserter.onchange = () => {
      const chosen = inserter.selectedOptions[0];
      inserter.value = "";
      if (!chosen?.value) return;
      if (chosen.value === "icon") return void insertIcon();
      if (chosen.value === MAKE_PICTURE) return void picturing.open();
  // editor.ts:1381-1382: the theme picker re-themes on `change`
    themePicker.onchange = async () => {
      const chosen = themePicker.value;
  // cue.ts:444-446: Add an animation, the same
    adder.onchange = () => {
      const chosen = adder.value;
      adder.value = "";
  ```
- **Verified:**
  ```
  $ git grep -n -E '(inserter|themePicker|adder)\.(onkeydown|addEventListener)' origin/main -- web/src
  # (no matches)
  $ git grep -n -E 'export function menu|export const menu' origin/main -- web/src/commands.ts
  origin/main:web/src/commands.ts:190:export function menu(x: number, y: number, items: Command[], back: HTMLElement | null) {
  ```
- **Do:**
  - Make Insert and Add an animation buttons that open `menu()` (commands.ts:190) with the same items.
  - Make Theme a button that opens a picker dialog `(new)` and re-themes only on an explicit choice.
- **Why:** In Chromium on Windows and Linux, ↑/↓ on a focused closed select fires `change` at each press. Browsing Insert inserts an object with every arrow (the reset to "Insert…" brings the same item back), and browsing Theme re-themes the deck at every key, against WCAG 3.2.2.
- **Effort:** M · **Impact:** M

### USE-4. Say what Undo and Redo took back
- **Location:** `web/src/editor.ts:1294`
- **Proof:**
  ```ts
  // editor.ts:1294-1296: the page-wide ⌘Z/⌘⇧Z, silent
      if (key === "y" || e.shiftKey) {
        if (redo(view)) forceLinting(view);
      } else if (undo(view)) forceLinting(view);
  // editor.ts:2364: the palette calls it plain "Undo"
        { label: "Undo", keys: `${MOD}Z`, group: "Edit", applies: () => undoDepth(view.state) > 0 && !assisting, run: () => undo(view) && forceLinting(view) },
  ```
- **Verified:**
  ```
  $ git grep -n -E 'Undone' origin/main -- web/src/data.ts web/src/files.ts web/src/editor.ts
  origin/main:web/src/data.ts:299:        editor.say(redo ? `Made again: the last change to ${name}` : `Undone: the last change to ${name}`);
  origin/main:web/src/files.ts:174:        editor.say(`${redo ? "Made again" : "Undone"}: ${fileName(path)} ${out ? "taken out of the deck" : "put back"}`);
  $ git grep -n -E 'invertedEffects' origin/main -- web/src/editor.ts
  origin/main:web/src/editor.ts:37:  invertedEffects,
  origin/main:web/src/editor.ts:466:const rewrites = invertedEffects.of((tr) =>
  origin/main:web/src/editor.ts:473:const takes = invertedEffects.of((tr) => tr.effects.filter((e) => e.is(fileTaken)).map((e) => fileTaken.of({ ...e.value, out: !e.value.out })));
  ```
  The words already exist where each change is made: canvas.ts:2333 (`editor.say(done ?? …placed…)`) and look.ts:780 (`` around.say(`${name}: ${given}…`) ``).
- **Do:**
  - Carry each change's words in a `described` StateEffect `(new)` on the transaction `made` dispatches, mirrored through `invertedEffects` as `filesWritten` and `fileTaken` are (editor.ts:464-473).
  - On undo and redo, read it back and `say("Undone: …")` / `say("Made again: …")`.
  - Label the palette entries "Undo <what>" and "Redo <what>".
- **Why:** The Data and Files panels say what an undo did; the source history, which holds every canvas and inspector edit, says nothing. The Mac already names each step in its Edit menu.
- **Effort:** M · **Impact:** M

### USE-5. Give the Mac's menus macOS and Keynote conventions
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/NodeCommands.swift:170`
- **Proof:**
  ```swift
  // NodeCommands.swift:169-173: Group on ⌘G, Ungroup on ⇧⌘G
              Button("Group") { deck?.group?() }
                  .keyboardShortcut("g", modifiers: .command)
                  .disabled(deck?.group == nil)
              Button("Ungroup") { deck?.ungroup?() }
                  .keyboardShortcut("g", modifiers: [.command, .shift])
  // CanvasViews.swift:126-127: Find lives under View, on ⇧⌘F
              Button("Find in Deck…") { canvas?.find() }
                  .keyboardShortcut("f", modifiers: [.command, .shift])
  // ScaenaApp.swift:57-63: the app's command groups
          .commands {
              #if os(macOS)
              PromptCommands()
              #endif
              NodeCommands()
              ViewCommands()
              WindowCommands()
  ```
- **Verified:**
  ```
  $ git grep -n -E 'TextEditingCommands|TextFormattingCommands|CommandGroup\(replacing: \.importExport|\.textEditing' origin/main -- apps/mac/ScaenaKit/Sources/Scaena
  # (no matches)
  ```
- **Do:**
  - Move Group and Ungroup to ⌥⌘G and ⇧⌥⌘G, which are Keynote's.
  - Add `TextEditingCommands()` with an Edit › Find item wired to `canvas?.find()`.
  - Add a Format menu (`CommandMenu("Format")` `(new)`) for Bold and Italic.
  - Add File › Export To through `CommandGroup(replacing: .importExport)` `(new)`.
- **Why:** On a Mac, ⌘G is Find Next everywhere. Keynote users press ⌥⌘G to group, look for Find under Edit and Bold under Format, and Help-menu search can't find commands that aren't in the menus.
- **Effort:** M · **Impact:** M

## Attractiveness / Sexiness

### ATTR-1. Color-manage the Mac and iPad canvas as sRGB
- **Location:** `apps/mac/ScaenaKit/Sources/ScaenaKit/ScaenaView.swift:106`
- **Proof:**
  ```swift
  // ScaenaView.swift:105-109 (macOS) and 141-142 (iOS): no colorspace on the Metal layer
      public override func makeBackingLayer() -> CALayer {
          let layer = CAMetalLayer()
          layer.pixelFormat = .bgra8Unorm
          return layer
      }
  …
      private func made() {
          metalLayer?.pixelFormat = .bgra8Unorm
  // Editing.swift:602, then 606: while thumbnails are tagged sRGB
      /// The pixels as an image: straight-alpha sRGB, as the painter gives them.
  …
              let space = CGColorSpace(name: CGColorSpace.sRGB)
  ```
- **Verified:**
  ```
  $ git grep -n -E 'colorspace|CGColorSpace' origin/main -- apps/mac/ScaenaKit/Sources/ScaenaKit/ScaenaView.swift crates/scaena-paint/src/lib.rs
  # (no matches)
  $ git grep -n -E 'layerClass|func made\(|CAMetalLayer\(' origin/main -- apps/mac/ScaenaKit/Sources/ScaenaKit/ScaenaView.swift
  origin/main:apps/mac/ScaenaKit/Sources/ScaenaKit/ScaenaView.swift:106:        let layer = CAMetalLayer()
  origin/main:apps/mac/ScaenaKit/Sources/ScaenaKit/ScaenaView.swift:129:    public override class var layerClass: AnyClass { CAMetalLayer.self }
  origin/main:apps/mac/ScaenaKit/Sources/ScaenaKit/ScaenaView.swift:141:    private func made() {
  ```
- **Do:** Set `layer.colorspace = CGColorSpace(name: CGColorSpace.sRGB)` in `makeBackingLayer()` (:105), and the same on `metalLayer` in the iOS `made()` (:141). Compare a theme's accent on a P3 display against the thumbnail and the exported PDF.
- **Why:** A Metal layer with no color space is not color-matched. On the P3 displays every current Mac and iPad has, the canvas can show the theme's colors more saturated than the thumbnails, the light table, the PNG and the PDF show them. A brand color that changes between views looks wrong.
- **Effort:** S · **Impact:** M

### ATTR-2. Let the browser player fill the screen at full sharpness
- **Location:** `web/src/worker.ts:1326`
- **Proof:**
  ```ts
  // worker.ts:1325-1326: the canvas is the deck's size
  /** The canvas frames are laid out on now, in whole pixels. */
  const size = () => Array.from(player.canvasSize(), Math.round) as [number, number];
  ```
  ```css
  /* player.css:7-9: shown at that size, only ever scaled down */
  /* At its own size where it fits, scaled down to fit where it does not, and in the middle of the
   * room the bar leaves, a window of another shape than the slide's showing it whole. */
  #stage { display: block; margin: auto; max-width: 100%; max-height: 100%; touch-action: none; }
  ```
- **Verified:**
  ```
  $ git grep -n -E 'devicePixelRatio|devicePixelContentBoxSize|object-fit' origin/main -- web/src/player.ts web/src/worker.ts web/src/player.css web/src/stage.ts
  origin/main:web/src/player.css:32:.presenter #upnext { display: block; width: 100%; height: auto; max-height: 35vh; object-fit: contain; opacity: 0.85; }
  $ git grep -n -E 'pub fn resize' origin/main -- crates/scaena-wasm/src/lib.rs
  origin/main:crates/scaena-wasm/src/lib.rs:1799:        pub fn resize(&mut self, width: u32, height: u32) {
  ```
  `Player.paint` already scales to the canvas: `let scale = width as f32 / dl.viewport[0];` (scaena-wasm lib.rs:1885).
- **Do:**
  - On the player page and the presenter view, size the canvas's backing store to its displayed box times `devicePixelRatio`, with a ResizeObserver on `devicePixelContentBoxSize`, through `Canvas.resize` (lib.rs:1799).
  - Show it at `width/height: 100%` with `object-fit: contain`.
  - Cap the CPU painter's raster at `MAX_PIXELS`.
- **Why:** A 1920×1080 deck in fullscreen sits small in a black frame on a 2560-wide display, and on a Retina screen it is upscaled, so text is soft. That is what the audience sees.
- **Effort:** M · **Impact:** L

### ATTR-3. Dissolve one photo into another without dipping toward the background
- **Location:** `crates/scaena-engine/src/sample.rs:1133`
- **Proof:**
  ```rust
  // sample.rs:1129-1134: both sides fade, one over the other
                  Track::Crossfade { from: i, to: j } => {
                      let (Some(seen), _) = self.seen(&to[*j].id, false, false, t_ms) else { continue };
                      if moving {
                          let rest = self.from.as_ref().map_or(Seen::REST, |s| s.rest(&from[*i].id));
                          push(&mut dl, &from[*i], from[*i].opacity * (1.0 - p), time, rest);
                          self.put(&mut dl, &to[*j], seen, to[*j].opacity * p, false, t_ms);
  // images.rs:563-567: the pattern that avoids it for adjustments
          let images = match self.under {
              None => vec![drawn(self.adjust)],
              Some((under, p)) => {
                  let below = if self.info.photo { 1.0 } else { 1.0 - p };
                  vec![faded(drawn(under), below), faded(drawn(self.adjust), p)]
  ```
  An exit and an enter of two nodes fade the same way (sample.rs:1115 and :1121).
- **Verified:**
  ```
  $ git grep -n -E 'photo|opaque' origin/main -- crates/scaena-engine/src/sample.rs
  origin/main:crates/scaena-engine/src/sample.rs:697:/// anything in it. A group that changes nothing (opaque, at rest) adds no layer.
  origin/main:crates/scaena-engine/src/sample.rs:2045:        // Each mark at this frame, and how opaque, by key, for the paths through them.
  ```
  Neither match is in the transition's fades: no photo or opacity test guards the crossfade.
- **Do:** Where the entering node hides what lies under it (an opaque photo over the same box), draw the leaving node at full opacity and fade only the entering one, as images.rs:566 does. Apply this in the crossfade branch and in an exit/enter pair over the same box. Re-bless and review the transition goldens.
- **Why:** With both layers faded and drawn one over the other, a full-bleed photo swap shows 25% of the background at the midpoint, a visible flash Keynote's Dissolve doesn't have.
- **Effort:** M · **Impact:** M

### ATTR-4. Fit the editor's panel tabs on one row, Keynote-style
- **Location:** `web/editor.html:373`
- **Proof:**
  ```html
  <!-- editor.html:25 and 373: a panel 280–420 px wide, whose tabs wrap -->
        main.folded { grid-template-columns: minmax(0, 1fr) clamp(280px, 28vw, 420px); }
  …
        #tabs { display: flex; flex-wrap: wrap; flex: none; gap: 0 4px; padding: 0 12px; border-bottom: 1px solid var(--line); }
  <!-- editor.html:761, then 768: eight text tabs, first and last (all eight are in the grep below) -->
            <button role="tab" id="tab-inspector" aria-selected="true" aria-controls="inspector">Inspector</button>
  …
            <button role="tab" id="tab-assistant" aria-selected="false" aria-controls="assistant">Assistant</button>
  ```
- **Verified:**
  ```
  $ git grep -n -E '#tabs \{|#right \{|grid-template-columns:.*clamp|role="tab"' origin/main -- web/editor.html
  origin/main:web/editor.html:25:      main.folded { grid-template-columns: minmax(0, 1fr) clamp(280px, 28vw, 420px); }
  origin/main:web/editor.html:373:      #tabs { display: flex; flex-wrap: wrap; flex: none; gap: 0 4px; padding: 0 12px; border-bottom: 1px solid var(--line); }
  origin/main:web/editor.html:761:          <button role="tab" id="tab-inspector" aria-selected="true" aria-controls="inspector">Inspector</button>
  origin/main:web/editor.html:762:          <button role="tab" id="tab-layers" aria-selected="false" aria-controls="layers">Layers</button>
  origin/main:web/editor.html:763:          <button role="tab" id="tab-data" aria-selected="false" aria-controls="data">Data</button>
  origin/main:web/editor.html:764:          <button role="tab" id="tab-theming" aria-selected="false" aria-controls="theming">Theme</button>
  origin/main:web/editor.html:765:          <button role="tab" id="tab-files" aria-selected="false" aria-controls="files">Files</button>
  origin/main:web/editor.html:766:          <button role="tab" id="tab-versions" aria-selected="false" aria-controls="versions">Versions</button>
  origin/main:web/editor.html:767:          <button role="tab" id="tab-story" aria-selected="false" aria-controls="story">Story</button>
  origin/main:web/editor.html:768:          <button role="tab" id="tab-assistant" aria-selected="false" aria-controls="assistant">Assistant</button>
  ```
- **Do:** Put a three-part segmented control `(new)` at the head of the panel: Format, Animate and Slide, the Mac's inspector tabs (PLAN 3.18). Move Layers, Data, Theme, Files, Versions, Story and Assistant under a More menu `(new)` opened with `menu()` (commands.ts:190). Keep the tab roles, ids and roving focus, so the walks and axe still find them.
- **Why:** Eight text tabs need well over 420 px, so they always wrap into two or more ragged rows above the inspector. That is the first thing that reads as unfinished next to Keynote's three tidy tabs.
- **Effort:** M · **Impact:** M

### ATTR-5. One grouped icon toolbar for the editor, as the Mac app has
- **Location:** `web/editor.html:733`
- **Proof:**
  ```html
  <!-- editor.html:668-670: a File row of text buttons (669-680 hold 12 controls) -->
          <nav id="bundle" aria-label="File">
            <button type="button" id="source-toggle" aria-controls="source" aria-pressed="false" title="The deck's source as .scn, beside the canvas (⌘\)">Source</button>
            <button id="new-deck" title="A new deck in a built-in theme">New…</button>
  <!-- editor.html:733-736: and a paragraph of controls under the strip (734-752 hold 4 selects and 12 buttons) -->
          <p>
            <select id="state" aria-label="Slide"></select>
            <select id="format" aria-label="Size"><option value="">Own size</option></select>
            <button type="button" id="slides-open" aria-pressed="false" aria-controls="slides" title="The light table: every slide, to move, copy, and take out across the deck (Escape closes it)">Slides</button>
  ```
- **Verified:**
  ```
  $ git grep -n -E 'role="toolbar"' origin/main -- web/editor.html
  # (no matches)
  $ git grep -n -E '^export ' origin/main -- web/src/icon-set.ts
  origin/main:web/src/icon-set.ts:10:export interface IconEntry {
  origin/main:web/src/icon-set.ts:18:export interface IconSet {
  origin/main:web/src/icon-set.ts:23:export type Weight = "regular" | "fill";
  origin/main:web/src/icon-set.ts:26:export interface Icon {
  origin/main:web/src/icon-set.ts:35:export function iconSet(): Promise<IconSet> {
  origin/main:web/src/icon-set.ts:53:export function rank(asked: string[], icon: IconEntry): number | undefined {
  origin/main:web/src/icon-set.ts:68:export function findIcons(set: IconSet, query: string): IconEntry[] {
  ```
- **Do:**
  - Replace both rows with one top `role="toolbar"` `(new)` of grouped icon-and-label buttons, mirroring the Mac's PLAN 3.18 toolbar (View, Zoom, Add Slide, Play, Text, Shape, Image, Chart, Table, Share, Format/Animate/Document). Draw the icons from the set that already ships (`iconSet()`, icon-set.ts:35).
  - Move the File actions under a File menu opened with `menu()`.
  - Keep every id, so the walks keep working.
- **Why:** Some 28 controls in two wrapping rows of plain text, the view controls stranded under the strip, read as a developer tool. The Mac app already presents the same commands the way a presentation app does.
- **Effort:** L · **Impact:** M

---

## First move

**SEC-1. Escape theme slot names in the editor's Layout overlay** (from Security)

This is the one finding where opening someone else's deck can hand them the user's API keys.
- A theme's slot names are free text, and a shared bundle carries its own theme.
- The Layout view writes those names into `svg.innerHTML` raw, on the same origin that keeps the assistant's keys, which keys.ts says any page script can use.

The fix is the smallest in the plan: one character class in `escaped` and three interpolations. It touches no format, no golden, no schema and no Rust, and it needs no design call. It is a fix, not a feature, so it can land as soon as Jay says so.

It also unblocks SEC-4. A CSP is only safe to tighten once the known injection is closed, and the SEC-1 test becomes the first check that the CSP doesn't break Layout. Nothing else in the plan pairs that severity with that little effort.

## Dropped during verification

- **"Move the Mac app and Remote targets to Swift 6 language mode"**: decided and documented. Package.swift:23-25 explains why those two targets build in Swift 5 mode.
- **"First-letter slices `x[..1]` panic on non-ASCII text"** (words.rs:969, health.rs:453, narrative.rs:3126): the cited code is safe. Each slices a fixed English string built just above it: `format!("a {} pt …")`, a join of fixed phrases, and a matched `"since" | "after" | "from"`. Byte 1 is always a char boundary.
- **"`fetched::date` slices `&at[..10]` unchecked"** (fetched.rs:339): guarded. `instant(at)` first requires at least 20 bytes, with ASCII digits in place (fetched.rs:344-349).
- **"Lint's `dated` panics on any short first date"**: narrowed. When `days()` parses a date, its first ten bytes are ASCII (fetched.rs:417), so only a date `days()` refuses, with the fetch day passed, reaches the slice. Kept as STAB-5 with that trigger.
- **"⌘G collides with the Mac's Find Next"**: narrowed. The app adds no Edit › Find menu, so no menu item actually collides. What remains is the convention and Keynote's ⌥⌘G, which USE-5 keeps.
- **"The display link keeps the Metal view and its session alive after SwiftUI dismantles it"** (Mac survey): unproven. Only `viewDidMoveToWindow` → `attach(nil)` was read, not the representables' dismantle path. PERF-2 keeps only the pausing.
- **"The offline site breaks because the host caches HTML for 600 s"**: narrowed. A host's headers can't be read from the repo, so REL-2 rests only on what the code does: `addAll` through the HTTP cache, `cache.put` inside the try, and no update path.
- **"Automatic optical size uses canvas units"** (text.rs:1102, where `role.opsz.unwrap_or(role.size)` meets SPEC.md:1952's "2 units to the point"): confirmed in code, but moved to Deferred. Whether slides should get display optical sizes is a design call, and it changes the goldens.

## Deferred

- **Held slides repaint every frame.** The browser player repaints a held slide at every display frame though nothing moves (worker.ts:1844-1863). The reduced-motion path already waits with `setTimeout` (:1862). Cut for room; it needs a "draws a shader at rest" query.
- **The strip rebuilds on every compile.** Every item's markup and canvas is recreated each time (strip.ts:92-103). PERF-4 removes the heavier half.
- **Browser saves rewrite every file.** Each save writes the whole bundle (worker.ts:1194-1197), which is slow for photo-heavy decks on folders and OPFS.
- **Optical size from canvas units.** text.rs:1102; see the last drop. Jay's design call.
- **Mac paint errors go nowhere.** They land in `ScaenaView.failure` (ScaenaView.swift:77, :242), which nothing in the app reads, so a blank canvas says nothing.
- **No retry on Narrato's slides request.** narrative.rs:424-428 has none, though pictures retry (`TRIES`, :136). The chat also reads a provider's refusal as `Stop::Other` (scaena-chat providers.rs:340-345).
- **`write_deck` has no changed-since-read check.** It writes without checking that the deck changed since it was read (scaena-ops lint.rs:283-296), so an MCP or CLI edit and the editor's save can overwrite each other.
- **The zip reader trusts declared sizes.** It reads entries unbounded (scaena-store lib.rs:325-326), so a zip bomb exhausts memory on open.
- **The browser fetches any path a deck names.** Absolute URLs included (worker.ts:936); the native store refuses them (`normal()`, scaena-store lib.rs:347-352). A privacy leak on open.
- **Bundle skills outrank the general ones.** A bundle's skills go into the assistant's prompt "to be followed over the general ones" (scaena-chat prompt.rs:140-148). Prompt injection from a shared deck, limited to the open bundle.
- **The store's folder walk follows symlinked directories** (scaena-store lib.rs:333-344), so a folder bundle can pull outside files into a saved zip.
- **Hardware key codes are logged** at debug level, including while the Keychain sheet is up (CanvasTextInput.swift:934-939).
- **No privacy manifest.** No `PrivacyInfo.xcprivacy` is in the tree, while the apps use `@AppStorage` (AssistantPanel.swift:13). App Store Connect flags that at the first upload. Tooling.
- **Main pushes skip macOS and iPad CI.** The `changes` job turns those jobs off for pushes to main (ci.yml:80). Tooling.
- **A panicked MCP export leaves its job entry behind** (scaena-mcp lib.rs:801-816). Rare.
