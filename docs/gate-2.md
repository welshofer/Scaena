# Gate 2

Phase 2's exit criteria (PLAN, "Exit criteria (gate 2)"), each with its evidence so far.

**Two of the four are met in headless Chromium. The other two need Jay:**
- the first needs a real machine's browsers;
- the fourth needs his own key.

The gate log stays open until both are run. Each section below gives the steps, about ten minutes each. Phase 2's tasks are done but for 2.7's deploy, which waits on where the site goes, and 2.12, a new deck in the editor, which the authoring guide (2.10) found.

## 1. A Phase 1 deck plays at 60 fps on WebGPU in Chrome and Safari 26+, and acceptably on the CPU fallback in Firefox: needs a real machine

CI cannot say. Headless Chromium composites WebGPU on SwiftShader at about a frame a second.

**The CPU fallback has room.** On a shared container's CPU, headless Chromium's CPU painter plays a torture cue at 42 fps (`web/player.mjs`). It paints each 1080p frame in about 20 ms, and its worst frame comes 33 ms after the one before. Before ADR-0004 finding 15, the same cue played at 13 fps, 71 ms a paint. In V8 alone, the engine paints B1's frames in 13 ms at the median and 18 ms at worst. The page then puts each frame on the canvas in under 2 ms.

**The player measures itself.** With `?fps` it shows a frame meter while the deck plays, for the run so far:
- frames a second;
- the worst frame;
- how many frames came more than 25 ms after the one before, which is late for a 60 Hz display;
- the mean paint.

The worker times each frame by the display's clock.

**To run it:**
1. Serve the pages: `just web` once, then `just web-dev`. Or use the site once it is deployed (PLAN 2.7).
2. In Chrome, open `http://localhost:5173/?bundle=/docs/examples/trails.deck.json&fps`. On the site, open `index.html?fps`.
3. The status line should say `WebGPU`. Make the window the size you present at, or press F for fullscreen.
4. Go through the deck with →. Each cue with motion fills the meter: the cover's rise, the charts growing, the table, and the diagram. Note the lowest frames a second and the worst frame.
5. Do the same in Safari 26 on the Mac.
6. Do the same in Firefox, which paints with the CPU where it has no WebGPU; `&painter=cpu` forces that anywhere. The status line says `CPU painter`.
7. Record each browser, the machine, and its numbers here and in the gate log.

**Proposed bar:**
- Met on WebGPU: 55 fps or more on every cue, with no more than a few late frames.
- Acceptable on the CPU fallback: 30 fps or more at the size you present at.

The bar is Jay's call.

## 2. Edit → lint → preview round trip under 200 ms on a 40-state deck: met

`web/editor.mjs` times six edits on B1 (`tests/bench/b1.scaena`, 40 states) in headless Chromium, painting with the CPU. Its latest run had a median round trip of 85 ms. The last edit took:
- compile 15 ms;
- the frame 19 ms;
- the lint of the state shown 34 ms.

PLAN 2.3 measured 114 ms. Before ADR-0004 finding 15, the frame alone took 76 ms.

The lint of every state, once typing stops, takes 1.2 s. It is not part of the round trip.

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

## 4. The assistant, with a user's own key, performs the Phase 1 agent loop in the browser: needs Jay's key

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
