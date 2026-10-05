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
}

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
  node: string;
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
}

/** Where a caret stands in a text at rest (ADR-0013, PLAN 2.32): each character as written, as a
 * reader counts it, on its line, from the glyphs the engine set. Offsets are UTF-16 code units of
 * `text`, as the page counts a string; x and y are canvas units. */
export interface Carets {
  /** The text as written: the node's `text`, or its runs' texts end to end. */
  text: string;
  lines: CaretLine[];
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

/** What an inspector offers for a state itself (PLAN 2.36), as `scaena inspect --state-choices`
 * says it: its layout, each key of its transition, its hold, and its notes. */
export interface StateChoices {
  state: string;
  /** What `set_state` names. A state that sets no key of its transition cuts in. */
  fields: Field[];
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
  /** One node, listed just before or just after another child of its container (PLAN 2.50). */
  | { before: string }
  | { after: string }
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
      snap?: { how: SnapMode; to: Rect; fork: boolean };
      /** With `with`: where they all land, moved `by` together, `free` off the grid. */
      together?: { by: [number, number]; free: boolean; fork: boolean };
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
  | { type: "inserting"; id: number; source: string; state: string; n: number; at: [number, number]; format?: string }
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
  | { type: "thumbnails"; id: number; height: number; known: Record<string, string>; format?: string }
  /** The patch that adds a state after `state`, on the deck the editor's `source` compiles to: a
   * `step` of its slide, or a `slide` of its own (PLAN 2.35). */
  | { type: "addingState"; id: number; source: string; state: string; what: "step" | "slide" }
  /** Text typed on the canvas (PLAN 2.32): `ops`, a `replace_text`, made by the user on the deck
   * the editor's `source` compiles to, validated but not linted; then the deck's source, shown
   * at slot `index` and linted as an edit of it is, and where a caret stands in `node`'s text in
   * `state` now. */
  | { type: "type"; id: number; source: string; ops: unknown[]; index: number; state: string; node: string; format?: string }
  /** Save the bundle with the deck `source` compiles to as `scaena save` does (SPEC §3.1),
   * fonts kept whole, where it is kept; one kept nowhere goes into the browser's storage under
   * its name (`name-2`, … where that is taken). A source that does not compile, or a deck
   * that does not validate, is not saved. The session goes on from the save (PLAN 2.4). */
  | { type: "save"; id: number; source: string }
  /** Save as (PLAN 2.12): the bundle saved as `save` saves it, but `to` a place of its own,
   * where it is kept from then on. */
  | { type: "saveAs"; id: number; source: string; to: SaveTo }
  /** The bundle with the deck `source` compiles to, saved as a `.scaena` zip, fonts subset to
   * what the deck draws. */
  | { type: "zip"; id: number; source: string }
  /** A file dropped on the page, into the bundle: where it goes is what it is, and an image
   * is named by its SHA-256 (`Player.place`). */
  | { type: "drop"; id: number; name: string; bytes: ArrayBuffer }
  /** Ask the assistant (PLAN 2.6): the editor's `source` must compile to a deck that
   * validates, which its tools then work on. Each step comes back as an `assistant` event,
   * until one that is `done` or `failed`. */
  | { type: "ask"; id: number; source: string; ask: Asking }
  /** Stop the assistant: the call it is in finishes, and it says no more. */
  | { type: "stop" }
  /** Start a new conversation with the assistant. */
  | { type: "forget" }
  /** The models `key` can use at `provider`. */
  | { type: "models"; id: number; provider: ProviderId; key: string; base?: string }
  /** The workers `helpers` asked for: a port to each (PLAN 2.28). */
  | { type: "helpers"; ports: MessagePort[] }
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
 * `base`, with the user's key, and the model they picked. */
export interface Asking {
  provider: ProviderId;
  model: string;
  key: string;
  base?: string;
  text: string;
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
   * nothing of a source the assistant has moved past. */
  | { kind: "edited"; source: string; edited: Edited }
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
   * what the last lint of every state found in them. */
  whole: boolean;
  /** The deck's timeline now. */
  slots: Slot[];
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
  /** The preview is painted through the view asked for. */
  | { type: "viewed"; id: number }
  | { type: "found"; id: number; found: Found[] }
  /** The ops of a replacement, a `replace_text` for each match it replaces. */
  | { type: "replacement"; id: number; ops: unknown[] }
  /** Where a focal point picked there would be; `null` off the image. */
  | { type: "focal"; id: number; at: [number, number] | null }
  | { type: "targets"; id: number; targets: Targets }
  /** Where a drag's box would land (`null`: nowhere that way), and the states its patch changes. */
  | { type: "dragged"; id: number; snapped?: Snapped | null; states?: string[] }
  | { type: "arranged"; id: number; arranged: Arranged | null }
  /** The patch is made: the deck's source now, and what the edit came to. */
  | { type: "made"; id: number; source: string; edited: Edited }
  | { type: "choices"; id: number; choices: Choices }
  | { type: "stateChoices"; id: number; choices: StateChoices }
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
  /** The text is typed: the deck's source now, what the edit came to, and the text's carets. */
  | { type: "typed"; id: number; source: string; edited: Edited; carets: Carets | null }
  /** The bundle is saved `where`, and the session goes on from it: the files the save
   * renamed, from and to, each a path the source may name; and how many files it wrote. */
  | { type: "saved"; id: number; where: Where; renamed: [string, string][]; files: number; recorded: boolean }
  /** The bundle as a `.scaena` zip, and each font subset: its path, and its size before and
   * after, bytes. */
  | { type: "zipped"; id: number; bytes: ArrayBuffer; subset: [string, number, number][] }
  /** The dropped file is in the bundle at `path`. */
  | { type: "dropped"; id: number; path: string }
  /** A step of the assistant's answer to `ask` request `id`. */
  | { type: "assistant"; id: number; event: AssistantEvent }
  | { type: "models"; id: number; models: string[] }
  /** The CPU painter met a shader whose rows `count` more workers could share (PLAN 2.28): the
   * page starts each as it started this one, and hands this one a port to each (`helpers`). */
  | { type: "helpers"; count: number }
  /** Request `id` failed, or, without one, opening or playing did. `webgpu`: setting
   * WebGPU up failed, and the CPU painter may still paint. */
  | { type: "error"; id?: number; message: string; webgpu?: boolean };
