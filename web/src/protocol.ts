// What the pages and the engine's worker say to each other (PLAN 2.1–2.3, SPEC §9.2).
// Each request names the format it is in: one of the deck's `formats`, or the deck's own
// canvas when it names none (SPEC §3.4).

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
  /** Load the bundle whose deck file is at `deck`, and paint into `canvas` by `painter`. */
  | { type: "open"; deck: string; painter: Painter; canvas: OffscreenCanvas }
  /** Paint `state` `t` ms into its cue, or at rest without `t`. */
  | { type: "show"; id: number; state: string; t?: number; format?: string }
  /** The deck's timeline. */
  | { type: "timeline"; id: number; format?: string }
  /** Play the deck from slot `index`, `t` ms into its cue, a frame each time the display
   * takes one: each cue, then its state's hold, then the next state's cue. A state that does
   * not hold, and the last, comes to rest and waits there. */
  | { type: "run"; index: number; t: number; format?: string }
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
  | { type: "inspect"; id: number; state: string; format?: string };

/** An open bundle, as the worker reports it. */
export interface Opened {
  states: string[];
  formats: string[];
  /** Each state's speaker notes: its own, else its beat's (SPEC §3.11), else empty. */
  notes: string[];
  /** What paints: vello on WebGPU, or vello_cpu. */
  painter: "webgpu" | "cpu";
  /** The adapter WebGPU paints with, as far as the browser tells. */
  adapter: string;
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
  /** A frame of `run` is on the canvas, or the one request `id` sought; posted for each, and
   * when the clock stops. */
  | ({ type: "at"; id?: number } & At)
  | { type: "source"; id: number; source: string }
  | ({ type: "edited"; id: number } & Edited)
  | ({ type: "linted"; id: number } & Linted)
  | { type: "fixed"; id: number; source: string }
  | { type: "inspected"; id: number; inspected: Inspected }
  /** Request `id` failed, or, without one, opening or playing did. `webgpu`: setting
   * WebGPU up failed, and the CPU painter may still paint. */
  | { type: "error"; id?: number; message: string; webgpu?: boolean };
