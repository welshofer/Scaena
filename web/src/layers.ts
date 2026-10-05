// The layers panel (PLAN 2.50, ADR-0013): the state shown's nodes, topmost first (a stack's in the
// order it lays them out), nested as their containers and groups hold them, as the engine lists
// them (`Player.layers`). The page orders nothing itself.
//
// - A click on a node selects it on the canvas, one under the rest too. A node the state does not
//   show (it leaves here, or another state of its slide shows it) is listed dimmed.
// - The eye shows a node in the state shown, or hides it there, with what it holds (`show_node`,
//   `hide_node`): one patch, one step to undo.
// - A double click on a name, or F2, renames the node everywhere (`rename_node`): Enter renames,
//   Escape leaves it.
import type { Edited, Layer } from "./protocol";
import type { Stage } from "./stage";

/** What the panel asks of the editor around it. */
export interface LayersEditor {
  /** The state shown, by its id and its slot's index; none while the source does not compile. */
  shown(): { state: string; index: number } | undefined;
  format(): string | undefined;
  source(): string;
  /** Select `node` on the canvas, as a click on it does. */
  select(node: string): void;
  /** Take `source`, a patch's, as one change: one step to undo. */
  apply(source: string, edited: Edited): void;
  say(text: string): void;
}

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
const html = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

/** The panel in `into`, over `stage`. */
export function layers(stage: Stage, into: HTMLElement, editor: LayersEditor) {
  const list = into.querySelector<HTMLUListElement>("ul")!;
  /** The layers of the state they were asked for, as the engine lists them. */
  let shown: { state: string; layers: Layer[] } | undefined;
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
    try {
      found = await stage.layers(state);
    } catch {
      return;
    }
    if (asked !== asking) return;
    shown = { state, layers: found };
    draw();
  }

  /** The list, topmost first: each node with its eye, its name, and its type. */
  function draw() {
    if (!shown) return;
    const focused = (document.activeElement as HTMLElement | null)?.closest<HTMLElement>("[data-layer]")?.dataset.layer;
    const row = (l: Layer, depth: number): string => {
      const on = chosen.includes(l.node);
      const eye = l.shown ? `Hide ${l.node} in ${shown!.state}` : `Show ${l.node} in ${shown!.state}`;
      const held = (l.children ?? []).map((c) => row(c, depth + 1)).join("");
      return `<li data-layer="${html(l.node)}" class="${l.shown ? "shown" : "hidden"}"${on ? ' aria-current="true"' : ""}>
        <div class="row" style="padding-left:${depth * 14}px">
          <button type="button" class="eye" data-eye aria-pressed="${l.shown}" aria-label="${html(eye)}" title="${html(eye)}">${l.shown ? "◉" : "○"}</button>
          <button type="button" class="name" data-pick title="${html(l.shown ? `Select ${l.node} (double click or F2 renames it)` : `${l.node} is not shown in ${shown!.state} (double click or F2 renames it)`)}">${html(l.node)}</button>
          <span class="kind">${html(l.type)}</span>
        </div>${held ? `<ul>${held}</ul>` : ""}</li>`;
    };
    list.innerHTML = shown.layers.map((l) => row(l, 0)).join("");
    if (focused !== undefined) list.querySelector<HTMLElement>(`[data-layer="${CSS.escape(focused)}"] [data-pick]`)?.focus();
  }

  /** Make `ops` on the source as it stands: one change to undo, then say `done`. */
  function make(ops: unknown[], doing: string, done: string) {
    const run = async () => {
      const now = editor.shown();
      if (!now) return editor.say("the layers wait for a source that compiles");
      editor.say(doing);
      try {
        const { source, edited } = await stage.make(editor.source(), ops, now.index, editor.format());
        editor.apply(source, edited);
        editor.say(done);
      } catch (e) {
        editor.say(`not made: ${said(e)}`);
      }
    };
    making = making.then(run, run);
    return making;
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
    return make(ops, layer.shown ? `hiding ${node}…` : `showing ${node}…`, `${node} ${layer.shown ? "hidden" : "shown"} in ${state}`);
  }

  /** Rename `node`: a field where its name stands, Enter to rename and Escape to leave it. */
  function rename(node: string) {
    const name = list.querySelector<HTMLElement>(`[data-layer="${CSS.escape(node)}"] [data-pick]`);
    if (!name) return;
    const field = Object.assign(document.createElement("input"), { value: node, spellcheck: false });
    field.setAttribute("aria-label", `Rename ${node}`);
    name.replaceWith(field);
    field.select();
    field.focus();
    let done = false;
    const end = (to?: string) => {
      if (done) return;
      done = true;
      field.replaceWith(name);
      if (to && to !== node) void make([{ op: "rename_node", id: node, to }], "renaming…", `${node} renamed ${to}`);
      else name.focus();
    };
    field.onkeydown = (e) => {
      e.stopPropagation();
      if (e.key === "Enter") end(field.value.trim());
      else if (e.key === "Escape") end();
    };
    field.onblur = () => end();
  }

  const layerOf = (e: Event) => (e.target as Element).closest<HTMLElement>("[data-layer]")?.dataset.layer;
  list.onclick = (e) => {
    const node = layerOf(e);
    if (node === undefined) return;
    if ((e.target as Element).closest("[data-eye]")) return void toggle(node);
    if (!(e.target as Element).closest("[data-pick]")) return;
    const layer = shown && find(shown.layers, node);
    if (layer?.shown) editor.select(node);
    else editor.say(`${node} is not shown in ${shown?.state}: its eye shows it here`);
  };
  list.ondblclick = (e) => {
    const node = layerOf(e);
    if (node !== undefined && (e.target as Element).closest("[data-pick]")) rename(node);
  };
  list.onkeydown = (e) => {
    const node = layerOf(e);
    if (node === undefined || (e.target as Element).tagName === "INPUT") return;
    if (e.key === "F2") {
      e.preventDefault();
      rename(node);
    }
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
    /** The changes made through the panel, for a test: once they are. */
    settled: () => making,
    toggle,
    rename,
  };
}
