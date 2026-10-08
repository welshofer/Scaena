// The light table (PLAN 2.97, ADR-0013): every slide of the deck in a grid, each drawn at its
// last state, at rest, as the PDF draws it, to reorder, copy, and take out across a large deck.
// The page lays nothing out and paints nothing itself.
//
// - Slides shows it in place of the preview; Slides again, or Escape, puts the preview back.
// - A click selects a slide; Shift+click selects every slide from the one last clicked to it; ⌘-click
//   (Ctrl) adds one to the selection, or takes it away. The arrow keys move among the slides, a row
//   at a time with ↑ and ↓, and Home and End; Space adds the slide focused, or takes it away.
// - Enter, or a double click, shows the slide's first state in the canvas, and the preview back.
// - The slides selected, dragged among the others, go before or after the slide under the pointer,
//   by which half of it the pointer is over; Alt with ← or → moves them a place. ⌘D (Ctrl+D)
//   copies them, each just after itself, and Delete (or Backspace) takes them out. Each is one
//   patch of `move_slide`, `duplicate_slide`, or `remove_slide`, by the user, one step to undo, and
//   every other state shows what it showed (`scaena_core::tracking::keep_looks`).
// - Each slide says its number, its id, how many steps it has, and its findings in the format
//   shown, counted in the color of the worst as the strip counts them (PLAN 2.49).
import { ALT, type Key, MOD, SHIFT } from "./commands";
import { counted, worst } from "./marks";
import type { Edited, Finding, Slot } from "./protocol";
import type { Stage } from "./stage";

/** What the light table answers, focused (PLAN 2.65), as the keys sheet lists it. */
export const slidesKeys = (): Key[] => [
  { keys: "← ↑ → ↓ Home End", label: "In the light table: move among the slides", group: "Slides" },
  { keys: `${MOD}Click, Space`, label: "Add a slide to the selection, or take it away", group: "Slides" },
  { keys: `${SHIFT}Click`, label: "Select the slides from the one last clicked", group: "Slides" },
  { keys: `${ALT}← ${ALT}→`, label: "Move the slides selected a place", group: "Slides" },
  { keys: `${MOD}D`, label: "Copy the slides selected, each just after itself", group: "Slides" },
  { keys: "Delete", label: "Take the slides selected out", group: "Slides" },
  { keys: "Enter", label: "Show the slide in the canvas", group: "Slides" },
  { keys: "Escape", label: "Close the light table", group: "Slides" },
];

/** What the light table asks of the editor around it. */
export interface SlidesEditor {
  /** The state shown, by its id and its slot's index; none while the source does not compile. */
  shown(): { state: string; index: number } | undefined;
  format(): string | undefined;
  source(): string;
  /** Show the state at slot `index`, as the state picker does. */
  show(index: number): void;
  /** Take `source`, a patch's, as one change: one step to undo. */
  apply(source: string, edited: Edited): void;
  /** The light table opened or closed: the preview, hidden or shown again. */
  opened(open: boolean): void;
  say(text: string): void;
}

/** A slide: its first state's id, which names it, and its states in order. */
interface Slide {
  id: string;
  states: string[];
}

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
const html = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

/** How high a slide is painted, CSS pixels. */
const HIGH = 126;
/** How many pixels high a slide is painted: `HIGH` at the display's density, at most twice. */
const high = () => Math.round(HIGH * Math.min(2, window.devicePixelRatio || 1));

/** The slides `slots` hold, in order: the states each builds on, together. */
export function slidesOf(slots: Slot[]): Slide[] {
  const out: Slide[] = [];
  for (const s of slots) {
    const id = s.slide ?? s.state;
    const last = out.at(-1);
    if (last?.id === id) last.states.push(s.state);
    else out.push({ id, states: [s.state] });
  }
  return out;
}

/** The light table in `into` (its `ol` the slides), over `stage`; `button` opens and closes it. */
export function slides(stage: Stage, into: HTMLElement, button: HTMLButtonElement, editor: SlidesEditor) {
  const list = into.querySelector<HTMLOListElement>("ol")!;
  let all: Slide[] = [];
  /** The slides selected, by id, and the one a Shift+click selects from. */
  let chosen = new Set<string>();
  let anchor: string | undefined;
  /** The slide with the focus, by id: the one the keys act from. */
  let focus: string | undefined;
  /** Each last state's picture, by state: what identifies its drawing, and its pixels. */
  const pictures = new Map<string, { digest: string; image: ImageData }>();
  /** Each state's findings in the format shown, by id. */
  let findings = new Map<string, Finding[]>();
  /** One change at a time, each made on the source the one before left. */
  let making: Promise<unknown> = Promise.resolve();
  /** Pictures asked for: the last answer wins. */
  let painting = 0;
  /** The slides a drag carries. */
  let dragging: string[] | undefined;
  /** What a change selects once the table has the deck it made: the slides `keep` names then.
   * `from` is the states as they were, which the deck the change made does not have. */
  let pending: { from: string; keep: () => string[] } | undefined;
  /** The states in order, as `pending` compares them. */
  const order_ = () => all.map((s) => s.states.join(",")).join(" ");
  const open = () => !into.hidden;

  /** What a slide's tile says of its findings: how many, in the color of the worst. */
  function tally(slide: Slide) {
    const found = slide.states.flatMap((s) => findings.get(s) ?? []);
    if (!found.length) return "";
    const says = counted(found);
    return `<span class="found ${worst(found)}" title="${html(says)}"><span aria-hidden="true">${found.length}</span><span class="sr">, ${html(says)}</span></span>`;
  }

  /** The tiles, as `all` has the slides, `chosen` selected and `focus` focusable. */
  function render() {
    const had = list.contains(document.activeElement);
    chosen = new Set([...chosen].filter((id) => all.some((s) => s.id === id)));
    if (!all.some((s) => s.id === focus)) focus = [...chosen][0] ?? all[0]?.id;
    list.innerHTML = all
      .map((s, i) => {
        const steps = s.states.length > 1 ? `${s.states.length} steps` : "";
        const label = `${i + 1}, ${s.id}${steps ? `, ${steps}` : ""}`;
        return `<li role="option" data-slide="${html(s.id)}" aria-selected="${chosen.has(s.id)}" aria-label="${html(label)}" tabindex="${s.id === focus ? 0 : -1}" draggable="true">
          <canvas aria-hidden="true"></canvas><span class="n" aria-hidden="true">${i + 1}</span><span class="id" aria-hidden="true">${html(s.id)}</span>${
            steps ? `<span class="steps" aria-hidden="true">${steps}</span>` : ""
          }${tally(s)}</li>`;
      })
      .join("");
    for (const s of all) draw(s);
    if (had) tile(focus)?.focus();
  }

  const tile = (id: string | undefined) => (id === undefined ? null : list.querySelector<HTMLElement>(`li[data-slide="${CSS.escape(id)}"]`));
  const lastOf = (s: Slide) => s.states[s.states.length - 1];

  /** `slide`'s picture onto its tile, if there is one yet. */
  function draw(slide: Slide) {
    const canvas = tile(slide.id)?.querySelector("canvas");
    const picture = pictures.get(lastOf(slide));
    if (!canvas || !picture) return;
    [canvas.width, canvas.height] = [picture.image.width, picture.image.height];
    canvas.getContext("2d")?.putImageData(picture.image, 0, 0);
  }

  /** Ask the engine for the pictures whose drawing changed, each slide's last state, and draw
   * them. Nothing while the light table is closed. */
  async function paint() {
    if (!open() || !all.length) return;
    const asked = ++painting;
    const known = Object.fromEntries([...pictures].map(([id, p]) => [id, p.digest]));
    try {
      const got = await stage.thumbnails(high(), known, editor.format(), all.map(lastOf));
      if (asked !== painting) return;
      for (const t of got) {
        if (!t.pixels || !t.width || !t.height) continue;
        pictures.set(t.state, { digest: t.digest, image: new ImageData(new Uint8ClampedArray(t.pixels), t.width, t.height) });
      }
      for (const s of all) draw(s);
    } catch {
      // The deck changed under it: the next paint asks again.
    }
  }

  /** The deck's slides, as `slots` hold its states. */
  function states(slots: Slot[]) {
    all = slidesOf(slots);
    for (const id of [...pictures.keys()]) if (!slots.some((s) => s.state === id)) pictures.delete(id);
    if (pending && order_() !== pending.from) {
      const kept = pending.keep().filter((id) => all.some((s) => s.id === id));
      pending = undefined;
      if (kept.length) [chosen, focus, anchor] = [new Set(kept), kept[0], kept[0]];
    }
    if (open()) {
      render();
      void paint();
    }
  }

  /** What lint found: each state's findings in the format shown, counted on its slide. */
  function found(every: Finding[]) {
    findings = new Map();
    for (const f of every) if (f.shown && f.state !== undefined) findings.set(f.state, [...(findings.get(f.state) ?? []), f]);
    if (!open()) return;
    for (const s of all) {
      const li = tile(s.id);
      li?.querySelector(".found")?.remove();
      li?.insertAdjacentHTML("beforeend", tally(s));
    }
  }

  /** Show it, or put the preview back. */
  function toggle(on = !open()) {
    if (on === open()) return;
    into.hidden = !on;
    button.setAttribute("aria-pressed", String(on));
    editor.opened(on);
    if (!on) return;
    // The slide shown is selected, and has the focus.
    const shown = editor.shown();
    const slide = shown && all.find((s) => s.states.includes(shown.state));
    if (slide) [chosen, anchor, focus] = [new Set([slide.id]), slide.id, slide.id];
    render();
    tile(focus)?.focus();
    tile(focus)?.scrollIntoView({ block: "nearest" });
    void paint();
    editor.say(`${all.length} slides: select them, drag them, ${ALT}← ${ALT}→ moves them, ${MOD}D copies them, Delete takes them out · Enter shows one`);
  }

  /** The slides selected, in the deck's order. */
  const picked = () => all.filter((s) => chosen.has(s.id));

  /** Select `ids`, focusing `to`. */
  function choose(ids: string[], to: string) {
    chosen = new Set(ids);
    focus = to;
    for (const li of list.querySelectorAll<HTMLElement>("li[data-slide]")) {
      li.setAttribute("aria-selected", String(chosen.has(li.dataset.slide!)));
      li.tabIndex = li.dataset.slide === to ? 0 : -1;
    }
    tile(to)?.focus();
    tile(to)?.scrollIntoView({ block: "nearest" });
  }

  /** A click on slide `id`: `range`, from the one last clicked; `add`, in or out of the
   * selection; else it alone. */
  function click(id: string, range: boolean, add: boolean) {
    if (range && anchor !== undefined) {
      const [a, b] = [all.findIndex((s) => s.id === anchor), all.findIndex((s) => s.id === id)];
      return choose(all.slice(Math.min(a, b), Math.max(a, b) + 1).map((s) => s.id), id);
    }
    anchor = id;
    if (add) return choose(chosen.has(id) ? [...chosen].filter((c) => c !== id) : [...chosen, id], id);
    choose([id], id);
  }

  /** Show slide `id` in the canvas: its first state, and the preview back. */
  function showSlide(id: string) {
    const slide = all.find((s) => s.id === id);
    if (!slide) return;
    toggle(false);
    editor.show(all.slice(0, all.indexOf(slide)).reduce((n, s) => n + s.states.length, 0));
  }

  /** Make `ops` on the source as it stands: one change to undo. The deck after it shows the
   * state shown before it, where it has it still, else the first of the slides `keep` names;
   * then those are selected. */
  function make(ops: unknown[], doing: string, done: string, keep: () => string[]) {
    const run = async () => {
      const shown = editor.shown();
      if (!shown) return editor.say("the light table waits for a source that compiles");
      editor.say(doing);
      try {
        // The slot the state shown will have, from the order the ops leave.
        const ids = order(ops);
        const at = ids.indexOf(shown.state);
        const index = at >= 0 ? at : Math.max(0, ids.indexOf(keep()[0] ?? ""));
        const { source, edited } = await stage.make(editor.source(), ops, index, editor.format());
        // Selected once the deck the change made reaches the table.
        pending = { from: order_(), keep };
        editor.apply(source, edited);
        editor.say(done);
      } catch (e) {
        pending = undefined;
        editor.say(`not made: ${said(e)}`);
      }
    };
    making = making.then(run, run);
    return making;
  }

  /** The states' ids in the order `ops` leave them, copies as they come: where the state shown
   * lands. */
  function order(ops: unknown[]) {
    let slides = all.map((s) => ({ ...s, states: [...s.states] }));
    for (const op of ops as { op: string; slide: string; after?: string; before?: string }[]) {
      const at = slides.findIndex((s) => s.id === op.slide);
      if (at < 0) continue;
      if (op.op === "remove_slide") slides.splice(at, 1);
      else if (op.op === "duplicate_slide") slides.splice(at + 1, 0, { id: `${op.slide} copy`, states: slides[at].states.map((s) => `${s} copy`) });
      else if (op.op === "move_slide") {
        const [moved] = slides.splice(at, 1);
        const there = slides.findIndex((s) => s.id === (op.after ?? op.before));
        slides.splice(there + (op.after ? 1 : 0), 0, moved);
      }
    }
    return slides.flatMap((s) => s.states);
  }

  /** The slides selected before or after slide `to`, in their order, together. */
  function move(where: "before" | "after", to: string) {
    const moving = picked().map((s) => s.id);
    if (!moving.length || moving.includes(to)) return;
    const ops = moving.map((slide, i) => (i === 0 ? { op: "move_slide", slide, [where]: to } : { op: "move_slide", slide, after: moving[i - 1] }));
    const what = moving.length > 1 ? `${moving.length} slides` : moving[0];
    return make(ops, "moving…", `${what} moved ${where} ${to} · each slide shows what it showed`, () => moving);
  }

  /** The slides selected a place before (`-1`) or after (`1`), past the slide beside them. */
  function step(by: -1 | 1) {
    const moving = picked();
    if (!moving.length) return;
    const ends = by < 0 ? all.indexOf(moving[0]) - 1 : all.indexOf(moving[moving.length - 1]) + 1;
    const beside = all[ends];
    if (!beside || chosen.has(beside.id)) return editor.say(by < 0 ? "the first slide goes no further" : "the last slide goes no further");
    return move(by < 0 ? "before" : "after", beside.id);
  }

  /** A copy of each slide selected, just after it. */
  function duplicate() {
    const copying = picked().map((s) => s.id);
    if (!copying.length) return;
    const what = copying.length > 1 ? `${copying.length} slides` : copying[0];
    // The copies selected: each the slide just after the one it copies.
    const copies = () => copying.flatMap((id) => all[all.findIndex((s) => s.id === id) + 1]?.id ?? []);
    return make(
      copying.map((slide) => ({ op: "duplicate_slide", slide })),
      "copying…",
      `${what} copied, each just after itself · an edit to a copy leaves the slide it copies`,
      copies,
    );
  }

  /** The slides selected, taken out; the slide after them selected. */
  function remove() {
    const going = picked().map((s) => s.id);
    if (!going.length) return;
    if (going.length === all.length) return editor.say("a deck keeps at least one slide");
    const after = all.slice(all.findIndex((s) => s.id === going[going.length - 1]) + 1).find((s) => !chosen.has(s.id));
    const before = [...all].reverse().find((s) => !chosen.has(s.id));
    const next = (after ?? before)!.id;
    const what = going.length > 1 ? `${going.length} slides` : going[0];
    return make(
      going.map((slide) => ({ op: "remove_slide", slide })),
      "taking out…",
      `${what} taken out · every other slide shows what it showed`,
      () => [next],
    );
  }

  const item = (e: Event) => (e.target as Element).closest<HTMLElement>("li[data-slide]");
  list.onclick = (e) => {
    const li = item(e);
    if (li) click(li.dataset.slide!, e.shiftKey, e.metaKey || e.ctrlKey);
  };
  list.ondblclick = (e) => {
    const li = item(e);
    if (li) showSlide(li.dataset.slide!);
  };
  /** How many tiles stand in a row. */
  const across = () => {
    const tiles = [...list.children] as HTMLElement[];
    const top = tiles[0]?.offsetTop;
    return Math.max(1, tiles.filter((t) => t.offsetTop === top).length);
  };
  into.onkeydown = (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      toggle(false);
      return;
    }
    const li = item(e);
    if (!li) return;
    const id = li.dataset.slide!;
    const at = all.findIndex((s) => s.id === id);
    const mod = e.metaKey || e.ctrlKey;
    if (e.altKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
      e.preventDefault();
      void step(e.key === "ArrowLeft" ? -1 : 1);
      return;
    }
    const steps: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -across(), ArrowDown: across(), Home: -at, End: all.length - 1 - at };
    if (e.key in steps && !mod) {
      e.preventDefault();
      const to = all[Math.max(0, Math.min(all.length - 1, at + steps[e.key]))];
      if (!to) return;
      if (e.shiftKey) {
        anchor ??= id;
        const a = all.findIndex((s) => s.id === anchor);
        const b = all.indexOf(to);
        return choose(all.slice(Math.min(a, b), Math.max(a, b) + 1).map((s) => s.id), to.id);
      }
      anchor = to.id;
      return choose([to.id], to.id);
    }
    if (e.key === " ") {
      e.preventDefault();
      anchor = id;
      return choose(chosen.has(id) ? [...chosen].filter((c) => c !== id) : [...chosen, id], id);
    }
    if (e.key === "Enter") {
      e.preventDefault();
      return showSlide(id);
    }
    if (mod && e.key.toLowerCase() === "d" && !e.shiftKey) {
      e.preventDefault();
      if (!chosen.has(id)) choose([id], id);
      return void duplicate();
    }
    if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      if (!chosen.has(id)) choose([id], id);
      return void remove();
    }
  };

  // The slides selected, dragged among the others: they drop before or after the slide under the
  // pointer, by which half of it the pointer is over.
  const half = (e: DragEvent, li: HTMLElement) => {
    const r = li.getBoundingClientRect();
    return e.clientX < r.left + r.width / 2 ? "before" : "after";
  };
  const unmark = () => list.querySelectorAll(".drop-before, .drop-after").forEach((li) => li.classList.remove("drop-before", "drop-after"));
  list.ondragstart = (e) => {
    const li = item(e);
    if (!li || !e.dataTransfer) return;
    const id = li.dataset.slide!;
    if (!chosen.has(id)) choose([id], id);
    dragging = picked().map((s) => s.id);
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", dragging.join(" "));
  };
  list.ondragover = (e) => {
    const li = item(e);
    if (!li || dragging === undefined) return;
    e.preventDefault();
    unmark();
    if (!dragging.includes(li.dataset.slide!)) li.classList.add(`drop-${half(e, li)}`);
  };
  list.ondragleave = () => unmark();
  list.ondrop = (e) => {
    const li = item(e);
    const carried = dragging;
    unmark();
    dragging = undefined;
    if (!li || !carried || carried.includes(li.dataset.slide!)) return;
    e.preventDefault();
    void move(half(e, li), li.dataset.slide!);
  };
  list.ondragend = () => {
    dragging = undefined;
    unmark();
  };
  button.onclick = () => toggle();

  return {
    states,
    found,
    paint,
    toggle,
    /** The format changed: every picture is painted again. */
    reformat: () => {
      pictures.clear();
      void paint();
    },
    open,
    move,
    step,
    duplicate,
    remove,
    /** The slides as it lists them, each its states, and those selected, for a test. */
    slides: () => all.map((s) => ({ ...s, selected: chosen.has(s.id) })),
    /** Select these slides, for a test, as clicks would. */
    select: (ids: string[]) => ids.length && choose(ids, ids[0]),
    /** The changes made through it, for a test: once they are. */
    settled: () => making,
    /** How many slides it has drawn, for a test. */
    drawn: () => all.filter((s) => pictures.has(lastOf(s))).length,
  };
}
