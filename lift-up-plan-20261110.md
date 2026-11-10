# Lift-Up Plan: Scaena

> Platform: mixed — a Rust core (`crates/`), the browser player and editor (`web/`, Vite + TS + WASM), and the Mac and iPad apps (`apps/mac`, `apps/ipad`, SwiftUI over a C ABI)
> Surveyed: 2026-11-10
> Baseline: `main`@`fce3e91` — fast-forwarded from `2e08ab1` to `origin/main`; clean tree; drift: main moved 3 commits during the audit (#798, #809, #831), touching `health.rs`, `narrative.rs`, `prompt.ts`, `protocol.ts`, and `worker.ts`. The items citing them were re-verified at `fce3e91`, and their line numbers are the new ones.
> Scope: full repo, with Functionality and Usability weighted toward what a person making a talk feels: the outline, the deck made, health, the assistant, rehearsing, and presenting (the coordinator's brief).
> Coverage: full. Eight read-only survey passes covered generation in Rust, the browser's From a prompt…, health and the Story, the assistant, rehearsing and presenting, the browser editor's core, the Mac and iPad, and untrusted input. Every item below was re-read and grepped in this session.
> Attractiveness anchor: Keynote's presenter display and Pitch's or Gamma's generator, inferred from the product's category.
> Excluded: everything `lift-up-plan-20261031.md` has, its Deferred list included, and every open PR's subject (listed under Dropped).

## Performance

### PERF-1. The browser's whole-deck lint runs on the editor's only engine worker, so typing after a pause waits behind it
- **Location:** `web/src/worker.ts:357-361`; scheduled at `web/src/editor.ts:1916`
- **Proof:**
  ```ts
      case "lint": {
        const start = performance.now();
        const now = Math.floor(Date.now() / 1000);
        const linted = JSON.parse(player.lint(undefined, now)) as { findings: Finding[]; laid: boolean; whole: boolean };
        return post({ type: "linted", id: data.id, ...linted, ms: performance.now() - start });
  ```
- **Verified:** `grep -n "pending = setTimeout(() => void lintAll" web/src/editor.ts` → `1916:      pending = setTimeout(() => void lintAll(read), pause);`; `grep -n "laying\|Laying\|lint_laid\|lintLaid" crates/scaena-wasm/src/lib.rs web/src/worker.ts | grep -iv playing` → `crates/scaena-wasm/src/lib.rs:691:    /// frames at rest that follow, laying nothing out: …` / `web/src/worker.ts:1893: * patch changes, … laying nothing out, …` (no binding); `grep -n "pub fn laying\|pub fn lint_laid" crates/scaena-session/src/*.rs` → `crates/scaena-session/src/editor.rs:443:    pub fn laying(&self) -> Option<Laying> {` / `crates/scaena-session/src/editor.rs:508:    pub fn lint_laid(&mut self, laying: &Laying, now: Option<i64>) -> Result<Option<Linting>, Error> {`; `web/src/stage.ts:220-222` → "a large deck's whole lint or a PDF's preparation, holds it for seconds".
- **Do:** Bind `Session::laying` and `Session::lint_laid` in `crates/scaena-wasm/src/lib.rs` (new bindings `laying`/`lintLaid`). In `worker.ts`'s `lint` case, take the laying and lay it out in slices that yield between states. A newer `edit` message drops the laying in progress, as the Mac's `DeckEditor.lintEvery` does on its own thread. Keep `player.lint` for the single-state lint an edit asks.
- **Why:** On a 40-slide deck, the first key after each half-second pause queues behind seconds of laying out, so every pause in typing makes the next key stutter.
- **Effort:** M · **Impact:** L

### PERF-2. Every chart and table load compares the whole header per cell and clones it per row, with no cache
- **Location:** `crates/scaena-core/src/data.rs:211-213`, `:344`
- **Proof:**
  ```rust
          for (i, (c, v)) in header.iter().zip(values).enumerate() {
              let k = if header == columns { i } else { columns.iter().position(|x| x == c).expect("a column") };
              let kind = ColumnType::of(schema.get(c).map(String::as_str)).map_err(|e| at(format!("`{c}`: {e}")))?;
  ...
              Ok((header.clone(), r.into_iter().map(Value::String).collect()))
  ```
- **Verified:** `grep -rn "data::load(" crates --include=*.rs | grep -v test | wc -l` → `14`; the `ColumnType::of` call for every column is already worked out once above the loop at `:205` (`.map(|c| ColumnType::of(schema.get(c)…`), and the per-cell one at `:213` repeats it.
- **Do:** In `data::load`, work out the column index map and each column's `ColumnType` once, then index into both per cell. Have the CSV reader at `:340-346` hand rows a shared header (an `Rc<[String]>` or one header for the whole table) in place of `header.clone()`. Add a test that loads a 50k×30 CSV within a set time.
- **Why:** About 14 call sites load a source fresh: each chart compile, table, validation, and inspector. A 200k-row, 30-column CSV costs about 180M string compares per load, felt on every edit of a slide that charts it.
- **Effort:** S · **Impact:** M

### PERF-3. The Mac's Versions panel replays history and draws versions on the main actor, and reads the list again after every edit
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/VersionsPanel.swift:41`, `:103-106`, `:115`
- **Proof:**
  ```swift
          .task(id: editor.revision) { read() }
  ...
              states = try editor.session.viewVersion(chosen)
              slides = try editor.session.versionSlides()
              if state == nil || !states.contains(state ?? "") { state = states.first }
              compared = try editor.session.compareVersions(from: chosen)
  ...
              drawing = Image(png: try editor.session.versionPNG(state, width: 640))
  ```
- **Verified:** `grep -n 'OffMain\|Task.detached' apps/mac/ScaenaKit/Sources/Scaena/VersionsPanel.swift` → `# (no matches)`.
- **Do:** Take a snapshot (`editor.session.snapshot()`) and run `viewVersion`, `compareVersions`, and `versionPNG` on `OffMain`, as `Snapshot.exported` and the health read do. Re-read `versions()` only when the history's version count changes, not on every `editor.revision`.
- **Why:** On a deck with a long history, picking a version or stepping through its slides blocks the window. Each keystroke elsewhere also re-reads the list while the panel is open.
- **Effort:** S · **Impact:** M

### PERF-4. The editor keeps every edit's timing and every whole-deck lint for the life of the tab
- **Location:** `web/src/editor.ts:765-767`, pushed at `:1862` and `:1935`
- **Proof:**
  ```ts
    const trips: { ms: number; compile: number; paint: number; lint: number }[] = [];
    /** Each lint of every state, and how long it took in the worker. */
    const wholes: Linted[] = [];
  ```
- **Verified:** `grep -n "trips\b\|trips\.\|wholes" web/src/editor.ts` → `765:  const trips: …` / `767:  const wholes: Linted[] = [];` / `1862:    trips.push({ ms: performance.now() - sent, ...edited.ms });` / `1935:    wholes.push(linted);` / `3352:      trips: () => trips,` / `3353:      wholes: () => wholes,`. Nothing trims either.
- **Do:** Cap both as ring buffers (the last 200 trips, the last 5 wholes), or record them only under the walks' test flag that already reads them through the getters at `:3352-3353`.
- **Why:** An hour editing a 40-slide deck keeps thousands of full `Finding[]` lists alive. The tab's memory only grows, and on a laptop running a talk all afternoon that ends in a slow or reloaded tab.
- **Effort:** S · **Impact:** S

### PERF-5. Make a Picture queries the Keychain several times on every keystroke on the Mac
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/PictureSheet.swift:48-51`, read at `:83`, `:95`, `:102`, `:132`
- **Proof:**
  ```swift
      /// The image providers whose key is kept.
      private var kept: [Provider] {
          _ = keysChanged
          return [.openai, .gemini].filter { keychain.key(for: $0) != nil }
      }
  ```
- **Verified:** `grep -n "kept" apps/mac/ScaenaKit/Sources/Scaena/PictureSheet.swift` → `48:    private var kept: [Provider] {` / `83:                if !kept.contains(provider) {` / `95:                        making != nil || !kept.contains(provider)` / `102:            if !kept.contains(provider), let first = kept.first { provider = first }` / `107:        // Back from Settings, …` / `132:        guard kept.contains(provider) else {`; `grep -n "func key(for" apps/mac/ScaenaKit/Sources/ScaenaKit/Keychain.swift` → `15:    public func key(for provider: Provider) -> String? { read(provider.rawValue) }`.
- **Do:** Make `kept` a `@State` array read once on appear and again when `keysChanged` fires, as PromptView's `readKeys()` keeps its `kept`.
- **Why:** Each keystroke in the subject field re-evaluates `body`, which makes four or more `SecItemCopyMatching` calls on the main thread, a hitch while typing a description.
- **Effort:** S · **Impact:** S

## Functionality

### FUNC-1. A rehearsal's results never compare the time taken with the time the talk planned
- **Location:** `web/src/rehearse.ts:211-218`; `apps/mac/ScaenaKit/Sources/Scaena/Rehearsing.swift`
- **Proof:**
  ```ts
      kept = slots.map((slot, i) => ({
        state: slot.state,
        said: slot.said ?? slot.state,
        spent: spent[i],
        span: slot.span,
        hold: slot.hold,
        keeps: spent[i] > 0 ? Math.max(0, Math.round((spent[i] - slot.span) / 100) * 100) : undefined,
      }));
  ```
- **Verified:** `grep -rn "planned" web/src/rehearse.ts apps/mac/ScaenaKit/Sources/Scaena/Rehearsing.swift` → `# (no matches)`; `grep -n "planned" web/src/protocol.ts | head -3` → `98: * beat's claim, the seconds the beat is planned to take, and the seconds planned before the state` / `103:  planned?: number;`. Every slot carries the plan (`Slot.story`), and the presenter view uses it for "behind/ahead".
- **Do:** Give each result row its planned seconds (`slot.story.planned`, shared across the beat's slots) and a signed difference. Add a total, "18:40 taken against a 15:00 plan", and mark the three slides that ran furthest over, in `rehearse.ts`'s results table and `Rehearsing.swift`'s sheet. Where no slot has a plan, say so and show times alone.
- **Why:** After a run-through, the speaker learns only how long each slide took. The talk planned by the brief and its beats already knows what each should take, and that is the feedback a rehearsal exists to give.
- **Effort:** M · **Impact:** L

### FUNC-2. "Outline again" throws away every claim the person typed, and one beat cannot be asked for again
- **Location:** `web/src/prompt.ts:571-574`, `:231`; `crates/scaena-narrato/src/outline.rs:55`
- **Proof:**
  ```ts
      $<HTMLButtonElement>("#prompting-outline-again").onclick = () => {
        outlinePane.hidden = true;
        making.postMessage({ type: "outlineAgain", arc: outlineArcs.value || undefined } satisfies ToWorker);
      };
  ...
      beats.replaceChildren(...outlined.outline.beats.map(beatRow));
  ```
  ```rust
  pub fn prompt(brief: &Brief, arc: &NarrativeArc, reasoning: &str, plan: &[(usize, usize, usize)]) -> String {
  ```
- **Verified:** `grep -rni "regenerat\|claim again\|beat.*again" crates/scaena-narrato/src crates/scaena-ops/src/narrative* web/src | head -5` → `crates/scaena-narrato/src/outline.rs:490:                Beat { name: "".into(), claim: "Give again.".into(), slides: 1 },` / `crates/scaena-narrato/src/outline.rs:512:…` / `crates/scaena-narrato/src/outline.rs:520:…` (tests) / `crates/scaena-ops/src/narrative/one.rs:82:…"a slide written again keeps its beat"` (another thing); `outline::prompt`'s signature takes no outline already settled and no note.
- **Do:** Give `outline::prompt` an optional `kept: &[(usize, String)]` (new). These are the beats the person edited, which the prompt names as fixed and asks the model to keep. Post the outline as it stands with `outlineAgain` (a new `kept` field in `protocol.ts`), and have `Generation` pass it through. Add a per-beat "Ask again" that re-asks that beat's claim alone (new `Generation::claim_again`, a short request with the other claims fixed).
- **Why:** The outline is where a person shapes the talk before paying for slides. Today, trying another take loses every edit, so people stop editing or stop exploring.
- **Effort:** M · **Impact:** L

### FUNC-3. Health's Arc measure scores intensities no slide declared as though they were read
- **Location:** `crates/scaena-ops/src/health.rs:736-742`, `:828-834`; `crates/scaena-narrato/src/health/arc.rs:137-141`
- **Proof:**
  ```rust
          None => (m.arc_fidelity.score, m.arc_fidelity.deviant_slides.clone()),
  ...
      let fidelity = Measure {
          score: Some(fidelity_score as u32),
          says: match bends.len() {
              0 => format!("The intensity the slides declare follows {curve_of}."),
  ```
  ```rust
                  // Old decks declare none: a baseline, not a blow to the score.
                  None => {
                      let t = if n > 1 { i as f64 / (n - 1) as f64 } else { 0.5 };
                      0.25 + 0.5 * (1.0 - (t - 0.7).abs())
  ```
- **Verified:** `grep -n "declaring(" crates/scaena-ops/src/health.rs` → `883:        declaring(|n| n.expected_intensity),` / `890:        declaring(|n| n.expected_valence),` (intensity and tone are guarded; fidelity is not).
- **Do:** Count the slides whose `media.narrato` declares an intensity. With none, give `fidelity` `score: None` and the words "No slide declares an intensity yet: plan the arc to measure it", and leave it out of the overall as intensity and tone are left out. This applies "Health says nothing it did not read" (Jay's call, 2026-10-25) to the one measure it missed, so it needs Jay's nod against ADR-0032's "Vzestup's to the last bit".
- **Why:** A deck made by hand shows "Arc 9x: the intensity the slides declare follows a talk's curve" when nothing was declared. The praise is invented and lifts the overall score.
- **Effort:** S · **Impact:** M

### FUNC-4. The browser's assistant forgets the whole conversation on a reload or a reopened deck
- **Location:** `web/src/assistant/index.ts:67-68`
- **Proof:**
  ```ts
  /** The conversation, kept between questions until the page forgets it. */
  let conversation: Message[] = [];
  ```
- **Verified:** `grep -rn "sessionStorage\|localStorage\|indexedDB" web/src/assistant/*.ts | grep -v keys.ts` → `# (no matches)`.
- **Do:** After each finished question, keep the conversation in the trimmed form `recent()` sends (no PNGs) in IndexedDB, keyed by the bundle's recovery key (`copyKey()`). Offer "Continue the last conversation" when that deck opens again. New conversation clears it.
- **Why:** "Make this talk stronger" is a long back-and-forth. A reload, a crash, or reopening the deck tomorrow loses an hour of context, and the model then repeats suggestions already declined.
- **Effort:** M · **Impact:** M

### FUNC-5. The theme's Chyron (Lower Third) layout can never be reached by a made deck
- **Location:** `crates/scaena-narrato/src/layouts.rs:218`; `crates/scaena-ops/src/narrative.rs:3363`
- **Proof:**
  ```rust
      ("chyron", "Chyron", None, &[]),
      ("recipe-card", "Recipe", Some("Playbook"), &["playbook", "recipe"]),
  ```
- **Verified:** `grep -c '"chyron"' crates/scaena-ops/src/narrative.rs` → `0`; `grep -ci chyron crates/scaena-narrato/src/prompt.rs` → `0`; `grep -n '"recipe-card" =>' crates/scaena-ops/src/narrative.rs` → `3363:        "recipe-card" => "recipe",`; `grep -l '"chyron"' docs/examples/themes/*.json` → `dusk.theme.json` / `ember.theme.json` / `salon.theme.json`.
- **Do:** Name `chyron` in Scaena's slot-shapes block (`prompt.rs`'s `SLOT_SHAPES`) as a pictured slide with a name and a role over its lower third. In `narrative.rs`'s layout match, map a pictured `chyron` slide to the theme's `chyron` layout, falling back to the pictured statement where it holds no picture.
- **Why:** Every shipped theme draws a Lower Third, the shape a talk uses to introduce a speaker or a customer over their photo. No generated deck ever uses it, and a slide the model names so becomes plain bullets.
- **Effort:** S · **Impact:** S

## Stability

### STAB-1. The look, fit, and outline readers panic on an answer whose first `}` comes before its first `{`
- **Location:** `crates/scaena-narrato/src/look.rs:229-232`, `crates/scaena-narrato/src/fit.rs:64-67`, `crates/scaena-narrato/src/outline.rs:163-166`; `crates/scaena-narrato/src/one.rs:140-142`
- **Proof:**
  ```rust
  pub fn read(text: &str, seen: &[Seen]) -> Result<Vec<Change>, String> {
      let start = text.find('{').ok_or("the answer holds no JSON object")?;
      let end = text.rfind('}').ok_or("the answer holds no JSON object")?;
      let value: Value =
          serde_json::from_str(&text[start..=end]).map_err(|e| format!("the answer is not the JSON asked for: {e}"))?;
  ```
- **Verified:** `grep -rn "rfind('}')" crates` → `crates/scaena-ffi/tests/generate.rs:300:…` / `crates/scaena-narrato/src/outline.rs:164:…` / `crates/scaena-narrato/src/fit.rs:65:…` / `crates/scaena-narrato/src/answer.rs:104:            let within = text.find('{').zip(text.rfind('}')).filter(|(start, end)| start < end);` / `crates/scaena-narrato/src/look.rs:230:…`. Only `answer.rs` guards `start < end`; `one.rs` reads only `unfenced(text)` and refuses prose around its JSON.
- **Do:** Add a shared `json_within(text) -> Option<&str>` (new, in `crates/scaena-narrato/src/lib.rs`) holding `answer.rs`'s guarded reading. Use it in `look::read`, `fit::read`, `outline::read`, `answer::arc`, and as `one::answer`'s fallback. Add a unit test of `"}{"` and of `"Here's the slide: {…}"` for each.
- **Why:** An answer like `"}{"` (a custom `base` endpoint, a hand-written `--look-answer`) aborts the WASM module or the ABI call during the "polish" steps, after the slides are paid for. A one-slide answer wrapped in a sentence says "Nothing changed".
- **Effort:** S · **Impact:** M

### STAB-2. An HTML page or an SEC filing with an unclosed tag panics the source reader
- **Location:** `crates/scaena-sources/src/html.rs:53-54`; `crates/scaena-sources/src/sec.rs:259-260`, `:321-323`
- **Proof:**
  ```rust
              let end = html[i..].find('>').map_or(s.len(), |e| i + e + 1);
              let tag = &lower[i + 1..end.saturating_sub(1).max(i + 1)];
  ```
  ```rust
              let open_end = lower[start..].find('>').map_or(row_end, |e| start + e + 1);
              let close = lower[open_end..row_end].find("</t").map_or(row_end, |e| open_end + e);
              let text = strip(&inner[open_end..close]);
  ```
- **Verified:** `grep -n 'is_char_boundary' crates/scaena-sources/src/html.rs` → `# (no matches)`; `grep -n 'open_end.min\|min(row_end)\|is_char_boundary' crates/scaena-sources/src/sec.rs` → `474:    while end > 0 && !text.is_char_boundary(end) {` (another function); `grep -n "let lower" crates/scaena-sources/src/html.rs` → `36:    let lower = html.to_ascii_lowercase();`; `grep -rn "html::read" crates/scaena-sources/src/lib.rs` → `139:        "html" | "htm" | "xhtml" => Ok(source(Kind::Html, html::read(&decode(bytes)))),`.
- **Do:** In both readers, when no `>` follows, end the walk (the rest is no tag), so `end - 1` never lands inside a letter. In `sec.rs`'s table walk, clamp `open_end = open_end.min(row_end)`. Add tests for `html::read("<body><a title=é")` and a `<tr><td</tr>` row.
- **Why:** A dropped `.html` source, or a fetched page that ends mid-tag with a non-ASCII letter, kills the generator's worker in the browser and fails the CLI, before any call is made.
- **Effort:** S · **Impact:** M

### STAB-3. On the Mac, a failed or stopped question stays in the conversation, and a failure mid-round leaves tool calls unanswered
- **Location:** `apps/mac/ScaenaKit/Sources/ScaenaKit/Assistant.swift:250-258`, `:294-300`, `:512-516`
- **Proof:**
  ```swift
                  } catch let refusal as Refusal {
                      ...
                      if refusal.tooLong, try chat.shorter() { continue }
                      unsaid()
                      add(.failed(refusal.said))
                      return
  ...
          } catch {
              if Task.isCancelled { settle(chat) } else { add(.failed(Why.said(error, of: chat.provider))) }
          }
  ```
- **Verified:** `grep -n "settle(chat)\|func settle\|take_back\|takeBack\|unsaid" apps/mac/ScaenaKit/Sources/ScaenaKit/Assistant.swift` → `256:                    unsaid()` / `295:            settle(chat)` / `297:            settle(chat)` / `299:            if Task.isCancelled { settle(chat) } else { …` / `443:            unsaid()` / `506:    private func unsaid() {` / `512:    private func settle(_ chat: ScaenaChat) {`; `unsaid()` removes only the shown entry (`if let at = saying { entries.remove(at: at) }`); `grep -rn "take_back\|fn take\b\|pop()" crates/scaena-chat/src crates/scaena-ffi/src/chat.rs` → `crates/scaena-chat/src/lib.rs:262:            let call = calls.pop().expect("a last call");` (the CUT path); the web does it: `grep -n "conversation.pop" web/src/assistant/converse.ts` → `163:    if (o.conversation.at(-1)?.role === "user") o.conversation.pop();`.
- **Do:** Add `Chat::take_back()` (new, `crates/scaena-chat/src/lib.rs`), which pops a trailing user message, and a `"takeBack"` call in `crates/scaena-ffi/src/chat.rs`. Call it from `Assistant.converse` on a refusal, and on a failure or stop before any answer. In the generic `catch`, call `settle(chat)` before `add(.failed(...))`, so every call the round made is answered.
- **Why:** If the key is refused, the person fixes it and asks again, and the model answers both questions as one. If a call throws mid-round, every later question is refused for "tool_use without tool_result" until the window is closed.
- **Effort:** S · **Impact:** L

### STAB-4. A crash of the player's engine mid-show leaves a dead canvas and a status line fullscreen hides
- **Location:** `web/src/player.ts:395`
- **Proof:**
  ```ts
    stage.onError = (e) => (status.textContent = `Error: ${e.message}`);
  ```
- **Verified:** `grep -n "onCrashed\|onLost\|restart(" web/src/player.ts web/src/editor.ts` → `web/src/editor.ts:620:  stage.onCrashed = (crash) => void restarted(crash);` / `web/src/editor.ts:638:      await stage.restart(crash.files ? …` (the editor only); `grep -n "onCrashed" web/src/stage.ts` → `256:  onCrashed: (crash: …) => void = (crash) =>` / `1585:        this.onCrashed({ … });`.
- **Do:** In `player.ts`, set `stage.onCrashed` and `stage.onDied` to `stage.restart(deck)` with the source the player holds. Then `stage.seek` to the index last reported, and show "This slide could not be drawn; → goes on past it" over the stage, not only in the bar.
- **Why:** One slide that runs the engine out of memory, or a panic, freezes the room's screen mid-talk, with no way back short of a reload that lands on slide 1.
- **Effort:** S · **Impact:** M

### STAB-5. A cancelled pointer leaves a turn or a slot drag armed, so the node keeps turning as the pointer moves
- **Location:** `web/src/canvas.ts:3981-3993`; the hover path at `:3851-3852`
- **Proof:**
  ```ts
    overlay.onpointercancel = () => {
      press = starting = marquee = undefined;
      if (reshaping || cropping) {
        reshaping = cropping = undefined;
        draw();
      }
      if (sketch) disarm("Not drawn: the drag was cancelled");
      if (stroke) {
        stroke = undefined;
        draw();
        editor.say("Not drawn: the stroke was cancelled");
      }
      if (drag) void still("The drag was cancelled");
    };
  ```
- **Verified:** `grep -n "turning = \|slotDrag = " web/src/canvas.ts` → `1965:      slotting = slotDrag = slotKeyed = undefined;` / `2038:    slotDrag = { … };` / `2057:    slotDrag = undefined;` / `2294:    turning = { … };` / `3936:      turning = undefined;` / `4064:        slotDrag = undefined;` / `4282:        turning = undefined;`; `sed -n 3851,3852p web/src/canvas.ts` → `    if (turning) return turned(turning, at, e.shiftKey, e.altKey);` / `    if (slotDrag) return slotMove(at);`.
- **Do:** In `onpointercancel`, clear `turning` and `slotDrag` and redraw, saying "The turn was cancelled", as Escape does at `:4279-4283`.
- **Why:** A pen lifted out of range, a touch an OS gesture takes, or a palm rejection mid-turn leaves the node rotating under a hovering pointer, and presses on the desk are ignored until Escape.
- **Effort:** S · **Impact:** S

## Reliability

### REL-1. In the browser, a failure after the outline is settled loses the outline, the research, and the written slides; the next Generate pays for all of it again
- **Location:** `web/src/prompt.ts:595-601`; `web/src/generate.ts:300`, `:389`; `crates/scaena-narrato/src/lib.rs:388-393`
- **Proof:**
  ```ts
      } catch (err) {
        // Closed while it ran, it says nothing more.
        if (worker !== making) return;
        step(err instanceof Error ? err.message : String(err), true);
        busy(false);
        // A worker that failed starts afresh.
        start();
  ```
  ```rust
      /// what a client keeps so a run that fails after them resumes from it, as `--slides` takes
      /// it, with no call paid for again (narrative REL-2).
      pub fn written_text(&self) -> Option<&str> {
  ```
- **Verified:** `grep -rn "written_text\|writtenText" crates/scaena-generator crates/scaena-ffi/src web/src apps/mac apps/ipad crates/scaena-narrato/src/lib.rs crates/scaena-cli/src` → `crates/scaena-narrato/src/lib.rs:391:    pub fn written_text(&self) -> Option<&str> {` (no client uses it); `grep -n "Wrote\|kept.slides" crates/scaena-cli/src/generate.rs` → `934:            Progress::Wrote { text, .. } if paid => kept.slides(text),` (the CLI keeps them); `grep -n "sources.free\|research(" web/src/generate.ts` → `300:    if (job.research) await research(g, brief, asking, job, sources, hear);` / `389:    sources.free();` / …; `grep -n 'type: "generate"' web/src/protocol.ts` → `1066:  | { type: "generate"; generate: Generate; engine?: WebAssembly.Module }` (`Generate` has `research: boolean` and no outline or written slides).
- **Do:** Keep the browser's paid work as the CLI's `.generating` does:
  - Export `writtenText` from `crates/scaena-generator`.
  - In the generator worker, keep the research report, the settled outline (`storySettled`), and the written slides between runs while the dialog is open.
  - Add optional `outline` and `slides` fields to `Generate` (new), which skip the calls they stand for.
  - After a failure, offer "Go on from what was made".
  - If `open()` fails after a deck is made, keep `deck.files` and offer Download.
- **Why:** A 529, a timeout, or a deck that would not validate after the slides were written sends the person back to the arc and the outline. They pay again for research that takes minutes and lose the claims they edited, at the most expensive step of the product.
- **Effort:** M · **Impact:** L

### REL-2. Out of credit is retried six times on every text step, though the picture path knows waiting will not mend it
- **Location:** `crates/scaena-narrato/src/lib.rs:86-88`, `:606-610`
- **Proof:**
  ```rust
  pub fn busy(status: u16) -> bool {
      matches!(status, 0 | 408 | 429 | 500..=599)
  }
  ...
          if busy(status) && self.tried + 1 < TRIES {
              self.tried += 1;
              self.again = Some(providers::wait(after, body, self.tried));
              return Ok(self.step);
  ```
- **Verified:** `grep -rn "insufficient_quota" crates --include=*.rs | grep -v test` → `crates/scaena-ops/src/narrative.rs:9386:…` (a test body) / `crates/scaena-narrato/src/pictures.rs:449:            "insufficient_quota",` / `crates/scaena-narrato/src/pictures.rs:629:…` (test); `grep -n "pub const TRIES" crates/scaena-narrato/src/lib.rs` → `54:pub const TRIES: usize = 6;`.
- **Do:** Move the quota signatures from `pictures.rs:447-458` into a `for_good(status, body)` (new, `lib.rs`). Have `Generation::answer` and `retry_after` treat a 429 that matches it as final, saying "no credit is left on this key", as the pictures already say.
- **Why:** With a key at a $0 balance, the arc step waits about a minute and falls back to the heuristic in silence. The slides step then waits another minute before saying what was wrong all along.
- **Effort:** S · **Impact:** M

### REL-3. The Mac's assistant keeps calling the model after its deck's window closes
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/DeckView.swift:773-777`; `apps/mac/ScaenaKit/Sources/ScaenaKit/Assistant.swift:207`
- **Proof:**
  ```swift
              .onChange(of: undo, initial: true) { _, now in
                  // The assistant's edits are each one step of this window's undo.
                  if assistant == nil { assistant = Assistant(editor: editor) }
                  assistant?.edited = { [weak document = self.document, weak now] before, files in
                      document?.took(before, files: files, undo: now)
  ```
- **Verified:** `grep -rn "assistant?.stop()\|assistant.stop()\|\.stop()" apps/mac/ScaenaKit/Sources/Scaena/*.swift apps/mac/Scaena/*.swift | grep -i assist` → `apps/mac/ScaenaKit/Sources/Scaena/AssistantPanel.swift:192:                Button("Stop") { assistant.stop() }`; `grep -rn "onDisappear" apps/mac/ScaenaKit/Sources/Scaena/DeckView.swift` → `# (no matches)`.
- **Do:** Stop the assistant when its deck's window goes. Call `assistant?.stop()` from an `.onDisappear` on DeckView's root (new). While a turn runs, have the window's close ask first, as `PromptWindow`'s `CloseGuard` does.
- **Why:** Closing a window mid-turn (⌘W) keeps paying for up to 32 rounds on the person's key, and the edits land in a session no one will save.
- **Effort:** S · **Impact:** M

### REL-4. A remote that loses Wi-Fi mid-talk can only go Back and have its code typed again on stage
- **Location:** `apps/mac/ScaenaKit/Sources/ScaenaRemote/RemoteView.swift:130-137`, `:110-114`; `RemoteClient.swift:209-214`
- **Proof:**
  ```swift
              case .lost(let why):
                  ContentUnavailableView {
                      Label("Not Joined", systemImage: "wifi.exclamationmark")
                  } description: {
                      Text(why)
                  } actions: {
                      Button("Back", action: leave)
                  }
  ...
      private func leave() {
          client?.leave()
          client = nil
          chosen = nil
      }
  ```
- **Verified:** `grep -rniE "join again|rejoin|reconnect" --include=*.swift apps` → `apps/mac/ScaenaKit/Tests/ScaenaKitTests/NodesTests.swift:132:@Test func twoNodesAreJoinedByAnArrowPickedByItsLine() throws {` (unrelated).
- **Do:** Keep `chosen` and the code typed when the link is lost. Offer "Join Again" on the `.lost` screen, and try it automatically after 1, 2, and 4 s. Show the connection's `.waiting` state as "Reconnecting…" over the last slide, not as joined.
- **Why:** The presenter's code holds until five wrong tries, so a rejoin would work. Today a Wi-Fi blip strands the speaker on stage, typing eight digits on a phone in front of the room.
- **Effort:** M · **Impact:** M

### REL-5. The presenter view's timer lives in page memory, so a reload or a reopened window shows 0:00 mid-talk
- **Location:** `web/src/player.ts:1178-1187`
- **Proof:**
  ```ts
    let since: number | undefined;
    let counted = 0;
    /** Whether the show has begun, and whether Pause holds the timer. */
    let begun = false;
    let held = false;
    const begin = (at: At) => {
      if (begun || (at.index === 0 && !at.playing)) return;
      begun = true;
      if (!held) since = performance.now();
    };
  ```
- **Verified:** `grep -n "sessionStorage\|localStorage" web/src/player.ts` → `239:    const size = Number(localStorage.getItem(NOTES_SIZE));` / `1101:      localStorage.setItem(NOTES_SIZE, String(notesSize));` (the notes' size only).
- **Do:** Have the room's player keep the show's start (a wall-clock time, so it survives a reload), the time counted, and whether it is paused. It sends them with each `at` it reports, and the presenter view adopts them on `hello`. Keep the same three in `sessionStorage` under the deck's key, so reloading either window keeps them.
- **Why:** Reloading the presenter view, or closing and reopening it with P, resets the timer twenty minutes in. The pace line then says "20:00 ahead" exactly when the speaker needs it to be right.
- **Effort:** S · **Impact:** M

## Security

### SEC-1. The JPEG decoder admits a frame twice the painters' pixel budget, about 1.3 GB from a 1 MB file
- **Location:** `crates/scaena-pixels/src/jpeg.rs:638-642`, `:696`; called from `crates/scaena-paint/src/lib.rs:326-330`
- **Proof:**
  ```rust
          let side = MAX_IMAGE_SIDE as usize;
          if !self.header && (width > side || height > side) {
              return Err(JpegError(format!(
                  "{width} × {height} px; images are at most {MAX_IMAGE_SIDE} px a side (SPEC §3.3)"
              )));
  ```
- **Verified:** `grep -n "MAX_PIXELS\|MAX_IMAGE_SIDE" crates/scaena-pixels/src/jpeg.rs | head` → `29:use crate::MAX_IMAGE_SIDE;` / `638:        let side = MAX_IMAGE_SIDE as usize;` / `641:…`; `grep -n "c.coefs = vec!\[0" crates/scaena-pixels/src/jpeg.rs` → `696:                c.coefs = vec![0; c.bw * c.bh * 64];`; `grep -rn "pub const MAX_PIXELS" crates/` → `crates/scaena-paint/src/lib.rs:671:pub const MAX_PIXELS: u64 = 1 << 25;`; `sed -n 326,330p crates/scaena-paint/src/lib.rs` → `fn jpeg(…)` reads the `Header` then calls `decode` with no pixel check.
- **Do:** In the SOF handler, refuse `width × height` past `MAX_IMAGE_SIDE²/2` (= `1 << 25`, which `scaena-pixels` cannot import from paint, so define `MAX_IMAGE_PIXELS` (new) beside `MAX_IMAGE_SIDE` and have paint's `MAX_PIXELS` read it). Also cap the coefficient total by components. Add a test with a crafted 8192×8192 CMYK header.
- **Why:** A bundle carrying one crafted progressive JPEG makes every painter allocate coefficients, planes, and RGBA for 67M pixels × 4 components. That gets the iPad app killed and runs the browser's worker out of memory as the deck opens. The 33.5M-pixel limit CLAUDE.md promises for a raster does not hold here.
- **Effort:** S · **Impact:** M

### SEC-2. A spreadsheet's repeated header names hang the import, and the workbook has no total-cell bound across sheets
- **Location:** `crates/scaena-sources/src/xlsx.rs:36-40`, `:70-77`
- **Proof:**
  ```rust
              let (mut name, mut n) = (base.clone(), 2);
              while names.contains(&name) {
                  name = format!("{base} {n}");
                  n += 1;
              }
  ...
      for sheet in workbook.child("sheets").into_iter().flat_map(|s| s.all("sheet")) {
          ...
          let Some(doc) = pkg.xml(&target) else { continue };
          out.push(Sheet { name, rows: rows(&doc, &strings)? });
  ```
- **Verified:** `grep -n 'HashSet\|HashMap\|MOST_COLUMNS\|MOST_CELLS' crates/scaena-sources/src/xlsx.rs` → `57:const MOST_CELLS: usize = 1 << 21;` / `60:const MOST_COLUMNS: usize = 16_384;` / `113:        if n > MOST_COLUMNS {` / `149:        if held > MOST_CELLS {` (per sheet; no set, no workbook total).
- **Do:** Keep the taken names in a `HashSet`, with a per-base next-suffix `HashMap`. Carry the cells counted across sheets against one workbook total, and skip a sheet target already read.
- **Why:** A header row of 16,384 cells all reading "x", which the bounds allow, is about 10¹² string compares, so the generator's worker never finishes. A workbook whose sheets all point at one dense part re-reads it until the 1 GB zip bound is spent.
- **Effort:** S · **Impact:** M

### SEC-3. The Office XML parser bounds depth but not the number of elements, so one 64 MB part becomes gigabytes of tree
- **Location:** `crates/scaena-sources/src/xml.rs:96-99`, `:151-152`
- **Proof:**
  ```rust
                      Some(b'/') => {
                          p.i += 1;
                          p.expect(b'>')?;
                          let e = Element { name, attrs, children: Vec::new() };
                          stack.last_mut().expect("the root").children.push(Node::Element(e));
  ```
- **Verified:** `grep -n 'MAX_ELEMENTS\|MOST_ELEMENTS\|nodes' crates/scaena-sources/src/xml.rs | head` → `# (no matches)`; `sed -n 99p crates/scaena-sources/src/xml.rs` → `pub const MAX_DEPTH: usize = 256;`.
- **Do:** Count every element and text node pushed. Past a budget (new `MAX_NODES`, 2M per part), return `XmlError { why: "more than 2,000,000 elements" }`, and test it with a part of 3M `<a/>`.
- **Why:** `<a/>` is 4 bytes and about 100 bytes as a node, so a part inside `ooxml::BOUNDS.part` reaches about 1.6 GB. That is enough to get the iPad killed or exhaust the generator's WASM memory from one dropped Word file.
- **Effort:** S · **Impact:** M

### SEC-4. The RTF reader's brace stack grows without bound
- **Location:** `crates/scaena-sources/src/rtf.rs:44-52`
- **Proof:**
  ```rust
          let (skipping, uc) = *stack.last().expect("the document's group");
          match s[i] {
              b'{' => {
                  stack.push((skipping, uc));
                  i += 1;
              }
              b'}' => {
                  if stack.len() > 1 {
                      stack.pop();
  ```
- **Verified:** `grep -n "MAX_DEPTH\|stack.len() >" crates/scaena-sources/src/rtf.rs` → `51:                if stack.len() > 1 {` (the pop guard alone).
- **Do:** Past 256 open groups, as `xml.rs`'s `MAX_DEPTH`, count further `{` without pushing and pop the count first, or refuse the file with a reason. Add a test of 10 MB of `{`.
- **Why:** At 16 bytes per `{`, a 64 MB source of braces grows a 1 GB `Vec` before a word is read. The generator's worker or the iPad dies on a file a person was handed.
- **Effort:** S · **Impact:** S

Four items in Security. The rest of the untrusted-input surface surveyed clean: `scaena serve`'s tokens and sandbox, `fetch`'s address checks, the SVG/HTML escaping, the OOXML zip bounds, and the FFI's guards. The outline's unfenced prompt is the 20261031 plan's Deferred.

## Usability

### USE-1. Whether a Story finding is an error, a warning, or an info is said by colour alone
- **Location:** `web/src/story.ts:286`; `web/editor.html:559-560`; `apps/mac/ScaenaKit/Sources/Scaena/StoryPanel.swift:107-109`
- **Proof:**
  ```ts
      li.dataset.severity = SEVERITY[Math.max(...run.map((f) => SEVERITY.indexOf(f.severity)))];
  ```
  ```css
        #story li[data-severity="error"] [data-show] { color: var(--error); }
        #story li[data-severity="warning"] [data-show] { color: var(--warning); }
  ```
- **Verified:** `grep -n "severity" web/src/story.ts` → `80:  const errors = h.findings.filter((f) => f.severity === "error");` / `83:    .filter(([f]) => f.severity === "warning")` / `286:    li.dataset.severity = …` (no text, label, or icon); `grep -n 'data-severity' web/editor.html` → `559:…` / `560:…`.
- **Do:** Prefix each finding's slide button with a glyph and a hidden word ("Error: Slide 3"), and give the button that `aria-label`, in `story.ts`. On the Mac, add the matching SF Symbol and an `.accessibilityLabel` with the severity beside `.foregroundStyle(Self.tint(...))`.
- **Why:** The Story's job is to say what to fix first. A colour-blind or VoiceOver user cannot tell a slide lint broke from a suggestion (WCAG 1.4.1).
- **Effort:** S · **Impact:** M

### USE-2. The assistant clears the question before sending it, so a refused key or a lost connection loses what was typed
- **Location:** `web/src/assistant/panel.ts:407-415`; `apps/mac/ScaenaKit/Sources/Scaena/AssistantPanel.swift:211-212`
- **Proof:**
  ```ts
    async function ask(text: string): Promise<AssistantEvent> {
      text = text.trim();
      if (!text || working) return { kind: "failed", message: working ? "the assistant is working" : "nothing asked" };
      ...
      entry("user", text);
      question.value = "";
  ```
- **Verified:** `grep -n "question.value" web/src/assistant/panel.ts` → `260:    void ask(question.value);` / `265:      void ask(question.value);` / `415:    question.value = "";` (never put back); `failure()` (`:460-463`) only adds an entry.
- **Do:** When the outcome is `failed`, or `stopped` before any answer, put the text back into `#question` (web) and `question` (Mac), with the caret at its end. Clear it only once the model has begun to answer.
- **Why:** A paragraph-long "rewrite slides 4–7 so they build to the ask" is gone after a 401 or a dropped connection, so the person must remember and retype it.
- **Effort:** S · **Impact:** M

### USE-3. The outline never shows its slides in all against the length asked, before the slides are paid for
- **Location:** `web/src/prompt.ts:174-180`
- **Proof:**
  ```ts
      slides.className = "slides";
      slides.type = "number";
      slides.min = "0";
      slides.max = "60";
      slides.step = "1";
      slides.value = String(beat.slides);
  ```
- **Verified:** `grep -n "reduce\|total" web/src/prompt.ts | head -3` → `# (no matches)`.
- **Do:** Add a live line under the beats in `#prompting-outline`, such as "14 slides in all · the length asked: 10–15", updated on each beat's `input`, moved row, or removed row. Flag it when outside the range the session's `length` sets, since CLAUDE.md's outline scales a sum outside it to the nearer end. Show the same on the Mac's `outlineView`.
- **Why:** The person sets each beat's slides but cannot see that they now add up to 22 against a 10-slide talk. The scaling then quietly undoes their choice.
- **Effort:** S · **Impact:** M

### USE-4. Every beat's fields carry the same label, and Remove beat cannot be undone
- **Location:** `web/src/prompt.ts:168-202`
- **Proof:**
  ```ts
      name.setAttribute("aria-label", "The beat's name");
  ...
      claim.setAttribute("aria-label", "The beat's claim");
  ...
      act("↑", "Move the beat up", () => li.previousElementSibling?.before(li));
      act("↓", "Move the beat down", () => li.nextElementSibling?.after(li));
      act("Remove", "Remove the beat", () => li.remove());
  ```
- **Verified:** `grep -n "aria-label" web/src/prompt.ts` → `173:    name.setAttribute("aria-label", "The beat's name");` / `181:    slides.setAttribute("aria-label", "The slides the beat takes");` / `187:    claim.setAttribute("aria-label", "The beat's claim");` / `194:      button.setAttribute("aria-label", aria);` / `262:        remove.setAttribute("aria-label", \`Remove ${item.textContent}\`);` (only the sources name their item).
- **Do:** Label each field by its place ("Beat 3 of 6: claim") and relabel after a move or removal. Say the move in `#prompting-progress` ("Moved to 2 of 6"). Keep a removed row with its index and show "Beat removed · Undo" for ten seconds.
- **Why:** A screen reader hears six identical "The beat's claim" fields. A click on Remove deletes a claim the person rewrote, and nothing brings it back.
- **Effort:** S · **Impact:** S

### USE-5. On the Mac, a finding in the issues list can be reached only by a tap
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/FindingsPanel.swift:20-28`, `:66-70`
- **Proof:**
  ```swift
          List {
              ForEach(found.indices, id: \.self) { i in
                  FindingRow(finding: found[i], about: about(found[i]), explain: explain, fix: fix)
                      .contentShape(Rectangle())
                      .onTapGesture { go(found[i]) }
              }
          }
  ```
- **Verified:** `grep -n "accessibility" apps/mac/ScaenaKit/Sources/Scaena/FindingsPanel.swift` → `84:                .accessibilityLabel("Explain")`; `:70` → `Image(systemName: symbol).foregroundStyle(tint)` (no label).
- **Do:** Give the `List` a `selection:` so the arrows and Return go to a finding's slide. Add `.accessibilityAction(named: "Show") { go(found[i]) }` and an accessibility label on the severity symbol ("Error", "Warning"), as the light table's slides have actions.
- **Why:** View › Issues is where a person sees what is broken. A keyboard or VoiceOver user can read the list but not get from a finding to the slide it is about.
- **Effort:** S · **Impact:** S

## Attractiveness / Sexiness

### ATTR-1. The longest waits in making a deck have nothing to watch: no elapsed time, and the look pass's rounds go unannounced
- **Location:** `web/src/generate.ts:693`, `:701-710`; `web/src/prompt.ts`
- **Proof:**
  ```ts
        if (!rounds.length) hear({ kind: "looking", slides: making.count });
  ...
      for (const [i, { round, part }] of rounds.entries()) {
        const reply = await replies[i];
        try {
          changes.push(...(JSON.parse(read(name, reply, () => generation.lookAnswer(reply.status, reply.statusText, reply.body, part))) as { slide: number }[]));
  ```
- **Verified:** `grep -n 'kind: "drawn"' web/src/generate.ts` → `687:…` / `765:…` (both inside the look); `grep -n "elapsed\|setInterval\|performance.now\|Date.now" web/src/prompt.ts` → `# (no matches)`.
- **Do:**
  - Give the current progress line a ticking elapsed time ("Writing the slides · 1 min 20 s").
  - Post a `{ kind: "lookedAt", round, of }` event (new, in `protocol.ts`) as each look round's answer is read, and mark that round's tiles in the strip.
  - Draw the strip as soon as the deck is built, even with the look off.
  - Do the same in the Mac's `PromptView`.
- **Why:** Gamma streams cards in as they are written. Scaena shows one static line for up to fifteen minutes, so the person cannot tell working from stuck.
- **Effort:** M · **Impact:** L

### ATTR-2. The assistant opens to an empty panel with no narrative-first starting points
- **Location:** `web/editor.html:1073`; `apps/mac/ScaenaKit/Sources/Scaena/AssistantPanel.swift:174-181`
- **Proof:**
  ```swift
      private var introduction: String {
          if let listing { return listing }
          return """
              Ask about the deck, or ask for an edit: "this" means the state shown and what is \
              selected. Each edit shows as it is made, and Undo takes it back. Your key goes to \
              \(provider.name) alone.
              """
  ```
- **Verified:** `grep -n "suggest\|starter\|chip" web/src/assistant/panel.ts apps/mac/ScaenaKit/Sources/Scaena/AssistantPanel.swift` → `# (no matches)`; `grep -n 'id="transcript"' web/editor.html` → `1073:          <ol id="transcript" aria-label="Conversation" aria-live="polite"></ol>`.
- **Do:** Under an empty transcript, offer three or four one-click asks built from the deck's own health: "Make this talk stronger" (which leads with `deck_health`), "Tighten {the weakest beat's name}", and "Check every slide says its claim". Each is a button that fills and sends the question, on the web and the Mac, and they go away once a conversation begins.
- **Why:** Scaena's edge is narrative health, and the assistant's first screen is a sentence of instructions. Pitch's and Gamma's panels greet a person with actions.
- **Effort:** S · **Impact:** M

### ATTR-3. The claim-restate bar and the slot placeholders still use plain material beside the canvas's Liquid Glass
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/DeckView.swift:2094`; `apps/mac/ScaenaKit/Sources/Scaena/CanvasHandles.swift:392`, `:405`
- **Proof:**
  ```swift
          .padding(.horizontal, 12)
          .padding(.vertical, 6)
          .background(.regularMaterial, in: Capsule())
  ```
  ```swift
              .background(.thinMaterial, in: Circle())
  ```
- **Verified:** `grep -rn '\.floating(in' apps/mac/ScaenaKit/Sources/Scaena | wc -l` → `3`; `grep -n "func floating" apps/mac/ScaenaKit/Sources/Scaena/Glass.swift` → `7:    @ViewBuilder func floating(in shape: some Shape, lifted: Bool = false) -> some View {`.
- **Do:** Replace the three `.background(.…Material, in:)` calls with `.floating(in: Capsule())`, `.floating(in: shape)`, and `.floating(in: Circle())`. They then turn to glass on macOS and iPadOS 26 and keep their material before it.
- **Why:** The Update/Keep bar sits beside the code chip and the canvas hint, which are already glass, so it reads as a different system on the one screen where the story is being edited.
- **Effort:** S · **Impact:** S

### ATTR-4. A deck that takes minutes to make on the Mac gives no sign when it is done or failed while the person is in another app
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/PromptWindow.swift:942-959`
- **Proof:**
  ```swift
                  newDocument { ScaenaDocument(generated: session, said: made.words) }
                  // Made: nothing is under way, so the window's close asks nothing.
                  running = nil
                  dismissWindow(id: PromptWindow.id)
  ...
              } catch {
                  ...
                  say(Why.said(error), failed: true)
  ```
- **Verified:** `grep -rn -i 'requestUserAttention\|UNUserNotification\|NSSound' apps/mac/ScaenaKit/Sources` → `# (no matches)`.
- **Do:** When the app is not active, post a local notification on success ("Your deck about {topic} is ready: 14 slides, story 82") and on failure, using its reason. On macOS, also call `NSApp.requestUserAttention(.informationalRequest)`. On the iPad, post it when the run kept in the background ends.
- **Why:** Research, outline, slides, and look run five to fifteen minutes, and people switch away. Today they come back to a deck long done, or to a run that stopped at minute three asking for a key.
- **Effort:** S · **Impact:** M

### ATTR-5. The inspector's sliders show nothing on the slide until they are let go
- **Location:** `web/src/look.ts:935-943`
- **Proof:**
  ```ts
    // A slider's readout follows it as it is dragged, before it is let go.
    for (const panel of panels) listen(panel, "input", (e) => {
      const target = e.target;
      if (!(target instanceof HTMLInputElement) || target.type !== "range") return;
      const said = adjustment(Number(target.value));
      target.setAttribute("aria-valuetext", said);
      const readout = target.nextElementSibling;
      if (readout?.classList.contains("lives")) readout.textContent = said;
    });
  ```
- **Verified:** `grep -rn "stage.preview(\|\.preview(" web/src/*.ts | head` → `web/src/backgrounds.ts:152:…stage.preview(now.state, o.patch, around.format())…` / `web/src/look.ts:782:…stage.preview(now.state, s.patch, around.format())…` / `web/src/worker.ts:440:        player.preview(undefined);` / … (the layouts and backgrounds use it; the sliders do not).
- **Do:** On each slider's `input`, send a throttled (one per frame) `stage.preview(state, patch)` of the `choose` the release would make, and on `change` commit it as today. Do the same for the Theme tab's colour swatches (`theme-panel.ts:202-209`), committing once when the picker closes.
- **Why:** Adjusting a photo's exposure or a theme colour is blind until release, and each try is a full edit and an undo step. Keynote and Pitch scrub live.
- **Effort:** S · **Impact:** M

---

## First move

**REL-1. In the browser, a failure after the outline is settled loses the outline, the research, and the written slides; the next Generate pays for all of it again** (from Reliability)

Making a deck from a prompt is the product's first impression and its most expensive step. Today the browser throws away every paid answer, and the outline the person shaped, the moment anything after it fails. The parts to fix it already exist:
- `Generation::written_text` keeps the written slides, and nothing calls it.
- The CLI's `.generating` folder shows exactly what to keep: research, outline, and slides.
- `Generate` needs only two optional fields to skip the calls already made.

It also turns STAB-1's look and fit panics from "start over" into "go on". It makes REL-2's credit errors cheap to recover from, and it gives FUNC-2's outline edits a place to survive. No other item saves a person both money and the work they put into their story.

## Dropped during verification

- **Pacing scored against a made-up 18 slides when no length was asked** (`health.rs:441-452`). Cited code does what ADR-0032 keeps: "Vzestup's, against the 18 slides a talk of no length is paced for" is the documented figure, which only Jay can change. Moved to Deferred.
- **Keep turns rehearsal times into auto-advance** (`rehearse.ts:282`). Cited code does what it says: "Keep makes each slide advance by itself after the time you spent on it" is PowerPoint's Rehearse Timings. Whether to keep times as pace instead is a design call. Moved to Deferred.
- **A timed-out slides call is retried six times, about 90 minutes** (`generate.ts:113-123`). The retry is deliberate (lift-up REL-4: "asked again as a busy one is past it"), so only its count is a question. Moved to Deferred.
- **The Mac's document save runs without the 16 MB stack** (`ffi/lib.rs:974-994`). Real (no `deep(...)` on `scaena_snapshot_save`, which #819 does not touch), but no overflow was measured. Kept in Deferred.
- **The Story graph is keyboard- and screen-reader-inaccessible**: in the 20261031 plan's Deferred ("The Story graph is mouse-only").
- **The Story never coaches W422/W423/W426/W429**: in the 20261031 plan's Deferred ("The Story never coaches W422/W423").
- **The outline's claims and big idea go into the slides prompt unfenced**: in the 20261031 plan's Deferred ("The outline prompt and block are unfenced").
- **Health's Drift fix replaces beat paths with no `test` guard**: in the 20261031 plan's Deferred ("Health's 'Make it part of' fix uses `replace`…").
- **The Mac's assistant retries timeouts alone, not dropped connections**: in the 20261031 plan's Deferred.
- **The Mac's assistant doesn't follow a streaming answer**: in the 20261031 plan's Deferred.
- **A stream that ends without `message_stop` is taken as whole**: in the 20261031 plan's Deferred.
- **Closing Write/Add a Slide while it prepares still sends the request**: in the 20261031 plan's Deferred (Make a Picture's sibling) and too close to #812's subject. Deferred.
- **New from a Prompt reads each file on the main actor**: in flight, #768.
- **⌘Z on the iPad while the assistant works**: in flight, #771.
- **VoiceOver hears no progress on the Mac**: in flight, #780.
- **The Mac's Format tab draws on the main actor**: in flight, #783.
- **An assistant answer as one undo step**: in flight, #728.
- **A made deck's speaker notes**: in flight, #743.
- **The Story offering a topic headline its claim**: in flight, #779.

## Deferred

- **Pacing with no length asked uses Vzestup's 18** (`health.rs:441-452`). It could pace by the talk's timed seconds where there are any: Jay's call against ADR-0032.
- **Keep the rehearsal's times as the talk's pace, not as auto-advance** (`rehearse.ts:282`, `Rehearsing.swift:138-139`). A design call, and FUNC-1 is the first half.
- **Ignore key repeats for go-on keys in the rehearsal and the show, and don't make Keep the default action** (`Rehearsing.swift:57`, `:171`).
- **A rehearsal's Keep has no assistant-at-work guard** (`editor.ts:1489-1491`), and results reopened later can name states since deleted.
- **The assistant's resources module, failing once to load, is never loaded again** (`assistant/index.ts:88-89`). `icon-set.ts:40` shows the one-line reset.
- **The browser's assistant does not retry `unreached` (status 0)** (`converse.ts:49-55`).
- **Anthropic's cached tokens are left out of "tokens in"** (`assistant/providers.ts:236`, `chat/providers.rs:414-415`).
- **Anthropic answers capped at 8192 output tokens** (`assistant/providers.ts:215`, `chat/providers.rs:363`).
- **A timed-out slides call is retried up to six times** (`narrato/lib.rs:606`, `generate.ts:150`).
- **A refused key (401/403) on the arc step falls back to the heuristic in silence** (`narrato/lib.rs:615-628`).
- **The outline is grounded in 6,000 UTF-16 units of the sources** (`outline.rs:18-19`).
- **W420/W421 offer no fix where one headline-role text stands on the slide** (`lint/narrative.rs:84`).
- **W425 fires on two beats with empty claims** (`lint/narrative.rs:306`).
- **A syntax error mid-typing blanks the Story tab**; the score's live region re-announces on every read (`story.ts:225-233`, `editor.html:1044`).
- **The Story's fix button does nothing while the assistant works**; "Ask the assistant" overwrites a draft (`editor.ts:1412-1422`, `DeckView.swift:1908-1909`).
- **A Mac or iPad show keeps the slide list from Play** while the assistant edits (`Presenting.swift:388`). The Mac's timer starts at Play, not at the first advance (`Show.swift:235-237`).
- **The web player forgets the slide on reload** (`player.ts:845-846`; `replaceState` is used only by the editor).
- **Remote presses that fail to send are dropped in silence** (`Remote.swift:458`).
- **Browser movie export has no Cancel** (`movie.ts:65-68`).
- **⌘Q quits during New from a Prompt without asking** (`PromptWindow.swift:507`, with no `applicationShouldTerminate`).
- **The Mac's brief takes no dropped files** (`PromptWindow.swift:412`).
- **The Mac's document save runs without the deep stack** (`ffi/lib.rs:974-994`).
- **A file dropped on the source while the assistant works is still inserted** (`editor.ts:911-916`).
- **Cell edits to a file-backed data source are not on the editor's undo stack** (`editor.ts:1312-1319`, `data.ts:292-294`).
- **A recovery copy that fails to be kept is said only to the console** (`editor.ts:704`).
- **Read again from disk enters undo history as a plain edit** (`editor.ts:2991-2996`).
- **A renamed or deleted kept deck strands its recovery copy** (`folders.ts:59`, `:82`).
- **`tell()` turns the document into a string three times per keystroke** (`editor.ts:606`, `:722`).
- **An HTML script that writes `<script>` in a string loses the rest of the page** (`html.rs:58-65`).
- **The history keeps a full copy of a data file for every edit** (`crdt.rs:854-858`).
- **A PNG's RGBA pixels are copied once more on decode** (`paint/lib.rs:288-293`).
- **A huge DOCX `outlineLvl` overflows in debug builds** (`docx.rs:75-76`).
- **The theme gallery's focused tile looks almost the same as the chosen one** (`editor.html:738-739`).
- **No Ctrl/⌘+Enter submits the prompt dialog or Write/Add a Slide** (`prompt.ts:461-465`, `slide-ask.ts:134-137`).

Run `/burn-down` to execute this plan.
