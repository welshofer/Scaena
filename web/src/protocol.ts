// What the pages and the engine's worker say to each other (PLAN 2.1–2.4, SPEC §9.2).
// Each request names the format it is in: one of the deck's `formats`, or the deck's own
// canvas when it names none (SPEC §3.4).

/** Where a bundle comes from (SPEC §3.1, §9.2). */
export type Source =
  /** A deck file at a URL, whose directory is the bundle: it reads the files the deck names,
   * and every file its manifest lists. `serve`, on a page `scaena serve` serves (PLAN 2.11):
   * the page's id, which its writes carry, for the bundle is kept there, and saves back to it. */
  | { url: string; serve?: Serving }
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
  | { create: { theme: string; title: string } }
  /** A bundle's files as a worker held them when its engine stopped on a panic (lift-up STAB-2),
   * and where it is kept, if anywhere: a fresh worker goes on from them, and saves there. */
  | { recovered: Record<string, ArrayBuffer>; name: string; kept?: Kept };

/** Where a recovered bundle is kept: the browser's storage by name, a folder on disk by its handle,
 * or the folder `scaena serve` serves it from, by the deck's URL and the page's id. */
export type Kept = { opfs: string } | { folder: FileSystemDirectoryHandle } | { url: string; serve: Serving };

/** What a page `scaena serve` opened writes the folder with (PLAN 2.11): its own id, so it does not
 * hear its own saves, and the token the address it was opened at carries (lift-up 10-24b SEC-5). */
export interface Serving {
  client: string;
  token: string;
}

/** Where Save as puts a bundle (PLAN 2.12): a folder on disk the page was given, or the
 * browser's storage under a name (`name-2`, … where that is taken). */
export type SaveTo = { folder: FileSystemDirectoryHandle } | { opfs: string };

/** What the editor exports (PLAN 2.54, SPEC §10), each as `scaena export` writes it:
 * - `png`: `state` at rest in `format` (the deck's canvas without one), `width` pixels wide, the
 *   height keeping the canvas's aspect, painted by the CPU painter;
 * - `slides`: every slide once its steps have played, each as `png` paints it, in one zip, named
 *   by its number, `{name} - Slide 3.png` (PLAN 2.122);
 * - `pdf`: the deck, each slide at its last state, drawn by the PDF's own module, which the
 *   worker loads the first time a PDF is asked for; with `notes`, each slide smaller with the
 *   presenter's notes under it, as `export --format pdf --notes` prints them;
 * - `html`: the deck as one file that plays offline, named `name`: `page`, the single-file
 *   player's page, filled in with the bundle, its fonts subset. */
export type Export =
  | { kind: "png"; state: string; width: number; format?: string }
  | { kind: "slides"; width: number; format?: string; name: string }
  | { kind: "pdf"; notes?: boolean }
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
  /** The state as a person counts it: `Slide 3`, or `Slide 3, step 2` for a step after a slide's
   * first (PLAN 2.99). */
  said?: string;
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
  /** A shape that takes words (PLAN 2.108): any but a line or an arrow. */
  takesWords?: boolean;
  /** The text that holds a shape's words (PLAN 2.108), which a click on them reaches as the shape
   * and a double click types in. */
  words?: string;
  /** A connector's ends, from and to (PLAN 2.109), which place it: the canvas moves, resizes, and
   * turns nothing of it. */
  joins?: [string, string];
  /** A connector's line, its points in canvas units (PLAN 2.109), which a pointer picks it by: its
   * box is mostly empty. */
  route?: [number, number][];
  /** A code text's language (PLAN 2.133): as the deck names it, and as a person calls it, which
   * its chip shows. */
  code?: { language: string; said: string };
}

/** What says pasted words are code, and in which language (PLAN 2.133): `plain` pastes them as
 * words whatever they look like (Paste as Text); `html` is the HTML copied with them, whose class
 * may name it (`language-rust`); `vscode` VS Code's `vscode-editor-data`, its `mode`; `file` the
 * name of the file they came from. */
export interface PasteHints {
  plain?: boolean;
  html?: string;
  vscode?: string;
  file?: string;
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
  /** What the words are (PLAN 2.83): a text, a node's description, notes, a beat's claim, the
   * words a chart or a table draws, or a data source's cell. */
  kind: "text" | "alt" | "notes" | "claim" | "chart" | "data";
  /** The text's node, the node the description describes, or the chart or table; none for notes,
   * a claim, or a cell. */
  node?: string;
  /** The data source, for a cell. */
  source?: string;
  /** The file a cell is in. */
  file?: string;
  /** Why a replacement leaves these words as they are: a value of the data, which a filter, a
   * quote, or a highlight may name. */
  kept?: string;
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
  /** Each stretch of it a link goes from (PLAN 2.70): its start and end, and where it goes. */
  links?: [number, number, LinkTarget][];
  /** Its words still as its slot's prompt put them (PLAN 3.30): typing begins with them all
   * selected, so it replaces them. Asked as typing begins, not after each change. */
  prompt?: boolean;
  /** Where it is code (PLAN 2.139): the columns between its tab stops, the number its first line
   * goes by, and its language, which its keys are given; none for prose. */
  code?: { tab: number; from: number; language: string } | null;
}

/** A key a code text answers as a code editor does (PLAN 2.139, `code::keyed`). */
export type CodeKey = "Tab" | "Shift+Tab" | "Return";

/** A text and its selection after a code key, offsets in UTF-16 code units. */
export interface Keyed {
  text: string;
  from: number;
  to: number;
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
  /** The node as a person names it, by what it shows: its words, else its kind (PLAN 2.99). */
  said?: string;
  /** Its kind as a person names it: `Text`, `Chart`, `Background Effect`, a shape's own, `Star`;
   * `Icon` for a path on a view, and `Drawing` for the pen's (PLAN 2.99). */
  noun?: string;
  /** An image's frame, said, where its theme frames pictures (PLAN T.3a): the theme's, which
   * nothing on the image changes. */
  framed?: string;
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
  /** The state as a person counts it: `Slide 3, step 2` (PLAN 2.99). */
  said?: string;
  /** The slide's number and footer (ADR-0028). */
  furniture: FurnitureChoice;
}

/** What the inspector offers of a slide's number and footer (ADR-0028): the deck's, which
 * `furnish` sets, and the slide's own, which `set_state`'s `furniture` writes on its first state. */
export interface FurnitureChoice {
  /** Whether the theme places a number or a footer at all. */
  placed: boolean;
  /** Whether the deck numbers its slides. */
  number: boolean;
  /** The words at the foot of its slides. */
  footer?: string;
  /** The slide's first state, where the slide says whether it shows them. */
  head: string;
  /** What the slide says: shown, hidden, or as its layout has them (absent). */
  slide?: boolean;
  /** Whether the slide shows them, as it stands. */
  shown: boolean;
}

/** The words an editor shows for a state (PLAN 2.99): each object it shows named by what it
 * shows and by its kind, each of the theme's names said by its kind of name (`color`,
 * `text-role`, `layout`, …, and `motion-exit`, each motion preset as a Build Out effect), and how a
 * motion, its split, and a shape are said. */
export interface Lexicon {
  objects: Record<string, string>;
  kinds: Record<string, string>;
  names: Record<string, Record<string, string>>;
  motions: Record<string, string>;
  splits: Record<string, string>;
  shapes: Record<string, string>;
  /** Each place a node is put by name, said: a slot of the theme's layouts, an area of a grid. */
  places: Record<string, string>;
  /** Each weight a type menu names, by its number: `700` is Bold. */
  weights: Record<string, string>;
  /** Each type of node by its name in the format: `shader` is Background Effect. */
  types: Record<string, string>;
  /** What lint finds, by its code, in a sentence a person would say: `E100` is The text doesn't
   * fit its box. */
  issues: Record<string, string>;
  /** Each data source the deck declares, by the name the deck writes it with, as a person names
   * it (PLAN 3.18): `q3`, read from `data/q3-revenue.csv`, is Q3 Revenue. */
  sources: Record<string, string>;
}

/** A slide a person may start (PLAN 3.30): one in a layout of the theme, a text in each slot that
 * says what goes there, or a blank one; and its picture. */
export interface Starter {
  /** The theme's layout it is in; none for the blank slide. */
  layout: string | null;
  /** What a person calls the layout, as the theme names it (theme format 0.12); none for the
   * blank slide, or a layout the theme names none for. */
  name: string | null;
  /** What the theme says the layout is for. */
  description: string | null;
  /** The gallery's section it is offered under; none for the blank slide. */
  group: string | null;
  /** The other names a search finds it by (ADR-0031): Roadmap's Kanban. */
  also?: string[];
  /** Its picture, straight-alpha RGBA: none, 0 × 0, for one listed before it is drawn. */
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
  /** Whether a picture goes there: no words, and no chart or table its example (PLAN 2.107). */
  picture: boolean;
  /** The text role words there are set in, which a press on its words inserts a text of. */
  role?: string;
}

/** A layout the state shown may take (PLAN 2.92), as `scaena inspect --layouts` judges it: what
 * lint finds in the state laid out in it, the states the change reaches, and the `set_state` that
 * makes it (none for the layout it takes now), with its picture at rest, `width` × `height` pixels
 * of straight-alpha RGBA. */
export interface LayoutSuggestion {
  layout: string;
  current?: boolean;
  errors: number;
  warnings: number;
  /** The layout's places, for words, a picture, or a figure, the state leaves empty in it. */
  empty: number;
  /** The state's texts its slots set in a role other than their own. */
  recast: number;
  /** The layouts that draw the state as this one does, lint judging them alike: left out. */
  alike?: string[];
  reach: string[];
  patch: unknown[];
  width: number;
  height: number;
  pixels: ArrayBuffer;
}

/** One background a slide may take (PLAN 2.148): None, or one of the theme's shader presets, by
 * its name and its kind in words, with the patch that gives the slide it (none for the one it has)
 * and the slide drawn with it at rest, `width` × `height` pixels of straight-alpha RGBA. */
export interface BackgroundOption {
  preset?: string;
  kind?: string;
  label: string;
  /** Its kind in words: Mesh Gradient. */
  said?: string;
  current?: boolean;
  patch: unknown[];
  width: number;
  height: number;
  pixels: ArrayBuffer;
}

/** What a slide may take as its background (PLAN 2.148): its first state, where one enters; why
 * backgrounds stay off it, where it shows a chart or a table (lint W311); and each option. */
export interface Backgrounds {
  first: string;
  held?: string;
  options: BackgroundOption[];
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
  /** What the theme gives a text's look where the deck sets none, which the state then shows: its
   * role's family, weight, size, italic, case, tracking, or color; for characters selected, the
   * text's own look where their run sets none, its role and emphasis too. Absent for the rest. */
  theme?: unknown;
  /** Its label, and each name or word it offers, its value, and the theme's, said as a person says
   * them (PLAN 2.99): `style/family` is Font, `onSurface` Text, `fast` Fast (0.18 s). */
  words?: { label: string; said: Record<string, string>; colors?: Record<string, string> };
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
  /** The data source it reads where the deck has none to offer (a chart or a table of sample
   * rows), which the patch that inserts it adds first. */
  sample?: { name: string; source: unknown };
}

/** A state at rest, painted small for the state strip (PLAN 2.35): what identifies its drawing,
 * and, where that is not what the strip holds, its pixels (straight-alpha RGBA, row by row). */
export interface Thumb {
  state: string;
  digest: string;
  width?: number;
  height?: number;
  pixels?: ArrayBuffer;
  /** Why it could not be painted, said in words (lift-up STAB-4): no pixels come with it. */
  failed?: string;
}

/** A node a patch adds: its id, where it lands, and the patch. */
export interface Added {
  id: string;
  cell: Rect;
  patch: unknown[];
}

/** The words of a shape to type in (PLAN 2.108): the text that holds them, and the patch that
 * adds it where the shape holds none, or brings it back where the state does not show it, empty
 * where it does. */
export interface Worded {
  id: string;
  patch: unknown[];
}

/** A connector made (PLAN 2.109): its id, and the patch that adds it. */
export interface Connected {
  id: string;
  patch: unknown[];
}

/** Copies of several nodes beside them (PLAN 2.42): each copy's id, those of the nodes copied
 * first, then those of the connectors among them that join two of them, copied to join their
 * copies (PLAN 2.109); and the patch that adds them all. */
export interface Duplicated {
  ids: string[];
  patch: unknown[];
}

/** What a diagram draws a text's lines as (PLAN 2.110). */
export type Diagram = "process" | "cycle" | "hierarchy";

/** A diagram made from a text's lines (PLAN 2.110): its container, and the patch that makes it
 * in the text's place. */
export interface Diagrammed {
  id: string;
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

/** A picture as the engine reads it before it joins the bundle (SPEC §3.3): its size as it is
 * seen, a JPEG's turned by its EXIF orientation, and the longest side an image may have. One too
 * large goes in as a smaller copy, with nothing asked (`web/src/pictures.ts`). */
export interface Picture {
  width: number;
  height: number;
  largest: number;
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
  /** What the change did, and who made it, in a person's words (PLAN 2.99,
   * `scaena_core::words::change`, `author`), as the Mac's Versions says them. */
  said: string;
  by: string;
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
  /** Each text whose words, or only their look, changed, by state and node: its words before and
   * after, which name it and show what changed, alike where only the look did. */
  texts?: Record<string, Record<string, [string, string]>>;
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
/** A source a deck made from a prompt was grounded in, with the beats citing it (narrative
 * FUNC-3, `scaena_core::files::Grounded`). */
export interface Grounded {
  name: string;
  kind: string;
  url?: string;
  bytes: number;
  truncated: boolean;
  /** The beats citing it, by id, and the first state of each, which a page names as a slide. */
  cited: string[];
  states: string[];
}

export interface BundleFile {
  path: string;
  type: "image" | "font" | "data";
  bytes: number;
  named: Named[];
  used: { node: string; states: string[] }[];
  /** Where a picture fetched from a service came from (ADR-0030): "From Geoapify, fetched 15
   * October 2026". */
  fetched?: { from: string; service: string; date: string; said: string };
}

/** What names a file of the bundle. */
export type Named =
  | { by: "node"; node: string }
  | { by: "evidence"; beat: string }
  | { by: "font"; family: string; style?: string }
  | { by: "theme"; family: string }
  | { by: "source"; source: string };

/** One of the deck's narrative health's measures (SPEC §7.7): 0–100, or none where it is not
 * scored (coherence read by words alone), and what it says. */
export interface Measure {
  score: number | null;
  says: string;
}

/** A slide as narrative health reads it: its first state, its beat, its headline, the arc's curve
 * at its place, the intensity and tone it declares and that its words read, how like its words are
 * to the slide's before it, and how it stands in its arc. */
export interface HealthRead {
  state: string;
  beat?: string;
  title: string;
  curve: number;
  declared?: number;
  read?: number;
  tone?: number;
  toneRead?: number;
  after?: number;
  standing: "ok" | "bends" | "drifted";
  /** The sources its beat cites of those the deck was grounded in, by name (narrative FUNC-3). */
  sources?: string[];
}

/** Something narrative health asks a person to look at, on one slide: what it says, the question
 * for the assistant, and a fix where one is safe, a patch the user makes. */
export interface HealthFinding {
  state: string;
  slide: number;
  severity: "error" | "warning" | "info";
  kind: "title" | "empty" | "turn" | "arc" | "intensity" | "tone" | "drift" | "layout" | "claim" | "bigIdea";
  says: string;
  ask: string;
  fix?: { says: string; patch: unknown[] };
}

/** The deck's narrative health (PLAN G.9), as `scaena health --json` gives it. */
export interface Health {
  score: number;
  arc?: string;
  beats: string[];
  pacing: Measure;
  balance: Measure;
  coherence: Measure;
  fidelity: Measure;
  intensity: Measure;
  tone: Measure;
  slides: HealthRead[];
  findings: HealthFinding[];
  /** The big idea the settled outline names, the one sentence the audience should remember
   * (lift-up 10-24b FUNC-1). */
  bigIdea?: string;
}

/** A picture a person asks for in the editor (PLAN 2.107): what it shows, in their words; made as
 * an illustration or a photo, in the theme's palette; by the best image model the key kept for
 * `provider` lists, at `base` where it is not the provider's own; for a place `width` × `height`
 * on the slide, which frames it. */
/** Where a map goes (ADR-0030, PLAN O.1c): the image it shows in, the empty place for a picture
 * it fills, or a box; with the style the theme's surface asks and the deck's language. */
export interface MapTarget {
  state: string;
  node?: string;
  slot?: string;
  rect: [number, number, number, number];
  style: "light" | "dark";
  lang?: string;
}

/** What a map is asked for, as the generator's `MapFetching` takes it. */
export interface MapAsk {
  place: string;
  known?: { name?: string; lat: number; lon: number };
  center?: { lat: number; lon: number };
  zoom?: number;
  show?: "block" | "neighborhood" | "city" | "region";
  style: string;
  size: [number, number];
  lang?: string;
  pick?: number;
  list?: boolean;
}

/** A place a name matched, to pick one of. */
export interface MapPlace {
  name: string;
  address: string;
  lat: number;
  lon: number;
  kind?: string;
}

/** A map fetched: its record, where its place stands in the picture, the place's name, and the
 * picture. */
export interface FetchedMap {
  fetched: unknown;
  pin: [number, number];
  place: string;
  png: ArrayBuffer;
}

/** What putting a map takes: the image, its picture's path, the place, what is said, the ops. */
export interface Mapped {
  node: string;
  src: string;
  place: string;
  said: string;
  ops: unknown[];
}

/** What fetching an image's map again asks for. */
export interface MapRefresh {
  target: MapTarget;
  asked: string;
  place: { name?: string; lat: number; lon: number };
  center: { lat: number; lon: number };
  zoom: number;
  style: string;
  label?: string;
}

export interface PictureJob {
  subject: string;
  look: "illustration" | "photo";
  provider: "openai" | "gemini";
  key: string;
  base?: string;
  width: number;
  height: number;
}

/** The page to the worker. */
export type ToWorker =
  /** Open the bundle at `source`, and paint into `canvas` by `painter`, with the engine's
   * module `engine`, compiled, if the page carries it (PLAN 2.5); the worker loads its own
   * without it. */
  | { type: "open"; source: Source; painter: Painter; canvas: OffscreenCanvas; engine?: WebAssembly.Module }
  /** Be the worker a deck is made from a prompt in (PLAN G.5), which holds no deck: say what a
   * brief may ask (`briefing`), loading the generator's module, then make the deck `generate`
   * asks for, saying how far it has come (`generating`) and handing back its files
   * (`generated`). Stopping the worker stops it. */
  | { type: "briefing"; engine?: WebAssembly.Module }
  /** Panic, to test the page's recovery from one (lift-up STAB-2): the editor's module alone
   * carries it; the player's answers with an error. */
  | { type: "trap" }
  | { type: "generate"; generate: Generate; engine?: WebAssembly.Module }
  /** The outline as the person settled it (narrative FUNC-1): the slides are written to it. */
  | { type: "storySettled"; outline: StoryOutline }
  /** Another arc for the outline, by name, or the heuristic's where none: outlined again. */
  | { type: "outlineAgain"; arc?: string }
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
  /** Paint frames at the size of the slide shown as large as fits in `room`, device pixels,
   * keeping the canvas's aspect, from now on; at the canvas's own size with none (lift-up
   * ATTR-2). What is shown is painted again so. */
  | { type: "fit"; id: number; room: [number, number] | null }
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
  /** What to ask a model for the person editing (PLAN 2.101), from the deck as it stands. */
  | { type: "writing"; id: number; ask: WritingAsk }
  /** A model's `answer` to `ask` as it is offered, and the patch that takes it (PLAN 2.101). */
  | { type: "written"; id: number; ask: WritingAsk; answer: string }
  | { type: "words"; id: number; state: string }
  /** The notes a presenter reads for `state` (SPEC §2.6), as written and as the blocks a
   * speaker reads: what a rehearsal shows. */
  | { type: "notes"; id: number; state: string }
  /** The layouts `state` may take, best first, each painted at rest `height` pixels high, in the
   * deck the editor's `source` compiles to (PLAN 2.92). */
  | { type: "layoutSuggestions"; id: number; state: string; height: number; format?: string }
  /** What `state`'s slide may take as its background, each drawn at rest `height` pixels high
   * (PLAN 2.148). */
  | { type: "backgrounds"; id: number; state: string; height: number; format?: string }
  /** The backgrounds drawn last, each painted again with its effect's clock at `t` seconds (PLAN 2.148). */
  | { type: "backgroundFrames"; id: number; t: number }
  /** The patch a background chosen makes, each text too faint over it set in a color that reads (PLAN 2.148). */
  | { type: "backgroundPatch"; id: number; state: string; preset?: string }
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
  /** The words of code text `node` as `state` shows them, which Copy Code copies (PLAN 2.133). */
  | { type: "codeWords"; id: number; source: string; state: string; node: string; format?: string }
  /** `text` tidied as a paste of code in `language` is (PLAN 2.133). */
  | { type: "tidied"; id: number; text: string; language: string }
  /** `text` with `from` to `to` selected after `key` in code whose tab stops are `tab` apart
   * (PLAN 2.139). */
  | { type: "keyed"; id: number; text: string; from: number; to: number; key: CodeKey; tab: number; language?: string }
  /** Make Code's patch of `node`, in `language` or the one its words read as (PLAN 2.139). */
  | { type: "coding"; id: number; source: string; node: string; language?: string }
  /** Not Code's patch of `node` (PLAN 2.139). */
  | { type: "uncoding"; id: number; source: string; node: string }
  /** Tidy Code's patch of `node` as `state` shows it (PLAN 2.139). */
  | { type: "tidying"; id: number; source: string; state: string; node: string }
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
  | { type: "insertingIcon"; id: number; source: string; state: string; icon: { name: string; path: string; view: [number, number, number, number] }; at: [number, number]; format?: string }
  /** What `inserts` offers `n`th, drawn in the box a drag from `from` to `to` covers (canvas
   * units), snapped to the theme's grid as a resize snaps, or, `free`, where it was drawn (PLAN
   * 2.48). */
  | { type: "drawing"; id: number; source: string; state: string; n: number; from: [number, number]; to: [number, number]; free: boolean; format?: string }
  /** A stroke of the pen in `state`, its points as `x, y` pairs in canvas units (PLAN 2.114): one
   * more stroke of the drawing `onto` where the state shows it as one, else a new drawing. */
  | { type: "inking"; id: number; source: string; state: string; points: number[]; onto?: string; format?: string }
  | { type: "duplicating"; id: number; source: string; state: string; node: string; format?: string }
  /** A copy of each of `nodes` beside it, in one patch (PLAN 2.42). */
  | { type: "duplicatingAll"; id: number; source: string; state: string; nodes: string[]; format?: string }
  | { type: "wording"; id: number; source: string; state: string; shape: string }
  | { type: "connecting"; id: number; source: string; state: string; from: string; to: string; arrow: boolean }
  /** The patch that draws text `node`'s lines in `state` as a diagram of `kind` in its place, in
   * the format shown (PLAN 2.110). */
  | { type: "diagramming"; id: number; source: string; state: string; node: string; kind: Diagram; format?: string }
  /** The patch that takes `nodes` away, in one patch (PLAN 2.34, 2.42). */
  | { type: "deleting"; id: number; source: string; state: string; nodes: string[]; everywhere: boolean }
  /** The patch that puts `nodes`, children of one container as `state` shows them, in a new group
   * where they stand, on the deck the editor's `source` compiles to (PLAN 2.43). */
  | { type: "grouping"; id: number; source: string; state: string; nodes: string[] }
  /** What the clipboard holds of `nodes` as `state` shows them, on the deck the editor's `source`
   * compiles to (PLAN 2.37, 2.42): the clip, as JSON text. The first is the node copied. */
  | { type: "copying"; id: number; source: string; state: string; nodes: string[]; format?: string }
  /** The patch that pastes `clip`, the clipboard's text, entering in `state` about `at` (canvas
   * units), on the deck the editor's `source` compiles to: a clip pastes what it holds, other
   * text a text in the theme's body role (PLAN 2.37). */
  | { type: "pasting"; id: number; source: string; state: string; clip: string; at: [number, number]; format?: string; hints?: PasteHints }
  /** Each state at rest in `format`, painted by the CPU painter `height` pixels high, for the
   * state strip (PLAN 2.35): only those whose drawing is not the one `known` holds (each state's
   * digest, as the strip last had it) come with pixels. With `each`, each painted is sent as it is
   * (`thumbed`), and the answer holds the rest. */
  | { type: "thumbnails"; id: number; height: number; known: Record<string, string>; format?: string; states?: string[]; each?: boolean }
  /** The patch that adds a state after `state`, on the deck the editor's `source` compiles to: a
   * `step` of its slide, or a `slide` of its own (PLAN 2.35). */
  | { type: "addingState"; id: number; source: string; state: string; what: "step" | "slide" }
  /** The slides a person may start after `state`'s slide, on the deck the editor's `source`
   * compiles to (PLAN 3.30): one in each of the theme's layouts, then a blank one, each painted at
   * rest `height` pixels high in `format`. */
  | {
      type: "starters";
      id: number;
      source: string;
      state: string;
      height: number;
      format?: string;
      /** Only these layouts, a null the blank slide, as a picker draws a few at a time (PLAN 3.31). */
      layouts?: (string | null)[];
      /** Listed, none drawn: what a picker offers before it draws any. */
      listed?: boolean;
    }
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
  /** The bundle the browser keeps as `from` renamed `to`, or with no `to` deleted (PLAN 2.124):
   * the open bundle, kept there, follows its rename, and is not deleted. */
  | { type: "rekeep"; id: number; from: string; to?: string }
  /** Export the deck `source` compiles to (PLAN 2.54), made here for the page to download:
   * nothing is sent anywhere. A source that does not compile, or a deck that does not validate,
   * is not exported. */
  | { type: "export"; id: number; source: string; as: Export }
  /** A file dropped on the page, into the bundle: where it goes is what it is, and an image
   * is named by its SHA-256 (`Player.place`). */
  | { type: "drop"; id: number; name: string; bytes: ArrayBuffer }
  | { type: "picture"; id: number; bytes: ArrayBuffer }
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
  /** The narrative health of the deck `source` compiles to (PLAN G.9), read by the generator's
   * module. */
  | { type: "health"; id: number; source: string }
  /** A picture made by the user's image model, in the deck's theme (PLAN 2.107), by the
   * generator's module. */
  | { type: "makePicture"; id: number; picture: PictureJob }
  | { type: "mapTarget"; id: number; source: string; state: string; node?: string; slot?: string; rect: [number, number, number, number] }
  | { type: "fetchMap"; id: number; ask: MapAsk; key: string }
  | { type: "mapping"; id: number; source: string; target: MapTarget; fetched: unknown; pin: [number, number]; label?: string; png: ArrayBuffer }
  | { type: "mapRefreshing"; id: number; source: string; state: string; node: string; zoom?: number; center?: { lat: number; lon: number } }
  /** `path` taken out of the bundle, by the user: one nothing names (PLAN 2.59). */
  | { type: "removeFile"; id: number; source: string; path: string; index: number; format?: string }
  /** `path`, taken out, put back by its path: the undo of its removal in the editor's one undo
   * (PLAN 2.59). Answered as `removeFile` is. */
  | { type: "putBack"; id: number; source: string; path: string; index: number; format?: string }
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
  /** Each kind of code token as the theme draws it (PLAN 2.144). */
  | { type: "codeColors"; id: number }
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
      files: { path: string; text: string | null; bytes?: ArrayBuffer }[];
      edit?: { source: string; index: number; format?: string };
    }
  /** The file at `path` in the bundle, as the session holds it (lift-up REL-4): what a recovery
   * copy keeps of a file written since the save. */
  | { type: "file"; id: number; path: string }
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
  /** Ask a model once, with the user's key and nothing to call (PLAN 2.101): its words. */
  | { type: "write"; id: number; call: WritingCall }
  /** What to ask a model for one slide at `place`, with the person's `ask` (lift-up FUNC-1). */
  | { type: "slideAsk"; id: number; place: SlidePlace; ask: string }
  /** The slide a model's `answer` holds, written at `place` by `author` as the deck's generator
   * builds slides, one patch and lint's fixes on it (lift-up FUNC-1); then the deck's source, and
   * what the edit came to, laid out for state `index` in `format`. */
  | { type: "slideWrite"; id: number; place: SlidePlace; answer: string; author: string; index: number; format?: string }
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

/** A deck made from a prompt (PLAN G.5, ADR-0026), as `scaena generate` makes one: Narrato's
 * brief, an arc of its by name or the model's choice, a theme that ships, who writes it with the
 * user's own key, and what it is grounded in. */
export interface Generate {
  brief: Brief;
  arc?: string;
  theme: string;
  provider: ProviderId;
  model: string;
  key: string;
  base?: string;
  /** Files, each by its name: Markdown, text, RTF, HTML, JSON, CSV, Excel, Word, PowerPoint, or
   * a PDF, which the model reads as itself. */
  files: { name: string; bytes: ArrayBuffer }[];
  /** Links: `github:OWNER/REPO` or a GitHub address, read from its tree file by file, or a web
   * page whose host lets another page read it. */
  links: string[];
  /** A GitHub token of the user's own, for a private repository; sent to GitHub alone. */
  github?: string;
  /** Deep research on the topic first, its report a source (PLAN G.8). */
  research: boolean;
  /** Who makes the slides' pictures (PLAN G.10): an image model the key lists, best first,
   * made as illustrations in the theme's colors or as photographs; none where it is not given. */
  pictures?: { provider: "openai" | "gemini"; key: string; base?: string; look: "illustration" | "photo" };
  /** Whether the model is shown each slide as it is drawn, and may give one another of its
   * layouts and tighten its words (PLAN G.13); yes where it is not given. */
  look?: boolean;
  /** Whether the outline is shown first (narrative FUNC-1): the arc, the big idea, and each beat's
   * claim, which the person settles before the slides are written; the slides at once where not. */
  outline?: boolean;
}

/** Narrato's brief (`scaena_narrato::Brief`): what the deck is about, in the user's words, and
 * what they say of it; a field left out takes Narrato's default. */
export interface Brief {
  topic: string;
  audience?: string;
  goal?: string;
  length?: string;
  modality?: string;
  desiredOutcomes?: string;
  language?: string;
  voice?: string;
}

/** What a brief may ask, as Narrato knows it: its goals, lengths, and ways a deck is delivered,
 * the audiences its personas name, its arcs, and the defaults. */
export interface Choices {
  goals: string[];
  lengths: string[];
  modalities: { id: string; label: string }[];
  audiences: string[];
  arcs: string[];
  defaults: { audience: string; goal: string; length: string };
}

/** How far a deck being made from a prompt has come, as `scaena generate` says it. */
export type GeneratingEvent =
  | { kind: "fetchingFont"; family: string; language: string; megabytes: number }
  | { kind: "reading"; name: string }
  | { kind: "read"; name: string; what: string }
  | { kind: "grounded"; name: string; what: string; kilobytes: number; cut: boolean }
  | { kind: "researching"; topic: string }
  | { kind: "researchingNow"; doing: string }
  | { kind: "researched"; sources: number; cut: boolean }
  | { kind: "choosing" }
  | { kind: "chose"; arc: string; fallback?: string }
  /** The outline before the slides (narrative FUNC-1): asked, written with this many beats, and
   * settled by the person with this many. */
  | { kind: "outlining" }
  | { kind: "outlined"; beats: number }
  | { kind: "settled"; beats: number }
  | { kind: "writing"; slides: number }
  | { kind: "retrying"; seconds: number; tried: number }
  | { kind: "wrote"; slides: number; truncated: boolean }
  | { kind: "painting"; pictures: number; model: string }
  | { kind: "painted"; pictures: number; unmade: string[] }
  | { kind: "making" }
  | { kind: "looking"; slides: number }
  | { kind: "looked"; slides: number }
  | { kind: "unlooked"; why: string }
  | { kind: "fitting"; texts: number }
  | { kind: "fitted"; texts: number }
  | { kind: "unfitted"; why: string };

/** The deck's outline before its slides (narrative FUNC-1): the big idea, and each beat with the
 * one claim its slides must leave the audience with and the slides it takes. */
export interface StoryOutline {
  bigIdea: string;
  beats: StoryBeat[];
}

export interface StoryBeat {
  name: string;
  claim: string;
  slides: number;
}

/** What a deck made from a prompt came to, as `scaena generate --json` says it. */
export interface Generated {
  created: { created: boolean; errors: number; findings: Finding[] };
  arc: string;
  fallback?: string;
  layouts: string[];
  fixed: Finding[];
  fitted?: string[];
  unfitted?: string;
  /** How many pictures were made for its slides, and why each other asked for was not. */
  pictures?: number;
  unpictured?: string[];
  /** What the model changed on the deck it saw drawn, slide by slide (PLAN G.13), or why not. */
  looked?: { slide: number; layout?: string; title?: string; subtitle?: string; bullets?: string[]; why: string }[];
  unlooked?: string;
  /** Each sheet given, kept as a data source, or why not. */
  data: { sheet: string; id?: string; rows?: number; why?: string }[];
  /** What each source gave the prompt, and whether it was cut short to fit. */
  sources: { name: string; kind: string; url?: string; bytes: number; truncated: boolean }[];
}

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
  /** The Geoapify key kept, which `map_fetch` fetches a map with (ADR-0030). */
  mapKey?: string;
  /** How long one turn may go without a word from the provider before it is asked again
   * (narrative REL-3); two minutes without it. */
  timeoutMs?: number;
}

/** What a person asks a model to write (PLAN 2.101): a text's words proofread, tightened, or
 * rewritten; a slide's notes drafted; an issue explained; a picture described (PLAN 2.119); a
 * slide's words checked against its beat's claim, its headline rewritten to carry it (narrative
 * FUNC-4). */
export type WritingKind = "proofread" | "tighten" | "rewrite" | "notes" | "explain" | "describe" | "claim";

/** A writing tool asked of the deck as it stands: the task, the state shown, and the text it is
 * about, or the finding to explain, as the window says it (`said`). */
export interface WritingAsk {
  kind: WritingKind;
  /** The state shown; none to explain an issue no state holds, as a source's that does not compile. */
  state?: string;
  node?: string;
  finding?: { code: string; severity: string; message: string; said?: string; hint?: string; node?: string };
  /** For a rewrite, how the person wants it rewritten, in their own words (lift-up FUNC-3). */
  instruction?: string;
}

/** What a model is asked for a writing tool (`scaena_core::writing`): the task, with the words it
 * works on where it works on a text's, or the description a picture has; what it is called; the
 * instructions and the prompt; and, for a description, the picture as the slide shows it, a PNG in
 * base64, which the model is shown (PLAN 2.119). */
export interface Writing {
  /** The task, with the words it works on where it works on a text's, the description a picture
   * has, or, for a claim check, the headline it may rewrite. */
  task: { kind: WritingKind; text?: string; headline?: string };
  title: string;
  instructions: string;
  prompt: string;
  picture?: string;
}

/** Where a slide goes in an open deck (lift-up FUNC-1): after the slide that holds a state, in
 * place of it, or, with neither, at the end. */
export interface SlidePlace {
  after?: string;
  replace?: string;
}

/** What `slide_write` did, as the MCP tool reports it: the patch's report, the slide's states,
 * the layout it took, its beat, the fixes taken, and what no fix mends. */
export interface SlideWritten {
  applied: boolean;
  errors: number;
  slide: string[];
  layout: string;
  beat: string;
  fixed: unknown[];
  left: { code?: string; message?: string }[];
}

/** An answer as it is offered, and the patch that takes it: null for one that is only read. */
export interface Written {
  text: string;
  patch: unknown[] | null;
}

/** A question asked once of the model the assistant uses (PLAN 2.101): who answers it, with the
 * user's key, and what it is told and asked. */
export interface WritingCall {
  provider: ProviderId;
  model: string;
  key: string;
  base?: string;
  instructions: string;
  prompt: string;
  /** The picture the question is about, a PNG in base64 (PLAN 2.119). */
  picture?: string;
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
  /** The model is saying `text`: more of the answer it is writing, which `text` then gives
   * whole (narrative ATTR-3). */
  | { kind: "saying"; text: string }
  /** The provider was busy, down, or silent, and the same turn is asked again after `seconds`
   * (narrative REL-3): `message` says so in a person's words, the `attempt`th of `of`. */
  | { kind: "waiting"; message: string; seconds: number; attempt: number; of: number }
  /** The model said `text`. */
  | { kind: "text"; text: string }
  /** It calls tool `name`. */
  | { kind: "call"; id: string; name: string; args: unknown }
  /** Call `id` returned: `summary` says what it came to, and `made` whether it did what its
   * words say (an edit refused, only tried, or stopped did not); `json` is what the model is
   * told, and `png` a frame it drew, in base64. */
  | { kind: "result"; id: string; name: string; error: boolean; summary: string; made: boolean; json: string; png?: string }
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

/** A run of words of a state's speaker notes in one look (`scaena_core::notes`). */
export interface NotesSpan {
  text: string;
  bold?: boolean;
  italic?: boolean;
  code?: boolean;
}

/** A block of a state's speaker notes as a presenter reads them (SPEC §2.6): lines as written,
 * each kept on its own; a list, each item at its level; or a table, its first row headings where
 * `header`. */
export type NotesBlock =
  | { kind: "lines"; lines: NotesSpan[][] }
  | { kind: "list"; ordered: boolean; items: { level: number; spans: NotesSpan[] }[] }
  | { kind: "table"; header: boolean; rows: NotesSpan[][][] };

/** A state's speaker notes as a presenter reads them: the words as the deck holds them, and the
 * blocks they read as. */
export interface SpeakerNotes {
  text: string;
  blocks: NotesBlock[];
}

/** An open bundle, as the worker reports it. */
export interface Opened {
  /** The bundle's name: its folder's, or its deck file's without `.deck.json`. */
  name: string;
  /** The deck's title (`meta.title`), where it has one: what the player's windows are called,
   * as a single file's page is. */
  title?: string;
  /** Where it is kept, if anywhere. */
  where?: Where;
  states: string[];
  formats: string[];
  /** The deck's own canvas named by its shape (`16:9`), where a format is it. A format the deck
   * lists of that shape is its own canvas, laid out as it is, so an editor offers it once, by
   * that name. */
  own?: string;
  /** Each state's speaker notes, the engine's (SPEC §2.6): its own, else its first beat's,
   * else empty; as written, and as the blocks a speaker reads. */
  notes: SpeakerNotes[];
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
/** One kind of code token as the theme draws it (PLAN 2.144): the kind as the theme keys it
 * (`keyword`) and as a person calls it (Keywords), the color it is drawn in, `#rrggbb`, the color
 * the theme names for it (none where it is derived from the accent), and whether that was moved to
 * read on its ground. */
export interface CodeColor {
  kind: string;
  said: string;
  drawn: string;
  named?: string;
  moved: boolean;
}

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
  /** The contrast errors the theme brought, each fixed: a text set in its color that reads
   * where it stands, one for each text (PLAN 2.82). */
  recolored: Finding[];
  /** The overflows and overlaps the theme brought (E100, E101), each fitted as lint's checked
   * fix fits it: a text shrunk to fit its box, one for each node. */
  refitted: Finding[];
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
  /** The data source to fetch again, where a fetch is what resolves it (W428, ADR-0030). */
  refresh?: string;
  at?: Place;
  fixable: boolean;
  /** Whether it holds in the format shown (PLAN 2.49): one lint found laying a format out holds
   * there, and the rest in every format. */
  shown: boolean;
  /** The formats it holds in, as the format menu names them: `""` for the deck's own canvas,
   * then each of the deck's `formats` (PLAN 2.62). */
  formats: string[];
  /** What it finds, in a sentence a person would say (PLAN 2.99): `The text doesn't fit its
   * box.` */
  said?: string;
  /** Its fix, as a button names it: `Shrink to Fit`, `Use a Color That Reads`, `Fix`. */
  fixing?: string;
  /** Whether the canvas marks it on its object (PLAN 3.20); the rest are listed. */
  marked?: boolean;
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
  /** The deck's own canvas named by its shape now, where a format is it (`Opened.own`): an edit
   * of the canvas changes it. */
  own?: string;
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
  /** Its transition; `style` and `direction` where it is a slide transition (PLAN 2.111), and no
   * `direction` for a dissolve, which goes no way. */
  transition: { duration: number; match: string; curve: Curve; style?: string; direction?: string };
  motions: Motion[];
}

/** An easing as its cubic Bézier, or a spring as its constants. */
export type Curve = { ease: [number, number, number, number] } | { spring: { stiffness: number; damping: number; mass: number } };

/** One motion on one node, placed on its state's clock (PLAN 2.44). */
export interface Motion {
  node: string;
  motion: "enter" | "exit" | "emphasis" | "anim";
  /** What it moves one at a time: lines, words, glyphs, paragraphs, children, or marks. */
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
  /** The theme's preset it plays, where it is written as one. */
  preset?: string;
  curve: Curve;
}

/** The worker to the page. */
export type FromWorker =
  | ({ type: "ready" } & Opened)
  /** What a brief may ask, as Narrato knows it (PLAN G.5). */
  | { type: "briefing"; choices: Choices }
  /** How far the deck being made from a prompt has come. */
  | { type: "generating"; event: GeneratingEvent }
  /** The outline the model wrote (narrative FUNC-1), on `arc`, chosen for `reasoning`: shown for
   * the person to settle (`settled`) or to outline again on another arc (`outlineAgain`). */
  | { type: "storyOutlined"; outline: StoryOutline; arc: string; reasoning: string; fallback?: string }
  /** The deck made from a prompt: the bundle's files by their paths, its name, and what it came
   * to. Kept nowhere until it is saved. */
  | { type: "generated"; files: Record<string, ArrayBuffer>; name: string; made: Generated }
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
  /** Frames are painted at `size` from now on: the canvas's attributes. */
  | { type: "fitted"; id: number; size: [number, number] }
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
  | { type: "writing"; id: number; writing: Writing }
  | { type: "written"; id: number; written: Written }
  | { type: "slideAsked"; id: number; asked: { system: string; prompt: string } }
  | { type: "slideWritten"; id: number; written: SlideWritten; source: string; edited: Edited }
  | { type: "words"; id: number; words: Lexicon }
  | { type: "notes"; id: number; notes: SpeakerNotes }
  | { type: "layoutSuggestions"; id: number; suggestions: LayoutSuggestion[] }
  | { type: "backgrounds"; id: number; backgrounds: Backgrounds }
  | { type: "backgroundFrames"; id: number; pixels: ArrayBuffer[] }
  | { type: "backgroundPatch"; id: number; patch: unknown[] }
  | { type: "look"; id: number; look: Look }
  | { type: "put"; id: number; put: Put }
  | { type: "carets"; id: number; carets: Carets | null }
  | { type: "characterChoices"; id: number; choices: Choices }
  | { type: "bolding"; id: number; look: Record<string, unknown> }
  | { type: "codeWords"; id: number; words: string }
  | { type: "tidied"; id: number; text: string }
  | { type: "keyed"; id: number; keyed: Keyed | null }
  | { type: "coding" | "uncoding" | "tidying"; id: number; patch: unknown[] }
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
  | { type: "mapTargeted"; id: number; target: MapTarget }
  | { type: "mapFetched"; id: number; places?: MapPlace[]; map?: FetchedMap }
  | { type: "mapped"; id: number; mapped: Mapped }
  | { type: "mapRefresh"; id: number; refresh: MapRefresh }
  /** Copies of several nodes (PLAN 2.42). */
  | { type: "duplicated"; id: number; duplicated: Duplicated }
  /** The words of a shape to type in, given it where it has none (PLAN 2.108). */
  | { type: "worded"; id: number; worded: Worded }
  /** A connector between two nodes (PLAN 2.109). */
  | { type: "connected"; id: number; connected: Connected }
  /** A text's lines drawn as a diagram (PLAN 2.110). */
  | { type: "diagrammed"; id: number; diagrammed: Diagrammed }
  /** The patch that takes nodes away. */
  | { type: "deleting"; id: number; patch: unknown[] }
  /** A node as the clipboard holds it, and what pasting a clip makes. */
  | { type: "copied"; id: number; clip: string }
  | { type: "pasted"; id: number; pasted: Pasted }
  | { type: "grouped"; id: number; grouped: Grouped }
  | { type: "thumbnails"; id: number; thumbs: Thumb[] }
  /** A thumbnail painted while the rest are, for a request that asked for each as it is. */
  | { type: "thumbed"; id: number; thumb: Thumb }
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
  /** A bundle the browser keeps is renamed or deleted, and the open bundle is kept `where`. */
  | { type: "rekept"; id: number; where?: Where }
  /** The bundle as a `.scaena` zip, and each font subset: its path, and its size before and
   * after, bytes. */
  | { type: "zipped"; id: number; bytes: ArrayBuffer; subset: [string, number, number][] }
  /** What `export` made: a PNG, a zip of PNGs, a PDF, or an HTML file's bytes. */
  | { type: "exported"; id: number; bytes: ArrayBuffer }
  /** The dropped file is in the bundle at `path`. */
  | { type: "dropped"; id: number; path: string }
  | { type: "pictured"; id: number; picture: Picture }
  /** The data file at `path`, read by source `data`: one the deck declares already (no
   * `attached`), or one the patch declares, or refused (`attached`, an empty patch). */
  | { type: "attached"; id: number; path: string; data: string; attached: Attached | null; patch: unknown[] }
  /** What text pasted on the canvas is as a sheet's cells; none where it is not cells. */
  | { type: "cells"; id: number; cells: Cells | null }
  /** The deck's data sources, and source `name` as a sheet, or why it does not read as one. */
  | { type: "sheet"; id: number; sources: DataSource[]; name?: string; sheet?: Sheet; file?: string; why?: string }
  | { type: "bundleFiles"; id: number; files: BundleFile[]; grounded: Grounded[] }
  | { type: "health"; id: number; health: Health }
  /** A picture made: its bytes, its file's kind (`png`, `jpg`), what it shows, and the model. */
  | { type: "pictureMade"; id: number; bytes: ArrayBuffer; ext: string; alt: string; model: string }
  /** A file taken out: what the deck came to, shown and linted as an edit is. */
  | { type: "removed"; id: number; edited: Edited }
  /** The bundle's versions, oldest first; none where it keeps no history. */
  | { type: "versions"; id: number; versions: Version[] | null }
  /** A version shown: its states, each as a person counts it (`said`, PLAN 2.99), and the one
   * drawn, as a PNG; or why it is not drawn (a file it names the bundle no longer holds). */
  | { type: "version"; id: number; states: string[]; said: string[]; state: string; png?: ArrayBuffer; why?: string }
  | { type: "compared"; id: number; compared: Compared }
  /** What restoring a version did; where it did, each data file it wrote, before and after, the
   * deck's source after, and what the edit came to. */
  | { type: "restored"; id: number; restored: Restored; files: Rewritten[]; source?: string; edited?: Edited }
  | { type: "filesWritten"; id: number; edited?: Edited }
  /** The file asked for, or null where the bundle holds none at its path. */
  | { type: "file"; id: number; bytes: ArrayBuffer | null }
  | { type: "themeText"; id: number; theme: ThemeText | null }
  | { type: "codeColors"; id: number; colors: CodeColor[] }
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
  /** What a model wrote, asked once (PLAN 2.101), and the tokens it took. */
  | { type: "wrote"; id: number; text: string; usage?: { input: number; output: number } }
  /** The CPU painter met a shader whose rows `count` more workers could share (PLAN 2.28): the
   * page starts each as it started this one, and hands this one a port to each (`helpers`). */
  | { type: "helpers"; count: number }
  /** The formats beside the canvas show `state` `t` ms into its cue (PLAN 2.62). */
  | { type: "besides"; state: string; t: number }
  /** The canvas's attributes are `size` from now on: frames are painted at it, the slide fitted
   * to the room (lift-up ATTR-2), the format shown, or the bundle read again changing it. */
  | { type: "sized"; size: [number, number] }
  /** Request `id` failed, or, without one, opening or playing did. `webgpu`: setting
   * WebGPU up failed, and the CPU painter may still paint. */
  | { type: "error"; id?: number; message: string; webgpu?: boolean }
  /** The engine stopped on a panic (lift-up STAB-2), which wasm32 does not unwind from: `message`
   * the panic's words, and the bundle's files as the worker held them, with its name, for a fresh
   * worker to go on from. The worker answers every request after with an error. */
  | { type: "crashed"; message: string; name: string; files: Record<string, ArrayBuffer> };
