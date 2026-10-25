// The state strip (PLAN 2.35, ADR-0013): the deck's states in order, each a thumbnail the engine
// paints at rest, with its cue's length. The page lays nothing out and paints nothing itself. Each
// is named as a person counts it (PLAN 2.99): a slide by its number, a step after a slide's first
// by its place in the slide (`Slot.said`).
//
// - A click on a state, or the arrow keys, Home, and End among them, shows it.
// - New step adds a state after the state shown that tracks from it, in its slide: it shows what
//   that state shows until it is changed. New slide opens the gallery of the theme's layouts
//   (PLAN 3.30): each drawn as a slide started in it, its words in its slots, by the theme's
//   sections, then a blank slide; a click starts that slide after the last step of the slide shown.
// - A state dragged to another place, or moved with Alt and an arrow key, moves there
//   (`move_state`), and Delete removes it (`remove_state`). No id names it here, so none is
//   renamed here: the source names it.
// - ⌘D copies the slide of the state focused, just after itself (`duplicate_slide`, as the light
//   table copies one), and keeps the key from the browser, whose ⌘D adds a bookmark.
// - Each is one patch, by the user, one step to undo; what the deck refuses, the status says why.
// - A thumbnail is painted again only when its state's drawing changed (the digest of its display
//   list), once every state is laid out after an edit.
// - Each state with findings in the format shown says how many, in the color of the worst (PLAN
//   2.49).
import { ALT, MOD, type Key } from "./commands";
import { counted, worst } from "./marks";
import type { Edited, Finding, Slot, Thumb } from "./protocol";
import type { Picking } from "./picker";
import type { Stage } from "./stage";

/** What a state in the strip answers, focused (PLAN 2.65), as the keys sheet lists it. */
export const stripKeys = (): Key[] => [
  { keys: "← → Home End", label: "Among the slides: show the slide or step before or after, the first, or the last", group: "Slides" },
  { keys: `${ALT}← ${ALT}→`, label: "Move the slide or step before or after the one beside it", group: "Slides" },
  { keys: "Delete", label: "Delete the slide or step", group: "Slides" },
  { keys: `${MOD}D`, label: "Duplicate the slide, just after itself", group: "Slides" },
  { keys: "Enter Space", label: "Show the slide or step", group: "Slides" },
];

/** What the strip asks of the editor around it. */
export interface StripEditor {
  /** The state shown, by its id and its slot's index; none while the source does not compile. */
  shown(): { state: string; index: number } | undefined;
  format(): string | undefined;
  source(): string;
  /** Show the state at slot `index`, as the state picker does. */
  show(index: number): void;
  /** Take `source`, a patch's, as one change: one step to undo. */
  apply(source: string, edited: Edited, said?: string): void;
  say(text: string): void;
  /** A slide made here, from the gallery or after the slide shown: the canvas outlines its
   * layout's empty slots (PLAN 3.30). */
  started?(state: string): void;
  /** A layout chosen from the theme's by their sections (PLAN 3.31): null a blank slide, undefined
   * none. */
  pick(picking: Picking): Promise<string | null | undefined>;
}

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
/** What the strip says while the source does not compile: nothing it does can be made. */
const WAITING = "Nothing changed: the source has an error to fix first";
const html = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
/** A cue's length, as the strip says it. */
const seconds = (ms: number) => `${(ms / 1000).toFixed(ms < 10000 ? 1 : 0)} s`;

/** How high a thumbnail is painted, CSS pixels, on a display of `ratio` device pixels each. */
const HIGH = 54;
/** How many pixels high a thumbnail is painted: `HIGH` at the display's density, at most twice. */
const high = () => Math.round(HIGH * Math.min(2, window.devicePixelRatio || 1));

/** Mark `canvas`'s tile as a slide that could not be painted, in words over it, or take the mark
 * away (lift-up STAB-4): its thumbnail is left blank. */
export function unpainted(canvas: HTMLCanvasElement, failed: boolean) {
  const mark = canvas.parentElement?.querySelector(":scope > .unpainted");
  if (!failed) return mark?.remove();
  canvas.getContext("2d")?.clearRect(0, 0, canvas.width, canvas.height);
  if (!mark) canvas.insertAdjacentHTML("afterend", `<span class="unpainted">This slide couldn’t be drawn</span>`);
}

/** The strip in `into` (its `ol` the states, its buttons the strip's own), over `stage`. */
export function strip(stage: Stage, into: HTMLElement, editor: StripEditor) {
  const list = into.querySelector<HTMLOListElement>("ol")!;
  /** Each state's thumbnail, by id: what identifies its drawing, and its pixels. */
  const thumbs = new Map<string, { digest: string; image?: ImageData }>();
  let slots: Slot[] = [];
  /** The state a drag carries, and where it would drop: before or after which. */
  let dragging: string | undefined;
  /** One change at a time, each made on the source the one before left. */
  let making: Promise<unknown> = Promise.resolve();
  /** Thumbnails asked for: the last answer wins. */
  let painting = 0;
  /** Each state's findings in the format shown, by id (PLAN 2.49). */
  let findings = new Map<string, Finding[]>();

  /** What a state's item says of its findings: how many, in the color of the worst. */
  function tally(state: string) {
    const all = findings.get(state) ?? [];
    if (!all.length) return "";
    const said = counted(all);
    return `<span class="found ${worst(all)}" title="${html(said)}"><span aria-hidden="true">${all.length}</span><span class="sr">, ${html(said)}</span></span>`;
  }

  /** The item for slot `s`, selected where `on`: its canvas empty. */
  function tile(s: Slot, on: boolean): HTMLLIElement {
    // A slide by its number, a step after its first by its place in the slide.
    const said = s.said ?? s.state;
    const step = said.split(", ")[1];
    const name = step ? `${step[0].toUpperCase()}${step.slice(1)}` : said;
    const made = document.createElement("template");
    made.innerHTML = `<li role="option" id="strip-${html(s.state)}" data-state="${html(s.state)}" aria-selected="${on}" tabindex="${on ? 0 : -1}" draggable="true" title="${html(said)}: it plays for ${seconds(s.span)}">
          <canvas aria-hidden="true"></canvas><span class="id"${step ? ` aria-label="${html(said)}"` : ""}>${html(name)}</span><span class="cue">${seconds(s.span)}</span>${tally(s.state)}</li>`;
    return made.content.firstElementChild as HTMLLIElement;
  }

  /** The states as `edited` says the deck has them, `shown` the one shown. Each state's item that
   * stays is kept, its canvas and what it drew with it, its words, cue, and selection said again
   * in place; only the items of states new to the strip are made and drawn (lift-up 10-24b
   * PERF-5), and a drawing that changed is drawn again as `paint` takes it. */
  function states(next: Slot[], shown: number) {
    slots = next;
    for (const id of [...thumbs.keys()]) if (!next.some((s) => s.state === id)) thumbs.delete(id);
    const focused = list.contains(document.activeElement);
    const had = new Map([...list.querySelectorAll<HTMLLIElement>(":scope > li[data-state]")].map((li) => [li.dataset.state!, li]));
    const made: string[] = [];
    const items = next.map((s, i) => {
      const fresh = tile(s, i === shown);
      const kept = had.get(s.state);
      if (!kept) {
        made.push(s.state);
        return fresh;
      }
      for (const name of ["aria-selected", "tabindex", "title"]) kept.setAttribute(name, fresh.getAttribute(name)!);
      // Its canvas stays, with the mark of a slide that could not be painted; the rest is said again.
      const canvas = kept.querySelector(":scope > canvas")!;
      const mark = kept.querySelector(":scope > .unpainted");
      kept.replaceChildren(canvas, ...(mark ? [mark] : []), ...[...fresh.children].filter((c) => c.tagName !== "CANVAS"));
      return kept;
    });
    if (items.length !== list.children.length || items.some((li, i) => list.children[i] !== li)) list.replaceChildren(...items);
    for (const state of made) draw(state);
    const shownItem = list.children[shown] as HTMLElement | undefined;
    inView(shownItem);
    if (focused) shownItem?.focus();
  }

  /** `item` scrolled into the strip's view, as `scrollIntoView` scrolls it to the nearest edge, by
   * the strip's own scroll: Chromium starts the page's next Tab at what `scrollIntoView` scrolls
   * to, which would take a page's first Tab past the bar into the strip (PLAN 2.126). */
  function inView(item: HTMLElement | undefined) {
    if (!item) return;
    const [strip, at] = [list.getBoundingClientRect(), item.getBoundingClientRect()];
    if (at.left < strip.left) list.scrollLeft -= strip.left - at.left;
    else if (at.right > strip.right) list.scrollLeft += at.right - strip.right;
  }

  /** The state shown is slot `shown`: it is the strip's selection, and takes the focus if the
   * strip has it. */
  function select(shown: number) {
    const focused = list.contains(document.activeElement) && document.activeElement?.tagName !== "INPUT";
    for (const [i, li] of [...list.children].entries()) {
      li.setAttribute("aria-selected", String(i === shown));
      (li as HTMLElement).tabIndex = i === shown ? 0 : -1;
    }
    const item = list.children[shown] as HTMLElement | undefined;
    inView(item);
    if (focused) item?.focus();
  }

  /** `state`'s thumbnail onto its item, if there is one yet. */
  function draw(state: string) {
    const canvas = list.querySelector<HTMLCanvasElement>(`li[data-state="${CSS.escape(state)}"] canvas`);
    const thumb = thumbs.get(state);
    if (!canvas || !thumb) return;
    unpainted(canvas, !thumb.image);
    if (!thumb.image) return;
    [canvas.width, canvas.height] = [thumb.image.width, thumb.image.height];
    canvas.getContext("2d")?.putImageData(thumb.image, 0, 0);
  }

  /** Ask the engine for the thumbnails whose drawing changed, and draw them: whether they were
   * drawn, which a newer paint begun meanwhile does in its place. */
  async function paint(): Promise<boolean> {
    const asked = ++painting;
    const known = Object.fromEntries([...thumbs].map(([id, t]) => [id, t.digest]));
    const take = (t: Thumb) => {
      // One that could not be painted keeps its drawing's digest, so it is asked for again only
      // once its drawing changes, and its tile says so (lift-up STAB-4).
      if (t.failed !== undefined) thumbs.set(t.state, { digest: t.digest });
      else if (!t.pixels || !t.width || !t.height) return;
      else thumbs.set(t.state, { digest: t.digest, image: new ImageData(new Uint8ClampedArray(t.pixels), t.width, t.height) });
      draw(t.state);
    };
    let got: Thumb[];
    try {
      // Each drawn as it is painted, first to last, the strip's first ones seen first.
      got = await stage.thumbnails(high(), known, editor.format(), undefined, (t) => asked === painting && take(t));
    } catch {
      return true;
    }
    if (asked !== painting) return false;
    for (const t of got) take(t);
    return true;
  }

  /** Paint until a paint lands that no newer one began after: the thumbnails are then the deck's
   * as it stands, which an assistant's question draws its edits against (PLAN 2.93). A paint a
   * newer one stops draws nothing, so waiting on it alone could leave a state undrawn. */
  async function painted() {
    while (!(await paint()));
  }

  /** What lint found now: each state's findings in the format shown, counted on its item. */
  function found(all: Finding[]) {
    findings = new Map();
    for (const f of all) if (f.shown && f.state !== undefined) findings.set(f.state, [...(findings.get(f.state) ?? []), f]);
    for (const li of list.querySelectorAll<HTMLElement>("li[data-state]")) {
      li.querySelector(".found")?.remove();
      li.insertAdjacentHTML("beforeend", tally(li.dataset.state!));
    }
  }

  /** The format changed: every thumbnail is painted again. */
  function reformat() {
    thumbs.clear();
    void paint();
  }

  /** Make `ops` on the source as it stands: one change to undo. The deck after it shows slot
   * `to`: `to(slots)` of the slots as they are, which the patch is made on. Then say `done`. */
  function make(ops: unknown[], doing: string, done: string, to: (slots: Slot[]) => number) {
    const run = async () => {
      if (!editor.shown()) return editor.say(WAITING);
      editor.say(doing);
      try {
        const { source, edited } = await stage.make(editor.source, ops, Math.max(0, to(slots)), editor.format());
        editor.apply(source, edited, done);
        editor.say(done);
      } catch (e) {
        editor.say(`Not changed: ${said(e)}`);
      }
    };
    making = making.then(run, run);
    return making;
  }

  /** A state after the one shown: a `step` of its slide, or a `slide` of its own. */
  function add(what: "step" | "slide") {
    const shown = editor.shown();
    if (!shown) return editor.say(WAITING);
    return stage
      .addingState(editor.source(), shown.state, what)
      .then((added) => {
        const after = (added.patch[0] as { after?: string }).after;
        if (what === "slide") editor.started?.(added.id);
        const done = what === "step" ? `A step added after ${named(shown.state)}` : `A slide added after ${slide(shown.state)}`;
        return make(added.patch, `Adding a ${what}…`, done, (all) => all.findIndex((s) => s.state === after) + 1);
      })
      .catch((e) => editor.say(`Not added: ${said(e)}`));
  }

  /** The slides to start (PLAN 3.30, 3.31): the theme's layouts by their sections, each drawn as
   * a slide started in it, or a blank one. One chosen starts after the slide shown. */
  async function gallery() {
    const shown = editor.shown();
    if (!shown) return editor.say(WAITING);
    const picked = await editor.pick({ purpose: "start", source: editor.source(), state: shown.state, format: editor.format() });
    if (picked !== undefined) await start(picked ?? undefined);
  }

  /** A slide started in `layout`, or a blank one with none, after the slide shown (PLAN 3.30). */
  function start(layout: string | undefined) {
    const shown = editor.shown();
    if (!shown) return editor.say(WAITING);
    return stage
      .starting(editor.source(), shown.state, layout)
      .then((added) => {
        const after = (added.patch[0] as { after?: string }).after;
        editor.started?.(added.id);
        return make(added.patch, "Adding a slide…", `A slide added after ${slide(shown.state)}`, (all) => all.findIndex((s) => s.state === after) + 1);
      })
      .catch((e) => editor.say(`Not added: ${said(e)}`));
  }

  /** Move `state` before or after `to`. */
  function move(state: string, where: "before" | "after", to: string) {
    if (state === to) return;
    const lands = (all: Slot[]) => {
      const rest = all.map((s) => s.state).filter((id) => id !== state);
      return rest.indexOf(to) + (where === "after" ? 1 : 0);
    };
    return make([{ op: "move_state", id: state, [where]: to }], "Moving…", `${named(state)} moved ${where} ${named(to)}`, lands);
  }

  /** Remove `state`: the state after it shows, or, past the last, the one before it. */
  function remove(state: string) {
    return make([{ op: "remove_state", id: state }], "Removing…", `${named(state)} removed`, (all) => Math.min(index(state), all.length - 2));
  }

  /** A copy of the slide `state` is on, just after it (PLAN 2.97's `duplicate_slide`): the
   * copy's first state shows. */
  function duplicate(state: string) {
    const on = slots.find((s) => s.state === state);
    if (!on) return editor.say(WAITING);
    const of = on.slide ?? on.state;
    const last = (all: Slot[]) => all.reduce((at, s, i) => ((s.slide ?? s.state) === of ? i : at), -1);
    return make(
      [{ op: "duplicate_slide", slide: of }],
      "Copying…",
      `${slide(state)} copied just after itself · an edit to the copy leaves the slide it copies`,
      (all) => last(all) + 1,
    );
  }

  const item = (e: Event) => (e.target as Element).closest<HTMLElement>("li[data-state]");
  const index = (state: string) => slots.findIndex((s) => s.state === state);
  /** `state` as a person counts it, and the slide it is on (PLAN 2.99). */
  const named = (state: string) => slots.find((s) => s.state === state)?.said ?? "the slide";
  const slide = (state: string) => named(state).replace(/, step \d+$/, "");

  list.onclick = (e) => {
    const li = item(e);
    if (li) editor.show(index(li.dataset.state!));
  };
  list.onkeydown = (e) => {
    const li = item(e);
    if (!li || (e.target as Element).tagName === "INPUT") return;
    const state = li.dataset.state!;
    const at = index(state);
    // ⌘D copies the slide; taken here, so the browser does not bookmark the page.
    if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "d") {
      e.preventDefault();
      void duplicate(state);
      return;
    }
    const steps: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1, Home: -at, End: slots.length - 1 - at };
    if (e.key in steps) {
      e.preventDefault();
      const to = Math.max(0, Math.min(slots.length - 1, at + steps[e.key]));
      if (e.altKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
        const other = slots[to]?.state;
        if (other && other !== state) void move(state, e.key === "ArrowLeft" ? "before" : "after", other);
        return;
      }
      if (to !== at) editor.show(to);
      return;
    }
    if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      void remove(state);
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      editor.show(at);
    }
  };

  // A state dragged among the others: it drops before or after the one under the pointer, by
  // which half the pointer is over.
  const half = (e: DragEvent, li: HTMLElement) => {
    const r = li.getBoundingClientRect();
    return e.clientX < r.left + r.width / 2 ? "before" : "after";
  };
  const unmark = () => list.querySelectorAll(".drop-before, .drop-after").forEach((li) => li.classList.remove("drop-before", "drop-after"));
  list.ondragstart = (e) => {
    const li = item(e);
    if (!li || !e.dataTransfer) return;
    dragging = li.dataset.state;
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", dragging!);
  };
  list.ondragover = (e) => {
    const li = item(e);
    if (!li || dragging === undefined) return;
    e.preventDefault();
    unmark();
    if (li.dataset.state !== dragging) li.classList.add(`drop-${half(e, li)}`);
  };
  list.ondragleave = () => unmark();
  list.ondrop = (e) => {
    const li = item(e);
    const state = dragging;
    unmark();
    dragging = undefined;
    if (!li || state === undefined) return;
    e.preventDefault();
    void move(state, half(e, li), li.dataset.state!);
  };
  list.ondragend = () => {
    dragging = undefined;
    unmark();
  };
  into.querySelector<HTMLButtonElement>("[data-add=step]")!.onclick = () => void add("step");
  into.querySelector<HTMLButtonElement>("[data-add=slide]")!.onclick = () => void gallery();

  return {
    states,
    select,
    paint,
    painted,
    reformat,
    found,
    add,
    gallery,
    start,
    move,
    remove,
    duplicate,
    /** The changes made through the strip, for a test: once they are. */
    settled: () => making,
    /** Each state's thumbnail digest, for a test. */
    digests: () => Object.fromEntries([...thumbs].map(([id, t]) => [id, t.digest])),
    /** The thumbnails it holds now, by state, and how many pixels high they are: what an
     * assistant's edit is drawn against (PLAN 2.93). */
    drawings: () => ({ height: high(), drawn: new Map(thumbs) }),
  };
}
