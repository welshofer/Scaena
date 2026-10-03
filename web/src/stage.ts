// One canvas and the engine's worker that paints it (PLAN 2.1–2.3): the page's side of
// `protocol.ts`. The player shows one; the presenter view, two; the editor, one.
import type { At, Edited, FromWorker, Inspected, Linted, Opened, Painter, Slot, ToWorker } from "./protocol";

type Reply = Extract<
  FromWorker,
  { type: "shown" | "timeline" | "at" | "source" | "edited" | "linted" | "fixed" | "inspected" }
>;

export class Stage {
  /** Where the deck is, as the worker last said. */
  at: At = { index: 0, t: 0, global: 0, playing: false };
  /** Called as the deck moves: each frame of a run, and each seek. */
  onAt: (at: At) => void = () => {};
  /** Called when the worker fails outside a request. */
  onError: (error: Error) => void = () => {};
  /** Requests not yet answered, by id. */
  private waiting = new Map<number, { resolve: (reply: Reply) => void; reject: (error: Error) => void }>();
  private asked = 0;

  private constructor(
    readonly canvas: HTMLCanvasElement,
    private worker: Worker,
    readonly opened: Opened,
  ) {
    worker.onmessage = ({ data }: MessageEvent<FromWorker>) => this.listen(data);
    worker.onerror = (e) => this.onError(new Error(e.message));
  }

  /** Open the deck at `deck` in a new worker, which paints into a new canvas in `canvas`'s
   * place: a canvas WebGPU has held takes no other painter, so falling back to the CPU
   * painter starts over. */
  static async open(canvas: HTMLCanvasElement, deck: string, painter: Painter): Promise<Stage> {
    const fresh = canvas.cloneNode() as HTMLCanvasElement;
    canvas.replaceWith(fresh);
    const worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
    const reply = new Promise<FromWorker>((resolve, reject) => {
      worker.onmessage = ({ data }: MessageEvent<FromWorker>) => resolve(data);
      worker.onerror = (e) => reject(new Error(e.message || "the worker failed to start"));
    });
    const offscreen = fresh.transferControlToOffscreen();
    worker.postMessage({ type: "open", deck, painter, canvas: offscreen } satisfies ToWorker, [offscreen]);
    const first = await reply;
    if (first.type === "ready") {
      const { type: _, ...opened } = first;
      return new Stage(fresh, worker, opened);
    }
    worker.terminate();
    const message = first.type === "error" ? first.message : `the worker said ${first.type} first`;
    if (first.type === "error" && first.webgpu && painter === "auto") {
      console.warn(`${message}; the CPU painter paints instead`);
      return Stage.open(fresh, deck, "cpu");
    }
    throw new Error(message);
  }

  /** Paint `state` `t` ms into its cue (at rest without `t`), in `format` or on the deck's
   * own canvas. Resolves once the frame is on the canvas. */
  show(state: string, t?: number, format?: string): Promise<{ size: [number, number]; ms: number }> {
    return this.ask<"shown">({ type: "show", id: ++this.asked, state, t, format }).then(({ size, ms }) => ({ size, ms }));
  }

  /** The deck's timeline in `format`, or on its own canvas. */
  timeline(format?: string): Promise<Slot[]> {
    return this.ask<"timeline">({ type: "timeline", id: ++this.asked, format }).then(({ slots }) => slots);
  }

  /** Play from slot `index`, `t` ms into its cue, until a state that waits comes to rest. */
  run(index: number, t = 0, format?: string) {
    this.send({ type: "run", index, t, format });
  }

  /** Show slot `index` `t` ms into its cue (at rest without `t`), still. Resolves once it is
   * on the canvas. */
  seek(index: number, t?: number, format?: string): Promise<At> {
    return this.ask<"at">({ type: "seek", id: ++this.asked, index, t, format }).then(() => this.at);
  }

  pause() {
    this.send({ type: "pause" });
  }

  /** The deck as canonical `.scn`. */
  source(): Promise<string> {
    return this.ask<"source">({ type: "source", id: ++this.asked }).then(({ source }) => source);
  }

  /** Compile `source`; once it validates, show slot `index` from it at rest, and lint that
   * slot's state. */
  edit(source: string, index: number, format?: string): Promise<Edited> {
    return this.ask<"edited">({ type: "edit", id: ++this.asked, source, index, format }).then((edited) => {
      if (edited.at) {
        this.at = edited.at;
        this.onAt(edited.at);
      }
      return edited;
    });
  }

  /** Lint the deck compiled last, laying out every state. */
  lint(): Promise<Linted> {
    return this.ask<"linted">({ type: "lint", id: ++this.asked });
  }

  /** The source compiled last with `patch`, a finding's fix, applied. */
  fix(patch: unknown[]): Promise<string> {
    return this.ask<"fixed">({ type: "fix", id: ++this.asked, patch }).then(({ source }) => source);
  }

  /** `state` inspected, in `format` or on the deck's own canvas. */
  inspect(state: string, format?: string): Promise<Inspected> {
    return this.ask<"inspected">({ type: "inspect", id: ++this.asked, state, format }).then(({ inspected }) => inspected);
  }

  private send(message: ToWorker) {
    this.worker.postMessage(message);
  }

  private ask<T extends Reply["type"]>(message: Extract<ToWorker, { id: number }>): Promise<Extract<Reply, { type: T }>> {
    return new Promise((resolve, reject) => {
      this.waiting.set(message.id, { resolve: resolve as (reply: Reply) => void, reject });
      this.send(message);
    });
  }

  private listen(data: FromWorker) {
    switch (data.type) {
      case "shown":
      case "timeline":
      case "source":
      case "edited":
      case "linted":
      case "fixed":
      case "inspected":
        this.waiting.get(data.id)?.resolve(data);
        this.waiting.delete(data.id);
        return;
      case "at": {
        const { type: _, id, ...at } = data;
        this.at = at;
        this.onAt(at);
        if (id !== undefined) {
          this.waiting.get(id)?.resolve(data);
          this.waiting.delete(id);
        }
        return;
      }
      case "error": {
        const waiting = data.id === undefined ? undefined : this.waiting.get(data.id);
        if (waiting) {
          waiting.reject(new Error(data.message));
          this.waiting.delete(data.id!);
        } else this.onError(new Error(data.message));
        return;
      }
      case "ready":
        return;
    }
  }
}
