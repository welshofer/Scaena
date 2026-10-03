// What the player page and the engine's worker say to each other (PLAN 2.1–2.2, SPEC §9.2).
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
  | { type: "pause" };

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

/** The worker to the page. */
export type FromWorker =
  | ({ type: "ready" } & Opened)
  /** The frame request `id` asked for is on the canvas, `size` pixels, painted in `ms`. */
  | { type: "shown"; id: number; size: [number, number]; ms: number }
  | { type: "timeline"; id: number; slots: Slot[] }
  /** A frame of `run` is on the canvas, or the one request `id` sought; posted for each, and
   * when the clock stops. */
  | ({ type: "at"; id?: number } & At)
  /** Request `id` failed, or, without one, opening or playing did. `webgpu`: setting
   * WebGPU up failed, and the CPU painter may still paint. */
  | { type: "error"; id?: number; message: string; webgpu?: boolean };
