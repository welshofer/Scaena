// The layers panel (PLAN 2.50, ADR-0013): the state shown's nodes, topmost first (a stack's in the
// order it lays them out), nested as their containers and groups hold them, as the engine lists
// them (`Player.layers`). The page orders nothing itself. Each is named as a person names it, by
// what it shows and by its kind (PLAN 2.99, `Player.words`).
//
// - A click on a node selects it on the canvas, one under the rest too. A node the state does not
//   show (it leaves here, or another state of its slide shows it) is listed dimmed.
// - The eye shows a node in the state shown, or hides it there, with what it holds (`show_node`,
//   `hide_node`): one patch, one step to undo.
// - The lock locks a node, or unlocks it (PLAN 2.95): the canvas passes over a node locked, and
//   moves, resizes, turns, and deletes nothing of it, while here it is selected as any other.
// - A node shown, dragged to another place in the list, or moved with Alt and an arrow key, goes
//   there (`Player.arranging`), one patch:
//   - Before or after the one it is dropped on, by which half of it the pointer is over: over or
//     under it by `z`, or, in a stack, before or after it in its order. Held by another
//     container, or on the canvas, it goes there with it, placed as that one places what it
//     holds (`place` with `parent`).
//   - Into a container dropped on its middle, first among what it holds.
//   - Alt with ↑ or ↓ moves it past the one before or after it; Alt with ← takes it out of its
//     container, listed just before it; Alt with → puts it into the container listed just
//     before it, last among what that holds.
import { ALT, type Key } from "./commands";
import type { Arrange, Edited, Layer, Lexicon } from "./protocol";
import type { Stage } from "./stage";
import { object } from "./words";

/** What a layer answers, focused (PLAN 2.65), as the keys sheet lists it. */
export const layerKeys = (): Key[] => [
  { keys: `${ALT}↑ ${ALT}↓`, label: "In the layers: move it before or after the one beside it", group: "Layers" },
  { keys: `${ALT}←`, label: "Take it out of its group", group: "Layers" },
  { keys: `${ALT}→`, label: "Put it in the group listed just before it", group: "Layers" },
];

/** What the panel asks of the editor around it. */
export interface LayersEditor {
  /** The state shown, by its id and its slot's index; none while the source does not compile. */
  shown(): { state: string; index: number } | undefined;
  format(): string | undefined;
  source(): string;
  /** Select `node` on the canvas, as a click on it does. */
  select(node: string): void;
  /** Take `source`, a patch's, as one change: one step to undo. */
  apply(source: string, edited: Edited, said?: string): void;
  /** Lock `node`, or with `on` false unlock it, as ⇧⌘L does (PLAN 2.95). */
  lock(node: string, on: boolean): Promise<unknown>;
  say(text: string): void;
}

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
/** What the panel says while the source does not compile: nothing it does can be made. */
const WAITING = "Nothing changed: the source has an error to fix first";
/** The types of node that hold others. */
const CONTAINERS = new Set(["stack", "grid", "frame", "group"]);
const html = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

/** The panel in `into`, over `stage`. */
export function layers(stage: Stage, into: HTMLElement, editor: LayersEditor) {
  const list = into.querySelector<HTMLUListElement>("ul")!;
  /** The layers of the state they were asked for, as the engine lists them, and its words. */
  let shown: { state: string; layers: Layer[]; words?: Lexicon } | undefined;
  /** The nodes selected on the canvas, the first leading. */
  let chosen: string[] = [];
  /** Layers asked for: the last answer wins. */
  let asking = 0;
  /** One change at a time, each made on the source the one before left. */
  let making: Promise<unknown> = Promise.resolve();

  /** The layer of `node`, anywhere in `all`. */
  const find = (all: Layer[], node: string): Layer | undefined => {
    for (const l of all) {
      const found = l.node === node ? l : find(l.children ?? [], node);
      if (found) return found;
    }
  };
  /** `layer` and everything it holds. */
  const within = (layer: Layer): Layer[] => [layer, ...(layer.children ?? []).flatMap(within)];

  /** Ask the engine for the state shown's layers, and list them. */
  async function refresh() {
    const state = editor.shown()?.state;
    if (state === undefined) return;
    const asked = ++asking;
    let found: Layer[];
    let words: Lexicon | undefined;
    try {
      [found, words] = await Promise.all([stage.layers(state), stage.words(state).catch(() => undefined)]);
    } catch {
      return;
    }
    if (asked !== asking) return;
    shown = { state, layers: found, words };
    draw();
  }

  /** `node` as a person names it in a sentence: “Revenue doubled”, the chart. */
  const named = (node: string) => object(shown?.words, node, true);

  /** The list, topmost first: each node with its eye, its name, and its type. */
  function draw() {
    if (!shown) return;
    const focused = (document.activeElement as HTMLElement | null)?.closest<HTMLElement>("[data-layer]")?.dataset.layer;
    const words = shown.words;
    const row = (l: Layer, depth: number): string => {
      const on = chosen.includes(l.node);
      // Each by what it shows, and its kind beside it where its words name it.
      const name = object(words, l.node);
      const kind = words?.kinds[l.node] ?? l.type;
      const eye = l.shown ? `Hide ${named(l.node)} on this slide` : `Show ${named(l.node)} on this slide`;
      const lock = l.locked ? `Unlock ${named(l.node)}` : `Lock ${named(l.node)}: the canvas passes over it`;
      const held = (l.children ?? []).map((c) => row(c, depth + 1)).join("");
      const holds = CONTAINERS.has(l.type) ? " data-container" : "";
      return `<li data-layer="${html(l.node)}" class="${l.shown ? "shown" : "hidden"}"${holds}${on ? ' aria-current="true"' : ""}>
        <div class="row" style="padding-left:${depth * 14}px"${l.shown ? ' draggable="true"' : ""}>
          <button type="button" class="eye" data-eye aria-pressed="${l.shown}" aria-label="${html(eye)}" title="${html(eye)}">${l.shown ? "◉" : "○"}</button>
          <button type="button" class="lock" data-lock aria-pressed="${!!l.locked}" aria-label="${html(`Lock ${named(l.node)}`)}" title="${html(lock)}">${l.locked ? "🔒" : "🔓"}</button>
          <button type="button" class="name" data-pick title="${html(l.shown ? "Select it" : "Not shown on this slide")}">${html(name)}</button>
          <span class="kind">${name === kind ? "" : html(kind)}</span>
        </div>${held ? `<ul>${held}</ul>` : ""}</li>`;
    };
    list.innerHTML = shown.layers.map((l) => row(l, 0)).join("");
    if (focused !== undefined) list.querySelector<HTMLElement>(`[data-layer="${CSS.escape(focused)}"] [data-pick]`)?.focus();
  }

  /** Make `ops`, or what `ask` gives, on the source as it stands: one change to undo, then say
   * `done`. Nothing to make says `none`. */
  function make(ops: unknown[] | (() => Promise<unknown[]>), doing: string, done: string, none = done) {
    const run = async () => {
      const now = editor.shown();
      if (!now) return editor.say(WAITING);
      editor.say(doing);
      try {
        const patch = typeof ops === "function" ? await ops() : ops;
        if (!patch.length) return editor.say(none);
        const { source, edited } = await stage.make(editor.source, patch, now.index, editor.format());
        editor.apply(source, edited, done);
        editor.say(done);
      } catch (e) {
        editor.say(`Not changed: ${said(e)}`);
      }
    };
    making = making.then(run, run);
    return making;
  }

  /** What holds `node` as listed: a container's id, or `""` for the canvas. */
  function holderOf(node: string): string | undefined {
    const look = (all: Layer[], holder: string): string | undefined => {
      for (const l of all) {
        if (l.node === node) return holder;
        const found = look(l.children ?? [], l.node);
        if (found !== undefined) return found;
      }
    };
    return shown && look(shown.layers, "");
  }

  /** `node`, listed just before or after another node, or first in a container: one patch.
   * Held by another container, it goes into that one. */
  function restack(node: string, how: { before: string } | { after: string } | { into: string }) {
    const state = shown?.state;
    if (!state) return;
    const ask = async () => (await stage.arranging(state, [node], how as Arrange, false, editor.format()))?.patch ?? [];
    if ("into" in how) {
      return make(ask, "Moving…", `${named(node)} moved into ${named(how.into)}`, `${named(node)} is first in ${named(how.into)} already`);
    }
    const [where, to] = "before" in how ? ["before", how.before] : ["after", how.after];
    const from = holderOf(node);
    const there = holderOf(to);
    const moved = there === undefined || there === from ? "" : there ? ` into ${named(there)},` : " out of its group,";
    return make(ask, "Moving…", `${named(node)} moved${moved} ${where} ${named(to)}`, `${named(node)} is ${where} ${named(to)} already`);
  }

  /** The children shown of what holds `node`, as listed, and where `node` is among them. */
  function siblings(node: string): { all: string[]; at: number } {
    const li = list.querySelector<HTMLElement>(`li[data-layer="${CSS.escape(node)}"]`);
    const all = [...(li?.parentElement?.children ?? [])]
      .filter((c): c is HTMLElement => c instanceof HTMLElement && c.classList.contains("shown"))
      .map((c) => c.dataset.layer!);
    return { all, at: all.indexOf(node) };
  }

  /** Show `node` in the state shown, or hide it there, with what it holds. */
  function toggle(node: string) {
    const state = shown?.state;
    const layer = shown && find(shown.layers, node);
    if (!state || !layer) return;
    const op = layer.shown ? "hide_node" : "show_node";
    const ops = within(layer)
      .filter((l) => l.shown === layer.shown)
      .map((l) => ({ op, node: l.node, state }));
    const name = named(node);
    return make(ops, layer.shown ? "Hiding…" : "Showing…", `${name[0].toUpperCase()}${name.slice(1)} ${layer.shown ? "hidden" : "shown"} on this slide`);
  }

  const layerOf = (e: Event) => (e.target as Element).closest<HTMLElement>("[data-layer]")?.dataset.layer;
  list.onclick = (e) => {
    const node = layerOf(e);
    if (node === undefined) return;
    if ((e.target as Element).closest("[data-eye]")) return void toggle(node);
    if ((e.target as Element).closest("[data-lock]")) {
      const layer = shown && find(shown.layers, node);
      return void editor.lock(node, !layer?.locked);
    }
    if (!(e.target as Element).closest("[data-pick]")) return;
    const layer = shown && find(shown.layers, node);
    if (layer?.shown) editor.select(node);
    else editor.say(`${named(node)} isn’t shown on this slide: its eye shows it here`);
  };
  list.onkeydown = (e) => {
    const node = layerOf(e);
    if (node === undefined || (e.target as Element).tagName === "INPUT") return;
    if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
      e.preventDefault();
      const { all, at } = siblings(node);
      if (at < 0) return;
      const up = e.key === "ArrowUp";
      const to = all[up ? at - 1 : at + 1];
      if (to === undefined) return editor.say(`${named(node)} is ${up ? "first" : "last"} already`);
      void restack(node, up ? { before: to } : { after: to });
    } else if (e.altKey && e.key === "ArrowLeft") {
      // Out of its container: listed just before it, among what holds that.
      e.preventDefault();
      const holder = holderOf(node);
      if (!holder) return editor.say(`${named(node)} is in no group`);
      void restack(node, { before: holder });
    } else if (e.altKey && e.key === "ArrowRight") {
      // Into the container listed just before it: last among what that holds.
      e.preventDefault();
      const { all, at } = siblings(node);
      const above = at > 0 ? shown && find(shown.layers, all[at - 1]) : undefined;
      if (!above || !CONTAINERS.has(above.type)) return editor.say(`${named(node)} has no group just before it to go into`);
      const last = (above.children ?? []).filter((c) => c.shown).at(-1);
      void restack(node, last ? { after: last.node } : { into: above.node });
    }
  };

  // A node shown dragged through the list: it drops before or after the one under the pointer,
  // by which half the pointer is over, or, on a container's middle half, into it.
  let dragging: string | undefined;
  const rowOf = (e: DragEvent) => (e.target as Element).closest<HTMLElement>("li[data-layer]");
  const half = (e: DragEvent, li: HTMLElement): "before" | "after" | "into" => {
    const r = li.querySelector(".row")!.getBoundingClientRect();
    const at = (e.clientY - r.top) / r.height;
    if (li.dataset.container !== undefined && at >= 0.25 && at < 0.75) return "into";
    return at < 0.5 ? "before" : "after";
  };
  const unmark = () =>
    list.querySelectorAll(".drop-before, .drop-after, .drop-into").forEach((li) => li.classList.remove("drop-before", "drop-after", "drop-into"));
  /** The row the dragged node may drop on: shown, and neither the node nor what it holds. */
  const target = (e: DragEvent) => {
    const li = rowOf(e);
    const from = dragging === undefined ? undefined : list.querySelector<HTMLElement>(`li[data-layer="${CSS.escape(dragging)}"]`);
    if (!li || !from || from.contains(li) || !li.classList.contains("shown")) return undefined;
    return li;
  };
  list.ondragstart = (e) => {
    const li = rowOf(e);
    if (!li || !e.dataTransfer || !li.classList.contains("shown")) return;
    dragging = li.dataset.layer;
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", dragging!);
  };
  list.ondragover = (e) => {
    const li = target(e);
    unmark();
    if (!li) return;
    e.preventDefault();
    li.classList.add(`drop-${half(e, li)}`);
  };
  list.ondragleave = () => unmark();
  list.ondrop = (e) => {
    const li = target(e);
    const node = dragging;
    unmark();
    dragging = undefined;
    if (!li || node === undefined) return;
    e.preventDefault();
    const to = li.dataset.layer!;
    const where = half(e, li);
    void restack(node, where === "into" ? { into: to } : where === "before" ? { before: to } : { after: to });
  };
  list.ondragend = () => {
    dragging = undefined;
    unmark();
  };

  return {
    refresh,
    /** The nodes selected on the canvas now, the first leading. */
    selected(nodes: string[]) {
      chosen = nodes;
      for (const li of list.querySelectorAll<HTMLElement>("li[data-layer]")) {
        if (chosen.includes(li.dataset.layer!)) li.setAttribute("aria-current", "true");
        else li.removeAttribute("aria-current");
      }
    },
    /** The layers listed, for a test: each node as listed, as `node`, `node (hidden)`, and what it
     * holds after it in brackets. */
    listed: () => {
      const say = (all: Layer[]): string[] =>
        all.map((l) => `${l.node}${l.shown ? "" : " (hidden)"}${l.children?.length ? ` [${say(l.children).join(", ")}]` : ""}`);
      return shown ? say(shown.layers) : [];
    },
    /** `node`'s type, as the layers of `state` list it, where they are those listed. */
    type: (node: string, state: string) => (shown?.state === state ? find(shown.layers, node)?.type : undefined),
    /** The changes made through the panel, for a test: once they are. */
    settled: () => making,
    toggle,
    restack,
  };
}
