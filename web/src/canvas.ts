// The editor's canvas (PLAN 2.31, ADR-0013): the preview takes the pointer and the keys, and a
// gesture ends in one `place` patch, made by the user: one step to undo. The engine answers
// every question about where things stand; the page lays nothing out.
//
// - A click selects what `hit` says is topmost there. A press in the node selected, or in what
//   holds it, keeps it, so a drag moves it; a click without a drag then selects what is topmost.
//   Escape selects what holds the node selected.
// - Several of one container's children are selected at once (PLAN 2.42): Shift+click puts one in
//   the selection or takes it out, and a drag across empty canvas, or across what fills it behind
//   all, selects the children of the canvas it encloses (with Shift, beside those selected). A
//   drag moves them together, the arrow keys too, and Delete, ⌘D, ⌘C, ⌘X, and the inspector act
//   on all of them; the inspector aligns and spreads them. ⌘] and ⌘[ put what is selected in
//   front of, or behind, the next thing it overlaps, and with Shift in front of, or behind,
//   everything its container holds. Each is one patch, one step to undo.
// - ⌘G (Ctrl+G) puts what is selected in a new group where it stands, the group selected; ⌘⇧G
//   takes the group selected apart, its children out to its container where they stand, all of
//   them selected (PLAN 2.43). Each is one patch, one step to undo.
// - A drag moves the node's layer, painted from the state as laid out at rest, and shows where it
//   would land from its targets: the theme's tracks, the template's slots, a grid container's
//   areas, a stack's order. On drop, where it lands is the patch. With Shift, it goes off the grid,
//   placed by a `rect`, an override lint flags (W301); or, off it, back onto it.
// - A handle resizes it the same way, by tracks. While it is dragged, the text keeps its old
//   wrapping; when the pointer pauses, the preview shows it laid out as the patch would make it.
// - The arrow keys move it a track, or a place along its stack; off the grid, a canvas unit. With
//   Shift, they resize it the same.
// - Before a patch is made, the status says which states it changes ("in 3 states"). It changes
//   the placement where it lives; Alt keeps it to the state shown (`fork`).
// - A double click on a text, or Enter on one selected, types in it where it stands (PLAN 2.32,
//   `typing.ts`): with Alt, what is typed is kept to the state shown. There, ⌘B makes the
//   characters selected bold, or not, and a role or a color chosen in the inspector is theirs
//   (PLAN 2.38).
// - Insert puts what the theme or the bundle offers where the pointer last pressed, or in the
//   middle, snapped to the grid as a drop snaps, or a text or an image into the empty slot there;
//   it enters in the state shown, selected (PLAN 2.34). Delete (or Backspace) takes the node
//   selected, with what it holds, out of the state shown and the states after it; Shift+Delete,
//   out of the deck. ⌘D (Ctrl+D) adds a copy beside it, with what it holds. Each is one patch,
//   one step to undo.
// - ⌘C and ⌘X put the nodes selected, with what they hold, on the clipboard as JSON
//   (`application/x-scaena+json`) and as text; a cut then takes them out as Delete does. ⌘V
//   pastes them where the pointer last pressed, as Insert places a node, several where they stood
//   about each other, under ids new to the deck, in this deck or another; text from elsewhere
//   comes in as a text in the theme's body role. What the clip names that the theme lacks is
//   taken out of it, and the status says so (PLAN 2.37). A page must fill the clipboard at once,
//   so what is selected is copied when it is selected.
import { CLIP } from "./protocol";
import type { Arrange, Edited, NodeBox, Rect, SnapMode, Snapped, Targets } from "./protocol";
import type { Stage } from "./stage";
import { covered, type Selected, typing } from "./typing";

/** What the canvas asks of the editor around it. */
export interface Editor {
  /** The state shown, by its id and its slot's index; none while the canvas waits: the source
   * does not compile, or the assistant is working on it. */
  shown(): { state: string; index: number } | undefined;
  format(): string | undefined;
  /** The source as it stands: what a patch is made on. */
  source(): string;
  /** One more with each change to the source: a drag the source changed under is dropped. */
  version(): number;
  /** How `node` is placed in the state shown, resolved: its `at`. */
  at(node: string): Placement | undefined;
  /** Take `source`, a patch's, as one change: one step to undo. */
  apply(source: string, edited: Edited): void;
  /** Take `source` as typed: one step to undo with what was typed just before it (`joins`), or
   * the first of a burst of typing. */
  typed(source: string, edited: Edited, joins: boolean): void;
  undo(): void;
  redo(): void;
  say(text: string): void;
  /** The node selected is now `node`, with `also` selected beside it (PLAN 2.42). */
  selected(node: string | undefined, also: string[]): void;
  /** The characters selected in a text typed in are now `selected`, or none are (PLAN 2.38). */
  chose(selected: Selected | undefined): void;
  /** Whether focus gone to `to` keeps typing on: the inspector. */
  keeps(to: EventTarget | null): boolean;
  /** The preview is zoomed to `zoom`: 1 shows the whole canvas (PLAN 2.46). */
  zoomed(zoom: number): void;
}

/** A node's `at`, resolved. */
export type Placement = Record<string, unknown>;

/** A handle: the edge or corner of a box it moves. */
type Edge = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";
const EDGES: Edge[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];
const CURSORS: Record<Edge, string> = { n: "ns", s: "ns", e: "ew", w: "ew", ne: "nesw", sw: "nesw", nw: "nwse", se: "nwse" };

/** The zoom's steps, ⌘+ and ⌘− (PLAN 2.46): from the whole canvas to eight times as close. */
const ZOOMS = [1, 1.5, 2, 3, 4, 6, 8];
const MAX_ZOOM = 8;

/** A drag under way: the node, where the pointer went down and is now, the keys held, where the
 * node may go, and where it lands as last asked. */
interface Drag {
  kind: "move" | "resize";
  node: string;
  /** The nodes selected beside it, which move with it (PLAN 2.42). */
  with: string[];
  edge?: Edge;
  from: [number, number];
  at: [number, number];
  shift: boolean;
  alt: boolean;
  /** The source's version when it began. */
  version: number;
  targets: Targets;
  how?: SnapMode;
  snapped?: Snapped | null;
  states?: string[];
  /** What was asked last, so a move that changes nothing asks nothing. */
  asked?: string;
  /** A resize's preview, once the pointer pauses. */
  pause?: ReturnType<typeof setTimeout>;
}

/** The pointer down, not yet moved far enough to drag: on `node` (by a handle, `edge`), and what a
 * click there selects. Until the engine says what is there (`asking`), `node` is unknown: the
 * pointer's last move meanwhile is kept (`moved`), and a pointer let go meanwhile (`released`)
 * selects what the engine says once it does, or, moved past the slop, drops it there. */
interface Press {
  node?: string;
  /** The nodes selected beside `node`, which a drag moves with it. */
  with?: string[];
  /** With Shift: what a click puts in the selection or takes out of it. */
  toggle?: string;
  /** On nothing, or on what fills the canvas behind all: a drag draws a marquee. */
  marquee?: boolean;
  shift?: boolean;
  edge?: Edge;
  from: [number, number];
  client: [number, number];
  click?: string;
  released?: boolean;
  asking?: boolean;
  moved?: Starting & { client: [number, number] };
}

/** A drag asking where its node may go: the pointer as it is now, the keys held, and whether it
 * was let go. */
interface Starting {
  at: [number, number];
  shift: boolean;
  alt: boolean;
  up?: boolean;
}

/** How far the pointer moves, CSS pixels, before a press is a drag. */
const SLOP = 4;
/** How soon a press follows the one before to count as a second click (or a third), ms. */
const AGAIN = 450;
/** How long a resize pauses before the preview shows it laid out, ms. */
const PAUSE = 300;

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
const has = (at: Placement | undefined, key: string) => at?.[key] !== undefined && at?.[key] !== null;

/** How a drag of a node placed `at`, held as `t` says, snaps: `resize` by a handle. Shift takes a
 * node on the theme's grid off it, by a `rect`, or one off it back onto it. `undefined`: nothing
 * places it that way. */
export function snapOf(t: Targets, at: Placement | undefined, resize: boolean, shift: boolean): SnapMode | undefined {
  switch (t.by) {
    case "stack":
      return resize ? undefined : "order";
    case "frame":
      return "free";
    case "cells":
      return has(at, "area") ? (resize ? undefined : "slot") : resize ? "resize" : "move";
    case "grid":
      if (has(at, "rect") !== shift) return "free";
      if (has(at, "in") && !shift) return resize ? undefined : "slot";
      return resize ? "resize" : "move";
  }
}

/** A placement as the status and the inspector say it. A `rect` on the theme's grid is off it: an
 * override, which lint flags (W301). */
export function placed(at: Placement, by?: Targets["by"]): string {
  const span = (v: unknown) => (Array.isArray(v) ? (v[0] === v[1] ? `${v[0]}` : `${v[0]}–${v[1]}`) : `${v}`);
  if (has(at, "in")) return `slot ${at.in}`;
  if (has(at, "area")) return `area ${at.area}`;
  if (has(at, "rect")) {
    const [x, y, w, h] = at.rect as number[];
    return `rect ${x}, ${y}, ${w} × ${h}${by === "grid" ? ", off the grid" : ""}`;
  }
  if (has(at, "index")) return `index ${at.index}`;
  const cells = [has(at, "col") ? `col ${span(at.col)}` : "", has(at, "row") ? `row ${span(at.row)}` : ""];
  return cells.filter(Boolean).join(", ") || "the grid";
}

const moved = ([x, y, w, h]: Rect, [dx, dy]: [number, number]): Rect => [x + dx, y + dy, w, h];

function resized([x, y, w, h]: Rect, edge: Edge, [dx, dy]: [number, number]): Rect {
  if (edge.includes("e")) w += dx;
  if (edge.includes("w")) [x, w] = [x + Math.min(dx, w - 1), w - Math.min(dx, w - 1)];
  if (edge.includes("s")) h += dy;
  if (edge.includes("n")) [y, h] = [y + Math.min(dy, h - 1), h - Math.min(dy, h - 1)];
  return [x, y, Math.max(1, w), Math.max(1, h)];
}

/** The canvas over `overlay`, which holds an `<svg>` the size of the preview, on `stage`'s state
 * shown. The editor makes one for each bundle it opens. */
export function canvas(stage: Stage, overlay: HTMLElement, editor: Editor) {
  const svg = overlay.querySelector("svg")!;
  /** The canvas in canvas units, and what stands where in the state shown. */
  let size: [number, number] = [1920, 1080];
  /** The part of the canvas the preview shows, canvas units: all of it, until it is zoomed in
   * (PLAN 2.46). Frames are painted through it at the size shown, and every point the pointer
   * names is read through it. */
  let view: Rect = [0, 0, 1920, 1080];
  /** A pan under way: the view where the pointer went down, and where it went down, client px. */
  let panning: { from: Rect; client: [number, number]; pointer: number } | undefined;
  /** Whether Space is held: a drag then pans. */
  let spaced = false;
  /** Characters marked in a text, as a find shows its match (PLAN 2.47): the node, and the
   * rects they cover, canvas units, from the engine's carets. */
  let marked: { node: string; rects: Rect[] } | undefined;
  /** The format the view is of: another shows the whole canvas again. */
  let framed: string | undefined;
  let boxes: NodeBox[] = [];
  /** The state `boxes` stand in. */
  let boxed: string | undefined;
  let selected: string | undefined;
  /** Selected beside it, children of what holds it too (PLAN 2.42). */
  let also: string[] = [];
  /** A drag across empty canvas: where it began, where the pointer is, and whether it adds to
   * what is selected. */
  let marquee: { from: [number, number]; at: [number, number]; adding: boolean } | undefined;
  /** Where the node selected may go: whether it has handles. */
  let aim: Targets | undefined;
  let hovered: string | undefined;
  /** Where the pointer last pressed, canvas units: where Insert puts what it inserts. */
  let pointed: [number, number] | undefined;
  /** What is selected as the clipboard would hold it, asked for when it is selected, and kept by
   * what it was asked of: a copy, which the page answers at once, finds it at hand. */
  let held: { key: string; clip: Promise<string>; text?: string } | undefined;
  let press: Press | undefined;
  let starting: Starting | undefined;
  let drag: Drag | undefined;
  /** How many drags began from moves made before the engine said what was pressed. */
  let early = 0;
  /** A drag's request is with the worker: the next waits for it, so a fast drag never queues. */
  let busy = false;
  /** The gesture being made into a patch: the next waits for it, so each is made on the source the
   * one before left, as a key held down repeats. */
  let making: Promise<void> = Promise.resolve();
  const inTurn = (work: () => Promise<void>) => {
    const next = making.then(work);
    making = next.catch(() => {});
    return next;
  };

  /** The last press: when, and where (CSS pixels), and how many clicks it counted. A key between
   * two presses makes the next a first click. */
  let pressed: { at: number; client: [number, number]; clicks: number } | undefined;
  const unpress = () => (pressed = undefined);
  document.addEventListener("keydown", unpress, true);
  const clicks = (e: PointerEvent) => {
    const again =
      pressed && e.timeStamp - pressed.at < AGAIN && Math.hypot(e.clientX - pressed.client[0], e.clientY - pressed.client[1]) < SLOP;
    pressed = { at: e.timeStamp, client: [e.clientX, e.clientY], clicks: again ? pressed!.clicks + 1 : 1 };
    return pressed.clicks;
  };

  const box = (node?: string) => (node === undefined ? undefined : boxes.find((b) => b.node === node));
  const point = (e: MouseEvent): [number, number] => {
    const r = overlay.getBoundingClientRect();
    return [view[0] + ((e.clientX - r.left) / r.width) * view[2], view[1] + ((e.clientY - r.top) / r.height) * view[3]];
  };
  /** Canvas units to a CSS pixel: what handles and lines are sized in. */
  const unit = () => view[2] / Math.max(1, overlay.getBoundingClientRect().width);
  const inside = ([x, y, w, h]: Rect, [px, py]: [number, number]) => px >= x && px <= x + w && py >= y && py <= y + h;
  /** Every node selected: the one selected, then those beside it. */
  const chosen = () => (selected === undefined ? [] : [selected, ...also]);
  /** What holds `node` in the state shown: `null` at the root. */
  const holder = (node: string) => box(node)?.parent ?? null;
  /** Whether `r` covers the canvas: what stands behind everything. */
  const covers = (r: Rect | undefined) => r !== undefined && r[0] <= 0 && r[1] <= 0 && r[0] + r[2] >= size[0] && r[1] + r[3] >= size[1];

  /** Typing in a text where it stands. */
  const text = typing(stage, overlay, {
    shown: () => editor.shown(),
    format: () => editor.format(),
    source: () => editor.source(),
    typed: (source, edited, joins) => editor.typed(source, edited, joins),
    undo: () => editor.undo(),
    redo: () => editor.redo(),
    say: (words) => editor.say(words),
    draw: () => draw(),
    unit,
    origin: () => [view[0], view[1]],
    box: (node) => box(node)?.rect,
    chose: (selected) => editor.chose(selected),
    keeps: (to) => editor.keeps(to),
  });

  /** Type in `node` (a text), at the caret nearest `at`, or at its end; `fork` keeps it to the
   * state shown. */
  async function type(node: string, at: [number, number] | undefined, fork: boolean) {
    if (selected !== node) select(node);
    if (!(await text.enter(node, at, fork))) editor.say(`${node} is no text: only a text takes typing`);
  }

  /** What stands where in the state shown, asked again: the deck, the state, or the format changed. */
  async function refresh() {
    const shown = editor.shown();
    if (!shown) return;
    const [was, format] = [size, editor.format()];
    ({ boxes, size } = await stage.boxes(shown.state, format));
    boxed = shown.state;
    // Another format, laid out again, or another canvas: the preview shows all of it again.
    if (size[0] !== was[0] || size[1] !== was[1] || format !== framed) void look([0, 0, size[0], size[1]]);
    framed = format;
    svg.setAttribute("viewBox", view.join(" "));
    also = also.filter((n) => box(n) !== undefined);
    if (selected !== undefined && !box(selected)) select(also[0]);
    else if (selected !== undefined) aimAt(selected);
    hold();
    draw();
    await text.sync();
  }

  /** How close the preview is: 1 shows the whole canvas (PLAN 2.46). */
  const zoom = () => size[0] / view[2];
  /** The view the worker paints through, as last sent, and the send under way: a wheel's many
   * moves send the last of them, one paint at a time. */
  let sent = "";
  let sending: Promise<void> | undefined;
  /** Show `next`, as close as it asks, from the whole canvas to eight times as close, kept on the
   * canvas, and have the preview painted there. Resolves once it is. */
  function look(next: Rect): Promise<void> {
    const z = Math.min(MAX_ZOOM, Math.max(1, size[0] / next[2]));
    const [w, h] = [size[0] / z, size[1] / z];
    view = [Math.min(Math.max(next[0], 0), size[0] - w), Math.min(Math.max(next[1], 0), size[1] - h), w, h];
    svg.setAttribute("viewBox", view.join(" "));
    draw();
    editor.zoomed(z);
    sending ??= (async () => {
      while (sent !== view.join(" ")) {
        const now: Rect = [...view];
        sent = now.join(" ");
        await stage.view(size[0] / now[2] > 1 ? now : undefined).catch(() => {});
      }
      sending = undefined;
    })();
    return sending;
  }
  /** As close as `z`, canvas point `about` staying where it is on the screen: the pointer's, or
   * the middle of what is shown. */
  function zoomTo(z: number, about?: [number, number]) {
    const [ax, ay] = about ?? [view[0] + view[2] / 2, view[1] + view[3] / 2];
    const [fx, fy] = [(ax - view[0]) / view[2], (ay - view[1]) / view[3]];
    const [w, h] = [size[0] / z, size[1] / z];
    return look([ax - fx * w, ay - fy * h, w, h]);
  }
  /** A step closer (`1`) or farther (`-1`), about `about`. */
  function zoomStep(by: 1 | -1, about?: [number, number]) {
    const z = zoom();
    const next = by > 0 ? (ZOOMS.find((s) => s > z + 1e-6) ?? MAX_ZOOM) : ([...ZOOMS].reverse().find((s) => s < z - 1e-6) ?? 1);
    return zoomTo(next, about);
  }
  /** The whole canvas again. */
  const fit = () => look([0, 0, size[0], size[1]]);

  function select(node: string | undefined) {
    if (node === selected && also.length === 0) return;
    if (marked && marked.node !== node) marked = undefined;
    if (text.node() !== undefined && text.node() !== node) text.leave();
    selected = node;
    also = [];
    aim = undefined;
    editor.selected(node, []);
    if (node !== undefined) {
      editor.say(`${node} selected: drag it, or move it with the arrow keys`);
      aimAt(node);
    }
    hold();
    draw();
  }

  /** Select `nodes`, children of one container, at once (PLAN 2.42): the first leads a drag. */
  function selectAll(nodes: string[]) {
    if (nodes.length <= 1) return select(nodes[0]);
    if (text.node() !== undefined) text.leave();
    [selected, also] = [nodes[0], nodes.slice(1)];
    aim = undefined;
    editor.selected(selected, also);
    editor.say(`${nodes.length} selected, ${nodes.join(", ")}: drag them, move them with the arrow keys, or align them in the inspector`);
    hold();
    draw();
  }

  /** Shift+click on `node`: out of the selection if it is in it, else in it beside what is
   * selected where one container holds them all, else selected alone. */
  function toggle(node: string) {
    const all = chosen();
    if (all.includes(node)) return selectAll(all.filter((n) => n !== node));
    if (selected === undefined || holder(node) !== holder(selected)) return select(node);
    selectAll([...all, node]);
  }

  /** The children of the canvas the marquee encloses, beside what is selected where it adds. */
  function enclosed(m: NonNullable<typeof marquee>) {
    const [x, y] = [Math.min(m.from[0], m.at[0]), Math.min(m.from[1], m.at[1])];
    const area: Rect = [x, y, Math.abs(m.at[0] - m.from[0]), Math.abs(m.at[1] - m.from[1])];
    const within = (r: Rect) => r[0] >= area[0] && r[1] >= area[1] && r[0] + r[2] <= area[0] + area[2] && r[1] + r[3] <= area[1] + area[3];
    const found = boxes.filter((b) => (b.parent ?? null) === null && within(b.rect)).map((b) => b.node);
    const kept = m.adding && selected !== undefined && holder(selected) === null ? chosen() : [];
    return [...kept, ...found.filter((n) => !kept.includes(n))];
  }

  /** What the clipboard would hold of what is selected, as the source stands, kept by what it is
   * asked of: the source's version, the state shown, the nodes, and the format. */
  const holding = () => {
    const shown = editor.shown();
    return shown && selected !== undefined ? JSON.stringify([editor.version(), shown.state, chosen(), editor.format() ?? null]) : undefined;
  };
  /** Ask for what the clipboard would hold of what is selected, unless it is at hand; not while
   * a text is typed in, whose keys copy what is selected in it. */
  function hold() {
    const key = holding();
    if (key === undefined || text.node() !== undefined) return void (held = undefined);
    if (held?.key === key) return;
    const shown = editor.shown()!;
    const now: NonNullable<typeof held> = { key, clip: stage.copying(editor.source(), shown.state, chosen(), editor.format()) };
    held = now;
    now.clip.then(
      (clip) => (now.text = clip),
      () => {},
    );
  }

  /** Where `node` may go, for its handles. */
  function aimAt(node: string) {
    const shown = editor.shown();
    if (!shown) return;
    stage
      .targets(shown.state, node, editor.format())
      .then((t) => {
        if (selected !== node) return;
        aim = t;
        draw();
      })
      .catch(() => (aim = undefined));
  }

  function draw() {
    const u = unit();
    const parts: string[] = [];
    const rect = ([x, y, w, h]: Rect, cls: string, extra = "") =>
      `<rect class="${cls}" x="${x}" y="${y}" width="${Math.max(0, w)}" height="${Math.max(0, h)}"${extra}/>`;
    const line = (x1: number, y1: number, x2: number, y2: number, cls: string) =>
      `<line class="${cls}" x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}"/>`;
    if (drag) {
      const t = drag.targets;
      const cols = t.columns ?? [];
      const rows = t.rows ?? [];
      if ((drag.how === "move" || drag.how === "resize") && cols.length && rows.length) {
        const [top, bottom] = [rows[0][0], rows[rows.length - 1][1]];
        const [left, right] = [cols[0][0], cols[cols.length - 1][1]];
        for (const [a, b] of cols) parts.push(line(a, top, a, bottom, "track"), line(b, top, b, bottom, "track"));
        for (const [a, b] of rows) parts.push(line(left, a, right, a, "track"), line(left, b, right, b, "track"));
      }
      if (drag.how === "slot") {
        for (const [name, r] of Object.entries(t.slots ?? {})) if (name !== "canvas" && name !== "grid") parts.push(rect(r, "slot"));
      }
      if (drag.how === "order") for (const id of t.flow ?? []) if (id !== drag.node && box(id)) parts.push(rect(box(id)!.rect, "flow"));
      if (drag.how === "free") parts.push(rect(t.within, "slot"));
      const land = drag.snapped?.cell;
      if (drag.snapped?.landed) for (const l of drag.snapped.landed) parts.push(rect(l.cell, "landing"));
      else if (land && drag.how === "order") parts.push(line(land[0], land[1], land[0] + land[2], land[1] + land[3], "landing-line"));
      else if (land) parts.push(rect(land, "landing"));
    }
    const typed = text.node() !== undefined;
    const by: [number, number] = drag?.kind === "move" ? [drag.at[0] - drag.from[0], drag.at[1] - drag.from[1]] : [0, 0];
    for (const node of also) {
      const other = box(node);
      if (other) parts.push(rect(moved(other.rect, by), "selected"));
    }
    const first = box(selected);
    if (first) {
      const r = drag?.kind === "move" ? moved(first.rect, by) : first.rect;
      parts.push(rect(r, typed ? "selected typed" : "selected"));
      if (!drag && !typed && also.length === 0 && aim && snapOf(aim, editor.at(first.node), true, false)) {
        const s = 8 * u;
        const [x, y, w, h] = r;
        const spot: Record<Edge, [number, number]> = {
          nw: [x, y], n: [x + w / 2, y], ne: [x + w, y], e: [x + w, y + h / 2],
          se: [x + w, y + h], s: [x + w / 2, y + h], sw: [x, y + h], w: [x, y + h / 2],
        };
        for (const edge of EDGES) {
          const [cx, cy] = spot[edge];
          parts.push(rect([cx - s / 2, cy - s / 2, s, s], "handle", ` data-edge="${edge}" style="cursor:${CURSORS[edge]}-resize"`));
        }
      }
    }
    const over = box(hovered);
    if (over && !chosen().includes(over.node) && !drag && !typed && !marquee) parts.push(rect(over.rect, "hover"));
    if (marquee) {
      const [x, y] = [Math.min(marquee.from[0], marquee.at[0]), Math.min(marquee.from[1], marquee.at[1])];
      parts.push(rect([x, y, Math.abs(marquee.at[0] - marquee.from[0]), Math.abs(marquee.at[1] - marquee.from[1])], "marquee"));
    }
    if (marked && box(marked.node)) for (const r of marked.rects) parts.push(rect(r, "found"));
    parts.push(...text.parts(u));
    svg.innerHTML = parts.join("");
  }

  /** Say where the drag lands, and which states that changes. */
  function tell(d: Drag) {
    const state = editor.shown()?.state;
    const op = d.snapped?.patch[0] as { at?: Placement } | undefined;
    if (d.with.length) {
      const n = d.states?.length ?? 0;
      const where = n === 1 && d.states?.[0] === state ? "in this state" : `in ${n} states`;
      if (!op) return editor.say(`${d.with.length + 1} selected stay where they are`);
      return editor.say(`${d.with.length + 1} selected move together · ${where}${d.alt ? ` · kept to ${state}` : ""}`);
    }
    if (!d.how) return editor.say(`${d.node} is placed by name: drag it into another slot, or with Shift off the grid`);
    if (!op) return editor.say(`${d.node} stays where it is`);
    const n = d.states?.length ?? 0;
    const where = n === 1 && d.states?.[0] === state ? "in this state" : `in ${n} states`;
    const keep = d.alt ? ` · kept to ${state}` : n > 1 ? ` · Alt keeps it to ${state}` : "";
    editor.say(`${d.node} → ${placed(op.at ?? {}, d.targets.by)} · ${where}${keep}`);
  }

  /** The box `d` leaves, and how it snaps, with the pointer where it is now. */
  function aimed(d: Drag): { how?: SnapMode; to: Rect; by: [number, number] } {
    const by: [number, number] = [d.at[0] - d.from[0], d.at[1] - d.from[1]];
    // Several move as the first does: on the grid, by its tracks, or with Shift off it.
    const how = d.with.length ? (d.shift ? "free" : "move") : snapOf(d.targets, editor.at(d.node), d.kind === "resize", d.shift);
    const to = d.kind === "move" ? moved(d.targets.cell, by) : resized(d.targets.cell, d.edge!, by);
    return { how, to, by };
  }

  /** Ask where the drag lands now, and paint its node there: one request at a time, the latest
   * pointer's. */
  async function pump(d: Drag) {
    const shown = editor.shown();
    if (busy || drag !== d || !shown) return;
    const { how, to, by } = aimed(d);
    const asked = JSON.stringify([by, how, d.alt]);
    if (asked === d.asked) return;
    d.asked = asked;
    d.how = how;
    busy = true;
    try {
      const move = d.with.length
        ? { by, with: d.with, together: { by, free: d.shift, fork: d.alt } }
        : { by: d.kind === "move" ? by : undefined, snap: how ? { how, to, fork: d.alt } : undefined };
      const reply = await stage.drag(shown.state, d.node, move, editor.format());
      if (drag !== d) return;
      d.snapped = how ? reply.snapped : null;
      d.states = reply.states;
      draw();
      tell(d);
      if (d.kind === "resize") {
        clearTimeout(d.pause);
        d.pause = setTimeout(() => void preview(d), PAUSE);
      }
    } catch (e) {
      editor.say(`error: ${said(e)}`);
    } finally {
      busy = false;
      if (drag === d) void pump(d);
    }
  }

  /** A resize that paused: the preview shows the node laid out as its patch would make it. */
  async function preview(d: Drag) {
    const shown = editor.shown();
    if (busy || drag !== d || !shown || !d.how || !d.snapped?.patch.length) return;
    busy = true;
    try {
      const { how, to } = aimed(d);
      if (how) await stage.drag(shown.state, d.node, { snap: { how, to, fork: d.alt }, preview: true }, editor.format());
    } catch (e) {
      editor.say(`error: ${said(e)}`);
    } finally {
      busy = false;
      if (drag === d) void pump(d);
    }
  }

  /** The drag ends where the pointer let go: the patch where it lands, or, where it lands nowhere
   * new, the state as it stands. */
  async function drop(d: Drag) {
    clearTimeout(d.pause);
    const shown = editor.shown();
    if (!shown || editor.version() !== d.version) return still("the source changed under the drag: nothing is placed");
    const { how, to, by } = aimed(d);
    let snapped: Snapped | null | undefined;
    try {
      const together = { with: d.with, together: { by, free: d.shift, fork: d.alt } };
      if (d.with.length) snapped = (await stage.drag(shown.state, d.node, together, editor.format())).snapped;
      else if (how) snapped = (await stage.drag(shown.state, d.node, { snap: { how, to, fork: d.alt } }, editor.format())).snapped;
    } catch (e) {
      return still(`not placed: ${said(e)}`);
    }
    if (!snapped?.patch.length) return still(d.with.length ? `${d.with.length + 1} selected stay where they are` : `${d.node} stays where it is`);
    await commit(snapped.patch, d.with.length ? `${d.with.length + 1} selected moved together` : undefined);
  }

  /** The state shown as it stands, after a drag that places nothing. */
  async function still(why: string) {
    drag = undefined;
    draw();
    editor.say(why);
    const shown = editor.shown();
    if (shown) await stage.rest(shown.state, editor.format()).catch((e) => editor.say(`error: ${said(e)}`));
  }

  /** Make `ops`, by the user, on the source as it stands: one change to undo; `done` is what the
   * status says of it, else where its node is placed. */
  async function commit(ops: unknown[], done?: string) {
    const shown = editor.shown();
    if (!shown) return;
    const op = ops[0] as { node?: string; at?: Placement } | undefined;
    editor.say("placing…");
    try {
      const { source, edited } = await stage.make(editor.source(), ops, shown.index, editor.format());
      editor.apply(source, edited);
      await refresh();
      editor.say(done ?? `${op?.node} placed: ${placed(op?.at ?? {}, aim?.by)}`);
    } catch (e) {
      await still(`not placed: ${said(e)}`);
    }
  }

  /** Make `ops`, a node added or taken away, on the source as it stands: one change to undo. Then
   * `next` is selected (several, children of one container, PLAN 2.42; nothing with `null`), and
   * the status says `done`. */
  async function change(ops: unknown[], doing: string, done: string, next?: string | string[] | null) {
    const shown = editor.shown();
    if (!shown) return;
    editor.say(doing);
    try {
      const { source, edited } = await stage.make(editor.source(), ops, shown.index, editor.format());
      editor.apply(source, edited);
      await refresh();
      if (Array.isArray(next)) selectAll(next.filter((n) => box(n) !== undefined));
      else if (next !== undefined) select(next ?? undefined);
      editor.say(done);
    } catch (e) {
      editor.say(`not made: ${said(e)}`);
    }
  }

  /** Where what is inserted or pasted goes: where the pointer last pressed, on the canvas, or its
   * middle. */
  const landing = (): [number, number] =>
    pointed ? [Math.min(Math.max(pointed[0], 0), size[0]), Math.min(Math.max(pointed[1], 0), size[1])] : [size[0] / 2, size[1] / 2];

  /** Insert what the deck offers `n`th (`Stage.inserts`) where the pointer last pressed, or in the
   * middle of the canvas: it enters in the state shown, selected. */
  function insert(n: number, label = "it") {
    return inTurn(async () => {
      const shown = editor.shown();
      if (!shown) return editor.say("the canvas waits for a source that compiles");
      try {
        const added = await stage.inserting(editor.source(), shown.state, n, landing(), editor.format());
        await change(added.patch, "inserting…", `${label} inserted as ${added.id}, in ${shown.state}`, added.id);
      } catch (e) {
        editor.say(`not inserted: ${said(e)}`);
      }
    });
  }

  /** A copy of `node` beside it, selected. */
  function duplicate(node: string) {
    return inTurn(async () => {
      const shown = editor.shown();
      if (!shown) return;
      try {
        const added = await stage.duplicating(editor.source(), shown.state, node, editor.format());
        await change(added.patch, "duplicating…", `${node} copied as ${added.id}`, added.id);
      } catch (e) {
        editor.say(`not copied: ${said(e)}`);
      }
    });
  }

  /** Take `node`, with what it holds, out of the state shown and the states after it, or,
   * `everywhere`, out of the deck; `how` says what took it (a cut). */
  function remove(node: string, everywhere: boolean, how = "deleted") {
    return inTurn(async () => {
      const shown = editor.shown();
      if (!shown) return;
      try {
        const ops = await stage.deleting(editor.source(), shown.state, node, everywhere);
        // A node no state shows once it is out of this one goes from the deck.
        const gone = ops.every((op) => (op as { op?: string }).op === "remove_node");
        const done = gone ? `${node} ${how} from the deck` : `${node} ${how} from ${shown.state} on`;
        await change(ops, `${how === "deleted" ? "deleting" : "cutting"}…`, done, null);
      } catch (e) {
        editor.say(`not ${how}: ${said(e)}`);
      }
    });
  }

  /** Take `nodes`, children of one container, out of the state shown on, or, `everywhere`, out of
   * the deck, as Delete takes one (PLAN 2.42): one patch. `how` says what took them (a cut). */
  function removeAll(nodes: string[], everywhere: boolean, how = "deleted") {
    return inTurn(async () => {
      const shown = editor.shown();
      if (!shown) return;
      try {
        const source = editor.source();
        const ops = (await Promise.all(nodes.map((n) => stage.deleting(source, shown.state, n, everywhere)))).flat();
        await change(ops, `${how === "deleted" ? "deleting" : "cutting"}…`, `${nodes.length} ${how}: ${nodes.join(", ")}`, null);
      } catch (e) {
        editor.say(`not ${how}: ${said(e)}`);
      }
    });
  }

  /** A copy of each of `nodes` beside it, as ⌘D makes one (PLAN 2.42): one patch, the copies
   * selected. */
  function duplicateAll(nodes: string[]) {
    return inTurn(async () => {
      const shown = editor.shown();
      if (!shown) return;
      try {
        const source = editor.source();
        const added = await Promise.all(nodes.map((n) => stage.duplicating(source, shown.state, n, editor.format())));
        const ids = added.map((a) => a.id);
        if (new Set(ids).size !== ids.length) throw new Error(`two copies would take one id: ${ids.join(", ")}`);
        await change(added.flatMap((a) => a.patch), "duplicating…", `${nodes.length} copied as ${ids.join(", ")}`, ids);
      } catch (e) {
        editor.say(`not copied: ${said(e)}`);
      }
    });
  }

  /** What is selected in a new group where it stands (PLAN 2.43): one patch, the group selected. */
  function group() {
    return inTurn(async () => {
      const shown = editor.shown();
      const nodes = chosen();
      if (!shown || !nodes.length) return;
      try {
        const grouped = await stage.grouping(editor.source(), shown.state, nodes);
        await change(grouped.patch, "grouping…", `${nodes.join(", ")} grouped as ${grouped.id}`, grouped.id);
      } catch (e) {
        editor.say(`not grouped: ${said(e)}`);
      }
    });
  }

  /** Mark characters `from`..`to` of `node`'s text in the state shown, counted as the page counts
   * a string (UTF-16), selecting the node; with none, take the mark away. */
  async function mark(node?: string, from = 0, to = 0) {
    marked = undefined;
    const shown = editor.shown();
    if (node !== undefined && shown) {
      const carets = await stage.carets(editor.source(), shown.state, node, editor.format()).catch(() => null);
      if (carets) marked = { node, rects: covered(carets, from, to) };
      if (selected !== node || also.length) select(node);
    }
    draw();
  }

  /** The image whose focal point the next press on it picks (PLAN 2.45). */
  let picking: string | undefined;
  /** Pick the focal point of the image selected: the next press on it sets it to the point of the
   * image under the pointer, where its subject is (PLAN 2.45). Escape leaves it as it is. */
  function pick() {
    if (selected === undefined || also.length) return editor.say("select one image to pick its focal point");
    picking = selected;
    overlay.classList.add("picking");
    overlay.focus();
    editor.say(`click ${selected} where its subject is · Escape leaves its focal point as it is`);
  }
  const unpick = () => {
    picking = undefined;
    overlay.classList.remove("picking");
  };

  /** Image `node`'s focal point, the point of it the engine draws under `at`: one `choose`. */
  function focus(node: string, at: [number, number]) {
    return inTurn(async () => {
      const shown = editor.shown();
      if (!shown) return;
      const focal = await stage.focalAt(editor.source(), shown.state, node, at, editor.format()).catch(() => null);
      if (!focal) return editor.say(`${node} is not drawn there: its focal point is as it was`);
      const op = { op: "choose", node, prop: "focal", value: focal, state: shown.state };
      await change([op], "choosing…", `${node}'s focal point: ${focal.join(", ")}`, node);
    });
  }

  /** An image file dropped on an image takes its place (PLAN 2.45): the file joins the bundle,
   * named by its SHA-256 as one dropped on the source is, and the image's `src` is its path, one
   * `choose` written where `src` lives. */
  function replace(at: [number, number], file: File) {
    return inTurn(async () => {
      const shown = editor.shown();
      if (!shown) return editor.say("the canvas waits for a source that compiles");
      const top = (await stage.hit(shown.state, at, editor.format()).catch(() => []))[0];
      const choices = top && (await stage.choices(shown.state, top.node).catch(() => undefined));
      if (!top || choices?.type !== "image") return editor.say("drop an image on an image to put it in its place; on the source, its path goes where it is dropped");
      // An image shows a PNG, in v1 (SPEC §3.3): anything else stays out of the bundle.
      if (!/\.png$/i.test(file.name)) return editor.say(`${file.name} is not a PNG: ${top.node} shows a PNG, and is as it was`);
      try {
        const path = await stage.drop(file.name, await file.arrayBuffer());
        const op = { op: "choose", node: top.node, prop: "src", value: path, state: shown.state };
        await change([op], "replacing…", `${top.node} shows ${file.name}, kept as ${path}`, top.node);
      } catch (e) {
        editor.say(`not replaced: ${said(e)}`);
      }
    });
  }
  overlay.addEventListener("dragover", (e) => {
    if (e.dataTransfer?.types.includes("Files")) e.preventDefault();
  });
  overlay.addEventListener("drop", (e) => {
    const file = e.dataTransfer?.files[0];
    if (!file) return;
    e.preventDefault();
    void replace(point(e), file);
  });

  /** The group selected taken apart (PLAN 2.43): its children out to its container where they
   * stand, all of them selected; one patch. */
  function ungroup() {
    return inTurn(async () => {
      const node = selected;
      if (node === undefined || also.length) return editor.say("select one group to take it apart");
      const held = boxes.filter((b) => b.parent === node).map((b) => b.node);
      try {
        await change([{ op: "ungroup", group: node }], "ungrouping…", `${node} taken apart: ${held.join(", ")}`, held);
      } catch (e) {
        editor.say(`not ungrouped: ${said(e)}`);
      }
    });
  }

  /** What is selected arranged `how` (PLAN 2.42): aligned, spread, or put in front or behind;
   * one patch. `fork` keeps it to the state shown. */
  function arrange(how: Arrange, fork = false) {
    return inTurn(async () => {
      const shown = editor.shown();
      const nodes = chosen();
      if (!shown || !nodes.length) return;
      const what = Object.entries(how)[0]?.join(" ") ?? "";
      try {
        const arranged = await stage.arranging(shown.state, nodes, how, fork, editor.format());
        if (!arranged?.patch.length) return editor.say(`${nodes.join(", ")}: ${what}, where they are already`);
        // What is selected stays so: the canvas keeps it as the source changes.
        await commit(arranged.patch, `${nodes.join(", ")}: ${what}`);
      } catch (e) {
        editor.say(`not arranged: ${said(e)}`);
      }
    });
  }

  /** ⌘C, and ⌘X with `cut`: what is selected onto the clipboard, as a clip and as its text; a
   * cut then takes it out of the state shown on, as Delete does. */
  function copy(e: ClipboardEvent, cut: boolean) {
    const shown = editor.shown();
    const nodes = chosen();
    if (!copies() || !nodes.length || !shown) return;
    e.preventDefault();
    const at = held !== undefined && held.key === holding() ? held : undefined;
    const them = nodes.length > 1 ? `${nodes.length} selected` : nodes[0];
    const done = () => editor.say(`${them} ${cut ? "cut" : "copied"}: ⌘V pastes ${nodes.length > 1 ? "them" : "it"}, in this deck or another`);
    if (at?.text !== undefined && e.clipboardData) {
      e.clipboardData.setData(CLIP, at.text);
      e.clipboardData.setData("text/plain", at.text);
      done();
    } else {
      // Not at hand yet: written once it is, as the page may write the clipboard, as text.
      const clip = at?.clip ?? stage.copying(editor.source(), shown.state, nodes, editor.format());
      const blob = clip.then((t) => new Blob([t], { type: "text/plain" }));
      const write =
        typeof ClipboardItem === "function"
          ? navigator.clipboard.write([new ClipboardItem({ "text/plain": blob })])
          : clip.then((t) => navigator.clipboard.writeText(t));
      write.then(done, (err) => editor.say(`not ${cut ? "cut" : "copied"}: ${said(err)}`));
    }
    if (cut) void (nodes.length > 1 ? removeAll(nodes, false, "cut") : remove(nodes[0], false, "cut"));
  }

  /** ⌘V: what the clipboard holds, `clip`, where the pointer last pressed, as Insert places a
   * node: a clip's nodes under ids new to the deck, or other text as a text in the theme's body
   * role. It enters in the state shown, selected; the status says what the theme lacked. */
  function paste(clip: string) {
    return inTurn(async () => {
      const shown = editor.shown();
      if (!shown) return editor.say("the canvas waits for a source that compiles");
      try {
        const pasted = await stage.pasting(editor.source(), shown.state, clip, landing(), editor.format());
        const lacked = pasted.findings.map((f) => `${f.message.replace(/, which has .*$/, "")}, ${f.hint ?? "taken out"}`);
        const ids = [pasted.id, ...(pasted.also ?? [])];
        const done = [`${ids.join(", ")} pasted in ${shown.state}`, ...lacked].join("; ");
        await change(pasted.patch, "pasting…", done, ids.length > 1 ? ids : pasted.id);
      } catch (e) {
        editor.say(`not pasted: ${said(e)}`);
      }
    });
  }

  /** Whether the canvas takes a copy, a cut, or a paste: it has the focus, and no text is typed
   * in, whose own clipboard it is; a copy or a cut, of a node selected. */
  const pastes = () => document.activeElement === overlay && text.node() === undefined && !drag && !starting;
  const copies = () => pastes() && selected !== undefined && editor.shown() !== undefined;
  // The browser fires a clipboard event at the page's text selection, wherever it was left, else
  // at the focus; and where nothing is editable, Chromium and WebKit fire one from the keys only
  // where the page cancels the `before…` event that asks whether it takes it. So the canvas hears
  // them on the document, and takes each while it has the focus.
  const clipboard: [string, (e: ClipboardEvent) => void][] = [
    ["beforecopy", (e) => copies() && e.preventDefault()],
    ["beforecut", (e) => copies() && e.preventDefault()],
    ["beforepaste", (e) => pastes() && e.preventDefault()],
    ["copy", (e) => copy(e, false)],
    ["cut", (e) => copy(e, true)],
    [
      "paste",
      (e) => {
        if (!pastes()) return;
        const clip = e.clipboardData?.getData(CLIP) || e.clipboardData?.getData("text/plain");
        if (!clip) return;
        e.preventDefault();
        void paste(clip);
      },
    ],
  ];
  for (const [type, hear] of clipboard) document.addEventListener(type, hear as EventListener);

  /** Move the node selected a step, or, to `grow` it, resize it: a track on a grid, a place along
   * its stack, a canvas unit off the grid. `fork` keeps it to the state shown. */
  async function nudge(node: string, [sx, sy]: [number, number], grow: boolean, fork: boolean) {
    const shown = editor.shown();
    if (!shown) return;
    const t = await stage.targets(shown.state, node, editor.format());
    const how = snapOf(t, editor.at(node), grow, false);
    const [x, y, w, h] = t.cell;
    let to: Rect | undefined;
    if (how === "order") {
      const flow = t.flow ?? [];
      const next = box(flow[flow.indexOf(node) + (sx || sy)])?.rect;
      // Past the middle of the one before or after it.
      const s = sx + sy;
      if (next) to = [next[0] + next[2] / 2 + s - w / 2, next[1] + next[3] / 2 + s - h / 2, w, h];
    } else if (how === "free") {
      to = grow ? [x, y, w + sx, h + sy] : [x + sx, y + sy, w, h];
    } else if (how === "move" || how === "resize") {
      // The next track's start (a move) or end (a resize) past the box's start or end.
      const step = (tracks: [number, number][], from: number, s: number, end: 0 | 1) => {
        const lines = tracks.map((track) => track[end]);
        const found = s > 0 ? lines.find((v) => v > from + 0.5) : [...lines].reverse().find((v) => v < from - 0.5);
        return found === undefined ? undefined : found - from;
      };
      const [cols, rows] = [t.columns ?? [], t.rows ?? []];
      const d = grow
        ? [sx ? step(cols, x + w, sx, 1) : 0, sy ? step(rows, y + h, sy, 1) : 0]
        : [sx ? step(cols, x, sx, 0) : 0, sy ? step(rows, y, sy, 0) : 0];
      if (d[0] !== undefined && d[1] !== undefined) to = grow ? [x, y, w + d[0], h + d[1]] : [x + d[0], y + d[1], w, h];
    }
    if (also.length && to) {
      // Several move as far as the first steps, together (PLAN 2.42).
      const nodes = chosen();
      try {
        const by: [number, number] = [to[0] - x, to[1] - y];
        const arranged = await stage.arranging(shown.state, nodes, { by, free: how === "free" }, fork, editor.format());
        if (!arranged?.patch.length) return editor.say(`${nodes.length} selected stay where they are`);
        return await commit(arranged.patch, `${nodes.length} selected moved together`);
      } catch (e) {
        return editor.say(`not placed: ${said(e)}`);
      }
    }
    if (!how) return editor.say(`${node} is placed by name: drag it into another slot`);
    if (!to) return editor.say(`${node} is at the edge`);
    try {
      const { snapped } = await stage.drag(shown.state, node, { snap: { how, to, fork } }, editor.format());
      if (!snapped?.patch.length) return editor.say(`${node} stays where it is`);
      await commit(snapped.patch);
    } catch (e) {
      editor.say(`not placed: ${said(e)}`);
    }
  }

  overlay.onpointerdown = async (e) => {
    // The middle button, or a drag with Space held, pans what is zoomed in (PLAN 2.46).
    if (e.button === 1 || (e.button === 0 && spaced)) {
      e.preventDefault();
      if (zoom() <= 1) return;
      overlay.setPointerCapture(e.pointerId);
      panning = { from: [...view], client: [e.clientX, e.clientY], pointer: e.pointerId };
      overlay.classList.add("panning");
      return;
    }
    if (e.button !== 0) return;
    // A press that picks an image's focal point picks it, and does nothing else (PLAN 2.45).
    if (picking !== undefined) {
      e.preventDefault();
      const node = picking;
      unpick();
      return void focus(node, point(e));
    }
    const count = clicks(e);
    const from = point(e);
    const shift = e.shiftKey;
    pointed = from;
    // Typing: a press in the text puts the caret there, and one outside it stops typing.
    if (text.node() !== undefined) {
      if (text.down(from, e.shiftKey, count)) {
        e.preventDefault();
        overlay.setPointerCapture(e.pointerId);
        return;
      }
      text.leave();
    }
    overlay.focus();
    const shown = editor.shown();
    if (!shown || drag) return editor.say(shown ? "" : "the canvas waits for a source that compiles");
    overlay.setPointerCapture(e.pointerId);
    const client: [number, number] = [e.clientX, e.clientY];
    const edge = (e.target as Element).closest?.("[data-edge]")?.getAttribute("data-edge") as Edge | null;
    if (edge && selected !== undefined) {
      press = { node: selected, edge, from, client };
      return;
    }
    const mine: Press = { from, client, asking: true, shift };
    press = mine;
    const hits = await stage.hit(shown.state, from, editor.format()).catch(() => []);
    mine.asking = false;
    const top = hits[0];
    const chain = top ? [top.node, ...top.containers] : [];
    // In what is selected, or in what holds it, the press keeps it: a drag moves it, with the
    // rest of what is selected, and a click selects what is topmost.
    const all = chosen();
    const keeps = selected !== undefined && chain.some((n) => all.includes(n));
    const node = keeps ? selected : top?.node;
    if (keeps) mine.with = also.slice();
    // With Shift, a click puts what was clicked in the selection, or takes it out of it: in what
    // holds the selection, the child of it the click is in.
    if (shift && selected !== undefined) {
      const holds = holder(selected);
      mine.toggle = chain.find((n) => all.includes(n)) ?? chain.find((n) => holder(n) === holds) ?? top?.node;
    }
    // On nothing, or on what fills the canvas behind all, a drag draws a marquee.
    if (!keeps && (!top || covers(box(top.node)?.rect))) mine.marquee = true;
    // Moved past the slop before the engine answered, as a drag made while the worker paints is:
    // the press was a drag all along, from where the pointer is now, dropped there if let go.
    const moved = mine.moved;
    const far = moved !== undefined && Math.hypot(moved.client[0] - client[0], moved.client[1] - client[1]) >= SLOP;
    if (far && mine.marquee && (press === mine || mine.released)) {
      if (press === mine) press = undefined;
      marquee = { from, at: moved.at, adding: shift };
      if (mine.released) return finish();
      return draw();
    }
    if (far && node !== undefined && mine.toggle === undefined && (press === mine || mine.released)) {
      if (press === mine) press = undefined;
      early++;
      if (!keeps) select(node);
      return begin({ ...mine, node }, { at: moved.at, shift: moved.shift, alt: moved.alt, up: mine.released });
    }
    // A click let go before the engine answered selects what is topmost.
    if (mine.released) return mine.toggle !== undefined ? toggle(mine.toggle) : select(top?.node);
    if (press !== mine) return;
    if (keeps) [mine.node, mine.click] = [selected, top?.node];
    else if (mine.toggle !== undefined || mine.marquee) mine.click = top?.node;
    else {
      select(top?.node);
      mine.node = top?.node;
    }
  };

  /** The marquee let go: what it encloses is selected. */
  function finish() {
    const m = marquee;
    marquee = undefined;
    if (!m) return;
    const found = enclosed(m);
    if (found.length) selectAll(found);
    else if (!m.adding) select(undefined);
    draw();
  }

  /** Drag `begun`'s node, the pointer as `now` says, once the engine says where it may go; let go
   * meanwhile, it drops there. */
  function begin(begun: Press, now: Starting) {
    const shown = editor.shown();
    if (!shown) return;
    const version = editor.version();
    starting = now;
    void stage
      .targets(shown.state, begun.node!, editor.format())
      .then((targets) => {
        if (starting !== now) return;
        starting = undefined;
        const d: Drag = {
          kind: begun.edge ? "resize" : "move",
          node: begun.node!,
          with: begun.edge ? [] : (begun.with ?? []),
          edge: begun.edge,
          from: begun.from,
          at: now.at,
          shift: now.shift,
          alt: now.alt,
          version,
          targets,
        };
        hovered = undefined;
        if (now.up) return void inTurn(() => drop(d));
        drag = d;
        void pump(d);
      })
      .catch((err) => {
        starting = undefined;
        editor.say(`error: ${said(err)}`);
      });
  }

  // A double click on a text types in it, where it was clicked: in what is topmost there, which its
  // first click selects.
  overlay.ondblclick = (e) => {
    if (text.node() !== undefined || drag) return;
    const [at, alt] = [point(e), e.altKey];
    void inTurn(async () => {
      const shown = editor.shown();
      const top = shown && (await stage.hit(shown.state, at, editor.format()).catch(() => []))[0];
      if (top) await type(top.node, at, alt);
    });
  };

  overlay.onpointermove = (e) => {
    if (panning) {
      const [p, u] = [panning, unit()];
      return void look([p.from[0] - (e.clientX - p.client[0]) * u, p.from[1] - (e.clientY - p.client[1]) * u, p.from[2], p.from[3]]);
    }
    const at = point(e);
    if (text.drag(at)) return;
    if (starting) {
      [starting.at, starting.shift, starting.alt] = [at, e.shiftKey, e.altKey];
      return;
    }
    if (drag) {
      [drag.at, drag.shift, drag.alt] = [at, e.shiftKey, e.altKey];
      if (drag.kind === "move") draw();
      void pump(drag);
      return;
    }
    if (marquee) {
      marquee.at = at;
      return draw();
    }
    if (!press) {
      // What a click would select, from the boxes the engine gave: nothing is asked.
      const under = boxes.filter((b) => b.draws && inside(b.rect, at)).at(-1)?.node;
      if (under !== hovered) {
        hovered = under;
        draw();
      }
      return;
    }
    if (press.asking) {
      press.moved = { at, shift: e.shiftKey, alt: e.altKey, client: [e.clientX, e.clientY] };
      return;
    }
    const far = Math.hypot(e.clientX - press.client[0], e.clientY - press.client[1]) >= SLOP;
    if (far && press.marquee) {
      marquee = { from: press.from, at, adding: press.shift === true };
      press = undefined;
      return draw();
    }
    if (!press.node || !far) return;
    const begun = press;
    press = undefined;
    begin(begun, { at, shift: e.shiftKey, alt: e.altKey });
  };

  overlay.onpointerup = (e) => {
    if (overlay.hasPointerCapture(e.pointerId)) overlay.releasePointerCapture(e.pointerId);
    if (panning?.pointer === e.pointerId) {
      panning = undefined;
      if (!spaced) overlay.classList.remove("panning");
      return;
    }
    if (text.up()) return;
    if (starting) {
      [starting.at, starting.shift, starting.alt, starting.up] = [point(e), e.shiftKey, e.altKey, true];
      return;
    }
    if (drag) {
      const d = drag;
      drag = undefined;
      [d.at, d.shift, d.alt] = [point(e), e.shiftKey, e.altKey];
      void inTurn(() => drop(d));
      return;
    }
    if (marquee) {
      marquee.at = point(e);
      return finish();
    }
    const clicked = press;
    press = undefined;
    if (!clicked) return;
    if (clicked.asking) {
      clicked.moved = { at: point(e), shift: e.shiftKey, alt: e.altKey, client: [e.clientX, e.clientY] };
      clicked.released = true;
    } else if (clicked.toggle !== undefined) toggle(clicked.toggle);
    else if (clicked.marquee) select(clicked.click);
    else if (clicked.click !== undefined) select(clicked.click);
  };

  overlay.onpointercancel = () => {
    press = starting = marquee = undefined;
    if (drag) void still("the drag was cancelled");
  };

  overlay.onpointerleave = () => {
    if (hovered === undefined) return;
    hovered = undefined;
    draw();
  };

  overlay.onkeydown = (e) => {
    const mod = e.metaKey || e.ctrlKey;
    const key = e.key.toLowerCase();
    // Zoom (PLAN 2.46): ⌘+ and ⌘− a step about the middle of what is shown, ⌘0 the whole canvas;
    // while typing too, which no text takes.
    if (mod && !e.altKey && ["=", "+", "-", "_", "0"].includes(key) && !drag) {
      e.preventDefault();
      return void (key === "0" ? fit() : zoomStep(key === "-" || key === "_" ? -1 : 1));
    }
    // The text typed in takes its own keys.
    if (text.node() !== undefined) return;
    if (picking !== undefined && e.key === "Escape") {
      e.preventDefault();
      unpick();
      return editor.say("the focal point is as it was");
    }
    if (e.key === " " && !mod && !drag) {
      e.preventDefault();
      spaced = true;
      if (zoom() > 1) overlay.classList.add("panning");
      return;
    }
    if (e.key === "Enter" && selected !== undefined && !drag && !mod) {
      e.preventDefault();
      return void type(selected, undefined, e.altKey);
    }
    if (mod && (key === "z" || key === "y")) {
      e.preventDefault();
      return key === "y" || e.shiftKey ? editor.redo() : editor.undo();
    }
    if (selected !== undefined && !drag && !starting) {
      if ((e.key === "Delete" || e.key === "Backspace") && !mod && !e.altKey) {
        e.preventDefault();
        return void (also.length ? removeAll(chosen(), e.shiftKey) : remove(selected, e.shiftKey));
      }
      if (mod && key === "d" && !e.shiftKey && !e.altKey) {
        e.preventDefault();
        return void (also.length ? duplicateAll(chosen()) : duplicate(selected));
      }
      // ⌘G groups what is selected, and ⌘⇧G takes the group selected apart (PLAN 2.43).
      if (mod && key === "g" && !e.altKey) {
        e.preventDefault();
        return void (e.shiftKey ? ungroup() : group());
      }
      // ⌘] and ⌘[: in front of, or behind, the next it overlaps; with Shift, of all (PLAN 2.42).
      if (mod && (e.code === "BracketRight" || e.code === "BracketLeft") && !e.altKey) {
        e.preventDefault();
        const up = e.code === "BracketRight";
        return void arrange({ order: e.shiftKey ? (up ? "front" : "back") : up ? "forward" : "backward" });
      }
    }
    if (e.key === "Escape") {
      if (starting) {
        e.preventDefault();
        starting = undefined;
        return editor.say("the drag was cancelled");
      }
      if (drag) {
        e.preventDefault();
        return void still("the drag was cancelled");
      }
      if (selected !== undefined) {
        e.preventDefault();
        if (also.length) select(selected);
        else select(box(selected)?.parent ?? undefined);
        if (selected === undefined) editor.say("nothing selected");
      }
      return;
    }
    const steps: Record<string, [number, number]> = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
    const step = steps[e.key];
    if (!step || selected === undefined || drag || mod) return;
    e.preventDefault();
    const [node, grow, fork] = [selected, e.shiftKey, e.altKey];
    if (also.length && grow) return editor.say("several move together; resize one at a time");
    void inTurn(() => nudge(node, step, grow, fork));
  };

  overlay.onkeyup = (e) => {
    if (e.key !== " ") return;
    spaced = false;
    if (!panning) overlay.classList.remove("panning");
  };
  overlay.addEventListener("blur", () => {
    spaced = false;
    if (!panning) overlay.classList.remove("panning");
  });
  // Pinch, or the wheel with ⌘ or Ctrl, zooms about the pointer; the wheel alone pans what is zoomed
  // in, and scrolls the page past what is not (PLAN 2.46).
  overlay.addEventListener(
    "wheel",
    (e) => {
      if (e.ctrlKey || e.metaKey) {
        e.preventDefault();
        return void zoomTo(Math.min(MAX_ZOOM, Math.max(1, zoom() * Math.exp(-e.deltaY / 240))), point(e));
      }
      if (zoom() <= 1) return;
      e.preventDefault();
      const u = unit() * (e.deltaMode === WheelEvent.DOM_DELTA_LINE ? 16 : 1);
      void look([view[0] + e.deltaX * u, view[1] + e.deltaY * u, view[2], view[3]]);
    },
    { passive: false },
  );

  const sized = new ResizeObserver(() => draw());
  sized.observe(overlay);

  return {
    refresh,
    /** Zoom the preview (PLAN 2.46): a step closer or farther, as ⌘+ and ⌘− do, the whole canvas, as
     * ⌘0 does, or as close as a number, about canvas point `about` or the middle of what is shown.
     * Resolves once the preview is painted so. */
    zoom: (how: "in" | "out" | "fit" | number, about?: [number, number]) =>
      how === "fit" ? fit() : typeof how === "number" ? zoomTo(how, about) : zoomStep(how === "in" ? 1 : -1, about),
    /** Move what is zoomed in by `dx`, `dy` canvas units, as the wheel and a drag with Space do. */
    pan: (dx: number, dy: number) => look([view[0] + dx, view[1] + dy, view[2], view[3]]),
    /** The part of the canvas shown, canvas units, and how close it is. */
    view: () => [...view] as Rect,
    zoomed: zoom,
    /** Let the overlay go: the editor opens another bundle, which makes a canvas of its own. */
    close: () => {
      sized.disconnect();
      document.removeEventListener("keydown", unpress, true);
      for (const [type, hear] of clipboard) document.removeEventListener(type, hear as EventListener);
      text.close();
      drag = press = starting = undefined;
      svg.replaceChildren();
    },
    /** Select `node`, as a click on it does; nothing with `undefined`. */
    select,
    /** Select `nodes` at once, children of one container, as Shift+click and a marquee do. */
    selectAll,
    /** Mark characters of a text in the state shown, as a find shows its match, and select it;
     * with no node, take the mark away (PLAN 2.47). */
    mark,
    /** Group what is selected, as ⌘G does, and take the group selected apart, as ⌘⇧G does. */
    group,
    ungroup,
    /** Pick the focal point of the image selected with the next press on it, as the inspector's
     * Pick does; and an image file dropped on an image, put in its place (PLAN 2.45). */
    pick,
    replace,
    /** Arrange what is selected, as the inspector and ⌘] do (PLAN 2.42). */
    arrange,
    /** Insert what the deck offers `n`th, as the Insert menu does; `label` is what the status
     * calls it. */
    insert,
    /** A copy of `node` beside it, as ⌘D does. */
    duplicate,
    /** Take `node` out of the state shown on, as Delete does; `everywhere`, out of the deck, as
     * Shift+Delete does. */
    remove,
    /** Paste `clip`, the clipboard's text, as ⌘V does. */
    paste,
    /** Give the characters selected in the text typed in `look`, as the inspector does (PLAN 2.38). */
    style: (look: Record<string, unknown>) => text.style(look),
    /** Make the characters selected bold, or not, as ⌘B does. */
    bold: () => text.bold(),
    /** What the clipboard would hold of the node selected, once it is at hand: what a test
     * waits for before ⌘C. */
    held: () => held?.clip,
    /** Where the pointer last pressed, canvas units. */
    pointed: () => pointed,
    selected: () => selected,
    /** Every node selected: the one selected, then those beside it. */
    chosen,
    boxes: () => boxes,
    /** The state the boxes stand in: what a test waits for once it shows another. */
    boxed: () => boxed,
    size: () => size,
    /** Where `node` may go in the state shown. */
    targets: (node: string) => {
      const shown = editor.shown();
      return shown ? stage.targets(shown.state, node, editor.format()) : Promise.reject(new Error("nothing is shown"));
    },
    /** Whether a drag is under way, or its request with the worker. */
    busy: () => busy || drag !== undefined || starting !== undefined,
    /** The text typed in, if one is. */
    typing: () => text.node(),
    /** What is typed in it, for a test. */
    typed: () => text.now(),
    /** How many drags began from moves made before the engine said what was pressed, as a drag
     * pressed while the worker paints or lints is. */
    early: () => early,
  };
}
