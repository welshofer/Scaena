// The player page (PLAN 2.1–2.2, SPEC §9.2). `?bundle=` is a bundle's directory or its deck
// file (the torture deck by default); `?painter=gpu` or `cpu` chooses who paints (WebGPU where
// the browser has an adapter, else the CPU painter, by default); `?state=` is the state to
// open on. `?view=presenter` is the presenter view, which follows and steers the player that
// opened it, over a BroadcastChannel.
//
// Keys: → ↓ PageDown Space Enter go on; ← ↑ PageUp Backspace go back; Home and End; F
// fullscreen; P the presenter view. A click or a tap on the slide goes on; a swipe goes
// either way. Going on plays the next state's cue; a state that holds goes on by itself
// when its hold is over (SPEC §2.4). Going on during a cue finishes it.
import type { At, Painter, Slot } from "./protocol";
import { Stage } from "./stage";

const params = new URLSearchParams(location.search);
const deck = deckFile(new URL(params.get("bundle") ?? "../../tests/fixtures/torture.scaena", location.href));
const painter = (params.get("painter") ?? "auto") as Painter;
/** The player and its presenter view: the player says where the deck is; the presenter view
 * steers it. */
const channel = new BroadcastChannel(`scaena:${deck}`);
type Follow = { type: "at"; format?: string } & At;
type Steer = { type: "on" } | { type: "back" } | { type: "hello" };

/** A bundle's deck file: the bundle's `deck.json`, or a deck file named outright, whose
 * directory is then the bundle (SPEC §3.1). */
function deckFile(bundle: URL): string {
  if (bundle.pathname.endsWith(".json")) return bundle.href;
  const dir = new URL(bundle);
  if (!dir.pathname.endsWith("/")) dir.pathname += "/";
  return new URL("deck.json", dir).href;
}

const $ = <T extends HTMLElement>(selector: string) => document.querySelector<T>(selector)!;
const said = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** The keys the player and the presenter view answer to, as `on`, `back`, and the rest. */
function keys(act: Partial<Record<"on" | "back" | "first" | "last" | "full" | "present", () => void>>) {
  const names: Record<string, keyof typeof act> = {
    ArrowRight: "on", ArrowDown: "on", PageDown: "on", " ": "on", Enter: "on",
    ArrowLeft: "back", ArrowUp: "back", PageUp: "back", Backspace: "back",
    Home: "first", End: "last", f: "full", F: "full", p: "present", P: "present",
  };
  addEventListener("keydown", (e) => {
    const target = e.target as HTMLElement;
    if (e.metaKey || e.ctrlKey || e.altKey || target.closest("input, select, textarea, button")) return;
    const action = act[names[e.key]];
    if (!action) return;
    e.preventDefault();
    action();
  });
}

const fullscreen = () =>
  (document.fullscreenElement ? document.exitFullscreen() : document.documentElement.requestFullscreen()).catch(
    () => {},
  );

/** The player: the deck, its controls, and its clock. */
async function play() {
  const status = $("#status");
  const statesPicker = $<HTMLSelectElement>("#state");
  const formatPicker = $<HTMLSelectElement>("#format");
  const scrub = $<HTMLInputElement>("#scrub");
  const stage = await Stage.open($("#stage"), deck, painter);
  stage.onError = (e) => (status.textContent = `error: ${e.message}`);
  const format = () => formatPicker.value || undefined;
  let slots: Slot[] = await stage.timeline();
  // The scrubber runs over the states, each a step of it, however long its cue: most decks'
  // states take no time on the timeline but their cues. Within a step it runs through the
  // state's cue, and its end is the state at rest.
  const scrubbed = (at: At) => at.index + (slots[at.index].span > 0 ? Math.min(at.t / slots[at.index].span, 1) : 1);
  const scrubTo = (value: number) => {
    const index = Math.min(Math.max(Math.ceil(value) - 1, 0), slots.length - 1);
    return { index, t: Math.min(Math.max(value - index, 0), 1) * slots[index].span };
  };
  /** Say where the deck is: in the controls, and to the presenter view. */
  const report = (at: At) => {
    status.textContent = [
      `${at.index + 1} / ${slots.length}`,
      slots[at.index].state,
      format(),
      at.playing ? "playing" : "",
      stage.opened.painter === "webgpu" ? "WebGPU" : "CPU painter",
    ]
      .filter(Boolean)
      .join(" · ");
    if (statesPicker.selectedIndex !== at.index) statesPicker.selectedIndex = at.index;
    scrub.value = String(scrubbed(at));
    channel.postMessage({ type: "at", ...at, format: format() } satisfies Follow);
  };
  stage.onAt = report;

  /** On: finish a cue that is playing, else play the next state's. */
  const on = () => {
    const { index, t, playing } = stage.at;
    if (playing && t < slots[index].span) return void stage.seek(index, undefined, format());
    if (index + 1 < slots.length) stage.run(index + 1, 0, format());
  };
  /** Back: the state before, at rest. */
  const back = () => void stage.seek(Math.max(0, stage.at.index - 1), undefined, format());
  const go = (i: number) => void stage.seek(i, undefined, format());
  const present = () => {
    const url = new URL(location.href);
    url.searchParams.set("view", "presenter");
    open(url, "scaena-presenter", "popup,width=1280,height=800");
  };

  for (const slot of slots) statesPicker.add(new Option(slot.state, slot.state));
  for (const id of stage.opened.formats) formatPicker.add(new Option(id, id));
  statesPicker.onchange = () => stage.run(statesPicker.selectedIndex, 0, format());
  formatPicker.onchange = async () => {
    slots = await stage.timeline(format());
    go(Math.min(stage.at.index, slots.length - 1));
  };
  scrub.max = String(slots.length);
  scrub.oninput = () => {
    const { index, t } = scrubTo(Number(scrub.value));
    void stage.seek(index, t, format());
  };
  $("#on").onclick = on;
  $("#back").onclick = back;
  $("#full").onclick = fullscreen;
  $("#present").onclick = present;
  keys({ on, back, first: () => go(0), last: () => go(slots.length - 1), full: fullscreen, present });
  // A click or a tap goes on; a swipe goes either way.
  let down: PointerEvent | undefined;
  stage.canvas.onpointerdown = (e) => (down = e);
  stage.canvas.onpointerup = (e) => {
    if (!down) return;
    const [dx, dy] = [e.clientX - down.clientX, e.clientY - down.clientY];
    down = undefined;
    if (Math.abs(dx) > 40 && Math.abs(dx) > Math.abs(dy)) return dx < 0 ? on() : back();
    if (e.button === 0) on();
  };
  channel.onmessage = ({ data }: MessageEvent<Steer>) => {
    if (data.type === "on") on();
    else if (data.type === "back") back();
    else if (data.type === "hello") report(stage.at);
  };

  const first = Math.max(0, slots.findIndex((slot) => slot.state === params.get("state")));
  await stage.seek(first);
  // For tests and the console: the open bundle, its frames, and its clock.
  Object.assign(window, {
    scaena: {
      ...stage.opened,
      show: stage.show.bind(stage),
      timeline: stage.timeline.bind(stage),
      seek: stage.seek.bind(stage),
      run: stage.run.bind(stage),
      at: () => stage.at,
      on,
      back,
    },
  });
}

/** The presenter view: the deck as the player shows it, the next state, the notes, and a
 * clock. It steers the player, and follows it. */
async function presentView() {
  document.body.className = "presenter";
  document.body.innerHTML = `
    <main><canvas id="stage"></canvas></main>
    <aside>
      <div class="next"><canvas id="upnext"></canvas><p id="nextName"></p></div>
      <p class="clock"><span id="clock">0:00</span> · <span id="where"></span></p>
      <div id="notes" class="notes"></div>
      <p class="buttons"><button id="back" aria-label="Back">◀</button> <button id="on" aria-label="On">▶</button></p>
      <p id="status" role="status">loading…</p>
    </aside>`;
  const status = $("#status");
  const [now, upNext] = await Promise.all([
    Stage.open($("#stage"), deck, painter),
    Stage.open($("#upnext"), deck, painter),
  ]);
  const { states, notes } = now.opened;
  const steer = (message: Steer) => channel.postMessage(message);
  keys({ on: () => steer({ type: "on" }), back: () => steer({ type: "back" }), full: fullscreen });
  $("#on").onclick = () => steer({ type: "on" });
  $("#back").onclick = () => steer({ type: "back" });
  const opened = performance.now();
  const clock = () => {
    const s = Math.floor((performance.now() - opened) / 1000);
    $("#clock").textContent = `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  };
  setInterval(clock, 1000);

  // The player sends a frame's place as it paints it; this view paints the latest, and lets
  // the ones it cannot keep up with go.
  let latest: Follow | undefined;
  let painting = false;
  let next = -1;
  const follow = async () => {
    if (painting) return;
    painting = true;
    try {
      while (latest) {
        const at: Follow = latest;
        latest = undefined;
        await now.show(states[at.index], at.t, at.format);
        $("#where").textContent = `${at.index + 1} / ${states.length} · ${states[at.index]}`;
        $("#notes").textContent = notes[at.index] || "No notes.";
        if (at.index !== next) {
          next = at.index;
          const coming = states[at.index + 1];
          $("#nextName").textContent = coming ? `Next: ${coming}` : "The end of the deck.";
          if (coming) await upNext.show(coming, undefined, at.format);
        }
        status.textContent = "";
      }
    } catch (e) {
      status.textContent = `error: ${said(e)}`;
    } finally {
      painting = false;
    }
  };
  channel.onmessage = ({ data }: MessageEvent<Follow>) => {
    if (data.type !== "at") return;
    latest = data;
    void follow();
  };
  steer({ type: "hello" });
  Object.assign(window, { scaena: { ...now.opened, follows: () => $("#where").textContent } });
}

try {
  await (params.get("view") === "presenter" ? presentView() : play());
} catch (e) {
  $("#status").textContent = `error: ${said(e)}`;
}
