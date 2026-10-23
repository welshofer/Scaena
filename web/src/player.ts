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
// The presenter's keys, as a presentation app's (PLAN 2.105): B or . turns the screen black and
// W or , white, the deck waiting where it was until a key or a click brings the slide back; a
// slide's number then Return goes there; L is the laser, a red dot where the pointer is over the
// slide; ? lists the keys. The presenter view's own do the same to the player, its laser drawn
// where its pointer is over the slide it shows.
//
// The end of the show (PLAN 2.120), as Keynote's and PowerPoint's: going on past the last state
// turns the screen black and says the show has ended; going on again, or Escape, leaves it, out of
// fullscreen and back on the last slide, and going back shows the last slide. The presenter view's
// timer pauses and resets, and the time of day stands beside it (PLAN 2.121).
//
// For a screen reader (PLAN 2.8, SPEC §3.12): the canvas is hidden, and the page keeps how the
// state shown reads in a live region (`#reading`). The state picker is the spine's outline.
// For a reader who asks for less motion (`prefers-reduced-motion`, or `?motion=reduce`; and
// `?motion=full` to have it anyway), each cue is a cut, and the deck keeps its pace.
//
// Fullscreen, and in the presenter view, the screen stays awake (`awake.ts`, lift-up REL-1).
//
// On a page `scaena serve` serves (`?serve`, PLAN 2.11), each change to the bundle on disk shows
// as it is made: the deck is read again and shown where it was. A `deck.scn` that does not
// compile says so, at its line, and the deck stays as it was until it does.
import { letSleep, stayAwake } from "./awake";
import type { At, LinkTarget, Painter, Section, Slot, Source } from "./protocol";
import { reader } from "./reading";
import { line, listen, served } from "./served";
import { type Engine, Stage } from "./stage";
import { numbered, place } from "./words";

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

/** The screen as the presenter turns it, black or white, the slide out of sight (PLAN 2.105). */
type Blank = "black" | "white";
/** A point on the slide, fractions of its width and height. */
type Point = [number, number];

/** Where the player is, as it tells the presenter view: the place, the format, whether the
 * screen is black or white or the laser on, and whether the show is over. */
type Follow = { type: "at"; format?: string; blank?: Blank; laser?: boolean; ended?: boolean } & At;
/** Where the laser points, as the player tells the presenter view: none where it is off, or off
 * the slide. */
type Pointed = { type: "pointed"; laser: boolean; point: Point | null };
/** That the player's page is gone, as it tells the presenter view: closed, or loaded anew. */
type Gone = { type: "gone" };
/** What the presenter view asks of the player: on, back, the first or the last state, or where
 * it is; the screen black or white, a slide by its number, the laser, and where it points. */
type Steer =
  | { type: "on" | "back" | "first" | "last" | "hello" | "black" | "white" | "laser" }
  | { type: "slide"; number: number }
  | { type: "point"; at: Point | null };

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

type Acting = "on" | "back" | "first" | "last" | "full" | "present" | "black" | "white" | "laser" | "leave";
/** What the keys do on a page: each its action, where the page has it, and a slide by its number. */
type Act = Partial<Record<Acting, () => void>> & { slide?: (number: number) => void };

/** The keys the player and the presenter view answer to, as `on`, `back`, and the rest; a
 * slide's number, typed then Return, goes there, and ? lists them (PLAN 2.105). */
function keys(act: Act) {
  const names: Record<string, Acting> = {
    ArrowRight: "on", ArrowDown: "on", PageDown: "on", " ": "on", Enter: "on",
    ArrowLeft: "back", ArrowUp: "back", PageUp: "back", Backspace: "back",
    Home: "first", End: "last", f: "full", F: "full", p: "present", P: "present",
    // As a presentation app's: B or . a black screen, W or , a white one, and L the laser.
    b: "black", B: "black", ".": "black", w: "white", W: "white", ",": "white", l: "laser", L: "laser",
    // Escape leaves the show at its end (PLAN 2.120).
    Escape: "leave",
  };
  const list = keyList(act);
  const number = numbering(act.slide);
  addEventListener("keydown", (e) => {
    // A control keeps the keys it answers to: a button or a link Enter and Space, the scrubber,
    // a picker, or a text field every key; but a slide's number typed takes its Return from a
    // button, as Next is where a click left the keys.
    const control = (e.target as HTMLElement).closest("input, select, textarea, button, a");
    if (e.metaKey || e.ctrlKey || e.altKey || (control && !control.matches("button, a"))) return;
    if (number.key(e.key) || list.key(e.key)) return e.preventDefault();
    if (control && (e.key === "Enter" || e.key === " ")) return;
    const action = act[names[e.key]];
    if (!action) return;
    e.preventDefault();
    action();
  });
  return number;
}

/** A slide's number as it is typed (PLAN 2.105): its digits shown at the foot of the page, Return
 * going there, Backspace taking the last back, and Escape, or four seconds with no key, letting it
 * go. Its `key` says whether a key was the number's. */
function numbering(go?: (number: number) => void) {
  const shown = document.createElement("output");
  shown.className = "typed";
  shown.hidden = true;
  document.body.append(shown);
  let typed = "";
  let idle: ReturnType<typeof setTimeout> | undefined;
  const show = (words?: string) => {
    clearTimeout(idle);
    shown.textContent = words ?? (typed ? `Slide ${typed} · Return goes there` : "");
    shown.hidden = !shown.textContent;
    if (!shown.hidden) idle = setTimeout(() => ((typed = ""), show()), 4000);
  };
  return {
    key(key: string): boolean {
      if (!go) return false;
      if (/^[0-9]$/.test(key)) {
        // No slide's number begins with 0: a 0 before another digit goes, and a 0 alone stays, a
        // number the deck has no slide for, which Return says.
        typed = `${typed}${key}`.replace(/^0+(?=\d)/, "").slice(0, 4);
      } else if (!typed) {
        return false;
      } else if (key === "Backspace") {
        typed = typed.slice(0, -1);
      } else if (key === "Escape") {
        typed = "";
      } else if (key === "Enter") {
        const n = Number(typed);
        typed = "";
        show();
        go(n);
        return true;
      } else {
        return false;
      }
      show();
      return true;
    },
    /** Say `words` where the number was typed, for as long as one stands there. */
    say: (words: string) => show(words),
  };
}

/** What a slide's number typed says where the deck has no such slide (PLAN 2.105). */
const noSlide = (slots: Slot[], number: number) => {
  const count = new Set(slots.map((s) => s.slide ?? s.state)).size;
  return `No slide ${number}: the deck has ${count === 1 ? "1 slide" : `${count} slides`}`;
};

/** The keys, listed (PLAN 2.105): ? shows and hides the list, and Escape or a click on it hides it.
 * Its `key` says whether a key was the list's. */
function keyList(act: Act) {
  const rows: [string, string, unknown][] = [
    ["<kbd>→</kbd> <kbd>Space</kbd> <kbd>Return</kbd>, or a click", "Go on: a cue playing finishes, then the next plays", true],
    ["<kbd>←</kbd> <kbd>Backspace</kbd>", "Go back", true],
    ["<kbd>Home</kbd> <kbd>End</kbd>", "The first slide, the last", true],
    ["A slide’s number, then <kbd>Return</kbd>", "That slide", act.slide],
    ["<kbd>B</kbd> or <kbd>.</kbd>", "A black screen, and the slide again", act.black],
    ["<kbd>W</kbd> or <kbd>,</kbd>", "A white screen, and the slide again", act.white],
    ["<kbd>L</kbd>", "The laser, where the pointer is over the slide", act.laser],
    ["<kbd>F</kbd>", "Fullscreen", act.full],
    ["<kbd>P</kbd>", "The presenter view, in a window of its own", act.present],
    ["<kbd>Escape</kbd>", "At the end of the show, leave it", act.leave],
    ["<kbd>?</kbd>", "These keys", true],
  ];
  const dialog = document.createElement("dialog");
  dialog.className = "keylist";
  dialog.setAttribute("aria-labelledby", "keylist-title");
  // The words are the page's own, none a deck's.
  dialog.innerHTML = `<h2 id="keylist-title">Keys</h2><dl>${rows
    .filter(([, , has]) => has)
    .map(([keys, does]) => `<dt>${keys}</dt><dd>${does}</dd>`)
    .join("")}</dl><p>A key or a click brings back a black or white screen. Past the last slide, a black screen says the show has ended. <kbd>Escape</kbd> or <kbd>?</kbd> closes this.</p>`;
  dialog.onclick = () => dialog.close();
  document.body.append(dialog);
  return {
    key(key: string): boolean {
      if (key === "?") dialog.open ? dialog.close() : dialog.show();
      else if (key === "Escape" && dialog.open) dialog.close();
      else return false;
      return true;
    },
  };
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

/** What a deck's windows are called: its title, else its bundle's name. */
const titled = (opened: { title?: string; name: string }) => opened.title ?? opened.name;

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
  stage.onError = (e) => (status.textContent = `Error: ${e.message}`);
  // The window is the deck's, by its title, as a single file's page is.
  document.title = titled(stage.opened);
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
  /** The screen black or white, where the presenter turned it so (PLAN 2.105). */
  let blank: Blank | undefined;
  /** Whether the laser is on, and where it points on the slide while the pointer is over it. */
  let laser = false;
  let point: Point | undefined;
  /** Whether the show is over: gone on past the last state, the screen black and saying so
   * (PLAN 2.120). */
  let ended = false;
  /** Whether this page has gone, closed or loading anew: from then on it tells the presenter view
   * nothing and answers it nothing, so a view that heard it go hears no answer from it as it
   * closes; shown again from the browser's cache, it tells again. */
  let away = false;
  const tell = (message: Follow | Pointed) => {
    if (!away) channel.postMessage(message);
  };

  // A link on the slide at rest shows a hand under the pointer, as a click there follows it and
  // does not go on (PLAN 2.70); the laser hides the pointer instead. One question at a time: an
  // answer the pointer, the deck, or the laser has moved on from asks again.
  let pointer: Point | undefined;
  let asking = false;
  const resting = (at: At) => !at.playing || at.t >= (slots[at.index]?.span ?? 0);
  const hovered = () => JSON.stringify([pointer, stage.at.index, resting(stage.at), laser, format()]);
  const hover = async () => {
    if (asking) return;
    asking = true;
    let link: LinkTarget | undefined;
    let asked: string;
    try {
      do {
        asked = hovered();
        const at = pointer;
        const shown = !laser && resting(stage.at) && slots[stage.at.index];
        link = at && shown ? await stage.linkAt(shown.state, at, format()).catch(() => undefined) : undefined;
      } while (asked !== hovered());
    } finally {
      asking = false;
    }
    stage.canvas.style.cursor = link ? "pointer" : "";
  };
  /** Say where the deck is: in the controls, to the presenter view, and to the page. */
  const report = (at: At) => {
    // Where the deck is, as a person counts its slides (PLAN 2.99); what paints it is said on a
    // pointer over it.
    const screen = blank ? `${blank} screen` : ended ? "end of the show" : "";
    const line = [place(slots, at.index), format(), at.playing && !screen ? "playing" : "", screen].filter(Boolean).join(" · ");
    // The status is a live region: it changes when the deck's place does, not every frame.
    if (status.textContent !== line) status.textContent = line;
    status.title = stage.opened.painter === "webgpu" ? "Painted by WebGPU" : "Painted by the CPU";
    if (statesPicker.value !== String(at.index)) statesPicker.value = String(at.index);
    scrub.value = String(scrubbed(at));
    scrub.setAttribute("aria-valuetext", slots[at.index].said ?? `${at.index + 1} of ${slots.length}`);
    tell({ type: "at", ...at, format: format(), blank, laser, ended });
    void read?.(slots[at.index].state, format());
  };
  const measure = meter();
  /** The state shown, and whether it is at rest: where it changes, the pointer's link is asked
   * again. */
  let rested = "";
  stage.onAt = (at) => {
    report(at);
    measure?.(at);
    const now = `${at.index} ${resting(at)}`;
    if (now !== rested) {
      rested = now;
      void hover();
    }
  };

  // The screen black or white (PLAN 2.105): over the page, the slide out of sight, and the deck
  // waiting where it was, a cue or a hold where it stood, until the slide is back.
  const curtain = document.createElement("div");
  curtain.className = "curtain";
  curtain.hidden = true;
  curtain.setAttribute("aria-hidden", "true");
  document.body.append(curtain);
  /** Where a cue or a hold stood when the screen went black or white, to go on from there. */
  let resume: { index: number; t: number } | undefined;
  /** The screen `to`, black or white; the slide again where it is that already, or for none. */
  const blanking = async (to?: Blank) => {
    // At the end of the show, the black or white screen takes the end's place.
    if (to) ending(false);
    const was = blank;
    blank = to === blank ? undefined : to;
    curtain.hidden = !blank;
    curtain.dataset.blank = blank ?? "";
    // The laser goes with the slide, and comes back with it.
    drawDot();
    if (blank && !was) {
      const { index, t, playing } = stage.at;
      resume = playing ? { index, t } : undefined;
      if (playing) await stage.seek(index, t, format());
    } else if (!blank && resume) {
      const { index, t } = resume;
      resume = undefined;
      stage.run(index, t, format(), still());
    }
    report(stage.at);
  };
  /** Whether the slide is in sight: a key or a click while the screen is black or white brings
   * it back, and does no more. */
  const shown = () => {
    if (!blank) return true;
    void blanking();
    return false;
  };
  curtain.onclick = () => void blanking();

  // The end of the show (PLAN 2.120), as Keynote's and PowerPoint's: past the last state, a black
  // screen over the page that says the show has ended, until the show is left or the deck shown.
  const end = document.createElement("div");
  end.className = "end";
  end.hidden = true;
  // The words are the page's own, none a deck's.
  end.innerHTML = `<p>The show has ended.</p><p>Click to leave, or press ← for the last slide.</p>`;
  document.body.append(end);
  /** The show over, or the deck shown again. */
  const ending = (to: boolean) => {
    if (ended === to) return;
    ended = to;
    end.hidden = !ended;
    // The laser goes with the slide, and comes back with it.
    drawDot();
    report(stage.at);
  };
  /** Leave the show at its end, as a presentation app's last click does: out of fullscreen, and
   * back on the last slide. */
  const leave = () => {
    if (!ended) return;
    ending(false);
    if (document.fullscreenElement) void document.exitFullscreen().catch(() => {});
  };
  // Out of fullscreen by the browser's own Escape, which the page does not hear, the show is left
  // at its end too. Fullscreen, the screen stays awake (lift-up REL-1); out of it, it may sleep.
  document.addEventListener("fullscreenchange", () => {
    if (document.fullscreenElement) return stayAwake();
    letSleep();
    ending(false);
  });

  /** On: finish a cue that is playing, else play the next state's; past the last, the show is
   * over, and on from there leaves it. */
  const on = () => {
    if (!shown()) return;
    if (ended) return leave();
    const { index, t, playing } = stage.at;
    if (playing && t < slots[index].span) return void stage.seek(index, undefined, format());
    if (index + 1 < slots.length) stage.run(index + 1, 0, format(), still());
    else ending(true);
  };
  /** Back: the state before, at rest; at the end of the show, the last again. */
  const back = () => {
    if (ended) return ending(false);
    if (shown()) void stage.seek(Math.max(0, stage.at.index - 1), undefined, format());
  };
  const go = (i: number) => {
    ending(false);
    if (shown()) void stage.seek(i, undefined, format());
  };
  /** Slide `number`, as a person counts them, its first state's cue played; the slide again where
   * the screen is black or white. Said where the deck has no such slide. */
  const slide = (number: number) => {
    const to = slots.findIndex((s) => s.state === numbered(slots, String(number)));
    if (to < 0) return typing.say(noSlide(slots, number));
    resume = undefined;
    ending(false);
    if (blank) void blanking();
    stage.run(to, 0, format(), still());
  };
  /** The presenter view's window, once this page opens it. */
  let view: Window | null = null;
  /** The presenter view, in a window of its own; one open already comes to the front as it is,
   * its timer running, since opening its address again would load it anew. */
  const present = () => {
    if (view && !view.closed) return view.focus();
    const url = new URL(location.href);
    url.searchParams.set("view", "presenter");
    // By its name alone, which finds a window of that name without loading it again, as one this
    // page opened before it was loaded again; a new one is blank, and goes to the view.
    view = open("", "scaena-presenter", "popup,width=1280,height=800");
    if (!view) return;
    let there = false;
    try {
      there = view.location.href === url.href;
    } catch {
      // Another page's window, out of reach: the view is loaded there.
    }
    if (there) view.focus();
    else view.location.href = url.href;
  };
  // The presenter view hears that the player is gone (PLAN 2.2), and says so; and where it is
  // again once the page shows again from the browser's cache.
  addEventListener("pagehide", () => {
    away = true;
    channel.postMessage({ type: "gone" } satisfies Gone);
  });
  addEventListener("pageshow", (e) => {
    if (!e.persisted) return;
    away = false;
    report(stage.at);
  });

  // The laser (PLAN 2.105): a red dot where the pointer is over the slide, the pointer hidden
  // there; the presenter view's points it too, from its own slide.
  const dot = document.createElement("div");
  dot.className = "laser";
  dot.hidden = true;
  dot.setAttribute("aria-hidden", "true");
  document.body.append(dot);
  /** The dot where the laser points, none while the screen is black or white: the room sees no
   * dot moving over it, as the Mac's and the iPad's stage show none. */
  const drawDot = () => {
    dot.hidden = !laser || !point || !!blank || ended;
    if (!point) return;
    const r = stage.canvas.getBoundingClientRect();
    dot.style.left = `${r.left + point[0] * r.width}px`;
    dot.style.top = `${r.top + point[1] * r.height}px`;
  };
  /** The laser pointing at `at` on the slide, or at nothing. */
  const pointAt = (at: Point | null | undefined) => {
    point = laser && at ? at : undefined;
    drawDot();
    tell({ type: "pointed", laser, point: point ?? null });
  };
  const lasering = () => {
    laser = !laser;
    document.body.classList.toggle("lasering", laser);
    pointAt(undefined);
    report(stage.at);
    void hover();
  };
  const over = (e: PointerEvent): Point => {
    const r = stage.canvas.getBoundingClientRect();
    return [(e.clientX - r.left) / r.width, (e.clientY - r.top) / r.height];
  };
  stage.canvas.addEventListener("pointermove", (e) => {
    pointer = over(e);
    if (laser) pointAt(pointer);
    void hover();
  });
  stage.canvas.addEventListener("pointerleave", () => {
    pointer = undefined;
    if (laser) pointAt(undefined);
    void hover();
  });
  addEventListener("resize", drawDot);

  outline(statesPicker, slots, stage.opened.outline);
  // The deck's own size by its shape (`16:9`) where a format is it, which it is then offered as.
  if (stage.opened.own) formatPicker.options[0].text = stage.opened.own;
  for (const id of stage.opened.formats) if (id !== stage.opened.own) formatPicker.add(new Option(id, id));
  statesPicker.onchange = () => {
    ending(false);
    stage.run(Number(statesPicker.value), 0, format(), still());
  };
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
    ending(false);
    const { index, t } = scrubTo(Number(scrub.value));
    void stage.seek(index, t, format());
  };
  $("#on").onclick = on;
  $("#back").onclick = back;
  $("#full").onclick = fullscreen;
  $("#present").onclick = present;
  const typing = keys({
    on,
    back,
    first: () => go(0),
    last: () => go(slots.length - 1),
    full: fullscreen,
    present,
    black: () => void blanking("black"),
    white: () => void blanking("white"),
    laser: lasering,
    leave,
    slide,
  });
  // A click or a tap goes on; a swipe goes either way: on the slide, and on the end of the show,
  // where a click leaves it.
  let down: PointerEvent | undefined;
  /** A press let go: a swipe goes on or back, and a click does `click`. */
  const lifted = (e: PointerEvent, click: () => void) => {
    if (!down) return;
    const [dx, dy] = [e.clientX - down.clientX, e.clientY - down.clientY];
    down = undefined;
    if (Math.abs(dx) > 40 && Math.abs(dx) > Math.abs(dy)) return dx < 0 ? on() : back();
    if (e.button === 0) click();
  };
  stage.canvas.onpointerdown = end.onpointerdown = (e) => (down = e);
  stage.canvas.onpointerup = (e) => lifted(e, () => void follow(e).then((followed) => followed || on()));
  end.onpointerup = (e) => lifted(e, on);
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
    ending(false);
    stage.run(to, 0, format(), still());
  });
  channel.onmessage = ({ data }: MessageEvent<Steer>) => {
    if (away) return;
    if (data.type === "on") on();
    else if (data.type === "back") back();
    else if (data.type === "first") go(0);
    else if (data.type === "last") go(slots.length - 1);
    else if (data.type === "black" || data.type === "white") void blanking(data.type);
    else if (data.type === "slide") slide(data.number);
    else if (data.type === "laser") lasering();
    else if (data.type === "point") pointAt(data.at);
    else if (data.type === "hello") {
      report(stage.at);
      pointAt(point);
    }
  };

  /** The bundle read again, and shown where the deck was: the same state, at rest. */
  let reloads = 0;
  const reload = async () => {
    const state = slots[stage.at.index]?.state;
    await stage.reload();
    document.title = titled(stage.opened);
    const chosen = formatPicker.value;
    const own = stage.opened.own;
    const formats = stage.opened.formats.filter((f) => f !== own);
    formatPicker.replaceChildren(new Option(own ?? "Own size", ""), ...formats.map((id) => new Option(id, id)));
    formatPicker.value = formats.includes(chosen) ? chosen : "";
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
        if (paths.some((path) => path !== "deck.scn")) void reload().catch((e) => (status.textContent = `Error: ${said(e)}`));
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
      /** The screen as the presenter turned it, black or white, or null (PLAN 2.105). */
      blank: () => blank ?? null,
      /** Whether the laser is on, and where it points on the slide, fractions, or null. */
      laser: () => ({ on: laser, point: point ?? null }),
      /** Whether the show is over, past the last state (PLAN 2.120). */
      ended: () => ended,
    },
  });
}

/** The state picker as the spine's outline (SPEC §3.11–3.12): a group for each section, in
 * the spine's order, each state named with its beat's claim; a deck without a spine lists its
 * states as they play. Each option's value is its slot. */
function outline(picker: HTMLSelectElement, slots: Slot[], sections: Section[]) {
  const slotOf = new Map(slots.map((slot, i) => [slot.state, i]));
  // Each as a person counts it (PLAN 2.99): Slide 3, Slide 3, step 2.
  const said = (state: string) => slots[slotOf.get(state) ?? -1]?.said ?? state;
  const option = (state: string, claim?: string) =>
    new Option(claim ? `${said(state)} · ${claim}` : said(state), String(slotOf.get(state)));
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

/** The presenter view: the deck as the player shows it, the next state, the notes, a timer that
 * pauses and resets, and the time of day. It steers the player, and follows it. */
async function presentView(deck: Source, how: Play, channel: BroadcastChannel) {
  document.body.className = "presenter";
  // The presenter's screen stays awake for as long as the view is open (lift-up REL-1).
  stayAwake();
  document.body.innerHTML = `
    <main><canvas id="stage"></canvas><p id="blanked" class="blanked" hidden></p></main>
    <aside>
      <div class="next"><canvas id="upnext"></canvas><p id="nextName"></p></div>
      <div class="clock"><span id="clock" title="The time since the timer began">0:00</span><button id="pause" type="button" title="Pause the timer">Pause</button><button id="reset" type="button" title="Put the timer back to 0:00">Reset</button><span id="now" title="The time of day"></span></div>
      <p id="where"></p>
      <div id="notes" class="notes"></div>
      <p class="buttons"><button id="back" aria-label="Back" disabled><span aria-hidden="true">◀</span> Back</button><button id="on" aria-label="Next" disabled><span aria-hidden="true">▶</span> Next</button></p>
      <p id="status" role="status">Loading…</p>
    </aside>`;
  const status = $("#status");
  const [now, upNext] = await Promise.all([
    Stage.open($("#stage"), deck, how.painter, how.engine),
    Stage.open($("#upnext"), deck, how.painter, how.engine),
  ]);
  let { states, notes } = now.opened;
  // The window is the deck's, marked as the presenter's, so that a window shared with the room
  // is told from it.
  const title = () => (document.title = `Presenter view · ${titled(now.opened)}`);
  title();
  // Each state as a person counts it (PLAN 2.99).
  let slots: Slot[] = await now.timeline();
  const steer = (message: Steer) => channel.postMessage(message);
  // The player's own keys, Home and End among them, steer it from here; and the presenter's
  // (PLAN 2.105): the room's screen black or white, a slide by its number, and the laser.
  const typing = keys({
    on: () => steer({ type: "on" }),
    back: () => steer({ type: "back" }),
    first: () => steer({ type: "first" }),
    last: () => steer({ type: "last" }),
    full: fullscreen,
    black: () => steer({ type: "black" }),
    white: () => steer({ type: "white" }),
    laser: () => steer({ type: "laser" }),
    // A number the deck has no slide for is said here, where the presenter typed it.
    slide: (number) => (numbered(slots, String(number)) ? steer({ type: "slide", number }) : typing.say(noSlide(slots, number))),
  });
  $("#on").onclick = () => steer({ type: "on" });
  $("#back").onclick = () => steer({ type: "back" });
  // Without a player to follow, this view says so, and how to have one: none answered its hello,
  // or the one it followed is gone. Its buttons wait until a player is heard.
  const buttons = [$<HTMLButtonElement>("#back"), $<HTMLButtonElement>("#on")];
  let heard = false;
  const alone = (words: string, again: string) => {
    if (heard) return;
    for (const button of buttons) button.disabled = true;
    const player = new URL(location.href);
    player.searchParams.delete("view");
    const link = document.createElement("a");
    link.href = player.href;
    link.target = "_blank";
    link.textContent = again;
    status.replaceChildren(`${words} `, link, ", and this view follows it.");
  };
  /** The wait for an answer to the last hello: a hello asked again ends the one before, so a player
   * that closes as this view opens is said to have closed, not never to have been open. */
  let waiting: ReturnType<typeof setTimeout> | undefined;
  /** Ask where the player is; with no answer in `ms`, say `words`. */
  const hello = (words: string, again: string, ms: number) => {
    heard = false;
    steer({ type: "hello" });
    clearTimeout(waiting);
    waiting = setTimeout(() => alone(words, again), ms);
  };
  // The laser as the player draws it on the room's screen, drawn here on the slide this view shows;
  // with it on, the pointer over that slide points it.
  const canvas = $<HTMLCanvasElement>("#stage");
  const dot = document.createElement("div");
  dot.className = "laser";
  dot.hidden = true;
  dot.setAttribute("aria-hidden", "true");
  document.body.append(dot);
  let laser = false;
  let point: Point | null = null;
  const drawDot = () => {
    dot.hidden = !laser || !point;
    if (!point) return;
    const r = canvas.getBoundingClientRect();
    dot.style.left = `${r.left + point[0] * r.width}px`;
    dot.style.top = `${r.top + point[1] * r.height}px`;
  };
  canvas.addEventListener("pointermove", (e) => {
    if (!laser) return;
    const r = canvas.getBoundingClientRect();
    steer({ type: "point", at: [(e.clientX - r.left) / r.width, (e.clientY - r.top) / r.height] });
  });
  canvas.addEventListener("pointerleave", () => laser && steer({ type: "point", at: null }));
  addEventListener("resize", drawDot);
  const blanked = $("#blanked");
  // The timer (PLAN 2.121), as a presentation app's presenter's: the time since this view opened,
  // which Pause stops and runs on, and Reset puts back to 0:00, running or paused as it was; and
  // beside it the time of day.
  /** When the timer last ran on, none while it is paused, and what it counted before that, ms. */
  let since: number | undefined = performance.now();
  let counted = 0;
  const elapsed = () => counted + (since === undefined ? 0 : performance.now() - since);
  const clock = () => {
    const s = Math.floor(elapsed() / 1000);
    $("#clock").textContent = `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
    $("#now").textContent = new Date().toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  };
  const pause = $<HTMLButtonElement>("#pause");
  const reset = $<HTMLButtonElement>("#reset");
  // A press on Pause or Reset leaves the keys where they were, as a presentation app's timer
  // does: Space and Return go on after it, and do not press it again. Tab reaches each, and then
  // its own keys press it.
  for (const button of [pause, reset]) button.onmousedown = (e) => e.preventDefault();
  pause.onclick = () => {
    if (since === undefined) since = performance.now();
    else [counted, since] = [elapsed(), undefined];
    pause.textContent = since === undefined ? "Resume" : "Pause";
    pause.title = since === undefined ? "Run the timer on" : "Pause the timer";
    document.body.classList.toggle("paused", since === undefined);
    clock();
  };
  reset.onclick = () => {
    counted = 0;
    if (since !== undefined) since = performance.now();
    clock();
  };
  clock();
  // A tick four times a second keeps the timer within a quarter second of the time it counts.
  setInterval(clock, 250);

  // The player sends a frame's place as it paints it; this view paints the latest, and lets
  // the ones it cannot keep up with go.
  let latest: Follow | undefined;
  let painting = false;
  const pane = $("#notes");
  /** The state and the format the next state was painted for, and the size it was painted at. */
  let next = "";
  let nextSize: [number, number] | null = null;
  const follow = async () => {
    if (painting) return;
    painting = true;
    try {
      while (latest) {
        const at: Follow = latest;
        latest = undefined;
        await now.show(states[at.index], at.t, at.format);
        $("#where").textContent = place(slots, at.index);
        // Other notes start at their top, however far the last were scrolled; a step that keeps its
        // slide's notes keeps them where they stood.
        const written = notes[at.index] || "No notes.";
        if (pane.textContent !== written) {
          pane.textContent = written;
          pane.scrollTop = 0;
        }
        // The next state, painted again for another state, or in the format the player turned to.
        const after = `${at.index} ${at.format ?? ""}`;
        if (after !== next) {
          next = after;
          const coming = states[at.index + 1];
          $("#nextName").textContent = coming ? `Next: ${slots[at.index + 1]?.said ?? coming}` : "The end of the deck.";
          // After the last state nothing comes: no slide stands where the next would.
          $("#upnext").hidden = !coming;
          nextSize = coming ? (await upNext.show(coming, undefined, at.format)).size : null;
        }
        // A frame painted once the player has gone leaves what this view says of it.
        if (heard) status.textContent = "";
      }
    } catch (e) {
      status.textContent = `Error: ${said(e)}`;
    } finally {
      painting = false;
    }
  };
  channel.onmessage = ({ data }: MessageEvent<Follow | Pointed | Gone>) => {
    if (data.type === "pointed") {
      ({ laser, point } = data);
      return drawDot();
    }
    // The player closed, or is loading again: another player of the deck answers, or none does.
    if (data.type === "gone") return hello("The player has closed.", "Open it again", 1000);
    if (data.type !== "at") return;
    if (!heard) {
      heard = true;
      for (const button of buttons) button.disabled = false;
    }
    // What the room sees while its screen is black or white, or the show is over, said over the
    // slide shown here.
    const sees = data.blank
      ? `The room sees a ${data.blank} screen. A key or a click brings the slide back.`
      : data.ended
        ? "The room sees the end of the show. Next leaves it, and Back shows the last slide."
        : "";
    blanked.hidden = !sees;
    blanked.textContent = sees;
    document.body.classList.toggle("blanking", !!sees);
    document.body.classList.toggle("lasering", !!data.laser);
    latest = data;
    void follow();
  };
  // Served (PLAN 2.11): the bundle changed on disk, so this view reads it again, as the player
  // does, and then follows the player where it shows the deck.
  if (served)
    listen({
      changed: (paths) => {
        if (!paths.some((path) => path !== "deck.scn")) return;
        void Promise.all([now.reload(), upNext.reload()]).then(async () => {
          ({ states, notes } = now.opened);
          title();
          slots = await now.timeline();
          next = "";
          steer({ type: "hello" });
        });
      },
    });
  hello("The player is not open.", "Open it", 2000);
  Object.assign(window, {
    scaena: {
      ...now.opened,
      follows: () => $("#where").textContent,
      /** What this view says the room sees, while its screen is black or white (PLAN 2.105) or the
       * show is over (PLAN 2.120). */
      blanked: () => (blanked.hidden ? null : blanked.textContent),
      /** The timer as it reads, and whether it is paused (PLAN 2.121). */
      timer: () => ({ shows: $("#clock").textContent, paused: since === undefined, now: $("#now").textContent }),
      /** Where the laser points on the slide, as the player says, or null. */
      pointed: () => (laser ? point : null),
      /** The size the next state is painted at, in the format the player shows, or null after the
       * last. */
      coming: () => nextSize,
    },
  });
}
