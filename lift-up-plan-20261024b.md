# Lift-up plan — Scaena, 2026-10-24 (b)

- **Baseline:** `main` @ `a3a3db4` (clean tree).
  - The audit was pinned at `99fcbff`, after `git fetch origin main`.
  - The drift re-check before write-up found `origin/main` at `a3a3db4`, one commit on (#579, presenter notes size). It touches `web/src/player.ts`, `Presenting.swift`, `Show.swift`, and tests.
  - Every item citing `web/src/player.ts` was re-read on `a3a3db4`, and its lines were re-pinned there. No other cited file changed.
- **Scope:** the full product. The weighting follows what was asked:
  1. narrative quality and the AI-first surfaces first: generation, the outline, the look pass, deck health and its findings, the assistant, and the agent tools;
  2. then the editor and presenting.
- **Platforms:** web, macOS, and iPadOS. Detected from `web/package.json` + Vite, `apps/mac/ScaenaKit/Package.swift`, and `apps/ipad/project.yml`. There is also a CLI and an MCP server.
- **Anchors:**
  - Keynote, for the editor and presenting.
  - Gamma, Pitch, and Beautiful.ai, for a generated deck.
  - For a generated deck's story, what a strong human presentation coach would say of it: one big idea, claims that build on it, evidence for each claim, an opening that hooks, and an ending that lands.
- **Excluded, and not repeated here:**
  - every item of `lift-up-plan-20261019.md`, `lift-up-plan-20261023.md` (narrative), and `lift-up-plan-20261024.md`. Their unlanded items stay on those plans: 10-24's PERF-3, PERF-4, FUNC-1, FUNC-2, FUNC-3, STAB-3, REL-5, USE-2, USE-5, ATTR-2, and ATTR-5;
  - the open PRs #480, #502, #503, #563, #564, #568, #570, #574, #593, #596, #597, #599, #604, #605, #607, #608, #610, #611, #612, and #613;
  - the queued items: ATTR-3, USE-3, quote marks by language, the CJK unit, the slot `maxSize`, PLAN 2.136, and O.3e;
  - everything waiting on Jay.

  A gap in an in-flight PR is noted where one was found (SEC-4).
- **Coverage:** five read-only survey agents mapped five areas:
  - generation;
  - health, the assistant, and the agent tools;
  - the browser editor and player;
  - the Mac and iPad apps;
  - the engine, store, CLI, and serve.

  I re-ran every proof and every grep below myself, on this checkout. Nothing was built or run.
- **Model:** a frontier-tier session.

---

## Performance

### PERF-1. Ask the look pass's rounds at once, not one after another
- **Where:** `crates/scaena-ops/src/narrative.rs:548-557` (`made`'s look loop) and `web/src/generate.ts:507-514`.
- **Proof:**
  ```rust
  for round in look_rounds(seen.len()) {
      let Some(request) = generation.look_request(key, &seen[round.clone()], &drawn[round.clone()])
      else {
          continue;
      };
      ...
      let reply = transport.send(&request);
  ```
- **Verified:**
  - `grep -n "send_all\|Promise.all\|look_rounds(" crates/scaena-ops/src/narrative.rs web/src/generate.ts` →
    ```
    crates/scaena-ops/src/narrative.rs:115:    fn send_all(&mut self, requests: &[Request]) -> Vec<Reply> {
    crates/scaena-ops/src/narrative.rs:159:        let wait = p.answer(transport.send_all(&requests));
    crates/scaena-ops/src/narrative.rs:328:pub fn look_rounds(slides: usize) -> impl Iterator<Item = std::ops::Range<usize>> {
    crates/scaena-ops/src/narrative.rs:548:                for round in look_rounds(seen.len()) {
    web/src/generate.ts:342:      const replies = await Promise.all(requests.map(send));
    ```
  - `send_all` is used for the pictures (`:159`) and never for the look. `Promise.all` is used for the pictures (`:342`) and never for the look.
- **Do:**
  - Build every round's request first.
  - Send them with `Transport::send_all`, then read each answer with `generation.look_answer` in round order.
  - Do the same in `web/src/generate.ts` with `Promise.all(requests.map(send))`, as the pictures do at `:342`.
  - In each round's prompt, add one line naming the layouts of the slides just before and after it (new). That keeps "three or more slides in a row drawn alike" true across a round's edge.
- **Why:** a 45-slide deck makes four vision calls in a row, and each is tens of seconds. That wait sits between "slides written" and the deck opening, the moment a person is watching most closely. Gamma opens in about the time of one call.
- **Effort:** S · **Impact:** M

### PERF-2. Let the assistant's prompt cache survive an edit: stable text first, and a breakpoint on the conversation
- **Where:**
  - `web/src/assistant/prompt.ts:83-88`, with its twin `crates/scaena-chat/src/prompt.rs` (`system_of`);
  - `crates/scaena-chat/src/providers.rs:298` and `web/src/assistant/providers.ts:172`.
- **Proof:**
  ```ts
  The deck: ${deck.title ? `"${deck.title}", ` : ""}${deck.states.length} state${...} (${deck.states.join(", ")})...${story(deck)}

  The resources:
  ...
  --- scaena://skills/author-deck ---
  ${authorDeck}`;
  ```
- **Verified:**
  - `grep -n "The deck: \|The resources:\|--- scaena://skills/author-deck" web/src/assistant/prompt.ts` →
    ```
    83:The deck: ${deck.title ? ...}.${story(deck)}
    85:The resources:
    88:--- scaena://skills/author-deck ---
    ```
  - `grep -n "cache_control" crates/scaena-chat/src/providers.rs web/src/assistant/providers.ts` →
    ```
    crates/scaena-chat/src/providers.rs:298:        "system": [{ "type": "text", "text": ask.system, "cache_control": { "type": "ephemeral" } }],
    web/src/assistant/providers.ts:172:      system: [{ type: "text", text: ask.system, cache_control: { type: "ephemeral" } }],
    ```
- **Do:**
  - Reorder the system prompt in both twins. The fixed rules, the resources list, and the author-deck skill go first. The deck's facts (title, states, story) go last, as their own system block.
  - Put the system prompt's `cache_control` on the stable block only.
  - Add a second `cache_control` to the last content block of the newest message in each Anthropic request, in both `providers.rs` and `providers.ts`.
  - Re-record `tests/assistant/exchanges.json` with `web/exchanges.mjs`.
  - #604 also edits `prompt.ts` and `prompt.rs`. Land this on top of it.
- **Why:**
  - Adding a slide or editing a claim changes the deck facts. Those sit before roughly 18 KB of skill text, so every question after an edit misses the cache on the whole system prompt.
  - Within a question, each of up to 32 tool rounds re-sends the growing conversation uncached.
  - This is the assistant's main cost and latency lever, at no cost in quality.
- **Effort:** S · **Impact:** M

### PERF-3. Elide old tool results from the conversation as old frames are elided
- **Where:** `crates/scaena-chat/src/lib.rs:85-104` (`recent`) and `web/src/assistant/converse.ts:31-52`.
- **Proof:**
  ```rust
  for r in results.iter_mut().rev() {
      if r.png.is_none() {
          continue;
      }
  ```
- **Verified:**
  - `grep -n "r.png.is_none\|if (!r.png)\|byte\|budget" crates/scaena-chat/src/lib.rs web/src/assistant/converse.ts` →
    ```
    crates/scaena-chat/src/lib.rs:93:            if r.png.is_none() {
    web/src/assistant/converse.ts:44:          if (!r.png) return r;
    ```
    There is no byte budget. Only frames are elided.
- **Do:**
  - In `recent` (both twins), keep the newest results whole within a byte budget (new; about 60 KB). Replace each older result's `json` with its first line and "(no longer shown: call the tool again to see it)", as frames are replaced.
  - Where a provider answers `tooLong` (`web/src/words.ts:215`), elide once more at half the budget and ask again, before showing the refusal.
- **Why:**
  - Every `deck_read`, `deck_inspect`, and `deck_health` result stays in the conversation, and is re-sent on every round of every later question.
  - A long session grows until the provider refuses it. The person is then told to "start again" and loses the thread.
- **Effort:** S · **Impact:** M

### PERF-4. Resolve a deck's states and fingerprint it once per export, not once per state
- **Where:** `crates/scaena-engine/src/render.rs:187-200` (`Engine::frame`) and `:1384-1389` (`fingerprint`). It is called once per state from `crates/scaena-ops/src/export.rs:442`, `:536`, and `crates/scaena-ops/src/sheet.rs:130`.
- **Proof:**
  ```rust
  let (deck, theme) = project(req.deck, req.theme, req.format)?;
  let (deck, theme) = (deck.as_ref(), theme.as_ref());
  let snapshots = scaena_core::resolve_states(deck)?;
  let i = state_index(&snapshots, req.state)?;
  let timeline = self.timeline_to(deck, theme, req.data, i)?;
  ```
  ```rust
  fn fingerprint(deck: &Deck, theme: &Theme, data: &DataFiles) -> u64 {
      let mut h = std::hash::DefaultHasher::new();
      serde_json::to_vec(deck).unwrap_or_default().hash(&mut h);
  ```
- **Verified:**
  - `grep -n "fingerprint(\|resolve_states(deck)" crates/scaena-engine/src/render.rs` → `190:`, `215:`, `266: let key = fingerprint(deck, theme, data);`, `271:`, `317:`, `328:`, `347:`, `367:`, `421:`, `455:`, `840:`, and `1384:fn fingerprint(...)`.
  - `grep -n "engine.frame(&req)" crates/scaena-ops/src/export.rs crates/scaena-ops/src/sheet.rs` →
    ```
    crates/scaena-ops/src/export.rs:442:            let list = engine.frame(&req)?.display_list;
    crates/scaena-ops/src/export.rs:536:    Ok(engine.frame(&req)?.display_list)
    crates/scaena-ops/src/sheet.rs:130:        let list = engine.frame(&req)?.display_list;
    ```
- **Do:**
  - Add `Engine::frames_at_rest(deck, theme, data, states, format)` (new). It projects, resolves the snapshots, and builds the timeline once, then draws each state at rest.
  - Use it in the PNG, SVG, spine, and PDF export paths and in `sheet.rs`.
  - In `fingerprint`, return an error rather than hashing an empty buffer when serialization fails.
- **Why:**
  - Each per-state call re-serializes the deck and theme, hashes every data byte, and resolves every state. A deck's PNG, PDF, or spine export is therefore O(states²·nodes).
  - The editor's Export… runs the same code in the worker, where a 60-slide deck's PDF stalls the tab.
- **Effort:** M · **Impact:** M

### PERF-5. Keep the strip's tiles across an edit; redraw only what changed
- **Where:** `web/src/strip.ts:100-118` (`states`), called on every compile from `web/src/editor.ts:1740`.
- **Proof:**
  ```ts
  function states(next: Slot[], shown: number) {
    slots = next;
    ...
    list.innerHTML = next
      .map((s, i) => {
  ...
    for (const s of next) draw(s.state);
  ```
- **Verified:** `grep -n "innerHTML\|for (const s of next) draw" web/src/strip.ts` →
  ```
  104:    list.innerHTML = next
  115:    for (const s of next) draw(s.state);
  ```
- **Do:**
  - Diff `next` against the `<li>`s by `data-state`.
  - Keep each `<li>` and its canvas whose state stays, and update its label, cue, and selection in place.
  - Insert or remove only the rest, and `draw` only new tiles or those whose digest changed. `thumbs` already keys by digest.
- **Why:**
  - Every keystroke that compiles recreates every tile and re-blits every thumbnail with `putImageData`.
  - On a 60- to 100-slide deck that is visible jank while typing.
  - It also drops the strip's scroll and focus state that the code restores by hand.
- **Effort:** S · **Impact:** M

---

## Functionality

### FUNC-1. Read the settled outline in deck health: does the deck land its big idea, and does each beat carry its claim?
- **Where:** `crates/scaena-ops/src/health.rs:399-` (`health_in`) and `Kind` at `:116-133`. The outline is written to `meta.narrative.outline` at `crates/scaena-ops/src/narrative.rs:1823-1825`.
- **Proof:**
  ```rust
  pub enum Kind {
      /// The slide has no headline.
      Title,
      ...
      /// A state of it has an error lint finds: it does not stand as it is to be read.
      Layout,
  }
  ```
- **Verified:** `grep -rn "outline" crates/scaena-ops/src/health.rs crates/scaena-narrato/src/health/` →
  ```
  crates/scaena-ops/src/health.rs:798:            outline: None,
  crates/scaena-narrato/src/health/intensity.rs:160:    ("outline", -0.45),
  crates/scaena-narrato/src/health/intensity.rs:496:            "overview summary appendix agenda status update recap outline inventory",
  crates/scaena-narrato/src/health/intensity.rs:534:        let list = "Appendix A: ...summary outline for the standard baseline.";
  ```
  The only code hit is a test fixture's `outline: None`. The rest are lexicon words.
- **Do:**
  - Read `meta.narrative.outline` in `health_in`, where it exists.
  - Add a deck-level `Kind::BigIdea` (new). It fires when no slide in the last beat shares the big idea's content words, read as `coherence` reads words. Its `ask` quotes the big idea and asks for the closing to land it.
  - Add a `Kind::Claim` (new) for each outline beat whose slides' words share nothing with its claim. Its `ask` is "Check the claim", with the outline's claim, and it reuses the writing task's prompt.
  - Show the big idea above the curve in `web/src/story.ts` and `StoryPanel.swift`.
- **Why:**
  - The outline is the most coach-like act in the pipeline: the person settles the one sentence the audience should remember. After that, nothing reads it. Health scores the deck against the arc's curve, never against what the person agreed it must say.
  - A coach's first question is "where do you land your big idea?". Today health can't ask it.
- **Effort:** M · **Impact:** L

### FUNC-2. Make the narrative lint warnings and per-slide density health findings, each with its ask
- **Where:**
  - `crates/scaena-ops/src/health.rs:448-457` (balance, deck average only);
  - `:731-737` (only lint *errors* become findings);
  - W420, W421, W424, and W425 in `crates/scaena-core/src/lint/narrative.rs`.
- **Proof:**
  ```rust
  let errors: Vec<&scaena_core::lint::Finding> = (lint.iter())
      .filter(|f| f.severity == Severity::Error && f.state.as_deref().is_some_and(|s| ids.contains(&s)))
      .collect();
  ```
  ```rust
  if balance.avg_bullets_per_slide < 2.0 {
      short
          .push(format!("slides make {} points each, where two to five read as full", balance.avg_bullets_per_slide));
  ```
- **Verified:**
  - `grep -n "Severity::Error &&\|avg_bullets_per_slide <\|Kind::Empty," crates/scaena-ops/src/health.rs` →
    ```
    448:    if balance.avg_bullets_per_slide < 2.0 {
    591:                Kind::Empty,
    736:            .filter(|f| f.severity == Severity::Error && f.state.as_deref().is_some_and(|s| ids.contains(&s)))
    ```
  - `grep -n "\"W42[0145]\"" crates/scaena-core/src/lint/narrative.rs` →
    ```
    63:        "W420"
    95:        "W421"
    252:        "W424"
    294:        "W425"
    ```
- **Do:**
  - In `health_in`, turn the W420, W421, W424, and W425 warnings at a slide's states into findings. Use a `Kind::Story` (new), whose `says` is the code's `scaena_core::words::said` sentence and whose `ask` names the fix ("Say this slide's claim on it", "Make slide 6 say something slide 5 doesn't").
  - Add a per-slide `Kind::Dense` (new): more than five points, or a body over a word ceiling. Its ask is "Tighten" or "split this slide".
  - Add `Kind::Notes` (new): a content slide with no speaker notes. Its ask is "Draft notes", the writing tool's `Notes` task.
  - Add a deck-level pacing finding when `pacing` is outside the brief's range. It names the weakest-turn slides as candidates to cut.
- **Why:**
  - The deck-level `says` ("slides make 6.4 points each") is a measure, not coaching.
  - A coach names the slide: "slide 7 makes nine points", "slide 12 has nothing for you to say", "slides 5 and 6 make the same claim".
  - Lint already finds the narrative breaks, but at warning severity they never reach the Story tab or the assistant's `deck_health`.
- **Effort:** M · **Impact:** L

### FUNC-3. Ground the outline in the sources whole, the brief whole, and ask it for the opening's hook
- **Where:** `crates/scaena-narrato/src/lib.rs:386-392` (`Step::Outline` sends no documents) and `crates/scaena-narrato/src/outline.rs:19`, `:55-85` (`prompt`).
- **Proof:**
  ```rust
  Step::Outline => {
      ...
      let text = outline::prompt(&self.brief, arc, &chosen.reasoning, &plan);
      (outline::OUTLINE_SYSTEM, text, Shape::Outline, room(outline::OUTLINE_MAX_TOKENS), 0.3, &[][..])
  }
  ```
  ```rust
  let mut out = format!(
      "A presentation is to be written about \"{}\", for {}, to {}, {} long.",
      brief.topic, brief.audience, brief.goal, brief.length
  );
  ```
- **Verified:**
  - `grep -n -i "voice\|modality\|hook\|documents" crates/scaena-narrato/src/outline.rs` → `# (no matches)`
  - `grep -n "outline::OUTLINE_SYSTEM, text" crates/scaena-narrato/src/lib.rs` → `391: (outline::OUTLINE_SYSTEM, text, Shape::Outline, room(outline::OUTLINE_MAX_TOKENS), 0.3, &[][..])`
  - `grep -n "pub struct Document\|pub voice\|pub modality" crates/scaena-narrato/src/providers.rs crates/scaena-narrato/src/brief.rs` →
    ```
    crates/scaena-narrato/src/providers.rs:144:pub struct Document {
    crates/scaena-narrato/src/brief.rs:120:    pub modality: Option<String>,
    crates/scaena-narrato/src/brief.rs:131:    pub voice: Option<String>,
    ```
- **Do:**
  - Pass `documents` to the outline step, as `Step::Slides` does. A deck grounded only in a PDF otherwise has its claims written blind.
  - Take `SOURCE_UNITS` from each `=== SOURCE ===` part in turn, not only the head of the first.
  - Add `brief.voice` and `brief.modality` to `outline::prompt`.
  - Ask the first beat's claim to open on a tension, a surprising figure, or a question drawn from the material (the hook). Leave the closing to queued ATTR-3.
  - The outline is Scaena's own call (ADR-0026 keeps Narrato's prompts byte for byte), so this changes no Narrato text.
- **Why:**
  - Claims the person settles are written from the first 6,000 characters of text and never from a PDF.
  - A coach's second question, after the big idea, is "how do you open?". Nothing asks the outline for an opening that earns attention.
- **Effort:** S · **Impact:** L

### FUNC-4. Let the person weight a beat: keep the slide counts they set in the outline
- **Where:** `crates/scaena-narrato/src/outline.rs:141-163` (`settled`) and `web/src/prompt.ts:137-146` (the beat row).
- **Proof:**
  ```rust
  for (beat, count) in beats.iter_mut().zip(beats::allocate(target.max(1), outline.beats.len().max(1))) {
      beat.slides = count;
  }
  // Fewer beats than before leave the spread short: spread again over those that stand.
  let n = beats.len();
  for (beat, count) in beats.iter_mut().zip(beats::allocate(target.max(n), n)) {
      beat.slides = count;
  }
  ```
- **Verified:**
  - `grep -n "beats::allocate" crates/scaena-narrato/src/outline.rs` →
    ```
    155:    for (beat, count) in beats.iter_mut().zip(beats::allocate(target.max(1), outline.beats.len().max(1))) {
    160:    for (beat, count) in beats.iter_mut().zip(beats::allocate(target.max(n), n)) {
    ```
  - `grep -n "pub fn allocate" crates/scaena-narrato/src/beats.rs` → `10:pub fn allocate(slides: usize, beats: usize) -> Vec<usize> {`
  - `grep -n "slides.textContent\|type=\"number\"\|stepper" web/src/prompt.ts` → `146: slides.textContent = beat.slides === 0 ? "no slides" : ...` (shown as text only).
- **Do:**
  - In `settled`, keep each beat's `slides` as given when they sum to the target or near it. Otherwise scale them to `target` (new helper, proportional, each at least 1). Fall back to `allocate` only when none is given.
  - Delete the dead first loop.
  - Make the count in `prompt.ts`'s beat row a number field. Do the same in the Mac's and the iPad's outline view (`PromptView`'s `outlineView`).
  - `--from-outline FILE` already carries `slides`. It starts being honoured.
- **Why:** a coach weights the evidence: three slides for the proof, one for the problem. Today the counts are shown, cannot be changed, and are discarded and spread evenly even when a file gives them.
- **Effort:** S · **Impact:** M

### FUNC-5. Give agents a tool to put a picture into the bundle
- **Where:** the MCP tools, `crates/scaena-mcp/src/lib.rs` (`data_attach` at `:1218-1229` is the only "put a file in" tool), and `docs/schema/mcp/`.
- **Proof:**
  ```rust
  #[tool(description = "Attach a CSV or JSON data file: copy it into the bundle's `data/` and declare it as a \
      source charts and tables name as `@id`, each column typed (inferred from its values without `schema`).")]
  async fn data_attach(
  ```
- **Verified:**
  - `ls docs/schema/mcp/` → `background_set.json data_attach.json data_edit.json deck_create.json deck_diff.json deck_export.json deck_find.json deck_generate.json deck_health.json deck_history.json deck_inspect.json deck_lint.json deck_patch.json deck_read.json deck_render.json icon_find.json map_fetch.json spine_read.json spine_update.json state_start.json theme_apply.json theme_edit.json`
  - `grep -rn "image_add\|file_add\|asset_add" crates/scaena-mcp/src crates/scaena-ops/src` → `# (no matches)`
  - `grep -n "fn readable\|async fn data_attach" crates/scaena-mcp/src/lib.rs` → `129: fn readable(...)`, `1220: async fn data_attach(`
  - `grep -n "pub use save::" crates/scaena-store/src/lib.rs` → `29:pub use save::{SaveOptions, Saved, Saving, place, sha256, zip};`
- **Do:**
  - Add `image_add` (new) to `scaena_ops` and the MCP server. It takes a PNG or JPEG `file`, held by `self.readable` as `data_attach` is, and optional `into` (an image node), `at`, and `alt`.
  - It copies the picture to `assets/` named by `scaena_store::sha256`, scales one past 8192 px a side as the browser does, and puts it in the image named, or the slide's empty place for a picture, or a box. This is one patch, as `map_fetch` places its picture.
  - Add it to the assistant's tools where the client holds the bytes, and to `docs/schema/mcp/` by `just bless`.
- **Why:**
  - An agent can make a deck, attach data, fetch a map, and set a background. It cannot use the logo, the product shot, or the chart image the user hands it.
  - That is the first thing a person asks of an agent building their deck. The editors take a dropped picture, so agent parity (ADR-0009) is missing exactly here.
- **Effort:** M · **Impact:** M

---

## Stability

### STAB-1. The Story tab and the assistant never see the slides lint finds broken
- **Where:**
  - `crates/scaena-generator/src/lib.rs:68`: the browser's Story tab and its assistant;
  - `crates/scaena-session/src/assistant.rs:503`: the assistant's `deck_health`;
  - `crates/scaena-ffi/src/lib.rs:385` and `:1979`: the Mac's and iPad's Story panel.
- **Proof:**
  ```rust
  let health = scaena_ops::health::health(&deck).map_err(error)?;
  ```
  The doc comment above `health_in` reads: "Each slide with an error in one of its states is a finding of its own, ahead of every other".
- **Verified:** `grep -rn "health::health(\|health_in(" crates --include=*.rs | grep -v "^crates/scaena-ops/src/health.rs"` →
  ```
  crates/scaena-ffi/src/lib.rs:385:        serde_json::to_value(scaena_ops::health::health(laying.0.deck()).map_err(said)?).map_err(said)
  crates/scaena-ffi/src/lib.rs:1979:        "health" => value(serde_json::to_value(scaena_ops::health::health(s.deck()).map_err(said)?))?,
  crates/scaena-mcp/src/lib.rs:1314:            scaena_ops::health::health_in(&b.deck, &scaena_ops::lint::lint(&b)?.findings)
  crates/scaena-generator/src/lib.rs:68:    let health = scaena_ops::health::health(&deck).map_err(error)?;
  crates/scaena-cli/src/main.rs:1411:    let h = scaena_ops::health::health_in(&b.deck, &scaena_ops::lint::lint(&b)?.findings)?;
  crates/scaena-session/src/assistant.rs:503:                Called::of(scaena_ops::health::health(self.deck())?)
  ```
  `health` is `health_in(deck, &[])` (`health.rs:399-401`).
- **Do:**
  - Pass the session's last whole-deck lint findings into `health_in` at each of the four call sites. The editors already lint every state once typing stops (`Session::lint`, `crates/scaena-session/src/editor.rs:358`; `lint_laid`, `:399`).
  - For the browser, the generator's `health` takes the findings as JSON beside the deck.
  - Add a test that a deck with an E100 shows its `Kind::Layout` finding through the session's `deck_health`.
- **Why:** #532 made "a slide lint finds broken comes first" the top of every health report, but only the CLI and MCP receive it. In the editors, where people read the Story, a slide whose words overflow scores as healthy, and the assistant asked to "fix the story" never hears of it.
- **Effort:** S · **Impact:** M

### STAB-2. A worker that dies outside a message leaves every waiting request hanging
- **Where:** `web/src/stage.ts:265-268` (`hear`), compared with `halt` at `:1235-1239`.
- **Proof:**
  ```ts
  private hear(worker: Worker) {
    worker.onmessage = ({ data }: MessageEvent<FromWorker>) => this.listen(data);
    worker.onerror = (e) => this.onError(new Error(e.message));
  }
  ```
  ```ts
  private halt(error: Error) {
    const waiting = [...this.waiting.values()];
    this.close();
    for (const { reject } of waiting) reject(error);
  }
  ```
- **Verified:** `grep -n "worker.onerror\|onmessageerror\|private halt" web/src/stage.ts` →
  ```
  267:    worker.onerror = (e) => this.onError(new Error(e.message));
  306:        worker.onerror = (e) => reject(new Error(e.message || "the worker failed to start"));
  1235:  private halt(error: Error) {
  ```
- **Do:**
  - In `hear`, call `this.halt(new Error(\`the engine stopped: ${e.message}\`))` before `onError`.
  - Set `worker.onmessageerror` to the same.
  - The editor's existing STAB-2 recovery (`restart`) then takes over, as it does for a panic.
- **Why:** an out-of-memory error or a script error outside a message handler fires `onerror`, not a crash message. Every `await stage.…` then waits forever. "Opening…", Export…, and Make never end, and Save waits behind them.
- **Effort:** S · **Impact:** M

### STAB-3. Bound a theme's grid and a video's length, so neither can run the engine out of memory or time
- **Where:**
  - `crates/scaena-engine/src/layout.rs:59-79` (`Grid::from_theme`);
  - `crates/scaena-ops/src/export.rs:735-742`;
  - the schemas' `grid.columns`, `grid.rows`, `State.hold`, and `Beat.duration`.
- **Proof:**
  ```rust
  let count = |key: &str, n: u32| -> Result<usize, EngineError> {
      (n >= 1).then_some(n as usize).ok_or_else(|| EngineError::Theme(format!("grid.{key} must be ≥ 1")))
  };
  ...
      (0..n).map(|i| start + i as f32 * (size + gutter)).map(|s| (s, s + size)).collect()
  ```
  ```rust
  if length <= 0.0 {
  ...
  let frames = (length * f64::from(fps) / 1000.0).ceil() as u64;
  ```
- **Verified:**
  - `grep -n "grid.{key} must be\|(0..n).map" crates/scaena-engine/src/layout.rs` →
    ```
    61: ... EngineError::Theme(format!("grid.{key} must be ≥ 1"))
    78:            (0..n).map(|i| start + i as f32 * (size + gutter)).map(|s| (s, s + size)).collect()
    ```
  - In the schemas, `Grid.columns` is `{"type": "integer", "minimum": 1}` and `State.hold` is `{"type": "number", "minimum": 0}`, with no maximum (read from `docs/schema/theme.schema.json` and `deck.schema.json`).
  - `grep -n "let frames = (length\|length <= 0.0\|is_finite" crates/scaena-ops/src/export.rs` →
    ```
    735:        if length <= 0.0 {
    741:        let frames = (length * f64::from(fps) / 1000.0).ceil() as u64;
    ```
    There is no finiteness check and no cap.
- **Do:**
  - Refuse `grid.columns` or `grid.rows` over 96 with `EngineError::Theme`, and refuse a track size ≤ 0 (margins and gutters past the canvas).
  - Refuse a video whose length is not finite or is over 24 h, saying which state's hold or beat's duration made it.
  - Add `maximum`s to the schemas (by the model, then `just bless`). That narrows a value, so also raise `OLDEST_THEME_FORMAT_VERSION` or `OLDEST_FORMAT_VERSION` per CLAUDE.md rule 3, or read the older formats as they drew.
- **Why:**
  - CLAUDE.md requires that a deck or theme the engine cannot serve is "an error that says why, never a panic or a hang".
  - A shared `.scaena` whose theme says `columns: 4000000000` aborts the CLI and kills the editor's worker.
  - `hold: 1e300` makes `export --format mp4` run effectively forever.
- **Effort:** S · **Impact:** S

### STAB-4. The fit pass keeps a rewrite that is still too long, and never looks again
- **Where:** `crates/scaena-narrato/src/fit.rs:55-68` (`read`) and `crates/scaena-ops/src/narrative.rs:679-697` (`fitted`).
- **Proof:**
  ```rust
  .filter(|(id, text)| {
      longs.iter().any(|l| &l.id == id && !text.is_empty() && text.chars().count() < l.text.chars().count())
  })
  ```
- **Verified:** `grep -n "text.chars().count() < l.text.chars().count()\|most" crates/scaena-narrato/src/fit.rs` →
  ```
  9:/// A text that runs past its place: the node it is, its words, and the most characters that
  15:    pub most: usize,
  24:        "Each text below is set on a slide and runs past its place. Rewrite each in at most the number of characters given. ...
  31:        out.push_str(&format!("id: {}\nat most: {} characters\ntext:\n{}\n\n", long.id, long.most, long.text));
  66:            longs.iter().any(|l| &l.id == id && !text.is_empty() && text.chars().count() < l.text.chars().count())
  79:            most: 40,
  86:        assert!(p.contains("id: a-header\nat most: 40 characters\ntext:\nFreight emissions"));
  ```
  `most` is asked for but never checked.
- **Do:**
  - In `fit::read`, keep a rewrite only where `text.chars().count() <= l.most + l.most / 10`.
  - Keep the first rewrite per id. Today a duplicate id applies twice in `fitted`.
  - In `made`, after `fitted`, run `overflowing` once more. Ask again for what still runs past its place, one more round at most, and say in `unfitted` what still does.
- **Why:** a headline rewritten one character shorter but still three lines long is accepted, and the deck lands with the E100 the fit pass exists to remove. These overflows are the most visible flaw in a generated deck next to Gamma's.
- **Effort:** S · **Impact:** M

### STAB-5. The player's and presenter's async handlers reject into nothing
- **Where:** `web/src/player.ts:631-634` (`formatPicker.onchange`) and `:1044` (the served reload).
- **Proof:**
  ```ts
  formatPicker.onchange = async () => {
    slots = await stage.timeline(format());
    go(Math.min(stage.at.index, slots.length - 1));
  };
  ```
  ```ts
  void Promise.all([now.reload(), upNext.reload()]).then(async () => {
  ```
- **Verified:** `grep -n "formatPicker.onchange = async\|void Promise.all(\[now.reload\|unhandledrejection" web/src/player.ts` (on `a3a3db4`) →
  ```
  631:  formatPicker.onchange = async () => {
  1044:        void Promise.all([now.reload(), upNext.reload()]).then(async () => {
  ```
  There is no `unhandledrejection` handler in the page. The only one is the worker's, at `web/src/worker.ts:123`.
- **Do:**
  - Catch both, and route each to the player's status line as `reload` routes its errors (`status.textContent = \`Error: …\``).
  - Add a page-level `unhandledrejection` listener that does the same, so a missed one is said, not silent.
- **Why:**
  - A format switch on a deck that no longer lays out in it, or a `scaena serve` save that breaks the deck mid-talk, leaves the player frozen on the old slide with nothing said.
  - The presenter sees nothing wrong until the room does.
- **Effort:** S · **Impact:** S

---

## Reliability

### REL-1. Ask a busy provider again for the look and the fit, as the slides and pictures are
- **Where:**
  - `crates/scaena-ops/src/narrative.rs:557` (look) and `:604` (fit);
  - `web/src/generate.ts:514` (look) and `:591` (fit);
  - the same steps in the Mac's `Generator`.
- **Proof:**
  ```rust
  let reply = transport.send(&request);
  match generation.look_answer(reply.status, &reply.status_text, &reply.body, &seen[round.clone()]) {
      Ok(answered) => changes.extend(answered),
      Err(why) => {
  ```
  ```rust
  let reply = transport.send(&request);
  let rewrites = match generation.fit_answer(reply.status, &reply.status_text, &reply.body, &longs) {
      Ok(rewrites) => rewrites,
      Err(why) => {
          made.unfitted = Some(why.to_string());
          return Ok((made, Some(write)));
  ```
- **Verified:**
  - `grep -n "transport.send(&request)" crates/scaena-ops/src/narrative.rs` →
    ```
    499:        let reply = transport.send(&request);
    557:                    let reply = transport.send(&request);
    604:    let reply = transport.send(&request);
    ```
  - `grep -n "TRIES\|providers::wait\|busy" crates/scaena-ops/src/narrative.rs | head -8` → `109:`, `138:`, `140:const TRIES: usize = scaena_narrato::TRIES;`, `145:`, `170:`, `207:`, `219:`, `241:`. Every hit is in the pictures' `Painting`.
  - `grep -n "await send(JSON.parse(asked))\|at.wait != null" web/src/generate.ts` →
    ```
    203:        const reply = await send(JSON.parse(asked));
    216:        if (at.wait != null) {
    514:      const reply = await send(JSON.parse(asked));
    591:  const reply = await send(JSON.parse(asked));
    ```
    Only the arc, outline, and slides loop (`:203`) waits and retries.
- **Do:**
  - Wrap the look's and the fit's `send` in the same busy-retry the pictures use: up to `scaena_narrato::TRIES`, after `Retry-After` or the doubling wait. Expose the wait from `look_answer` and `fit_answer` as `answer` exposes `wait`.
  - Do the same in `generate.ts` and in `Generator.swift`.
  - Say "asking again in N s" in the progress line.
- **Why:**
  - A single 429 or 529 throws away a whole look round or the entire fit pass. These are the two polish passes that make a generated deck look finished.
  - Busy errors cluster exactly after the long slides call, which REL-1 (narrative) already retries.
- **Effort:** S · **Impact:** M

### REL-2. Refuse a look or fit rewrite that changes a slide's figures
- **Where:**
  - `crates/scaena-narrato/src/look.rs:163-200` (`read`);
  - `crates/scaena-narrato/src/fit.rs:55-68` (`read`);
  - `scaena_ops::narrative::figures` at `crates/scaena-ops/src/narrative.rs:4264`.
- **Proof:**
  ```rust
  let words = |key: &str, was: &str| {
      let now = said(key);
      (!now.is_empty() && now != was && now.chars().count() <= longest(was)).then_some(now)
  };
  ```
- **Verified:**
  - `grep -n -i "figure" crates/scaena-narrato/src/look.rs crates/scaena-narrato/src/fit.rs` →
    ```
    crates/scaena-narrato/src/look.rs:114:         Keep every figure, name, date, and claim a slide makes, and add none. ...
    crates/scaena-narrato/src/look.rs:256:        assert!(p.contains("Keep every figure") && ...);
    crates/scaena-narrato/src/fit.rs:24:        "... Keep its meaning, its figures, and its names; ... Drop words, never facts.",
    ```
    The figures are asked for in the prompt and never checked in code.
  - `grep -n "^fn figures\|^pub fn looked" crates/scaena-ops/src/narrative.rs` →
    ```
    847:pub fn looked(
    4264:fn figures(text: &str) -> Vec<f64> {
    ```
- **Do:**
  - Move `figures` into `scaena_narrato` (new: `pub fn figures`), and have `scaena-ops` use it from there.
  - In `look::read`'s `words` and bullets filter, and in `fit::read`, keep a rewrite only where the multiset of figures is the one it replaces: none dropped, none added, none changed.
  - Add a test with a rewrite that turns "41%" into "about 40%".
- **Why:**
  - The evidence under each claim is the deck's credibility. A model asked to "tighten" can quietly round 41.2% to 40%, or drop the year.
  - The prompt forbids it, but nothing holds the model to it, and the person never sees the original.
- **Effort:** S · **Impact:** M

### REL-3. An outline that comes back unreadable ends the paid run in the browser
- **Where:** `crates/scaena-narrato/src/lib.rs:549-562` (`Step::Outline` in `answer`) and `web/src/generate.ts:198-230`.
- **Proof:**
  ```rust
  // An outline that fails to come back is an error that says why, and the step stands
  // to be asked again, or passed over for the slides at once.
  Step::Outline => {
      ...
      self.outline = Some(outline::read(&read.text, arc, &plan).map_err(NarratoError::Answer)?);
  ```
  ```ts
  const reply = await send(JSON.parse(asked));
  const at = JSON.parse(generation.answer(reply.status, reply.statusText, reply.body, reply.after)) as {
  ```
- **Verified:**
  - `grep -n "map_err(NarratoError::Answer)?\|pub fn outline_first" crates/scaena-narrato/src/lib.rs` →
    ```
    278:    pub fn outline_first(&mut self) -> Result<(), NarratoError> {
    299:        self.outline = Some(outline::settled(outline, self.target()).map_err(NarratoError::Answer)?);
    560:                self.outline = Some(outline::read(&read.text, arc, &plan).map_err(NarratoError::Answer)?);
    586:        let written = answer::slides(text).map_err(NarratoError::Answer)?;
    ```
  - `grep -n "step === \"outline\"\|catch" web/src/generate.ts | head -8` → `60:`, `71:`, `118:`, `119:`, `125:`, `126:`, `135:`, and `141:`. All are fetch and cache catches. None is around `generation.answer` in the step loop.
- **Do:**
  - When `answer` refuses an outline (`NarratoError::Answer`), ask once more.
  - Then offer the person "Write without an outline" or "Stop", in the browser's dialog, the Mac's and the iPad's `PromptView`, and the CLI (`--from-outline` aside). This is the pass-over the doc comment promises.
  - Add a pass-over method on the generation (new: `skip_outline`), which sets `Step::Slides` with no outline.
- **Why:**
  - A model that wraps its JSON in prose, or answers with no claims, throws away the paid arc call and the run.
  - The comment says the step "stands to be asked again, or passed over", but no client does either.
- **Effort:** S · **Impact:** M

### REL-4. Ask the browser to keep the decks it holds
- **Where:** `web/src/folders.ts` and `web/src/recovery.ts`: OPFS decks and recovery copies.
- **Proof:** in `web/src/folders.ts:11-15`:
  ```ts
  /** The names of the bundles the browser keeps, sorted. */
  ```
  This is backed by OPFS, which the browser may clear under storage pressure unless the origin's storage is persistent.
- **Verified:** `grep -rn "storage.persist\|persisted()" web/src` → `# (no matches)`
- **Do:**
  - On the first keep in this browser (OPFS save, Keep a history, or a recovery copy written), call `navigator.storage.persist()`.
  - If it is refused, say once in the status line that the browser may clear decks kept here and that Download keeps a copy.
  - Show `navigator.storage.persisted()` in Kept in this browser….
- **Why:** "Kept in this browser" and the recovery copy that REL-1 (10-24) relies on are both best-effort storage. Safari and Chrome evict best-effort origins under pressure, and the person loses decks they believed were saved.
- **Effort:** S · **Impact:** M

### REL-5. Never lose a kept API key to a failed read or write, in the browser or the Keychain
- **Where:** `web/src/assistant/keys.ts:17-34` (`vault`) and `:40-55` (`load`); `apps/mac/ScaenaKit/Sources/ScaenaKit/Keychain.swift:71-94` (`write`).
- **Proof:**
  ```ts
  } catch {
    // Kept by another vault, which this browser no longer has: forget it.
    localStorage.removeItem(item(provider));
    return undefined;
  }
  ```
  ```swift
  for query in [item(account), legacy(account)] {
      let gone = SecItemDelete(query as CFDictionary)
  ...
  var status = SecItemAdd(added as CFDictionary, nil)
  ```
- **Verified:**
  - `grep -n "removeItem(item(provider))\|generateKey\|let vaulted" web/src/assistant/keys.ts` →
    ```
    28:        const key = await crypto.subtle.generateKey({ name: "AES-GCM", length: 256 }, false, ["encrypt", "decrypt"]);
    52:    localStorage.removeItem(item(provider));
    68:    sessionStorage.removeItem(item(provider));
    69:    localStorage.removeItem(item(provider));
    ```
    There is no cached vault promise. Two first callers each `generateKey`, and the last `put` wins.
  - `grep -n "SecItemDelete\|SecItemAdd\|SecItemUpdate" apps/mac/ScaenaKit/Sources/ScaenaKit/Keychain.swift` →
    ```
    36:                SecItemUpdate(item(account) as CFDictionary, narrowed as CFDictionary)
    44:        if SecItemAdd(moved as CFDictionary, nil) == errSecSuccess {
    45:            SecItemDelete(legacy(account) as CFDictionary)
    73:            let gone = SecItemDelete(query as CFDictionary)
    84:        var status = SecItemAdd(added as CFDictionary, nil)
    92:            status = SecItemAdd(kept as CFDictionary, nil)
    ```
    `write` deletes before it adds, and never uses `SecItemUpdate`.
- **Do:**
  - Browser:
    - Cache `vault()`'s promise in a module variable (new), so concurrent first calls share one key.
    - In `load`, forget the stored key only when `decrypt` throws `OperationError`, which means another vault. On an IndexedDB open or transaction error, return `undefined` and keep the key.
  - Mac and iPad:
    - In `write`, `SecItemUpdate` the existing item and fall back to `SecItemAdd` on `errSecItemNotFound`.
    - Delete the legacy item only after that succeeds.
- **Why:**
  - Every AI feature runs on the person's own key (ADR-0006).
  - A private-mode or blocked-upgrade IndexedDB error, a first-run race between the assistant and From a prompt, or a locked keychain silently deletes it. The next question then fails as "no key".
- **Effort:** S · **Impact:** M

---

## Security

### SEC-1. A bundle's own skills are told to override the built-in one
- **Where:** `crates/scaena-chat/src/prompt.rs:255-258` and `web/src/assistant/prompt.ts:68-70`. Names come from `web/src/assistant/index.ts:85`.
- **Proof:**
  ```ts
  const skills = bundleSkills.length
    ? `\nThe bundle carries its own skills too; follow them over the general ones where they differ:\n${bundleSkills.map((s) => `- bundle://skills/${s}`).join("\n")}\n`
    : "";
  ```
- **Verified:**
  - `grep -n "follow them over" crates/scaena-chat/src/prompt.rs web/src/assistant/prompt.ts` →
    ```
    crates/scaena-chat/src/prompt.rs:257:            "\nThe bundle carries its own skills too; follow them over the general ones where they differ:\n{}\n",
    web/src/assistant/prompt.ts:69:    ? `\nThe bundle carries its own skills too; follow them over the general ones where they differ:\n...`
    ```
  - `grep -n "skills\\/(\[^/\]+)" web/src/assistant/index.ts` →
    ```
    85:  const bundleSkills = session.files().flatMap((path) => /^skills\/([^/]+)\/SKILL\.md$/.exec(path)?.[1] ?? []);
    234:  const skill = /^bundle:\/\/skills\/([^/]+)$/.exec(uri)?.[1];
    ```
    `[^/]+` admits newlines and any text into the system prompt.
- **Do:**
  - List a bundle skill only where its name matches `^[a-z0-9-]{1,48}$`.
  - Change the sentence to say the bundle's skills are the deck author's notes, read as data, never over the built-in skill or the user.
  - Fence a bundle `SKILL.md` read through `resource_read` as #604 fences `deck_read`.
  - Optionally, ask the person once per bundle before the assistant reads its skills.
- **Why:**
  - A shared `.scaena` is untrusted input. Today it can ship `skills/x/SKILL.md` that the system prompt itself tells the model to obey over Scaena's own guidance, and its file name is pasted into the system prompt verbatim.
  - #604's fence covers the deck's words, not this, and the instruction to follow them defeats any fence.
- **Effort:** S · **Impact:** M

### SEC-2. `scaena serve` serves a bundle's HTML and SVG as live pages on its own origin
- **Where:** `crates/scaena-serve/src/pages.rs:30`, `:39` (`media_type`), used for `/bundle/*` by `read` (`crates/scaena-serve/src/lib.rs:189`, `:236-250`), with headers from `answer` (`:402-409`).
- **Proof:**
  ```rust
  headers.insert(header::CONTENT_TYPE, HeaderValue::from_str(media).expect("a media type"));
  headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
  headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
  response
  ```
- **Verified:**
  - `grep -n "Some(\"html\")\|Some(\"svg\")" crates/scaena-serve/src/pages.rs` →
    ```
    30:        Some("html") => "text/html; charset=utf-8",
    39:        Some("svg") => "image/svg+xml",
    ```
  - `grep -n "sandbox\|Content-Disposition\|content-security" crates/scaena-serve/src/lib.rs crates/scaena-serve/src/pages.rs` → `# (no matches)`
- **Do:**
  - For every `/bundle/` response, add `Content-Security-Policy: sandbox; default-src 'none'` and `Content-Disposition: attachment` for `html`, `htm`, `svg`, `js`, and `mjs`.
  - Alternatively, serve those as `application/octet-stream`. Images the pages draw are read as bytes by the worker, so nothing in the editor needs them live.
  - Add a test that `GET /bundle/x.html` carries the sandbox.
- **Why:**
  - Script in a bundle file opened at `http://127.0.0.1:PORT/bundle/x.svg` runs on the editor's origin. Its `PUT` and `DELETE` then carry that origin and pass `ours(ORIGIN)`, so it can rewrite any file in the bundle.
  - A deck someone else sent is exactly what a person serves to look at.
- **Effort:** S · **Impact:** M

### SEC-3. A web source's name is checked, but not where it resolves
- **Where:** `crates/scaena-sources/src/fetch.rs:136-158` (`public`), used by `crates/scaena-cli/src/sources.rs:46-66` (`fetched`), and by the Mac's `Generator.fetch`.
- **Proof:**
  ```rust
  if let Ok(ip) = host.parse::<std::net::IpAddr>() {
      return if reached(ip) { Ok(()) } else { Err(refused()) };
  }
  ...
  if host.is_empty() || !host.contains('.') || host.ends_with(".localhost") || numeric {
      return Err(refused());
  }
  Ok(())
  ```
- **Verified:**
  - `grep -n "to_socket_addrs\|lookup_host\|Resolver" crates/scaena-cli/src/sources.rs crates/scaena-sources/src/fetch.rs` → `# (no matches)`
  - `grep -n "pub fn public\|pub fn redirected\|fn reached" crates/scaena-sources/src/fetch.rs` →
    ```
    136:pub fn public(url: &str) -> Result<(), SourceError> {
    160:fn reached(ip: std::net::IpAddr) -> bool {
    189:pub fn redirected(from: &str, location: &str) -> Result<String, SourceError> {
    ```
- **Do:**
  - In the CLI's and MCP's transport, resolve the host before each GET and each redirect (`std::net::ToSocketAddrs`). Refuse when any address fails `reached`, making that function `pub`.
  - Then connect to the checked address, keeping the Host header, so a rebinding DNS answer cannot change it between the check and the GET.
  - On the Mac and iPad, resolve with `getaddrinfo` before the `URLSession` request and refuse the same addresses.
  - The browser is bounded by CORS and needs nothing.
- **Why:**
  - `--source https://anything.nip.io/` that resolves to `127.0.0.1`, `10.x`, or `169.254.169.254` is fetched today. Its body then goes into a deck and to a model.
  - A brief or a pasted link from someone else is how such a URL arrives.
- **Effort:** M · **Impact:** M

### SEC-4. Fence every tool result that carries the deck's words, not only `deck_read` and `deck_find` (gap in #604)
- **Where:**
  - `crates/scaena-chat/src/providers.rs:315-316` (`anthropic_result`) and its OpenAI and Gemini twins;
  - the assistant's tools in `crates/scaena-session/src/assistant.rs:32-50`.
- **Proof:**
  ```rust
  fn anthropic_result(r: &ToolResult) -> Value {
      let mut content = vec![json!({ "type": "text", "text": r.json })];
  ```
- **Verified:**
  - `grep -rn "scaena_fence\|scaena-fence" crates/scaena-chat crates/scaena-session web/src/assistant` → `# (no matches)` on `a3a3db4`.
  - #604's description: "what `deck_read` and `deck_find` return" is fenced. It says nothing of `deck_inspect`, `spine_read`, `deck_health`, `deck_lint`, or `resource_read` of `bundle://`. Each of those returns the deck's texts, notes, claims, or alt.
- **Do:**
  - Once #604 lands, fence the results of `deck_inspect`, `spine_read`, `deck_health`, `deck_lint`, `data_edit` (rows), and `resource_read` of any `bundle://` URI the same way, in both twins.
  - Test with a node whose text is an instruction and that is read through `deck_inspect`.
- **Why:** a deck from someone else, or one generated from a hostile web source, carries its words to the model through whichever read tool the model happens to call. Fencing two of seven leaves the door open.
- **Effort:** S · **Impact:** M

### SEC-5. `scaena serve` takes a write with no Origin from anyone on the machine
- **Where:** `crates/scaena-serve/src/lib.rs:228-235` (`ours`) and `:190-194`.
- **Proof:**
  ```rust
  fn ours(&self, headers: &HeaderMap, name: header::HeaderName, scheme: &str) -> bool {
      match headers.get(&name) {
          Some(value) => names_us(value.as_bytes(), scheme, self.port),
          None => name == header::ORIGIN,
      }
  }
  ```
- **Verified:**
  - `grep -n "None => name == header::ORIGIN" crates/scaena-serve/src/lib.rs` → `233:            None => name == header::ORIGIN,`
  - `grep -n -i "token\|secret" crates/scaena-serve/src/lib.rs` → `# (no matches)`
- **Do:**
  - Make a random token per launch (new), and hand it to the pages in the URL `scaena serve` prints (`?serve=TOKEN`).
  - Require it in an `x-scaena-token` header on every `PUT` and `DELETE`, refusing with 403.
  - `web/src/served.ts` sends it.
- **Why:**
  - 127.0.0.1 is shared by every user and process on the machine. On a shared host or a dev box, another account can rewrite or delete the files in the bundle being served, as the serving user.
  - A token the pages carry closes that at no cost to the person.
- **Effort:** S · **Impact:** S

---

## Usability

### USE-1. Give the presenter view a slide navigator, as Keynote's has
- **Where:** `web/src/player.ts:842-853` (the presenter's keys), with the Mac's and the iPad's `PresenterView` in `apps/mac/ScaenaKit/Sources/Scaena/Presenting.swift`.
- **Proof:**
  ```ts
  // A number the deck has no slide for is said here, where the presenter typed it.
  slide: (number) => (numbered(slots, String(number)) ? steer({ type: "slide", number }) : typing.say(noSlide(slots, number))),
  ```
- **Verified:** `grep -n -i "navigator\|thumbnails" web/src/player.ts | head -5` → `# (no matches)`. A slide is reached by typing its number, or by stepping.
- **Do:**
  - Add a navigator to the presenter view (new): a strip of the deck's slides at rest, opened by a key (`N`) and a button. Arrows move through it, and Return or a click goes to that slide through the existing `steer({ type: "slide", number })`.
  - Draw it with the `upNext` stage's thumbnails.
  - Do the same in `PresenterView` on the Mac and the iPad, with `Show` going there.
- **Why:**
  - In a Q&A someone asks "can you go back to the chart?". Keynote's presenter opens its navigator, and Scaena's makes the presenter remember that the chart was slide 14.
  - It is the presenting feature most missed against Keynote.
- **Effort:** M · **Impact:** M

### USE-2. Say when the presenter view is blocked, and start its timer when the show starts
- **Where:** `web/src/player.ts:535-536` (opening the presenter window) and `:936` (the timer).
- **Proof:**
  ```ts
  view = open("", "scaena-presenter", "popup,width=1280,height=800");
  if (!view) return;
  ```
  ```ts
  let since: number | undefined = performance.now();
  ```
- **Verified:** `grep -n "if (!view) return;\|let since: number | undefined = performance.now()" web/src/player.ts` (on `a3a3db4`) →
  ```
  536:    if (!view) return;
  936:  let since: number | undefined = performance.now();
  ```
- **Do:**
  - When `open` returns null, say "The presenter view was blocked: allow pop-ups for this page, then press the button again" in the player's status line.
  - Start `since` as `undefined`, and set it on the first `at` the presenter hears from the player (the show's first slide). Reset is unchanged.
- **Why:**
  - A blocked pop-up today does nothing at all, minutes before a talk.
  - A timer that counts from when the view opened says the talk ran ten minutes longer than it did. That is the presenter's one number.
- **Effort:** S · **Impact:** M

### USE-3. Say why a bundle could not be read or written: the io cause is dropped
- **Where:** `crates/scaena-store/src/lib.rs:41-45` and `crates/scaena-ops/src/lib.rs:108-118`.
- **Proof:**
  ```rust
  pub enum StoreError {
      #[error("io: {0}")]
      Io(#[from] std::io::Error),
      #[error("reading {}", path.display())]
      Read { path: PathBuf, source: std::io::Error },
  ```
  ```rust
  impl From<$error> for OpsError {
      fn from(e: $error) -> OpsError {
          OpsError::new(e.to_string())
  ```
- **Verified:**
  - `grep -n "#\[error(\"io: {0}\")\]\|#\[error(\"reading {}\", path.display())\]" crates/scaena-store/src/lib.rs` →
    ```
    42:    #[error("io: {0}")]
    44:    #[error("reading {}", path.display())]
    ```
  - `grep -n "OpsError::new(e.to_string())" crates/scaena-ops/src/lib.rs` → `112:                OpsError::new(e.to_string())`
- **Do:**
  - Change `Read`'s message to `"reading {}: {source}"`.
  - Replace the blanket `Io(#[from])` at its call sites (`walk`, `write_all`, `create_dir_all`, `save.rs`'s `read_dir`) with `Read` or a `Write { path, source }` variant (new) carrying the path.
  - In the `from!` macro, join `e.source()`'s chain into the message.
- **Why:** an agent, the CLI, or the Mac reports "reading /x/deck.json", with no "permission denied" and no "no such file". An agent cannot recover from that, and a person cannot tell a sandbox refusal from a typo.
- **Effort:** S · **Impact:** M

### USE-4. Read the slide list's rows in VoiceOver by their headlines, selection, and actions
- **Where:** `apps/mac/ScaenaKit/Sources/Scaena/StateList.swift:206-209`. The same applies at `LightTable.swift:247-250`.
- **Proof:**
  ```swift
  .accessibilityElement(children: .ignore)
  .accessibilityLabel(step > 1 ? "Slide \(number), step \(step)" : "Slide \(number)")
  .accessibilityIdentifier("state-\(slot.state)")
  ```
- **Verified:**
  - `grep -n "accessibilityLabel\|accessibilityAddTraits\|accessibilityAction" apps/mac/ScaenaKit/Sources/Scaena/StateList.swift` → `208: .accessibilityLabel(step > 1 ? "Slide \(number), step \(step)" : "Slide \(number)")`
  - `grep -rn "accessibilityRotor" apps/mac/ScaenaKit/Sources` → `# (no matches)`
  - `grep -n "public func reading(state" apps/mac/ScaenaKit/Sources/ScaenaKit/Editing.swift` → `598: public func reading(state: String) throws -> String { ... }`
- **Do:**
  - Label each row "Slide 3: <its headline>", taking the headline from the session's reading of the state (`reading(state:)`, or a `headline` on `Slot` (new)).
  - Add `.accessibilityAddTraits(picked ? .isSelected : [])`, as `LightTable.swift:250` already does.
  - Add `accessibilityAction`s for Move Up, Move Down, Duplicate, and Delete, each the list's own command.
  - Add a "Slides" `accessibilityRotor` on `DeckView` that goes to a state.
- **Why:** a VoiceOver user hears "Slide 1, Slide 2, Slide 3…" and must open each to know what it is. Keynote's navigator reads each slide's title. The engine already says how each state reads (`scaena_core::reading`).
- **Effort:** S · **Impact:** M

### USE-5. When an agent names a node or state that isn't there, say which ones are
- **Where:** `crates/scaena-core/src/patch/ops.rs:455-457` (`node`), `:469-471` (`state`), and `:2140`.
- **Proof:**
  ```rust
  fn node(&self, id: &str) -> Result<&'a Map<String, Value>, String> {
      self.0.get("nodes").and_then(|n| n.get(id)).and_then(Value::as_object).ok_or_else(|| format!("no node `{id}`"))
  }
  ```
- **Verified:**
  - `grep -rn -i "did you mean" crates/scaena-core/src/patch` → `# (no matches)`
  - `grep -n "fn distance\|no node \`{id}\`\|no state \`{id}\`" crates/scaena-core/src/model/check.rs crates/scaena-core/src/patch/ops.rs` →
    ```
    crates/scaena-core/src/model/check.rs:771:fn distance(a: &str, b: &str) -> usize {
    crates/scaena-core/src/patch/ops.rs:456:        ... .ok_or_else(|| format!("no node `{id}`"))
    crates/scaena-core/src/patch/ops.rs:470:        ... .ok_or_else(|| format!("no state `{id}`"))
    crates/scaena-core/src/patch/ops.rs:2140:    let find = |id: &str| ids.iter().position(|s| *s == id).ok_or_else(|| format!("no state `{id}`"));
    ```
- **Do:**
  - Make `check::distance` `pub(crate)`.
  - In `node` and `state` (and `:2140`), append "did you mean `x`?" for the nearest id within distance 3. Otherwise append the first few ids of the state shown, or the deck's states.
  - Add a test.
- **Why:**
  - Agents misspell or guess ids (`title` for `cover-title`) more than any other error. Each miss costs a full round of `deck_inspect`.
  - The property checker already does this for keys (`check.rs:526`). Ids, the more common miss, don't get it.
- **Effort:** S · **Impact:** M

---

## Attractiveness / Sexiness

### ATTR-1. Name an outlined deck's sections by what they claim, not by the arc's template
- **Where:** `crates/scaena-ops/src/narrative.rs:1659-1662` (`names` from the outline) and `:1739-1749` (the section's title).
- **Proof:**
  ```rust
  let outlined = narrative.outline.is_some();
  let named = (slide.beat_name.as_deref().map(str::trim))
      .filter(|n| !n.is_empty() && !outlined && !names.iter().any(|arc| arc.eq_ignore_ascii_case(n)));
  let headline = Some(slide.slide_title.as_str()).filter(|t| !t.trim().is_empty() && !outlined);
  let name = match (divider, named, headline) {
      ...
      (false, None, None) => names.get(beat).copied().unwrap_or("Section"),
  ```
- **Verified:** `grep -n "let outlined\|names.get(beat).copied()\|Some(outline) => (outline.beats" crates/scaena-ops/src/narrative.rs` →
  ```
  1660:        Some(outline) => (outline.beats.iter()).map(|b| b.name.split(':').next().unwrap_or(&b.name).trim()).collect(),
  1740:            let outlined = narrative.outline.is_some();
  1748:                (false, None, None) => names.get(beat).copied().unwrap_or("Section"),
  ```
  The outline's beat names default to the arc's ("Problem", "Solution") unless the person renamed them.
- **Do:**
  - Where an outline was settled, title a section without a divider by its beat's name only when the person changed it from the arc's own.
  - Otherwise title it by the beat's `claim`, shortened to its first clause or at most eight words (new helper).
  - Keep the divider-wins rule.
  - Update `a_section_named_as_the_arc_names_its_beat_takes_its_first_headline` and add an outlined twin.
- **Why:**
  - Narrative ATTR-5 removed template names for decks with no outline. The outline path, now on by default, brought them back.
  - The PDF's outline, the player's picker, and the Story tab read "Problem / Solution / Proof" for a deck whose person wrote "Freight can pay for its carbon". Gamma and Pitch name sections by content.
- **Effort:** S · **Impact:** M

### ATTR-2. Show the look pass the story, so "tighten" keeps each slide's takeaway
- **Where:** `crates/scaena-narrato/src/look.rs:66-90` (`caption`) and `:94-120` (`prompt`), called at `crates/scaena-narrato/src/lib.rs:457`.
- **Proof:**
  ```rust
  pub fn prompt(topic: &str, audience: &str, count: usize, language: Option<&str>, shown: (usize, usize)) -> String {
  ...
         - A title that runs past two lines: tighten it to one idea.\n\
  ```
- **Verified:**
  - `grep -n "pub fn prompt(topic\|pub fn caption" crates/scaena-narrato/src/look.rs` →
    ```
    66:pub fn caption(seen: &Seen, count: usize) -> String {
    94:pub fn prompt(topic: &str, audience: &str, count: usize, language: Option<&str>, shown: (usize, usize)) -> String {
    ```
  - `grep -n -i "claim\|big idea\|outline" crates/scaena-narrato/src/look.rs | head -5` → only `114:` ("Keep every figure, name, date, and claim a slide makes…"). No slide's claim and no big idea is given.
- **Do:**
  - Add the beat's claim (from the outline where settled, else the slide's `purpose`) to each slide's `caption` as "It must leave the audience with: …".
  - Add the big idea to `prompt`.
  - Add one rule: "A title states the slide's takeaway as a sentence, never a topic; where one names a topic, rewrite it to say the claim within the same length."
  - The look is Scaena's own call, so ADR-0026 is untouched.
- **Why:**
  - Today the art director sees pictures and words but not what each slide is for. "Tighten to one idea" can cut the one idea the slide exists to say.
  - Action titles that state the takeaway are what separate a coach-grade deck from a Beautiful.ai one, and this pass is the cheapest place to enforce them.
- **Effort:** S · **Impact:** L

### ATTR-3. Give the Mac's and the iPad's floating chrome Liquid Glass on macOS and iPadOS 26
- **Where:**
  - `apps/mac/ScaenaKit/Sources/Scaena/CanvasSelection.swift:2123` (the hint);
  - `CodeChip.swift:32`;
  - `Presenting.swift:941` (the keys list, on `a3a3db4`).
- **Proof:**
  ```swift
  .padding(.vertical, down)
  .background(.regularMaterial, in: Capsule())
  .shadow(color: .black.opacity(0.15), radius: 6, y: 2)
  ```
- **Verified:**
  - `grep -rn "glassEffect\|GlassEffectContainer" apps` → `# (no matches)`
  - `grep -rn "regularMaterial" apps/mac/ScaenaKit/Sources/Scaena` →
    ```
    apps/mac/ScaenaKit/Sources/Scaena/CanvasSelection.swift:2123:                .background(.regularMaterial, in: Capsule())
    apps/mac/ScaenaKit/Sources/Scaena/Presenting.swift:941:        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
    apps/mac/ScaenaKit/Sources/Scaena/CodeChip.swift:32:                .background(.regularMaterial, in: Capsule())
    ```
- **Do:**
  - Under `if #available(macOS 26, iOS 26, *)`, use `.glassEffect(.regular, in: Capsule())` (and the rounded rectangle) in place of `.regularMaterial` plus the hand-made shadow.
  - Wrap neighbouring floating controls (the cue bar's Play, the typed-words chip, the presenter's timer bar) in a `GlassEffectContainer`.
  - The Mac's floor stays macOS 15 (`Package.swift`). That is Jay's call, not this item's.
- **Why:** on macOS and iPadOS 26, every Apple app's floating controls are glass. Scaena's are the previous generation's material, the one place the app reads as dated next to Keynote 26.
- **Effort:** S · **Impact:** M

### ATTR-4. Carry the deck's author, description, and dates into the PDF, and its title into the video
- **Where:** `crates/scaena-export/src/pdf.rs:295-302` and `crates/scaena-export/src/video.rs:608-611`. `Meta` is at `crates/scaena-core/src/document.rs:98-117`.
- **Proof:**
  ```rust
  let mut metadata = Metadata::new().creator("Scaena".to_string());
  if let Some(title) = meta.and_then(|m| m.title.clone()) {
      metadata = metadata.title(title);
  }
  ```
- **Verified:**
  - `grep -n "Metadata::new()\|metadata.title\|authors\|subject\|creation_date" crates/scaena-export/src/pdf.rs` →
    ```
    295:    let mut metadata = Metadata::new().creator("Scaena".to_string());
    297:        metadata = metadata.title(title);
    ```
  - `grep -n "map_metadata\|-metadata" crates/scaena-export/src/video.rs` →
    ```
    609:    // No metadata from the inputs. A bare `-map_metadata -1` would strip the chapters'
    611:    args.extend(["-map_metadata:g", "-1", "-map_metadata:s", "-1", "-fflags", "+bitexact", "-y"].map(OsString::from));
    ```
- **Do:**
  - Map `meta.author` to krilla's `authors`, `meta.description` to `subject`, and `meta.created` and `meta.modified` to the creation and modification dates.
  - These come from the deck, never the clock, so the bytes stay deterministic and the browser's PDF stays the CLI's.
  - In `video.rs`, after the strip, add `-metadata title=…` and `-metadata comment=…` from the deck.
- **Why:**
  - A PDF sent to a client shows "Untitled author" in Preview's inspector and in a DMS. A video shows no title in QuickTime or on upload.
  - A generated deck's `description` is its executive summary, which is exactly what a PDF's subject is for.
- **Effort:** S · **Impact:** S

### ATTR-5. Reopen a deck's window on the slide and zoom it was closed on
- **Where:** `apps/mac/ScaenaKit/Sources/Scaena/DeckView.swift:26` and `:120`.
- **Proof:**
  ```swift
  /// The state chosen in the list.
  @State private var chosen: String?
  ```
  ```swift
  /// How close the canvas is shown, and the part of it shown (PLAN 3.16).
  @State private var zoom = Zoom(canvas: CGSize(width: 1920, height: 1080))
  ```
- **Verified:** `grep -n "@SceneStorage\|@State private var chosen\|@State private var zoom" apps/mac/ScaenaKit/Sources/Scaena/DeckView.swift` →
  ```
  26:    @State private var chosen: String?
  92:    @SceneStorage("assistant") private var showsAssistant = false
  ...
  118:    @SceneStorage("formats") private var showsFormats = false
  120:    @State private var zoom = Zoom(canvas: CGSize(width: 1920, height: 1080))
  ```
  Panels are restored with the window. The slide and the zoom are not.
- **Do:**
  - Keep the chosen state's id in `@SceneStorage("slide")` (new), and restore it when the deck opens if the deck still has it.
  - Keep the zoom's scale and centre in `@SceneStorage("zoom")` (new, as a string), applied after the canvas is sized.
- **Why:** Keynote reopens where you were. Scaena restores which panels were open but lands on slide 1 at fit. On a 40-slide deck that is a scroll and a pinch every time the person comes back to work.
- **Effort:** S · **Impact:** S

---

## First move

**STAB-1. The Story tab and the assistant never see the slides lint finds broken.**

The product's narrative promise rests on deck health. #532 made its first and strongest finding "this slide doesn't stand as it is to be read", but that finding never reaches the editors' Story tab or the assistant's `deck_health`. People and models there coach a deck whose overflowing slides score as healthy.

The fix:
- passes the lint findings the editors already compute into `health_in` at four call sites;
- touches no format and no prompt;
- ships in a day with one test;
- makes every later health item truer, FUNC-1 and FUNC-2 first.

FUNC-1 (read the outline's big idea) and FUNC-2 (narrative warnings and density as findings) should follow in the same lane. Together they turn health from a score into a coach.

---

## Dropped during verification

- **The writing tools know neither audience nor voice:** a repeat. It is part of 10-24's FUNC-3 ("give it the slide's claim and the deck's voice"), which has not landed.
- **A fit rewrite leaves the beat's claim behind:** a repeat of 10-24's FUNC-2 (keep a beat's claim in step with its headline).
- **Export runs on the Mac's main actor, export temp files are never removed, and New ends the app on `fatalError`:** repeats of 10-24's PERF-3, REL-5, and STAB-3, none landed.
- **The theme chooser appears after the window:** a repeat of 10-24's ATTR-5. The ATTR-5 that landed in #587 was 10-19's toolbar item.
- **A video output named `-x.mp4` is read by ffmpeg as an option:** the cited code does something less than claimed. The path is the user's own `--out`, and over MCP it is held under the working folder. At worst it is a confusing error, with no security effect.
- **Serve's 150 ms rescan holds the lock:** real, but the scan is of one bundle folder, so the cost on a real bundle is small. Deferred.
- **No upper bound on the slide count:** Narrato's prompt says "AT LEAST", and it is ported byte for byte. Health's pacing (FUNC-2) is where an overlong deck should be said. 10-24 deferred the same point to Jay.
- **The outline's per-beat order is not checked against the slides:** subsumed by FUNC-1's `Kind::Claim`, which reads each beat's slides against its claim.
- **The editors' `localStorage` is unguarded:** already in place (`editor.ts:908-934`, `slides.ts:82-87`, `keys.ts:80-93`).
- **A story finding doesn't show its slide, and Escape doesn't stop a generation:** already in place (`story.ts:143`, `:162`; `prompt.ts:415-416`).
- **FFI strings or bytes freed twice or leaked:** none found. Every string passes through `scaena_string_free` once, and `ScaenaBytes` of an empty `Vec` is handled.

## Deferred

- **Rehearse from the slide shown, and pause, in the browser and on the Mac** (`web/src/rehearse.ts:153`; `States.swift:297`): next after USE-1 on presenting.
- **Play is hidden for a deck opened from a folder** (`web/src/editor.ts:672-673`).
- **A recent-decks list, and reopening the last deck on a bare visit** (`web/src/folders.ts:11-15`, `editor.ts:446-449`).
- **The Mac reads a prompt's source files on the main actor** (`PromptWindow.swift:711`; `Generator.add` is synchronous, and `stepAway` takes no bytes).
- **Thumbnails held as `ImageData` without limit** (`web/src/strip.ts:80`, `:165`).
- **The crash mirror copies the whole bundle on each file change** (`web/src/worker.ts:129-142`).
- **History grows without compaction, and a damaged history blocks every save** (`scaena-store/src/crdt.rs:217-218`, `lib.rs:244`).
- **Deep research never hears the desired outcomes or the language** (`scaena-narrato/src/research.rs:33-37`): the prompt is Vzestup's byte for byte, so this needs a Scaena block after it.
- **`deck_health` cannot be narrowed to a slide, and `deck_inspect`'s description is a third of the tool payload** (`docs/schema/mcp/deck_inspect.json`).
- **The browser's and the Mac's assistant lack a read-only `deck_history`** (`web/src/assistant/prompt.ts`, "There is no … deck_history here").
- **The iPad presenting alone has no notes, next, or timer** (`Presenting.swift:290-294`), and there is no Handoff (`userActivity`, absent).
- **The span cache is cleared whole at 4,096 entries** (`scaena-engine/src/render.rs:274-278`).
- **The app and Remote targets build in Swift 5 language mode** (`Package.swift`, `apps/ipad/project.yml`).
- **Fetched script fonts live in Application Support, not Caches** (`Generator.swift:344-347`).
- **There are no fuzz targets for the DSL, the deck JSON, the PDF's `Prepared`, or the history** (no `fuzz/`).

Run `/burn-down` to execute this plan.
