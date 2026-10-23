# Lift-Up Plan: Scaena — narrative and AI-first surfaces

> Platform: mixed — a Rust workspace serving a Vite/TS web editor (`web/`), a SwiftUI Mac app and an iPad app (`apps/mac`, `apps/ipad`, one ScaenaKit), a CLI, and an MCP server. This plan audits the narrative and AI-first surfaces across all of them.
> Surveyed: 2026-10-23
> Baseline: main@004f66ae — fetched, up to date with origin/main (0 ahead, 0 behind; HEAD detached at main's commit); clean tree; drift re-check before write-up: HEAD still 004f66ae, origin/main still 004f66ae
> Scope: the narrative and AI-first surfaces only — generation (`scaena-narrato`, `scaena_ops::narrative`, `scaena generate`, the browser's, the Mac's, and the iPad's New from a prompt, `scaena-sources`); narrative health (the W4xx lints, the spine and beats, the Story tab and panel, presenter notes, Rehearse); the assistant (`web/src/assistant`, `scaena-chat`, `scaena-resources`, the writing tools and Foundation Models, the Mac's and the iPad's assistant); and agents (`scaena-mcp`, `scaena-ops` operations with no tool, `skills/`, `docs/authoring.md`, the agent-run findings). Nothing outside it was audited.
> Coverage: full within scope. Eight read-only survey agents mapped the areas (generation, the built deck and `generate`, sources, health and spine, the browser's assistant, the Mac's and iPad's assistant, MCP and skills, the browser's prompt flow); every item below was then re-read, grepped, and quoted inline in this session.
> Attractiveness anchor: Keynote for polish, Gamma for the AI-first flow (brief → outline → slides), Duarte's *Resonate* for narrative structure (a throughline, what is against what could be, a call to action) — user-supplied. The Functionality and Attractiveness lenses are weighted toward narrative quality, as asked.

A note on one constraint that shapes several `Do:` fields: ADR-0026 keeps Narrato's prompts byte for byte with Vzestup's, and Scaena adds one marked block of its own (`SLOT_SHAPES`, `crates/scaena-narrato/src/prompt.rs:188`) plus its own extra calls (`fit_request`, `look_request`). Every item that wants the model told something new routes it through that block or a Scaena-own call, never through Narrato's text.

## Performance

### PERF-1. Read the deck's health off the main actor on the Mac and the iPad
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/StoryPanel.swift:58-63`
- **Proof:**
  ```swift
        .task(id: editor.revision) { read() }
    }

    private func read() {
        do {
            health = try editor.session.health()
            why = nil
  ```
- **Verified:** `grep -n 'lintEvery\|laying\|Task.detached\|nonisolated' apps/mac/ScaenaKit/Sources/ScaenaKit/DeckEditor.swift | head -8` →
  ```
  13:/// once; `lintEvery` lints every state once the deck opens and once edits stop, laid out away
  93:        // it (`lintEvery`), away from the window: opening waits on no lint (PLAN 4.13).
  156:    /// browser's typing is: the findings shown stay until edits stop and `lintEvery` runs. The
  193:    /// Draw `nodes` moved `by` canvas units in frames at rest, laying nothing out: a drag as it
  241:    /// With no state shown, what compiling found stands until `lintEvery` comes back.
  253:    public func lintEvery(isolation: isolated (any Actor)? = #isolation) async {
  254:        guard valid, let laying = session.laying() else { return }
  259:        await linter.lay(laying, for: began)
  ```
  `grep -n 'func health' -B2 -A6 apps/mac/ScaenaKit/Sources/ScaenaKit/*.swift | head -20` → `apps/mac/ScaenaKit/Sources/ScaenaKit/Health.swift:64:    public func health() throws -> Health { try call("health") }` (a synchronous FFI call; no async or detached twin).
- **Do:** Give `DeckEditor` a `healthEvery` `(new)` beside `lintEvery` (`DeckEditor.swift:253`): take a snapshot the way `session.laying()` hands one, run the FFI `health` call away from the main actor, and drop stale answers with a counter as `web/src/story.ts:108-112` does with `asking`. Debounce it to "once edits stop", the cadence `lintEvery` already uses, instead of every `editor.revision`.
- **Why:** The Story panel resolves every state and runs coherence and arc warping on each revision while the user types; PLAN 4.13 moved lint off the main thread for exactly this reason and left health behind.
- **Effort:** M · **Impact:** M

### PERF-2. Run the Mac assistant's tool calls off the main actor
- **Location:** `apps/mac/ScaenaKit/Sources/ScaenaKit/Assistant.swift:152`
- **Proof:**
  ```swift
                        let ran: Ran
                        if call.name == "map_fetch" {
                            ran = try await mapFetch(index, args: call.args, chat: chat)
                        } else {
                            ran = try chat.run(index, on: editor.session)
                        }
                        entries[at].kind = .call(name: call.name, args: call.args, ran: ran)
  ```
- **Verified:** `grep -n '@MainActor' apps/mac/ScaenaKit/Sources/ScaenaKit/Assistant.swift | head -3` → `8:@MainActor` (the whole class is main-actor isolated; `chat.run` is synchronous).
- **Do:** Run `chat.run(index, on:)` through the same off-main path PERF-1 adds, handing the session a snapshot for `deck_lint`, `deck_render`, and `deck_health` (`deck_patch` must stay on the session's thread, as the editor's edits do). Keep the `Sweep()` animation (`AssistantPanel.swift:305`) running while the call works.
- **Why:** `deck_render` draws a frame and `deck_lint` lays every reached state out inside the window's actor, so the sweep animation and the window stall for each call of a multi-call answer; on B1-sized decks that is seconds per round (ADR-0011's own timings).
- **Effort:** M · **Impact:** M

### PERF-3. Build and draw the generated deck off the main actor on the Mac and the iPad
- **Location:** `apps/mac/ScaenaKit/Sources/ScaenaKit/Generator.swift:199-200`, `:622`
- **Proof:**
  ```swift
  @MainActor
  public final class Generator {
  ```
  ```swift
          hear(.making)
          let making: Making = try step("make", ["theme": .string(theme), "key": .string(key), "look": .bool(look)])
  ```
- **Verified:** `sed -n 455,470p crates/scaena-ffi/src/generate.rs` → the `make` step builds the deck and draws every slide synchronously: `let drawn = making.drawn(scaena_ops::narrative::LOOK_WIDTH).map_err(said)?;` after `Making::new(built, &arc, fallback, shipped.file, &theme, &fonts)`. `grep -n 'Task.detached\|nonisolated' apps/mac/ScaenaKit/Sources/ScaenaKit/Generator.swift` → `218:    nonisolated public static var fontsKept: URL? {` / `233:    nonisolated public static func choices() throws -> BriefChoices {` — two statics; the `step` calls and the class stay on the main actor.
- **Do:** Make the `step("make", …)` and `relook` calls `nonisolated` and await them from a detached task, as `lintEvery` does for lint; the `Made` handle already lives outside the view. Keep `hear(.making)` on the main actor.
- **Why:** The brief window freezes for the whole of building a 20- to 45-slide deck and drawing every slide for the look pass; on the iPad that also means the progress list stops scrolling at the step the user most wants to watch.
- **Effort:** M · **Impact:** M

### PERF-4. Give `deck_render` a smaller default size over MCP
- **Location:** `crates/scaena-mcp/src/lib.rs:438-439`, `:749-751`
- **Proof:**
  ```rust
      /// `WxH` pixels, with the canvas's aspect ratio; the canvas's size without it.
      #[serde(default)]
      pub size: Option<String>,
  ```
  ```rust
          let png = base64::engine::general_purpose::STANDARD.encode(&frame.png);
          let facts = serde_json::to_string(&facts).map_err(|e| failure(OpsError::new(e.to_string())))?;
          Ok(CallToolResult::success(vec![ContentBlock::image(png, "image/png"), ContentBlock::text(facts)]))
  ```
- **Verified:** `grep -n 'size' crates/scaena-mcp/src/lib.rs | sed -n 1,12p` →
  ```
  130:    (&a.bundle, &a.format, &a.states, &a.out, &a.size, &a.fps, &a.audio, &a.painter).hash(&mut hasher);
  224:    op: Option<usize>,
  360:    pub limit: Option<usize>,
  438:    /// `WxH` pixels, with the canvas's aspect ratio; the canvas's size without it.
  440:    pub size: Option<String>,
  466:    /// size without it.
  468:    pub size: Option<String>,
  573:    pub errors: usize,
  595:    pub size: [u32; 2],
  728:            size: a.size,
  742:            size: frame.size,
  765:            size: a.size.clone(),
  ```
  `size` is passed through unchanged at the render arm (`:728`) with no default; the example decks' canvas is 1920×1080.
- **Do:** When `size` is unset in `DeckRender`, render at half the canvas (960×540 for a 1920×1080 deck) and say so in the tool description; keep full size behind an explicit `size`. The session's `Player.png` already takes a width the same way.
- **Why:** An agent's render-look-fix loop sends each frame back as a base64 image in the conversation; a full-canvas PNG is four times the tokens of the half-size one for no gain at the model's resolution.
- **Effort:** S · **Impact:** M

### PERF-5. Send the look pass its slides in batches
- **Location:** `crates/scaena-ops/src/narrative.rs:775-782`
- **Proof:**
  ```rust
  pub fn drawn(write: &Write, width: u32) -> Result<Vec<Picture>, OpsError> {
      let mut drawing = Drawing::new(write)?;
      (write.deck.states.iter())
          .map(|state| {
              let raster = drawing.draw(&state.id, width)?;
              let jpeg = scaena_export::pdf::jpeg(&raster.rgba, raster.width, raster.height, 82)
  ```
- **Verified:** `grep -n 'chunk\|batch\|LOOK_MOST\|most_pictures' crates/scaena-ops/src/narrative.rs crates/scaena-narrato/src/look.rs crates/scaena-narrato/src/lib.rs` → `crates/scaena-ops/src/narrative.rs:834:        for p in raster.rgba.as_chunks_mut::<4>().0 {` (a pixel loop, not a batch of slides). `generation.look_request(key, &seen, &drawn)` (`narrative.rs:465`) takes every drawn slide in one request.
- **Do:** Cap one look request at a dozen slides and ask in rounds, `look_request` taking a slice of `seen` and `drawn` and `look_answer` folding each round's changes into one `Vec<Change>`; the CLI's `--look-answer FILE` keeps taking one file.
- **Why:** A "41+ slides" deck sends 41 JPEGs in one request, which is slow to upload on the iPad and is where a provider's per-request image limit bites first; the deck is then kept unlooked (`made.unlooked`) and the whole pass is lost.
- **Effort:** S · **Impact:** M

## Functionality

### FUNC-1. Show the outline before the slides are written, and let the user change it
- **Location:** `web/src/generate.ts:182-194`
- **Proof:**
  ```ts
        for (let asked = generation.request(job.key); asked; asked = generation.request(job.key)) {
          const step = generation.step();
          hear(step === "arc" ? { kind: "choosing" } : { kind: "writing", slides: generation.target() });
          const reply = await send(JSON.parse(asked));
          const at = JSON.parse(generation.answer(reply.status, reply.statusText, reply.body)) as {
            arc?: string;
            fallback?: string | null;
            written?: number | null;
            truncated?: boolean | null;
          };
          if (step === "arc") hear({ kind: "chose", arc: at.arc ?? "", fallback: at.fallback ?? undefined });
          else hear({ kind: "wrote", slides: at.written ?? 0, truncated: at.truncated ?? false });
        }
  ```
- **Verified:** `grep -n 'outline\|spine\|beats' web/src/prompt.ts web/src/generate.ts` → `# (no matches)`. `grep -n 'pub fn plan\|pub fn format_plan' crates/scaena-narrato/src/beats.rs` → `70:pub fn plan(arc: &NarrativeArc, target: usize) -> Vec<(usize, usize, usize)>` / `83:pub fn format_plan(arc: &NarrativeArc, plan: &[(usize, usize, usize)]) -> String`. `grep -n 'pub fn \(arc\|target\|choose_arc\|step\)' crates/scaena-generator/src/lib.rs` → `285:    pub fn choose_arc(&mut self, name: Option<String>) {` / `290:    pub fn step(&self) -> String {` / `295:    pub fn target(&self) -> usize {`. `grep -n 'pub fn arc(' crates/scaena-narrato/src/lib.rs` → `182:    pub fn arc(&self) -> Option<&ArcChoice> {`.
- **Do:** Split the loop at the arc: after `chose`, pause and show an outline pane in the dialog `(new)` — the arc with its reasoning (`Generation::arc`), the beat plan with slide counts per beat (`beats::plan`, `beats::format_plan`, exposed on the generator's `Generation` as `plan()` `(new)`), and a field for a one-sentence big idea `(new)` — with Change the arc (which calls `choose_arc`) and Write the slides. Carry the big idea and any edits into the slides' call through Scaena's own `SLOT_SHAPES` block `(new lines)`, never into Narrato's text (ADR-0026). Do the same in `PromptWindow.swift` and the CLI (`--outline` prints the plan and stops; `--arc`, `--idea` `(new)` resume).
- **Why:** This is the Gamma flow and the single largest narrative-quality lever: today the arc is chosen silently and the user first sees the story as finished slides; an outline reviewed for its throughline before 45 slides are paid for is what *Resonate* starts from.
- **Effort:** L · **Impact:** L

### FUNC-2. Let the assistant read the deck's health, and tell it the deck's story
- **Location:** `crates/scaena-session/src/assistant.rs:29-44`
- **Proof:**
  ```rust
  pub const TOOLS: &[&str] = &[
      "deck_read",
      "deck_patch",
      "deck_find",
      "deck_lint",
      "deck_inspect",
      "deck_diff",
      "deck_render",
      "spine_read",
      "spine_update",
      "data_attach",
      "data_edit",
      "theme_edit",
      "icon_find",
      "map_fetch",
  ];
  ```
- **Verified:** `grep -n 'deck_health\|health' crates/scaena-session/src/assistant.rs | head` → `# (no matches)`. The MCP schema exists: `grep -n '"deck_health"' crates/scaena-chat/src/tools.rs` → `18:    ("deck_health", include_str!("../../../docs/schema/mcp/deck_health.json")),`; the panels already have words for it: `web/src/assistant/panel.ts:61:  deck_health: ["Reading the deck's story…", …]`, `apps/mac/ScaenaKit/Sources/ScaenaKit/Assistant.swift:276:        "deck_health": ("Reading the deck's story…", …)`. The prompt's facts: `sed -n 31,52p crates/scaena-chat/src/prompt.rs` → `pub struct Facts { title, states, formats, theme }` and `Facts::of` reads only `/meta/title` and `theme`; `grep -n 'narrative\|brief\|arc\|claim' crates/scaena-chat/src/prompt.rs` → `179:Say briefly what you do and what you found, in the user's language. Lint to zero errors before you say a deck is done, and look at what you changed.` (the one hit is "briefly"; no brief, arc, or claim reaches the prompt). The health operation is pure on the deck: `crates/scaena-ops/src/health.rs:396: pub fn health(deck: &Deck) -> Result<Health, OpsError>`.
- **Do:** Add `"deck_health"` to `TOOLS` and a match arm beside `spine_read` (`assistant.rs:449`) that returns `scaena_ops::health::health(&self.deck())`; the FFI and the browser inherit it through `scaena_chat::tools(TOOLS)`. Extend `Facts` `(new fields)` with `meta.narrative.brief` (topic, audience, goal, length), `meta.narrative.arc`, and each beat's claim in spine order, and add one sentence to `system_of` (`prompt.rs:179`): "Before you say a deck is done, read its health with deck_health and leave each slide saying its beat's claim." Mirror the sentence in `web/src/assistant/prompt.ts:35`, which the recorded exchanges hold to the Rust (`tests/assistant/exchanges.json`).
- **Why:** The Story tab's Ask the assistant (`web/src/story.ts:198`) sends the model a question about a score it cannot read, and the model's only quality bar today is "lint to zero errors"; with the health tool and the brief in hand, every story question is answered against the same measures the user sees.
- **Effort:** S · **Impact:** L

### FUNC-3. Keep each source's provenance, and let a beat's evidence name it
- **Location:** `crates/scaena-ops/src/narrative.rs:1302-1303`
- **Proof:**
  ```rust
          if let Some(data) = &made.data {
              entry["evidence"] = json!([format!("@{data}")]);
          }
          entry["media"] = json!({ "narrato": media(slide) });
          sections.last_mut().expect("opened above")["beats"].as_array_mut().expect("an array").push(entry);
  ```
- **Verified:** `grep -n 'evidence' crates/scaena-ops/src/narrative.rs | head` → `3:`, `1303:`, `3072:` (a `"semantic": "evidence"` node), `3918`, `3922` (tests) — no other write of `evidence`. ADR-0026's own deferral: `grep -n 'citation' docs/adr/0026-decks-generated-by-narrato.md` → `86:- **−** A source's original file, and where it came from, are not yet kept in the bundle. Keeping them, so that a claim can be checked against them later, is a decision for when citations land.` The source record exists but goes no further than the report: `grep -n 'pub struct Given' -A 5 crates/scaena-sources/src/lib.rs` → `pub struct Given { pub name: String, pub kind: &'static str, pub bytes: usize, pub truncated: bool, }`; `supportingFacts` lands in `media.narrato.fact` only (`narrative.rs:1470`). W423 already skips URLs (`docs/SPEC.md:1470`).
- **Do:** Write `grounding.given` into `meta.narrative.sources` `(new)` — name, kind, URL where it came from one, bytes, truncated — at `narrative.rs:1311-1323` beside the brief. Ask, in Scaena's own `SLOT_SHAPES` block `(new line)`, for a `sourceRef` per slide naming the source its facts came from, read in `answer::normalize`; map it into `beat.evidence` as the source's URL, or as `source:NAME` `(new ref form)` for a file, which W423 (`crates/scaena-core/src/lint/narrative.rs:203`) learns to pass as it passes URLs. Show the names in the Story row and `scaena files`.
- **Why:** A grounded deck today cannot say where any claim came from: the user hands over a 10-K and three PDFs and gets slides with no trace back, which is the first thing a reviewer asks for and what *Resonate*'s evidence-under-claim structure needs.
- **Effort:** M · **Impact:** L

### FUNC-4. Check whether each slide's words carry its beat's claim, with the user's model
- **Location:** `crates/scaena-core/src/writing.rs:13-39`
- **Proof:**
  ```rust
  pub enum Task {
      /// A text's spelling, grammar, and punctuation put right, and nothing else changed.
      Proofread { text: String },
      /// A text in fewer words, its meaning kept.
      Tighten { text: String },
      /// A text said more clearly and directly.
      Rewrite { text: String },
      /// Notes for the speaker of a slide: from its words as it reads, a line for each part, and
      /// the point its beat makes, where the spine gives one.
      Notes {
          slide: String,
          #[serde(default, skip_serializing_if = "Option::is_none")]
          claim: Option<String>,
      },
      /// What a finding of lint means for the slide, and what could be done.
      Explain(Explained),
      /// A picture described for someone who cannot see it, as its description (`alt`): from the
      /// picture itself, which the model is shown beside the prompt, and the slide's words around
      /// it (PLAN 2.119). `text` is the description it has, if any.
      Describe {
          slide: String,
          #[serde(default, skip_serializing_if = "Option::is_none")]
          text: Option<String>,
      },
  }
  ```
- **Verified:** `grep -n 'Claim' crates/scaena-core/src/writing.rs` → `# (no matches)`. Health's finding kinds (`crates/scaena-ops/src/health.rs:111-126`) are `Title, Empty, Turn, Arc, Intensity, Tone, Drift` — none reads a claim against its slide. SPEC defers it to the user's model: `grep -n 'question for the user' docs/SPEC.md` → `1486:… Whether a slide's words say what its beat claims is a question for the user's model (BYOK, §11), not a rule.` The finding is still open from the authorability spike: `docs/examples/agent-authorability.md:172: | 11 | A beat's claim and its claim node's text can drift silently: W420 checks only that a claim node exists. | (c) | A narrative check judged by the user's model (BYOK, SPEC §11) …`.
- **Do:** Add `Task::Claims { slides: Vec<Claimed> }` `(new)` to `scaena_core::writing`, each `Claimed` a beat's claim with the words its last state shows, whose instructions ask for one line per slide: carries it, drifts from it (and how), or contradicts it. Offer it as Check the claims in the Story tab (`web/src/story.ts`), `StoryPanel.swift`, and the palette, its answer shown as rows with Rewrite the headline (a `replace_text` of the claim node, as `Player.written` takes one) and Change the claim (a `spine_update`). Run it through `Writer`/`write()` as the other tasks run, on the device where Foundation Models are present.
- **Why:** This is the narrative check SPEC says only a model can make and nothing yet makes: after a fit or look rewrite the headline and the beat's claim already differ by design (`narrative.rs` test at `:4083`), and a hand-edited deck drifts the same way with no warning.
- **Effort:** M · **Impact:** L

### FUNC-5. Give agents the operations the editors have: start a slide, set a background, build from written slides
- **Location:** `crates/scaena-ops/src/states.rs:110`, `crates/scaena-ops/src/backgrounds.rs:252`
- **Proof:**
  ```rust
  pub fn starting(deck: &Deck, theme: &Theme, shown: &str, layout: Option<&str>) -> Result<AddedState, OpsError> {
  ```
  ```rust
  pub fn background(b: &Bundle, state: &str, preset: Option<&str>) -> Result<Patched, OpsError> {
  ```
- **Verified:** `grep -n 'starting\|background' crates/scaena-mcp/src/lib.rs | head -3` → `# (no matches)`. `grep -rn 'deck_generate' crates/ docs/schema/mcp/` → `# (no matches)`. The CLI's no-call path exists: `crates/scaena-cli/src/generate.rs:520-524` → `Some(path) => { generation.choose_arc(a.arc.as_deref()); … generation.written(text, false) … }`. The MCP tool list is held exactly by `crates/scaena-mcp/tests/mcp.rs:88-110` (19 tools).
- **Do:** Add three MCP tools `(new)`, each a thin call in `crates/scaena-mcp/src/lib.rs` with a schema in `docs/schema/mcp/`: `state_start` over `states::starting` (a slide added after `shown` in a named layout, as Add Slide adds one, the theme's layouts listed by `inspect --layouts`); `background_set` over `backgrounds::background`; and `deck_generate` over `narrative::generate` with `slides` given, the `--slides` path, so an agent that writes Narrato's slide JSON gets the arc, the layouts, the builds, and the `media.narrato` fields `deck_health` reads. Update `tests/mcp.rs`'s list and the `author-deck` skill's table.
- **Why:** An agent today hand-builds every patch that Add Slide and Backgrounds make in one gesture, and a deck it writes itself carries none of the metadata health scores, so `deck_health` grades it against a synthetic curve (`crates/scaena-narrato/src/health/arc.rs:137-141`).
- **Effort:** M · **Impact:** M

## Stability

### STAB-1. The Mac's and the iPad's presenter view shows no notes for a generated deck
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/Presenting.swift:546-548`
- **Proof:**
  ```swift
      /// `state`'s notes, for the presenter.
      func notes(_ state: String) -> String {
          let choices = try? editor.session.stateChoices(state: state)
          return choices?.fields.first { $0.prop == "notes" }?.value?.string ?? ""
      }
  ```
- **Verified:** `grep -rn 'spine\b\|beats' apps/mac/ScaenaKit/Sources/Scaena/Presenting.swift apps/mac/ScaenaKit/Sources/ScaenaKit/Show.swift | head -3` → `# (no matches)`. The generator writes notes on the beat alone: `sed -n 1299,1300p crates/scaena-ops/src/narrative.rs` → `if !slide.speaker_notes.is_empty() { entry["notes"] = json!(slide.speaker_notes); }` (where `entry` is the beat, `:1298`). The browser falls back to the beat: `sed -n 1320,1326p web/src/worker.ts` → `/** Each state's notes, by its id: its own, else those of the first beat that names it. */ … return new Map(files.states.map((s) => [s.id, s.notes ?? beats.get(s.id) ?? ""]));`. `stateChoices`' `notes` field is the state's own (`crates/scaena-core/src/choices.rs:776: _ => own.notes.clone().map(Value::String)`).
- **Do:** Give the session one `notes(state)` `(new)` that answers as `worker.ts:1321-1326` does — the state's own notes, else the first beat's that names it — and call it from `Presenting.notes(_:)`, from `Rehearsing`, and from the browser's worker in place of its private copy; record the rule in SPEC §2.6 (`agent-authorability.md:176`, finding 18, is still open on exactly this).
- **Why:** Every deck Narrato writes carries speaker notes, and on the Mac and the iPad the presenter reads "No notes." on every slide of it.
- **Effort:** S · **Impact:** L

### STAB-2. A crafted cell reference or chart point index allocates gigabytes while reading a source
- **Location:** `crates/scaena-sources/src/xlsx.rs:107`, `:124-127`
- **Proof:**
  ```rust
      Some(letters.to_ascii_uppercase().bytes().fold(0usize, |n, b| n * 26 + (b - b'A' + 1) as usize) - 1)
  ```
  ```rust
              if at >= cells.len() {
                  cells.resize(at + 1, String::new());
              }
              cells[at] = text;
  ```
- **Verified:** `grep -n 'MOST_COLUMNS\|checked_mul\|checked_add\|saturating' crates/scaena-sources/src/xlsx.rs crates/scaena-sources/src/pptx.rs` → `# (no matches)`. The same shape in the chart reader, `sed -n 182,187p crates/scaena-sources/src/pptx.rs` → `let idx = pt.attr("idx").map_or(Some(0), |i| i.parse().ok())?; … let Some(most) = by_index.iter().map(|(i, _)| *i).max() else { return Vec::new() }; let mut out = vec![String::new(); most + 1];`.
- **Do:** Cap a column at Excel's own `XFD` (16,384) in `column()` with `checked_mul`/`checked_add`, returning `None` past it, and cap a chart point's `idx` at a few thousand in `pptx.rs:182`, skipping the point beyond; a reference that overflows is a malformed file, said as `SourceError::Unreadable`, not an allocation.
- **Why:** A sheet with `r="ZZZZZZ1"` (about 3×10⁸ columns) or a chart cache with `idx="9999999999"` aborts the CLI, the Mac app, or the browser's worker with an out-of-memory before any call is made; both are one attribute in a file the user dropped in.
- **Effort:** S · **Impact:** M

### STAB-3. The optional look pass can lose a deck that already validated
- **Location:** `crates/scaena-ops/src/narrative.rs:464`, `:477`
- **Proof:**
  ```rust
          (Looking::Asked, Some(key)) => {
              let drawn = drawn(&write, LOOK_WIDTH)?;
  ```
  ```rust
          Some(Ok(changes)) => {
              let shown = Shown { narrative: &narrative, theme: &model, pictures: &pictures, assets: &assets };
              (made, write) = looked(shown, changes, (made, write), theme_file, &theme, font)?;
              say(Progress::Looked { slides: made.looked.len() });
          }
          Some(Err(why)) => made.unlooked = Some(why.to_string()),
  ```
- **Verified:** The failed-answer path is soft (`:480` sets `made.unlooked`), but `drawn(...)?` and `looked(...)?` return `Err` from `generate`: `grep -n 'unlooked' crates/scaena-ops/src/narrative.rs | head -4` → `86:    pub unlooked: Option<String>,` / `480:        Some(Err(why)) => made.unlooked = Some(why.to_string()),` / `690:        before.0.unlooked = Some(why.to_string());` / `768:        made.unlooked = Some(as_made(refused));` — every soft path is a failed or refused *answer*; neither `drawn` nor `looked` is wrapped.
- **Do:** Treat a draw, JPEG, or rebuild error in the look step as `made.unlooked = Some(why)` and go on with the validated `write`, as a failed answer already does; same for `drawn` in the browser's `look()` (`web/src/generate.ts:432-478`) and the FFI's `make` step (`crates/scaena-ffi/src/generate.rs:457-467`).
- **Why:** A painter or JPEG failure on one state (a damaged font, a shader past `MAX_PIXELS`) throws away a whole deck the model has already written and paid for, where the step was only ever a polish.
- **Effort:** S · **Impact:** M

### STAB-4. A Foundation Models failure ends a writing tool with no fallback to the user's own model
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/OnDevice.swift:42-50`, `:88-89`
- **Proof:**
  ```swift
              let session = LanguageModelSession(instructions: instructions)
              do {
                  let response = try await session.respond(to: prompt)
                  return response.content.trimmingCharacters(in: .whitespacesAndNewlines)
              } catch let error as CancellationError {
                  throw error
              } catch {
                  throw Unanswered()
              }
  ```
  ```swift
          if OnDevice.available, picture == nil {
              return try await OnDevice.respond(instructions: instructions, prompt: prompt)
          }
          guard let who = byKey() else { throw OnDevice.Unavailable() }
  ```
- **Verified:** `grep -rn 'exceededContextWindowSize\|guardrailViolation\|unsupportedLanguage' apps/mac/ScaenaKit/Sources --include=*.swift | head -3` → `# (no matches)`. `byKey()` (`OnDevice.swift:63`) already resolves the provider, model, and key the assistant uses.
- **Do:** In `respond(instructions:prompt:picture:)`, catch `LanguageModelSession.GenerationError` by case: on `exceededContextWindowSize`, `guardrailViolation`, `unsupportedLanguageOrLocale`, and `assetsUnavailable`, fall through to `byKey()`'s cloud model when one is kept, and say which answered in `whence`; keep `Unanswered()` only when no key is kept. Note the reason in the sheet ("Apple Intelligence declined; asked Claude instead").
- **Why:** A long slide's notes or a rewrite in a language the on-device model does not take ends in "couldn't answer that" even though the user has a Claude or GPT key in the Keychain beside it; the narrative tools on the Mac are only as good as the 4,096-token on-device model when they could be as good as the user's model.
- **Effort:** S · **Impact:** M

### STAB-5. An arc the model names imperfectly becomes "Hero's Journey" without a word
- **Location:** `crates/scaena-narrato/src/answer.rs:97-100`
- **Proof:**
  ```rust
      let selection = match catalog.iter().find(|a| a.arc == selection) {
          Some(a) => a.arc.clone(),
          None => {
              let lc = selection.to_lowercase();
              catalog.iter().find(|a| a.arc.to_lowercase() == lc).unwrap_or(&catalog[0]).arc.clone()
          }
      };
  ```
- **Verified:** `grep -n 'fallback' crates/scaena-narrato/src/lib.rs | head` → `151:    fallback: Option<String>,` / `163:            fallback: None,` / `187:    pub fn fallback(&self) -> Option<&str> {` / `377:                        self.fallback = Some(why);` — `fallback` is set only when the arc call fails outright (`:377`), never on this silent substitution. `grep -n 'heuristic' crates/scaena-narrato/src/answer.rs` → `6:use crate::heuristic::{ArcChoice, Candidate};` (types only); `grep -n 'heuristic::choose' crates/scaena-narrato/src/answer.rs` → `# (no matches)`.
- **Do:** Match the name loosely (hyphen and en dash, straight and curly apostrophe, a prefix or a contained catalog name) before giving up, and when nothing matches, fall back to `heuristic::choose(&self.brief)` (`crates/scaena-narrato/src/heuristic.rs`) and set `self.fallback` to "the model named `X`, not an arc of the catalog", so the dialog's `chose` line says so as it does for a failed call.
- **Why:** "Problem-Solution" with a hyphen, or "Hero's Journey for Change" with a straight apostrophe, silently becomes the first arc of the catalog, and every slide is then written and scored against a structure nobody chose.
- **Effort:** S · **Impact:** M

## Reliability

### REL-1. Retry the slides call when the provider is busy, as the picture calls already are
- **Location:** `crates/scaena-ops/src/narrative.rs:427-428`
- **Proof:**
  ```rust
          let reply = transport.send(&request);
          generation.answer(reply.status, &reply.status_text, &reply.body).map_err(|e| OpsError::new(e.to_string()))?;
  ```
- **Verified:** `grep -n 'TRIES\|fn send\|retry\|again' crates/scaena-ops/src/narrative.rs | head` → `136:const TRIES: usize = 6;` / `141:/// once, each asked again while its provider is busy, up to six times in all, waiting as` (pictures only); the transport can wait: `116:    fn pause(&mut self, _seconds: f64) {}` and `Reply` carries `after: Option<String>` (`:101`). Narrato's own contract: `sed -n 362,363p crates/scaena-narrato/src/lib.rs` → `/// selection that fails in any way falls back to the heuristic, saying why; the slides that` / `/// fail are an error, and the step stands to be asked again.`
- **Do:** In `generate`'s loop, on status 0, 408, 429, or 5xx (and Anthropic's 529), `transport.pause` by `reply.after` or a doubling wait and ask the same step again up to `TRIES`, saying `Progress::Retrying { step, wait }` `(new)`; return the error only after the last try. The browser's `send()` (`web/src/generate.ts:53-56`) and the Mac's `Generator.send` go through the same loop since it lives in `generate`.
- **Why:** One busy answer on the most expensive call ends the whole run, after the research that may have taken minutes; pictures already survive six such answers.
- **Effort:** S · **Impact:** L

### REL-2. Keep the slides' answer and the research as they arrive, so a failed run resumes
- **Location:** `crates/scaena-cli/src/generate.rs:512-514`, `:575`
- **Proof:**
  ```rust
      if a.research {
          let key = key.as_deref().context("deep research asks a model: --slides makes no call")?;
          read.push(research(&brief, provider, &model, a.base.as_deref(), key, &agent, json)?);
      }
  ```
  ```rust
          scaena_ops::create::make(dir, write)?;
  ```
- **Verified:** `grep -n 'write_all\|fs::write\|create::make\|save' crates/scaena-cli/src/generate.rs | head` → `191:        let _ = std::fs::write(kept, &bytes);` (the script font cache) / `575:        scaena_ops::create::make(dir, write)?;` — nothing of the run is written before the deck is complete. The resume path exists: `sed -n 520,524p crates/scaena-cli/src/generate.rs` → `Some(path) => { generation.choose_arc(a.arc.as_deref()); let text = written.as_deref().unwrap_or_default(); generation.written(text, false)… }`.
- **Do:** As soon as `Progress::Wrote` fires, write the slides' answer text to `DIR/.generating/slides.json` `(new)` and the research report to `DIR/.generating/research.md` `(new)`, and on any later failure print the command that resumes: `scaena generate DIR … --slides DIR/.generating/slides.json --source DIR/.generating/research.md`. Remove the folder when `create::make` succeeds. In the browser, keep the same two texts in the dialog's worker and offer Try again from the slides written `(new)` on the failed line.
- **Why:** The two costliest steps, deep research (up to thirty minutes of polling) and the 64k-token slides answer, are discarded by any failure after them, and the user pays for both again.
- **Effort:** S · **Impact:** M

### REL-3. Give the assistants a retry on a busy provider and a timeout on a silent one
- **Location:** `web/src/assistant/converse.ts:68-76`
- **Proof:**
  ```ts
      } catch (e) {
        if (stopped()) return o.emit({ kind: "done", stop: "stopped" });
        // A refusal, or no way to the provider, in a person's words (PLAN 3.18), as the Mac's
        // assistant says it: never the provider's own, nor its status.
        return o.emit({ kind: "failed", message: unasked(o.provider.name, e) });
      }
  ```
- **Verified:** `grep -n 'stream\|retry\|Retry\|429\|529\|overloaded\|backoff' web/src/assistant/providers.ts | head` → `# (no matches)`; `grep -n 'AbortSignal.timeout\|setTimeout' web/src/assistant/converse.ts web/src/assistant/index.ts web/src/assistant/panel.ts web/src/assistant/providers.ts web/src/assistant/tools.ts web/src/assistant/keys.ts web/src/assistant/prompt.ts` → `web/src/assistant/panel.ts:204:  let typing: ReturnType<typeof setTimeout> | undefined;` / `web/src/assistant/panel.ts:208:    typing = setTimeout(() => {` (the key field's debounce; no deadline on a turn). The classification exists: `sed -n 195,196p web/src/words.ts` → `if (status === 408 || status === 429) return "busy"; if (status >= 500) return "down";`. The Mac is the same: `sed -n 126,131p apps/mac/ScaenaKit/Sources/ScaenaKit/Assistant.swift` → `next = try chat.answer(status: status, body: body) } catch { … add(.failed(Why.answered(chat.provider, status: status, body: body))) return }`.
- **Do:** In `converse`, when `unanswered()` says `busy` or `down`, wait by `Retry-After` or 2, 4, 8 s and ask the same turn again up to three times, emitting a `waiting` event the panel shows; give each `provider.turn` a per-turn deadline with `AbortSignal.any([o.signal, AbortSignal.timeout(120_000)])`. On the Mac, the same loop around `chat.request`/`chat.answer` in `Assistant.swift:120-131`, using `URLRequest.timeoutInterval` for the deadline.
- **Why:** A single 429 or 529 in the middle of a 20-round edit ends the question with "busy, try again" and leaves the deck half-edited, and a hung connection shows "thinking" forever until the user finds Stop.
- **Effort:** M · **Impact:** M

### REL-4. A failed image-model listing should drop the pictures, not the deck, on the CLI and the Mac
- **Location:** `crates/scaena-cli/src/generate.rs:290-291`
- **Proof:**
  ```rust
              let reply = fetch(agent, &provider.models(&key, a.picture_base.as_deref(), false));
              let listed =
                  pictures::models(provider, reply.status, &reply.status_text, &reply.body).map_err(|e| anyhow!(e))?;
  ```
- **Verified:** The Mac does the same: `sed -n 480p apps/mac/ScaenaKit/Sources/ScaenaKit/Generator.swift` → `guard (200..<300).contains(listed.status) else { throw unlisted() }`, thrown from `paint`, which `generate` calls with `try await paint(painter, theme: theme, hear: hear)` (`:619`). The browser degrades: `sed -n 272,275p web/src/generate.ts` → `const unlisted = () => hear({ kind: "painted", pictures: 0, unmade: [answered(named, listed.status, listed.body)] }); if (listed.status < 200 || listed.status >= 300) { unlisted(); return undefined; }`.
- **Do:** In `painter()` (`generate.rs:268-294`) and `Generator.paint` (`Generator.swift:474-486`), turn a listing that fails or lists no image model into `Progress::Painted { pictures: 0, unmade: [why] }` and go on with no pictures, as the browser does, keeping the hard error only for `--pictures` named explicitly with no key. Print `made.unpictured` in the CLI's final report beside `pictures:` (`generate.rs:606-608`).
- **Why:** A busy or refused image key aborts a generation whose narrative is already written, where the deck without pictures is what the user would have taken; the three clients should fail the same way.
- **Effort:** S · **Impact:** M

### REL-5. Protect an iPad generation from suspension
- **Location:** `apps/mac/ScaenaKit/Sources/Scaena/PromptWindow.swift:540-541`
- **Proof:**
  ```swift
          running = Task { @MainActor in
              defer { running = nil }
              let generator = Generator()
              do {
  ```
- **Verified:** `grep -rn 'beginBackgroundTask\|scenePhase\|backgroundTask\|BGTask' apps/mac/ScaenaKit/Sources apps/ipad --include=*.swift | head -5` → `# (no matches)`.
- **Do:** On the iPad, wrap the generation `Task` in `UIApplication.shared.beginBackgroundTask(withName:expirationHandler:)` ended in the `defer`, and on expiry cancel the task and show "The deck wasn't finished: iPadOS stopped Scaena in the background. Keep it in front while a deck is made." Pair it with REL-2's kept slides so what was written survives.
- **Why:** A generation takes one to three minutes, and with research far longer; a user who switches to Mail while it runs loses the whole run with no message that says why.
- **Effort:** S · **Impact:** M

## Security

### SEC-1. `deck_render` and `deck_export` write `out` to any path the server can
- **Location:** `crates/scaena-mcp/src/lib.rs:734-735`
- **Proof:**
  ```rust
          let frame = blocking(move || {
              let frame = scaena_ops::render::render(Path::new(&a.bundle), &req)?;
              if let Some(out) = &out {
                  std::fs::write(out, &frame.png).map_err(|e| OpsError::new(format!("writing {out}: {e}")))?;
              }
              Ok(frame)
          })
  ```
- **Verified:** `grep -n 'canonicalize\|starts_with(cwd\|confine\|within' crates/scaena-mcp/src/lib.rs` → `# (no matches)`; `deck_export`'s `out` (`:104`, `:261`) is passed to `scaena_ops::export` the same way.
- **Do:** Over MCP, drop `out` from `deck_render` (the PNG is returned as an image already) and confine `deck_export`'s `out` to a path under the bundle's parent directory or the server's working directory, refused otherwise with a message that says where it may write `(new check)`; the CLI keeps writing where it is told.
- **Why:** An agent reading a deck, a data file, or a fetched page can be told to "export to ~/.ssh/authorized_keys" and the server does it with the user's permissions; a local stdio server with no auth is exactly where a prompt-injected path lands.
- **Effort:** S · **Impact:** M

### SEC-2. A source link may name any address: plain `http`, loopback, link-local, a private host
- **Location:** `crates/scaena-sources/src/fetch.rs:122-126`
- **Proof:**
  ```rust
      if lower.starts_with("https://") || lower.starts_with("http://") {
          if let Some((kind, id)) = google(spec) {
              return Ok(Some(Remote::Google { kind, id }));
          }
          return Ok(Some(Remote::Web(spec.to_string())));
      }
  ```
- **Verified:** `grep -n 'localhost\|127\.0\|169\.254\|private\|loopback\|allowed\|is_global' crates/scaena-sources/src/fetch.rs crates/scaena-sources/src/sec.rs | head` → only SEC has one: `crates/scaena-sources/src/fetch.rs:454: if !sec::allowed(&next.url) {` / `crates/scaena-sources/src/sec.rs:20:pub fn allowed(url: &str) -> bool {` / `sec.rs:505: assert!(!allowed("https://169.254.169.254/latest"));`. Web links have no such check.
- **Do:** Before `Remote::Web`, refuse `http://`, a host that is a literal loopback, link-local, or private IP, `localhost`, or a single-label name, with "a web source is a public https address", and keep the check on each redirect the plan follows (`fetch.rs:459`); the browser's CORS already stops most of this, so the check matters for the CLI, the Mac, and the iPad.
- **Why:** What a link fetches is sent to a third-party model as source material: a brief that says "ground this in http://169.254.169.254/latest/meta-data/" or an intranet wiki page is read and uploaded with the user's key, and the user sees only "Read page".
- **Effort:** S · **Impact:** M

### SEC-3. `data_attach` reads any file the user can and copies it into the bundle
- **Location:** `crates/scaena-ops/src/create.rs:56`
- **Proof:**
  ```rust
  pub fn attach(b: &Bundle, req: &Attach) -> Result<Attached, OpsError> {
      let bytes = std::fs::read(&req.file).with_context(|| format!("reading {}", req.file.display()))?;
      let (attached, deck) = attaching(b, req, bytes)?;
  ```
- **Verified:** `sed -n 135,136p crates/scaena-ops/src/create.rs` → `let name = req.file.file_name().and_then(|n| n.to_str()).context("the data file has no name")?; let path = format!("data/{name}");` — the destination is confined to `data/`, the source is not. `grep -n 'starts_with\|canonicalize' crates/scaena-ops/src/create.rs` → `# (no matches)`.
- **Do:** Over MCP, confine `Attach.file` to the server's working directory and the bundle (the same helper as SEC-1), and in `scaena_core::data::infer` refuse a file whose first line does not read as a header of the declared type before copying it; the CLI's `new --data` keeps reading anywhere.
- **Why:** A prompt-injected `data_attach` of `~/.aws/credentials` (comma-free lines still infer as one text column) lands the file in the bundle that is then saved, shared, or exported as a `.scaena`.
- **Effort:** S · **Impact:** S

### SEC-4. Keychain items carry no data-protection or accessibility attributes
- **Location:** `apps/mac/ScaenaKit/Sources/ScaenaKit/Keychain.swift:62-67`
- **Proof:**
  ```swift
      private func item(_ account: String) -> [String: Any] {
          [
              kSecClass as String: kSecClassGenericPassword,
              kSecAttrService as String: service,
              kSecAttrAccount as String: account,
          ]
      }
  ```
- **Verified:** `grep -n 'kSecAttrAccessible\|kSecUseDataProtectionKeychain\|kSecAttrSynchronizable' apps/mac/ScaenaKit/Sources/ScaenaKit/Keychain.swift` → `# (no matches)`.
- **Do:** Add `kSecUseDataProtectionKeychain: true` and `kSecAttrAccessible: kSecAttrAccessibleWhenUnlockedThisDeviceOnly` to `item(_:)` and the add dictionary (`:55`), so the Mac uses the data-protection keychain the iPad already uses rather than the legacy file-based one, and the key is readable only while the device is unlocked. Migrate an item found without them once, on first read.
- **Why:** On the Mac the keys sit in the legacy keychain, where an ad-hoc-signed build prompts for access and the item is readable by any process the user runs while the login keychain is open; these are the user's paid provider keys.
- **Effort:** S · **Impact:** M

### SEC-5. Gemini image requests put the model's name in the path unencoded
- **Location:** `crates/scaena-narrato/src/pictures.rs:374`, `:379`
- **Proof:**
  ```rust
              let (path, body) = if model.starts_with("imagen") {
                  (
                      format!("/v1beta/models/{model}:predict"),
  ```
  ```rust
                      format!("/v1beta/models/{model}:generateContent"),
  ```
- **Verified:** `grep -n 'component(' crates/scaena-narrato/src/pictures.rs` → `# (no matches)`; the text path encodes it: `crates/scaena-narrato/src/providers.rs:295: let path = format!("/v1beta/models/{}:generateContent", component(call.model));` and `providers.rs:409: fn component(text: &str) -> String {` (private).
- **Do:** Make `component` `pub(crate)` and use it for the image model in both paths; the model comes from the key's listing or `--picture-model`, so refuse one that is not `[A-Za-z0-9._-]+` at `pictures::models` as well.
- **Why:** A model name with `/`, `?`, or `#` changes the request's path or query, so a listing a provider answers oddly, or a typed `--picture-model`, can call an endpoint the user did not mean with their key.
- **Effort:** S · **Impact:** S

## Usability

### USE-1. Generated speaker notes are markdown, and the presenter view shows it raw
- **Location:** `web/src/player.ts:843-846`
- **Proof:**
  ```ts
          const written = notes[at.index] || "No notes.";
          if (pane.textContent !== written) {
            pane.textContent = written;
            pane.scrollTop = 0;
          }
  ```
- **Verified:** The prompt asks for markdown: `sed -n 469p crates/scaena-narrato/src/prompt.rs | cut -c1-220` → `- \"speakerNotes\": Presenter's talking-point guide. DEFAULT FORM: 4-7 bullet points, each prefixed with \"- \" on its own line, …` and goes on to ask for "a GitHub-flavored markdown table … when the slide content is genuinely grid-shaped". The notes are copied unchanged: `sed -n 1299,1300p crates/scaena-ops/src/narrative.rs` → `if !slide.speaker_notes.is_empty() { entry["notes"] = json!(slide.speaker_notes); }`. `grep -n 'markdown' web/src/player.ts apps/mac/ScaenaKit/Sources/Scaena/Presenting.swift` → `# (no matches)`; the Mac shows `Text(notes.isEmpty ? "No notes." : notes)` (`Presenting.swift:984`), a variable, which SwiftUI does not parse.
- **Do:** Add `scaena_core::notes::plain` `(new)`: `- ` bullets to `•` lines, a GFM table to one line per row ("Option A — cost: low · speed: high"), `**`/`_` stripped; apply it at `narrative.rs:1300` so the deck's notes are the words a presenter reads, and in `scaena_core::reading` for the single file. Keep the markdown nowhere.
- **Why:** The presenter display, where the story is actually told, shows `- we lead with…` and `| h1 | h2 |` pipes on every slide of every generated deck, which is the first thing a user sees when they rehearse.
- **Effort:** S · **Impact:** M

### USE-2. After a deck is made, show its story
- **Location:** `web/src/prompt.ts:317-319`
- **Proof:**
  ```ts
        const deck = await done;
        dialog.close("generated");
        await open({ files: deck.files, name: deck.name });
        tell(made(deck.made));
  ```
- **Verified:** `grep -n 'tab-story\|showTab\|openTab\|story' web/src/editor.ts | head` → only the panel's wiring (`69:import { storyPanel } from "./story";`, `1045: const story = storyPanel(stage, $("#story"), {`); `open(source, asked)`'s second argument only skips the unsaved-changes prompt (`editor.ts:236-237`). The Story tab reads health only once shown: `grep -n 'into.hidden' web/src/story.ts` → `109:    if (into.hidden) return;`. The smoke test clicks the tab itself (`web/generate.mjs:315`).
- **Do:** After `open(...)`, show the Story tab and put the score in the status line: `tell(\`${made(deck.made)} · story ${health.score} of 100\`)`, reading it with `stage.health(editor.source())` as `story.ts:110` does; on the Mac, select the Document tab after `ScaenaDocument(generated:said:)` opens. Keep the arc and the heuristic's fallback (`chose`) in the line too.
- **Why:** Gamma lands the user on the outline and the score of what it made; Scaena lands them on slide 1 with one status line, and the arc graph, the pacing against the brief, and the per-slide fixes stay hidden behind a tab most users never open.
- **Effort:** S · **Impact:** M

### USE-3. Rehearsal shows the slide and not the notes
- **Location:** `web/src/rehearse.ts:96-100`
- **Proof:**
  ```ts
      const now = performance.now();
      // The slide as a person counts it (PLAN 2.99).
      const said = slots[index]?.said ?? `${index + 1} of ${slots.length}`;
      where.textContent = `Rehearsing ${said} · ${clock(spent[index] + now - since)} here · ${clock(now - began)} in all`;
    }
  ```
- **Verified:** `grep -n 'notes' web/src/rehearse.ts apps/mac/ScaenaKit/Sources/Scaena/Rehearsing.swift` → `# (no matches)`. The Mac's view is the canvas and a bar: `sed -n 22,24p apps/mac/ScaenaKit/Sources/Scaena/Rehearsing.swift` → `if let state = rehearsal.state { … ScaenaCanvas(session: editor.session, state: state, revision: editor.revision, playhead: $playhead)`. The notes are already available to both: the worker's `notes(deck)` map (`web/src/worker.ts:1321`) feeds `now.opened.notes` in the player, and `Presenting.notes(_:)` (`Presenting.swift:546`) on the Mac.
- **Do:** Under the rehearsal bar, show the state's notes (STAB-1's one `notes(state)`) in the presenter view's style, and the next slide's name; on the Mac, a `notes` pane under the canvas in `Rehearsing`, as `Presenting`'s presenter window lays it out. Keep each slide's time as before.
- **Why:** A speaker rehearses with their script; rehearsing without the notes times the slides, not the talk, and the timings kept as holds are then wrong for the talk they will give.
- **Effort:** S · **Impact:** M

### USE-4. Tell agents to start from the spine and to read the deck's health
- **Location:** `skills/author-deck/SKILL.md:69-72`
- **Proof:**
  ```markdown
  5. **Lint loop.**
     - Run `scaena lint <bundle> --json` (`deck_lint`). Fix every error, and take warnings seriously: W300 and W301 mean you reached for pixels, so use a role, slot, or token instead.
     - Many errors carry a `fix`. `lint --fix` (`deck_lint` with `fix`) applies the ones lint has checked by laying the state out again.
     - Re-run until clean. Then run the `tighten-copy` skill over the words.
  ```
- **Verified:** `grep -rn 'deck_health\|scaena health' skills/ docs/authoring.md` → `# (no matches)`. The server's first words: `sed -n 1000,1005p crates/scaena-mcp/src/lib.rs` → `"… Make a bundle with deck_create …, attach data with data_attach, edit with deck_patch, check with deck_lint, and look with deck_render. The resources hold the schemas, the lint catalog, the specification by section (scaena://spec is its index), the skills (procedures to follow: scaena://skills/author-deck first), and examples."` — no spine, no health. `crates/scaena-cli/tests/skills.rs` holds the skills to real commands and tools, so a new mention is checked.
- **Do:** Add a step 6 to `author-deck`, "Read the story": `scaena health <bundle> --json` (`deck_health`) after lint, with what each measure means and that a finding's `ask` is a question to answer in the deck; add `spine_read`/`spine_update` and `deck_health` to the server instructions (`lib.rs:1000-1005`); add `generate`, `health`, and `icons` to `docs/authoring.md`'s "More commands" (`:220-227`); and reconcile `tighten-copy`'s "as few words as carry it" with health's balance measure (`crates/scaena-ops/src/health.rs:437-446` marks down fewer than two points a slide), telling the agent where the detail goes (the notes).
- **Why:** The one tool built to judge the story is never named to the agents that write decks, and the two guides they do follow pull in opposite directions on how much a slide should say.
- **Effort:** S · **Impact:** M

### USE-5. Say what was cut from the sources before the paid call, not after
- **Location:** `crates/scaena-cli/src/generate.rs:609-611`
- **Proof:**
  ```rust
          for g in &grounding.given {
              let cut = if g.truncated { ", cut short to fit" } else { "" };
              println!("  grounded in {} ({}, {} KB{cut})", g.name, g.kind, g.bytes.div_ceil(1024));
          }
  ```
- **Verified:** `grounding` is known before any call: `sed -n 516,517p crates/scaena-cli/src/generate.rs` → `let grounding = scaena_ops::narrative::grounding(&read, provider)?; brief.source_content = grounding.content.clone();`; `grep -n 'grounded in\|truncated\|cut short' crates/scaena-cli/src/generate.rs | head` → `2://! prompt, with the user's own key, read from the environment, and grounded in the sources` / `72:    /// What the deck is grounded in, given once for each: a file (Markdown, text, RTF, CSV,` / `307:        Progress::Wrote { slides, truncated: false } => format!("{slides} slides written"),` / `308:        Progress::Wrote { slides, truncated: true } => format!("{slides} slides written, the answer cut short"),` / `363:        let cut = if report.cut { ", cut short" } else { "" };` / `610:            let cut = if g.truncated { ", cut short to fit" } else { "" };` / `611:            println!("  grounded in {} ({}, {} KB{cut})", g.name, g.kind, g.bytes.div_ceil(1024));` — the sources' cut is said only at `:610-611`, after `create::make`. The browser's `read` event carries no cut: `web/src/prompt.ts:20-21` → `case "read": return \`Read ${event.name} (${event.what})\`;`. The pack's cap is 1,000,000 bytes shared by all sources in the order given (`crates/scaena-sources/src/lib.rs:188-203`), the research report last (`generate.rs:512-514`).
- **Do:** Print each `Given` (with its cut) right after `grounding` at `:516`, before the arc call, and refuse to go on when a source got no room at all (`bytes == 0`) unless `--allow-cut`; carry `truncated` on the `read` event so the dialog's line says "Read 10-K (text, cut short to fit)"; pack the research report first, since it was asked for last and costs most.
- **Why:** A user who drops a 10-K, three PDFs, and asks for research learns that the research never fit only after the deck is made, and the grounding block then told the model every slide must be traceable to what it was never shown.
- **Effort:** S · **Impact:** S

## Attractiveness / Sexiness

### ATTR-1. Every generated slide enters by the same dissolve
- **Location:** `crates/scaena-ops/src/narrative.rs:2770-2771`
- **Proof:**
  ```rust
          let mut state = json!({ "id": s.state, "mode": "absolute", "layout": s.layout, "props": props });
          if after {
              state["transition"] = json!({ "duration": "standard", "ease": "standard" });
          }
          if let Some(choreography) = builds(s.theme, built, &charts) {
              state["choreography"] = choreography;
          }
  ```
- **Verified:** `grep -n '"style"\|push\|wipe\|reveal' crates/scaena-ops/src/narrative.rs | grep -i transit` → `# (no matches)`. The slide transitions exist in the model: `grep -n 'pub enum TransitionStyle' crates/scaena-core/src/model/states.rs` → `44:pub enum TransitionStyle {` (Dissolve, Push, Cover, Reveal, Wipe; PLAN 2.111). A section divider is already known at build time: `narrative.rs:1277: if layout.starts_with("section") && !slide.slide_title.is_empty() {`.
- **Do:** In `Builder::slide`, give a section divider, and the first slide after one, `{ "style": "push", "direction": "left", "duration": "standard" }`, the closing slide a slower dissolve (`"duration": "slow"` where the theme names one), and keep Magic Move's plain transition between content slides that share a layout; read `expected_valence` from the slide (`:1468`) so a grave turn takes the slower pace. Keep it within the theme's `motion` tokens, never a literal.
- **Why:** Keynote's presets and *Resonate*'s turning points both mark the movements of a story with motion; a generated deck today moves the same way from the cover to the ask, so the arc the model chose is invisible in play.
- **Effort:** S · **Impact:** M

### ATTR-2. A generated bullet list arrives all at once
- **Location:** `crates/scaena-ops/src/narrative.rs:2690-2695`
- **Proof:**
  ```rust
                  Fill::List(entries, kind) => {
                      let Some(role) = &slot.role else { continue };
                      if entries.is_empty() {
                          continue;
                      }
                      let list = vec![json!({ "kind": kind }); entries.len()];
                      let mut node =
                          json!({ "type": "text", "role": role, "text": entries.join("\n"), "list": list, "at": at });
  ```
- **Verified:** Builds come only from ordinal slots and charts: `sed -n 2757,2758p crates/scaena-ops/src/narrative.rs` → `if let Some((k, _)) = ordinal(name) { match built.iter_mut().find(|(i, _)| *i == k) {` and `grep -n '"enter": "grow"' crates/scaena-ops/src/narrative.rs` → `2802:    items.extend(charts.iter().map(|id| json!({ "target": id, "enter": "grow" })));` (and a test at `:3595`). `grep -n 'by_bullet\|paragraphs' crates/scaena-ops/src/narrative.rs` → `# (no matches)`. The build exists: `crates/scaena-core/src/patch/ops.rs:274: fn by_bullet(` (PLAN 2.129) and `scaena_core::builds::paragraphs` (`crates/scaena-core/src/builds.rs:11`).
- **Do:** For a slide whose only item is a list of three to six entries in the `bullets` layout, apply the theme's By Bullet build as the editor's `apply_preset` with a paragraph-splitting preset does (`patch/ops.rs:274`), one step a bullet, in the deck the builder returns; within `maxBuild` as `builds()` already checks for cards, and off for the reduced-motion reader as every build is.
- **Why:** The commonest generated layout is the one that arrives whole, while cards and charts build one by one (G.17); a list revealed a point at a time is how Keynote and every speaker pace an argument.
- **Effort:** M · **Impact:** M

### ATTR-3. Stream the assistant's answer as it is written
- **Location:** `web/src/assistant/providers.ts:177-183`
- **Proof:**
  ```ts
      const response = await fetch(at(ask.base, this.base, "/v1/messages"), {
        method: "POST",
        headers: { ...anthropicHeaders(ask.key), "content-type": "application/json" },
        body: JSON.stringify(body),
        signal: ask.signal,
      });
      const answer = await json(response);
  ```
- **Verified:** `grep -n 'stream' web/src/assistant/providers.ts crates/scaena-chat/src/providers.rs crates/scaena-narrato/src/providers.rs` → `# (no matches)`. The Mac awaits whole: `sed -n 34,35p apps/mac/ScaenaKit/Sources/ScaenaKit/Assistant.swift` → `let (data, response) = try await URLSession.shared.data(for: request)`.
- **Do:** Send `stream: true` and read the SSE text deltas in each provider's `turn` `(new parser per provider)`, emitting partial `text` events the panel appends to the current entry; keep tool calls until the stream ends. On the Mac, `URLSession.bytes(for:)` with the same parser in `ScaenaChat` `(new)`, since the request builder is shared. Record one streamed exchange per provider in `tests/assistant/exchanges.json`.
- **Why:** With `max_tokens` at 8192 and nothing streamed, a story critique shows "Thinking…" for twenty seconds and lands as a wall of text; Gamma and the browser's agent both feel live because the words arrive as they are written.
- **Effort:** M · **Impact:** M

### ATTR-4. Render the assistant's markdown, in the browser and on the Mac
- **Location:** `web/src/assistant/panel.ts:246-249`, `:266`
- **Proof:**
  ```ts
    const entry = (className: string, text = "") => {
      const li = document.createElement("li");
      li.className = className;
      li.textContent = text;
  ```
  ```ts
        case "text":
          return void entry("said", event.text);
  ```
- **Verified:** `grep -rn 'markdown\|marked\|innerHTML' web/src/assistant/panel.ts | head` → only `270: li.innerHTML = "<details><summary>…"` (a static template for a call). The Mac parses inline syntax alone: `sed -n 330p apps/mac/ScaenaKit/Sources/Scaena/AssistantPanel.swift` → `let inline = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)`.
- **Do:** In the browser, a small safe renderer `(new, web/src/markdown.ts)` for paragraphs, lists, headings, bold, code spans and fences, building elements (never `innerHTML` from model text) and keeping `html()`-escaped text; on the Mac, split the answer into blocks `(new)` and render lists and fences as stacked `Text` views with the inline options kept for each line.
- **Why:** A model's story feedback is a list of points by nature; today it shows `**` and `-` as characters in the browser and `##` and fences on the Mac, which reads as broken next to Gamma's chat.
- **Effort:** S · **Impact:** M

### ATTR-5. Name a deck's sections by what they say, not by the arc's template
- **Location:** `crates/scaena-ops/src/narrative.rs:1284-1286`
- **Proof:**
  ```rust
          if last_beat != Some(beat) {
              let name = names.get(beat).copied().unwrap_or("Section");
              let id = deck.unique(&slug(name, "section"), Taken::Section);
              sections.push(json!({ "id": id, "title": name, "beats": [] }));
              last_beat = Some(beat);
          }
  ```
- **Verified:** `names` is the arc's beat names (`arc::beat_names`, `crates/scaena-narrato/src/health/arc.rs:239`), so a section is titled "Problem", "Solution", or "Section". The divider's own title is at hand two lines up: `sed -n 1277,1279p crates/scaena-ops/src/narrative.rs` → `if layout.starts_with("section") && !slide.slide_title.is_empty() { running = Some(slide.slide_title.clone()); }`. The PDF's outline and the player's state picker read section titles (`SPEC §3.11`, `docs/SPEC.md:1302` `spine`).
- **Do:** Title a section by the first divider slide's `slide_title` where the run of slides has one (the `running` head already computed), else by the first slide's `beat_name` from Narrato (`media.narrato.beatName`), and keep the arc's beat name in `media` for health; fall back to the template name only when both are empty.
- **Why:** The PDF's bookmarks, the player's outline picker, and the Story tab's rows show "Problem · Solution · Closing" for every deck Scaena makes, which reads as a template and hides the talk's own structure.
- **Effort:** S · **Impact:** S

---

## First move

**FUNC-2. Let the assistant read the deck's health, and tell it the deck's story** (from Functionality)

It is a few dozen lines across three files (`crates/scaena-session/src/assistant.rs`, `crates/scaena-chat/src/prompt.rs`, `web/src/assistant/prompt.ts`), the MCP schema and the panels' words already exist, and the recorded exchanges hold the browser and the Mac to the same prompt, so it ships in a day with its test. It is also the item the others lean on: the Story tab's Ask the assistant becomes a question the model can check rather than guess at, FUNC-4's claims check and USE-4's "read the story" step both assume an assistant that can run `deck_health`, and the brief, the arc, and the beats' claims in the system prompt change what every edit the assistant makes is aimed at — from "lint to zero errors" to the story the user asked for. FUNC-1 is the larger lever, but it is a week's work across three clients and a dialog; FUNC-2 moves the assistant's whole behavior for the cost of an afternoon, and nothing else in the list is both that cheap and that central.

## Dropped during verification

- Temperature never reaches Anthropic or OpenAI (`crates/scaena-narrato/src/providers.rs:157`) — cited code does something else: the field is documented as "Gemini's temperature; the others keep their own", a choice that mirrors Vzestup's requests (ADR-0026), not a defect I can show.
- The generation prompt says "five fields" and lists six (`prompt.rs:425`) — not Scaena's to change: Narrato's prompt is kept byte for byte (ADR-0026, decision 34).
- A dropped source is reported as "grounded in … 0 KB" (`crates/scaena-sources/src/lib.rs:200-203`) — already in place: `each.push((name.clone(), 0, true))` sets `truncated`, so the CLI's line reads "0 KB, cut short to fit".
- The GitHub token is sent with every link (`web/src/generate.ts:169-173`) — already in place: `github::headers(reach.token.as_deref())` is attached only on the GitHub steps of the plan (`fetch.rs:244`; test at `:633`), never to a web or Google GET.
- Reuse the editor's last lint as a patch's "before" (ADR-0011's open idea) — cited code may already do it: the session's `deck_patch` arm calls `scaena_ops::patch::editing(&b, &ops, None, validated, judge)` with the editor's validated deck and its own judge (`assistant.rs`, the `deck_patch` arm); I could not show a second lint of the unchanged states.
- Rehearsal's Keep turns a click-driven deck into a self-running one (`web/src/rehearse.ts:197`) — by design: PLAN 2.63 says Keep makes the times the states' holds, and a player advances by holds (SPEC §7.5).
- Health's one-click fix "lowers the bar" (`crates/scaena-ops/src/health.rs:610-613`) — by design: Vzestup's fix sets what a slide declares to what its words read, and the finding's `ask` is the route to rewriting the words; FUNC-2 makes that route work.
- The voice block's closing tag is not neutralized (`prompt.rs:374-388`) — no realistic path: the voice is the user's own text in the user's own prompt; nothing crosses a trust boundary.
- `resource_read` runs outside `run`'s try (`web/src/assistant/index.ts:159`) — mislocated risk: `read()` throws only if `JSON.parse(session.themeText())` fails on JSON the session itself wrote.
- Deeply nested XML overflows the stack (`crates/scaena-sources/src/xml.rs:54-61`) — unverified: the parser is iterative and I did not establish that the recursive walk's depth is reachable from a 64 MiB part; dropped rather than guessed.
- The Keychain is read synchronously in the Inspector's body (`apps/mac/ScaenaKit/Sources/Scaena/Inspector.swift:117`) — unmeasured: `Writer.available` is `OnDevice.available || byKey() != nil`, one `SecItemCopyMatching` per render, and I could not show it costs a frame.
- The Mac's picture-listing failure diverges from the CLI (`Generator.swift:480`) — mislocated as a Mac bug: the CLI aborts the same way (`generate.rs:290-291`); only the browser degrades. Re-filed as REL-4 across both.

## Deferred

- Health never reads a talk's length in minutes (`crates/scaena-ops/src/health.rs:404`, `metrics(&read, None)`), and the brief has no minutes field in any client — real, but it needs a brief field, a `meta.narrative` key, and Rehearse's target in one change; after FUNC-1 settles the brief.
- No semantic patch op for one beat (`add_beat`/`set_beat`); `spine_update` replaces the whole spine (`crates/scaena-mcp/src/lib.rs:979`) — worth doing with FUNC-5's tools.
- Draft Notes overwrites the slide's notes with no "was", and sees neither neighbor slide nor the beat's evidence (`crates/scaena-session/src/writing.rs:181-186`, `crates/scaena-core/src/writing.rs:107-110`) — fold into FUNC-4's task family.
- The persona's `vocabulary`, `goalProfiles`, and `contentHints` in `personas.json` reach no prompt (`crates/scaena-narrato/src/arcs.rs:64-66` keeps them in `other`; `prompt.rs:278-286` uses `attention` alone) — only through Scaena's own block (ADR-0026); decide with FUNC-1.
- `kept()` subtracts one from a slide number unchecked (`crates/scaena-ops/src/narrative.rs:734`) where `changed()` uses `checked_sub` (`:648`) — a one-line fix; ride on STAB-3's push.
- The HTML scanners slice at `end - 1` when a `<` never closes and the text ends in a multi-byte character (`crates/scaena-sources/src/html.rs:53-54`, `sec.rs:259-260`) — one-line `is_char_boundary` guard; ride with STAB-2.
- A failed brief run resets Goal, Length, Modality, and Arc to their defaults (`web/src/prompt.ts:192-193`, `:326` restarts the worker, whose `briefing` runs `fill()`) — small; ride with USE-2.
- The Mac's `Generator.send` turns a cancellation into status 0, "could not be reached" (`Generator.swift:187-189`) — cosmetic once REL-1 retries by status.
- SEC's per-filing cut flag is thrown away (`crates/scaena-sources/src/sec.rs:456`, `let (body, _) = cut(…)`) and a sheet's 20,000-row cut is never recorded (`xlsx.rs:52-53`) — fold into USE-5's reporting.
- Research ends on one transient poll error (`crates/scaena-narrato/src/research.rs:274-276`) — REL-1's retry loop around the client's `send` covers it once research's `Next::Send` goes through the same path.
- Too few slides is an error that discards them all (`crates/scaena-narrato/src/lib.rs:406-409`) — with REL-2's kept answer, the retry becomes "ask for the rest"; later.
- The Story tab's arc graph has no keyboard access (`web/src/story.ts:139-141`; the Mac's is one accessibility element) — WCAG 2.1.1; a small follow-up.
- `docs/examples/agent-run.md:253-255` still lists PLAN 1.33–1.36 as open (all four are `[x]`) — a docs sweep.
- The `deck_inspect` tool description names none of its `layouts`, `choices`, `inserts`, or `arrange` views (`crates/scaena-mcp/src/lib.rs:712-714`) — a description edit; with USE-4.
- The Mac assistant locks its question box while working and never shows token usage (`AssistantPanel.swift:187`; `Assistant.swift:44` counts it) — parity polish after ATTR-3.
- The Mac assistant shows no before/after thumbnails for an edit where the browser does (`AssistantPanel.swift:311` vs `panel.ts:309` `changes()`, PLAN 2.93) — parity; after ATTR-3 and ATTR-4.
- The `stock` operation has no client (`crates/scaena-ops/src/stock.rs:319`; PLAN O.3d waits on Jay) — not this plan's to decide.
- The control-marker scan refuses a whole brief when a fetched page contains `[INST]` or `### Response` (`crates/scaena-narrato/src/containment.rs:63-70`) — Vzestup's behavior by design; a softer "that source is left out" belongs with USE-5.

Run `/burn-down` to execute this plan.
