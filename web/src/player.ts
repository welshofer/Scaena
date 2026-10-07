// The player (PLAN 2.1–2.2, SPEC §9.2), on any page that plays a bundle: the web player's
// (`main.ts`) and a single-file export's (`standalone.ts`, PLAN 2.5). `?painter=` and the
// page's other parameters are the page's to read; `?view=presenter` is the presenter view,
// which follows and steers the player that opened it, over a BroadcastChannel.
//
// Keys: → ↓ PageDown Space Enter go on; ← ↑ PageUp Backspace go back; Home and End; F
// fullscreen; P the presenter view. A click or a tap on the slide goes on; a swipe goes
// either way. Going on plays the next state's cue; a state that holds goes on by itself
// when its hold is over (SPEC §2.4). Going on during a cue finishes it.
//
// For a screen reader (PLAN 2.8, SPEC §3.12): the canvas is hidden, and the page keeps how the
// state shown reads in a live region (`#reading`). The state picker is the spine's outline.
// For a reader who asks for less motion (`prefers-reduced-motion`, or `?motion=reduce`; and
// `?motion=full` to have it anyway), each cue is a cut, and the deck keeps its pace.
//
// On a page `scaena serve` serves (`?serve`, PLAN 2.11), each change to the bundle on disk shows
// as it is made: the deck is read again and shown where it was. A `deck.scn` that does not
// compile says so, at its line, and the deck stays as it was until it does.
import type { At, Painter, Section, Slot, Source } from "./protocol";
import { reader } from "./reading";
import { line, listen, served } from "./served";
import { type Engine, Stage } from "./stage";

/** How a page plays a bundle. */
export interface Play {
  painter: Painter;
  engine: Engine;
  /** The channel the player and its presenter view share: one per bundle. */
  channel: string;
  /** The state to open on, by id. */
  state?: string | null;
  /** How a state reads, as HTML: the reading a single file carries for each state it plays.
   * Without it, the engine reads the state in the format shown. */
  read?: (state: string, format?: string) => string | Promise<string>;
}

type Follow = { type: "at"; format?: string } & At;
/** What the presenter view asks of the player: on, back, the first or the last state, or where it is. */
type Steer = { type: "on" | "back" | "first" | "last" | "hello" };

/** Play the bundle at `deck`: the player, or, at `?view=presenter`, its presenter view. */
export function start(deck: Source, how: Play): Promise<void> {
  const channel = new BroadcastChannel(how.channel);
  const presenter = new URLSearchParams(location.search).get("view") === "presenter";
  return presenter ? presentView(deck, how, channel) : play(deck, how, channel);
}

const $ = <T extends HTMLElement>(selector: string) => document.querySelector<T>(selector)!;
const said = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** Whether the reader asked for less motion: the page's `?motion=`, else the system's. */
const reduced = matchMedia("(prefers-reduced-motion: reduce)");
const still = () => {
  const motion = new URLSearchParams(location.search).get("motion");
  return motion === "reduce" || (motion !== "full" && reduced.matches);
};

/** The keys the player and the presenter view answer to, as `on`, `back`, and the rest. */
function keys(act: Partial<Record<"on" | "back" | "first" | "last" | "full" | "present", () => void>>) {
  const names: Record<string, keyof typeof act> = {
    ArrowRight: "on", ArrowDown: "on", PageDown: "on", " ": "on", Enter: "on",
    ArrowLeft: "back", ArrowUp: "back", PageUp: "back", Backspace: "back",
    Home: "first", End: "last", f: "full", F: "full", p: "present", P: "present",
  };
  addEventListener("keydown", (e) => {
    // A control keeps the keys it answers to: a button or a link Enter and Space, the scrubber,
    // a picker, or a text field every key.
    const control = (e.target as HTMLElement).closest("input, select, textarea, button, a");
    const its = control && (!control.matches("button, a") || e.key === "Enter" || e.key === " ");
    if (e.metaKey || e.ctrlKey || e.altKey || its) return;
    const action = act[names[e.key]];
    if (!action) return;
    e.preventDefault();
    action();
  });
}

/** With `?fps`, the frame meter: while the deck plays, its frames a second over the run so far,
 * its worst frame, how many frames came late for a 60 Hz display (each over 25 ms after the
 * one before it), and the mean paint, from the worker's timings (gate 2, criterion 1). */
function meter(): ((at: At) => void) | undefined {
  const out = document.querySelector<HTMLOutputElement>("#meter");
  if (!out || !new URLSearchParams(location.search).has("fps")) return undefined;
  out.hidden = false;
  out.value = "fps: play the deck";
  let intervals: number[] = [];
  let paints: number[] = [];
  let shown = 0;
  const show = () => {
    const frames = intervals.length;
    if (!frames) return;
    const sum = intervals.reduce((a, b) => a + b, 0);
    const late = intervals.filter((i) => i > 25).length;
    const paint = paints.reduce((a, b) => a + b, 0) / paints.length;
    out.value = `${((1000 * frames) / sum).toFixed(1)} fps · worst ${Math.max(...intervals).toFixed(0)} ms · ${late} late of ${frames} · paint ${paint.toFixed(1)} ms`;
  };
  return (at) => {
    if (at.frame) {
      // A run's first frame has no interval: a new run, measured on its own.
      if (at.frame.interval === undefined) [intervals, paints] = [[], []];
      else intervals.push(at.frame.interval);
      paints.push(at.frame.paint);
      if (performance.now() - shown > 250) {
        shown = performance.now();
        show();
      }
    }
    if (!at.playing) show();
  };
}

const fullscreen = () =>
  (document.fullscreenElement ? document.exitFullscreen() : document.documentElement.requestFullscreen()).catch(
    () => {},
  );

/** The player: the deck, its controls, and its clock. */
async function play(deck: Source, how: Play, channel: BroadcastChannel) {
  const status = $("#status");
  const statesPicker = $<HTMLSelectElement>("#state");
  const formatPicker = $<HTMLSelectElement>("#format");
  const scrub = $<HTMLInputElement>("#scrub");
  const stage = await Stage.open($("#stage"), deck, how.painter, how.engine);
  stage.onError = (e) => (status.textContent = `error: ${e.message}`);
  const format = () => formatPicker.value || undefined;
  let slots: Slot[] = await stage.timeline();
  const region = document.querySelector("#reading");
  const read = region && reader(region, how.read ?? ((state, format) => stage.reading(state, format)));
  // The scrubber runs over the states, each a step of it, however long its cue: most decks'
  // states take no time on the timeline but their cues. Within a step it runs through the
  // state's cue, and its end is the state at rest.
  const scrubbed = (at: At) => at.index + (slots[at.index].span > 0 ? Math.min(at.t / slots[at.index].span, 1) : 1);
  const scrubTo = (value: number) => {
    const index = Math.min(Math.max(Math.ceil(value) - 1, 0), slots.length - 1);
    return { index, t: Math.min(Math.max(value - index, 0), 1) * slots[index].span };
  };
  /** Say where the deck is: in the controls, to the presenter view, and to the page. */
  const report = (at: At) => {
    const line = [
      `${at.index + 1} / ${slots.length}`,
      slots[at.index].state,
      format(),
      at.playing ? "playing" : "",
      stage.opened.painter === "webgpu" ? "WebGPU" : "CPU painter",
    ]
      .filter(Boolean)
      .join(" · ");
    // The status is a live region: it changes when the deck's place does, not every frame.
    if (status.textContent !== line) status.textContent = line;
    if (statesPicker.value !== String(at.index)) statesPicker.value = String(at.index);
    scrub.value = String(scrubbed(at));
    scrub.setAttribute("aria-valuetext", `${at.index + 1} of ${slots.length}: ${slots[at.index].state}`);
    channel.postMessage({ type: "at", ...at, format: format() } satisfies Follow);
    void read?.(slots[at.index].state, format());
  };
  const measure = meter();
  stage.onAt = (at) => {
    report(at);
    measure?.(at);
  };

  /** On: finish a cue that is playing, else play the next state's. */
  const on = () => {
    const { index, t, playing } = stage.at;
    if (playing && t < slots[index].span) return void stage.seek(index, undefined, format());
    if (index + 1 < slots.length) stage.run(index + 1, 0, format(), still());
  };
  /** Back: the state before, at rest. */
  const back = () => void stage.seek(Math.max(0, stage.at.index - 1), undefined, format());
  const go = (i: number) => void stage.seek(i, undefined, format());
  const present = () => {
    const url = new URL(location.href);
    url.searchParams.set("view", "presenter");
    open(url, "scaena-presenter", "popup,width=1280,height=800");
  };

  outline(statesPicker, slots, stage.opened.outline);
  for (const id of stage.opened.formats) formatPicker.add(new Option(id, id));
  statesPicker.onchange = () => stage.run(Number(statesPicker.value), 0, format(), still());
  // A change of mind while the deck plays takes hold where it is.
  reduced.onchange = () => {
    if (stage.at.playing) stage.run(stage.at.index, stage.at.t, format(), still());
  };
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
    if (e.button === 0) void follow(e).then((followed) => followed || on());
  };
  /** A click on a link at rest follows it (PLAN 2.70): a state is shown, a web address opens in
   * a window of its own. Whether there was one. */
  const follow = async (e: PointerEvent): Promise<boolean> => {
    const { index, t, playing } = stage.at;
    if (playing && t < slots[index].span) return false;
    const r = stage.canvas.getBoundingClientRect();
    const at: [number, number] = [(e.clientX - r.left) / r.width, (e.clientY - r.top) / r.height];
    const link = await stage.linkAt(slots[index].state, at, format()).catch(() => undefined);
    if (!link) return false;
    if ("href" in link) {
      open(link.href, "_blank", "noopener");
      return true;
    }
    const to = slots.findIndex((s) => s.state === link.state);
    if (to < 0) return false;
    stage.run(to, 0, format(), still());
    return true;
  };
  // A link in how a state reads goes there too.
  region?.addEventListener("click", (e) => {
    const a = (e.target as Element).closest?.("a[data-state]");
    const to = a ? slots.findIndex((s) => s.state === a.getAttribute("data-state")) : -1;
    if (to < 0) return;
    e.preventDefault();
    stage.run(to, 0, format(), still());
  });
  channel.onmessage = ({ data }: MessageEvent<Steer>) => {
    if (data.type === "on") on();
    else if (data.type === "back") back();
    else if (data.type === "first") go(0);
    else if (data.type === "last") go(slots.length - 1);
    else if (data.type === "hello") report(stage.at);
  };

  /** The bundle read again, and shown where the deck was: the same state, at rest. */
  let reloads = 0;
  const reload = async () => {
    const state = slots[stage.at.index]?.state;
    await stage.reload();
    const chosen = formatPicker.value;
    formatPicker.replaceChildren(new Option("own canvas", ""), ...stage.opened.formats.map((id) => new Option(id, id)));
    formatPicker.value = stage.opened.formats.includes(chosen) ? chosen : "";
    slots = await stage.timeline(format());
    outline(statesPicker, slots, stage.opened.outline);
    scrub.max = String(slots.length);
    const index = slots.findIndex((slot) => slot.state === state);
    const at = await stage.seek(index >= 0 ? index : Math.min(stage.at.index, slots.length - 1), undefined, format());
    await read?.(slots[at.index].state, format(), true);
    reloads++;
  };
  if (served) {
    const alert = document.querySelector<HTMLElement>("#served");
    listen({
      // A change to the deck's source alone shows once it compiles, as `deck.json`.
      changed: (paths) => {
        if (paths.some((path) => path !== "deck.scn")) void reload().catch((e) => (status.textContent = `error: ${said(e)}`));
      },
      status: (failed) => {
        if (!alert) return;
        alert.hidden = !failed;
        alert.textContent = failed ? `deck.scn does not compile, so the deck is as it was. ${failed.problems.map(line).join(" · ")}` : "";
      },
    });
  }

  const first = Math.max(0, slots.findIndex((slot) => slot.state === how.state));
  await stage.seek(first);
  // For tests and the console: the open bundle, its frames, and its clock.
  Object.assign(window, {
    scaena: {
      ...stage.opened,
      opened: () => stage.opened,
      reloads: () => reloads,
      show: stage.show.bind(stage),
      timeline: stage.timeline.bind(stage),
      seek: stage.seek.bind(stage),
      /** Where a click at `at`, fractions of the canvas, in the state shown goes (PLAN 2.70). */
      linkAt: (at: [number, number]) => stage.linkAt(slots[stage.at.index].state, at, format()),
      run: stage.run.bind(stage),
      at: () => stage.at,
      on,
      back,
    },
  });
}

/** The state picker as the spine's outline (SPEC §3.11–3.12): a group for each section, in
 * the spine's order, each state named with its beat's claim; a deck without a spine lists its
 * states as they play. Each option's value is its slot. */
function outline(picker: HTMLSelectElement, slots: Slot[], sections: Section[]) {
  const slotOf = new Map(slots.map((slot, i) => [slot.state, i]));
  const option = (state: string, claim?: string) =>
    new Option(claim ? `${state} · ${claim}` : state, String(slotOf.get(state)));
  if (!sections.length) return void picker.replaceChildren(...slots.map((slot) => option(slot.state)));
  picker.replaceChildren(
    ...sections.map(({ title, states }) => {
      const options = states.filter(({ state }) => slotOf.has(state)).map(({ state, claim }) => option(state, claim));
      if (!title) return options;
      const group = document.createElement("optgroup");
      group.label = title;
      group.append(...options);
      return [group];
    }).flat(),
  );
}

/** The presenter view: the deck as the player shows it, the next state, the notes, and a
 * clock. It steers the player, and follows it. */
async function presentView(deck: Source, how: Play, channel: BroadcastChannel) {
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
    Stage.open($("#stage"), deck, how.painter, how.engine),
    Stage.open($("#upnext"), deck, how.painter, how.engine),
  ]);
  let { states, notes } = now.opened;
  const steer = (message: Steer) => channel.postMessage(message);
  // The player's own keys, Home and End among them, steer it from here.
  keys({
    on: () => steer({ type: "on" }),
    back: () => steer({ type: "back" }),
    first: () => steer({ type: "first" }),
    last: () => steer({ type: "last" }),
    full: fullscreen,
  });
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
  // Served (PLAN 2.11): the bundle changed on disk, so this view reads it again, as the player
  // does, and then follows the player where it shows the deck.
  if (served)
    listen({
      changed: (paths) => {
        if (!paths.some((path) => path !== "deck.scn")) return;
        void Promise.all([now.reload(), upNext.reload()]).then(() => {
          ({ states, notes } = now.opened);
          next = -1;
          steer({ type: "hello" });
        });
      },
    });
  steer({ type: "hello" });
  Object.assign(window, { scaena: { ...now.opened, follows: () => $("#where").textContent } });
}
