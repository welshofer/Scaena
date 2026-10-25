# Lift-up plan: narrative first, then the assistant, the agent path, and the clients (2026-10-25c)

> Platform: mixed. A Rust core; a web editor and player (Vite + TS, the engine as WASM in a worker); a SwiftUI Mac and iPad client over a C ABI (`apps/mac`, `apps/ipad`); a CLI; and an MCP server.
> Surveyed: 2026-10-25
> Baseline: `main` at `d3f5a13`, detached and then cut to `lift-up/plan-20261025c`. Freshness: `git fetch origin main` reported a forced update from `c8e3006` to `d3f5a13`, and `HEAD` matched `origin/main`. Clean tree (`git status --porcelain` printed nothing). Drift re-check before writing: `origin/main` was still `d3f5a13`, so nothing moved.
> Scope: the full product at `d3f5a13`, weighted narrative-first as the dispatch asks. In order: the deck generator; narrative health and the Story surfaces; the assistant and the writing tools; the agent path over MCP; the editors and clients.
> Coverage: full. Five read-only survey passes covered the generator, health and Story, the MCP agent path and the skills, the assistant and the writing tools, and the editors and clients. I verified every item below inline with my own Reads and greps.
> Attractiveness anchor: Keynote for the editor and presenting; Gamma, Pitch, and Beautiful.ai for generated decks; a strong human presentation coach for a made deck's story.
> Model: run at frontier tier.

**Excluded, and not repeated here:**
- the 31 items of `lift-up-plan-20261025b.md` (#742);
- the earlier plans still open (#683, #614, #571, #447);
- every open PR the dispatch names (#771 through #680, and #428, #427, #421);
- PLAN 2.137, 2.138, 2.141, and 2.142.

Where an item sits beside in-flight work, the item says so.

## Performance

### PERF-1. `slide_write`, `state_start`, `background_set`, `spine_update`, and `image_add` still return the whole compiled JSON Patch, and `slide_write`'s holds every state in the deck
- **Location:** `crates/scaena-ops/src/patch.rs:24-25`; `crates/scaena-ops/src/narrative/one.rs:427-435`; `crates/scaena-ops/src/states.rs:250-261`; `crates/scaena-mcp/src/lib.rs:1461-1466`, `:1508-1512`, `:1539-1543`
- **Proof:**
  ```
  /// The patch as RFC 6902, in order.
  pub patch: Vec<JsonOp>,
  ...
  pub struct SlideWritten {
      #[serde(flatten)]
      pub patched: Patched,
  ```
  and `-> Result<Json<scaena_ops::patch::Patched>, String>` for `spine_update` and `background_set`. `slide_write`'s patch replaces `/states` whole (`one.rs:261`: `patch.push(json!({ "op": "replace", "path": "/states", "value": kept }));`).
- **Verified:**
  - `grep -n "verbose" crates/scaena-ops/src/narrative/one.rs crates/scaena-ops/src/states.rs crates/scaena-ops/src/backgrounds.rs` → `# (no matches)`.
  - `grep -n "pub fn of(" crates/scaena-ops/src/patch.rs` → `117:    pub fn of(before: &Deck, after: Option<&Deck>, ops: &Value, patched: Patched, verbose: bool) -> Reported {`.
  - `patch.rs:53-56` says why `deck_patch` moved to `Reported`: "59 KB for seven slides removed".
- **Do:**
  - Give `SlideWritten`, `Started`, and `pictures::Added` a `Reported` (built with `Reported::of`) in place of the flattened `Patched`. Return `Reported` from `spine_update` and `background_set`.
  - Add a `verbose: bool` to the MCP argument structs `SlideWrite`, `StateStart`, `BackgroundSet`, `SpineUpdate`, and `ImageAdd`, as `DeckPatch` has (`lib.rs:576-578`).
  - Run `just bless` for `docs/schema/mcp/*`. Add an assertion to `crates/scaena-mcp/tests/mcp.rs` that `slide_write`'s result has no `patch` without `verbose`.
- **Why:** The recommended agent path is slide by slide with `slide_write`. Each call sends back the whole deck's states, so building a 20-slide deck pushes megabytes of JSON through the agent's context. That costs tokens, latency, and context it needs for the story.
- **Effort:** S · **Impact:** L

### PERF-2. `deck_read` with `theme` returns about 265 KB, and `state_start`'s own doc and the author-deck skill send agents there for the layouts
- **Location:** `crates/scaena-ops/src/read.rs:31-38`; `crates/scaena-mcp/src/lib.rs:830-831`; `skills/author-deck/SKILL.md:31`
- **Proof:**
  ```
  let theme = match theme {
      true => {
          let text = b.theme_json.as_deref().context("the deck names no theme")?;
          Some(serde_json::from_str(text).context("the bundle's theme")?)
  ```
  and `/// each by its name; `deck_read` with `theme` gives them all, in its `layouts`). Without it, a blank slide.`
- **Verified:**
  - `ls -l docs/examples/themes/dusk.theme.json` → `265614`.
  - `grep -n "layouts" crates/scaena-ops/src/read.rs` → `24:    /// colors, type roles, layouts and their slots, and presets, by the names the deck uses.`, so no trimming is done.
  - `scaena-resources` already serves the shipped themes "each in parts", with `layouts` as a `$ref` (`crates/scaena-resources/src/lib.rs:166`).
- **Do:**
  - In `read::read`, replace `theme["layouts"]` with an index of `{name, description, section}` per layout (new helper). Add a `layout: Option<String>` (new) to `DeckRead` that returns one layout whole.
  - Point `StateStart.layout`'s doc and `SKILL.md:31` at `deck_inspect` with `layouts`.
- **Why:** An agent that follows the docs to choose a layout loses about 70k tokens of context to one read. That pushes the brief and the story out of its window right when it is choosing how a slide tells its claim.
- **Effort:** S · **Impact:** M

### PERF-3. `deck_lint` without `fix`, `deck_find` without `replace`, and `deck_history` reads take the bundle's exclusive write lock
- **Location:** `crates/scaena-mcp/src/lib.rs:1104`; `:461` (`open_by`); `:483-494` (`writing`)
- **Proof:**
  ```
              let b = open_by(&a.bundle, author)?;
              let (findings, fixed, laid) = match a.fix {
                  true => {
  ```
  and `fn writing(bundle: &Path) -> Arc<tokio::sync::Mutex<()>> {`
- **Verified:** `grep -n "open(&a.bundle)" crates/scaena-mcp/src/lib.rs` → `1037:` (deck_read), `1138:` (deck_inspect), `1268:` (diff), `1425:` (spine_read), `1441:` (deck_health). `deck_lint` is not among them. `fn open(bundle: &str)` is at `:453`. The prior plan deferred this; it is still open.
- **Do:** Call `open(&a.bundle)` when `!a.fix` (deck_lint), when `a.replace.is_none()` (deck_find), and when the history call neither restores nor begins. Keep `open_by` for the writing paths.
- **Why:** A full lint lays out every state in every format. Holding the write lock while it runs makes an agent's parallel `deck_patch` or `slide_write` wait for nothing.
- **Effort:** S · **Impact:** M

### PERF-4. The Mac's inspector paints every background preset and every layout suggestion on the main actor after each edit
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/Inspector.swift:1181-1186`, `:1052-1057`
- **Proof:**
  ```
  .task(id: "\(state)\u{1f}\(editor.revision)\u{1f}\(editor.format ?? "")") {
      try? await Task.sleep(for: .milliseconds(150))
      guard !Task.isCancelled, let drawn = try? editor.session.backgroundsDrawn(state: state, height: Self.painted)
      else { return }
  ...
  guard let all = try? session.layoutsDrawn(height: Self.painted) else { return nil }
  ```
- **Verified:** `grep -n "Inspector\|offMain\|OffMain" apps/mac/ScaenaKit/Sources/Scaena/Inspector.swift | head -5` → only `16: struct Inspector`, `26:`, `195:`, `205:`. No off-main path exists. `OffMain` exists at `DeckEditor.swift:472` (`final class OffMain: Sendable`).
- **Do:** Draw both from a snapshot's session on `editor`'s `OffMain` queue, as `drawingAway` draws the strip. Drop a result whose `editor.revision` has moved on. Show the thrown reason in the grid in place of `try?`'s nothing.
- **Why:** With the Format tab open, every keystroke's revision CPU-paints seven shader presets and the layout suggestions on the main thread. Typing stalls exactly where the person is shaping a slide.
- **Effort:** M · **Impact:** M

### PERF-5. Closing Make a Picture… in the browser doesn't stop the image request, which is paid for anyway
- **Location:** `web/src/picturing.ts:114-122`, `:144-148`; `web/src/stage.ts:734`
- **Proof:**
  ```
  made = await stage.makePicture({
    subject: shows,
    look: look.value as "illustration" | "photo",
  ...
  // Closed, by Cancel or Escape: a picture still coming is dropped.
  listen(dialog, "close", () => {
    ++turn;
    busy(false);
  });
  ```
- **Verified:**
  - `grep -n "signal\|abort" web/src/picturing.ts` → `# (no matches)`.
  - `grep -n "makePicture\|stopPicture\|stopWrite" web/src/stage.ts web/src/worker.ts` → `stage.ts:734: makePicture(picture: PictureJob)…`, `stage.ts:1021: const stop = () => this.send({ type: "stopWrite", id });`, `worker.ts:738: case "makePicture"`, `worker.ts:917: case "stopWrite":`. Nothing stops a picture.
- **Do:** Give `makePicture` a `signal`. Add a `stopPicture` message (new), mirroring `write`/`stopWrite`. Abort the worker's fetch in the generator's picture step, and send the stop from the dialog's `close` handler.
- **Why:** The image call is the dearest one the editor makes. The prior plan's PERF-4 stopped the writing tools on close; this is the one call it missed.
- **Effort:** S · **Impact:** S

## Functionality

### FUNC-1. Slides written outside the model can't carry an outline: `deck_generate` has no `outline`, `--slides` conflicts with `--from-outline`, and a resumed CLI run drops it
- **Location:** `crates/scaena-mcp/src/lib.rs:894-928`, `:1567-1580`; `crates/scaena-ops/src/narrative.rs:738-765`; `crates/scaena-cli/src/generate.rs:129`, `:432-439`, `:497-511`
- **Proof:**
  ```
  let brief = scaena_ops::narrative::Brief {
      topic: a.topic.unwrap_or(summary),
      audience: a.audience.unwrap_or_default(),
      goal: a.goal.unwrap_or_default(),
      length: a.length.unwrap_or_default(),
      language: a.language,
      ..scaena_ops::narrative::Brief::default()
  ```
  and `#[arg(long, conflicts_with_all = ["slides", "outline"])]` on `from_outline`. `Kept` holds `research`, `slides`, and `arc`, and no outline.
- **Verified:**
  - `grep -n "outline" crates/scaena-mcp/src/lib.rs` → `588:` (an icon's weight) only.
  - `grep -n "voice\|persona\|outcomes\|modality" crates/scaena-mcp/src/lib.rs` → `# (no matches)`.
  - `Generation::settle` exists (`crates/scaena-narrato/src/lib.rs:322`) and accepts `Step::Slides` once the arc is chosen. `from_slides` calls `generation.choose_arc` then `generation.written` (`narrative.rs:764-765`).
  - Health reads `meta.narrative.outline` for its `claim` and `bigIdea` findings (`crates/scaena-ops/src/health.rs:1153-1158`), and `narrative.rs:2224-2225` writes it only from `narrative.outline`.
- **Do:**
  - Add `outline: Option<Outline>` (new parameter) to `from_slides`, and call `generation.settle(&outline)` between `choose_arc` and `written`.
  - Add `outline`, `voice`, `desiredOutcomes`, `modality`, and `persona` (new fields) to MCP `DeckGenerate`, filling `Brief`.
  - In the CLI, drop `"slides"` from `from_outline`'s `conflicts_with_all`. Keep the settled outline in `Kept` as `.generating/outline.json` (new), and pass `--from-outline` in the resume command's `Some(slides)` branch.
- **Why:** An agent writing its own slides is the path the server's instructions name first, and it has no way to say the big idea or the claims it wrote to. Every check health has on whether a deck lands its story (claim, big idea, the closing's takeaways headed by it) is therefore silent for agents, and for any CLI run that resumes from kept slides.
- **Effort:** M · **Impact:** L

### FUNC-2. The Story never coaches evidence: W422 and W423 are dropped, and a claim with nothing behind it is never found
- **Location:** `crates/scaena-ops/src/health.rs:1234-1252`
- **Proof:**
  ```
  for f in lint.iter().filter(|f| matches!(f.code.as_str(), "W420" | "W421" | "W424" | "W425")) {
      let at = (f.state.as_deref().and_then(slide_of_state))
          .or_else(|| f.path.as_deref().and_then(|p| slide_of_beat(p).or_else(|| slide_of_path(p))));
  ```
- **Verified:** `grep -n "W422\|W423\|evidence" crates/scaena-ops/src/health.rs` → `143:` (a doc comment), `1248:` (W421's ask), `1251:` (W424's ask), `1484:`, `1918:` (tests). Neither code is matched. `slide_of_beat` exists at `:1222` and parses `/spine/sections/s/beats/b/…` paths, which is where W423's evidence pointers point.
- **Do:**
  - Add `"W422" | "W423"` to the match, each with an ask, e.g. "Show the figure slide k cites, or drop the citation." The existing `slide_of_beat` places them.
  - Add an `Info` (new check) for a middle beat, not the cover and not in `closing`, whose `evidence` is empty and whose slide draws no picture, chart, or table (`drawn`, already computed for ADR-0032). Ask: "Slide k asserts with nothing behind it: what shows it?"
- **Why:** "Where is your proof?" is the first question a coach asks of a persuasive deck. The lint rules already know the answer, but the Story tab, the panel a person reads to make the talk better, never shows it.
- **Effort:** S · **Impact:** M

### FUNC-3. Draft Notes replaces the presenter's own notes without showing them to the model, and ignores the deck's voice
- **Location:** `crates/scaena-session/src/writing.rs:255-257`, `:128-130`; `crates/scaena-core/src/writing.rs:55-59`
- **Proof:**
  ```
  "notes" => {
      let slide = scaena_core::writing::slide(&self.reads(&ask.state)?);
      Task::Notes { slide, claim: self.claim_of(&ask.state) }
  ...
  Task::Notes { .. } => {
      Some(vec![json!({ "op": "set_state", "id": ask.state, "prop": "notes", "value": text })])
  ```
  and `Notes { slide: String, #[serde(default, …)] claim: Option<String>, },`
- **Verified:** `grep -n "notes\|voice" crates/scaena-core/src/writing.rs` → `voice` appears on `Tighten` (`:40`) and `Rewrite` (`:51`) and is framed at `:263-265`. `Notes` carries no `voice` and no prior notes. `self.voice()` is called for Tighten/Rewrite in `scaena-session/src/writing.rs`.
- **Do:** Add `notes: Option<String>` and `voice: Option<String>` (new fields) to `Task::Notes`. Fill them from the state's current notes and `self.voice()`, fence both in `prompt()`, and tell the model to build on what is there. Extend `the_writing_tools_words_reach_the_model_fenced`.
- **Why:** Notes are where a presenter's own story lives. Draft Notes on a slide the person already annotated throws that away for a generic draft in a voice the deck didn't ask for. (Distinct from #743, which writes a generated deck's notes.)
- **Effort:** S · **Impact:** M

### FUNC-4. Agents get no schema and no example for the Narrato slide JSON that `deck_generate` and `slide_write` take
- **Location:** `crates/scaena-mcp/src/lib.rs:869-872`, `:894-902`; `crates/scaena-resources/src/lib.rs:141-165`
- **Proof:**
  ```
  /// One slide, as Narrato writes each of `deck_generate`'s `slides`: `slideTitle`, `subtitle`,
  /// `layoutSuggestion`, `keyBullets`, `speakerNotes`, and `chartData`, `icons`, `beatName`,
  /// `purpose`, `intent`, `expectedIntensity`, and `expectedValence` where it has them.
  pub slide: Map<String, Value>,
  ```
- **Verified:**
  - `docs/schema/mcp/slide_write.json`'s `slide` is that description with `"type": "object"` and nothing more.
  - `grep -rn "schema/slides\|slides.json" crates/scaena-resources/src/lib.rs` → `# (no matches)`. The examples served are `revenue.deck.json`, `trails.deck.json`, `charts.deck.json`, `revenue.deck.scn`, and `revenue.patch.json`.
  - `chartData`'s shape (`categories`, `series`, `unit`, `sourceId`), `sourceRef`, and `visualDescription` are named nowhere an agent reads.
  - #766 changes how the slide is read (`slide_from`), not what is published.
- **Do:** Serve `scaena://schema/slides` (new), built from `scaena_narrato`'s answer types or written by hand and held to them by a test, and `scaena://examples/slides.json` (new), the slides the MCP test already feeds `deck_generate`. Name both in the two tools' descriptions and in `skills/author-deck/SKILL.md`. Land after #766.
- **Why:** The server's instructions put `deck_generate` and `slide_write` first. An agent that guesses `chartData` wrongly gets a slide drawn as words (lift-up STAB-1 refuses a miscounted series), so the best path produces the weakest charts.
- **Effort:** S · **Impact:** M

### FUNC-5. An agent has no undo: versions are named only by absolute number, and listing them is unbounded
- **Location:** `crates/scaena-ops/src/history.rs:186-192`
- **Proof:**
  ```
  /// The version of `versions` that `name` names: its number as listed (`3`, or `#3`), or its id.
  pub fn named<'a>(versions: &'a [Version], name: &str) -> Result<&'a Version, OpsError> {
      let n = name.strip_prefix('#').unwrap_or(name).parse::<usize>().ok();
      versions.iter().find(|v| Some(v.n) == n || v.id == name).ok_or_else(|| {
  ```
- **Verified:** `grep -n "\"-1\"\|previous\|strip_prefix('~')" crates/scaena-ops/src/history.rs` → `# (no matches)`. The prior plan deferred "no one-step undo tool"; it is still open.
- **Do:** In `named`, accept `-N` / `~N` (new) as N versions before the latest. Then `deck_history` with `restore: "-1"` is the undo. Say so in `deck_history`'s description and the server's instructions. Add `last: Option<usize>` (new) to the listing.
- **Why:** An agent that makes a slide worse has to list the whole history, find the number, and restore by it. A one-word undo makes an agent bolder at trying a better version of a slide, which is how narrative quality improves.
- **Effort:** S · **Impact:** M

## Stability

### STAB-1. Health reads a slide's beat claim as its headline, so "no headline" never fires and tone and intensity read words the room never sees
- **Location:** `crates/scaena-ops/src/health.rs:283-286`, `:922-932`
- **Proof:**
  ```
  let title = match beat.map(|b| b.claim.trim()).filter(|c| !c.is_empty()) {
      Some(claim) => claim.to_string(),
      None => texts.iter().find(|t| headline(t.1)).map(|t| words(t.1).trim().to_string()).unwrap_or_default(),
  };
  ...
  if s.title.trim().is_empty() {
      find(
          &s.id,
          Severity::Error,
          Kind::Title,
  ```
- **Verified:** `Kind::Title` is found only at `:928` (`grep -n "Kind::Title" crates/scaena-ops/src/health.rs` → `928:`). It is reached only when `s.title` is empty, and `title` is the claim whenever the beat has one.
- **Do:**
  - Take `title` from the headline drawn (the `headline(t.1)` text) first, and fall back to the claim only where none is drawn.
  - Give `SlideForHealth` the claim apart where a reader needs it.
  - Add a test: a slide whose headline was deleted on the canvas, in a beat with a claim, gets the `Title` error.
- **Why:** After a person deletes or rewrites a headline, health keeps judging the plan's words, not the slide's. A slide with no headline scores clean, and a slide whose new headline lost its force still "carries" its intensity.
- **Effort:** S · **Impact:** L

### STAB-2. Check the Claim rewrites the headline even when the model says the slide already carries it
- **Location:** `crates/scaena-session/src/writing.rs:97-112`; `crates/scaena-core/src/writing.rs:105-117`
- **Proof:**
  ```
  // otherwise; nothing where the words carry it, or no verdict can be read.
  Task::Claim { headline, .. } => {
      let offered = checked(&text).and_then(|c| c.headline);
      let node = self.claim_node(&ask.state)?;
      Some(match (offered, node, headline) {
          (Some(after), Some(id), Some(before)) if after.trim() != before.trim() => {
  ```
- **Verified:** `checked` returns a `Checked` with a `verdict` field (`Verdict::Carries`, `Drifts`, `Contradicts`, at `core/writing.rs:112-115`). `session/writing.rs:100` never reads `verdict`. `checked` returns `None` for a first line such as `Verdict: Drifts`, since only the first word is matched.
- **Do:** Take the headline only where `c.verdict != Verdict::Carries`. In `checked`, skip a leading `verdict`/`answer` label. Add both cases to `a_slides_words_are_checked_against_its_beats_claim`.
- **Why:** The comment promises "nothing where the words carry it". Instead, a slide that is right gets a rewrite offered, which teaches people to distrust the one tool that checks story against slide.
- **Effort:** S · **Impact:** M

### STAB-3. `slide_write` with a `beat` and no `after` lands at the end of the deck, out of the spine's order
- **Location:** `crates/scaena-ops/src/narrative/one.rs:80-91`, `:98-113`
- **Proof:**
  ```
  let place = match (place, &blank) {
      (Place::End, Some(state)) => {
          replacing = Place::Replace(state.clone());
          &replacing
      }
      _ => place,
  };
  ...
  let (index, at, old, previous) = match place {
      Place::End => (slides.len(), deck.states.len(), None, slides.len().checked_sub(1)),
  ```
- **Verified:** `planned` (`:80-91`) finds the beat's section and index but never adjusts `place`. Only the blank-deck case rewrites `Place::End`.
- **Do:** When `beat_named` is `Some` and `place` is `Place::End`, make `place` `Place::After` of the beat's last state, or of the last state of the nearest earlier beat that has one. Add an MCP test that writes beats 1, 3, then 2 and asserts no W426.
- **Why:** The skill tells agents to plan the spine, then fill each beat with `slide_write`. Filling beat 2 after beat 3 puts its slide after the close, and the deck's order no longer tells the story the spine plans.
- **Effort:** S · **Impact:** M

### STAB-4. Health's "Make it part of …" fix replaces a `beatName` that may not exist, and patches beats by position with no guard
- **Location:** `crates/scaena-ops/src/health.rs:917-921`, `:1129-1137`
- **Proof:**
  ```
  let at = |id: &str, key: &str| -> Option<String> {
      let (i, j, _) = slides[*place.get(id)?].beat?;
      Some(format!("/spine/sections/{i}/beats/{j}/media/narrato/{key}"))
  };
  ...
          json!({ "op": "replace", "path": index, "value": now_beat }),
          json!({ "op": "replace", "path": name, "value": now }),
  ```
- **Verified:** `at` builds the path whether or not the key exists. `narrative::media` writes `beatName` only where the model gave one. `scaena_core::claims::restated` already guards its fix with a `test` op (`claims.rs:84-85`).
- **Do:** Use `"op": "add"` for `beatName`. Lead with `{"op": "test", "path": index, "value": was_beat}`. Make `a_slide_moved_into_another_beat_has_drifted` apply the fix and then find no drift.
- **Why:** The Story offers this fix as safe. Today it can fail with "Not fixed", or land on another beat once the panel is stale, and that is the panel people trust to repair a story.
- **Effort:** S · **Impact:** M

### STAB-5. OpenAI is never told a tool call failed: a refused `deck_patch` reads as a success
- **Location:** `crates/scaena-chat/src/providers.rs:411-414`; `web/src/assistant/providers.ts:265`
- **Proof:**
  ```
  Message::Tool { results } => {
      for r in results {
          messages.push(json!({ "role": "tool", "tool_call_id": r.id, "content": r.json }));
      }
  ```
- **Verified:** `grep -n "is_error\|error" crates/scaena-chat/src/providers.rs` → `354: … "is_error": r.error …` (Anthropic) and `505: let response = if r.error {` (Gemini). The OpenAI branch never reads `r.error`.
- **Do:** In both `openai()` adapters, when `r.error` is set, send `{"error": <result>}` as Gemini does. Record an OpenAI failed-call exchange in `tests/assistant/exchanges.json`, so the Rust and TS adapters stay in step.
- **Why:** #754 made every call answered, failures included. On OpenAI the model still can't tell a failure, so it tells the person "done" over an edit that never landed.
- **Effort:** S · **Impact:** M

## Reliability

### REL-1. A refusal or a safety block becomes "other" in every adapter, and the person is told "the model stopped"
- **Location:** `web/src/assistant/providers.ts:213`, `:387`; `crates/scaena-chat/src/providers.rs:373-378`
- **Proof:**
  ```
  let stop = match answer.get("stop_reason").and_then(Value::as_str) {
      Some("tool_use") => Stop::Tools,
      Some("end_turn") => Stop::End,
      Some("max_tokens") => Stop::Length,
      _ => Stop::Other,
  };
  ```
- **Verified:** `grep -n "refusal\|SAFETY\|blockReason\|PROHIBITED" web/src/assistant/*.ts crates/scaena-chat/src/*.rs` → only comments (`converse.ts:177`, `stream.ts:255`, `:311`, `lib.rs:246`, `providers.rs:156`), which are about HTTP refusals. No stop reason is mapped.
- **Do:** Add `Stop::Refused` / `"refused"` (new) to `Turn`. Map Anthropic's `refusal`, Gemini's `SAFETY`, `PROHIBITED_CONTENT`, `BLOCKLIST`, and `RECITATION`, and a `promptFeedback.blockReason`, to it. Say it in a person's words in the panel and the writing dialog. Record one of each in `tests/assistant/exchanges.json`.
- **Why:** A refusal gives no reason, so people retry the same ask and pay again. The browser and the Mac should say what happened, as the generator already does for pictures (lift-up USE-3).
- **Effort:** S · **Impact:** M

### REL-2. The writing tools take a cut-off answer as whole, and a busy provider fails them on the first try
- **Location:** `web/src/assistant/index.ts:125-136`; `crates/scaena-chat/src/lib.rs:145-149`
- **Proof:**
  ```
  export async function write(writing: Writing, signal?: AbortSignal): Promise<{ text: string; usage?: { input: number; output: number } }> {
    const turn = await providers[writing.provider].turn({
  ...
    return { text: turn.text, usage: turn.usage };
  ```
  and `Ok(provider.read_turn(status, status_text, body)?.text)`
- **Verified:** `turn.stop` is read in neither. The assistant's own loop retries a busy provider (`converse.ts:72-115`), but `write` calls `turn` once.
- **Do:** Return `stop` from `write` and `answered`. Treat `length` (and REL-1's `refused`) as a failure with its own words, never an offer to Use. Route `write` through the same busy retry `converse.ts` uses, and on the Mac route `Provider.write` through `Answered.after`.
- **Why:** A Rewrite cut off mid-sentence is offered as the new headline. Write This Slide Again fails outright on a 529 that a retry would ride out.
- **Effort:** S · **Impact:** M

### REL-3. A proofread, tighten, or rewrite can change a figure, and nothing checks it
- **Location:** `crates/scaena-session/src/writing.rs:116-127`; `crates/scaena-narrato/src/lib.rs:59-75`
- **Proof:**
  ```
  Task::Proofread { text: before }
  | Task::Tighten { text: before, .. }
  | Task::Rewrite { text: before, .. } => {
      let node = ask.node.as_deref().unwrap_or_default();
      Some(
          (replacing(before, &text).into_iter())
  ```
- **Verified:**
  - `grep -n "same_figures\|figures" crates/scaena-session/src/writing.rs crates/scaena-core/src/writing.rs` → `# (no matches)`.
  - `pub fn same_figures(was: &str, now: &str) -> bool` exists at `scaena-narrato/src/lib.rs:70` and guards the look and fit passes (lift-up REL-2). Narrato is not in the editor's module.
- **Do:** Move `figures` and `same_figures` into `scaena_core::writing`, and re-export them from narrato. Give the writing tools' `Written` a `figures_changed: bool` (new). Have the dialog (`web/src/writing.ts`) and the Mac's sheet warn before Use. Do not refuse: a proofread may correct a typo in a number.
- **Why:** A rewrite that turns "$4.1M" into "$41M" or drops "−12%" ends up on a slide a person presents as fact. The generator already refuses this, but the editor's writing tools, used far more, don't.
- **Effort:** S · **Impact:** L

### REL-4. A dropped connection or a 408 skips a slide's picture with no retry
- **Location:** `crates/scaena-narrato/src/pictures.rs:428-430`, `:461`
- **Proof:**
  ```
  /// The statuses a provider answers with for now, not for good (Vzestup's retry list: a rate
  /// limit, a server's failure, and Cloudflare's edge before OpenAI's).
  const AGAIN: [u16; 8] = [429, 500, 502, 503, 504, 520, 522, 524];
  ```
- **Verified:** `grep -n "AGAIN" crates/scaena-narrato/src/pictures.rs` → `430:` and `461: if AGAIN.contains(&status) {`. `pub fn busy(status: u16) -> bool { matches!(status, 0 | 408 | 429 | 500..=599) }` is at `lib.rs:80-82`, and every other step uses it.
- **Do:** Use `crate::busy(status)` at `:461`, keeping the out-of-credit check first. Add a test for status `0` and `408`.
- **Why:** Pictures go out in parallel, so one network blip costs the cover or a divider its picture. Those are the slides that set the deck's look.
- **Effort:** S · **Impact:** S

### REL-5. An outline the engine refuses to settle ends the paid run, and on the Mac a negative slide count is enough
- **Location:** `apps/mac/ScaenaKit/Sources/ScaenaKit/Generator.swift:1019-1021`; `apps/mac/ScaenaKit/Sources/Scaena/PromptWindow.swift:290-299`, `:575`; `web/src/generate.ts:340`
- **Proof:**
  ```
  case .settled(let settled):
      let encoded = try JSONDecoder().decode(JSONValue.self, from: try JSONEncoder().encode(settled))
      let after: Standing = try step("settle", ["outline": encoded])
  ```
  and `TextField("Slides", value: $beat.slides, format: .number)` over `var slides: Int`.
- **Verified:**
  - `Beat.slides` is a `usize` (`crates/scaena-narrato/src/outline.rs`), so `-1` fails to decode.
  - `settle` refuses an outline with no claim left (`lib.rs:318-321`).
  - The browser clamps the count (`prompt.ts:197`, `Math.max(0, …)`) but `generate.ts:340` calls `generation.settle(...)` with no catch around it.
- **Do:** Clamp `EditedBeat.slides` to 0…60. In `Generator.generate`'s `.settled` branch and around `generate.ts`'s `settle`, catch the refusal, say it, and show the outline again via the existing `outline(shown)` / `settle!` callback rather than ending the run.
- **Why:** By this point the arc, the outline, and maybe deep research have been paid for. A typo in a number field shouldn't throw that away.
- **Effort:** S · **Impact:** M

## Security

Three items survived. The agent path's file access is already held (`Scaena::held`, `scaena_store::inside`), and the earlier plans covered the rest of the surface.

### SEC-1. `github::tidy` stops removing HTML comments once a `-->` appears before the first `<!--`
- **Location:** `crates/scaena-sources/src/github.rs:620-627`
- **Proof:**
  ```
  pub fn tidy(markdown: &str) -> String {
      let mut text = markdown.to_string();
      while let (Some(start), Some(end)) = (text.find("<!--"), text.find("-->")) {
          if end < start {
              break;
          }
          text.replace_range(start..end + 3, "");
  ```
- **Verified:** I read the function. Both searches start at 0, so an `A --> B` in a mermaid block or in prose, ahead of the first comment, ends the loop.
- **Do:** Find the close after the open (`text[start..].find("-->")`), and keep scanning from `start`. Add a test with `a --> b` before `<!-- x -->`.
- **Why:** Hidden comments in a README are where someone would put words meant for a model and not for a reader. A grounded deck's source should hold only what a person sees on GitHub. Each pass also scans from the start again, which is quadratic on a 512 KB file.
- **Effort:** S · **Impact:** S

### SEC-2. The outline prompt carries model-written reasoning, and the brief, unfenced, under a rider with no `RULE`
- **Location:** `crates/scaena-narrato/src/outline.rs:42`, `:55-82`
- **Proof:**
  ```
  let mut out = format!(
      "A presentation is to be written about \"{}\", for {}, to {}, {} long.",
      brief.topic, brief.audience, brief.goal, brief.length
  );
  ...
      "\n\nIt follows the {} arc: {} Why this arc: {}\n\nIts beats, with the slides each takes at this length:\n",
  ```
- **Verified:**
  - `grep -n "fenced\|RULE" crates/scaena-narrato/src/outline.rs crates/scaena-narrato/src/look.rs` → only `look.rs:71`, `:72`, `:89`, `:107`, `:117`.
  - `scaena-fence` is already a dependency of narrato (`Cargo.toml:18`).
  - The arc's `reasoning` is written by a model that read the sources' excerpt (`prompt.rs:242-246`).
- **Do:** Put the arc's `reasoning`, and the topic, audience, outcomes, modality, and voice, in a `scaena_fence::fenced("brief", …)` block, and end `OUTLINE_SYSTEM` with `scaena_fence::RULE`, as `look.rs` does. Add a hostile-reasoning test like `the_look_passes_words_reach_the_model_fenced`.
- **Why:** This is the one prompt between the sources and the story's claims that isn't fenced. A source can steer the arc's reasoning, and through it, every claim.
- **Effort:** S · **Impact:** S

### SEC-3. `icon_find`'s `limit` is unbounded, and every hit carries its full path
- **Location:** `crates/scaena-mcp/src/lib.rs:1452-1455`
- **Proof:**
  ```
  async fn icon_find(&self, Parameters(a): Parameters<IconFind>) -> Result<Json<scaena_ops::icons::Icons>, String> {
      let limit = a.limit.unwrap_or(20);
      blocking(move || Ok(scaena_ops::icons::find(&a.query, a.weight, limit))).await.map(Json)
  }
  ```
- **Verified:** I read the handler; there is no clamp. The set holds 1,434 icons (CLAUDE.md, PLAN 2.104).
- **Do:** Clamp to `limit.min(50)`. Add `paths: bool` (new, default false) so a first search returns names and tags alone.
- **Why:** One call with a large limit returns more than a megabyte into the agent's context and the server's memory. It is a cheap bound on a tool any connected client can call.
- **Effort:** S · **Impact:** S

## Usability

### USE-1. The Mac never speaks progress, results, or failures to VoiceOver
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/PromptWindow.swift:832-834`; `apps/mac/ScaenaKit/Sources/ScaenaKit/Assistant.swift:257`, `:264`
- **Proof:**
  ```
  private func say(_ words: String, failed: Bool = false) {
      steps.append(Line(words: words, failed: failed))
  }
  ```
- **Verified:** `grep -rn "AccessibilityNotification\|NSAccessibility.post\|UIAccessibility.post" apps/` → `# (no matches)`. The browser's progress list is a live region (`#prompting-progress`), and so is its transcript.
- **Do:** Post `AccessibilityNotification.Announcement` in `say` (at high priority when `failed`), in DeckView's status line task, and when the assistant adds a `.done` or `.failed` entry.
- **Why:** A blind presenter starting New from a Prompt hears nothing for minutes, and then nothing about why it failed. The browser already says it.
- **Effort:** S · **Impact:** M

### USE-2. Moving or removing an outline beat in the browser drops keyboard focus, and the Mac never focuses the outline
- **Location:** `web/src/prompt.ts:184-186`
- **Proof:**
  ```
  act("↑", "Move the beat up", () => li.previousElementSibling?.before(li));
  act("↓", "Move the beat down", () => li.nextElementSibling?.after(li));
  act("Remove", "Remove the beat", () => li.remove());
  ```
- **Verified:** `grep -n "focus()" web/src/prompt.ts` → `213: bigIdea.focus();`, `219:` (Add beat), `421: topic.focus();`. Nothing refocuses after a move or a remove. The prior plan deferred this; it is still open (the Mac's arrows came in #709).
- **Do:**
  - Refocus the pressed button after a move, and say "Beat moved to N of M" in `#prompting-progress`.
  - On Remove, focus the next or the previous row's `.name`.
  - Name the beat in each button's `aria-label`.
  - On the Mac, add a `@FocusState` for the big idea, set when the outline shows.
- **Why:** The outline is where the story is settled. With a keyboard, reordering it today means tabbing back from the top after every press.
- **Effort:** S · **Impact:** S

### USE-3. Three skills tell agents to do what the code won't: no `set_state` for transitions, a `spine.json` nothing reads, and `generate --slides` on a deck that exists
- **Location:** `skills/motion-pass/SKILL.md:56`; `skills/tighten-copy/SKILL.md:26`; `skills/author-deck/SKILL.md:93`
- **Proof:**
  ```
     - No semantic op sets a state's `transition`, `choreography`, or `hold`. Write JSON Patch at `/states/<i>/…`, …
  ...
  … `scaena export <bundle> --format spine` (the spine lives in `deck.json`, or in `spine.json` beside it).
  ...
     - A deck with no `meta.narrative` is measured against a talk's curve. Give it an arc with `scaena generate --slides`, …
  ```
- **Verified:**
  - `grep -rn "set_state\|time_motion" skills/` → `# (no matches)`. `SemanticOp::SetState` sets `transition` and `hold` (`crates/scaena-core/src/patch/mod.rs:521-546`).
  - `from_slides` begins with `crate::create::vacant(dir)?` (`narrative.rs:745`), so it refuses a folder that holds a deck.
  - This is distinct from the prior plan's USE-4, which covered `SKILL.md:94` and the missing tool rows.
- **Do:**
  - Rewrite motion-pass step 7 around `set_state` (`transition`, `hold`) and `time_motion`, keeping JSON Patch with `test` for `choreography` alone.
  - Say in tighten-copy that the spine lives in `deck.json`, read with `spine_read` and changed with `spine_update`.
  - In author-deck, give an existing deck an arc with a `deck_patch` `add` at `/meta/narrative`.
  - `crates/scaena-cli/tests/skills.rs` holds the skills to the code.
- **Why:** Agents follow skills literally. These three lead to index-based patches that land on the wrong state, to edits that change nothing, and to an error, or a lost deck, when an agent asks for an arc.
- **Effort:** S · **Impact:** M

### USE-4. An unknown layout, shader preset, or beat is refused with no near name and no list
- **Location:** `crates/scaena-ops/src/states.rs:153`; `crates/scaena-ops/src/backgrounds.rs:247`; `crates/scaena-ops/src/narrative/one.rs:90`
- **Proof:**
  ```
  Some(theme.layouts.get(name).ok_or_else(|| OpsError::new(format!("the theme has no layout `{name}`")))?)
  ...
  .ok_or_else(|| OpsError::new(format!("the theme has no shader preset `{name}`")))?,
  ...
  .ok_or_else(|| OpsError::new(format!("no beat `{id}` in the spine")))?,
  ```
- **Verified:** `grep -n "pub fn nearest" crates/scaena-core/src/patch/*.rs` → `ops.rs:458: pub fn nearest<'k>(whats: &str, id: &str, known: impl Iterator<Item = &'k str>) -> String {`. `OpsError::unknown_state` exists at `crates/scaena-ops/src/lib.rs:78`. #751 used both for arguments and states only. `one.rs:109` still says "no state `{state}` in the deck" bare.
- **Do:** Append `scaena_core::patch::nearest(…)` to each of the three errors, over `theme.layouts.keys()`, the theme's presets, and the spine's beat ids. Use `OpsError::unknown_state` at `one.rs:109`.
- **Why:** Agents guess layout names ("two-column" vs "Two Column"). "Did you mean `two-columns`?" saves a round trip and a fallback to a worse layout.
- **Effort:** S · **Impact:** S

### USE-5. When From a Prompt's choices fail to load, nothing says so, in the browser or on the Mac
- **Location:** `web/src/prompt.ts:372-375`; `apps/mac/ScaenaKit/Sources/Scaena/PromptWindow.swift:248-250`
- **Proof:**
  ```
  worker.onmessage = ({ data }: MessageEvent<FromWorker>) => {
    if (data.type === "briefing") fill(data.choices);
  };
  ```
  and `private static let offered = try? Generator.choices()`
- **Verified:** I read the handler; an `{type: "error"}` from the worker's catch is dropped. `worker.onerror` (`:375`) catches only a worker that fails to start.
- **Do:** In the browser, handle `data.type === "error"` with `step(…, true)` and disable Generate. On the Mac, keep the `Result` and `say(Why.said(error), failed: true)` on appear.
- **Why:** Goal, Length, Arc, and Persona stay empty while Generate stays live. The first paid call then fails on a brief that can't be filled in, with no clue why.
- **Effort:** S · **Impact:** S

## Attractiveness / Sexiness

Four items survived. The rest of this lens is the Story graph (#770), the theme chooser (the prior plan's ATTR-1), and presenting (in flight).

### ATTR-1. How the deck was made disappears the moment it opens: pictures not made, sources cut, texts not fitted
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/PromptWindow.swift:912-920`; `apps/mac/ScaenaKit/Sources/Scaena/ScaenaDocument.swift:66-70`; `web/src/prompt.ts:555`
- **Proof:**
  ```
  newDocument { ScaenaDocument(generated: session, said: made.words) }
  // Made: nothing is under way, so the window's close asks nothing.
  running = nil
  dismissWindow(id: PromptWindow.id)
  ```
  and `init(generated session: ScaenaSession, said: String) {`, which keeps one line.
- **Verified:** `ScaenaDocument(generated:said:)` takes one `String`. `steps` (the full log) is never passed on. The look's refusals alone survive, in `meta.narrative.refused` (#726).
- **Do:** Carry the run's lines into the opened document (`made: [String]`, new) and show them in the Story panel under a "How It Was Made" disclosure. In the browser, pass `#prompting-progress`'s lines to the Story tab under a `<details>`.
- **Why:** Gamma and Pitch show what they did. "2 pictures not made: the provider declined", "Source cut at 1 MB", and "Slide 6 not fitted" are exactly what a person needs to finish the deck, and today they vanish with the dialog.
- **Effort:** S · **Impact:** M

### ATTR-2. The browser forgets every From a Prompt choice between visits; the Mac keeps them
- **Location:** `web/src/prompt.ts:141`, `:249-268`; `apps/mac/ScaenaKit/Sources/Scaena/PromptWindow.swift:199-210`
- **Proof:**
  ```
  @AppStorage("generate.pictures") private var pictures = "openai"
  @AppStorage("generate.look") private var look = "illustration"
  @AppStorage("generate.quality") private var quality = "medium"
  @AppStorage("generate.see") private var see = true
  @AppStorage("generate.theme") private var theme = "dusk"
  ```
- **Verified:** `grep -n "localStorage" web/src/prompt.ts` → `# (no matches)`. `assistant/keys.ts:111-120` already keeps the assistant's settings in browser storage.
- **Do:** Keep `{theme, pictures, look, quality, see, outline, length, goal}` under a key of their own (new), written on submit and restored in `fill()`, falling back to `choices.defaults`, every read and write in `try`.
- **Why:** Someone who always makes photo decks in Salon resets five controls each time. That's a small friction at the most important moment of the product, and the Mac already avoids it.
- **Effort:** S · **Impact:** S

### ATTR-3. The Mac's assistant doesn't follow its answer as it streams
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/AssistantPanel.swift:162-164`
- **Proof:**
  ```
  .onChange(of: assistant.entries.count) {
      if let last = assistant.entries.last { scroll.scrollTo(last.id, anchor: .bottom) }
  }
  ```
- **Verified:** `grep -n "scrollTo" apps/mac/ScaenaKit/Sources/Scaena/AssistantPanel.swift` → `163:` only. `Assistant.heard(_:)` (`Assistant.swift:486`) grows the last entry in place, which doesn't change `count`. The browser scrolls on each chunk (`assistant/panel.ts:284`).
- **Do:** Bump a counter (new, e.g. `spoken`) in `Assistant.heard`, and scroll on its change unless the person has scrolled up.
- **Why:** A streamed answer that runs off the bottom of the panel reads as stalled. The browser feels alive here and the Mac doesn't.
- **Effort:** S · **Impact:** S

### ATTR-4. A deck made in the browser opens on its Story with focus left on the From a Prompt button
- **Location:** `web/src/editor.ts:463-470`
- **Proof:**
  ```
  async () => {
    // The Story tab shown reads the deck's health itself; the score goes in the line.
    $<HTMLButtonElement>("#tab-story").click();
    try {
      return await current?.story();
  ```
- **Verified:** I read the callback; no `focus()` follows `story()`. New (`:454`) moves focus into the deck.
- **Do:** Once `story()` resolves, focus the Story panel's score (give it `tabindex="-1"`). Its first finding is then one Tab away.
- **Why:** The made deck's arrival is the product's reveal. Landing on the story's score, with the first thing to do next to it, makes that moment read as intended, by keyboard and by screen reader alike.
- **Effort:** S · **Impact:** S

---

## First move

**FUNC-1. Slides written outside the model can't carry an outline** (Functionality)

Jay's goal is narrative quality, and the agent path is the one the server's instructions name first. Today an agent that writes its own slides, and any CLI run that resumes from kept slides, can't hand Scaena the big idea or the claims it wrote to. So the deck keeps no `meta.narrative.outline`. Its sections follow the arc's template, and health's `claim` and `bigIdea` checks, the two that ask "does this deck land its story?", never run.

The fix is small and uses what exists: `Generation::settle` already takes an outline after the arc is chosen. It's a few parameters threaded through `from_slides`, `DeckGenerate`, and the CLI's `Kept`. It touches no in-flight PR (#766 changes how a slide is read, not the brief). It also turns FUNC-2's evidence coaching and STAB-1's honest headlines into checks that fire on agents' decks too. **PERF-1** is the close second: the same agents pay for every `slide_write` in context, and it is a mechanical change.

## Dropped during verification

- **The slides prompt says "AT LEAST {target}" while the settled outline counts fewer** (`narrato/src/lib.rs:432`, `prompt.rs:349`): real, but `outline::block` says "this outline wins", and the count rule is Narrato's byte for byte. Left for a Narrato-prompt decision.
- **A slides answer with a sentence around its JSON reads as cut short** (`answer.rs:127-141`): real, but #701 asks the rest of a cut-short answer and sits on the same path. It belongs in #701's follow-up.
- **The outline prompt is unfenced against the sources** (agent lead): the sources are already wrapped by `containment::wrap_source_excerpt` (`outline.rs:89-94`). Only the reasoning and the brief remain, which is SEC-2.
- **Two `deck_create` calls to one path race** (agent lead): `write_dir` checks the folder again at save time. Not a data-loss path.
- **No `deck_read` of one slide** (prior plan's deferred list): `deck_inspect` takes a `state` (`lib.rs:651-654`), which covers it.
- **Health's coherence and claim checks read ASCII letters only** (`health.rs:1459-1462`, `coherence.rs:130`): real, but a script with no ASCII says nothing by design (`health.rs:1151-1152`). The Latin-with-accents case truncates consistently on both sides, and #757 and ADR-0032 own the language policy. Moved to Deferred.
- **"Turn" warnings fire on every deck** (agent lead): #740 (H4, H5) reshaped turns and runs. What remains is a threshold call on Vzestup's figures for Jay.
- **Lexicon false positives (prefix stems, caps as shouting) and negation** (agent leads): real, but `every_figure_is_vzestups_to_the_last_bit` pins the figures, so any change is an ADR-0032 departure for Jay. Moved to Deferred.

## Deferred

- **The Story arc graph is mouse-only, and its legend omits "drifted"** (`web/src/story.ts:149`, `:200-201`; `StoryPanel.swift:236-244`): sits on the same graph #770 redraws, so it should follow it.
- **Claim, big-idea, and turn findings rank last because coherence is unscored by keywords** (`story.ts:62-64`, `Health.swift:81`): a triage-order call beside S1 (#685).
- **The Look finding points at its slide by the number it had when made** (`health.rs:1418-1419`): store the slide's first state id in `Refused`. Small, and next after #726's follow-ups.
- **W425 fires on two empty claims** (`core/src/lint/narrative.rs:304-306`): a one-line guard plus a fixture.
- **With no brief, pacing docks a hand-made deck against 18 slides** (`health.rs:730-731`): needs Jay's call beside FUNC-1 of 10-25 (time-based pacing).
- **Health's word reading keeps only ASCII, and has no stemming** ("tripled" vs "triples" misses a claim): an ADR-0032 item.
- **The assistant's token counts leave out cache reads and Gemini's thoughts, and the writing tools' spend is never counted** (`providers.ts:218`, `:389`; `writing.ts:156`).
- **The assistant says "done" without linting after it edited** (`converse.ts:188`, `Assistant.swift:261-265`).
- **The deck's facts sit in the system prompt, so an edit between questions throws away the cached conversation** (`providers.rs:321-325`): a cache-policy change beside 10-24b PERF-2.
- **The Mac re-decodes every frame and re-reads states for each streamed chunk** (`AssistantPanel.swift:348`, `:382`).
- **The Mac simulates a whole-deck delete on the main actor after each edit, to title one menu item** (`DeckView.swift:797`, `:1745-1746`).
- **A source that fails to compile on the Mac clears the reason** (`DeckEditor.swift:454-458`).
- **The Mac's inspector arranges, groups, and places silently when the engine refuses** (`Inspector.swift:504-533`).
- **`deck_generate` records its first version as `deck_create`** (`lib.rs:1592-1594`).
- **`slide_write`'s `left` hides a finding whose fix didn't mend it** (`one.rs:504-506`).
- **Deep research is asked and written in English whatever the deck's language** (`cli/src/generate.rs:373`, `research.rs:32-38`).
- **An outline answer missing a beat's claim drops that beat without a word** (`outline.rs:190-201`): beside #680.
- **No MCP tool removes unused files** (`lib.rs:554-557`).

Run `/burn-down` to execute this plan.
