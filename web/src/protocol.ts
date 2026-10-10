// What the pages and the engine's worker say to each other (PLAN 2.1–2.4, SPEC §9.2).
// Each request names the format it is in: one of the deck's `formats`, or the deck's own
// canvas when it names none (SPEC §3.4).

/** Where a bundle comes from (SPEC §3.1, §9.2). */
export type Source =
  /** A deck file at a URL, whose directory is the bundle: it reads the files the deck names,
   * and every file its manifest lists. `serve`, on a page `scaena serve` serves (PLAN 2.11):
   * the page's id, which its writes carry, for the bundle is kept there, and saves back to it. */
  | { url: string; serve?: string }
  /** A bundle kept in the browser's own storage (the origin-private file system), by its
   * name there. */
  | { opfs: string }
  /** A folder on disk the page was given (File System Access). */
  | { folder: FileSystemDirectoryHandle }
  /** A `.scaena` zip's bytes and its name, without `.scaena`: copied into the browser's
   * storage, and kept there. */
  | { zip: ArrayBuffer; name: string }
  /** A bundle's files by their paths inside it, and its name: a single-file export's, which
   * carries them (PLAN 2.5). Kept nowhere. It plays `states`, in that order, or every state
   * without them. */
  | { files: Record<string, ArrayBuffer>; name: string; states?: string[] }
  /** A new deck (PLAN 2.12), as `deck_create` makes one: one of the themes that ship
   * (`themes.ts`, by its name), the fonts it names, and one state with nothing on it, titled
   * `title`. Kept nowhere until it is saved. */
  | { create: { theme: string; title: string } };

/** Where Save as puts a bundle (PLAN 2.12): a folder on disk the page was given, or the
 * browser's storage under a name (`name-2`, … where that is taken). */
export type SaveTo = { folder: FileSystemDirectoryHandle } | { opfs: string };

/** What the editor exports (PLAN 2.54, SPEC §10), each as `scaena export` writes it:
 * - `png`: `state` at rest in `format` (the deck's canvas without one), `width` pixels wide, the
 *   height keeping the canvas's aspect, painted by the CPU painter;
 * - `pdf`: the deck, each slide at its last state, drawn by the PDF's own module, which the
 *   worker loads the first time a PDF is asked for;
 * - `html`: the deck as one file that plays offline, named `name`: `page`, the single-file
 *   player's page, filled in with the bundle, its fonts subset. */
export type Export =
  | { kind: "png"; state: string; width: number; format?: string }
  | { kind: "pdf" }
  | { kind: "html"; page: string; name: string };

/** Where an open bundle is kept, and saves to: a folder on disk, the browser's storage, or the
 * folder `scaena serve` serves it from (PLAN 2.11), by name. A bundle read from any other URL is
 * kept nowhere until it is saved. */
export interface Where {
  kind: "folder" | "opfs" | "serve";
  name: string;
}

/** Who paints: WebGPU where the browser has an adapter, else the CPU painter (`auto`), or
 * one of them whatever the browser has (`gpu`, `cpu`). */
export type Painter = "auto" | "gpu" | "cpu";

/** A state's place on the deck's timeline, ms (SPEC §2.4): where its cue starts, how long
 * its transition and motions run, and how long it then holds before the next state. */
export interface Slot {
  state: string;
  /** The slide it builds on: its first state's id (PLAN 2.97). */
  slide?: string;
  start: number;
  span: number;
  hold: number;
}

/** A box, canvas units: `[x, y, width, height]`. */
export type Rect = [number, number, number, number];

/** A visible node's place in a state at rest (ADR-0013): its box, what it sits in, and whether it
 * draws anything (a container with no panel, and a group, only hold others). */
export interface NodeBox {
  node: string;
  rect: Rect;
  parent: string | null;
  draws: boolean;
  /** Where its `transform` and those of what holds it draw `rect` (SPEC §3.3, PLAN 2.51): the
   * map `[a, b, c, d, e, f]` (`x' = a·x + c·y + e`), where something turns, scales, leans, or
   * moves it. */
  transform?: Map6;
  /** Locked (PLAN 2.95): the node whose lock holds it, itself or what holds it. The canvas passes
   * over it, and moves, resizes, turns, and deletes nothing of it. */
  locked?: string;
}

/** A map of canvas points, `[a, b, c, d, e, f]`: `x' = a·x + c·y + e`, `y' = b·x + d·y + f`. */
export type Map6 = [number, number, number, number, number, number];

/** What is sought across the deck's texts (PLAN 2.47). */
export interface Query {
  find: string;
  /** Upper and lower case apart. */
  case?: boolean;
  /** Whole words only. */
  words?: boolean;
}

/** A text the deck shows that a query matches, once for each place it is written (PLAN 2.47):
 * the node's own, a state's delta, or the deck's overrides (`lives`, a JSON pointer), with the
 * states that show it from there. `matches` are `[from, to]` in characters (Unicode scalar
 * values), as `replace_text` counts them. */
export interface Found {
  /** What the words are (PLAN 2.83): a text, a node's description, notes, or a beat's claim. */
  kind: "text" | "alt" | "notes" | "claim";
  /** The text's node, or the node the description describes; none for notes or a claim. */
  node?: string;
  /** The beat, for its claim or its notes. */
  beat?: string;
  /** The first state that shows it, where a replacement is made. */
  state: string;
  states: string[];
  lives: string;
  text: string;
  matches: [number, number][];
}

/** A node that draws at a point, with the containers and groups it sits in, innermost first. */
export interface Hit {
  node: string;
  rect: Rect;
  containers: string[];
  /** Locked (PLAN 2.95): the node whose lock holds it, itself or what holds it. A pointer passes
   * over it. */
  locked?: string;
}

/** What a row of a data source draws in a state at rest (PLAN 2.64): a chart's mark, or a table's
 * row, with the rows of its source it was made from. */
export interface DataMark {
  /** The chart or table that draws it. */
  node: string;
  /** The source it reads, as the deck names it. */
  source: string;
  /** The mark's key, or the table row's. */
  key: string;
  /** The rows of the source it draws, from 0, as the source's sheet numbers them. */
  rows: number[];
  /** Its outline as laid out, canvas units: SVG path data. */
  outline: string;
  /** The box around it as laid out, canvas units. */
  rect: Rect;
  /** Where the node's transform, and those of what holds it, draw it from where it is laid out. */
  transform?: [number, number, number, number, number, number];
  /** A chart's mark's annotations (PLAN 2.67); none for a table's row. */
  notes?: MarkNotes;
}

/** A value an annotation names: a number, or text (a category, a series, a date in ISO 8601). */
export type Scalar = number | string;

/** Where a chart's annotation stands (SPEC §3.7): a category or an x, a value, a series, each one
 * or, for a band or a highlight, several. */
export interface AnnotationAt {
  x?: Scalar | Scalar[];
  y?: number | number[];
  series?: Scalar | Scalar[];
}

/** What a chart's mark is to the chart's annotations (PLAN 2.67). */
export interface MarkNotes {
  /** Its x as an annotation names it. */
  x: Scalar;
  /** Its value: where a rule at it stands. */
  value: number;
  series: string | null;
  /** Whether the chart has axes for a callout, a rule, or a band: a donut takes highlights alone. */
  axes: boolean;
  /** Where a callout on it stands. */
  callout: AnnotationAt;
  /** What a highlight of it picks out. */
  highlight: AnnotationAt;
  /** The chart's highlights that pick it out, by their places among its `annotations`. */
  highlighted: number[];
}

/** One of a chart's annotations as drawn in a state at rest (PLAN 2.67): what the canvas selects,
 * moves, and takes away. A highlight draws nothing of its own. */
export interface NoteMark {
  /** The chart. */
  node: string;
  /** Its place among the chart's `annotations`. */
  index: number;
  kind: "callout" | "rule" | "band";
  /** What it says, as written. */
  text: string | null;
  /** Its band, its rule or leader, and its text's box, as laid out: SVG path data, canvas units. */
  outline: string;
  rect: Rect;
  transform?: [number, number, number, number, number, number];
}

/** An image as a pointer crops it (PLAN 2.74): where its whole is drawn, at the scale its crop and
 * fit draw it, and the part that shows, canvas units as laid out (through `transform` where it is
 * drawn); its crop, fractions of the image; its focal point, fractions of the crop. */
export interface Framing {
  node: string;
  rect: [number, number, number, number];
  transform?: [number, number, number, number, number, number];
  whole: [number, number, number, number];
  shown: [number, number, number, number];
  crop: [number, number, number, number];
  focal: [number, number];
  fit: "cover" | "contain" | "fill";
  size: [number, number];
}

/** A shape's outline as a pointer edits it (PLAN 2.68): its kind, its box at rest, its points as
 * fractions of the box, and a rect's corner radius with the theme's radius steps, canvas units. */
export interface Outline {
  node: string;
  kind: "rect" | "ellipse" | "line" | "arrow" | "polygon" | "path";
  rect: Rect;
  transform?: Map6;
  /** A line's, an arrow's, or a polygon's points as it draws them: a line or an arrow that gives
   * none draws two across its box's middle. */
  points: [number, number][];
  /** The fewest points its kind takes: two for a line or an arrow, three for a polygon. */
  fewest: number;
  /** A rect's corner radius as it draws: at most half its shorter side. */
  radius?: number;
  /** The theme's radius steps, `radius.0` on, each as this rect would draw it. */
  radii: number[];
}

/** Where a caret stands in a text at rest (ADR-0013, PLAN 2.32): each character as written, as a
 * reader counts it, on its line, from the glyphs the engine set. Offsets are UTF-16 code units of
 * `text`, as the page counts a string; x and y are canvas units. */
export interface Carets {
  /** The text as written: the node's `text`, or its runs' texts end to end. */
  text: string;
  lines: CaretLine[];
  /** Its paragraphs as a list's items (ADR-0018, PLAN 2.69), by paragraph; none past the last. */
  items?: (ListMark | null)[];
}

/** Where a link goes (PLAN 2.70): a web address, or a state of the deck. */
export type LinkTarget = { href: string } | { state: string };

/** A paragraph as a list's item (ADR-0018): its kind, its level, 0 the outermost, and its marker. */
export interface ListMark {
  kind: "bullet" | "number";
  level: number;
  marker: string;
}

export interface CaretLine {
  /** Its line box's top and bottom. */
  top: number;
  bottom: number;
  /** Where a caret on it stands when it holds no character. */
  x: number;
  /** Where it starts in the text, and where the next line starts. */
  start: number;
  end: number;
  /** It ends at a line break the text sets: a caret after it stands on the next line. */
  broken: boolean;
  /** Its characters in the text's order: `[offset, lead, trail]`, where a caret before it stands
   * and one after it, the right edge first in right-to-left text. */
  chars: [number, number, number][];
}

/** What an inspector offers for a node as a state shows it (ADR-0013, PLAN 2.33), as `scaena
 * inspect --choices` says it. */
export interface Choices {
  node: string;
  type: string;
  state: string;
  /** Each property the inspector edits: the node type's own, then those every node has. */
  fields: Field[];
}

/** A node's look as a state shows it (PLAN 2.58), as `scaena inspect --look` says it: each
 * property of its type's look (a text's role and style, a shape's fill, stroke, and corners, …)
 * with the value shown, absent where the theme's shows. What ⌥⌘C picks up. */
export interface Look {
  node: string;
  type: string;
  props: { prop: string; value?: unknown }[];
}

/** A look put down on nodes (PLAN 2.58), as `scaena inspect --look --onto` says it: one patch of
 * `choose`s, each written where that node's own value lives; the nodes it changes, those that
 * look so already, and those that take none of it, with why. */
export interface Put {
  patch: unknown[];
  took: string[];
  same: string[];
  refused: { node: string; why: string }[];
}

/** What an inspector offers for a state itself (PLAN 2.36), as `scaena inspect --state-choices`
 * says it: its layout, each key of its transition, its hold, and its notes. */
export interface StateChoices {
  state: string;
  /** What `set_state` names. A state that sets no key of its transition cuts in. */
  fields: Field[];
}

/** A layout the state shown may take (PLAN 2.92), as `scaena inspect --layouts` judges it: what
 * lint finds in the state laid out in it, the states the change reaches, and the `set_state` that
 * makes it (none for the layout it takes now), with its picture at rest, `width` × `height` pixels
 * of straight-alpha RGBA. */
/** A slide a person may start (PLAN 3.30): one in a layout of the theme, a text in each slot that
 * says what goes there, or a blank one; and its picture. */
export interface Starter {
  /** The theme's layout it is in; none for the blank slide. */
  layout: string | null;
  /** What the theme says the layout is for. */
  description: string | null;
  /** The gallery's section it is offered under; none for the blank slide. */
  group: string | null;
  width: number;
  height: number;
  pixels: ArrayBuffer;
}

/** A slot of the state's layout with nothing placed in it, which waits for what its prompt says
 * goes there (PLAN 3.30): the canvas outlines it with the prompt's words. */
export interface Waiting {
  slot: string;
  /** Its box in the format shown, canvas units. */
  rect: [number, number, number, number];
  /** What goes there, as the prompt says it. */
  words: string;
  /** Whether words go there; else a picture or a figure. */
  typed: boolean;
  /** The text role words there are set in, which a press on its words inserts a text of. */
  role?: string;
}

export interface LayoutSuggestion {
  layout: string;
  current?: boolean;
  errors: number;
  warnings: number;
  /** The layouts that draw the state as this one does, lint judging them alike: left out. */
  alike?: string[];
  reach: string[];
  patch: unknown[];
  width: number;
  height: number;
  pixels: ArrayBuffer;
}

/** One property an inspector edits. */
export interface Field {
  /** A property, or one key of an object property (`style/color`): what `choose` names. */
  prop: string;
  takes: Takes;
  /** The value the state shows, as the deck sets it; absent where the theme's shows. */
  value?: unknown;
  /** Where that value lives: where a choice is written. */
  lives?: Lives;
  /** The value is written out where the theme has names: an override, or lint W300's. */
  literal?: boolean;
}

/** What a property takes: one of the theme's names of a kind (with `overrides`, or a value written
 * out, which is an override), one of some words, a number in range, or yes or no. */
export type Takes =
  | { kind: "name"; of: string; names: string[]; overrides?: boolean }
  | { kind: "word"; words: string[] }
  | { kind: "number"; min?: number; above?: number; max?: number; whole?: boolean; overrides?: boolean }
  | { kind: "flag" }
  | { kind: "text" }
  /** Fractions of an image, 0 to 1, one for each name: a point's `x` and `y`, a part's `x`, `y`,
   * `w`, and `h` (PLAN 2.45). */
  | { kind: "fractions"; names: string[] };

/** Where a value a state shows lives: the deck's overrides, a state's delta, or the node. */
export type Lives = "overrides" | "node" | { state: string };

/** How a box dropped by a drag snaps (`scaena inspect --snap`): moved as many cells as it spans,
 * resized to the nearest tracks, into the slot or area it covers most, where it was dropped as a
 * `rect`, or among a stack's children. */
export type SnapMode = "move" | "resize" | "slot" | "free" | "order";

/** Where a node may go in a state at rest (ADR-0013, `scaena inspect --targets`). */
export interface Targets {
  /** What holds it: the theme's `grid` (by cells, a slot, or a `rect`), a `stack` (by order), a
   * grid container's `cells` (by its cells or areas), or a `frame` (by a `rect`). */
  by: "grid" | "stack" | "cells" | "frame";
  parent?: string;
  /** The box its placement names now, before its inset, offset, alignment, and size: what a
   * drag moves. */
  cell: Rect;
  /** The tracks a placement by cells takes, each `[start, end]`. */
  columns?: [number, number][];
  rows?: [number, number][];
  /** The boxes a placement by name takes: the template's slots, `canvas`, and `grid`; or a grid
   * container's areas. */
  slots?: Record<string, Rect>;
  /** A stack's children in their order, this node among them. */
  flow?: string[];
  /** What a `rect` is measured in. */
  within: Rect;
  snaps: SnapMode[];
}

/** Where a dropped box lands: the box a guide shows, and the patch that puts the node there,
 * empty where it is already. */
export interface Snapped {
  cell: Rect;
  patch: unknown[];
  /** With several nodes moved together (PLAN 2.42): where each lands. */
  landed?: Landed[];
  /** Where the box's edges, or its middle, meet another box's or the canvas's (PLAN 2.57). */
  guides?: Line[];
}

/** A guide: `[x1, y1, x2, y2]`, canvas units, a line down or across the canvas. */
export type Line = [number, number, number, number];

/** The theme's grid in the format shown (PLAN 2.57), canvas units: its columns and rows, each
 * `[start, end]`, the gutters between them and the margins around them; and the baseline grid's
 * lines, each a `y`. */
/** A slot of a theme's layout as the canvas shows it (PLAN 2.71): its name, its box in the format
 * shown, canvas units, its cells as the theme writes them, and whether that format writes it. */
export interface SlotBox {
  name: string;
  rect: Rect;
  col?: number | [number, number];
  row?: number | [number, number];
  own: boolean;
}

/** The layout a state uses and its slots (PLAN 2.71). */
export interface LayoutSlots {
  layout: string;
  slots: SlotBox[];
}

export interface Grid {
  canvas: [number, number];
  columns: [number, number][];
  rows: [number, number][];
  baselines: number[];
  /** What lies inside the safe area's strip, 3/8 inch in from each edge (PLAN 3.29). */
  safe: Rect;
}

/** Where a node arranged with others lands (PLAN 2.42). */
export interface Landed {
  node: string;
  cell: Rect;
}

/** How several nodes are arranged (PLAN 2.42): one of these. */
export type Arrange =
  | { align: "left" | "center" | "right" | "top" | "middle" | "bottom" }
  | { spread: "across" | "down" }
  | { order: "forward" | "backward" | "front" | "back" }
  /** One node, listed just before or just after another (PLAN 2.50): into what holds that one,
   * if it is another container; or into a container, listed first there. */
  | { before: string }
  | { after: string }
  | { into: string }
  | { by: [number, number]; free?: boolean };

/** Several nodes arranged: where each lands, and the patch that puts them there. */
export interface Arranged {
  landed: Landed[];
  patch: unknown[];
}

/** A node of a state's layers (PLAN 2.50), as `scaena inspect --layers` says it: each list the
 * topmost first (a stack's in the order it lays them out), with what it holds. */
export interface Layer {
  node: string;
  type: string;
  /** Whether the state shows it: one it does not leaves in it, another state of its slide shows
   * it, or no state does. */
  shown: boolean;
  /** Locked (PLAN 2.95): the canvas passes over it, and the layers select it. */
  locked?: boolean;
  children?: Layer[];
}

/** Something the editor may insert (PLAN 2.34), as `scaena inspect --inserts` says it: a node the
 * theme or the bundle names, as `add_node` adds it, unplaced. */
export interface Insert {
  /** What a menu says: the node's type, and the name it is made from. */
  label: string;
  node: { type: string } & Record<string, unknown>;
  /** What its id starts from. */
  id: string;
  /** The box it takes at first: about the pointer, each side a share of the canvas's; or a slot
   * it fills, under what is there. */
  start: { box: { w: number; h: number } } | { slot: string };
}

/** A state at rest, painted small for the state strip (PLAN 2.35): what identifies its drawing,
 * and, where that is not what the strip holds, its pixels (straight-alpha RGBA, row by row). */
export interface Thumb {
  state: string;
  digest: string;
  width?: number;
  height?: number;
  pixels?: ArrayBuffer;
}

/** A node a patch adds: its id, where it lands, and the patch. */
export interface Added {
  id: string;
  cell: Rect;
  patch: unknown[];
}

/** What a paste makes (PLAN 2.37): the copy of the node copied, where it lands, and the patch;
 * the copies of the others copied with it (PLAN 2.42); the files the clip carried that the
 * bundle lacked, now in it; and what the copies named that the theme lacks, each taken out of
 * them. */
export interface Pasted extends Added {
  also?: string[];
  files: string[];
  findings: Finding[];
}

/** What grouping makes (PLAN 2.43): the new group's id, and the patch that makes it. */
export interface Grouped {
  id: string;
  patch: unknown[];
}

/** The media type a clip goes on the clipboard as, beside its text (PLAN 2.37). */
export const CLIP = "application/x-scaena+json";
/** What a drag from the Files panel carries (PLAN 2.59): an image's path in the bundle. */
export const BUNDLE_PATH = "application/x-scaena-path";
/** A file an image node can show, by its name: a PNG or a JPEG (SPEC §3.3, PLAN 2.66). */
export const PICTURE = /\.(png|jpe?g)$/i;

/** The files a data source reads (SPEC §3.10): a CSV, or JSON, an array of objects. */
export const DATA = /\.(csv|json)$/i;

/** Cells pasted from a sheet, as a data source (PLAN 2.96, `scaena_core::data::cells`). */
export interface Cells {
  /** A name for the source and its file, from the first columns' names. */
  name: string;
  columns: string[];
  /** Each column's type, as the source declares it. */
  schema: Record<string, string>;
  /** Each column's format, which prints its numbers as they were copied; none for text. */
  formats: (string | null)[];
  rows: number;
  /** The source's file, its numbers written plainly. */
  csv: string;
}

/** A data file attached as a source, as `data_attach` says it (`scaena_ops::create::Attached`). */
export interface Attached {
  /** Whether the deck takes it: not where it would make the deck invalid (in `added`). */
  attached: boolean;
  id: string;
  /** Its path in the bundle. */
  source: string;
  /** Each column's type, as the deck declares it. */
  schema: Record<string, string>;
  rows: number;
  added: Finding[];
  removed: Finding[];
  errors: number;
}

/** A version of the deck (PLAN 2.60, SPEC §8): as it was just after one change its history
 * keeps, numbered from the oldest, and named by its change's id for as long as the history lasts. */
export interface Version {
  n: number;
  id: string;
  author?: string | null;
  message?: string | null;
  /** When the change was made, in ISO 8601, UTC. */
  at?: string | null;
  ops: number;
}

/** How a node changes from one version to another, as `deck_diff` says it: a prop gone is null. */
export type NodeChange = { enter: Record<string, unknown> } | { exit: true } | { change: Record<string, unknown> };

/** How a state changes from one version to another. */
export type StateChange =
  | { added: true }
  | { removed: true }
  | { changed: { fields?: Record<string, unknown>; nodes?: Record<string, NodeChange> } };

/** What changed from one version to another, as `scaena history --diff` says it. */
export interface Compared {
  states: Record<string, StateChange>;
  /** The deck's own fields that changed, each as the later version has it. */
  deck: Record<string, unknown>;
  /** The data files whose bytes changed. */
  files: string[];
}

/** What restoring a version did, as `scaena history --restore` says it. */
export interface Restored {
  version: Version;
  applied: boolean;
  files: string[];
  added: Finding[];
  removed: Finding[];
  errors: number;
  states: string[];
}

/** A file an edit wrote beside the deck, a data file a restore wrote or the theme a theme edit
 * did: its text before and after, null where the bundle did not hold it. */
export interface Rewritten {
  path: string;
  before: string | null;
  after: string | null;
}

/** One of a bundle's images, fonts, or data files (PLAN 2.59), as `scaena files` lists it: what in
 * the deck or its theme names it (none where nothing does: it may be taken out), and the nodes
 * drawn from it, each with the states that show it so. */
export interface BundleFile {
  path: string;
  type: "image" | "font" | "data";
  bytes: number;
  named: Named[];
  used: { node: string; states: string[] }[];
}

/** What names a file of the bundle. */
export type Named =
  | { by: "node"; node: string }
  | { by: "evidence"; beat: string }
  | { by: "font"; family: string; style?: string }
  | { by: "theme"; family: string }
  | { by: "source"; source: string };

/** The page to the worker. */
export type ToWorker =
  /** Open the bundle at `source`, and paint into `canvas` by `painter`, with the engine's
   * module `engine`, compiled, if the page carries it (PLAN 2.5); the worker loads its own
   * without it. */
  | { type: "open"; source: Source; painter: Painter; canvas: OffscreenCanvas; engine?: WebAssembly.Module }
  /** Paint `state` `t` ms into its cue, or at rest without `t`. */
  | { type: "show"; id: number; state: string; t?: number; format?: string }
  /** The deck's timeline. */
  | { type: "timeline"; id: number; format?: string }
  /** How `state` reads at rest, as HTML (SPEC §3.12; PLAN 2.8). */
  | { type: "read"; id: number; state: string; format?: string }
  /** The link drawn at `at`, fractions of the canvas, in `state` at rest (PLAN 2.70). */
  | { type: "linkAt"; id: number; state: string; at: [number, number]; format?: string }
  /** Play the deck from slot `index`, `t` ms into its cue, a frame each time the display
   * takes one: each cue, then its state's hold, then the next state's cue. A state that does
   * not hold, and the last, comes to rest and waits there. `still`, for a reader who asks for
   * less motion (PLAN 2.8): each cue is a cut to its state at rest, and the deck keeps its
   * pace, a state that holds going on when its cue and hold are over. */
  | { type: "run"; index: number; t: number; format?: string; still?: boolean; alone?: boolean }
  /** Slot `index`, `t` ms into its cue (at rest without `t`), still. */
  | { type: "seek"; id: number; index: number; t?: number; format?: string }
  /** Stop where the deck is. */
  | { type: "pause" }
  /** Read the bundle again from its URL, keeping the canvas and who paints: `scaena serve` said
   * it changed on disk (PLAN 2.11). */
  | { type: "reload"; id: number }
  /** The deck as canonical `.scn` (SPEC §4): what the editor opens on. */
  | { type: "source"; id: number }
  /** Compile `source`. A deck that validates is shown from now on: slot `index` repaints at
   * rest, then lint runs over it, laying out that slot's state alone (PLAN 2.3). */
  | { type: "edit"; id: number; source: string; index: number; format?: string }
  /** Lint the deck compiled last, laying out every state: what an edit leaves for when
   * typing stops. */
  | { type: "lint"; id: number }
  /** The source compiled last with `patch`, a finding's `fix`, applied. */
  | { type: "fix"; id: number; patch: unknown[] }
  /** `state` inspected, in `format` or on the deck's own canvas. */
  | { type: "inspect"; id: number; state: string; format?: string }
  /** What stands where in `state` at rest (ADR-0013): each visible node's box, canvas units. */
  | { type: "boxes"; id: number; state: string; format?: string }
  /** The nodes that draw at `point` in `state` at rest, topmost first. */
  | { type: "hit"; id: number; state: string; point: [number, number]; format?: string }
  /** The chart mark or table row at `point` in `state` at rest (PLAN 2.64). */
  | { type: "markAt"; id: number; state: string; point: [number, number]; format?: string }
  /** What `rows` of data source `source` draw in `state` at rest (PLAN 2.64). */
  | { type: "marksOf"; id: number; state: string; source: string; rows: number[]; format?: string }
  /** The chart annotation drawn at `point` in `state` at rest (PLAN 2.67). */
  | { type: "noteAt"; id: number; state: string; point: [number, number]; format?: string }
  /** The layout `state` uses and its slots, in the format shown (PLAN 2.71). */
  | { type: "layout"; id: number; state: string; format?: string }
  /** Chart `node`'s marks and annotations in `state` at rest (PLAN 2.75). */
  | { type: "marksIn"; id: number; state: string; node: string; format?: string }
  /** Image `node`'s framing in `state` at rest (PLAN 2.74). */
  | { type: "framing"; id: number; state: string; node: string; format?: string }
  /** Shape `node`'s outline in `state` at rest (PLAN 2.68). */
  | { type: "outline"; id: number; state: string; node: string; format?: string }
  /** Where a callout of chart `node` dropped at `point` in `state` at rest would stand (PLAN 2.67). */
  | { type: "calloutAt"; id: number; state: string; node: string; point: [number, number]; format?: string }
  /** Paint the preview through `view`, `[x, y, w, h]` canvas units, at the size shown, or the whole
   * canvas with none; and paint what is shown again so (PLAN 2.46). */
  | { type: "view"; id: number; view: [number, number, number, number] | null }
  /** Each text of the deck `source` compiles to that `query` matches (PLAN 2.47). */
  | { type: "find"; id: number; source: string; query: Query }
  /** The patch that replaces what `query` matches with `with`: every match, or with `one`,
   * `[text, match]` into what `find` gives, that one (PLAN 2.47). */
  | { type: "replacing"; id: number; source: string; query: Query; with: string; one?: [number, number] }
  /** The point of image `node` under `point` in `state` at rest, in fractions of its crop:
   * what a focal point picked there is (PLAN 2.45). */
  | { type: "focalAt"; id: number; source: string; state: string; node: string; point: [number, number]; format?: string }
  /** Where `node` may go in `state` at rest. */
  | { type: "targets"; id: number; state: string; node: string; format?: string }
  /** The theme's grid in `format` (PLAN 2.57). */
  | { type: "grid"; id: number; format?: string }
  /** A drag's move (ADR-0013). With `by`, `state` painted at rest with `node`, and what it holds,
   * that far from where it stands, laying nothing out. With `snap`, where its cell would land,
   * `snap.to` snapped `snap.how`, and the states the patch changes; `fork` keeps the patch to
   * `state`. With `preview`, `state` painted as that patch would make it, laid out once: a resize
   * that pauses. */
  | {
      type: "drag";
      id: number;
      state: string;
      node: string;
      /** The nodes moved with it, children of what holds it (PLAN 2.42). */
      with?: string[];
      by?: [number, number];
      /** `reach`: off the grid, how near, canvas units, an edge or the middle goes onto
       * another's (PLAN 2.57). */
      snap?: { how: SnapMode; to: Rect; fork: boolean; reach?: number };
      /** With `with`: where they all land, moved `by` together, `free` off the grid. */
      together?: { by: [number, number]; free: boolean; fork: boolean; reach?: number };
      preview?: boolean;
      format?: string;
    }
  /** `nodes`, children of one container, arranged in `state` at rest (PLAN 2.42): where each
   * lands and the patch that puts them there, kept to the state to `fork` it. */
  | { type: "arrange"; id: number; state: string; nodes: string[]; how: Arrange; fork: boolean; format?: string }
  /** A drag is over, and changes nothing: `state` painted at rest as it stands. */
  | { type: "rest"; id: number; state: string; format?: string }
  /** Make `ops`, the patch a gesture on the canvas or a choice in the inspector ends in, by the
   * user, on the deck the editor's `source` compiles to (ADR-0013); then the deck's source,
   * compiled, shown at slot `index`, and linted, as an edit of it is. */
  | { type: "make"; id: number; source: string; ops: unknown[]; index: number; format?: string }
  /** What an inspector offers for `node` as `state` shows it (PLAN 2.33). */
  | { type: "choices"; id: number; state: string; node: string }
  | { type: "stateChoices"; id: number; state: string }
  /** The layouts `state` may take, best first, each painted at rest `height` pixels high, in the
   * deck the editor's `source` compiles to (PLAN 2.92). */
  | { type: "layoutSuggestions"; id: number; state: string; height: number; format?: string }
  /** `state` painted at rest as the patch `ops` would make it, nothing made, until a `rest` lets
   * it go: a suggested layout pointed at (PLAN 2.92). */
  | { type: "preview"; id: number; state: string; ops: unknown[]; format?: string }
  /** `node`'s look as `state` shows it, in the deck the editor's `source` compiles to (PLAN 2.58). */
  | { type: "look"; id: number; source: string; state: string; node: string }
  /** The patch that puts `look` on `nodes` in `state`, on the deck the editor's `source` compiles
   * to (PLAN 2.58). */
  | { type: "putting"; id: number; source: string; state: string; look: Look; nodes: string[] }
  /** Where a caret stands in `node`'s text in `state` at rest (PLAN 2.32), in the deck the
   * editor's `source` compiles to: compiled first, if the deck shown is not. */
  | { type: "carets"; id: number; source: string; state: string; node: string; format?: string }
  /** What an inspector offers for the characters `from` to `to` (Unicode scalar values) of
   * `node`'s text as `state` shows it (PLAN 2.38): the looks a run takes, which `style_text`
   * sets, with the first character's. */
  | { type: "characterChoices"; id: number; state: string; node: string; from: number; to: number }
  /** What ⌘B gives those characters, from the weight the engine sets each in, in the deck the
   * editor's `source` compiles to, laid out in `format` (PLAN 2.38): `style_text`'s `look`. */
  | { type: "bolding"; id: number; source: string; state: string; node: string; from: number; to: number; format?: string }
  | { type: "italicizing"; id: number; source: string; state: string; node: string; from: number; to: number; format?: string }
  /** The theme the deck names, and the theme files the bundle holds (PLAN 2.39). */
  | { type: "themes"; id: number }
  /** The deck the editor's `source` compiles to, in another theme (PLAN 2.39): one that ships, by
   * its name, or one the bundle holds, by its path. Then, unless it is refused, the deck's source,
   * compiled, shown at slot `index`, and linted, as an edit of it is. */
  | { type: "retheme"; id: number; source: string; theme: { ships: string } | { path: string }; index: number; format?: string }
  /** The states `ops` (a patch) would change, with nothing made (ADR-0013). */
  | { type: "reach"; id: number; ops: unknown[] }
  /** What may be inserted in the deck (PLAN 2.34). */
  | { type: "inserts"; id: number }
  | { type: "layers"; id: number; state: string }
  /** A patch to add or take away a node, with nothing made, on the deck the editor's `source`
   * compiles to (PLAN 2.34): what `inserts` offers `n`th, entering in `state` about `at` (canvas
   * units); a copy of `node` beside it in `state`; or `node` taken out of `state` and the states
   * after it, with what it holds, or, `everywhere`, out of the deck. */
  | { type: "inserting"; id: number; source: string; state: string; n: number; at: [number, number]; format?: string; named?: string; with?: Record<string, unknown> }
  /** What `inserts` offers `n`th, drawn in the box a drag from `from` to `to` covers (canvas
   * units), snapped to the theme's grid as a resize snaps, or, `free`, where it was drawn (PLAN
   * 2.48). */
  | { type: "drawing"; id: number; source: string; state: string; n: number; from: [number, number]; to: [number, number]; free: boolean; format?: string }
  | { type: "duplicating"; id: number; source: string; state: string; node: string; format?: string }
  | { type: "deleting"; id: number; source: string; state: string; node: string; everywhere: boolean }
  /** The patch that puts `nodes`, children of one container as `state` shows them, in a new group
   * where they stand, on the deck the editor's `source` compiles to (PLAN 2.43). */
  | { type: "grouping"; id: number; source: string; state: string; nodes: string[] }
  /** What the clipboard holds of `nodes` as `state` shows them, on the deck the editor's `source`
   * compiles to (PLAN 2.37, 2.42): the clip, as JSON text. The first is the node copied. */
  | { type: "copying"; id: number; source: string; state: string; nodes: string[]; format?: string }
  /** The patch that pastes `clip`, the clipboard's text, entering in `state` about `at` (canvas
   * units), on the deck the editor's `source` compiles to: a clip pastes what it holds, other
   * text a text in the theme's body role (PLAN 2.37). */
  | { type: "pasting"; id: number; source: string; state: string; clip: string; at: [number, number]; format?: string }
  /** Each state at rest in `format`, painted by the CPU painter `height` pixels high, for the
   * state strip (PLAN 2.35): only those whose drawing is not the one `known` holds (each state's
   * digest, as the strip last had it) come with pixels. */
  | { type: "thumbnails"; id: number; height: number; known: Record<string, string>; format?: string; states?: string[] }
  /** The patch that adds a state after `state`, on the deck the editor's `source` compiles to: a
   * `step` of its slide, or a `slide` of its own (PLAN 2.35). */
  | { type: "addingState"; id: number; source: string; state: string; what: "step" | "slide" }
  /** The slides a person may start after `state`'s slide, on the deck the editor's `source`
   * compiles to (PLAN 3.30): one in each of the theme's layouts, then a blank one, each painted at
   * rest `height` pixels high in `format`. */
  | { type: "starters"; id: number; source: string; state: string; height: number; format?: string }
  /** The patch that starts a slide in `layout`, or a blank one with none, after `state`'s slide. */
  | { type: "starting"; id: number; source: string; state: string; layout?: string }
  /** The slots of `state`'s layout, in `format`, that wait for what their prompts say goes there. */
  | { type: "waiting"; id: number; state: string; format?: string }
  /** The patch that fills `slot` of `state`'s layout, which waits for words, as a new slide in the
   * layout fills it (PLAN 3.30). */
  | { type: "filling"; id: number; source: string; state: string; slot: string }
  /** Text typed on the canvas (PLAN 2.32): `ops`, a `replace_text`, made by the user on the deck
   * the editor's `source` compiles to, validated but not linted; then the deck's source, shown
   * at slot `index` and linted as an edit of it is, and where a caret stands in `node`'s text in
   * `state` now. */
  | { type: "type"; id: number; source: string; ops: unknown[]; index: number; state: string; node: string; format?: string }
  /** Save the bundle with the deck `source` compiles to as `scaena save` does (SPEC §3.1),
   * fonts kept whole, where it is kept; one kept nowhere goes into the browser's storage under
   * its name (`name-2`, … where that is taken). A source that does not compile, or a deck
   * that does not validate, is not saved. The session goes on from the save (PLAN 2.4). With
   * `keep`, a bundle that keeps no history begins one with this save (PLAN 2.87). */
  | { type: "save"; id: number; source: string; keep?: boolean }
  /** Save as (PLAN 2.12): the bundle saved as `save` saves it, but `to` a place of its own,
   * where it is kept from then on. */
  | { type: "saveAs"; id: number; source: string; to: SaveTo }
  /** The bundle with the deck `source` compiles to, saved as a `.scaena` zip, fonts subset to
   * what the deck draws. */
  | { type: "zip"; id: number; source: string }
  /** Export the deck `source` compiles to (PLAN 2.54), made here for the page to download:
   * nothing is sent anywhere. A source that does not compile, or a deck that does not validate,
   * is not exported. */
  | { type: "export"; id: number; source: string; as: Export }
  /** A file dropped on the page, into the bundle: where it goes is what it is, and an image
   * is named by its SHA-256 (`Player.place`). */
  | { type: "drop"; id: number; name: string; bytes: ArrayBuffer }
  /** A data file dropped on the canvas (PLAN 2.76), into the bundle under its own name, or a
   * number after it where other bytes have that name (`Player.placing`), as the source a chart
   * of it reads in the deck `source` compiles to: the one that reads it, or one declared as
   * `data_attach` declares it, with the patch that declares it. */
  | { type: "attaching"; id: number; source: string; name: string; bytes: ArrayBuffer; schema?: Record<string, string> }
  /** `text` pasted on the canvas, read as a sheet's cells in the deck's language (PLAN 2.96). */
  | { type: "cells"; id: number; source: string; text: string }
  /** The deck's data sources, and source `name` as a sheet (PLAN 2.55, `data_edit` with no
   * edits): the first source without it. The deck `source` compiles to names them; it need not
   * validate, since a cell its column does not read is what the sheet shows, to fix. */
  | { type: "sheet"; id: number; source: string; name?: string }
  /** Edits of data source `name`, by the user (PLAN 2.55, `data_edit`): all or none, one write of
   * its file or one patch of rows written inline, refused where a value does not read or the deck
   * would be invalid. The deck's source after is compiled, shown at slot `index`, and linted, as
   * an edit of it is. */
  | { type: "dataEdit"; id: number; source: string; name: string; edits: RowEdit[]; index: number; format?: string }
  /** The last edit of a data file undone, or with `redo` the last undone made again (PLAN 2.55):
   * the file as it was, written, then shown and linted as an edit is. */
  | { type: "dataUndo"; id: number; source: string; redo: boolean; index: number; format?: string }
  /** The bundle's images, fonts, and data, and what uses each, in the deck `source` compiles to
   * (PLAN 2.59). */
  | { type: "bundleFiles"; id: number; source: string }
  /** `path` taken out of the bundle, by the user: one nothing names (PLAN 2.59). */
  | { type: "removeFile"; id: number; source: string; path: string; index: number; format?: string }
  /** The bundle's versions (PLAN 2.60), read from its history by the history's own module: none
   * where it keeps no history. */
  | { type: "versions"; id: number }
  /** Version `version` (its id) shown read-only: its states, and `state` of it, or its first
   * where it has none such, at rest as a PNG `width` pixels wide. */
  | { type: "version"; id: number; version: string; state?: string; width: number }
  /** What changed from version `from` to version `to`, or without it to the deck `source`
   * compiles to, and its data files as they are now. */
  | { type: "compareVersions"; id: number; source: string; from: string; to?: string }
  /** `version` made the deck again, with its data files, as one change by the user; refused
   * where the deck would not validate in the bundle as it is. A source that does not compile
   * is no bar: restoring a version is a way back from one. */
  | { type: "restoreVersion"; id: number; source: string; version: Version; index: number; format?: string }
  /** The theme frames are drawn in, as its text (PLAN 2.61). */
  | { type: "themeText"; id: number }
  /** The theme the deck names edited by `ops`, RFC 6902 operations on it, as one change by the
   * user, the deck `source` compiles to drawn in it; refused where the deck would not validate in
   * it (ADR-0016). */
  | { type: "themeEdit"; id: number; source: string; ops: unknown[]; photo?: string; index: number; format?: string }
  /** Files written back, as an undo or a redo of a restore or a theme edit has them: `text` null
   * to take one out. With `edit`, the deck its source compiles to shown and linted again after,
   * as an edit is: the source did not change, so nothing else compiles it. */
  | {
      type: "writeFiles";
      id: number;
      files: { path: string; text: string | null }[];
      edit?: { source: string; index: number; format?: string };
    }
  /** Ask the assistant (PLAN 2.6): the editor's `source` must compile to a deck that
   * validates, which its tools then work on. Each step comes back as an `assistant` event,
   * until one that is `done` or `failed`. */
  | { type: "ask"; id: number; source: string; ask: Asking; drawn?: Drawn }
  /** Stop the assistant: the call it is in finishes, and it says no more. */
  | { type: "stop" }
  /** Start a new conversation with the assistant. */
  | { type: "forget" }
  /** The models `key` can use at `provider`. */
  | { type: "models"; id: number; provider: ProviderId; key: string; base?: string }
  /** The workers `helpers` asked for: a port to each (PLAN 2.28). */
  | { type: "helpers"; ports: MessagePort[] }
  /** Paint the state shown in each of `besides`' formats beside the canvas, on its own canvas,
   * `height` pixels high, after each frame of the canvas's, as it plays (PLAN 2.62): each a
   * format of the deck's, or its own canvas (`format` unset). None stops it. */
  | { type: "besides"; besides: { format?: string; canvas: OffscreenCanvas }[]; height: number }
  /** Be a helper: work out shaders' rows for the engine's worker at the other end of `port`,
   * which hands over the engine's module first (PLAN 2.28). A helper holds no deck. */
  | { type: "help"; port: MessagePort };

/** The engine's worker to a helper, over the port between them (PLAN 2.28). */
export type ToHelper =
  /** The engine's module, compiled: the first message. Without it, the helper loads its own. */
  | { type: "module"; module?: WebAssembly.Module }
  /** Rows `first` to `first + rows` of the shader whose spec is `spec` (`Player.shaderSpec`). */
  | { type: "rows"; id: number; spec: Uint8Array; first: number; rows: number };

/** A helper to the engine's worker. */
export type FromHelper =
  | { type: "ready" }
  /** The rows request `id` asked for: straight-alpha RGBA, four bytes a pixel, row by row. */
  | { type: "rows"; id: number; bytes: ArrayBuffer }
  /** Request `id` failed, or, without one, the helper could not start. */
  | { type: "error"; id?: number; message: string };

/** Who answers the assistant's questions: Anthropic, OpenAI (or a server that speaks its Chat
 * Completions), or Gemini, with the user's own key (SPEC §11). */
export type ProviderId = "anthropic" | "openai" | "gemini";

/** A question for the assistant, and who answers it: the provider, at its own address or at
 * `base`, with the user's key, and the model they picked; and what the editor shows as it is
 * asked (PLAN 2.52), which "this" and "shorter" mean. */
/** Each state's drawing as the page holds it, by the digest of its display list, and how many
 * pixels high its pictures are (PLAN 2.93): what an assistant's edit is drawn against. */
export interface Drawn {
  height: number;
  known: Record<string, string>;
}

export interface Asking {
  provider: ProviderId;
  model: string;
  key: string;
  base?: string;
  text: string;
  seeing?: Seeing;
}

/** What the editor shows as a question is asked (PLAN 2.52): the state shown, in a format if not
 * the deck's own, the nodes selected, the one selected first, and the characters selected in a
 * text typed in, from `from` to `to` in Unicode scalar values, as `replace_text` and `style_text`
 * count them, with the text they make. */
export interface Seeing {
  state: string;
  format?: string;
  nodes: { node: string; type?: string }[];
  characters?: { node: string; from: number; to: number; text: string };
}

/** What the page hears as the assistant works. */
export type AssistantEvent =
  /** The model said `text`. */
  | { kind: "text"; text: string }
  /** It calls tool `name`. */
  | { kind: "call"; id: string; name: string; args: unknown }
  /** Call `id` returned: `summary` says what it came to; `json` is what the model is told,
   * and `png` a frame it drew, in base64. */
  | { kind: "result"; id: string; name: string; error: boolean; summary: string; json: string; png?: string }
  /** The deck changed: its source now, which the editor takes, compiled, shown, and linted
   * as an edit of it would be: the worker does it as the change is made, so the editor asks
   * nothing of a source the assistant has moved past. `touched` are the nodes the question has
   * changed so far, in the deck's order: those whose own props, a state's delta for them, or
   * the deck's overrides of them differ from before it was asked, and those it added. */
  | {
      kind: "edited";
      source: string;
      edited: Edited;
      touched?: string[];
      files?: Rewritten[];
      /** Each state whose drawing the edit changed, at rest, as the page's pictures are drawn
       * (`Drawn`), and each it took away (PLAN 2.93). */
      drawn?: Thumb[];
      gone?: string[];
    }
  /** Tokens in and out of one answer, as the provider counts them. */
  | { kind: "usage"; input: number; output: number }
  /** It stopped: its answer is done (`end`), it ran out of room (`length`), it called tools
   * as many rounds as one question allows (`steps`), or the user stopped it (`stopped`). */
  | { kind: "done"; stop: "tools" | "end" | "length" | "other" | "steps" | "stopped" }
  /** It failed: the provider refused, or the network did. */
  | { kind: "failed"; message: string };

/** An open bundle, as the worker reports it. */
export interface Opened {
  /** The bundle's name: its folder's, or its deck file's without `.deck.json`. */
  name: string;
  /** Where it is kept, if anywhere. */
  where?: Where;
  states: string[];
  formats: string[];
  /** Each state's speaker notes: its own, else its beat's (SPEC §3.11), else empty. */
  notes: string[];
  /** The spine's sections in its order, each with its title (else its first beat's claim)
   * and its states, each with its beat's claim; then the states no beat names, in a section
   * with no title. Empty for a deck without a spine (SPEC §3.11–3.12). */
  outline: Section[];
  /** What paints: vello on WebGPU, or vello_cpu. */
  painter: "webgpu" | "cpu";
  /** The adapter WebGPU paints with, as far as the browser tells. */
  adapter: string;
}

/** The theme the deck names (a path in the bundle, `(inline)`, or `null`), and the theme files
 * the bundle holds (PLAN 2.39). */
export interface Themes {
  current: string | null;
  files: string[];
}

/** The theme frames are drawn in (PLAN 2.61): the file the deck names, or `(inline)`, and its
 * JSON as text. */
export interface ThemeText {
  theme: string;
  text: string;
}

/** What a theme edit did, as `scaena theme --edit` says it (PLAN 2.61, ADR-0016). */
export interface ThemeEdited {
  /** The theme edited: its path in the bundle, or `(inline)`. */
  theme: string;
  /** Where in the theme the edit wrote: each operation's path, once. */
  paths: string[];
  applied: boolean;
  /** Refused: the deck would not validate in the theme it leaves (in `added`). */
  refused: boolean;
  listed: string[];
  added: Finding[];
  removed: Finding[];
  errors: number;
  /** For an edit to a photo's colors (PLAN 2.94): what they are, and what each of the theme's took. */
  photo?: {
    image: string;
    read: { hues: { degrees: number; chroma: number; share: number }[]; cast: { degrees: number; chroma: number } };
    set: { path: string; was: unknown; now: unknown; reads?: [number, number] }[];
  };
}

/** What a re-theme did, as `scaena theme --apply` says it (PLAN 1.6, 2.39). */
export interface Themed {
  /** The theme's path in the bundle. */
  theme: string;
  was?: string | null;
  /** The deck names it now; a refused theme leaves the deck in its own. */
  applied: boolean;
  refused: boolean;
  mapped: string[];
  listed: string[];
  /** What validation and lint find in the theme that they did not before, and what they no
   * longer find. */
  added: Finding[];
  removed: Finding[];
  errors: number;
}

/** A section of the spine, as a reader goes through it (PLAN 2.8). */
export interface Section {
  title?: string;
  states: { state: string; claim?: string }[];
}

/** Where the deck is. A place on the timeline names it only where states take time: states
 * with no transition and no hold all stand at one instant. So it is a slot and a time in it. */
export interface At {
  /** The slot, by its index in the timeline. */
  index: number;
  /** ms into its cue: from its span on, at rest, and then in its hold. */
  t: number;
  /** ms from the deck's start: the slot's start plus `t`. */
  global: number;
  /** Whether the clock runs. */
  playing: boolean;
  /** While the clock runs, how this frame went: its paint, and the time since the run's frame
   * before it (none on a run's first frame), ms. What the player's frame meter reads. */
  frame?: { paint: number; interval?: number };
}

/** Where in the source something is: UTF-16 offsets, as a JavaScript string counts them,
 * and the 1-based line and column it starts at. */
export interface Place {
  from: number;
  to: number;
  line: number;
  col: number;
}

/** A finding (SPEC §7.4), where it is in the source, and whether it has a fix. */
export interface Finding {
  code: string;
  severity: "error" | "warning" | "info";
  message: string;
  hint?: string;
  /** A JSON pointer into the deck, or into `file`. */
  path?: string;
  /** The bundle file `path` points into, when it is not the deck: the theme. */
  file?: string;
  state?: string;
  node?: string;
  format?: string;
  /** The fix, as JSON Patch. */
  fix?: unknown[];
  at?: Place;
  fixable: boolean;
  /** Whether it holds in the format shown (PLAN 2.49): one lint found laying a format out holds
   * there, and the rest in every format. */
  shown: boolean;
  /** The formats it holds in, as the format menu names them: `""` for the deck's own canvas,
   * then each of the deck's `formats` (PLAN 2.62). */
  formats: string[];
}

/** A data source's rows as the Data panel shows them (PLAN 2.55, SPEC §3.10): its columns, each
 * one's type, each row's cells as written, and each cell its column does not read, with why. */
export interface Sheet {
  columns: { name: string; type: "number" | "string" | "boolean" | "date" }[];
  rows: string[][];
  problems?: { row: number; column: string; why: string }[];
}

/** A data source the deck declares: the file it is, or none for rows written inline. */
export interface DataSource {
  name: string;
  file?: string;
}

/** An edit of a data source's rows, row 0 the first after the header (`data_edit`). */
export type RowEdit =
  | { op: "set"; row: number; column: string; value: string }
  | { op: "add"; row?: number; values?: Record<string, string> }
  | { op: "remove"; row: number };

/** What `data_edit` did (SPEC §7.2). */
export interface DataEdited {
  edited: boolean;
  source: string;
  file?: string;
  sheet: Sheet;
  added: Finding[];
  removed: Finding[];
  errors: number;
  refused: boolean;
  /** The texts whose quoted figures the edits set again, by node (ADR-0019, PLAN 2.72). */
  quoted?: string[];
}

/** A value of a data source a run quotes (ADR-0019, PLAN 2.72). */
export interface Quote {
  data: string;
  dataTransform?: unknown[];
  row?: number | Record<string, string | number | boolean>;
  column: string;
  format?: string;
}

/** What an edit came to (PLAN 2.3). */
export interface Edited {
  /** Why the source does not compile, and where. */
  error?: Finding;
  /** What lint found; where the deck does not validate, what validation found. */
  findings: Finding[];
  /** Each state, by id, and where its declaration starts in the source. */
  states: [string, number][];
  /** Whether the deck validated, and so is what frames show from now on. */
  valid: boolean;
  /** Whether lint laid the deck out: it does once nothing is an error. */
  laid: boolean;
  /** Whether it laid out every state; an edit lays out the state shown, and the others keep
   * what the last lint of every state found in them, in the formats the deck still lists, each
   * while it shows every node it showed then. */
  whole: boolean;
  /** The deck's timeline now. */
  slots: Slot[];
  /** The formats the deck lists now, besides its own canvas (PLAN 2.62). */
  formats: string[];
  /** Where the deck is: the slot repainted at rest. */
  at?: At;
  /** How long each step took in the worker, ms. */
  ms: { compile: number; paint: number; lint: number };
}

/** What a lint of every state found in the deck compiled last, and how long it took, ms. */
export interface Linted {
  findings: Finding[];
  laid: boolean;
  whole: boolean;
  ms: number;
}

/** A state inspected (SPEC §7.1, `inspect --resolved --timeline`): its nodes resolved, each
 * text node's look, what each node's overrides set, and its cue. */
export interface Inspected {
  state_id: string;
  layout?: string;
  nodes: Record<string, Record<string, unknown>>;
  looks?: Record<string, { role: string; family: string; size: number; weight: number; leading: number; tracking: number; color: string; hex: string }>;
  overrides?: Record<string, string[]>;
  /** The nodes that come on screen in this state, and those that leave it (SPEC §2.2). */
  entered?: string[];
  exited?: string[];
  timeline?: Cue;
}

/** A state's cue, as `scaena inspect --timeline` places it (SPEC §2.4, §3.9), ms: where it falls
 * on the deck's timeline, its transition, and each motion on the state's clock. */
export interface Cue {
  start: number;
  span: number;
  hold: number;
  transition: { duration: number; match: string; curve: Curve };
  motions: Motion[];
}

/** An easing as its cubic Bézier, or a spring as its constants. */
export type Curve = { ease: [number, number, number, number] } | { spring: { stiffness: number; damping: number; mass: number } };

/** One motion on one node, placed on its state's clock (PLAN 2.44). */
export interface Motion {
  node: string;
  motion: "enter" | "exit" | "emphasis" | "anim";
  /** What it moves one at a time: lines, words, glyphs, children, or marks. */
  split?: string | null;
  units: number;
  start: number;
  /** From one unit's start to the next's. */
  stagger: number;
  /** Each unit's. */
  duration: number;
  end: number;
  /** When its first unit starts to change and its last comes to rest: an `anim`'s from its
   * first key. */
  moving: [number, number];
  /** Its `delay` as written: what `time_motion` reads and sets. */
  delay: number;
  /** Where it is written: a JSON pointer into the deck. */
  written?: string;
  curve: Curve;
}

/** The worker to the page. */
export type FromWorker =
  | ({ type: "ready" } & Opened)
  /** The bundle as `reload` read it again. */
  | ({ type: "reloaded"; id: number } & Opened)
  /** The frame request `id` asked for is on the canvas, `size` pixels, painted in `ms`. */
  | { type: "shown"; id: number; size: [number, number]; ms: number }
  | { type: "timeline"; id: number; slots: Slot[] }
  /** How the state `read` named reads: each node it shows that is read, in paint order, an
   * element that names it (`data-node`). */
  | { type: "reading"; id: number; html: string }
  /** A frame of `run` is on the canvas, or the one request `id` sought; posted for each, and
   * when the clock stops. */
  | ({ type: "at"; id?: number } & At)
  | { type: "source"; id: number; source: string }
  | ({ type: "edited"; id: number } & Edited)
  | ({ type: "linted"; id: number } & Linted)
  | { type: "fixed"; id: number; source: string }
  | { type: "inspected"; id: number; inspected: Inspected }
  /** Each visible node's box in the state asked about, and the canvas's size, canvas units. */
  | { type: "boxes"; id: number; boxes: NodeBox[]; size: [number, number] }
  | { type: "hits"; id: number; hits: Hit[] }
  /** The marks asked for: the one at a point, or none; or what rows draw. */
  | { type: "marked"; id: number; marks: DataMark[] }
  | { type: "noted"; id: number; note: NoteMark | null }
  | { type: "calledOut"; id: number; at: AnnotationAt | null }
  /** Where the link asked about goes, or none there. */
  | { type: "linked"; id: number; link: LinkTarget | null }
  | { type: "outlined"; id: number; outline: Outline | null }
  | { type: "framed"; id: number; framing: Framing | null }
  | { type: "markedIn"; id: number; found: { marks: DataMark[]; notes: NoteMark[] } | null }
  | { type: "laidOut"; id: number; layout: LayoutSlots | null }
  /** The preview is painted through the view asked for. */
  | { type: "viewed"; id: number }
  | { type: "found"; id: number; found: Found[] }
  /** The ops of a replacement, a `replace_text` for each match it replaces. */
  | { type: "replacement"; id: number; ops: unknown[] }
  /** Where a focal point picked there would be; `null` off the image. */
  | { type: "focal"; id: number; at: [number, number] | null }
  | { type: "targets"; id: number; targets: Targets }
  | { type: "grid"; id: number; grid: Grid }
  /** Where a drag's box would land (`null`: nowhere that way), and the states its patch changes. */
  | { type: "dragged"; id: number; snapped?: Snapped | null; states?: string[] }
  | { type: "arranged"; id: number; arranged: Arranged | null }
  /** The patch is made: the deck's source now, and what the edit came to. */
  | { type: "made"; id: number; source: string; edited: Edited }
  | { type: "choices"; id: number; choices: Choices }
  | { type: "stateChoices"; id: number; choices: StateChoices }
  | { type: "layoutSuggestions"; id: number; suggestions: LayoutSuggestion[] }
  | { type: "look"; id: number; look: Look }
  | { type: "put"; id: number; put: Put }
  | { type: "carets"; id: number; carets: Carets | null }
  | { type: "characterChoices"; id: number; choices: Choices }
  | { type: "bolding"; id: number; look: Record<string, unknown> }
  | { type: "italicizing"; id: number; look: Record<string, unknown> }
  | { type: "themes"; id: number; themes: Themes }
  /** A theme chosen: what it did, as `theme --apply` says it, and unless it was refused, the
   * deck's source now and what the edit came to. */
  | { type: "rethemed"; id: number; themed: Themed; source?: string; edited?: Edited }
  | { type: "reached"; id: number; states: string[] }
  | { type: "inserts"; id: number; inserts: Insert[] }
  | { type: "layers"; id: number; layers: Layer[] }
  /** The patch that adds a node: an insert, or a copy. */
  | { type: "adding"; id: number; added: Added }
  /** The patch that takes a node away. */
  | { type: "deleting"; id: number; patch: unknown[] }
  /** A node as the clipboard holds it, and what pasting a clip makes. */
  | { type: "copied"; id: number; clip: string }
  | { type: "pasted"; id: number; pasted: Pasted }
  | { type: "grouped"; id: number; grouped: Grouped }
  | { type: "thumbnails"; id: number; thumbs: Thumb[] }
  /** The patch that adds a state, and the state's id. */
  | { type: "addingState"; id: number; added: { id: string; patch: unknown[] } }
  /** The slides a person may start, each with its picture. */
  | { type: "starters"; id: number; starters: Starter[] }
  /** The patch that starts a slide, and the slide's id. */
  | { type: "starting"; id: number; added: { id: string; patch: unknown[] } }
  | { type: "waiting"; id: number; waiting: Waiting[] }
  /** The patch that fills a slot that waits for words, and the node's id. */
  | { type: "filling"; id: number; filled: { id: string; patch: unknown[] } }
  /** The text is typed: the deck's source now, what the edit came to, and the text's carets. */
  | { type: "typed"; id: number; source: string; edited: Edited; carets: Carets | null }
  /** The bundle is saved `where`, and the session goes on from it: the files the save
   * renamed, from and to, each a path the source may name; and how many files it wrote. */
  | { type: "saved"; id: number; where: Where; renamed: [string, string][]; files: number; recorded: boolean }
  /** The bundle as a `.scaena` zip, and each font subset: its path, and its size before and
   * after, bytes. */
  | { type: "zipped"; id: number; bytes: ArrayBuffer; subset: [string, number, number][] }
  /** What `export` made: a PNG, a PDF, or an HTML file's bytes. */
  | { type: "exported"; id: number; bytes: ArrayBuffer }
  /** The dropped file is in the bundle at `path`. */
  | { type: "dropped"; id: number; path: string }
  /** The data file at `path`, read by source `data`: one the deck declares already (no
   * `attached`), or one the patch declares, or refused (`attached`, an empty patch). */
  | { type: "attached"; id: number; path: string; data: string; attached: Attached | null; patch: unknown[] }
  /** What text pasted on the canvas is as a sheet's cells; none where it is not cells. */
  | { type: "cells"; id: number; cells: Cells | null }
  /** The deck's data sources, and source `name` as a sheet, or why it does not read as one. */
  | { type: "sheet"; id: number; sources: DataSource[]; name?: string; sheet?: Sheet; file?: string; why?: string }
  | { type: "bundleFiles"; id: number; files: BundleFile[] }
  /** A file taken out: what the deck came to, shown and linted as an edit is. */
  | { type: "removed"; id: number; edited: Edited }
  /** The bundle's versions, oldest first; none where it keeps no history. */
  | { type: "versions"; id: number; versions: Version[] | null }
  /** A version shown: its states, and the one drawn, as a PNG; or why it is not drawn (a file
   * it names the bundle no longer holds). */
  | { type: "version"; id: number; states: string[]; state: string; png?: ArrayBuffer; why?: string }
  | { type: "compared"; id: number; compared: Compared }
  /** What restoring a version did; where it did, each data file it wrote, before and after, the
   * deck's source after, and what the edit came to. */
  | { type: "restored"; id: number; restored: Restored; files: Rewritten[]; source?: string; edited?: Edited }
  | { type: "filesWritten"; id: number; edited?: Edited }
  | { type: "themeText"; id: number; theme: ThemeText | null }
  /** What a theme edit did; where it wrote, the theme file before and after (none for an inline
   * theme), the deck's source after, and what the edit came to. */
  | { type: "themeEdited"; id: number; result: ThemeEdited; files: Rewritten[]; source?: string; edited?: Edited }
  /** What `dataEdit` did; where it wrote, the deck's source after and what the edit of it came to. */
  | { type: "dataEdited"; id: number; result: DataEdited; source?: string; edited?: Edited }
  /** The source whose file an undo or redo wrote, and what the edit came to; none where there was
   * nothing to undo or redo. */
  | { type: "dataUndone"; id: number; name?: string; edited?: Edited }
  /** A step of the assistant's answer to `ask` request `id`. */
  | { type: "assistant"; id: number; event: AssistantEvent }
  | { type: "models"; id: number; models: string[] }
  /** The CPU painter met a shader whose rows `count` more workers could share (PLAN 2.28): the
   * page starts each as it started this one, and hands this one a port to each (`helpers`). */
  | { type: "helpers"; count: number }
  /** The formats beside the canvas show `state` `t` ms into its cue (PLAN 2.62). */
  | { type: "besides"; state: string; t: number }
  /** Request `id` failed, or, without one, opening or playing did. `webgpu`: setting
   * WebGPU up failed, and the CPU painter may still paint. */
  | { type: "error"; id?: number; message: string; webgpu?: boolean };
