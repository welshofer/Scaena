# Gate 2

Phase 2's exit criteria (PLAN, "Exit criteria (gate 2)"), each with its evidence so far.

**Three of the four are met:**
- the second and the third in headless Chromium;
- the fourth by Jay, with his own key, on the published site (2026-10-07).

**The first needs a real machine's browsers,** held to the bar Jay set on 2026-10-04. The gate log stays open until it is run: about ten minutes a browser, on the site (PLAN 2.7).

**Phase 3 began before this gate closed, on Jay's call.** On 2026-10-08 Jay said: "Move forward. Next phase." Phase 3 starts at PLAN 3.1 with the first criterion open. It is not waived: the bar stands, and the gate is logged met when Jay's browsers read it.
- **The one reading so far** is 59.6 fps, the worst frame 25 ms, painting in 0.7–0.8 ms. Jay read it on his machine before the steps below were written; which browser it was is not recorded.
- **The risk.** In this repository's 4-core container, the noise slide on the CPU path ran 5.6 fps against a bar of 30. If Firefox's CPU path misses it on Jay's machine, that is work on the web player, which Phase 3 does not wait on.

## 1. A Phase 1 deck plays at 60 fps on WebGPU in Chrome and Safari 26+, and on the CPU fallback in Firefox: needs a real machine

**The bar.** Jay set it on 2026-10-04 ("a high bar for fps: 30, or even 60 if it is remotely achievable"). It holds on the machine you present on, at the size you present at:
- **WebGPU,** in Chrome and in Safari 26+: 60 fps on every cue. The meter reads 58 or more, with no more than 2 frames late.
- **The CPU fallback,** in Firefox where it has no WebGPU, or with `?painter=cpu` anywhere: 60 fps on every cue that draws no shader, and 30 or more on a cue that draws one.

`just fps` holds each cue to it, and names the cues that fall short.

CI cannot say. Headless Chromium composites WebGPU on SwiftShader at about a frame a second.

**The CPU fallback works a shader out on every core (PLAN 2.28).** A full-canvas shader is computed afresh for every frame, and the engine's module has no threads. Now the worker shares a shader's rows with helpers, one fewer than the browser's cores, each holding the engine's module and no deck. The bytes are the same: every torture frame still holds to the goldens through them (`just web-smoke`). A frame also goes onto the canvas with one copy, not two, and an opaque shader is drawn without blending.

On this 4-core container, the trails example at 1920 × 1080, by the same build:

| | before PLAN 2.28 | with it, `?helpers=0` | with it |
|---|---|---|---|
| `cover`, the mesh backdrop | 16 fps, 64 ms a paint | 19.5 fps, 50 ms | 21.5 fps, 44 ms |
| `next`, the noise texture | 4.5 fps, 218 ms | 3.9 fps, 254 ms | 5.6 fps, 165 ms |
| the twelve without a shader | 44–57 fps, 14–20 ms | | 30–56 fps, 10–16 ms |

The first column was measured on another container, before this one; read the last two against each other.

Headless Chromium on four cores holds the helpers to about half speed. It composites every frame in software, on the same cores. In V8 alone (Node, the same module and container), four workers take a frame's shader rows:
- `next`'s noise: from 213 ms to 64 ms;
- `cover`'s mesh: from 30 ms to 10 ms.

`cover`'s whole frame then comes to about 32 ms. The noise is about 400 floating-point operations a pixel, four octaves of simplex noise, and on a slower core it stays the heaviest state: 213 ms on one core here.

The same headless runs hold even the states without a shader to 30–56 fps, while their frames paint in 10–16 ms. The display's frames, not the paint, set that pace: the browser's software compositor shares the four cores with the page. A real machine composites on its GPU. So only a real machine can say whether the bar is met; this container says the paint got faster.

**The player measures itself.** With `?fps` it shows a frame meter while the deck plays, for the run so far:
- frames a second;
- the worst frame;
- how many frames came more than 25 ms after the one before, which is late for a 60 Hz display;
- the mean paint.

The worker times each frame by the display's clock.

**To run it:**
1. Serve the pages: `just web` once, then `just web-dev`. Or use the site (PLAN 2.7), published from `main` first (Actions → site → Run workflow, with "deploy" ticked) so it plays the engine as it is now.
2. In Chrome, open `http://localhost:5173/?bundle=/docs/examples/trails.deck.json&fps`. On the site, open `https://welshofer.github.io/Scaena/index.html?fps`.
3. The status line should say `WebGPU`. Make the window the size you present at, or press F for fullscreen.
4. Go through the deck with →. Each cue with motion fills the meter: the cover's rise, the charts growing, the table, and the diagram. Note the lowest frames a second and the worst frame.
5. Do the same in Safari 26 on the Mac.
6. Do the same in Firefox, which paints with the CPU where it has no WebGPU; `&painter=cpu` forces that anywhere. The status line says `CPU painter`. `&helpers=0` shows what the shader states cost on one core.
7. Record each browser, the machine, and its numbers here and in the gate log.

**Or let a script play it.** `just fps chrome` runs `web/fps.mjs`:
- It serves the repository and opens the player, built by `just web`, in the Chrome installed on the machine, in a window.
- It plays the trails example state by state, each cue and then its hold, as a presentation does.
- It prints what the meter read for each state, against the bar: 58 fps, or 30 where the state draws a shader on the CPU painter. Then it prints the lowest frames a second, the worst frame, the late frames, and the cues that fall short.

`just fps firefox --painter cpu` does the same in Playwright's Firefox, on the CPU fallback the criterion asks about; the first line it prints names the painter. `--helpers N` caps the CPU painter's helpers. Both need Playwright: `npm i -g playwright`, then `npx playwright install firefox` for Firefox, with `NODE_PATH=$(npm root -g)` set. Safari stays by hand, since Playwright's WebKit is not Safari. Headless on this container with the CPU painter (`just fps chromium --headless --painter cpu`), it reads what the table above holds.

## 2. Edit → lint → preview round trip under 200 ms on a 40-state deck: met

`web/editor.mjs` times six edits on B1 (`tests/bench/b1.scaena`, 40 states) in headless Chromium, painting with the CPU. After PLAN 2.19 and 2.20, two runs had median round trips of 36 and 34 ms. The last edit of the second took:
- compile 10 ms;
- the frame 15 ms;
- the lint of the state shown 9 ms.

Before, the median was 85 ms, the lint of the state shown 34 ms of it. PLAN 2.3 measured 114 ms. Before ADR-0004 finding 15, the frame alone took 76 ms.

The lint of every state, once typing stops, takes 85–88 ms, from 1.2 s before PLAN 2.19. It is not part of the round trip.

## 3. A single-file export opens offline from a USB stick: met in headless Chromium

`web/standalone.mjs` exports the torture deck with `scaena export --format html`, then opens the file from its address on disk with the network off:
- it shows its first frame 0.47 s after navigation;
- all 68 golden frames are within SPEC §13.5;
- its content security policy lets nothing load, and nothing asks for more than the file and its worker's blob;
- WebGPU paints a file, and the presenter view opens from one.

See PLAN 2.5.

A USB stick is a disk, so copying the file to one and opening it there is the same test in the world, a minute's work:

```
scaena export docs/examples/trails.deck.json --format html --out /Volumes/STICK/trails.html
```

## 4. The assistant, with a user's own key, performs the Phase 1 agent loop in the browser: met

**Met on 2026-10-07.** Jay ran it on the published site, in Safari, with his own key: Anthropic, `claude-haiku-5-5`. He asked an empty Dusk deck, made with New, the question below, word for word. The assistant:
1. read the deck (`deck_read`, `deck_inspect`: no nodes, one state);
2. read the theme's layouts and type roles (`resource_read`, twice);
3. put the headline in the `full` layout's `header` slot, a patch lint refused: E100, the headline sets in two lines and the slot holds one;
4. took lint's fix, `fit: shrink`, and patched again;
5. linted the deck, which found nothing;
6. rendered the state at 1920 × 1080, and showed it in its answer.

All three things below held. It took 61,057 tokens in and 1,893 out.

What the run found after it: a drag of the headline up off its slot landed it in the whole canvas, flush in the corner. A box in a margin now goes into the nearest of the layout's slots (PLAN 2.31).

`web/assistant.mjs` runs PLAN 1.19's loop through the editor against a scripted server for each wire format: Anthropic, OpenAI, and Gemini.
- **The loop:** a headline too long for its slot, the E100, its fix, a clean lint, and a render.
- **What it checks** is what the browser sends: the key in the provider's header, the tools, and each result. It also checks that the editor ends with the source edited and linting clean.

All three pass. A real key is the only full test.

**To run it:**
1. Serve the pages (`just web-dev`). Open `http://localhost:5173/editor.html?bundle=/target/web-assistant/loop/deck.json`, the empty Dusk deck `node web/assistant.mjs` makes, or any deck.
2. In the Assistant tab, choose a provider, paste a key, and pick a model. The key stays in the tab unless you tick "keep on this device".
3. Ask: "Put the headline 'Volunteers rebuilt fifty-two miles of trail' on the slide, fix what lint finds, and show me."
4. It is met when three things hold:
   - the source takes the headline;
   - lint ends with no error;
   - the answer shows the frame.
5. Record the provider and model here and in the gate log.
