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
// - Lint's findings stand on what they are about, a mark at each box's corner, and a mark opened
//   offers each finding's fix (PLAN 2.49, `marks.ts`).
// - Guides (PLAN 2.57): ⌘' (Ctrl+'), the Grid button, or the command draws the theme's grid of the
//   format shown over the canvas, its columns and rows, the gutters and margins between them, and
//   the baseline grid. As a node moves or is resized, a line shows wherever one of its edges, or
//   its middle, meets another's or the canvas's; off the grid (Shift), a box within a few pixels
//   of one goes onto it, unless ⌘ (Ctrl) is held. The engine says where they meet; the page draws
//   what it says.
// - ⌥⌘C (Ctrl+Alt+C) copies the look of the node selected, as the state shown shows it: a text's
//   role and style, a shape's fill, stroke, and corners, an image's corners, a shader's preset and
//   palette, a chart's labels, a stack's or a grid's gap. ⌥⌘V (Ctrl+Alt+V) pastes it on each node
//   selected that takes it, one patch of `choose`s, each written where that node's own value lives,
//   as the inspector writes one; where the look's value is the theme's, the node's own is taken
//   away. A node of another type takes what its look shares with it (PLAN 2.58).
// - An image dragged from the Files panel (PLAN 2.59) takes the place of the image node it is
//   dropped on, a `choose` of `src`; dropped anywhere else, it is inserted there, as Insert does.
import { ALT, type Key, MOD, SHIFT } from "./commands";
import { marks } from "./marks";
import { BUNDLE_PATH, CLIP, DATA, PICTURE } from "./protocol";
import type { Added, Arrange, DataMark, Edited, Finding, Framing, Grid, Insert, LayoutSlots, Look, Map6, NodeBox, NoteMark, Outline, Rect, SlotBox, SnapMode, Snapped, Targets } from "./protocol";
import { pointer } from "./theme-panel";
import * as notes from "./notes";
import { annotate, askWords, markName, noteName } from "./notes";
import type { Stage } from "./stage";
import { covered, paragraphs, type Selected, typing } from "./typing";

/** What the canvas answers that no command runs by name (PLAN 2.65): the pointer's gestures and the
 * keys held with them, and the keys that move what is selected. The keys sheet lists them with the
 * commands' keys. */
export const canvasKeys = (): Key[] => [
  { keys: "Click", label: "Select what is topmost there", group: "Select" },
  { keys: "Tab, Shift+Tab", label: "Select the next, or the one before, in reading order; past the last, leave the canvas", group: "Select" },
  { keys: "Enter", label: "Go into the container or group selected: its first node; Escape goes back out", group: "Select" },
  { keys: `${SHIFT}Click`, label: "Put it in the selection, or take it out", group: "Select" },
  { keys: "Drag across nothing", label: "Select what the drag encloses", group: "Select" },
  { keys: `${SHIFT}Drag across nothing`, label: "Select what it encloses too", group: "Select" },
  { keys: `${MOD}A`, label: "Select what is selected and all beside it, or all on the canvas", group: "Select" },
  { keys: "Drag", label: "Move what is selected onto the grid; a handle resizes it", group: "Move and resize" },
  { keys: `${SHIFT}Drag`, label: "Move or resize it off the grid, or back onto it", group: "Move and resize" },
  { keys: `${MOD}Drag`, label: "Off the grid, go where the pointer says, not onto an edge it meets", group: "Move and resize" },
  { keys: "← ↑ → ↓", label: "Move what is selected a track, a place along its stack, or a unit off the grid", group: "Move and resize" },
  { keys: `${SHIFT}← ↑ → ↓`, label: "Resize it the same", group: "Move and resize" },
  { keys: `${ALT}Drag, ${ALT}← ↑ → ↓`, label: "Keep the move to the state shown", group: "Move and resize" },
  { keys: "Drag the round handle", label: "Turn it", group: "Move and resize" },
  { keys: `${SHIFT}Drag the round handle`, label: "Turn it by 15°", group: "Move and resize" },
  { keys: "[ ]", label: "Turn what is selected 15° back or on; with Shift, 1°", group: "Move and resize" },
  { keys: "Escape", label: "Cancel the drag, the turn, or the drawing under way", group: "Move and resize" },
  { keys: "Drag a point of the shape selected", label: "Move it within the shape's box: a line's, an arrow's, or a polygon's", group: "Points and corners" },
  { keys: `${ALT}Drag a point`, label: "Keep the move to the state shown", group: "Points and corners" },
  { keys: "Click an edge's middle", label: "Add a point there; a drag from it places the point", group: "Points and corners" },
  { keys: "Click a point, then Delete", label: "Take it away, though never below the two or three its kind keeps", group: "Points and corners" },
  { keys: "Drag a rect's corner", label: "Round its corners to the theme's radius steps", group: "Points and corners" },
  { keys: "Drag a crop handle inside an image", label: "Crop it from that side, the crop shown on the whole image as it goes", group: "Points and corners" },
  { keys: "Drag an image's focal point", label: "Keep that point of the image in view as its box cuts it", group: "Points and corners" },
  { keys: `${ALT}Drag a crop handle or the focal point`, label: "Keep the crop or the focal point to the state shown", group: "Points and corners" },
  { keys: "Enter on a shape or an image", label: "Go to its handles: its points, its corner, its crop's sides, its focal point", group: "Points and corners" },
  { keys: "Tab, Shift+Tab", label: "On its handles: the next handle, or the one before", group: "Points and corners" },
  { keys: "← ↑ → ↓", label: "On a handle: move it a hundredth, a corner a radius step; with Shift, a tenth", group: "Points and corners" },
  { keys: "+, Delete", label: "On a point: add one after it, or take it away", group: "Points and corners" },
  { keys: "Escape", label: "Leave the handles: the node stays selected", group: "Points and corners" },
  { keys: "Double-click a text", label: "Type in it where it was clicked", group: "Type" },
  { keys: `${ALT}Double-click, ${ALT}Enter`, label: "Type in it, what is typed kept to the state shown", group: "Type" },
  { keys: "Space Drag, Wheel", label: "Pan what is zoomed in", group: "See" },
  { keys: `${MOD}Wheel, Pinch`, label: "Zoom about the pointer", group: "See" },
  { keys: "Click a chart's mark", label: "With the Data tab shown, choose the rows it was made from", group: "The data" },
  { keys: "Double-click a chart's mark", label: "Open the Data tab on its rows", group: "The data" },
  { keys: "Click a mark of the chart selected", label: "Pick it: its menu highlights it, calls it out, rules its value, or bands from it", group: "Annotate" },
  { keys: "Click an annotation of the chart selected", label: "Select it; Delete takes it away", group: "Annotate" },
  { keys: "Drag a callout", label: "Move it onto the mark there, or to the value there", group: "Annotate" },
  { keys: `${ALT}Drag a callout`, label: "Keep the move to the state shown", group: "Annotate" },
  { keys: "Double-click an annotation", label: "Change what it says", group: "Annotate" },
  { keys: "Escape", label: "Let go of the mark picked, or the band begun", group: "Annotate" },
  { keys: "Drag a slot, the layout shown", label: "Move it onto the theme's grid: every state that uses the layout shows it moved", group: "Layouts" },
  { keys: "Drag a slot's handle", label: "Resize it onto the grid", group: "Layouts" },
  { keys: "Tab, then ← ↑ → ↓", label: "Key a slot, then move it a track; with Shift, resize it", group: "Layouts" },
  { keys: "Escape", label: "Leave the layout, or the drag under way as it was", group: "Layouts" },
];

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
  /** `node`'s `transform` in the state shown, resolved (PLAN 2.51): what its rotate handle turns. */
  transform(node: string): { rotate?: number; anchor?: [number, number] } | undefined;
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
  /** The theme's grid is drawn over the canvas, or not (PLAN 2.57). */
  ruled(on: boolean): void;
  /** Take `f`'s fix: one patch, one step to undo (PLAN 2.49). */
  fix(f: Finding): Promise<void>;
  /** Show where the source writes what `f` is about. */
  go(f: Finding): void;
  /** Open the inspector on the node `f` is about, its field for `prop` in focus (PLAN 2.56). */
  edit(f: Finding, prop: string): void;
  /** A right click, or the menu key (PLAN 2.53): offer, at `x`, `y` (client pixels), what can be
   * done to what is selected, `on` a node, or to the canvas where nothing is. */
  menu(x: number, y: number, on: "node" | "canvas"): void;
  /** A press at `at`, canvas units, on what may be a chart's mark or a table's row (PLAN 2.64): its
   * rows chosen in the data, the Data tab `open`ed on them, else only where it is shown. Whether
   * the press was on one. */
  pointedAt?(at: [number, number], open: boolean): Promise<boolean>;
  /** Whether a change is kept to the state shown, as the inspector's "Only in this state" says
   * (PLAN 2.67): an annotation made from a mark is. */
  keeping?(): boolean;
  /** Edit the theme by `ops`, RFC 6902 operations, as the Theme tab does (PLAN 2.71, ADR-0016):
   * one change, one step to undo; whether it was made (the editor says why not). */
  themeEdit?(ops: unknown[], what: string): Promise<boolean>;
  /** The canvas shows the layout's slots to edit, or stops (PLAN 2.71). */
  layouting?(on: boolean): void;
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
  /** ⌘ or Ctrl held: off the grid, it goes where the pointer says, onto no guide (PLAN 2.57). */
  loose: boolean;
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

/** A node turned by its rotate handle (PLAN 2.51): about `pivot`, where its anchor is drawn, from
 * `start` degrees to `now`, as the pointer goes round from where it pressed. */
interface Turn {
  node: string;
  pivot: Point;
  /** The pointer's last angle about the pivot, radians, and how far it has gone round since the
   * press, degrees: past a half turn it keeps going. */
  last: number;
  round: number;
  start: number;
  now: number;
  /** -1 where what holds the node mirrors it, so that its own clockwise turn goes the other way
   * on the canvas; else 1. */
  way: number;
  /** The source's version when it began. */
  version: number;
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
  /** On the one node selected already: a click there picks a chart's mark or annotation (PLAN 2.67). */
  within?: boolean;
  /** On an annotation of the chart selected: a drag moves a callout. */
  note?: NoteMark;
}

/** A shape's point, or a rect's corner, dragged by its handle (PLAN 2.68): the shape's outline when
 * the press began, where it pressed (canvas units and client px), whether it has moved past the
 * slop, and Alt. A point dragged is `index` among `points`, as they will be written (`added`: put
 * there by the press, at an edge's middle); a corner dragged, the theme's radius step it rounds to,
 * `step`. */
interface Reshape {
  state: string;
  outline: Outline;
  from: Point;
  client: Point;
  moved: boolean;
  alt: boolean;
  version: number;
  points: Point[];
  index?: number;
  added?: boolean;
  step?: number;
}

/** An image's crop, from one side, or its focal point, dragged by its handle (PLAN 2.74): its
 * framing when the press began, where it pressed (canvas units as laid out, and client px), whether
 * it has moved past the slop, Alt, and the crop and the focal point as the drag leaves them. */
interface Cropping {
  state: string;
  framing: Framing;
  side?: "n" | "e" | "s" | "w";
  from: Point;
  client: Point;
  moved: boolean;
  alt: boolean;
  version: number;
  crop: Rect;
  focal: Point;
}

/** Where a slot dragged lands on the grid (PLAN 2.71): its cells, from 1, and its box. */
interface Landing {
  col: [number, number];
  row: [number, number];
  rect: Rect;
}

/** A drag asking where its node may go: the pointer as it is now, the keys held, and whether it
 * was let go. */
interface Starting {
  at: [number, number];
  shift: boolean;
  alt: boolean;
  loose?: boolean;
  up?: boolean;
}

/** How far the pointer moves, CSS pixels, before a press is a drag. */
const SLOP = 4;
/** The keys that arm the canvas to draw (PLAN 2.48), each with what it draws of what the deck
 * offers: a text, in the theme's `body` where it has one, a rectangle, an ellipse, a line, or an
 * arrow. */
const DRAWS: Record<string, (i: Insert) => boolean> = {
  t: (i) => i.node.type === "text",
  r: (i) => i.node.type === "shape" && i.node.kind === "rect",
  o: (i) => i.node.type === "shape" && i.node.kind === "ellipse",
  l: (i) => i.node.type === "shape" && i.node.kind === "line",
  a: (i) => i.node.type === "shape" && i.node.kind === "arrow",
};
/** A drag drawing what the canvas is armed with (PLAN 2.48): where it began, in canvas units and
 * client px, where the pointer is, and whether Shift is held, off the grid; where it lands, as the
 * engine last said, and what was last asked of it. */
interface Sketch {
  from: [number, number];
  client: [number, number];
  at: [number, number];
  shift: boolean;
  cell?: Rect;
  asking?: boolean;
  asked?: string;
}
/** How soon a press follows the one before to count as a second click (or a third), ms. */
const AGAIN = 450;
/** How long a resize pauses before the preview shows it laid out, ms. */
const PAUSE = 300;
/** How near, CSS pixels, an edge or a middle moved off the grid goes onto another's (PLAN 2.57). */
const REACH = 6;

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

type Point = [number, number];

/** `p` through the map `m` (PLAN 2.51). */
const apply = (m: Map6, [x, y]: Point): Point => [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];

/** The map that undoes `m`, where one does: none for one that flattens the plane. */
function invert(m: Map6): Map6 | undefined {
  const det = m[0] * m[3] - m[1] * m[2];
  if (!Number.isFinite(det) || Math.abs(det) < 1e-12) return undefined;
  const [a, b, c, d] = [m[3] / det, -m[1] / det, -m[2] / det, m[0] / det];
  return [a, b, c, d, -(a * m[4] + c * m[5]), -(b * m[4] + d * m[5])];
}

/** `by`, a move on the canvas, in the units `m` draws in: what moves a box it maps that far. */
function across(m: Map6 | undefined, by: Point): Point {
  const back = m && invert(m);
  return back ? [back[0] * by[0] + back[2] * by[1], back[1] * by[0] + back[3] * by[1]] : by;
}

/** A box's corners, clockwise from its top left. */
const corners = ([x, y, w, h]: Rect): Point[] => [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];

/** Where `b` is drawn: its corners through its map. */
const outline = (b: NodeBox): Point[] => corners(b.rect).map((p) => (b.transform ? apply(b.transform, p) : p));

/** The box around `points`. */
function around(points: Point[]): Rect {
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  const [x, y] = [Math.min(...xs), Math.min(...ys)];
  return [x, y, Math.max(...xs) - x, Math.max(...ys) - y];
}

/** The box around where `b` is drawn: its own, where nothing moves it. */
const drawnBox = (b: NodeBox): Rect => (b.transform ? around(outline(b)) : b.rect);

/** `at`, a point on the canvas, read back into `b`'s box as laid out: none where its map
 * flattens it. */
function laidOut(b: NodeBox, at: Point): Point | undefined {
  if (!b.transform) return at;
  const back = invert(b.transform);
  return back && apply(back, at);
}

function resized([x, y, w, h]: Rect, edge: Edge, [dx, dy]: [number, number]): Rect {
  if (edge.includes("e")) w += dx;
  if (edge.includes("w")) [x, w] = [x + Math.min(dx, w - 1), w - Math.min(dx, w - 1)];
  if (edge.includes("s")) h += dy;
  if (edge.includes("n")) [y, h] = [y + Math.min(dy, h - 1), h - Math.min(dy, h - 1)];
  return [x, y, Math.max(1, w), Math.max(1, h)];
}

/** The canvas over `overlay`, which holds an `<svg>` the size of the preview, on `stage`'s state
 * shown, with lint's findings marked in `layer` over it, each opened in `pop`. The editor makes one
 * for each bundle it opens. */
export function canvas(stage: Stage, overlay: HTMLElement, editor: Editor, layer: HTMLElement, pop: HTMLElement) {
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
  /** What the rows chosen in the data draw in a state, outlined while it is shown (PLAN 2.64). */
  let rowMarks: { state: string; marks: DataMark[] } | undefined;
  /** A mark of the chart selected, picked (PLAN 2.67): what its menu's Highlight, Call out, Rule,
   * and Band annotate. */
  let picked: { state: string; mark: DataMark } | undefined;
  /** An annotation of the chart selected (PLAN 2.67): what Delete takes away. */
  let noted: { state: string; note: NoteMark } | undefined;
  /** A band begun at a mark (PLAN 2.67): the next press on a mark of its chart ends it there. */
  let banding: { state: string; from: DataMark } | undefined;
  /** A callout dragged (PLAN 2.67): where it was pressed and where the pointer is, canvas units. */
  let carrying: { state: string; note: NoteMark; from: Point; at: Point; alt: boolean } | undefined;
  /** The outline of the shape selected (PLAN 2.68): its points' handles, and a rect's corner's. */
  let shaped: { state: string; outline: Outline } | undefined;
  /** A point of the shape selected, picked by a click: what Delete takes away. */
  let pointPicked: number | undefined;
  /** A point, or a rect's corner, dragged by its handle. */
  let reshaping: Reshape | undefined;
  /** The framing of the image selected (PLAN 2.74): its crop's handles and its focal point's. */
  let imaged: { state: string; framing: Framing } | undefined;
  /** A crop handle, or the focal point, dragged. */
  let cropping: Cropping | undefined;
  /** The handle the keys work (PLAN 2.75): of the shape's or the image's handles, in order, the
   * `index`th. */
  let handling: { node: string; index: number } | undefined;
  /** The marks and annotations of the chart selected, which the keys step through (PLAN 2.75). */
  let charted: { state: string; node: string; marks: DataMark[]; notes: NoteMark[] } | undefined;
  /** The layout of the state shown, as its slots, while the canvas edits it (PLAN 2.71): the
   * state and the format it was asked in, and the grid its slots snap to. */
  let slotting: { state: string; format?: string; layout: LayoutSlots; grid?: Grid } | undefined;
  /** A slot dragged, or resized by a handle (`edge`): where the pointer pressed and is, the box
   * it leaves, and where that lands on the grid. */
  let slotDrag: { slot: SlotBox; edge?: Edge; from: Point; rect: Rect; landed?: Landing; version: number } | undefined;
  /** The slot the keys work, by its place among the layout's (PLAN 2.75). */
  let slotKeyed: number | undefined;
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
  /** Whether the theme's grid is drawn over the canvas (PLAN 2.57), and the grid: the format
   * shown's, asked again whenever what stands where is. */
  let ruled = false;
  let grid: Grid | undefined;
  let hovered: string | undefined;
  /** Where the pointer last pressed, canvas units: where Insert puts what it inserts. */
  let pointed: [number, number] | undefined;
  /** What is selected as the clipboard would hold it, asked for when it is selected, and kept by
   * what it was asked of: a copy, which the page answers at once, finds it at hand. */
  let held: { key: string; clip: Promise<string>; text?: string } | undefined;
  let press: Press | undefined;
  let starting: Starting | undefined;
  let drag: Drag | undefined;
  /** A turn by the rotate handle, under way (PLAN 2.51). */
  let turning: Turn | undefined;
  /** What a drag on the canvas draws, armed by its key (PLAN 2.48): what the deck offers `n`th,
   * named `label`; and the drag drawing it. */
  let armed: { key: string; n: number; label: string; text: boolean; line: boolean } | undefined;
  let sketch: Sketch | undefined;
  /** What the deck offers, as last asked: at hand when a key arms the canvas, so a drag that
   * follows it at once draws. */
  let offered: Insert[] = [];
  /** How many drags began from moves made before the engine said what was pressed. */
  let early = 0;
  /** A drag's request is with the worker: the next waits for it, so a fast drag never queues. */
  let busy = false;
  /** The gesture being made into a patch: the next waits for it, so each is made on the source the
   * one before left, as a key held down repeats. */
  let making: Promise<void> = Promise.resolve();
  const inTurn = (work: () => Promise<unknown>): Promise<void> => {
    const next = making.then(work).then(() => {});
    making = next.catch(() => {});
    return next;
  };

  /** The last press: when, and where (CSS pixels), and how many clicks it counted. A key between
   * two presses makes the next a first click. */
  let pressed: { at: number; client: [number, number]; clicks: number } | undefined;
  /** Whether the menu asked for next is a right click's, at the pointer, rather than the keyboard's,
   * at what is selected (PLAN 2.53). */
  let righted = false;
  const unpress = () => {
    pressed = undefined;
    righted = false;
  };
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
  /** `p`, a point on the canvas, in client pixels. */
  const onScreen = ([px, py]: Point): Point => {
    const r = overlay.getBoundingClientRect();
    return [r.left + ((px - view[0]) / view[2]) * r.width, r.top + ((py - view[1]) / view[3]) * r.height];
  };
  /** Canvas units to a CSS pixel: what handles and lines are sized in. */
  const unit = () => view[2] / Math.max(1, overlay.getBoundingClientRect().width);
  /** How near, canvas units, an edge `d` moves off the grid goes onto another's: a few pixels;
   * none with ⌘ or Ctrl held. */
  const reach = (d: Drag) => (d.loose ? 0 : REACH * unit());
  const inside = ([x, y, w, h]: Rect, [px, py]: [number, number]) => px >= x && px <= x + w && py >= y && py <= y + h;
  /** Whether `at` is over `b` where it is drawn (PLAN 2.51). */
  const over = (b: NodeBox, at: Point) => {
    const p = laidOut(b, at);
    return p !== undefined && inside(b.rect, p);
  };
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
    map: (node) => box(node)?.transform,
    chose: (selected) => editor.chose(selected),
    keeps: (to) => editor.keeps(to),
  });

  /** Lint's findings on the state shown (PLAN 2.49), on the boxes of the state they stand in. */
  const pins = marks(layer, pop, {
    shown: () => editor.shown(),
    boxes: () => (boxed !== undefined && boxed === editor.shown()?.state ? boxes : undefined),
    view: () => view,
    select: (node) => select(node),
    fix: (f) => editor.fix(f),
    go: (f) => editor.go(f),
    edit: (f, prop) => {
      if (f.node !== undefined && (selected !== f.node || also.length)) select(f.node);
      editor.edit(f, prop);
    },
    say: (words) => editor.say(words),
  });

  /** Type in `node` (a text), at the caret nearest `at`, or at its end, or with `all` its words
   * selected; `fork` keeps it to the state shown. */
  async function type(node: string, at: [number, number] | undefined, fork: boolean, all = false) {
    if (selected !== node) select(node);
    if (!(await text.enter(node, at, fork, all))) editor.say(`${node} is no text: only a text takes typing`);
  }

  /** What stands where in the state shown, asked again: the deck, the state, or the format changed. */
  async function refresh() {
    const shown = editor.shown();
    if (!shown) return;
    const [was, format] = [size, editor.format()];
    const ruling = ruled ? stage.grid(format).catch(() => undefined) : undefined;
    ({ boxes, size } = await stage.boxes(shown.state, format));
    if (ruling) grid = await ruling;
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
    if (slotting) void relayout();
    void stage.inserts().then((i) => (offered = i), () => {});
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
    letGo(node);
    if (text.node() !== undefined && text.node() !== node) text.leave();
    selected = node;
    also = [];
    aim = undefined;
    [shaped, pointPicked, imaged, handling, charted] = [undefined, undefined, undefined, undefined, undefined];
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
    letGo(undefined);
    [selected, also] = [nodes[0], nodes.slice(1)];
    aim = undefined;
    [shaped, pointPicked] = [undefined, undefined];
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
    const found = boxes.filter((b) => (b.parent ?? null) === null && within(drawnBox(b))).map((b) => b.node);
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
    // A shape's points and corners (PLAN 2.68).
    stage
      .outline(shown.state, node, editor.format())
      .then((o) => {
        if (selected !== node) return;
        shaped = o && { state: shown.state, outline: o };
        if (pointPicked !== undefined && pointPicked >= (o?.points.length ?? 0)) pointPicked = undefined;
        draw();
      })
      .catch(() => (shaped = undefined));
    // An image's crop and focal point (PLAN 2.74).
    stage
      .framing(shown.state, node, editor.format())
      .then((f) => {
        if (selected !== node) return;
        imaged = f && { state: shown.state, framing: f };
        draw();
      })
      .catch(() => (imaged = undefined));
    // A chart's marks and annotations, for the keys (PLAN 2.75).
    stage
      .marksIn(shown.state, node, editor.format())
      .then((found) => {
        if (selected !== node) return;
        charted = found && { state: shown.state, node, ...found };
        if (handling?.node === node && handling.index >= handleList(node).length) handling.index = 0;
      })
      .catch(() => (charted = undefined));
  }

  /** Draw the theme's grid of the format shown over the canvas, or stop (PLAN 2.57): `on`, or
   * the other way from now. Resolves once it is drawn so. */
  async function rule(on = !ruled) {
    ruled = on;
    if (on) {
      try {
        grid = await stage.grid(editor.format());
      } catch (e) {
        ruled = false;
        editor.ruled(false);
        return editor.say(`no grid: ${said(e)}`);
      }
    }
    if (ruled !== on) return;
    draw();
    editor.ruled(on);
    const where = grid?.baselines.length ? ", and the baseline grid" : "";
    editor.say(on ? `the theme's grid: ${grid?.columns.length ?? 0} columns and ${grid?.rows.length ?? 0} rows${where}` : "the grid is hidden");
  }

  function draw() {
    const u = unit();
    const parts: string[] = [];
    const rect = ([x, y, w, h]: Rect, cls: string, extra = "") =>
      `<rect class="${cls}" x="${x}" y="${y}" width="${Math.max(0, w)}" height="${Math.max(0, h)}"${extra}/>`;
    const line = (x1: number, y1: number, x2: number, y2: number, cls: string) =>
      `<line class="${cls}" x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}"/>`;
    // The theme's grid (PLAN 2.57), under all else: the columns and rows as bands, the gutters
    // between them, the margins around them, and the baseline grid's lines.
    if (ruled && grid && grid.canvas[0] === size[0] && grid.canvas[1] === size[1]) {
      const [left, right] = [grid.columns[0]?.[0] ?? 0, grid.columns.at(-1)?.[1] ?? size[0]];
      const [top, bottom] = [grid.rows[0]?.[0] ?? 0, grid.rows.at(-1)?.[1] ?? size[1]];
      for (const [a, b] of grid.columns) parts.push(rect([a, top, b - a, bottom - top], "grid-track"));
      for (const [a, b] of grid.rows) parts.push(rect([left, a, right - left, b - a], "grid-track"));
      for (const y of grid.baselines) parts.push(line(left, y, right, y, "baseline"));
      parts.push(rect([left, top, right - left, bottom - top], "grid-margin"));
    }
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
      // Where its edges, or its middle, meet another's or the canvas's (PLAN 2.57).
      for (const [x1, y1, x2, y2] of drag.snapped?.guides ?? []) parts.push(line(x1, y1, x2, y2, "guide"));
      if (drag.snapped?.landed) for (const l of drag.snapped.landed) parts.push(rect(l.cell, "landing"));
      else if (land && drag.how === "order") parts.push(line(land[0], land[1], land[0] + land[2], land[1] + land[3], "landing-line"));
      else if (land) parts.push(rect(land, "landing"));
    }
    const typed = text.node() !== undefined;
    const by: [number, number] = drag?.kind === "move" ? [drag.at[0] - drag.from[0], drag.at[1] - drag.from[1]] : [0, 0];
    // A box drawn where its transform draws it (PLAN 2.51), moved `shift` on the canvas.
    const shape = (b: NodeBox, shift: Point, cls: string, turn = 0) => {
      if (!b.transform && turn === 0) return rect(moved(b.rect, shift), cls);
      const [sin, cos] = [Math.sin((turn * Math.PI) / 180), Math.cos((turn * Math.PI) / 180)];
      const [px, py] = turning?.pivot ?? [0, 0];
      const points = outline(b).map(([x, y]) => {
        const [dx, dy] = [x - px, y - py];
        const [tx, ty] = turn === 0 ? [x, y] : [px + dx * cos - dy * sin, py + dx * sin + dy * cos];
        return `${tx + shift[0]},${ty + shift[1]}`;
      });
      return `<polygon class="${cls}" points="${points.join(" ")}"/>`;
    };
    for (const node of also) {
      const other = box(node);
      if (other) parts.push(shape(other, by, "selected"));
    }
    const first = box(selected);
    if (first) {
      const turn = turning?.node === first.node ? (turning.now - turning.start) * turning.way : 0;
      parts.push(shape(first, by, typed ? "selected typed" : "selected", turn));
      // A point of its box, as laid out, where it is drawn.
      const place = (p: Point): Point => {
        const [x, y] = first.transform ? apply(first.transform, p) : p;
        return [x + by[0], y + by[1]];
      };
      const [x, y, w, h] = first.rect;
      const still = !drag && !typed && !turning && !reshaping && !cropping && also.length === 0;
      if (still && aim && snapOf(aim, editor.at(first.node), true, false)) {
        const s = 8 * u;
        const spot: Record<Edge, [number, number]> = {
          nw: [x, y], n: [x + w / 2, y], ne: [x + w, y], e: [x + w, y + h / 2],
          se: [x + w, y + h], s: [x + w / 2, y + h], sw: [x, y + h], w: [x, y + h / 2],
        };
        for (const edge of EDGES) {
          const [cx, cy] = place(spot[edge]);
          parts.push(rect([cx - s / 2, cy - s / 2, s, s], "handle", ` data-edge="${edge}" style="cursor:${CURSORS[edge]}-resize"`));
        }
      }
      // The rotate handle, above the box's top edge as it is drawn: a drag turns it.
      if (still && !marquee && !armed) {
        const [tx, ty] = place([x + w / 2, y]);
        const [cx, cy] = place([x + w / 2, y + h / 2]);
        const length = Math.hypot(tx - cx, ty - cy);
        const [ux, uy] = length > 1e-6 ? [(tx - cx) / length, (ty - cy) / length] : [0, -1];
        const [hx, hy] = [tx + ux * 24 * u, ty + uy * 24 * u];
        parts.push(line(tx, ty, hx, hy, "turn-arm"));
        parts.push(`<circle class="handle turn" data-turn="1" cx="${hx}" cy="${hy}" r="${5 * u}"><title>Turn ${first.node}; Shift by 15°</title></circle>`);
      }
      // A shape's points and corners (PLAN 2.68), over its box's handles.
      const o = shaped?.state === boxed && shaped?.outline.node === first.node ? shaped?.outline : undefined;
      if (o && still && !marquee && !armed) parts.push(...shapeHandles(o, u));
      // An image's crop and focal point (PLAN 2.74), inside its box.
      const f = imaged?.state === boxed && imaged?.framing.node === first.node ? imaged?.framing : undefined;
      if (f && still && !marquee && !armed) parts.push(cropHandles(f, u));
    }
    if (reshaping && reshaping.state === boxed) parts.push(reshaped(reshaping));
    if (cropping && cropping.state === boxed) parts.push(cropped(cropping, u));
    const over = box(hovered);
    if (over && !chosen().includes(over.node) && !drag && !typed && !marquee && !armed && !turning && !reshaping && !cropping) parts.push(shape(over, [0, 0], "hover"));
    if (sketch) {
      const [[fx, fy], [ax, ay]] = [sketch.from, sketch.at];
      if (sketch.cell) parts.push(rect(sketch.cell, "landing"));
      if (armed?.line) parts.push(line(fx, fy, ax, ay, "sketch-line"));
      else parts.push(rect([Math.min(fx, ax), Math.min(fy, ay), Math.abs(ax - fx), Math.abs(ay - fy)], "marquee"));
    }
    if (marquee) {
      const [x, y] = [Math.min(marquee.from[0], marquee.at[0]), Math.min(marquee.from[1], marquee.at[1])];
      parts.push(rect([x, y, Math.abs(marquee.at[0] - marquee.from[0]), Math.abs(marquee.at[1] - marquee.from[1])], "marquee"));
    }
    if (marked && box(marked.node)) for (const r of marked.rects) parts.push(rect(r, "found"));
    if (rowMarks && rowMarks.state === boxed && !drag && !marquee) {
      for (const m of rowMarks.marks) {
        const map = m.transform ? ` transform="matrix(${m.transform.join(" ")})"` : "";
        parts.push(`<path class="row-mark" data-node="${m.node}" data-key="${m.key.replace(/[\u001f"&<>]/g, " ")}" d="${m.outline}"${map}/>`);
      }
    }
    // A mark picked, a band begun, and an annotation selected or carried (PLAN 2.67).
    const matrix = (t?: Map6, by: Point = [0, 0]) =>
      t || by[0] || by[1] ? ` transform="translate(${by[0]} ${by[1]})${t ? ` matrix(${t.join(" ")})` : ""}"` : "";
    if (picked && picked.state === boxed && !drag) parts.push(`<path class="picked-mark" d="${picked.mark.outline}"${matrix(picked.mark.transform)}/>`);
    if (banding && banding.state === boxed) parts.push(`<path class="band-from" d="${banding.from.outline}"${matrix(banding.from.transform)}/>`);
    if (carrying && carrying.state === boxed) {
      const by: Point = [carrying.at[0] - carrying.from[0], carrying.at[1] - carrying.from[1]];
      parts.push(`<path class="note-carried" d="${carrying.note.outline}"${matrix(carrying.note.transform, by)}/>`);
    } else if (noted && noted.state === boxed && !drag) {
      parts.push(`<path class="noted" data-index="${noted.note.index}" d="${noted.note.outline}"${matrix(noted.note.transform)}/>`);
    }
    // A layout's slots (PLAN 2.71): each named, with handles to resize it, and where a drag lands.
    if (slotting && slotting.state === boxed) {
      const g = slotting.grid;
      if (g) {
        const [left, right] = [g.columns[0]?.[0] ?? 0, g.columns.at(-1)?.[1] ?? size[0]];
        const [top, bottom] = [g.rows[0]?.[0] ?? 0, g.rows.at(-1)?.[1] ?? size[1]];
        for (const [a, b] of g.columns) parts.push(rect([a, top, b - a, bottom - top], "grid-track"));
        for (const [a, b] of g.rows) parts.push(rect([left, a, right - left, b - a], "grid-track"));
      }
      for (const slot of slotting.layout.slots) {
        const r = slotDrag?.slot.name === slot.name ? slotDrag.rect : slot.rect;
        const keyedHere = slotting.layout.slots[slotKeyed ?? -1]?.name === slot.name ? " keyed" : "";
        parts.push(rect(r, (slot.own ? "layout-slot own" : "layout-slot") + keyedHere, ` data-slot="${slot.name}"`));
        parts.push(`<text class="slot-name" x="${r[0] + 8 * u}" y="${r[1] + 18 * u}" font-size="${13 * u}">${slot.name}</text>`);
        if (slotDrag) continue;
        const [x, y, w, h] = r;
        const s = 7 * u;
        const spot: Record<Edge, [number, number]> = {
          nw: [x, y], n: [x + w / 2, y], ne: [x + w, y], e: [x + w, y + h / 2],
          se: [x + w, y + h], s: [x + w / 2, y + h], sw: [x, y + h], w: [x, y + h / 2],
        };
        for (const edge of EDGES) {
          const [cx, cy] = spot[edge];
          parts.push(rect([cx - s / 2, cy - s / 2, s, s], "handle slot-handle", ` data-slot="${slot.name}" data-slot-edge="${edge}" style="cursor:${CURSORS[edge]}-resize"`));
        }
      }
      if (slotDrag?.landed) parts.push(rect(slotDrag.landed.rect, "landing"));
    }
    parts.push(...text.parts(u));
    svg.innerHTML = parts.join("");
    // The handle the keys work (PLAN 2.75), marked.
    const keyed = handling && handling.node === selected ? handleList(handling.node)[handling.index] : undefined;
    if (keyed) svg.querySelector(keyed.selector)?.classList.add("keyed");
    pins.aside(Boolean(drag || sketch || marquee));
    pins.draw();
  }

  /** A shape's points and corners (PLAN 2.68). A line's, an arrow's, or a polygon's points have
   * handles of their own: one dragged moves within the shape's box, and one clicked is picked, which
   * Delete takes away; a handle at an edge's middle adds a point there. A rect's corner handle rounds
   * it to the theme's radius steps. Each is one `choose` of `points` or `radius`, written where it
   * lives, or kept to the state shown with Alt or the inspector's "Only in this state". */
  /** Point `f`, fractions of outline `o`'s box, where it is drawn on the canvas. */
  const onOutline = (o: Outline, [fx, fy]: Point): Point => {
    const [x, y, w, h] = o.rect;
    const p: Point = [x + fx * w, y + fy * h];
    return o.transform ? apply(o.transform, p) : p;
  };
  /** Canvas point `at` as `o`'s box lays it out, through its transform. */
  const inOutline = (o: Outline, at: Point): Point | undefined => {
    if (!o.transform) return at;
    const m = invert(o.transform);
    return m && apply(m, at);
  };
  /** A fraction to a hundredth, as a person would write it, kept to the box. */
  const hundredth = (v: number) => Math.round(Math.min(1, Math.max(0, v)) * 100) / 100;
  /** The edges a shape's points make: a polygon's closes. */
  const edgesOf = (o: Outline) => (o.kind === "polygon" ? o.points.length : Math.max(0, o.points.length - 1));
  /** How far in from its top-left corner a rect's corner handle stands, canvas units: at its radius,
   * clear of the corner's resize handle, inside its box. */
  const cornerAt = (o: Outline, u: number) => Math.min(Math.max(o.radius ?? 0, 12 * u), Math.min(o.rect[2], o.rect[3]) / 2);
  /** The radius step nearest `r`: the first of those as near. */
  const nearest = (radii: number[], r: number) => radii.reduce((best, v, i) => (Math.abs(v - r) < Math.abs(radii[best] - r) - 1e-3 ? i : best), 0);

  function shapeHandles(o: Outline, u: number): string[] {
    const parts: string[] = [];
    const n = o.points.length;
    if (o.kind === "line" || o.kind === "arrow" || o.kind === "polygon") {
      for (let i = 0; i < edgesOf(o); i++) {
        const [a, b] = [o.points[i], o.points[(i + 1) % n]];
        const [cx, cy] = onOutline(o, [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2]);
        parts.push(`<circle class="handle add" data-add="${i}" cx="${cx}" cy="${cy}" r="${3.5 * u}"><title>Add a point to ${o.node} here</title></circle>`);
      }
      o.points.forEach((p, i) => {
        const [cx, cy] = onOutline(o, p);
        const cls = i === pointPicked ? "handle point picked" : "handle point";
        parts.push(`<circle class="${cls}" data-point="${i}" cx="${cx}" cy="${cy}" r="${5 * u}"><title>Point ${i + 1} of ${o.node}: drag it, or click it and press Delete</title></circle>`);
      });
    }
    if (o.kind === "rect" && o.radii.length) {
      const [x, y] = o.rect;
      const d = cornerAt(o, u);
      const [cx, cy] = o.transform ? apply(o.transform, [x + d, y + d]) : [x + d, y + d];
      parts.push(`<circle class="handle corner" data-radius="1" cx="${cx}" cy="${cy}" r="${4.5 * u}"><title>Round ${o.node}'s corners to the theme's radius steps</title></circle>`);
    }
    return parts;
  }

  /** The outline `r` leaves, drawn as it is dragged: its points joined, or the rect rounded. */
  function reshaped(r: Reshape): string {
    const o = r.outline;
    const map = o.transform ? ` transform="matrix(${o.transform.join(" ")})"` : "";
    if (r.index === undefined) {
      const [x, y, w, h] = o.rect;
      const radius = o.radii[r.step ?? 0] ?? 0;
      return `<rect class="reshape" x="${x}" y="${y}" width="${w}" height="${h}" rx="${radius}" ry="${radius}"${map}/>`;
    }
    const [x, y, w, h] = o.rect;
    const points = r.points.map(([fx, fy]) => `${x + fx * w},${y + fy * h}`).join(" ");
    return `<${o.kind === "polygon" ? "polygon" : "polyline"} class="reshape" points="${points}"${map}/>`;
  }

  /** A press on handle `handle` of the shape selected, at `from`: a point or a corner dragged, or a
   * point added at an edge's middle. */
  function reshape(handle: Element, from: Point, client: Point, alt: boolean) {
    const s = shaped!;
    const o = s.outline;
    const points = o.points.map(([x, y]) => [x, y] as Point);
    const r: Reshape = { state: s.state, outline: o, from, client, moved: false, alt, version: editor.version(), points };
    const [point, add] = [handle.getAttribute("data-point"), handle.getAttribute("data-add")];
    if (point !== null) r.index = Number(point);
    else if (add !== null) {
      const i = Number(add);
      const [a, b] = [points[i], points[(i + 1) % points.length]];
      points.splice(i + 1, 0, [hundredth((a[0] + b[0]) / 2), hundredth((a[1] + b[1]) / 2)]);
      [r.index, r.added] = [i + 1, true];
    } else r.step = nearest(o.radii, o.radius ?? 0);
    reshaping = r;
    pointPicked = undefined;
    draw();
  }

  /** The pointer at `at` in reshape `r`: the point goes there, kept to the box, or the corner's
   * radius goes as far as the pointer went along its diagonal, to the nearest step. */
  function reshapeTo(r: Reshape, at: Point, client: Point, alt: boolean) {
    r.alt = alt;
    if (!r.moved && Math.hypot(client[0] - r.client[0], client[1] - r.client[1]) < SLOP) return;
    r.moved = true;
    const o = r.outline;
    const kept = keeping(alt) ? ` · kept to ${r.state}` : "";
    const p = inOutline(o, at);
    if (!p) return;
    const [x, y, w, h] = o.rect;
    if (r.index !== undefined) {
      const f: Point = [w > 0 ? hundredth((p[0] - x) / w) : 0, h > 0 ? hundredth((p[1] - y) / h) : 0];
      r.points[r.index] = f;
      editor.say(`${o.node}'s point ${r.index + 1} → ${f.join(", ")}${kept}`);
    } else {
      const from = inOutline(o, r.from) ?? r.from;
      const half = Math.min(w, h) / 2;
      const radius = Math.min(half, Math.max(0, (o.radius ?? 0) + (p[0] - from[0] + p[1] - from[1]) / 2));
      r.step = nearest(o.radii, radius);
      editor.say(`${o.node}'s corners round to radius.${r.step}${kept}`);
    }
    draw();
  }

  /** Reshape `r` let go: one `choose` of the shape's `points` or `radius`; a point pressed and let
   * go where it was is picked. */
  async function reshapeEnd(r: Reshape) {
    const shown = editor.shown();
    const o = r.outline;
    if (!shown || shown.state !== r.state) return draw();
    if (editor.version() !== r.version) {
      draw();
      return editor.say("the source changed under the drag: nothing is changed");
    }
    const fork = keeping(r.alt);
    const kept = fork ? ` · kept to ${r.state}` : "";
    const choose = (prop: string, value: unknown) => ({ op: "choose", node: o.node, prop, value, state: r.state, ...(fork ? { fork } : {}) });
    if (r.index === undefined) {
      const step = r.step ?? 0;
      if (!r.moved || Math.abs((o.radius ?? 0) - (o.radii[step] ?? 0)) < 0.01) {
        draw();
        return editor.say(r.moved ? `${o.node}'s corners stay as they are` : `drag ${o.node}'s corner handle to round it to the theme's radius steps`);
      }
      return change([choose("radius", `radius.${step}`)], "rounding…", `${o.node}'s corners round to radius.${step}${kept}`, o.node);
    }
    if (!r.moved && !r.added) {
      pointPicked = r.index;
      draw();
      return editor.say(`${o.node}'s point ${r.index + 1} picked: Delete takes it away, a drag moves it`);
    }
    if (!r.added && r.points.every(([px, py], i) => px === o.points[i][0] && py === o.points[i][1])) {
      draw();
      return editor.say(`${o.node}'s point ${r.index + 1} stays where it is`);
    }
    const done = r.added ? `${o.node} has a point added${kept}` : `${o.node}'s point ${r.index + 1} moved${kept}`;
    await change([choose("points", r.points)], r.added ? "adding a point…" : "moving the point…", done, o.node);
  }

  /** An image's crop and focal point on the canvas (PLAN 2.74). With an image selected, a handle
   * stands inside each side of the part that shows: one dragged crops the image from that side, the
   * whole image outlined as it goes, and the part the crop keeps over it. The focal point, the point
   * of the crop that lines up with the same point of the box, as CSS `object-position` does, is a
   * handle there: one dragged moves it within the box. Each is one `choose` of `crop` or `focal`, written where it lives, or
   * kept to the state shown with Alt; Escape leaves it as it was. */
  /** `f`'s map, as an SVG attribute: what draws a part laid out where it is drawn. */
  const framedMap = (f: Framing) => (f.transform ? ` transform="matrix(${f.transform.join(" ")})"` : "");
  /** The part of `f`'s whole image `crop` keeps, canvas units as laid out. */
  const cropRect = (f: Framing, crop: Rect): Rect => {
    const [x, y, w, h] = f.whole;
    return [x + crop[0] * w, y + crop[1] * h, crop[2] * w, crop[3] * h];
  };
  /** Where focal point `focal` stands in `f`, canvas units as laid out: that point of the crop lines
   * up with the same point of the box, as CSS `object-position` does, so it stands there. */
  const focalPoint = (f: Framing, focal: Point): Point => {
    const [x, y, w, h] = f.rect;
    return [x + focal[0] * w, y + focal[1] * h];
  };
  /** A fraction to a thousandth. */
  const thousandth = (v: number) => Math.round(Math.min(1, Math.max(0, v)) * 1000) / 1000;
  /** The least a crop keeps of the image, each way. */
  const LEAST_CROP = 0.05;

  function cropHandles(f: Framing, u: number): string {
    const [x, y, w, h] = f.shown;
    const inset = Math.min(10 * u, w / 4, h / 4);
    const [long, thick] = [16 * u, 5 * u];
    const sides: Record<"n" | "e" | "s" | "w", Rect> = {
      n: [x + w / 2 - long / 2, y + inset - thick / 2, long, thick],
      s: [x + w / 2 - long / 2, y + h - inset - thick / 2, long, thick],
      w: [x + inset - thick / 2, y + h / 2 - long / 2, thick, long],
      e: [x + w - inset - thick / 2, y + h / 2 - long / 2, thick, long],
    };
    const names = { n: "top", e: "right", s: "bottom", w: "left" };
    const bars = (Object.keys(sides) as (keyof typeof sides)[]).map((side) => {
      const [bx, by, bw, bh] = sides[side];
      const cursor = side === "n" || side === "s" ? "ns-resize" : "ew-resize";
      return `<rect class="handle crop-handle" data-crop="${side}" x="${bx}" y="${by}" width="${bw}" height="${bh}" style="cursor:${cursor}"><title>Crop ${f.node} from the ${names[side]}</title></rect>`;
    });
    const [fx, fy] = focalPoint(f, f.focal);
    const focal = `<g class="focal-handle" data-focal="1" style="cursor:move"><circle class="handle focal" data-focal="1" cx="${fx}" cy="${fy}" r="${6 * u}"/><path class="focal-cross" d="M${fx - 10 * u} ${fy}H${fx + 10 * u}M${fx} ${fy - 10 * u}V${fy + 10 * u}"/><title>${f.node}'s focal point: drag it to keep that part in view</title></g>`;
    return `<g class="crop"${framedMap(f)}>${bars.join("")}${focal}</g>`;
  }

  /** What cropping `c` shows as it is dragged: the whole image outlined, what the crop cuts away
   * shaded, the part it keeps, and the focal point. */
  function cropped(c: Cropping, u: number): string {
    const f = c.framing;
    const [wx, wy, ww, wh] = f.whole;
    const [cx, cy, cw, ch] = cropRect(f, c.crop);
    const [fx, fy] = focalPoint(f, c.focal);
    const shade = `<path class="crop-shade" fill-rule="evenodd" d="M${wx} ${wy}h${ww}v${wh}h${-ww}ZM${cx} ${cy}h${cw}v${ch}h${-cw}Z"/>`;
    return `<g class="cropping"${framedMap(f)}>${shade}<rect class="image-whole" x="${wx}" y="${wy}" width="${ww}" height="${wh}"/><rect class="crop-frame" x="${cx}" y="${cy}" width="${cw}" height="${ch}"/><circle class="focal" cx="${fx}" cy="${fy}" r="${6 * u}"/></g>`;
  }

  /** A press on crop handle `handle` of the image selected, at `from`, canvas units. */
  function cropStart(handle: Element, from: Point, client: Point, alt: boolean) {
    const f = imaged!.framing;
    const at = f.transform ? (invert(f.transform) ? apply(invert(f.transform)!, from) : from) : from;
    const side = handle.getAttribute("data-crop") as Cropping["side"] | null;
    cropping = {
      state: imaged!.state,
      framing: f,
      side: side ?? undefined,
      from: at,
      client,
      moved: false,
      alt,
      version: editor.version(),
      crop: [...f.crop] as Rect,
      focal: [...f.focal] as Point,
    };
    draw();
  }

  /** The pointer at `at` in cropping `c`: the side goes as far as the pointer went, the crop kept to
   * the image and to at least a twentieth of it; or the focal point goes there, kept to the crop. */
  function cropTo(c: Cropping, at: Point, client: Point, alt: boolean) {
    c.alt = alt;
    if (!c.moved && Math.hypot(client[0] - c.client[0], client[1] - c.client[1]) < SLOP) return;
    c.moved = true;
    const f = c.framing;
    const m = f.transform && invert(f.transform);
    const p = f.transform ? (m ? apply(m, at) : undefined) : at;
    if (!p) return;
    const kept = keeping(alt) ? ` · kept to ${c.state}` : "";
    const [x, y, w, h] = f.crop;
    if (c.side) {
      const [dx, dy] = [(p[0] - c.from[0]) / f.whole[2], (p[1] - c.from[1]) / f.whole[3]];
      let crop: Rect = [x, y, w, h];
      if (c.side === "w") {
        const nx = Math.min(x + w - LEAST_CROP, Math.max(0, x + dx));
        crop = [nx, y, x + w - nx, h];
      } else if (c.side === "e") crop = [x, y, Math.min(1 - x, Math.max(LEAST_CROP, w + dx)), h];
      else if (c.side === "n") {
        const ny = Math.min(y + h - LEAST_CROP, Math.max(0, y + dy));
        crop = [x, ny, w, y + h - ny];
      } else crop = [x, y, w, Math.min(1 - y, Math.max(LEAST_CROP, h + dy))];
      c.crop = crop.map(thousandth) as Rect;
      editor.say(`${f.node} cropped to ${c.crop.join(", ")} of the image${kept}`);
    } else {
      const [rx, ry, rw, rh] = f.rect;
      c.focal = [thousandth((p[0] - rx) / rw), thousandth((p[1] - ry) / rh)];
      editor.say(`${f.node}'s focal point → ${c.focal.join(", ")}${kept}`);
    }
    draw();
  }

  /** Cropping `c` let go: one `choose` of the image's `crop` or `focal`. */
  async function cropEnd(c: Cropping) {
    const shown = editor.shown();
    const f = c.framing;
    if (!shown || shown.state !== c.state) {
      draw();
      return editor.say(`another state is shown: ${f.node} is as it was`);
    }
    if (editor.version() !== c.version) {
      draw();
      return editor.say("the source changed under the drag: nothing is changed");
    }
    const same = (a: number[], b: number[]) => a.every((v, i) => Math.abs(v - b[i]) < 5e-4);
    const fork = keeping(c.alt);
    const kept = fork ? ` · kept to ${c.state}` : "";
    const choose = (prop: string, value: unknown) => ({ op: "choose", node: f.node, prop, value, state: c.state, ...(fork ? { fork } : {}) });
    if (c.side) {
      if (!c.moved || same(c.crop, f.crop)) {
        draw();
        return editor.say(c.moved ? `${f.node}'s crop stays as it is` : `drag ${f.node}'s crop handle to crop it from that side`);
      }
      const whole = same(c.crop, [0, 0, 1, 1]);
      return change([choose("crop", whole ? null : c.crop)], "cropping…", whole ? `${f.node} shows the whole image${kept}` : `${f.node} cropped to ${c.crop.join(", ")}${kept}`, f.node);
    }
    if (!c.moved || same(c.focal, f.focal)) {
      draw();
      return editor.say(c.moved ? `${f.node}'s focal point stays where it is` : `drag ${f.node}'s focal point to keep that part of the image in view`);
    }
    return change([choose("focal", c.focal)], "moving the focal point…", `${f.node}'s focal point → ${c.focal.join(", ")}${kept}`, f.node);
  }

  /** The canvas by keys alone (PLAN 2.75, WCAG 2.1.1). Tab and Shift+Tab select the next node,
   * or the one before, in reading order among those beside the one selected; past either end the
   * focus leaves the canvas, so the keys are never held there. Enter goes into a container or a
   * group, and Escape back out. Enter on a shape or an image goes to its handles, which Tab steps
   * through and the arrows move: each move is the patch its drag makes. `[` and `]` turn what is
   * selected, as its round handle does. */
  /** The nodes in container `parent` (`null`, the canvas), in reading order: as they are painted,
   * a container where the first it holds is (SPEC §3.12). */
  function readingOrder(parent: string | null): string[] {
    const painted = new Map<string, number>();
    boxes.forEach((b, i) => b.draws && painted.set(b.node, i));
    const first = (node: string): number => {
      const own = painted.get(node);
      if (own !== undefined) return own;
      const kids = boxes.filter((b) => b.parent === node).map((b) => first(b.node));
      return kids.length ? Math.min(...kids) : Number.MAX_SAFE_INTEGER;
    };
    const here = boxes.filter((b) => (b.parent ?? null) === parent).map((b) => b.node);
    return [...new Set(here)].sort((a, b) => first(a) - first(b) || a.localeCompare(b));
  }

  /** Tab, or with `back` Shift+Tab, in reading order. Whether the canvas took it: past the end it
   * does not, and the focus goes on. */
  function tabTo(back: boolean): boolean {
    if (!editor.shown()) return false;
    const level = selected === undefined ? null : (box(selected)?.parent ?? null);
    const order = readingOrder(level);
    const at = selected === undefined ? (back ? order.length : -1) : order.indexOf(selected);
    const next = at + (back ? -1 : 1);
    if (next < 0 || next >= order.length) {
      if (selected !== undefined) select(undefined);
      editor.say("nothing selected: the focus leaves the canvas");
      return false;
    }
    select(order[next]);
    editor.say(`${order[next]} selected, ${next + 1} of ${order.length}${level ? ` in ${level}` : ""} · Enter goes into it or its handles`);
    return true;
  }

  /** A handle the keys work: what it is, and where its mark is drawn. */
  interface Handle {
    kind: "point" | "corner" | "crop" | "focal" | "mark" | "note";
    index?: number;
    mark?: DataMark;
    note?: NoteMark;
    side?: "n" | "e" | "s" | "w";
    label: string;
    selector: string;
  }

  /** Node `node`'s handles, in order: a shape's points, a rect's corner; an image's crop's sides,
   * then its focal point. */
  function handleList(node: string): Handle[] {
    const out: Handle[] = [];
    const o = shaped?.outline.node === node ? shaped.outline : undefined;
    if (o && (o.kind === "line" || o.kind === "arrow" || o.kind === "polygon")) {
      o.points.forEach((_, i) => out.push({ kind: "point", index: i, label: `point ${i + 1}`, selector: `[data-point="${i}"]` }));
    }
    if (o && o.kind === "rect" && o.radii.length) out.push({ kind: "corner", label: "its corner", selector: "[data-radius]" });
    const f = imaged?.framing.node === node ? imaged.framing : undefined;
    if (f) {
      const names = { n: "the crop's top", e: "the crop's right", s: "the crop's bottom", w: "the crop's left" } as const;
      for (const side of ["n", "e", "s", "w"] as const) out.push({ kind: "crop", side, label: names[side], selector: `[data-crop="${side}"]` });
      out.push({ kind: "focal", label: "its focal point", selector: ".handle.focal" });
    }
    // A chart's marks, picked as the keys reach each, then its annotations, selected so.
    const c = charted?.node === node ? charted : undefined;
    if (c) {
      for (const mark of c.marks) out.push({ kind: "mark", mark, label: markName(mark), selector: ".picked-mark" });
      for (const note of c.notes) out.push({ kind: "note", note, label: noteName(note), selector: ".noted" });
    }
    return out;
  }

  /** What the status says of handle `h` of `node`, and what its keys do. */
  const handleSaid = (node: string, h: Handle, n: number, of: number) => {
    const does =
      h.kind === "mark"
        ? "picked: Shift+F10 opens its menu, to highlight it, call it out, rule its value, or band from it"
        : h.kind === "note"
          ? "selected: Enter changes what it says, Delete takes it away"
          : `arrows move it${h.kind === "point" ? ", + adds a point after it, Delete takes it away" : ""}`;
    return `${node}: ${h.label}, ${n + 1} of ${of} · ${does} · Tab the next · Escape leaves them`;
  };

  /** The keys on handle `h`: a mark is picked and an annotation selected, as a click on each does. */
  function reachHandle(h: Handle) {
    const shown = editor.shown();
    [picked, noted] = [undefined, undefined];
    if (shown && h.mark) picked = { state: shown.state, mark: h.mark };
    if (shown && h.note) noted = { state: shown.state, note: h.note };
  }

  /** Handle `h` of `node` moved by the arrow `[dx, dy]`, a hundredth (with `far`, a tenth), or a
   * corner a radius step: the patch its drag makes, kept to the state shown with `fork`. */
  async function nudgeHandle(node: string, h: Handle, [dx, dy]: Point, far: boolean, alt: boolean) {
    const shown = editor.shown();
    if (!shown) return;
    const by = far ? 0.1 : 0.01;
    const client: Point = [0, 0];
    const version = editor.version();
    if (h.kind === "point" || h.kind === "corner") {
      const o = shaped?.outline;
      if (!o || o.node !== node) return;
      const points = o.points.map(([x, y]) => [x, y] as Point);
      const r: Reshape = { state: shown.state, outline: o, from: [0, 0], client, moved: true, alt, version, points };
      if (h.kind === "point") {
        const i = h.index!;
        points[i] = [hundredth(points[i][0] + dx * by), hundredth(points[i][1] + dy * by)];
        r.index = i;
      } else {
        const now = nearest(o.radii, o.radius ?? 0);
        r.step = Math.min(o.radii.length - 1, Math.max(0, now + (dx > 0 || dy < 0 ? 1 : -1)));
      }
      return reshapeEnd(r);
    }
    const f = imaged?.framing;
    if (!f || f.node !== node) return;
    const c: Cropping = { state: shown.state, framing: f, from: [0, 0], client, moved: true, alt, version, crop: [...f.crop] as Rect, focal: [...f.focal] as Point };
    if (h.kind === "focal") c.focal = [thousandth(f.focal[0] + dx * by), thousandth(f.focal[1] + dy * by)];
    else {
      c.side = h.side;
      const [x, y, w, ht] = f.crop;
      if (h.side === "w" && dx) {
        const nx = Math.min(x + w - LEAST_CROP, Math.max(0, x + dx * by));
        c.crop = [nx, y, x + w - nx, ht];
      } else if (h.side === "e" && dx) c.crop = [x, y, Math.min(1 - x, Math.max(LEAST_CROP, w + dx * by)), ht];
      else if (h.side === "n" && dy) {
        const ny = Math.min(y + ht - LEAST_CROP, Math.max(0, y + dy * by));
        c.crop = [x, ny, w, y + ht - ny];
      } else if (h.side === "s" && dy) c.crop = [x, y, w, Math.min(1 - y, Math.max(LEAST_CROP, ht + dy * by))];
      else return editor.say(`${h.label} moves ${h.side === "n" || h.side === "s" ? "up and down" : "left and right"}`);
      c.crop = c.crop.map(thousandth) as Rect;
    }
    return cropEnd(c);
  }

  /** A point added after point `index` of shape `node`, at the middle of the edge to the next. */
  async function addAfter(node: string, index: number, alt: boolean) {
    const shown = editor.shown();
    const o = shaped?.outline;
    if (!shown || !o || o.node !== node) return;
    const points = o.points.map(([x, y]) => [x, y] as Point);
    const [a, b] = [points[index], points[(index + 1) % points.length]];
    points.splice(index + 1, 0, [hundredth((a[0] + b[0]) / 2), hundredth((a[1] + b[1]) / 2)]);
    const r: Reshape = { state: shown.state, outline: o, from: [0, 0], client: [0, 0], moved: false, alt, version: editor.version(), points, index: index + 1, added: true };
    await reshapeEnd(r);
  }

  /** What is selected turned `by` degrees, as its round handle turns it: one `choose`. */
  async function turnBy(node: string, by: number) {
    const t = editor.transform(node);
    const start = typeof t?.rotate === "number" ? t.rotate : 0;
    const now = Math.round((start + by) * 1000) / 1000;
    await turnTo({ node, pivot: [0, 0], last: 0, round: 0, start, now, way: 1, version: editor.version() });
  }

  /** Bullets or numbers on a text (ADR-0018, PLAN 2.69), as ⌘⇧8 and ⌘⇧7 do: typed in, on the
   * paragraphs its selection touches; selected, on all of its paragraphs. Paragraphs all of that
   * kind already leave the list. One `list` patch, one step to undo. */
  function listing(kind: "bullet" | "number") {
    if (text.node() !== undefined) return void text.toggle(kind);
    return inTurn(async () => {
      const shown = editor.shown();
      const node = selected;
      if (!shown || node === undefined || also.length) return editor.say("select a text to make it a list");
      const carets = await stage.carets(editor.source(), shown.state, node, editor.format()).catch(() => null);
      if (!carets) return editor.say(`${node} is no text: only a text is a list`);
      const count = paragraphs(carets.text).length;
      const all = Array.from({ length: count }, (_, k) => carets.items?.[k]?.kind).every((k) => k === kind);
      const fork = keeping();
      const op = { op: "list", node, state: shown.state, from: 0, to: [...carets.text].length, kind: all ? "none" : kind, ...(fork ? { fork } : {}) };
      const what = all ? "out of the list" : kind === "bullet" ? "bulleted" : "numbered";
      await change([op], "listing…", `${node}: ${what}${fork ? ` · kept to ${shown.state}` : ""}`, node);
    });
  }

  /** Take the point picked away, unless the shape keeps no fewer: a line or an arrow two, a polygon three. */
  function unpoint() {
    return inTurn(async () => {
      const [shown, s, i] = [editor.shown(), shaped, pointPicked];
      if (!shown || !s || i === undefined) return;
      const o = s.outline;
      if (o.points.length <= o.fewest) {
        const kind = o.kind === "arrow" ? "an arrow" : `a ${o.kind}`;
        return editor.say(`${kind} keeps ${o.fewest === 2 ? "two" : "three"} points: ${o.node}'s point ${i + 1} stays`);
      }
      pointPicked = undefined;
      const fork = keeping();
      const op = { op: "choose", node: o.node, prop: "points", value: o.points.filter((_, k) => k !== i), state: shown.state, ...(fork ? { fork } : {}) };
      await change([op], "taking the point away…", `${o.node}'s point ${i + 1} taken away${fork ? ` · kept to ${shown.state}` : ""}`, o.node);
    });
  }

  /** A theme's layout on the canvas (PLAN 2.71, ADR-0016). The layout the state shown uses is
   * drawn as its slots, each named, on the theme's grid in the format shown. A slot dragged, or
   * resized by a handle, lands on the grid's tracks, and is one `theme_edit`: its `col` and `row`
   * where the layout writes them on the deck's own canvas, or in the format shown's own slots
   * (`layouts.L.formats.F.slots`), which take the layout's others with them. Every state that
   * uses the layout shows it moved. Escape leaves it. */
  async function layoutMode(on = slotting === undefined) {
    if (!on) {
      slotting = slotDrag = slotKeyed = undefined;
      editor.layouting?.(false);
      draw();
      return editor.say("the layout is left as it is");
    }
    const shown = editor.shown();
    if (!shown) return editor.say("the canvas waits for a source that compiles");
    const format = editor.format();
    const [layout, g] = await Promise.all([
      stage.layout(shown.state, format).catch(() => undefined),
      stage.grid(format).catch(() => undefined),
    ]);
    if (!layout) return editor.say(`${shown.state} uses no layout of the theme's: choose one in the inspector`);
    if (text.node() !== undefined) text.leave();
    select(undefined);
    slotting = { state: shown.state, format, layout, grid: g };
    editor.layouting?.(true);
    draw();
    editor.say(`the layout ${layout.layout}${format ? `, in ${format}` : ""}: drag a slot onto the grid, or a handle to resize it · Escape leaves it`);
  }
  /** The layout asked again: the deck, the theme, the state, or the format changed. */
  async function relayout() {
    if (!slotting) return;
    const shown = editor.shown();
    if (!shown) return;
    const format = editor.format();
    const [layout, g] = await Promise.all([stage.layout(shown.state, format).catch(() => undefined), stage.grid(format).catch(() => undefined)]);
    if (!slotting) return;
    if (!layout) return void layoutMode(false);
    slotting = { state: shown.state, format, layout, grid: g };
    draw();
  }

  /** Where box `r` lands on the grid: the tracks nearest its edges, `span` cells (columns, rows)
   * kept where a slot is moved, not resized. */
  function slotLanding(r: Rect, span?: [number, number]): Landing | undefined {
    const g = slotting?.grid;
    if (!g?.columns.length || !g.rows.length) return undefined;
    const nearest = (tracks: [number, number][], v: number, end: 0 | 1) =>
      tracks.reduce((best, t, i) => (Math.abs(t[end] - v) < Math.abs(tracks[best][end] - v) ? i : best), 0);
    const axis = (tracks: [number, number][], from: number, to: number, n?: number): [number, number] => {
      if (n === undefined) {
        const a = nearest(tracks, from, 0);
        return [a, Math.max(a, nearest(tracks, to, 1))];
      }
      const a = Math.min(nearest(tracks, from, 0), tracks.length - n);
      return [Math.max(0, a), Math.max(0, a) + n - 1];
    };
    const [c0, c1] = axis(g.columns, r[0], r[0] + r[2], span?.[0]);
    const [r0, r1] = axis(g.rows, r[1], r[1] + r[3], span?.[1]);
    const box: Rect = [g.columns[c0][0], g.rows[r0][0], g.columns[c1][1] - g.columns[c0][0], g.rows[r1][1] - g.rows[r0][0]];
    return { col: [c0 + 1, c1 + 1], row: [r0 + 1, r1 + 1], rect: box };
  }
  /** A slot's cells as written, `[from, to]`: one number is one track. */
  const cellsOf = (v: number | [number, number] | undefined, all: number): [number, number] =>
    v === undefined ? [1, all] : typeof v === "number" ? [v, v] : v;

  /** A press on a slot, or on one of its handles. */
  function slotPress(e: PointerEvent) {
    if (!slotting) return;
    const at = point(e);
    const target = (e.target as Element).closest?.("[data-slot-edge]");
    let slot: SlotBox | undefined;
    let edge: Edge | undefined;
    if (target) {
      slot = slotting.layout.slots.find((s) => s.name === target.getAttribute("data-slot"));
      edge = target.getAttribute("data-slot-edge") as Edge;
    } else {
      // The smallest slot under the pointer: one inside another is the one meant.
      const under = slotting.layout.slots.filter((s) => inside(s.rect, at));
      slot = under.sort((a, b) => a.rect[2] * a.rect[3] - b.rect[2] * b.rect[3])[0];
    }
    if (!slot) return editor.say("press on a slot to move it, or on its handle to resize it");
    slotDrag = { slot, edge, from: at, rect: slot.rect, version: editor.version() };
    editor.say(`slot ${slot.name} of ${slotting.layout.layout}: drag it onto the grid`);
    draw();
  }
  /** The pointer at `at` in a slot's drag: its box follows, and lands on the grid. */
  function slotMove(at: Point) {
    const d = slotDrag;
    const g = slotting?.grid;
    if (!d || !slotting || !g) return;
    const by: [number, number] = [at[0] - d.from[0], at[1] - d.from[1]];
    d.rect = d.edge ? resized(d.slot.rect, d.edge, by) : moved(d.slot.rect, by);
    const [c, r] = [cellsOf(d.slot.col, g.columns.length), cellsOf(d.slot.row, g.rows.length)];
    d.landed = slotLanding(d.rect, d.edge ? undefined : [c[1] - c[0] + 1, r[1] - r[0] + 1]);
    if (d.landed) editor.say(`slot ${d.slot.name} → ${placed({ col: d.landed.col, row: d.landed.row })}`);
    draw();
  }
  /** A slot let go: one `theme_edit` that puts it where it landed. */
  async function slotDrop() {
    const d = slotDrag;
    slotDrag = undefined;
    const now = slotting;
    if (!d || !now || !d.landed || !now.grid) return draw();
    if (editor.version() !== d.version) {
      draw();
      return editor.say("the source changed under the drag: the layout is as it was");
    }
    const was = [cellsOf(d.slot.col, now.grid.columns.length), cellsOf(d.slot.row, now.grid.rows.length)];
    const { col, row } = d.landed;
    if (was[0][0] === col[0] && was[0][1] === col[1] && was[1][0] === row[0] && was[1][1] === row[1]) {
      draw();
      return editor.say(`slot ${d.slot.name} stays where it is`);
    }
    let ops: unknown[];
    try {
      ops = await slotOps(now, d.slot, col, row);
    } catch (e) {
      draw();
      return editor.say(`the layout is as it was: ${said(e)}`);
    }
    const where = placed({ col, row });
    const what = `slot ${d.slot.name} of ${now.layout.layout}${now.format ? ` in ${now.format}` : ""} → ${where}`;
    const done = await editor.themeEdit?.(ops, what);
    await relayout();
    if (done) editor.say(`${what} · every state that uses it shows it moved · ⌘Z undoes it`);
  }
  /** Slot `slot` moved a track by arrow `[dx, dy]`, or, with `grow`, its far side moved so: one
   * theme edit, as a drag's drop makes (PLAN 2.75). */
  async function slotNudge(slot: SlotBox, [dx, dy]: Point, grow: boolean) {
    const now = slotting;
    const g = now?.grid;
    if (!now || !g) return;
    const [cols, rows] = [g.columns.length, g.rows.length];
    let [c0, c1] = cellsOf(slot.col, cols);
    let [r0, r1] = cellsOf(slot.row, rows);
    if (grow) {
      c1 = Math.min(cols, Math.max(c0, c1 + dx));
      r1 = Math.min(rows, Math.max(r0, r1 + dy));
    } else {
      const sx = Math.min(cols - c1, Math.max(1 - c0, dx));
      const sy = Math.min(rows - r1, Math.max(1 - r0, dy));
      [c0, c1, r0, r1] = [c0 + sx, c1 + sx, r0 + sy, r1 + sy];
    }
    const was = [cellsOf(slot.col, cols), cellsOf(slot.row, rows)];
    if (was[0][0] === c0 && was[0][1] === c1 && was[1][0] === r0 && was[1][1] === r1) return editor.say(`slot ${slot.name} goes no farther that way`);
    let ops: unknown[];
    try {
      ops = await slotOps(now, slot, [c0, c1], [r0, r1]);
    } catch (e) {
      return editor.say(`the layout is as it was: ${said(e)}`);
    }
    const what = `slot ${slot.name} of ${now.layout.layout}${now.format ? ` in ${now.format}` : ""} → ${placed({ col: [c0, c1], row: [r0, r1] })}`;
    const done = await editor.themeEdit?.(ops, what);
    await relayout();
    if (done) editor.say(`${what} · every state that uses it shows it moved · ⌘Z undoes it`);
  }

  /** The operations that put `slot` on cells `col` and `row`: where the layout writes it on the
   * deck's own canvas; in a format, in that format's own slots, the layout's others carried along. */
  async function slotOps(now: NonNullable<typeof slotting>, slot: SlotBox, col: [number, number], row: [number, number]) {
    const text = await stage.themeText();
    if (!text) throw new Error("the deck names no theme to edit");
    const theme = JSON.parse(text.text);
    const name = now.layout.layout;
    const layout = theme.layouts?.[name];
    if (!layout) throw new Error(`the theme has no layout ${name}`);
    const cells = (v: [number, number]) => (v[0] === v[1] ? v[0] : v);
    if (!now.format) {
      return [
        { op: "add", path: pointer("layouts", name, "slots", slot.name, "col"), value: cells(col) },
        { op: "add", path: pointer("layouts", name, "slots", slot.name, "row"), value: cells(row) },
      ];
    }
    const base = layout.slots?.[slot.name] ?? {};
    const own = layout.formats?.[now.format]?.slots?.[slot.name];
    const value = { ...base, ...(own ?? {}), col: cells(col), row: cells(row) };
    if (!layout.formats) return [{ op: "add", path: pointer("layouts", name, "formats"), value: { [now.format]: { slots: { [slot.name]: value } } } }];
    if (!layout.formats[now.format]) return [{ op: "add", path: pointer("layouts", name, "formats", now.format), value: { slots: { [slot.name]: value } } }];
    if (!layout.formats[now.format].slots) return [{ op: "add", path: pointer("layouts", name, "formats", now.format, "slots"), value: { [slot.name]: value } }];
    return [{ op: "add", path: pointer("layouts", name, "formats", now.format, "slots", slot.name), value }];
  }
  /** A layout added from the one shown, by a name asked for (PLAN 2.71): one `theme_edit` that
   * copies it, then the state shown takes it, kept to that state, so it is the one edited. */
  async function newLayout() {
    if (!slotting) await layoutMode(true);
    const now = slotting;
    const shown = editor.shown();
    if (!now || !shown) return;
    const r = overlay.getBoundingClientRect();
    const from = now.layout.layout;
    const name = (await askWords([r.left + r.width / 2, r.top + 48], `${from}-2`, `The new layout's name: a copy of ${from}`))?.trim();
    if (!name) return editor.say("no layout added");
    if (!/^[a-z][a-z0-9_-]{0,63}$/.test(name)) return editor.say(`${name} is no layout's name: lower-case letters, digits, - and _`);
    const text = await stage.themeText();
    const theme = text && JSON.parse(text.text);
    if (!theme?.layouts?.[from]) return editor.say(`the theme has no layout ${from}`);
    if (theme.layouts[name]) return editor.say(`the theme has a layout ${name} already`);
    const done = await editor.themeEdit?.([{ op: "add", path: pointer("layouts", name), value: theme.layouts[from] }], `layout ${name} added, a copy of ${from}`);
    if (!done) return;
    await change([{ op: "set_state", id: shown.state, prop: "layout", value: name, fork: true }], "choosing it…", `layout ${name} added, a copy of ${from}; ${shown.state} uses it`, null);
    await relayout();
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

  /** The box `d` leaves, and how it snaps, with the pointer where it is now: `by` on the canvas,
   * and `held` as what holds it lays it out, through whatever turns or scales that (PLAN 2.51). A
   * resize goes by the node's own axes. */
  function aimed(d: Drag): { how?: SnapMode; to: Rect; by: [number, number]; held: [number, number] } {
    const by: [number, number] = [d.at[0] - d.from[0], d.at[1] - d.from[1]];
    const parent = holder(d.node);
    const held = across(parent === null ? undefined : box(parent)?.transform, by);
    // Several move as the first does: on the grid, by its tracks, or with Shift off it.
    const how = d.with.length ? (d.shift ? "free" : "move") : snapOf(d.targets, editor.at(d.node), d.kind === "resize", d.shift);
    const to = d.kind === "move" ? moved(d.targets.cell, held) : resized(d.targets.cell, d.edge!, across(box(d.node)?.transform, by));
    return { how, to, by, held };
  }

  /** Ask where the drag lands now, and paint its node there: one request at a time, the latest
   * pointer's. */
  async function pump(d: Drag) {
    const shown = editor.shown();
    if (busy || drag !== d || !shown) return;
    const { how, to, by, held } = aimed(d);
    const asked = JSON.stringify([by, how, d.alt, d.loose]);
    if (asked === d.asked) return;
    d.asked = asked;
    d.how = how;
    busy = true;
    try {
      const move = d.with.length
        ? { by, with: d.with, together: { by: held, free: d.shift, fork: d.alt, reach: reach(d) } }
        : { by: d.kind === "move" ? by : undefined, snap: how ? { how, to, fork: d.alt, reach: reach(d) } : undefined };
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
      if (how) await stage.drag(shown.state, d.node, { snap: { how, to, fork: d.alt, reach: reach(d) }, preview: true }, editor.format());
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
    const { how, to, held } = aimed(d);
    let snapped: Snapped | null | undefined;
    try {
      const together = { with: d.with, together: { by: held, free: d.shift, fork: d.alt, reach: reach(d) } };
      if (d.with.length) snapped = (await stage.drag(shown.state, d.node, together, editor.format())).snapped;
      else if (how) snapped = (await stage.drag(shown.state, d.node, { snap: { how, to, fork: d.alt, reach: reach(d) } }, editor.format())).snapped;
    } catch (e) {
      return still(`not placed: ${said(e)}`);
    }
    if (!snapped?.patch.length) return still(d.with.length ? `${d.with.length + 1} selected stay where they are` : `${d.node} stays where it is`);
    await commit(snapped.patch, d.with.length ? `${d.with.length + 1} selected moved together` : undefined);
  }

  /** A turn of `node` begun on its rotate handle at `from` (PLAN 2.51): it turns about where its
   * anchor is drawn as the pointer goes round it, from the angle the state shows. */
  function turn(node: string, from: Point) {
    const b = box(node);
    if (!b) return;
    const t = editor.transform(node);
    const [ax, ay] = t?.anchor ?? [0.5, 0.5];
    const [x, y, w, h] = b.rect;
    const anchor: Point = [x + ax * w, y + ay * h];
    const pivot = b.transform ? apply(b.transform, anchor) : anchor;
    const start = typeof t?.rotate === "number" ? t.rotate : 0;
    const last = Math.atan2(from[1] - pivot[1], from[0] - pivot[0]);
    // What holds it, mirrored, draws its own clockwise turn anticlockwise.
    const parent = holder(node);
    const m = parent === null ? undefined : box(parent)?.transform;
    const way = m && m[0] * m[3] - m[1] * m[2] < 0 ? -1 : 1;
    turning = { node, pivot, last, round: 0, start, now: start, way, version: editor.version() };
    editor.say(`turning ${node} about its anchor · Shift by 15° · Escape leaves it as it is`);
    draw();
  }

  /** The pointer at `at` in turn `t`: the node's angle follows it, in whole degrees, or with Shift
   * in fifteens. */
  function turned(t: Turn, at: Point, shift: boolean) {
    const angle = Math.atan2(at[1] - t.pivot[1], at[0] - t.pivot[0]);
    let step = angle - t.last;
    if (step > Math.PI) step -= 2 * Math.PI;
    if (step < -Math.PI) step += 2 * Math.PI;
    t.last = angle;
    t.round += (step * 180) / Math.PI;
    const now = t.start + t.round * t.way;
    t.now = shift ? Math.round(now / 15) * 15 : Math.round(now);
    editor.say(`${t.node} turns to ${t.now}°`);
    draw();
  }

  /** Turn `t` let go: one `choose` of its angle, written where it lives. */
  async function turnTo(t: Turn) {
    const shown = editor.shown();
    if (!shown) return draw();
    if (editor.version() !== t.version) {
      draw();
      return editor.say("the source changed under the turn: nothing is turned");
    }
    if (t.now === t.start) {
      draw();
      return editor.say(`${t.node} stays as it is`);
    }
    const op = { op: "choose", node: t.node, prop: "transform/rotate", value: t.now, state: shown.state };
    await change([op], "turning…", `${t.node} turned to ${t.now}°`, t.node);
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
  async function change(ops: unknown[], doing: string, done: string, next?: string | string[] | null): Promise<boolean> {
    const shown = editor.shown();
    if (!shown) return false;
    editor.say(doing);
    try {
      const { source, edited } = await stage.make(editor.source(), ops, shown.index, editor.format());
      editor.apply(source, edited);
      await refresh();
      if (Array.isArray(next)) selectAll(next.filter((n) => box(n) !== undefined));
      else if (next !== undefined) select(next ?? undefined);
      editor.say(done);
      return true;
    } catch (e) {
      editor.say(`not made: ${said(e)}`);
      return false;
    }
  }

  /** Where what is inserted or pasted goes: where the pointer last pressed, on the canvas, or its
   * middle. */
  const landing = (): [number, number] =>
    pointed ? [Math.min(Math.max(pointed[0], 0), size[0]), Math.min(Math.max(pointed[1], 0), size[1])] : [size[0] / 2, size[1] / 2];

  /** Insert what the deck offers `n`th (`Stage.inserts`) where the pointer last pressed, or in the
   * middle of the canvas: it enters in the state shown, selected. */
  function insert(n: number, label = "it") {
    return inTurn(() => inserting(n, label));
  }
  async function inserting(n: number, label: string) {
    const shown = editor.shown();
    if (!shown) return editor.say("the canvas waits for a source that compiles");
    try {
      const added = await stage.inserting(editor.source(), shown.state, n, landing(), editor.format());
      await change(added.patch, "inserting…", `${label} inserted as ${added.id}, in ${shown.state}`, added.id);
    } catch (e) {
      editor.say(`not inserted: ${said(e)}`);
    }
  }

  /** Arm the canvas to draw what `key` draws (PLAN 2.48); armed with it already, stop. */
  function arm(key: string) {
    if (armed?.key === key) return disarm("not drawing");
    const fits = DRAWS[key];
    const n = Math.max(offered.findIndex((i) => fits(i) && i.id === "body"), offered.findIndex(fits));
    if (n < 0) return editor.say(`nothing to draw with ${key.toUpperCase()}: the deck offers none`);
    const { label, node } = offered[n];
    armed = { key, n, label: label.split(" · ").at(-1)!, text: node.type === "text", line: key === "l" || key === "a" };
    overlay.classList.add("drawing");
    draw();
    editor.say(`drawing ${label}: drag where it goes, or click · Shift draws off the grid · Escape stops`);
  }

  function disarm(why?: string) {
    armed = sketch = undefined;
    overlay.classList.remove("drawing");
    draw();
    if (why) editor.say(why);
  }

  /** Ask where the drag drawing lands, one request at a time: the last asked once the one before
   * answers. */
  async function land(s: Sketch) {
    const shown = editor.shown();
    const asked = JSON.stringify([s.at, s.shift]);
    if (s.asking || sketch !== s || !armed || !shown || asked === s.asked) return;
    [s.asked, s.asking] = [asked, true];
    try {
      const added = await stage.drawing(editor.source(), shown.state, armed.n, [s.from, s.at], s.shift, editor.format());
      if (sketch !== s) return;
      s.cell = added.cell;
      draw();
    } catch {
      // Said when it is let go, if it is drawn nowhere.
    } finally {
      s.asking = false;
      if (sketch === s) void land(s);
    }
  }

  /** The drag drawing let go: what is armed, drawn where it covers, or, a click, placed there as
   * Insert places it. Either is one patch; it enters in the state shown, selected, and a text
   * takes the caret, its words selected, so what is typed takes their place. */
  function drawn(s: Sketch, click: boolean) {
    const now = armed;
    disarm();
    if (!now) return;
    return inTurn(async () => {
      const shown = editor.shown();
      if (!shown) return editor.say("the canvas waits for a source that compiles");
      try {
        const added: Added = click
          ? await stage.inserting(editor.source(), shown.state, now.n, s.from, editor.format())
          : await stage.drawing(editor.source(), shown.state, now.n, [s.from, s.at], s.shift, editor.format());
        const [doing, done] = click ? ["inserting…", "inserted"] : ["drawing…", "drawn"];
        await change(added.patch, doing, `${now.label} ${done} as ${added.id}, in ${shown.state}`, added.id);
        if (now.text && selected === added.id) await type(added.id, undefined, false, true);
      } catch (e) {
        editor.say(`not ${click ? "inserted" : "drawn"}: ${said(e)}`);
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

  /** A chart annotated from its marks (PLAN 2.67). A click on a mark of the chart selected picks
   * it, and its menu annotates it; a click on one of the chart's annotations selects it, Delete
   * takes it away, a double click changes what it says, and a callout dragged moves onto the mark
   * there or to the value there. Each is one `annotate` op, written where the chart's annotations
   * live, or kept to the state shown with Alt or the inspector's "Only in this state". */
  /** Let go of the mark picked and the annotation selected, unless they are `node`'s. */
  function letGo(node: string | undefined) {
    if (picked && picked.mark.node !== node) picked = undefined;
    if (noted && noted.note.node !== node) noted = undefined;
    if (banding && banding.from.node !== node) banding = undefined;
  }
  /** What is at `at` of the chart selected: an annotation, selected; else a mark, picked; else
   * neither. Whether one was. */
  async function pickAt(at: Point): Promise<boolean> {
    const shown = editor.shown();
    if (!shown || selected === undefined || also.length) return false;
    const chart = selected;
    const note = await stage.noteAt(shown.state, at, editor.format()).catch(() => undefined);
    if (note?.node === chart) {
      [noted, picked] = [{ state: shown.state, note }, undefined];
      draw();
      editor.say(`${noteName(note)} of ${chart} selected: Delete takes it away${note.kind === "callout" ? ", a drag moves it" : ""}, a double click changes what it says`);
      return true;
    }
    const mark = await stage.markAt(shown.state, at, editor.format()).catch(() => undefined);
    if (mark?.node === chart && mark.notes) {
      [picked, noted] = [{ state: shown.state, mark }, undefined];
      draw();
      const what = mark.notes.axes ? "highlight it, call it out, rule its value, or band from it" : "highlight it";
      editor.say(`${markName(mark)} of ${chart} picked: its menu will ${what}`);
      return true;
    }
    if (picked || noted) [picked, noted] = [undefined, undefined];
    draw();
    return false;
  }
  /** Whether a change is kept to the state shown. */
  const keeping = (alt = false) => alt || (editor.keeping?.() ?? false);
  /** `ops`, annotations of the chart selected, as one patch; the mark picked asked for again, so
   * it says what the change made of it. */
  async function annotating(ops: unknown[], doing: string, done: string) {
    const was = picked;
    noted = undefined;
    await change(ops, doing, done);
    const shown = editor.shown();
    if (!was || !shown || was.state !== shown.state) return;
    const [x, y, w, h] = was.mark.rect;
    const middle: Point = was.mark.transform ? apply(was.mark.transform, [x + w / 2, y + h / 2]) : [x + w / 2, y + h / 2];
    const mark = await stage.markAt(shown.state, middle, editor.format()).catch(() => undefined);
    picked = mark?.node === was.mark.node && mark.key === was.mark.key ? { state: shown.state, mark } : undefined;
    draw();
  }
  /** Annotate the mark picked: highlight it, or its series; call it out, with words asked for over
   * it; rule its value; begin a band at it; or take away the highlights that pick it out. */
  function annotateMark(how: "highlight" | "series" | "callout" | "rule" | "band" | "unhighlight") {
    return inTurn(async () => {
      const shown = editor.shown();
      const p = picked;
      if (!shown || !p || p.state !== shown.state || !p.mark.notes) return editor.say("pick a mark of a chart first: select the chart, then click the mark");
      const [node, n, name, fork] = [p.mark.node, p.mark.notes, markName(p.mark), keeping()];
      const one = (annotation: Record<string, unknown>) => [annotate(node, shown.state, annotation, undefined, fork)];
      const kept = fork ? ` · kept to ${shown.state}` : "";
      switch (how) {
        case "highlight":
          return annotating(one(notes.highlight(n)), "highlighting…", `${name} highlighted in ${node}${kept}`);
        case "series":
          if (n.series === null) return editor.say(`${node} has no series to highlight`);
          return annotating(one(notes.highlightSeries(n)), "highlighting…", `${n.series} highlighted in ${node}${kept}`);
        case "unhighlight":
          if (!n.highlighted.length) return editor.say(`no highlight picks out ${name}`);
          return annotating(notes.unhighlight(node, shown.state, n, fork), "taking the highlight away…", `${name} is no longer highlighted${kept}`);
        case "rule":
          if (!n.axes) return editor.say(`${node} has no axes to rule`);
          return annotating(one(notes.rule(n)), "ruling…", `${node} ruled at ${name}'s value, ${n.value}${kept}`);
        case "band":
          if (!n.axes) return editor.say(`${node} has no axes to band`);
          banding = { state: shown.state, from: p.mark };
          draw();
          return editor.say(`click the mark of ${node} the band from ${name} ends at · Escape stops`);
        case "callout": {
          if (!n.axes) return editor.say(`${node} takes highlights alone`);
          const r = overlay.getBoundingClientRect();
          const [x, y, w] = p.mark.rect;
          const top = onScreen(p.mark.transform ? apply(p.mark.transform, [x + w / 2, y]) : [x + w / 2, y]);
          const words = (await askWords([top[0], Math.max(top[1], r.top + 32)], "", `What the callout on ${name} says`))?.trim();
          if (!words) return editor.say(`${name} is not called out`);
          return annotating(one(notes.callout(n, words)), "calling out…", `${name} called out: “${words}”${kept}`);
        }
      }
    });
  }
  /** The band begun ends at the mark of its chart at `at`: one band from the one to the other. */
  function endBand(at: Point) {
    return inTurn(async () => {
      const b = banding;
      const shown = editor.shown();
      if (!b || !shown || b.state !== shown.state || !b.from.notes) return;
      const mark = await stage.markAt(shown.state, at, editor.format()).catch(() => undefined);
      if (!mark?.notes || mark.node !== b.from.node) return editor.say(`click a mark of ${b.from.node} the band ends at · Escape stops`);
      if (mark.key === b.from.key) return editor.say(`the band from ${markName(mark)} ends at another mark · Escape stops`);
      banding = undefined;
      const op = annotate(b.from.node, shown.state, notes.band(b.from.notes, mark.notes), undefined, keeping());
      await annotating([op], "banding…", `${b.from.node} banded from ${markName(b.from)} to ${markName(mark)}`);
    });
  }
  /** The callout carried, let go at `at`: on the mark there, or at the value there. */
  function dropNote(c: NonNullable<typeof carrying>) {
    return inTurn(async () => {
      const shown = editor.shown();
      if (!shown || shown.state !== c.state) return;
      const moved = Math.hypot(c.at[0] - c.from[0], c.at[1] - c.from[1]) > 0;
      const at = moved ? await stage.calloutAt(shown.state, c.note.node, c.at, editor.format()).catch(() => undefined) : undefined;
      if (!at) {
        noted = { state: c.state, note: c.note };
        draw();
        return editor.say(`${noteName(c.note)} of ${c.note.node} stays where it is`);
      }
      const op = annotate(c.note.node, shown.state, { at }, c.note.index, keeping(c.alt));
      await annotating([op], "moving the callout…", `${noteName(c.note)} of ${c.note.node} moved${keeping(c.alt) ? ` · kept to ${shown.state}` : ""}`);
    });
  }
  /** Change what the annotation selected says, in words asked for over it; none takes a rule's or a
   * band's words away. */
  const reword = () => inTurn(rewording);
  async function rewording() {
    const shown = editor.shown();
    const n = noted;
    if (!shown || !n || n.state !== shown.state) return editor.say("select an annotation of a chart first");
    const [x, y, w] = n.note.rect;
    const top = onScreen(n.note.transform ? apply(n.note.transform, [x + w / 2, y]) : [x + w / 2, y]);
    const words = await askWords(top, n.note.text ?? "", `What ${noteName(n.note)} of ${n.note.node} says`);
    if (words === undefined || words.trim() === (n.note.text ?? "")) return editor.say(`${noteName(n.note)} says what it said`);
    const text = words.trim();
    if (!text && n.note.kind === "callout") return editor.say("a callout says something: Delete takes it away");
    const op = annotate(n.note.node, shown.state, { text: text || null }, n.note.index, keeping());
    await annotating([op], "rewording…", text ? `${noteName(n.note)} of ${n.note.node} says “${text}”` : `${noteName(n.note)} of ${n.note.node} says nothing`);
  }
  /** Take the annotation selected away. */
  function unnote() {
    return inTurn(async () => {
      const shown = editor.shown();
      const n = noted;
      if (!shown || !n || n.state !== shown.state) return editor.say("select an annotation of a chart first");
      const op = annotate(n.note.node, shown.state, null, n.note.index, keeping());
      await annotating([op], "taking it away…", `${noteName(n.note)} of ${n.note.node} taken away`);
    });
  }

  /** A file dropped on the canvas. A picture dropped on an image takes its place (PLAN 2.45): the
   * file joins the bundle, named by its SHA-256 as one dropped on the source is, and the image's
   * `src` is its path, one `choose` written where `src` lives. A picture dropped anywhere else
   * joins the bundle and is inserted there, as Insert inserts it; a CSV or JSON file joins it as
   * a data source, declared as `data_attach` declares it, with a chart of it there (the
   * first-deck walk). */
  function dropped(at: [number, number], file: File) {
    return inTurn(async () => {
      const shown = editor.shown();
      if (!shown) return editor.say("the canvas waits for a source that compiles");
      if (DATA.test(file.name)) return attach(at, file);
      // An image shows a PNG or a JPEG (SPEC §3.3): anything else stays out of the bundle.
      if (!PICTURE.test(file.name)) return editor.say(`${file.name} is neither a picture (PNG, JPEG) nor data (CSV, JSON): nothing was added`);
      const top = (await stage.hit(shown.state, at, editor.format()).catch(() => []))[0];
      const choices = top && (await stage.choices(shown.state, top.node).catch(() => undefined));
      try {
        const path = await stage.drop(file.name, await file.arrayBuffer());
        if (top && choices?.type === "image") {
          const op = { op: "choose", node: top.node, prop: "src", value: path, state: shown.state };
          return void (await change([op], "replacing…", `${top.node} shows ${file.name}, kept as ${path}`, top.node));
        }
        offered = await stage.inserts();
        await inserted(at, path, file.name);
      } catch (e) {
        editor.say(`not added: ${said(e)}`);
      }
    });
  }
  /** A data file dropped on the canvas joins the bundle as a source, as `data_attach` declares
   * it, one change; a chart of it goes where it was dropped, as Insert inserts one, another. The
   * same file again is the source that reads it, and other rows under its name go beside it. */
  async function attach(at: [number, number], file: File) {
    try {
      const { path, data, attached, patch } = await stage.attaching(editor.source(), file.name, await file.arrayBuffer());
      let made = `${file.name} is @${data} already`;
      if (attached) {
        if (!patch.length) {
          const why = attached.added.map((f) => `${f.code} ${f.message}`).join("; ");
          return editor.say(`${file.name} not attached: ${why || "the deck would not validate with it"}`);
        }
        made = `${file.name} attached as @${data}, ${attached.rows} rows${path.endsWith(`/${file.name}`) ? "" : `, kept as ${path}`}`;
        if (!(await change(patch, "attaching…", made, null))) return;
      }
      offered = await stage.inserts();
      const n = offered.findIndex((i) => i.node.type === "chart" && i.node.data === `@${data}`);
      if (n < 0) return editor.say(`${made}: Insert offers a table of it, since its columns make no chart`);
      pointed = at;
      await inserting(n, `a chart of @${data}`);
    } catch (e) {
      editor.say(`not attached: ${said(e)}`);
    }
  }
  /** An image of the bundle dragged from the Files panel (PLAN 2.59): over an image node, it takes
   * that image's place, one `choose` of `src` written where it lives; anywhere else, it is inserted
   * there, as Insert inserts it. */
  function place(at: [number, number], path: string) {
    return inTurn(async () => {
      const shown = editor.shown();
      if (!shown) return editor.say("the canvas waits for a source that compiles");
      const top = (await stage.hit(shown.state, at, editor.format()).catch(() => []))[0];
      const choices = top && (await stage.choices(shown.state, top.node).catch(() => undefined));
      if (top && choices?.type === "image") {
        const op = { op: "choose", node: top.node, prop: "src", value: path, state: shown.state };
        return change([op], "replacing…", `${top.node} shows ${path}`, top.node);
      }
      await inserted(at, path, path);
    });
  }
  /** The image of the bundle at `path` inserted at `at`, as Insert inserts it; `label` names it. */
  async function inserted(at: [number, number], path: string, label: string) {
    const n = offered.findIndex((i) => i.node.type === "image" && (i.node as { src?: unknown }).src === path);
    if (n < 0) return editor.say(`${path} is not an image the deck can insert: a PNG or a JPEG in the bundle`);
    pointed = at;
    await inserting(n, label);
  }
  overlay.addEventListener("dragover", (e) => {
    const types = e.dataTransfer?.types ?? [];
    if (types.includes("Files") || types.includes(BUNDLE_PATH)) e.preventDefault();
  });
  overlay.addEventListener("drop", (e) => {
    const path = e.dataTransfer?.getData(BUNDLE_PATH);
    if (path) {
      e.preventDefault();
      return void place(point(e), path);
    }
    const file = e.dataTransfer?.files[0];
    if (!file) return;
    e.preventDefault();
    void dropped(point(e), file);
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

  /** The look ⌥⌘C copied last (PLAN 2.58): what ⌥⌘V pastes, until another is copied. */
  let copiedLook: Look | undefined;
  /** What a look sets, as the status says it: each value the deck sets, else the theme's. */
  const lookSaid = (look: Look) => {
    const set = look.props.flatMap((p) => (p.value === undefined ? [] : [`${p.prop} ${typeof p.value === "string" ? p.value : JSON.stringify(p.value)}`]));
    return set.length ? set.join(", ") : "the theme's";
  };

  /** ⌥⌘C: the look of the node selected, as the state shown shows it, copied for ⌥⌘V. */
  function copyLook() {
    return inTurn(async () => {
      const shown = editor.shown();
      const nodes = chosen();
      if (!shown || nodes.length !== 1) return editor.say("select one node to copy its look");
      try {
        const look = await stage.look(editor.source(), shown.state, nodes[0]);
        if (!look.props.length) return editor.say(`a ${look.type} has no look of its own to copy`);
        copiedLook = look;
        editor.say(`${look.node}'s look copied (${lookSaid(look)}): ⌥⌘V pastes it on what is selected`);
      } catch (e) {
        editor.say(`no look copied: ${said(e)}`);
      }
    });
  }

  /** ⌥⌘V: the look copied pasted on each node selected that takes it, in the state shown, one
   * patch: one step to undo. What is selected stays so; the status says which nodes looked so
   * already and which take none of it. */
  function pasteLook() {
    return inTurn(async () => {
      const shown = editor.shown();
      const nodes = chosen();
      const look = copiedLook;
      if (!look) return editor.say("no look is copied: ⌥⌘C copies the look of the node selected");
      if (!shown || !nodes.length) return editor.say(`select what takes ${look.node}'s look`);
      try {
        const put = await stage.putting(editor.source(), shown.state, look, nodes);
        const same = put.same.length ? [`${put.same.join(", ")} ${put.same.length > 1 ? "look" : "looks"} so already`] : [];
        const others = [...same, ...put.refused.map((r) => `${r.node}: ${r.why}`)];
        if (!put.patch.length) return editor.say(others.join("; ") || `nothing takes ${look.node}'s look`);
        const done = [`${look.node}'s look pasted on ${put.took.join(", ")}`, ...others].join("; ");
        await change(put.patch, "pasting the look…", done);
      } catch (e) {
        editor.say(`no look pasted: ${said(e)}`);
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
    righted = e.button === 2;
    if (e.button !== 0) return;
    // A layout shown (PLAN 2.71): a press moves or resizes a slot, and nothing else.
    if (slotting) {
      e.preventDefault();
      overlay.focus();
      overlay.setPointerCapture(e.pointerId);
      return slotPress(e);
    }
    // Armed to draw (PLAN 2.48): a drag draws what is armed, and a click places it.
    if (armed) {
      e.preventDefault();
      if (text.node() !== undefined) text.leave();
      overlay.focus();
      overlay.setPointerCapture(e.pointerId);
      const from = point(e);
      pointed = from;
      sketch = { from, at: from, client: [e.clientX, e.clientY], shift: e.shiftKey };
      return draw();
    }
    // A press that picks an image's focal point picks it, and does nothing else (PLAN 2.45).
    if (picking !== undefined) {
      e.preventDefault();
      const node = picking;
      unpick();
      return void focus(node, point(e));
    }
    // A band begun ends at the mark pressed, and the press does nothing else (PLAN 2.67).
    if (banding) {
      e.preventDefault();
      overlay.focus();
      return void endBand(point(e));
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
    // A shape's point, an edge's middle, or a rect's corner (PLAN 2.68).
    const handle = (e.target as Element).closest?.("[data-point],[data-add],[data-radius]");
    if (handle && selected !== undefined && shaped?.outline.node === selected) {
      e.preventDefault();
      return reshape(handle, from, client, e.altKey);
    }
    // An image's crop handle, or its focal point (PLAN 2.74).
    const crop = (e.target as Element).closest?.("[data-crop],[data-focal]");
    if (crop && selected !== undefined && imaged?.framing.node === selected) {
      e.preventDefault();
      return cropStart(crop, from, client, e.altKey);
    }
    const edge = (e.target as Element).closest?.("[data-edge]")?.getAttribute("data-edge") as Edge | null;
    if (edge && selected !== undefined) {
      press = { node: selected, edge, from, client };
      return;
    }
    // The rotate handle (PLAN 2.51). The turn draws the handle anew under the press, so the press
    // does not move focus itself: the canvas keeps it, and Escape reaches it.
    if ((e.target as Element).closest?.("[data-turn]") && selected !== undefined) {
      e.preventDefault();
      return turn(selected, from);
    }
    const mine: Press = { from, client, asking: true, shift };
    press = mine;
    // With one node selected, what annotation of a chart is there too (PLAN 2.67).
    const one = selected !== undefined && also.length === 0;
    const [hits, note] = await Promise.all([
      stage.hit(shown.state, from, editor.format()).catch(() => []),
      one ? stage.noteAt(shown.state, from, editor.format()).catch(() => undefined) : Promise.resolve(undefined),
    ]);
    mine.asking = false;
    const top = hits[0];
    const chain = top ? [top.node, ...top.containers] : [];
    // In what is selected, or in what holds it, the press keeps it: a drag moves it, with the
    // rest of what is selected, and a click selects what is topmost.
    const all = chosen();
    const keeps = selected !== undefined && chain.some((n) => all.includes(n));
    const node = keeps ? selected : top?.node;
    if (keeps) mine.with = also.slice();
    // On the chart selected itself, a click picks a mark or selects an annotation, and a callout
    // pressed is dragged where it goes (PLAN 2.67).
    mine.within = keeps && also.length === 0 && top?.node === selected;
    if (mine.within && note?.node === selected) mine.note = note;
    // With Shift, a click puts what was clicked in the selection, or takes it out of it: in what
    // holds the selection, the child of it the click is in.
    if (shift && selected !== undefined) {
      const holds = holder(selected);
      mine.toggle = chain.find((n) => all.includes(n)) ?? chain.find((n) => holder(n) === holds) ?? top?.node;
    }
    // On nothing, or on what fills the canvas behind all, a drag draws a marquee.
    const below = top && box(top.node);
    if (!keeps && (!top || covers(below && drawnBox(below)))) mine.marquee = true;
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
    if (far && mine.note?.kind === "callout" && (press === mine || mine.released)) {
      if (press === mine) press = undefined;
      const c = { state: shown.state, note: mine.note, from, at: moved.at, alt: moved.alt };
      if (mine.released) return void dropNote(c);
      carrying = c;
      return draw();
    }
    if (far && node !== undefined && mine.toggle === undefined && (press === mine || mine.released)) {
      if (press === mine) press = undefined;
      early++;
      if (!keeps) select(node);
      return begin({ ...mine, node }, { at: moved.at, shift: moved.shift, alt: moved.alt, loose: moved.loose, up: mine.released });
    }
    // A click let go before the engine answered selects what is topmost; in the chart selected, it
    // picks a mark or selects an annotation there (PLAN 2.67).
    if (mine.released) {
      if (mine.toggle !== undefined) return toggle(mine.toggle);
      select(top?.node);
      if (mine.within && !shift) await pickAt(from);
      return;
    }
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
          loose: now.loose === true,
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
      if (!top) return;
      // On an annotation of the chart selected, what it says, changed (PLAN 2.67).
      if (top.node === selected && also.length === 0) {
        const note = await stage.noteAt(shown.state, at, editor.format()).catch(() => undefined);
        if (note?.node === top.node) {
          [noted, picked] = [{ state: shown.state, note }, undefined];
          draw();
          return rewording();
        }
      }
      // On a chart's mark, or a table's row, the Data tab opens on its rows (PLAN 2.64).
      if (await editor.pointedAt?.(at, true)) return;
      await type(top.node, at, alt);
    });
  };

  // A right click (PLAN 2.53): what is topmost there is selected, as a click selects it, unless it
  // is in what is selected already; then the editor offers what can be done to it, or to the canvas
  // where nothing is, and what is pasted or inserted from there lands there. From the keyboard (the
  // menu key, Shift+F10), what is selected is offered, beside it.
  overlay.oncontextmenu = (e) => {
    e.preventDefault();
    const pointer = righted;
    righted = false;
    if (drag || starting || sketch || turning || panning) return;
    if (text.node() !== undefined) text.leave();
    if (!pointer) {
      const b = box(selected);
      const r = overlay.getBoundingClientRect();
      const [x, y, , h] = b ? drawnBox(b) : [0, 0, 0, 0];
      const [cx, cy] = b ? onScreen([x, y + h]) : [r.left + r.width / 2, r.top + r.height / 2];
      return editor.menu(cx, cy, selected === undefined ? "canvas" : "node");
    }
    const at = point(e);
    const client: Point = [e.clientX, e.clientY];
    pointed = at;
    overlay.focus();
    void inTurn(async () => {
      const shown = editor.shown();
      if (!shown) return editor.say("the canvas waits for a source that compiles");
      const top = (await stage.hit(shown.state, at, editor.format()).catch(() => []))[0];
      const chain = top ? [top.node, ...top.containers] : [];
      if (!chain.some((n) => chosen().includes(n))) select(top?.node);
      // On a chart's mark, or one of its annotations, the menu offers what annotates it (PLAN 2.67).
      if (top && top.node === selected && also.length === 0) await pickAt(at);
      editor.menu(client[0], client[1], selected === undefined ? "canvas" : "node");
    });
  };

  overlay.onpointermove = (e) => {
    if (panning) {
      const [p, u] = [panning, unit()];
      return void look([p.from[0] - (e.clientX - p.client[0]) * u, p.from[1] - (e.clientY - p.client[1]) * u, p.from[2], p.from[3]]);
    }
    if (sketch) {
      [sketch.at, sketch.shift] = [point(e), e.shiftKey];
      draw();
      return void land(sketch);
    }
    const at = point(e);
    if (text.drag(at)) return;
    if (starting) {
      [starting.at, starting.shift, starting.alt, starting.loose] = [at, e.shiftKey, e.altKey, e.metaKey || e.ctrlKey];
      return;
    }
    if (drag) {
      [drag.at, drag.shift, drag.alt, drag.loose] = [at, e.shiftKey, e.altKey, e.metaKey || e.ctrlKey];
      if (drag.kind === "move") draw();
      void pump(drag);
      return;
    }
    if (turning) return turned(turning, at, e.shiftKey);
    if (slotDrag) return slotMove(at);
    if (reshaping) return reshapeTo(reshaping, at, [e.clientX, e.clientY], e.altKey);
    if (cropping) return cropTo(cropping, at, [e.clientX, e.clientY], e.altKey);
    if (carrying) {
      [carrying.at, carrying.alt] = [at, e.altKey];
      return draw();
    }
    if (marquee) {
      marquee.at = at;
      return draw();
    }
    if (!press) {
      // What a click would select, from the boxes the engine gave: nothing is asked.
      const under = boxes.filter((b) => b.draws && over(b, at)).at(-1)?.node;
      if (under !== hovered) {
        hovered = under;
        draw();
      }
      return;
    }
    if (press.asking) {
      press.moved = { at, shift: e.shiftKey, alt: e.altKey, loose: e.metaKey || e.ctrlKey, client: [e.clientX, e.clientY] };
      return;
    }
    const far = Math.hypot(e.clientX - press.client[0], e.clientY - press.client[1]) >= SLOP;
    if (far && press.marquee) {
      marquee = { from: press.from, at, adding: press.shift === true };
      press = undefined;
      return draw();
    }
    if (far && press.note?.kind === "callout") {
      const state = editor.shown()?.state;
      if (state) carrying = { state, note: press.note, from: press.from, at, alt: e.altKey };
      press = undefined;
      return draw();
    }
    if (!press.node || !far) return;
    const begun = press;
    press = undefined;
    begin(begun, { at, shift: e.shiftKey, alt: e.altKey, loose: e.metaKey || e.ctrlKey });
  };

  overlay.onpointerup = (e) => {
    if (overlay.hasPointerCapture(e.pointerId)) overlay.releasePointerCapture(e.pointerId);
    if (panning?.pointer === e.pointerId) {
      panning = undefined;
      if (!spaced) overlay.classList.remove("panning");
      return;
    }
    if (sketch) {
      const s = sketch;
      [s.at, s.shift] = [point(e), e.shiftKey];
      return void drawn(s, Math.hypot(e.clientX - s.client[0], e.clientY - s.client[1]) < SLOP);
    }
    if (text.up()) return;
    if (starting) {
      [starting.at, starting.shift, starting.alt, starting.loose, starting.up] = [point(e), e.shiftKey, e.altKey, e.metaKey || e.ctrlKey, true];
      return;
    }
    if (drag) {
      const d = drag;
      drag = undefined;
      [d.at, d.shift, d.alt, d.loose] = [point(e), e.shiftKey, e.altKey, e.metaKey || e.ctrlKey];
      void inTurn(() => drop(d));
      return;
    }
    if (turning) {
      const t = turning;
      turned(t, point(e), e.shiftKey);
      turning = undefined;
      return void inTurn(() => turnTo(t));
    }
    if (slotDrag) {
      slotMove(point(e));
      return void inTurn(slotDrop);
    }
    if (reshaping) {
      const r = reshaping;
      reshapeTo(r, point(e), [e.clientX, e.clientY], e.altKey);
      reshaping = undefined;
      return void inTurn(() => reshapeEnd(r));
    }
    if (cropping) {
      const c = cropping;
      cropTo(c, point(e), [e.clientX, e.clientY], e.altKey);
      cropping = undefined;
      return void inTurn(() => cropEnd(c));
    }
    if (marquee) {
      marquee.at = point(e);
      return finish();
    }
    if (carrying) {
      const c = carrying;
      carrying = undefined;
      [c.at, c.alt] = [point(e), e.altKey];
      return void dropNote(c);
    }
    const clicked = press;
    press = undefined;
    if (!clicked) return;
    // A click on a chart's mark, or a table's row, chooses its rows where the data is shown (PLAN 2.64).
    if (!clicked.shift) void editor.pointedAt?.(clicked.from, false);
    if (clicked.asking) {
      clicked.moved = { at: point(e), shift: e.shiftKey, alt: e.altKey, loose: e.metaKey || e.ctrlKey, client: [e.clientX, e.clientY] };
      clicked.released = true;
    } else if (clicked.toggle !== undefined) toggle(clicked.toggle);
    else if (clicked.marquee) select(clicked.click);
    else if (clicked.click !== undefined) select(clicked.click);
    // In the chart selected, a click picks a mark or selects an annotation (PLAN 2.67).
    if (!clicked.asking && clicked.within && !clicked.shift) void pickAt(clicked.from);
  };

  overlay.onpointercancel = () => {
    press = starting = marquee = undefined;
    if (reshaping || cropping) {
      reshaping = cropping = undefined;
      draw();
    }
    if (sketch) disarm("not drawn: the drag was cancelled");
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
    // The theme's grid, drawn or not (PLAN 2.57); while typing too.
    if (mod && !e.altKey && !e.shiftKey && key === "'" && !drag) {
      e.preventDefault();
      return void rule();
    }
    // The text typed in takes its own keys.
    if (text.node() !== undefined) return;
    // A layout shown takes Escape, and leaves the rest alone (PLAN 2.71).
    if (slotting && !(mod && ["z", "y"].includes(key))) {
      // Tab and Shift+Tab step through the slots; an arrow moves the one keyed a track, and with
      // Shift resizes it (PLAN 2.75).
      const slots = slotting.layout.slots;
      if (e.key === "Tab" && !mod && !e.altKey && !slotDrag && slots.length) {
        const next = slotKeyed === undefined ? (e.shiftKey ? slots.length - 1 : 0) : slotKeyed + (e.shiftKey ? -1 : 1);
        slotKeyed = next < 0 || next >= slots.length ? undefined : next;
        draw();
        if (slotKeyed === undefined) return editor.say("no slot keyed: the focus leaves the canvas");
        e.preventDefault();
        return editor.say(`slot ${slots[slotKeyed].name}, ${slotKeyed + 1} of ${slots.length} · an arrow moves it a track, Shift with one resizes it`);
      }
      const arrow = ({ ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] } as Record<string, Point>)[e.key];
      if (arrow && slotKeyed !== undefined && !mod && !slotDrag) {
        e.preventDefault();
        const [slot, grow] = [slots[slotKeyed], e.shiftKey];
        return void inTurn(() => slotNudge(slot, arrow, grow));
      }
      if (e.key !== "Escape") return;
      e.preventDefault();
      if (slotKeyed !== undefined && !slotDrag) {
        slotKeyed = undefined;
        draw();
        return editor.say("no slot keyed: Escape again leaves the layout");
      }
      if (slotDrag) {
        slotDrag = undefined;
        draw();
        return editor.say("the slot stays where it was");
      }
      return void layoutMode(false);
    }
    if (picking !== undefined && e.key === "Escape") {
      e.preventDefault();
      unpick();
      return editor.say("the focal point is as it was");
    }
    if (banding && e.key === "Escape") {
      e.preventDefault();
      banding = undefined;
      draw();
      return editor.say("no band made");
    }
    if (armed && e.key === "Escape") {
      e.preventDefault();
      return disarm(sketch ? "not drawn" : "not drawing");
    }
    // T, R, O, L, and A arm the canvas to draw a text, a rectangle, an ellipse, a line, or an
    // arrow (PLAN 2.48); the same key again stops.
    if (Object.hasOwn(DRAWS, key) && !mod && !e.altKey && !drag && !starting && !sketch) {
      e.preventDefault();
      return arm(key);
    }
    if (e.key === " " && !mod && !drag) {
      e.preventDefault();
      spaced = true;
      if (zoom() > 1) overlay.classList.add("panning");
      return;
    }
    // Tab and Shift+Tab: the next node in reading order, or the next handle (PLAN 2.75).
    if (e.key === "Tab" && !mod && !e.altKey && !drag && !starting) {
      if (handling && handling.node === selected) {
        const list = handleList(handling.node);
        if (list.length) {
          e.preventDefault();
          handling.index = (handling.index + (e.shiftKey ? list.length - 1 : 1)) % list.length;
          reachHandle(list[handling.index]);
          draw();
          return editor.say(handleSaid(handling.node, list[handling.index], handling.index, list.length));
        }
      }
      if (tabTo(e.shiftKey)) e.preventDefault();
      return;
    }
    if (e.key === "Enter" && selected !== undefined && !drag && !mod) {
      e.preventDefault();
      // On an annotation's handle: what it says, asked for, as a double click asks (PLAN 2.75).
      const on = handling?.node === selected ? handleList(selected)[handling.index] : undefined;
      if (on?.kind === "note") return void rewording();
      if (on) return editor.say(handleSaid(selected, on, handling!.index, handleList(selected).length));
      // Into a container or a group: its first node (PLAN 2.75).
      const inside = readingOrder(selected);
      if (inside.length && !also.length) {
        const into = selected;
        select(inside[0]);
        return editor.say(`${inside[0]} selected, in ${into} · Tab goes on, Escape goes back out`);
      }
      // A shape's or an image's handles.
      const list = also.length ? [] : handleList(selected);
      if (list.length && !e.altKey) {
        handling = { node: selected, index: 0 };
        reachHandle(list[0]);
        draw();
        return editor.say(handleSaid(selected, list[0], 0, list.length));
      }
      return void type(selected, undefined, e.altKey);
    }
    // [ and ] turn what is selected 15° back or on, with Shift 1° (PLAN 2.75).
    if ((e.code === "BracketLeft" || e.code === "BracketRight") && !mod && !e.altKey && selected !== undefined && !also.length && !drag) {
      e.preventDefault();
      const node = selected;
      const by = (e.code === "BracketRight" ? 1 : -1) * (e.shiftKey ? 1 : 15);
      return void inTurn(() => turnBy(node, by));
    }
    if (mod && (key === "z" || key === "y")) {
      e.preventDefault();
      return key === "y" || e.shiftKey ? editor.redo() : editor.undo();
    }
    // ⌘A: what is selected and everything beside it, in what holds it; with nothing selected, all
    // that stands on the canvas (PLAN 2.75).
    if (mod && key === "a" && !e.shiftKey && !e.altKey && !drag) {
      e.preventDefault();
      const level = selected === undefined ? null : (box(selected)?.parent ?? null);
      const all = readingOrder(level);
      selectAll(all);
      return editor.say(`${all.length} selected${level ? ` in ${level}` : ""}`);
    }
    // ⌥⌘C copies the look of the node selected, and ⌥⌘V pastes it on what is selected (PLAN 2.58):
    // by the key's place, as Option makes ⌥C a character of its own.
    if (mod && e.altKey && !e.shiftKey && (e.code === "KeyC" || e.code === "KeyV") && !drag && !starting) {
      e.preventDefault();
      return void (e.code === "KeyC" ? copyLook() : pasteLook());
    }
    if (selected !== undefined && !drag && !starting) {
      // On a point's handle, Delete takes it away and + adds one after it (PLAN 2.75).
      const keyed = handling?.node === selected ? handleList(selected)[handling.index] : undefined;
      if (keyed?.kind === "point" && (e.key === "+" || e.key === "=") && !mod) {
        e.preventDefault();
        const [node, i] = [selected, keyed.index!];
        return void inTurn(async () => {
          await addAfter(node, i, e.altKey);
          if (handling) handling.index = i + 1;
          draw();
        });
      }
      if (keyed?.kind === "point" && (e.key === "Delete" || e.key === "Backspace") && !mod) {
        e.preventDefault();
        pointPicked = keyed.index;
        return void unpoint().then(() => {
          if (handling) handling.index = Math.max(0, handling.index - 1);
          draw();
        });
      }
      if ((e.key === "Delete" || e.key === "Backspace") && !mod && !e.altKey) {
        e.preventDefault();
        // An annotation of the chart selected goes, and the chart stays (PLAN 2.67); so does a point
        // of the shape selected (PLAN 2.68).
        if (noted?.note.node === selected && !also.length) return void unnote();
        if (pointPicked !== undefined && shaped?.outline.node === selected && !also.length) return void unpoint();
        return void (also.length ? removeAll(chosen(), e.shiftKey) : remove(selected, e.shiftKey));
      }
      if (mod && key === "d" && !e.shiftKey && !e.altKey) {
        e.preventDefault();
        return void (also.length ? duplicateAll(chosen()) : duplicate(selected));
      }
      // ⌘⇧8 bullets a text, ⌘⇧7 numbers it (PLAN 2.69), by the keys' places.
      if (mod && e.shiftKey && !e.altKey && (e.code === "Digit8" || e.code === "Digit7")) {
        e.preventDefault();
        return void listing(e.code === "Digit8" ? "bullet" : "number");
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
      if (carrying) {
        e.preventDefault();
        const c = carrying;
        carrying = undefined;
        draw();
        return editor.say(`${noteName(c.note)} of ${c.note.node} stays where it is`);
      }
      // A point or a corner dragged is left as it was, and a point picked is let go before the shape
      // is (PLAN 2.68).
      if (reshaping) {
        e.preventDefault();
        const r = reshaping;
        reshaping = undefined;
        draw();
        return editor.say(`${r.outline.node} stays as it is`);
      }
      if (cropping) {
        e.preventDefault();
        const c = cropping;
        cropping = undefined;
        draw();
        return editor.say(`${c.framing.node}'s crop and focal point stay as they are`);
      }
      if (handling) {
        e.preventDefault();
        const node = handling.node;
        [handling, picked, noted] = [undefined, undefined, undefined];
        draw();
        return editor.say(`${node} selected: its handles left`);
      }
      if (pointPicked !== undefined) {
        e.preventDefault();
        pointPicked = undefined;
        draw();
        return editor.say(`${selected ?? "nothing"} selected`);
      }
      // A mark picked, or an annotation selected, is let go before the chart is (PLAN 2.67).
      if (picked || noted) {
        e.preventDefault();
        [picked, noted] = [undefined, undefined];
        draw();
        return editor.say(`${selected ?? "nothing"} selected`);
      }
      if (turning) {
        e.preventDefault();
        const t = turning;
        turning = undefined;
        draw();
        return editor.say(`${t.node} stays as it is`);
      }
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
    const keyed = handling?.node === selected ? handleList(selected)[handling.index] : undefined;
    if (keyed && (keyed.kind === "mark" || keyed.kind === "note")) return editor.say(`${keyed.label} does not move by the keys: pick another mark and call it out there`);
    if (keyed) {
      const [node, far, alt] = [selected, e.shiftKey, e.altKey];
      return void inTurn(() => nudgeHandle(node, keyed, step, far, alt));
    }
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
      drag = press = starting = armed = sketch = undefined;
      svg.replaceChildren();
      pins.found([]);
    },
    /** Draw the theme's grid over the canvas, or stop, as ⌘' does (PLAN 2.57); and whether it is
     * drawn, and what. */
    rule,
    ruled: () => ruled,
    grid: () => (ruled ? grid : undefined),
    /** The guides the drag under way shows: where its box meets others (PLAN 2.57). */
    guides: () => drag?.snapped?.guides ?? [],
    /** What lint found, every finding: those about the state shown, in the format shown, stand on
     * it (PLAN 2.49). */
    found: (findings: Finding[]) => pins.found(findings),
    /** The findings' marks shown, for a test. */
    marks: () => pins.shown(),
    /** Select `node`, as a click on it does; nothing with `undefined`. */
    select,
    /** Select `nodes` at once, children of one container, as Shift+click and a marquee do. */
    selectAll,
    /** Mark characters of a text in the state shown, as a find shows its match, and select it;
     * with no node, take the mark away (PLAN 2.47). */
    mark,
    /** Outline `marks`, what the rows chosen in the data draw in `state`, while it is shown; none
     * with none (PLAN 2.64). */
    markRows: (state: string | undefined, marks: DataMark[]) => {
      rowMarks = state === undefined || !marks.length ? undefined : { state, marks };
      draw();
    },
    /** The marks outlined, for a test. */
    rowMarks: () => rowMarks,
    /** The mark of the chart selected picked, the annotation selected, and the mark a band begun
     * starts at (PLAN 2.67). */
    picked: () => picked?.mark,
    noted: () => noted?.note,
    banding: () => banding?.from,
    /** The layout of the state shown as its slots, to edit, or not, as the Layout button does; a
     * layout added from it, as the command does; and the layout shown (PLAN 2.71). */
    layoutMode: (on?: boolean) => layoutMode(on),
    newLayout,
    slotting: () => slotting?.layout,
    /** The outline of the shape selected, and the point of it picked (PLAN 2.68). */
    outlined: () => shaped?.outline,
    /** The framing of the image selected (PLAN 2.74), for a test. */
    framing: () => imaged?.framing,
    /** The handle the keys work, for a test (PLAN 2.75). */
    handle: () => (handling ? { node: handling.node, index: handling.index, ...handleList(handling.node)[handling.index] } : undefined),
    pointPicked: () => pointPicked,
    /** Pick what is at `at` of the chart selected, as a click there does. */
    pickAt: (at: Point) => pickAt(at),
    /** What `rows` of data source `source` draw in the state shown, and the annotation drawn at
     * `at`, for a test. */
    marksOf: (source: string, rows: number[]) => {
      const shown = editor.shown();
      return shown ? stage.marksOf(shown.state, source, rows, editor.format()) : Promise.resolve([]);
    },
    noteAt: (at: Point) => {
      const shown = editor.shown();
      return shown ? stage.noteAt(shown.state, at, editor.format()) : Promise.resolve(undefined);
    },
    /** Annotate the mark picked, as its menu does; change what the annotation selected says, as a
     * double click does; take it away, as Delete does. */
    annotate: annotateMark,
    reword,
    unnote,
    /** Group what is selected, as ⌘G does, and take the group selected apart, as ⌘⇧G does. */
    group,
    ungroup,
    /** Pick the focal point of the image selected with the next press on it, as the inspector's
     * Pick does; and a file dropped on the canvas: a picture on an image, put in its place (PLAN
     * 2.45), else inserted there, and data attached as a source with a chart of it there. */
    pick,
    dropped,
    /** Arrange what is selected, as the inspector and ⌘] do (PLAN 2.42). */
    arrange,
    /** Insert what the deck offers `n`th, as the Insert menu does; `label` is what the status
     * calls it. */
    insert,
    /** Arm the canvas to draw what `key` draws, as T, R, O, L, and A do (PLAN 2.48). */
    arm,
    /** What the canvas is armed to draw, if anything: the key, and its name. */
    armed: () => armed && { key: armed.key, label: armed.label },
    /** A copy of `node` beside it, as ⌘D does. */
    duplicate,
    /** Take `node` out of the state shown on, as Delete does; `everywhere`, out of the deck, as
     * Shift+Delete does. */
    remove,
    /** Paste `clip`, the clipboard's text, as ⌘V does. */
    paste,
    /** Copy the look of the node selected, as ⌥⌘C does, and paste it on what is selected, as ⌥⌘V
     * does (PLAN 2.58); and the look copied, if any. */
    copyLook,
    pasteLook,
    copiedLook: () => copiedLook,
    /** Give the characters selected in the text typed in `look`, as the inspector does (PLAN 2.38). */
    style: (look: Record<string, unknown>, words?: string) => text.style(look, words),
    /** Make the characters selected bold, or not, as ⌘B does. */
    bold: () => text.bold(),
    /** Bullets or numbers on the text selected, or the paragraphs selected in it, as ⌘⇧8 and ⌘⇧7
     * do (PLAN 2.69). */
    list: listing,
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
    busy: () => busy || drag !== undefined || starting !== undefined || reshaping !== undefined || cropping !== undefined,
    /** The text typed in, if one is. */
    typing: () => text.node(),
    /** What is typed in it, for a test. */
    typed: () => text.now(),
    /** How many drags began from moves made before the engine said what was pressed, as a drag
     * pressed while the worker paints or lints is. */
    early: () => early,
  };
}
