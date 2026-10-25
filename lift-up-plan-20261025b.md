# Lift-up plan: the whole product, narrative and AI-first first (2026-10-25b)

> Platform: mixed. A Rust core; a web editor and player (Vite + TS, the engine as WASM in a worker); a SwiftUI Mac and iPad client over a C ABI (`apps/mac`, `apps/ipad`); a CLI; and an MCP server.
> Surveyed: 2026-10-25
> Baseline: `lift-up/plan-20261025b` at `d9a420c`, cut from `origin/main` after a fetch (origin/main had been force-updated from `c8e3006` to `d9a420c`). Clean tree. Drift re-check before writing: `origin/main` is still `d9a420c`, so nothing moved.
> Scope: the full product at `d9a420c`.
> Coverage: full. Five read-only survey passes covered generation and health, the MCP agent surface, the web editor and player, the Mac and iPad app, and the core, store, chat, serve, and sources crates. Every item below was verified inline against the code.
> Attractiveness anchor: Keynote for the editor and presenting; Gamma, Pitch, and Beautiful.ai for generated decks; a strong human presentation coach for a made deck's story.

**Excluded, and not repeated here:**
- the three earlier plans (#571, #614, #683);
- every open PR named in the dispatch (#629 through #741);
- the queued lanes and everything waiting on Jay.

Where an item touches in-flight work, the item says so.

## Performance

### PERF-1. An MCP or CLI patch lints the whole deck twice, and one `slide_write` lints it about seven times
- **Location:** `crates/scaena-ops/src/patch.rs:222-224`, `:328`; `crates/scaena-ops/src/narrative/one.rs:390-432`
- **Proof:**
  ```
  pub fn patching(b: &Bundle, ops: &Value, what: Option<&str>) -> Result<(Patched, Option<Write>), OpsError> {
      made(b, ops, what, Judge::Whole, None, BTreeMap::new())
  ...
              Judge::Whole => (lint(b)?.findings, lint_in(&next, &view)?.findings),
  ```
  In `slide_polishing`, `patching(...)` runs at `:390` and again at `:432`, with `lint_in(&deck, &view)` between them at `:409` and `:420`, and once more after at `:435`.
- **Verified:** `grep -n "Judge::Reached\|lint_state_in" crates/scaena-mcp/src/lib.rs crates/scaena-ops/src/narrative/one.rs` → `# (no matches)`. `grep -n "Judge::Whole" crates/scaena-ops/src/*.rs` → `patch.rs:223`, `patch.rs:234`, `patch.rs:328`. `editing` exists at `patch.rs:248` with `Judge::Reached(&mut layout)` at `:255`. `lint_state_in` exists at `lint.rs:121`.
- **Do:**
  - Give `patching` and `patching_with` a reach-limited judge for callers that keep no engine. The new layout closure (new) builds a `scaena_engine` engine once per call and lays out only the states that `changed`/`reached` name, the way `editing` does for the editors.
  - In `slide_polishing`, compute the "before" lint once and reuse it across both `patching` calls. Scope the three `lint_in` calls to `written.states` with `lint_state_in`.
  - Validation and the document rules stay whole-deck.
- **Why:** An agent building a deck slide by slide pays O(slides²) in layout. The editors already pay only for what an edit reaches; agents, the flow we most want to feel fast, pay the most.
- **Effort:** M · **Impact:** L

### PERF-2. Typing on the Mac's canvas decompiles and recompiles the whole deck on the main actor, for every character
- **Location:** `apps/mac/ScaenaKit/Sources/ScaenaKit/DeckEditor.swift:181-188`, `:398-419`
- **Proof:**
  ```
  guard try session.typed(ops, at: date) else { throw ScaenaError(message: "it reads so already") }
  guard let before = take(try session.source(), lint: false) else {
  ...
  guard let compiling = try? session.compile(next) else {
  ...
  slots = (try? session.timeline()) ?? slots
  ```
- **Verified:**
  - `grep -n "OffMain\|Task.detached\|stepAway" apps/mac/ScaenaKit/Sources/ScaenaKit/DeckEditor.swift` → `83: …let offMain = OffMain(label: "ScaenaKit.health")`, `440:`, `450: OffMain.waiting("ScaenaKit.draw")…`, `472: final class OffMain: Sendable {`, `514:`. These are health and drawing only; `typed`/`take` run synchronously.
  - `Editing.swift:516: public func compile(_ source: String) throws -> Compiling`.
- **Do:**
  - `session.typed` already holds the patched deck. Add a session call (new, e.g. `typedSource`) that returns its source and timeline without compiling the source again. Use it in `DeckEditor.typed` in place of `take(try session.source(), lint: false)`.
  - Bump `revision` once per typing burst (the burst `Typing` already groups for undo), not once per key, so the `.task(id: editor.revision)` readers in `CanvasSelection.swift` and `DeckView.swift` run once per pause.
- **Why:** Typing is the most frequent gesture. Its cost grows with deck size and lands on the main thread, which drops keys and makes the caret lag. That is the opposite of Keynote's feel.
- **Effort:** M · **Impact:** M

### PERF-3. `deck_lint` with a `state` lints every state, `fix` ignores `state` and `now`, and `deck_health` always lints the whole deck
- **Location:** `crates/scaena-mcp/src/lib.rs:1088-1100`, `:1423-1425`
- **Proof:**
  ```
  true => {
      let f = scaena_ops::lint::lint_fix(&b)?;
  ...
  false => {
      let l = scaena_ops::lint::lint_at(&b, now)?;
  ...
  .filter(|f| f.severity >= min && a.state.as_ref().is_none_or(|s| f.state.as_deref() == Some(s)))
  ```
  and `scaena_ops::health::health_in(&b.deck, &scaena_ops::lint::lint(&b)?.findings)`.
- **Verified:** `grep -n "lint_state_in\|states_in\|lint_scoped" crates/scaena-mcp/src/lib.rs` → `# (no matches)`. `lint.rs:121: pub fn lint_state_in(`, `lint.rs:97: pub fn lint_at(`, `lint.rs:224: pub fn lint_fix(b: &Bundle)`.
- **Do:**
  - With `state`, call `lint_state_in`. Without it, keep `lint_at`.
  - Give `lint_fix` a `now` and an optional state (new parameters), so `fix` honours both, and W428 is no longer dropped under `fix`.
- **Why:** The common agent loop is "patch a slide, lint that slide". Today every such lint costs a whole deck, and a forecast's staleness disappears from the findings whenever the agent asks for fixes.
- **Effort:** S · **Impact:** M

### PERF-4. New from a Prompt on the Mac reads and parses each source file on the main actor
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/PromptWindow.swift:764`; `apps/mac/ScaenaKit/Sources/ScaenaKit/Generator.swift:562-565`
- **Proof:**
  ```
  say(try generator.add(name: url.lastPathComponent, data: try Data(contentsOf: url)).words)
  ...
  public func add(name: String, data: Data) throws -> GeneratingStep {
      let read: Read = try step("add", ["name": .string(name)], bytes: data)
  ```
- **Verified:** `Generator.swift:390: public final class Generator {` sits under `@MainActor` (`:389`). `Generator.swift:441: func stepAway<T: Decodable & Sendable>(_ method: String, _ args: [String: JSONValue] = [:]) async throws -> T {` takes no bytes. This was deferred in 10-24b and is still open.
- **Do:**
  - Give `stepAway` a `bytes:` parameter (new) and make `add` async over it.
  - Read the file with `Data(contentsOf:)` inside the off-main step.
  - `PromptWindow` awaits it, so Cancel stays live while a 50 MB PDF or workbook is read.
- **Why:** This is the first ten minutes. Handing over a large source freezes the window, Cancel included, right after the person committed to a paid run.
- **Effort:** S · **Impact:** M

### PERF-5. The Mac's show lays out the next cue synchronously on the main actor as each slide comes to rest
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/Presenting.swift:585-590`
- **Proof:**
  ```
  private func rested() {
      holding?.cancel()
      if let next = show.next, next.span > 0 { try? editor.session.prepare(next.state) }
      guard let hold = show.rested() else { return }
      waits(hold)
  }
  ```
- **Verified:** `grep -n "OffMain\|Task.detached\|prepare" apps/mac/ScaenaKit/Sources/Scaena/Presenting.swift` → `588:` only. `ScaenaSession.swift:186: public func prepare(_ state: String) throws {`.
- **Do:**
  - Run `prepare` on the editor's `OffMain` queue (`DeckEditor.swift:472`) on a snapshot's session, as `drawingAway` does.
  - Or yield before it (`Task { await … }`) so the hold timer and the presenter's next picture are set first.
  - Keep lift-up PERF-1's guarantee: going on draws at once once `prepare` has finished.
- **Why:** This runs on the live stage. A heavy next slide stalls the presenter window and the hold timer at the exact moment the room is watching.
- **Effort:** M · **Impact:** S

## Functionality

### FUNC-1. Write This Slide Again and Add a Slide About write from nothing in a grounded deck: no sources, no big idea, no settled claim
- **Location:** `crates/scaena-ops/src/narrative/one.rs:140-148`; `crates/scaena-narrato/src/one.rs:23-38`, `:51-60`
- **Proof:**
  ```
  brief: brief_of(doc),
  icons,
  sources: Vec::new(),
  outline: None,
  ```
  and `OneSlide { brief, arc, beat, claim, before, after, now, ask }`. Its prompt's fields are audience, goal, outcomes, voice, modality, language, arc, and part. There is no source excerpt and no big idea.
- **Verified:** `grep -n "source_content\|big_idea\|bigIdea" crates/scaena-narrato/src/one.rs crates/scaena-ops/src/narrative/one.rs` → `# (no matches)`. The deck keeps its sources at `narrative.rs:2101: meta["narrative"]["sources"] = json!(narrative.sources);`.
- **Do:**
  - Add `big_idea` and `sources` fields to `OneSlide` (new). `one_asked` fills them from `meta.narrative.outline.bigIdea`, from the settled outline beat's claim (not the spine beat's claim, which is the old headline), and from the names and addresses in `meta.narrative.sources` plus the rows of the data sources the deck kept from them.
  - Fence each one, as the brief is fenced.
  - Pass `sources` into the `Narrative` that `one.rs:146` builds, so `cited()` can name a new slide's `sourceRef`.
  - Ask for `beatIndex` in `ONE_SLIDE_SYSTEM`.
- **Why:** A coach's first question about a new slide is "where's this from, and does it serve the point?" Today an added slide in a deck grounded in a 10-K invents its figures and drifts from the big idea, which undoes the main thing that sets Scaena's decks apart from Gamma's.
- **Effort:** M · **Impact:** L

### FUNC-2. The look pass asks to turn a topic title into a claim, then refuses any claim longer than the topic
- **Location:** `crates/scaena-narrato/src/look.rs:138`, `:199-202`, `:227-229`
- **Proof:**
  ```
  - A content slide's title that names a topic rather than the slide's takeaway: rewrite it to state its claim as a sentence, within the same length; ...
  fn longest(was: &str) -> usize {
      let n = was.chars().count();
      n + (n / 10).max(4)
  ...
  (!now.is_empty() && now != was && now.chars().count() <= longest(was) && crate::same_figures(was, &now))
  ```
- **Verified:** `grep -n "fn longest\|title_longest\|claim.*longest" crates/scaena-narrato/src/look.rs` → `199:fn longest(was: &str) -> usize {`. No title-specific allowance exists.
- **Do:**
  - Give the title its own limit (new `title_longest`): the longer of `longest(was)` and the characters the slide's headline place holds at its role's size.
  - `scaena_ops::narrative::looked` knows that place, so pass it on `Seen` as a new `room` field.
  - Drop "within the same length" from the instruction in favour of "within N characters", where N is that room.
  - Bullets keep `longest`.
- **Why:** "Market overview" (15 characters) may grow only to 19, so the action title a coach asks for ("Mid-market demand doubled in two years") can never pass. Every made deck keeps its topic labels while the look pass appears to fix them.
- **Effort:** S · **Impact:** L

### FUNC-3. The cover uses up the hook: with the extra slides handed out from the last beat back, the opening beat's only slide is the title slide
- **Location:** `crates/scaena-narrato/src/beats.rs:7-31`; `crates/scaena-narrato/src/prompt.rs:309`; `crates/scaena-narrato/src/outline.rs:88`
- **Proof:**
  ```
  /// `slides` slides over `beats` beats: ⌊N/B⌋ each, the rest handed out one per beat from the
  /// last beat back.
  ...
  "**Deck bookends (required)**: ALWAYS open the deck with a Title slide (layout \"title\") as slide 1 ...
  ```
  The outline asks: "The first beat's claim opens the talk: make it a hook drawn from the material…". `narrative.rs:834-842` then drops the claim from any `"title"` slide in the look (`claim: if framing { String::new() } …`).
- **Verified:** `grep -n "Title slide\|cover\|opens" crates/scaena-narrato/src/beats.rs` → `9:/// none, so the deck opens on the arc's first beats.`. Nothing reserves a slide for the cover. `outline.rs:222: return beats::allocate(target.max(n), n);`.
- **Do:**
  - In Scaena's outline layer, leaving Narrato's `allocate` byte for byte: when `outline::counted` (or `beats::plan`'s caller) gives the first beat a single slide and the deck opens on a Title, add one slide to the first beat, taken from the largest beat.
  - Have `outline::block` say: "the cover is beat 1's first slide; its second states the hook."
- **Why:** A strong talk opens on its hook, not on its name. Today an 8-slide, 6-beat deck spends the hook claim on a title slide, and the look pass then strips that claim away.
- **Effort:** S · **Impact:** M

### FUNC-4. Deck health never coaches a headline that names a topic instead of stating the slide's point
- **Location:** `crates/scaena-ops/src/health.rs:774-795`; `Kind` at `:120-153`
- **Proof:**
  ```
  if s.title.trim().is_empty() {
      find(
          &s.id,
          Severity::Error,
          Kind::Title,
  ...
  if s.body.trim().is_empty() && s.notes.trim().is_empty() {
  ```
  `Kind` lists Title, Empty, Turn, Arc, Intensity, Tone, Drift, Layout, Claim, BigIdea, Story, Dense, Notes, and Pacing.
- **Verified:** `grep -n "Kind::Topic\|topic headline\|names a topic\|takeaway" crates/scaena-ops/src/health.rs` → `490:const CLOSING: [&str; 4] = ["closing", "call-to-action", "takeaways", "poster"];` and `584:` (a comment). No headline-as-label check exists.
- **Do:**
  - Add `Kind::Label` (new). A content slide (not cover, divider, agenda, or closing, the same set `closing()` and the look pass's `framing` use) whose headline has no verb in English decks gets an info finding with an ask: "State slide k's point as its headline: what should the room believe after it?" Use a short word-shape heuristic (new: no finite-verb token from a small list, and three to six words).
  - Give it no patch. Skip other languages, as intensity is skipped (`in_english`).
- **Why:** Action titles are the first thing a coach marks. The look pass tries once at generation (FUNC-2), but after the person's edits nothing flags "Q3 results" again. The Story tab is the place to.
- **Effort:** M · **Impact:** M

### FUNC-5. The slides an agent hands `deck_generate` and `slide_write` drop `intensity` and `tone` without a word, and `slide_write` skips normalising
- **Location:** `crates/scaena-mcp/src/lib.rs:888-893`, `:1526-1527`; `crates/scaena-narrato/src/answer.rs:10-14`, `:148-171`
- **Proof:**
  ```
  /// `speakerNotes`, `beatIndex` and `beatName` (the arc's beat it belongs to), `intensity`
  /// (0–1), `tone` (−1–1), and `visualDescription`; `chartData` where a chart is drawn.
  ...
  expected_intensity: finite("expectedIntensity").map(|v| v.clamp(0.0, 1.0)),
  expected_valence: finite("expectedValence").map(|v| v.clamp(-1.0, 1.0)),
  ...
  serde_json::from_value(Value::Object(a.slide)).map_err(|e| OpsError::new(format!("the slide: {e}")))?;
  ```
- **Verified:** `grep -n "normalize\|deny_unknown_fields" crates/scaena-mcp/src/lib.rs crates/scaena-narrato/src/answer.rs` → only `deny_unknown_fields` on the tool argument structs (`lib.rs:507, 514, 547, …`). There is no `normalize` call in the MCP server, and `answer.rs:148: pub fn normalize(raw: &Value) -> Slide {` is not reached by `slide_write`. `Slide` is `#[serde(rename_all = "camelCase", default)]` with no `deny_unknown_fields`.
- **Do:**
  - Change the `DeckGenerate.slides` doc to the names the parser reads (`expectedIntensity`, `expectedValence`).
  - In `slide_write`, build the slide with `scaena_narrato::answer::normalize`, so it gets its layout default, clamping, and chart cleanup.
  - Reject a slide object with none of `slideTitle`, `keyBullets`, or `layoutSuggestion` as a tool error that names those fields.
- **Why:** An agent following the tool's own description writes a deck whose declared arc health can't read. A `{"title": …}` slide becomes an empty slide with no error, which is the agent-building flow at its worst.
- **Effort:** S · **Impact:** M

## Stability

### STAB-1. Health names an outlined deck's beats from the arc, so drift findings, and their fix, use the wrong beat
- **Location:** `crates/scaena-ops/src/health.rs:573-574`, `:898-907`
- **Proof:**
  ```
  let beats: Vec<String> = arc::beat_names(arc_name.as_deref()).into_iter().map(str::to_string).collect();
  let beat_name = |k: usize| beats.get(k).cloned().unwrap_or_else(|| format!("beat {}", k + 1));
  ...
  json!({ "op": "replace", "path": index, "value": now_beat }),
  json!({ "op": "replace", "path": name, "value": now }),
  ```
  Meanwhile `outline.rs:278` tells the model "`beatIndex` the number of a slide's beat here less one and `beatName` that beat's name", which is the outline's own beat.
- **Verified:** `grep -n "beat_names" crates/scaena-ops/src/health.rs` → `573:` only. The outline is read only from `:923` on, for Claim and BigIdea.
- **Do:** Where `meta.narrative.outline.beats` exists, take `beats` from its names in order (read at `:923`, moved above `:573`). Fall back to `arc::beat_names` only without an outline. The drift fix then writes the outline's beat name.
- **Why:** After a person adds, removes, or reorders a beat, which the outline invites, the Story tab says a slide "drifted" into a beat the deck doesn't have, and its one-click fix writes that wrong name into the spine.
- **Effort:** S · **Impact:** M

### STAB-2. The prompt's language is free text: "german" sets no language, the brief doesn't keep it, and health then reads a German deck as English
- **Location:** `web/editor.html:1103`; `crates/scaena-ops/src/narrative.rs:2302-2320`, `:2071-2093`; `crates/scaena-ops/src/health.rs:565-567`
- **Proof:**
  ```
  <label>Language <input id="prompting-language" name="language" placeholder="English" autocomplete="off" /></label>
  ...
  fn language_tag(language: &str) -> Option<&'static str> {
      Some(match language {
          "English" => "en",
  ...
  ("modality", &brief.modality), ("desiredOutcomes", &brief.desired_outcomes), ("personaId", &brief.persona_id), ("voice", &brief.voice),
  ```
- **Verified:**
  - `grep -n '"language"' crates/scaena-ops/src/narrative.rs` → `# (no matches)`, so `brief.language` is never written to `meta.narrative.brief`.
  - `language_tag` matches exactly. `deck_lang` is `brief.language.as_deref().map_or(Some("en"), language_tag)` (`:2325-2326`), so "german" gives `None` and no `meta.lang`.
  - Health's `language` is then `None`, and `in_english(None)` holds.
- **Do:**
  - Make the browser's field a `<select>` of `scaena_narrato::brief`'s languages, as the Mac's picker is.
  - Match `language_tag` case-insensitively and by endonym ("Deutsch", "Français", "日本語").
  - Add `("language", &brief.language)` to the keys written at `narrative.rs:2087`.
- **Why:** This is DF7-4's bug returning by another door. A German deck gets English intensity and tone readings, typesetting falls back to `en` (no German quotes or hyphenation), and the Story tab's coaching is wrong for it.
- **Effort:** S · **Impact:** M

### STAB-3. One tool edit that fails leaves the browser assistant's conversation broken until New Conversation
- **Location:** `web/src/assistant/index.ts:238`; `web/src/assistant/converse.ts:181-197`
- **Proof:**
  ```
  if (changed || files.length) await edited(session.source(), files);
  return out;
  ...
  o.conversation.push({ role: "assistant", text: answered.text, calls: answered.calls, raw: answered.raw });
  ...
  const { result, summary, made } = await o.call(call);
  ```
- **Verified:** `grep -n "catch\|try {" web/src/assistant/converse.ts web/src/assistant/index.ts` → `converse.ts:86, 102, 165, 167`; `index.ts:143, 224, 226, 234, 293, 330`. None of them spans `index.ts:238` or the call loop at `converse.ts:190-200`. `grep -n "conversation\.\(pop\|splice\|length =\)" web/src/assistant/*.ts` → `# (no matches)`.
- **Do:**
  - In `run` (`index.ts`), wrap `edited(...)` and turn a throw into an error result (`answer(JSON.stringify({ message }), true)`), so every `tool_use` gets its `tool_result`.
  - In `converse`, on any throw after the assistant turn is pushed, append a tool turn that answers each remaining call as failed.
- **Why:** Anthropic rejects a `tool_use` with no `tool_result`. After one failed edit (a worker `edit` that throws), every later question fails, and the only way out is to discard the conversation.
- **Effort:** S · **Impact:** M

### STAB-4. On the Mac, an undo of a data edit or of written files ignores failure and still registers its redo
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/ScaenaDocument.swift:266-284`
- **Proof:**
  ```
  undo?.registerUndo(withTarget: self) { [weak undo] document in
      _ = try? document.session.dataUndo(redo: redo)
      document.editor.reread()
      document.dataStep(redo: !redo, undo: undo)
  ...
      try? document.session.write(files.map { (path: $0.path, text: $0.before) })
  }
  guard let now = document.editor.restore(source) else { return }
  ```
- **Verified:** `grep -n "try?" apps/mac/ScaenaKit/Sources/Scaena/ScaenaDocument.swift` → `57, 67, 84, 164, 269, 280`.
- **Do:**
  - Replace both `try?` with `do/catch`. On failure, say why through the editor's `failure` (as the Inspector's edits do) and don't register the opposite step.
  - When `restore` returns nil, register the undo again with the same `source`, so Redo isn't lost silently.
- **Why:** Undo is the safety net. When it quietly does nothing, or restores a source against a theme it didn't put back, the deck is left in a state the person never made, and they can't see that it happened.
- **Effort:** S · **Impact:** S

### STAB-5. A picture refused in the last round is reported as "it was not asked for"
- **Location:** `crates/scaena-ops/src/narrative.rs:217-219`, `:241-246`, `:276-278`
- **Proof:**
  ```
  if self.stopped.is_some() || self.round >= TRIES {
  ...
  Err(pictures::Unmade::Plain(why)) => {
      if self.plain[i] {
          self.why[i] = Some(why);
      }
      self.plain[i] = true;
  ...
  .unwrap_or_else(|| "it was not asked for".into());
  ```
- **Verified:** `grep -n "not asked for" crates/scaena-ops/src/narrative.rs` → `278:`.
- **Do:** In the `Plain` arm, set `self.why[i] = Some(why)` when `self.round + 1 == TRIES` (`last`, already computed at `:229`), as the `Again` arm does.
- **Why:** The person paid for pictures and is told one was never requested, when the provider actually refused it. The real reason, usually a content filter on the visual concept, is what they would need to fix it.
- **Effort:** S · **Impact:** S

## Reliability

### REL-1. A damaged `history/deck.loro` blocks every save of the bundle
- **Location:** `crates/scaena-store/src/lib.rs:247-254`; `crates/scaena-store/src/save.rs:105-121`
- **Proof:**
  ```
  pub fn history(&self) -> Result<Option<DeckDoc>, StoreError> {
      if !self.files.exists(HISTORY) {
          return Ok(None);
      }
      let doc = DeckDoc::load(&self.read(HISTORY)?)?;
  ...
  match self.record(deck, written, &edit)? {
  ```
- **Verified:** `grep -n "quarantine\|damaged\|history.*unreadable" crates/scaena-store/src/*.rs` → matches only in `subset.rs` (fonts: `:101, :124, :136, :139, :146, :147`). Nothing handles a history that won't load. `.record(` callers are `save.rs:113`, `crdt.rs:177`, `lint.rs:308`, and `scaena-wasm/src/lib.rs:1196`.
- **Do:**
  - In `Bundle::history`, when `DeckDoc::load` fails, return a typed `StoreError::HistoryDamaged` (new).
  - Have `recorded` and `saving_with` save the deck anyway. They move the bad file to `history/deck.loro.damaged-<date>` (new) and begin a fresh history with the change "history begins again: the earlier one could not be read". The status line, the MCP result, and the Mac's save then say so.
- **Why:** A truncated file, bit-rot, or a newer Loro format now makes the deck unsaveable everywhere (CLI, MCP, browser, Mac) until someone deletes a hidden file by hand. That is data loss caused by a feature meant to prevent it.
- **Effort:** M · **Impact:** L

### REL-2. A deck made from a prompt whose bundle doesn't open is dropped, and the status line says it was made
- **Location:** `web/src/prompt.ts:533-550`; `web/src/editor.ts:265-272`
- **Proof:**
  ```
  const deck = await done;
  dialog.close("generated");
  await open({ files: deck.files, name: deck.name });
  ...
  tell(made(deck.made));
  ```
  and, in `open`:
  ```
  } catch (e) {
    const why = `${namedFor(source)} can't be opened: ${said(e)}`;
    if (current) current.say(why);
    else $("#status").textContent = why;
    return;
  ```
- **Verified:** `grep -n "download\|keep.*files\|deck.files" web/src/prompt.ts` → `536: await open({ files: deck.files, name: deck.name });` only. Nothing keeps or offers the files if `open` fails. `dialog.onclose` (`:461-465`) sets `worker = undefined`, so the `catch` at `:543` returns silently.
- **Do:**
  - Have `open` return whether it opened (it returns `void` today).
  - In the prompt flow, call it before closing the dialog. On failure, keep the dialog open and offer "Download the deck" (a `.scaena` zip of `deck.files`, through the existing download path), with the reason.
  - Say "Made" only after the open succeeds.
- **Why:** The person paid for an arc, an outline, slides, a look pass, and pictures. One failed open throws all of that away, while the status line and the Story tab (of the old deck) say it worked.
- **Effort:** S · **Impact:** L

### REL-3. The browser saves over a newer copy on disk without a word: another tab, the CLI, or an agent loses its work
- **Location:** `web/src/worker.ts:1429-1446`
- **Proof:**
  ```
  const saved = player.save(new Date().toISOString(), false, recorder);
  try {
    const paths = saved.paths();
    const last = ["deck.json", "manifest.json"];
    for (const path of [...paths.filter((p) => !last.includes(p)), ...last.filter((p) => paths.includes(p))])
      await home.write(path, saved.file(path)!);
  ```
- **Verified:** `grep -n "lastModified\|changed on disk\|savedAt\|modified" web/src/worker.ts web/src/folders.ts` → `# (no matches)` (the only `savedAt` is in `recovery.ts`).
- **Do:**
  - When the deck opens from a folder or OPFS, record `deck.json`'s bytes digest (new `openedAs`).
  - Before `save` writes, read `deck.json` again. If its digest differs, ask: "This deck was changed elsewhere since it opened (another tab, `scaena`, or an agent). Save over it, or open theirs?"
  - Take a `navigator.locks` lock (new) per `opfs:` name so two tabs never save together.
- **Why:** Agents over MCP and the CLI write the same folders the editor opens. Today the last writer silently wins, and with a kept history it overwrites `history/deck.loro` too, so the other side's versions disappear as well. Keynote warns here.
- **Effort:** M · **Impact:** M

### REL-4. One busy answer while deep research polls ends the run, and a failed research ends the deck even when the sources were enough
- **Location:** `crates/scaena-narrato/src/research.rs:288-292`, `:315`; `web/src/generate.ts:295`
- **Proof:**
  ```
  State::Polling { id, polls } => {
      let it = read(status, status_text, body).map_err(NarratoError::Provider)?;
  ...
  if (job.research) await research(g, brief, asking, job, sources, hear);
  const generation = new g.Generation(brief, asking, sources);
  ```
- **Verified:** `grep -n "retry_after\|TRIES\|busy" crates/scaena-narrato/src/research.rs` → `516: let busy = …` and `517:` (a test's in-progress body, not a retry). `scaena_narrato::retry_after` and `TRIES` exist (CLAUDE.md, lift-up 10-24b REL-1) and are used by the other steps.
- **Do:**
  - In `Research::answer`'s `Polling` and `Searching` arms, treat 429 and 5xx as `Next::Wait` after `scaena_narrato::retry_after`, up to `TRIES`, as the arc, outline, and slides steps do.
  - In `generate.ts`, catch a research failure. Say "Research stopped: … The deck is grounded in what you gave", and go on when `sources` holds anything; otherwise stop as now.
  - Do the same in the CLI's and the Mac's research callers.
- **Why:** Research runs up to 30 minutes and is the step most likely to meet a 429. Losing the whole paid run to one transient answer is the worst failure in the flow.
- **Effort:** S · **Impact:** M

### REL-5. Closing "Write a slide" while it is being built leaves the worker's deck holding a slide the page never takes
- **Location:** `web/src/slide-ask.ts:110-115`, `:138-143`; `web/src/worker.ts:890-899`
- **Proof:**
  ```
  thinking.textContent = "Writing the slide…";
  try {
    const { written, source, edited } = await stage.slideWrite(where, text, `agent:${who.model}`, now.index, host.format());
    if (mine !== turn) return;
  ...
  dialog.addEventListener("close", () => {
    turn++;
  ```
  The worker's `case "slideWrite"` applies the slide to `player` and repaints (`edit(source, …)`) before it answers.
- **Verified:** `grep -n "slideWrite(" web/src/stage.ts` → `1001: slideWrite(place: SlidePlace, answer: string, author: string, index: number, format?: string)`. It sends no source, unlike `writing.ts`'s `stage.make(host.source, …)`.
- **Do:**
  - Send `host.source` with `slideWrite`, and have the worker build on it, as `make` does.
  - Once the request is sent, either let it complete and apply it (disable Cancel while writing, say "Writing… it lands in a moment") or, on close, tell the worker to compile `host.source` again (`stage.make(host.source, [], …)`), so the canvas, a save, and the source pane agree.
- **Why:** Escape during "Writing the slide…" leaves a canvas, and a `save()` from the worker's `player`, holding a slide the source pane doesn't have. The next keystroke silently removes it. It is unclear which deck is real.
- **Effort:** S · **Impact:** M

## Security

Three items survived verification. The other security candidates were already guarded (the sources' `most` is enforced by every client), by design (the bundle's folder as a root), or covered by in-flight PRs (#707, #715). See Dropped.

### SEC-1. `deck_create` copies `data[].file` from anywhere on disk into a new bundle, skipping the read guard `data_attach` applies
- **Location:** `crates/scaena-mcp/src/lib.rs:1005`; `crates/scaena-ops/src/create.rs:253-255`, `:439-440`
- **Proof:**
  ```
  title: a.title,
  data: a.data,
  ..Default::default()
  ...
  let data = |file: &Path| -> Result<Vec<u8>, OpsError> {
      std::fs::read(file).with_context(|| format!("reading {}", file.display()))
  ...
  let Source { path, bytes, decl, .. } = source(attach, data(&attach.file)?)?;
  ```
  Compare `data_attach` at `lib.rs:1310`: `data.file = self.readable(&data.file, &a.bundle).map_err(failure)?;`.
- **Verified:** `grep -n "readable(" crates/scaena-mcp/src/lib.rs` → `156` (definition), `165`, `998`, `1270`, `1310`, `1326`, `1567`. None is on `a.data` in `deck_create`.
- **Do:** In `deck_create`'s handler, before building `Create`, map each `a.data` entry's `file` through `self.readable(&d.file, &a.bundle)`, as `data_attach` does, and refuse with its error.
- **Why:** A prompt-injected agent can call `deck_create` with `data: [{ file: "~/.aws/credentials" }]`. Almost any text parses as CSV, so the secret lands in a bundle made to be shared. This is exactly the exfiltration `readable` exists to stop (lift-up Security 2).
- **Effort:** S · **Impact:** L

### SEC-2. `deck_export`'s `audio` is never path-checked, and ffmpeg will read a URL there
- **Location:** `crates/scaena-mcp/src/lib.rs:704-707`, `:1186`; `crates/scaena-export/src/video.rs:598-599`
- **Proof:**
  ```
  /// A video's sound track, any file ffmpeg reads, from the first frame: cut where the
  ...
  audio: a.audio.clone().map(Into::into),
  ...
  if let Some(audio) = &settings.audio {
      inputs.extend(["-i".into(), audio.as_os_str().to_os_string()]);
  ```
- **Verified:** `grep -n "protocol_whitelist\|-protocol" crates/scaena-export/src/video.rs` → `# (no matches)`. `readable(` is not called for `audio` (see SEC-1's list).
- **Do:**
  - In the MCP handler, map `a.audio` through `self.readable`.
  - In `video.rs`, put `-protocol_whitelist file,pipe` before the audio `-i` (and the chapters input), so ffmpeg never fetches from the network.
  - The CLI keeps any local file.
- **Why:** An agent can mux any readable file into a shared video, or make ffmpeg reach `http(s)://` and leak that the export ran. That breaks ADR-0030's rule that only the client's own transport reaches the network.
- **Effort:** S · **Impact:** M

### SEC-3. A streamed answer's `index` is taken as given, and one event with a huge index makes the editor allocate without bound
- **Location:** `crates/scaena-chat/src/stream.rs:147`, `:166`, `:241-243`; `web/src/assistant/stream.ts:117`, `:126`, `:173-174`
- **Proof:**
  ```
  let index = v.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
  ...
  while a.blocks.len() < index {
  ...
  const index = typeof call.index === "number" ? call.index : 0;
  while (o.calls.length <= index) o.calls.push(["", "", ""]);
  ```
- **Verified:** `grep -rn "\.index" web/src/assistant/*.ts` → `stream.ts:117` and `stream.ts:173` read it with no cap. There is no `MAX`/`min(` near either site in `stream.rs` or `stream.ts`.
- **Do:** Add a cap of 256 blocks or calls (new `MOST_BLOCKS`) in both parsers. Ignore an event whose index is past it, and end the turn with "the provider's answer did not read".
- **Why:** The provider address is user-set (a proxy, a custom base). One SSE line with `"index": 4000000000` hangs or kills the browser's worker or the Mac's assistant. A parser of untrusted bytes should never be one field away from OOM.
- **Effort:** S · **Impact:** M

## Usability

### USE-1. Keeping a rehearsal's timings turns the show into a self-running one, with no way to present it by clicks
- **Location:** `web/src/rehearse.ts:252-256`; `web/src/worker.ts:2182-2184`; `web/editor.html:1252`
- **Proof:**
  ```
  const ops = kept.filter((k) => k.keeps !== undefined && k.keeps !== k.hold).map((k) => ({ op: "set_state", id: k.state, prop: "hold", value: k.keeps }));
  ...
  const goesOn = (i: number) => !alone && slots[i].hold > 0 && i < slots.length - 1;
  ...
  <p class="note">… Kept, the deck plays at the pace you rehearsed.</p>
  ```
- **Verified:** `grep -n "alone" web/src/worker.ts` → `306: return run(data.index, data.t, ++latest, data.still, data.alone);` and `2183-2184`. Only the editor's cue preview passes `alone`. `grep -n "pause\|Pause" web/src/player.ts` → only the presenter's timer (`964, 1099-1133, 1259-1260`); no advance mode.
- **Do:**
  - Add a player setting, "Advance: by click | by the deck's timings" (new, `?advance=click`, key `A`), kept like the notes size. The presenter view shows it and passes `alone` to `run` for every state while it is set to by click.
  - Make "by click" the default when the deck's holds came from a rehearsal (a new `rehearsed: true` set alongside the `set_state` ops).
  - Do the same in the Mac's `Show`.
- **Why:** Keynote keeps rehearsed timings for self-playing shows and never advances a presenter's talk by itself. A speaker who rehearses to check pacing and presses Keep will find slides moving on mid-sentence in front of the room.
- **Effort:** S · **Impact:** L

### USE-2. Rehearse always starts at slide 1 and can't pause, and on the Mac the display can sleep during it
- **Location:** `web/src/rehearse.ts:147-160`; `apps/mac/ScaenaKit/Sources/Scaena/DeckView.swift:701-705`; `apps/mac/ScaenaKit/Sources/ScaenaKit/States.swift:322`, `:330`
- **Proof:**
  ```
  slots = now;
  spent = slots.map(() => 0);
  index = 0;
  ...
  rehearsal = Rehearsal(
      slots: editor.slots, notes: editor.slots.map { (try? editor.session.notes(state: $0.state)) ?? .none })
  ...
  public private(set) var index = 0
  ```
- **Verified:** `grep -n "Awake" apps/mac/ScaenaKit/Sources/Scaena/Rehearsing.swift` → `# (no matches)`. `Presenting.swift:57` takes `Awake(...)`. Deferred in 10-24 and 10-24b and still open.
- **Do:**
  - Start both rehearsals at the slide shown: `index = editor.shown index` in `rehearse.ts`, and a new `at:` parameter on `Rehearsal.init`. Keep only the times of the slides rehearsed.
  - Add Pause (Space, P) to both, which stops `count()` or the Mac's clock.
  - Hold `Awake` for the Mac's rehearsal's lifetime.
- **Why:** People rehearse the section they just rewrote. Starting over from slide 1 each time, unable to stop for a phone call, makes rehearsing something people skip. Rehearsal is where the story gets tested.
- **Effort:** M · **Impact:** M

### USE-3. Play doesn't play what's on screen: it offers to save first, plays the last save if declined, and is hidden for a folder deck
- **Location:** `web/src/editor.ts:718-721`, `:3091-3096`
- **Proof:**
  ```
  play.hidden = where?.kind === "folder";
  ...
  if (!fresh && !dirty()) return;
  if (!fresh && !confirm("The deck has changes not saved, which the player would not play. Save it first?")) return;
  e.preventDefault();
  ```
- **Verified:** `grep -n "play.hidden" web/src/editor.ts` → `721:`. The comment at `:3088-3090` says "Declined, the link plays the deck as last saved."
- **Do:**
  - Play hands the player the deck as it stands. Through a `BroadcastChannel` (new `scaena-play`) to the window it opens, or a recovery copy keyed for it (`recovery.ts`), so nothing needs saving and a folder deck plays too.
  - Drop the `confirm` and the `folder` hide.
- **Why:** Keynote's Play plays what you see. In the first ten minutes, "Save it first?" before the first look at a show, or no Play at all for a deck opened from a folder, reads as broken.
- **Effort:** M · **Impact:** M

### USE-4. The agent-facing docs hide the ops and tools agents need, and the skill describes an old `deck_patch` result
- **Location:** `crates/scaena-mcp/src/lib.rs:1025-1026`, `:1594-1598`; `skills/author-deck/SKILL.md:94`; `crates/scaena-ops/src/patch.rs:60-62`
- **Proof:**
  ```
  #[tool(description = "Apply a patch: JSON Patch and semantic ops (add_node, rename_node, set_prop, set_text, \
      show_node, hide_node, add_state, …; SPEC §7.3), all or none. ...
  ...
  deck_lint, read the story again with deck_health, and look with deck_render. ...
  ...
  - Once a deck exists, change it with ops (...). Dry-run first: the result is the patch as JSON Patch and what lint finds differently (`added`, `removed`).
  ...
  /// With `verbose`, the patch as RFC 6902, in order.
  pub patch: Option<Vec<JsonOp>>,
  ```
- **Verified:** The quoted `INSTRUCTIONS` names deck_create, data_attach, deck_patch, deck_lint, deck_health, deck_render, spine_read, and spine_update. It does not name `state_start`, `slide_write`, `deck_generate`, `image_add`, `deck_find`, or `deck_history`.
- **Do:**
  - List the slide-level ops in `deck_patch`'s description: `move_slide`, `remove_slide`, `duplicate_slide`, `set_state` (notes), `place`, `choose`, `replace_text`.
  - Add "write a slide with slide_write; start one with state_start; undo with deck_history" to `INSTRUCTIONS`.
  - Fix `SKILL.md:94` to "`verbose: true` returns the JSON Patch".
  - Add rows for `image_add`, `deck_find`, `deck_history`, `theme_edit`, and `data_edit` to the skill's table. `crates/scaena-cli/tests/skills.rs` already holds the skills to the code.
- **Why:** Agents learn the product from these strings. Today they build slide by slide with `add_node` and never discover `slide_write`, or undo, so they make worse decks more slowly.
- **Effort:** S · **Impact:** M

### USE-5. Over MCP, errors name CLI flags and bare ids, with no way forward
- **Location:** `crates/scaena-ops/src/export.rs:202-264`; `crates/scaena-ops/src/states.rs:69`, `:654`; `crates/scaena-ops/src/inspect.rs:529`
- **Proof:**
  ```
  return Err(OpsError::new("--audio sets a video's sound track: mp4, webm, or prores"));
  ...
  let out = out.ok_or_else(|| OpsError::new("`export --format pdf` writes a file: give it --out FILE"))?;
  ...
  .ok_or_else(|| OpsError::new(format!("unknown state `{shown}`")))?;
  ```
- **Verified:** `grep -n "unknown state" crates/scaena-ops/src/states.rs crates/scaena-ops/src/inspect.rs` → `states.rs:69, 158, 644, 654, 671, 715`; `inspect.rs:529, 673, 980, 1046`. Did-you-mean exists only in `scaena-core/src/patch/ops.rs` (lift-up 10-24b USE-5).
- **Do:**
  - Name the field (`audio`, `out`, `states`) as well as the flag, as "an argument that does not read is named by its field" already does for parsing.
  - Route every "unknown state" through the patch's near-id helper (the one at `patch/ops.rs`, made `pub(crate)` or moved to `scaena_core`), so it says "did you mean `s3`?" or lists the first few.
- **Why:** An agent that gets "give it --out FILE" has no `--out`. The fix is a one-line hint, and without it the agent loops or gives up.
- **Effort:** S · **Impact:** S

## Attractiveness / Sexiness

Three items survived. Other polish candidates had already been handled by earlier plans, were in flight (#739 presenter timer against plan, #738 code walkthrough), or are Usability items above.

### ATTR-1. Choose a theme from its pictures in New and in From a Prompt, as the Mac's chooser and Keynote's do
- **Location:** `web/src/editor.ts:426-441`; `web/editor.html:1072`
- **Proof:**
  ```
  const theme = $<HTMLSelectElement>("#making-theme");
  theme.replaceChildren(...familySections());
  // The theme chosen, as its title slide drawn at build (PLAN T.0c).
  const preview = $<HTMLImageElement>("#making-preview");
  ...
  <label>Theme <select id="prompting-theme" name="theme"></select></label>
  ```
- **Verified:** `grep -n "thumbnail" web/src/prompt.ts` → `# (no matches)`. Each theme's `thumbnail` exists in `themes[...]` (`editor.ts:437`). The Mac has `ThemeChooser.swift`.
- **Do:**
  - Replace both `<select>`s with one radio-group gallery component (new `web/src/theme-gallery.ts`) of the shipped themes' title slides (`themes[name].thumbnail`) by family section, arrow keys moving between them.
  - Use it in `#making` and `#prompting`, and keep the `<select>`'s value contract for the code that reads it.
- **Why:** Choosing a theme is the first visual decision, made in the first minute. Gamma, Pitch, and Keynote show the looks. A dropdown of names (with one 240×135 picture, or none in the prompt dialog) makes Scaena look plainer than it is.
- **Effort:** M · **Impact:** M

### ATTR-2. The Mac presenter's notes open where the last slide's were scrolled, and "Next" briefly shows the slide before
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/Presenting.swift:1121-1127`, `:1093-1096`
- **Proof:**
  ```
  ScrollView {
      NotesText(notes.blocks)
          .font(.system(size: notesSize))
          .frame(maxWidth: .infinity, alignment: .leading)
          .textSelection(.enabled)
          .accessibilityIdentifier("presenter-notes")
  }
  ```
- **Verified:** `grep -n "ScrollViewReader\|scrollPosition\|\.id(show" apps/mac/ScaenaKit/Sources/Scaena/Presenting.swift` → `1209: ScrollViewReader { scroller in` (the navigator) only. The browser resets its notes: `web/src/player.ts:1182: pane.scrollTop = 0;`.
- **Do:**
  - Give the notes' `ScrollView` `.id(show.index)` (or `.scrollPosition` reset on `show.index`), so each slide's notes open at the top.
  - Clear `next` when the index changes until its picture is drawn, showing the slide's number meanwhile.
- **Why:** A presenter glancing down mid-talk who finds the notes halfway through, or sees the wrong next slide, loses their place. Keynote's presenter display never does this.
- **Effort:** S · **Impact:** S

### ATTR-3. A PNG exported on the Mac is named by the state's internal id, not "Slide 3"
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/Exporting.swift:64-80`
- **Proof:**
  ```
  case .png(let state, _): return "\(safe) – \(state).png"
  ...
  let read = try? editor.session.tool("deck_read", [:])
  let title = read?.result["deck"]?["meta"]?["title"]?.string ?? "Deck"
  ```
- **Verified:** The CLI and the browser name files "Slide 03.png" and "Slide 03, step 1.png" (CLAUDE.md, `export --format png`). PLAN 2.99 requires that no id reaches the window.
- **Do:**
  - Name the file by the slide and step, as `DeckEditor` already says them for the strip (`Words`), e.g. "Q3 Review – Slide 3, step 1.png".
  - Read the title from the session's meta through a cheap call, not a whole `deck_read` tool call on the main actor.
- **Why:** A file called "Q3 Review – s7-b.png" in Downloads is exactly the id-leak the window is held to avoid, and it is the artifact people send to others.
- **Effort:** S · **Impact:** S

---

## First move

**FUNC-2. The look pass asks to turn a topic title into a claim, then refuses any claim longer than the topic** (from Functionality)

It is a change to one function and one prompt line in `crates/scaena-narrato/src/look.rs`, and it improves every deck generated from then on. The look pass already finds topic titles and already asks the model to state their claims. `longest` then throws away nearly every answer, because a sentence that states a point is longer than the label it replaces. Action titles are the single change a presentation coach would mark first on a made deck, and they decide whether the deck "reads as well as it looks". The fix touches nothing in flight (#726 says refusals and #732 pipelines rounds; neither changes `longest`). FUNC-4 then keeps titles honest after the person's edits. **SEC-1** is the close second: smaller still, and it closes a real exfiltration path for agents.

## Dropped during verification

- **`Fetch::answer` trusts the client to have enforced `most`** (`scaena-sources/src/fetch.rs:440`): every client enforces it (the CLI's `sources.rs:148`, `web/src/generate.ts:170`, and the Mac's `Generator.swift:270-284`). This is defense in depth with no current path to harm.
- **The MCP `bundle` argument's parent becomes a root** (`scaena-mcp/src/lib.rs:203`): this is the documented design ("its working directory or the bundle's folder"), and the hidden-folder check still applies below it.
- **The presenter's timer counts up with no target** (`web/src/player.ts:1098-1143`): #739 (time against plan) covers it.
- **Every generated deck gets "turn may lose the room" warnings** (`coherence.rs:264`, `health.rs:797-810`): #740 (health turns and runs) covers it.
- **The default brief asks for ~15 slides while its length says 5–10** (`prompt.rs:296-301`, `brief.rs:84`): Narrato's prompt byte for byte. Earlier plans deferred it for Jay's call on a slide ceiling, and with the outline on by default the counts come from the outline.
- **"Cancel" on Play's save prompt plays the last saved deck** (`editor.ts:3095`): documented behaviour (`:3088-3090`). It is folded into USE-3 rather than kept as its own item.
- **The Mac's "Other Sizes" strip paints on the main actor at every revision** (`CanvasViews.swift:378`, `:422`): real, but the strip is off by default (View menu). Ranked below PERF-5.
- **The web strip keeps thumbnails without limit** (`strip.ts:80`): one entry per state, replaced by digest. Bounded by deck size, as deferred before.

## Deferred

- **The history is never compacted, and data files are stored whole in every version** (`scaena-store/src/crdt.rs:217-219`, `:839-842`): real and L, and REL-1 should land first.
- **`slide_write`, `state_start`, `image_add`, `background_set`, and `spine_update` always return the full compiled JSON Patch** (`narrative/one.rs:358`, `states.rs:266`), the 59 KB problem `deck_patch` fixed with `verbose`. Next after USE-4.
- **The MCP `tools/list` payload is about 229 KB, 140 KB of it output schemas** (`docs/schema/mcp/deck_inspect.json` is 49 KB). This needs a schema policy.
- **No `deck_read` of one slide, and no one-step undo tool** (`lib.rs:548-563`; `history.rs:555-577`).
- **Read-only MCP calls take the write lock, and the lock is in-process only** (`lib.rs:484`, `:1058`, `:1087`). REL-3 covers the browser's side.
- **Closing detection misses `¿Preguntas?` and several languages, and `language_named` lacks `cs` and `uk`** (`health.rs:468-519`).
- **Health's word matching keeps only unaccented a–z** (`health.rs:1154-1158`, `coherence.rs:133`), which skews French, German, and Czech decks. It sits beside the queued ADR-0032 item 5 (CJK).
- **Arc selection sees only a PDF's file name** (`scaena-narrato/src/lib.rs:406-420`): a Narrato prompt change.
- **Moving or removing a beat in the outline pane drops keyboard focus** (`web/src/prompt.ts:178-180`).
- **No VoiceOver announcements anywhere in the Mac app, and the Story graph shows severity by colour alone** (`DeckView.swift:759-765`, `StoryPanel.swift:225-240`).
- **The Mac's Inspector arranges, groups, and places silently when the engine refuses** (`Inspector.swift:504-533`).
- **The Mac's assistant runs its editing and export tools on the main actor** (`Assistant.swift:278`). This is next to #721 and should follow it.
- **The browser assistant's conversation keeps every user and assistant turn whole** (`scaena-chat/src/lib.rs:102`; `converse.ts`). Deferred before as a budget policy.
- **The browser writes a `.scaena` it opens into OPFS file by file, leaving a half bundle on quota errors and a copy per open** (`web/src/worker.ts:1144-1149`).

Run `/burn-down` to execute this plan.
