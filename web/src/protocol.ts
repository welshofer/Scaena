// What the player page and the engine's worker say to each other (PLAN 2.1, SPEC §9.2).
// Each request names the format it is in: one of the deck's `formats`, or the deck's own
// canvas when it names none (SPEC §3.4).

/** Who paints: WebGPU where the browser has an adapter, else the CPU painter (`auto`), or
 * one of them whatever the browser has (`gpu`, `cpu`). */
export type Painter = "auto" | "gpu" | "cpu";

/** A state's place on the deck's timeline, ms (SPEC §2.4): where its cue starts, how long
 * its transition and motions run, and how long it then holds. */
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
  /** Play the cue into `state`, a frame each time the display takes one, then rest there. */
  | { type: "play"; state: string; format?: string }
  /** The deck's timeline. */
  | { type: "timeline"; id: number; format?: string };

/** An open bundle, as the worker reports it. */
export interface Opened {
  states: string[];
  formats: string[];
  /** What paints: vello on WebGPU, or vello_cpu. */
  painter: "webgpu" | "cpu";
  /** The adapter WebGPU paints with, as far as the browser tells. */
  adapter: string;
}

/** The worker to the page. */
export type FromWorker =
  | ({ type: "ready" } & Opened)
  /** The frame request `id` asked for is on the canvas, `size` pixels, painted in `ms`. */
  | { type: "shown"; id: number; size: [number, number]; ms: number }
  | { type: "timeline"; id: number; slots: Slot[] }
  /** A cue `play` started has come to rest. */
  | { type: "rested"; state: string }
  /** Request `id` failed, or, without one, opening or playing did. `webgpu`: setting
   * WebGPU up failed, and the CPU painter may still paint. */
  | { type: "error"; id?: number; message: string; webgpu?: boolean };
