// What the pages and the engine's worker say to each other (PLAN 2.1–2.4, SPEC §9.2).
// Each request names the format it is in: one of the deck's `formats`, or the deck's own
// canvas when it names none (SPEC §3.4).

/** Where a bundle comes from (SPEC §3.1, §9.2). */
export type Source =
  /** A deck file at a URL, whose directory is the bundle: it reads the files the deck names,
   * and every file its manifest lists. */
  | { url: string }
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
  | { files: Record<string, ArrayBuffer>; name: string; states?: string[] };

/** Where an open bundle is kept, and saves to: a folder on disk, or the browser's storage,
 * by name. A bundle read from a URL is kept nowhere until it is saved. */
export interface Where {
  kind: "folder" | "opfs";
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
  | { type: "run"; index: number; t: number; format?: string; still?: boolean }
  /** Slot `index`, `t` ms into its cue (at rest without `t`), still. */
  | { type: "seek"; id: number; index: number; t?: number; format?: string }
  /** Stop where the deck is. */
  | { type: "pause" }
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
  /** Save the bundle with the deck `source` compiles to as `scaena save` does (SPEC §3.1),
   * fonts kept whole, where it is kept; one kept nowhere goes into the browser's storage under
   * its name (`name-2`, … where that is taken). A source that does not compile, or a deck
   * that does not validate, is not saved. The session goes on from the save (PLAN 2.4). */
  | { type: "save"; id: number; source: string }
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
  | { type: "models"; id: number; provider: ProviderId; key: string; base?: string };

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
  timeline?: {
    start: number;
    span: number;
    hold: number;
    transition: { duration: number; match: string };
    motions: { node: string; motion: string; units: number; start: number; end: number }[];
  };
}

/** The worker to the page. */
export type FromWorker =
  | ({ type: "ready" } & Opened)
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
  /** Request `id` failed, or, without one, opening or playing did. `webgpu`: setting
   * WebGPU up failed, and the CPU painter may still paint. */
  | { type: "error"; id?: number; message: string; webgpu?: boolean };
