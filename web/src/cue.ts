// The cue of the state shown (PLAN 2.44, ADR-0013, SPEC §3.9), under the preview: its transition,
// then each motion on the cue's clock, a bar each, where `scaena inspect --timeline` places them.
// The page lays nothing out and times nothing itself: the engine says when each motion starts and
// rests, its delay as written, and where it is written.
//
// - A bar dragged changes its motion's delay, and its end its duration (`time_motion`, written
//   where the motion is). The transition's end changes its duration (`set_state`). Times move in
//   whole tens of ms. With a bar or an end focused, the arrow keys do the same, 10 ms a press,
//   100 with Shift. A motion on a spring lasts as long as it takes to settle, so its end does not
//   move.
// - Add a motion: one of the theme's presets for the node the canvas selects, as it enters or
//   for emphasis, or for a node that leaves, as it leaves (`apply_preset`; an exit is written in
//   the state the node leaves). A press on a bar selects its node.
// - A press on the ruler or a track, or a drag along it, shows the cue at that time. Play plays
//   the cue from there, and the preview comes to rest at its end; the canvas shows the state at
//   rest again when it is pressed, and an edit shows it at rest.
// - Each change is one patch, by the user, one step to undo. What the deck refuses, the status
//   says why.
import { type Key, SHIFT } from "./commands";
import type { At, Cue, Edited, Inspected, Motion } from "./protocol";
import type { Stage } from "./stage";

/** What the cue asks of the editor around it. */
export interface CueEditor {
  /** The state shown, by its id and its slot's index; none while the source does not compile. */
  shown(): { state: string; index: number } | undefined;
  format(): string | undefined;
  source(): string;
  /** Take `source`, a patch's, as one change: one step to undo. */
  apply(source: string, edited: Edited): void;
  say(text: string): void;
  /** The node the canvas selects, and select one there. */
  chosen(): string | undefined;
  select(node: string): void;
  /** The deck's states, by id, in cue-list order. */
  states(): string[];
}

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
const html = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
/** A drag moves a time in whole tens of ms. */
const STEP = 10;
const snap = (ms: number) => Math.round(ms / STEP) * STEP;

/** What a bar of the cue answers, focused (PLAN 2.65), as the keys sheet lists it. */
export const cueKeys = (): Key[] => [
  { keys: "← →", label: `In the cue: move a motion's start, or its end, ${STEP} ms`, group: "The cue" },
  { keys: `${SHIFT}← ${SHIFT}→`, label: "Move it 100 ms", group: "The cue" },
];

/** One bar: the transition, or a motion. */
interface Bar {
  /** Which bar it is, kept across a redraw: `transition`, or the motion's node and kind. */
  key: string;
  label: string;
  motion?: Motion;
  /** When it starts to change and when it rests, ms into the cue. */
  from: number;
  to: number;
  /** Its delay as written, and its duration: per unit, and for `anim` from its first key to its
   * last. */
  delay: number;
  duration: number;
  /** On a spring, which lasts as long as it settles: its end does not move. */
  sprung: boolean;
}

/** The bars of `cue`: its transition, then its motions in the order the state lists them. */
function bars(cue: Cue): Bar[] {
  const seen = new Map<string, number>();
  const motions = cue.motions.map((m): Bar => {
    const name = `${m.node} ${m.motion}`;
    const n = (seen.get(name) ?? 0) + 1;
    seen.set(name, n);
    const units = m.units > 1 ? ` ×${m.units}` : "";
    const span = m.moving[1] - m.moving[0] - (m.units - 1) * m.stagger;
    return {
      key: n === 1 ? name : `${name} ${n}`,
      label: `${m.node} · ${m.motion}${m.split ? ` by ${m.split}` : ""}${units}`,
      motion: m,
      from: m.moving[0],
      to: m.moving[1],
      delay: m.delay,
      duration: m.motion === "anim" ? Math.max(0, span) : m.duration,
      sprung: "spring" in m.curve,
    };
  });
  const t = cue.transition;
  const transition: Bar = { key: "transition", label: "transition", from: 0, to: t.duration, delay: 0, duration: t.duration, sprung: "spring" in t.curve };
  return [transition, ...motions];
}

/** The cue in `into` (its play button, clock, motion menu, and lanes), over `stage`, beside the
 * preview `preview`. */
export function cue(stage: Stage, into: HTMLElement, preview: HTMLElement, editor: CueEditor) {
  const lanes = into.querySelector<HTMLElement>(".lanes")!;
  const play = into.querySelector<HTMLButtonElement>("[data-play]")!;
  const clock = into.querySelector<HTMLElement>(".clock")!;
  const adder = into.querySelector<HTMLSelectElement>("[data-add]")!;
  /** The state as the inspector last read it. */
  let read: Inspected | undefined;
  let drawn: Bar[] = [];
  /** How many ms the lanes span. */
  let scale = 1000;
  /** Where the preview is in the cue: a time a press on the ruler or Play put it at, or at rest. */
  let t: number | undefined;
  let playing = false;
  /** One change at a time, each made on the source the one before left. */
  let making: Promise<unknown> = Promise.resolve();
  /** The theme's motion presets, as the inspector offers them. */
  let presets: string[] = [];
  /** The bar or end focused, to focus again once the lanes are drawn again. */
  let focused: string | undefined;

  const pct = (ms: number) => `${(Math.min(Math.max(ms, 0), scale) / scale) * 100}%`;
  const span = () => read?.timeline?.span ?? 0;

  /** The cue of the state the inspector read: drawn, the preview at rest. */
  function show(found: Inspected) {
    read = found;
    t = undefined;
    playing = false;
    draw();
    void offer();
  }

  function draw() {
    const c = read?.timeline;
    into.hidden = !c;
    if (!c) return lanes.replaceChildren();
    drawn = bars(c);
    scale = Math.ceil((Math.max(1000, c.span, c.transition.duration) * 1.25) / 100) * 100;
    const step = scale <= 2000 ? 250 : scale <= 5000 ? 500 : 1000;
    const ticks = Array.from({ length: Math.floor(scale / step) + 1 }, (_, k) => k * step)
      .map((ms) => `<span class="tick" style="left:${pct(ms)}">${ms === 0 ? "0" : ms % 1000 === 0 ? `${ms / 1000} s` : ms}</span>`)
      .join("");
    const rows = drawn
      .map((b, i) => {
        const row = i + 2;
        const kind = b.motion?.motion ?? "transition";
        const what = b.motion ? `${b.label}, ${Math.round(b.from)}–${Math.round(b.to)} ms` : `the transition, ${Math.round(b.duration)} ms`;
        const body = b.motion
          ? `<div class="bar ${kind}" data-bar="${i}" data-part="delay" role="slider" tabindex="0" aria-label="${html(b.label)}: when it starts" aria-valuemin="0" aria-valuenow="${Math.round(b.delay)}" aria-valuetext="waits ${Math.round(b.delay)} ms, starts ${Math.round(b.from)} ms in" title="${html(what)}" style="left:${pct(b.from)};width:${pct(b.to - b.from)}"></div>`
          : `<div class="bar transition" data-bar="${i}" title="${html(what)}" style="left:0;width:${pct(b.to)}"></div>`;
        const end = b.sprung
          ? ""
          : `<div class="end" data-bar="${i}" data-part="duration" role="slider" tabindex="0" aria-label="${html(b.label)}: how long it lasts" aria-valuemin="0" aria-valuenow="${Math.round(b.duration)}" aria-valuetext="${Math.round(b.duration)} ms${b.motion && b.motion.units > 1 ? " each" : ""}" title="drag to change how long it lasts" style="left:${pct(b.to)}"></div>`;
        const tip = b.sprung ? ' title="on a spring: it lasts as long as it takes to settle"' : "";
        return `<span class="label" style="grid-row:${row}"${tip}>${html(b.label)}</span><div class="track" data-bar="${i}" style="grid-row:${row}">${body}${end}</div>`;
      })
      .join("");
    lanes.innerHTML = `<span class="label" style="grid-row:1"></span><div class="ruler" style="grid-row:1" title="press or drag to see the cue at a time">${ticks}</div>${rows}<div class="head" style="grid-row:1 / ${drawn.length + 2}" aria-hidden="true"><i></i></div>`;
    place();
    for (const el of lanes.querySelectorAll<HTMLElement>("[data-part]")) {
      const b = drawn[Number(el.dataset.bar)];
      el.dataset.key = `${b.key}/${el.dataset.part}`;
    }
    if (focused) lanes.querySelector<HTMLElement>(`[data-key="${CSS.escape(focused)}"]`)?.focus();
  }

  /** The playhead and the clock where the preview is. */
  function place() {
    const head = lanes.querySelector<HTMLElement>(".head i");
    const at = t ?? span();
    if (head) head.style.left = pct(at);
    clock.textContent = t === undefined ? `${Math.round(span())} ms, at rest` : `${Math.round(at)} of ${Math.round(span())} ms`;
    play.textContent = playing ? "❚❚" : "▶";
    play.setAttribute("aria-label", playing ? "Pause the cue" : "Play the cue");
    preview.classList.toggle("scrubbing", t !== undefined);
  }

  /** The preview at `ms` into the cue: the last time asked for wins. */
  let seeking = false;
  let wanted: number | undefined;
  async function seek(ms: number) {
    wanted = ms;
    if (seeking) return;
    seeking = true;
    while (wanted !== undefined) {
      const at = editor.shown();
      if (!at) break;
      t = Math.min(Math.max(0, wanted), span());
      wanted = undefined;
      playing = false;
      place();
      await stage.seek(at.index, t, editor.format()).catch((e) => editor.say(said(e)));
    }
    seeking = false;
  }

  /** The preview at rest again, where a press on the ruler or Play left it in the cue. */
  async function rest() {
    if (t === undefined && !playing) return;
    t = undefined;
    playing = false;
    place();
    const at = editor.shown();
    if (at) await stage.seek(at.index, undefined, editor.format()).catch(() => {});
  }

  /** Play the cue from the playhead, or from its start at rest; pause where it plays. */
  function toggle() {
    const at = editor.shown();
    if (!at || !read?.timeline) return;
    if (playing) {
      stage.pause();
      playing = false;
      return place();
    }
    playing = true;
    const from = t === undefined || t >= span() ? 0 : t;
    t = from;
    place();
    stage.run(at.index, from, editor.format(), false, true);
  }

  /** Where the deck is as it plays. */
  function follow(at: At) {
    const shown = editor.shown();
    if (!playing || !shown || at.index !== shown.index) return;
    if (at.playing) t = at.t;
    else [t, playing] = [at.t >= span() ? undefined : at.t, false];
    place();
  }

  /** Make `op` on the source as it stands: one change to undo. Then say `done`. */
  function make(op: Record<string, unknown>, doing: string, done: string) {
    const run = async () => {
      const at = editor.shown();
      if (!at) return editor.say("the cue waits for a source that compiles");
      editor.say(doing);
      try {
        const { source, edited } = await stage.make(editor.source(), [op], at.index, editor.format());
        editor.apply(source, edited);
        editor.say(done);
      } catch (e) {
        editor.say(`not made: ${said(e)}`);
      }
    };
    making = making.then(run, run);
    return making;
  }

  /** Bar `b`'s delay or duration `by` ms more (or less, never under 0): one patch. */
  function time(b: Bar, part: "delay" | "duration", by: number) {
    const at = editor.shown();
    if (!at || by === 0) return Promise.resolve();
    focused = `${b.key}/${part}`;
    if (!b.motion) {
      const value = Math.max(0, snap(b.duration + by));
      const op = { op: "set_state", id: at.state, prop: "transition/duration", value };
      return make(op, "timing…", `the transition into ${at.state} lasts ${value} ms`);
    }
    const { node, motion } = b.motion;
    const value = Math.max(part === "duration" && motion === "anim" ? STEP : 0, snap(b[part] + by));
    const op = { op: "time_motion", node, motion, state: at.state, [part]: value };
    const done = part === "delay" ? `${node}'s ${motion} waits ${value} ms` : `${node}'s ${motion} lasts ${value} ms${b.motion.units > 1 ? " a unit" : ""}`;
    return make(op, "timing…", done);
  }

  /** A drag of bar `b`'s start or end from the press `down`: the bar follows, and where it is let
   * go is one patch. A press that does not move selects the bar's node. */
  function drag(down: PointerEvent, el: HTMLElement, b: Bar, part: "delay" | "duration") {
    const track = el.closest<HTMLElement>(".track")!;
    const width = track.getBoundingClientRect().width || 1;
    el.setPointerCapture(down.pointerId);
    let by = 0;
    const bar = track.querySelector<HTMLElement>(".bar")!;
    const end = track.querySelector<HTMLElement>(".end");
    const move = (e: PointerEvent) => {
      by = snap(((e.clientX - down.clientX) / width) * scale);
      if (part === "delay") {
        const moved = Math.max(by, -b.delay);
        bar.style.left = pct(b.from + moved);
        if (end) end.style.left = pct(b.to + moved);
      } else {
        const to = Math.max(b.to + by, b.to - b.duration + (b.motion?.motion === "anim" ? STEP : 0));
        bar.style.width = pct(to - b.from);
        if (end) end.style.left = pct(to);
      }
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
      const moved = part === "delay" ? Math.max(by, -b.delay) : by;
      if (moved === 0) {
        if (b.motion && part === "delay") editor.select(b.motion.node);
        return draw();
      }
      void time(b, part, moved);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  }

  /** A press on the ruler or a track's empty space: the cue at that time, as the pointer goes. */
  function scrub(down: PointerEvent, along: HTMLElement) {
    const box = along.getBoundingClientRect();
    const at = (e: PointerEvent) => snap(((e.clientX - box.left) / (box.width || 1)) * scale);
    along.setPointerCapture(down.pointerId);
    void seek(at(down));
    const move = (e: PointerEvent) => void seek(at(e));
    const up = () => {
      along.removeEventListener("pointermove", move);
      along.removeEventListener("pointerup", up);
      along.removeEventListener("pointercancel", up);
    };
    along.addEventListener("pointermove", move);
    along.addEventListener("pointerup", up);
    along.addEventListener("pointercancel", up);
  }

  lanes.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    const part = target.closest<HTMLElement>("[data-part]");
    if (part) {
      e.preventDefault();
      const b = drawn[Number(part.dataset.bar)];
      return drag(e, part, b, part.dataset.part as "delay" | "duration");
    }
    const along = target.closest<HTMLElement>(".ruler, .track");
    if (along) {
      e.preventDefault();
      scrub(e, along);
    }
  });
  lanes.addEventListener("keydown", (e) => {
    const part = (e.target as HTMLElement).closest<HTMLElement>("[data-part]");
    if (!part || (e.key !== "ArrowLeft" && e.key !== "ArrowRight")) return;
    e.preventDefault();
    const by = (e.key === "ArrowLeft" ? -1 : 1) * (e.shiftKey ? 100 : STEP);
    void time(drawn[Number(part.dataset.bar)], part.dataset.part as "delay" | "duration", by);
  });
  lanes.addEventListener("focusin", (e) => {
    focused = (e.target as HTMLElement).dataset.key ?? focused;
  });
  play.onclick = () => toggle();
  // A press on the canvas edits the state at rest: the preview goes back to it first.
  preview.addEventListener("pointerdown", () => void rest(), true);
  const before = stage.onAt;
  stage.onAt = (at) => {
    before(at);
    follow(at);
  };

  /** The motions that may be added: for the node the canvas selects, its entrance where it enters
   * (or a chart, whose marks enter as its data changes) and an emphasis, each where it has none;
   * and for each node that leaves, its exit. Each one of the theme's presets. */
  async function offer() {
    const at = editor.shown();
    const found = read;
    const none = () => {
      adder.replaceChildren(new Option("Add a motion…", ""));
      adder.disabled = true;
    };
    if (!at || !found?.timeline) return none();
    const node = editor.chosen();
    const asked = node && found.nodes[node] ? node : Object.keys(found.nodes)[0];
    if (asked) {
      const choices = await stage.choices(at.state, asked).catch(() => undefined);
      const enter = choices?.fields.find((f) => f.prop === "enter");
      if (enter?.takes.kind === "name") presets = enter.takes.names;
    }
    if (read !== found) return;
    const has = (n: string, kind: string) => found.timeline?.motions.some((m) => m.node === n && m.motion === kind);
    const groups: HTMLOptGroupElement[] = [];
    const group = (label: string, kind: string, of: string) => {
      const g = Object.assign(document.createElement("optgroup"), { label });
      for (const p of presets) g.append(new Option(p, JSON.stringify([kind, of, p])));
      if (presets.length) groups.push(g);
    };
    if (node && found.nodes[node]) {
      const enters = (found.entered ?? []).includes(node) || found.nodes[node].type === "chart";
      if (enters && !has(node, "enter")) group(`${node} enters`, "enter", node);
      if (!has(node, "emphasis")) group(`${node}, for emphasis`, "emphasis", node);
    }
    for (const gone of found.exited ?? []) if (!has(gone, "exit")) group(`${gone} leaves`, "exit", gone);
    adder.replaceChildren(new Option("Add a motion…", ""), ...groups);
    // With nothing to add, it says why: the canvas selects what a motion is for.
    adder.disabled = groups.length === 0;
    adder.title = groups.length
      ? "A motion from the theme's presets: for the node selected, as it enters or for emphasis, or for a node that leaves, as it leaves"
      : "Select a node on the canvas to add a motion to it";
  }
  adder.onchange = () => {
    const chosen = adder.value;
    adder.value = "";
    const at = editor.shown();
    if (!chosen || !at) return;
    const [motion, node, preset] = JSON.parse(chosen) as [string, string, string];
    // An exit is read from the state the node leaves: the one before this one.
    const states = editor.states();
    const state = motion === "exit" ? states[states.indexOf(at.state) - 1] : at.state;
    if (!state) return editor.say(`${node} leaves no state before ${at.state}`);
    const verb = { enter: "as it enters", emphasis: "for emphasis", exit: "as it leaves" }[motion];
    void make({ op: "apply_preset", node, preset, motion, state }, "adding…", `${preset} on ${node} ${verb}`);
  };

  return {
    show,
    offer,
    rest,
    seek,
    toggle,
    /** The bars drawn, for a test: each one's key, when it starts and rests, its delay and duration. */
    bars: () => drawn.map(({ key, from, to, delay, duration, sprung }) => ({ key, from, to, delay, duration, sprung })),
    /** Where the preview is in the cue, for a test: `undefined` at rest. */
    at: () => t,
    /** How many ms the lanes span. */
    scale: () => scale,
    /** The changes made through the cue, for a test: once they are. */
    settled: () => making,
    /** Time a bar as a drag or a key would: its delay or duration `by` ms more. */
    time: (key: string, part: "delay" | "duration", by: number) => {
      const b = drawn.find((d) => d.key === key);
      if (!b) throw new Error(`no bar ${key}: the cue has ${drawn.map((d) => d.key).join(", ")}`);
      return time(b, part, by);
    },
  };
}
