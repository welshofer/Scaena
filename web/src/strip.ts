// The state strip (PLAN 2.35, ADR-0013): the deck's states in order, each a thumbnail the engine
// paints at rest, with its cue's length. The page lays nothing out and paints nothing itself.
//
// - A click on a state, or the arrow keys, Home, and End among them, shows it.
// - New step adds a state after the state shown that tracks from it, in its slide: it shows what
//   that state shows until it is changed. New slide opens the gallery of the theme's layouts
//   (PLAN 3.30): each drawn as a slide started in it, its words in its slots, by the theme's
//   sections, then a blank slide; a click starts that slide after the last step of the slide shown.
// - A state dragged to another place, or moved with Alt and an arrow key, moves there
//   (`move_state`). F2, or a double click, renames it (`rename_state`), and Delete removes it
//   (`remove_state`).
// - Each is one patch, by the user, one step to undo; what the deck refuses, the status says why.
// - A thumbnail is painted again only when its state's drawing changed (the digest of its display
//   list), once every state is laid out after an edit.
// - Each state with findings in the format shown says how many, in the color of the worst (PLAN
//   2.49).
import { ALT, type Key } from "./commands";
import { counted, worst } from "./marks";
import type { Edited, Finding, Slot, Starter, Thumb } from "./protocol";
import type { Stage } from "./stage";

/** What a state in the strip answers, focused (PLAN 2.65), as the keys sheet lists it. */
export const stripKeys = (): Key[] => [
  { keys: "← → Home End", label: "In the strip: show the state before or after, the first, or the last", group: "States" },
  { keys: `${ALT}← ${ALT}→`, label: "Move the state before or after the one beside it", group: "States" },
  { keys: "F2", label: "Rename the state", group: "States" },
  { keys: "Delete", label: "Delete the state", group: "States" },
  { keys: "Enter Space", label: "Show the state", group: "States" },
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
  apply(source: string, edited: Edited): void;
  say(text: string): void;
  /** A slide made here, from the gallery or after the slide shown: the canvas outlines its
   * layout's empty slots (PLAN 3.30). */
  started?(state: string): void;
}

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
const html = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
/** How high a slide to start is drawn in the gallery, CSS pixels (PLAN 3.30). */
const TILE = 84;
/** A layout's name as a person reads it: `two-columns` as Two columns. */
const phrase = (name: string) => {
  const words = name.replace(/-/g, " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
};
/** A cue's length, as the strip says it. */
const seconds = (ms: number) => `${(ms / 1000).toFixed(ms < 10000 ? 1 : 0)} s`;

/** How high a thumbnail is painted, CSS pixels, on a display of `ratio` device pixels each. */
const HIGH = 54;
/** How many pixels high a thumbnail is painted: `HIGH` at the display's density, at most twice. */
const high = () => Math.round(HIGH * Math.min(2, window.devicePixelRatio || 1));

/** The strip in `into` (its `ol` the states, its buttons the strip's own), over `stage`. */
export function strip(stage: Stage, into: HTMLElement, editor: StripEditor) {
  const list = into.querySelector<HTMLOListElement>("ol")!;
  /** Each state's thumbnail, by id: what identifies its drawing, and its pixels. */
  const thumbs = new Map<string, { digest: string; image: ImageData }>();
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

  /** The states as `edited` says the deck has them, `shown` the one shown. */
  function states(next: Slot[], shown: number) {
    slots = next;
    for (const id of [...thumbs.keys()]) if (!next.some((s) => s.state === id)) thumbs.delete(id);
    const focused = list.contains(document.activeElement);
    list.innerHTML = next
      .map((s, i) => {
        const on = i === shown;
        return `<li role="option" id="strip-${html(s.state)}" data-state="${html(s.state)}" aria-selected="${on}" tabindex="${on ? 0 : -1}" draggable="true" title="${html(s.state)}: its cue ${seconds(s.span)}">
          <canvas aria-hidden="true"></canvas><span class="id">${html(s.state)}</span><span class="cue">${seconds(s.span)}</span>${tally(s.state)}</li>`;
      })
      .join("");
    for (const s of next) draw(s.state);
    const item = list.children[shown] as HTMLElement | undefined;
    item?.scrollIntoView({ block: "nearest", inline: "nearest" });
    if (focused) item?.focus();
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
    item?.scrollIntoView({ block: "nearest", inline: "nearest" });
    if (focused) item?.focus();
  }

  /** `state`'s thumbnail onto its item, if there is one yet. */
  function draw(state: string) {
    const canvas = list.querySelector<HTMLCanvasElement>(`li[data-state="${CSS.escape(state)}"] canvas`);
    const thumb = thumbs.get(state);
    if (!canvas || !thumb) return;
    [canvas.width, canvas.height] = [thumb.image.width, thumb.image.height];
    canvas.getContext("2d")?.putImageData(thumb.image, 0, 0);
  }

  /** Ask the engine for the thumbnails whose drawing changed, and draw them: whether they were
   * drawn, which a newer paint begun meanwhile does in its place. */
  async function paint(): Promise<boolean> {
    const asked = ++painting;
    const known = Object.fromEntries([...thumbs].map(([id, t]) => [id, t.digest]));
    let got: Thumb[];
    try {
      got = await stage.thumbnails(high(), known, editor.format());
    } catch {
      return true;
    }
    if (asked !== painting) return false;
    for (const t of got) {
      if (!t.pixels || !t.width || !t.height) continue;
      thumbs.set(t.state, { digest: t.digest, image: new ImageData(new Uint8ClampedArray(t.pixels), t.width, t.height) });
      draw(t.state);
    }
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
      if (!editor.shown()) return editor.say("the strip waits for a source that compiles");
      editor.say(doing);
      try {
        const { source, edited } = await stage.make(editor.source(), ops, Math.max(0, to(slots)), editor.format());
        editor.apply(source, edited);
        editor.say(done);
      } catch (e) {
        editor.say(`not made: ${said(e)}`);
      }
    };
    making = making.then(run, run);
    return making;
  }

  /** A state after the one shown: a `step` of its slide, or a `slide` of its own. */
  function add(what: "step" | "slide") {
    const shown = editor.shown();
    if (!shown) return editor.say("the strip waits for a source that compiles");
    return stage
      .addingState(editor.source(), shown.state, what)
      .then((added) => {
        const after = (added.patch[0] as { after?: string }).after;
        if (what === "slide") editor.started?.(added.id);
        const said = `${added.id} added after ${what === "step" ? shown.state : `${shown.state}'s slide`}`;
        return make(added.patch, `adding a ${what}…`, said, (all) => all.findIndex((s) => s.state === after) + 1);
      })
      .catch((e) => editor.say(`not added: ${said(e)}`));
  }

  /** The gallery of slides to start (PLAN 3.30): the theme's layouts, each drawn as a slide
   * started in it, under the theme's sections in the order it first names them, then a blank
   * slide. A click starts that slide after the slide shown. */
  async function gallery() {
    const shown = editor.shown();
    if (!shown) return editor.say("the strip waits for a source that compiles");
    const dialog = document.querySelector<HTMLDialogElement>("#starting")!;
    const box = dialog.querySelector<HTMLElement>(".starters")!;
    box.innerHTML = `<p class="note">Drawing the theme's layouts…</p>`;
    if (!dialog.open) dialog.showModal();
    const ratio = Math.min(2, window.devicePixelRatio || 1);
    let got: Starter[];
    try {
      got = await stage.starters(editor.source(), shown.state, Math.round(TILE * ratio), editor.format());
    } catch (e) {
      box.innerHTML = `<p class="note">not drawn: ${html(said(e))}</p>`;
      return;
    }
    if (!dialog.open) return;
    const groups: string[] = [];
    for (const one of got) if (one.group && !groups.includes(one.group)) groups.push(one.group);
    const tile = (one: Starter, i: number) => {
      const name = one.layout ? phrase(one.layout) : "Blank";
      return `<button type="button" data-start="${i}" title="${html(one.description ?? name)}"><canvas aria-hidden="true" width="${one.width}" height="${one.height}"></canvas><span>${html(name)}</span></button>`;
    };
    const section = (title: string, ones: [Starter, number][]) =>
      ones.length ? `<h3>${html(title)}</h3><div class="group">${ones.map(([one, i]) => tile(one, i)).join("")}</div>` : "";
    const indexed = got.map((one, i): [Starter, number] => [one, i]);
    box.innerHTML =
      groups.map((g) => section(g, indexed.filter(([one]) => one.group === g))).join("") +
      section(groups.length ? "Other" : "Layouts", indexed.filter(([one]) => one.layout && !one.group)) +
      section("Blank", indexed.filter(([one]) => !one.layout));
    box.querySelectorAll<HTMLButtonElement>("button[data-start]").forEach((button) => {
      const one = got[Number(button.dataset.start)];
      button.querySelector("canvas")?.getContext("2d")?.putImageData(new ImageData(new Uint8ClampedArray(one.pixels), one.width, one.height), 0, 0);
      button.onclick = () => {
        dialog.close();
        void start(one.layout ?? undefined);
      };
    });
    box.querySelector<HTMLButtonElement>("button[data-start]")?.focus();
  }

  /** A slide started in `layout`, or a blank one with none, after the slide shown (PLAN 3.30). */
  function start(layout: string | undefined) {
    const shown = editor.shown();
    if (!shown) return editor.say("the strip waits for a source that compiles");
    return stage
      .starting(editor.source(), shown.state, layout)
      .then((added) => {
        const after = (added.patch[0] as { after?: string }).after;
        editor.started?.(added.id);
        return make(added.patch, "adding a slide…", `${added.id} added after ${shown.state}'s slide`, (all) => all.findIndex((s) => s.state === after) + 1);
      })
      .catch((e) => editor.say(`not added: ${said(e)}`));
  }

  /** Move `state` before or after `to`. */
  function move(state: string, where: "before" | "after", to: string) {
    if (state === to) return;
    const lands = (all: Slot[]) => {
      const rest = all.map((s) => s.state).filter((id) => id !== state);
      return rest.indexOf(to) + (where === "after" ? 1 : 0);
    };
    return make([{ op: "move_state", id: state, [where]: to }], "moving…", `${state} moved ${where} ${to}`, lands);
  }

  /** Rename `state`: a field where its name stands, Enter to rename and Escape to leave it. */
  function rename(state: string) {
    const label = list.querySelector<HTMLElement>(`li[data-state="${CSS.escape(state)}"] .id`);
    if (!label) return;
    const field = Object.assign(document.createElement("input"), { value: state, spellcheck: false });
    field.setAttribute("aria-label", `Rename ${state}`);
    label.replaceChildren(field);
    field.select();
    field.focus();
    let done = false;
    const end = (to?: string) => {
      if (done) return;
      done = true;
      label.textContent = state;
      if (to && to !== state) void make([{ op: "rename_state", id: state, to }], "renaming…", `${state} renamed ${to}`, () => index(state));
      else (list.querySelector(`li[data-state="${CSS.escape(state)}"]`) as HTMLElement | null)?.focus();
    };
    field.onkeydown = (e) => {
      e.stopPropagation();
      if (e.key === "Enter") end(field.value.trim());
      else if (e.key === "Escape") end();
    };
    field.onblur = () => end();
  }

  /** Remove `state`: the state after it shows, or, past the last, the one before it. */
  function remove(state: string) {
    return make([{ op: "remove_state", id: state }], "removing…", `${state} removed`, (all) => Math.min(index(state), all.length - 2));
  }

  const item = (e: Event) => (e.target as Element).closest<HTMLElement>("li[data-state]");
  const index = (state: string) => slots.findIndex((s) => s.state === state);

  list.onclick = (e) => {
    const li = item(e);
    if (li) editor.show(index(li.dataset.state!));
  };
  list.ondblclick = (e) => {
    const li = item(e);
    if (li && (e.target as Element).closest(".id")) rename(li.dataset.state!);
  };
  list.onkeydown = (e) => {
    const li = item(e);
    if (!li || (e.target as Element).tagName === "INPUT") return;
    const state = li.dataset.state!;
    const at = index(state);
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
    if (e.key === "F2") {
      e.preventDefault();
      rename(state);
    } else if (e.key === "Delete" || e.key === "Backspace") {
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
    rename,
    /** The changes made through the strip, for a test: once they are. */
    settled: () => making,
    /** Each state's thumbnail digest, for a test. */
    digests: () => Object.fromEntries([...thumbs].map(([id, t]) => [id, t.digest])),
    /** The thumbnails it holds now, by state, and how many pixels high they are: what an
     * assistant's edit is drawn against (PLAN 2.93). */
    drawings: () => ({ height: high(), drawn: new Map(thumbs) }),
  };
}
