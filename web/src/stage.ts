// One canvas and the engine's worker that paints it (PLAN 2.1–2.4): the page's side of
// `protocol.ts`. The player shows one; the presenter view, two; the editor, one.
import type {
  Asking,
  AssistantEvent,
  At,
  Edited,
  FromWorker,
  Inspected,
  Linted,
  Opened,
  Painter,
  ProviderId,
  SaveTo,
  Slot,
  Source,
  ToWorker,
} from "./protocol";

type Reply = Extract<
  FromWorker,
  {
    type:
      | "shown"
      | "timeline"
      | "reading"
      | "at"
      | "source"
      | "edited"
      | "linted"
      | "fixed"
      | "inspected"
      | "saved"
      | "zipped"
      | "dropped"
      | "models"
      | "reloaded";
  }
>;

/** How a page starts the engine: a worker, and the engine's module, compiled, if the page
 * carries it, as a single-file export does (PLAN 2.5). A worker without it loads its own. */
export interface Engine {
  spawn: () => Worker;
  module?: WebAssembly.Module;
}

export class Stage {
  /** Where the deck is, as the worker last said. */
  at: At = { index: 0, t: 0, global: 0, playing: false };
  /** Called as the deck moves: each frame of a run, and each seek. */
  onAt: (at: At) => void = () => {};
  /** Called when the worker fails outside a request. */
  onError: (error: Error) => void = () => {};
  /** Requests not yet answered, by id. */
  private waiting = new Map<number, { resolve: (reply: Reply) => void; reject: (error: Error) => void }>();
  /** What hears each step of the assistant's answer to a question, by the question's id. */
  private hearing = new Map<number, (event: AssistantEvent) => void>();
  private asked = 0;

  private constructor(
    readonly canvas: HTMLCanvasElement,
    private worker: Worker,
    /** The bundle as the worker opened it, or last read it again. */
    public opened: Opened,
  ) {
    worker.onmessage = ({ data }: MessageEvent<FromWorker>) => this.listen(data);
    worker.onerror = (e) => this.onError(new Error(e.message));
  }

  /** Open the bundle at `source` in a new worker of `engine`'s, which paints into a new
   * canvas in `canvas`'s place: a canvas WebGPU has held takes no other painter, so falling
   * back to the CPU painter starts over. */
  static async open(canvas: HTMLCanvasElement, source: Source, painter: Painter, engine: Engine): Promise<Stage> {
    const fresh = canvas.cloneNode() as HTMLCanvasElement;
    canvas.replaceWith(fresh);
    const worker = engine.spawn();
    const reply = new Promise<FromWorker>((resolve, reject) => {
      worker.onmessage = ({ data }: MessageEvent<FromWorker>) => resolve(data);
      worker.onerror = (e) => reject(new Error(e.message || "the worker failed to start"));
    });
    const offscreen = fresh.transferControlToOffscreen();
    const open: ToWorker = { type: "open", source, painter, canvas: offscreen, engine: engine.module };
    worker.postMessage(open, [offscreen]);
    const first = await reply;
    if (first.type === "ready") {
      const { type: _, ...opened } = first;
      return new Stage(fresh, worker, opened);
    }
    worker.terminate();
    const message = first.type === "error" ? first.message : `the worker said ${first.type} first`;
    if (first.type === "error" && first.webgpu && painter === "auto") {
      console.warn(`${message}; the CPU painter paints instead`);
      return Stage.open(fresh, source, "cpu", engine);
    }
    throw new Error(message);
  }

  /** Paint `state` `t` ms into its cue (at rest without `t`), in `format` or on the deck's
   * own canvas. Resolves once the frame is on the canvas. */
  show(state: string, t?: number, format?: string): Promise<{ size: [number, number]; ms: number }> {
    return this.request<"shown">({ type: "show", id: ++this.asked, state, t, format }).then(({ size, ms }) => ({ size, ms }));
  }

  /** The deck's timeline in `format`, or on its own canvas. */
  timeline(format?: string): Promise<Slot[]> {
    return this.request<"timeline">({ type: "timeline", id: ++this.asked, format }).then(({ slots }) => slots);
  }

  /** How `state` reads at rest in `format`, as HTML (SPEC §3.12). */
  reading(state: string, format?: string): Promise<string> {
    return this.request<"reading">({ type: "read", id: ++this.asked, state, format }).then(({ html }) => html);
  }

  /** Play from slot `index`, `t` ms into its cue, until a state that waits comes to rest:
   * `still`, each cue a cut to its state at rest, at the deck's pace (PLAN 2.8). */
  run(index: number, t = 0, format?: string, still?: boolean) {
    this.send({ type: "run", index, t, format, still });
  }

  /** Show slot `index` `t` ms into its cue (at rest without `t`), still. Resolves once it is
   * on the canvas. */
  seek(index: number, t?: number, format?: string): Promise<At> {
    return this.request<"at">({ type: "seek", id: ++this.asked, index, t, format }).then(() => this.at);
  }

  pause() {
    this.send({ type: "pause" });
  }

  /** Read the bundle again from its URL, on the same canvas (PLAN 2.11). */
  async reload(): Promise<Opened> {
    const { type: _, id: __, ...opened } = await this.request<"reloaded">({ type: "reload", id: ++this.asked });
    this.opened = opened;
    return opened;
  }

  /** The deck as canonical `.scn`. */
  source(): Promise<string> {
    return this.request<"source">({ type: "source", id: ++this.asked }).then(({ source }) => source);
  }

  /** Compile `source`; once it validates, show slot `index` from it at rest, and lint that
   * slot's state. */
  edit(source: string, index: number, format?: string): Promise<Edited> {
    return this.request<"edited">({ type: "edit", id: ++this.asked, source, index, format }).then((edited) => {
      if (edited.at) {
        this.at = edited.at;
        this.onAt(edited.at);
      }
      return edited;
    });
  }

  /** Lint the deck compiled last, laying out every state. */
  lint(): Promise<Linted> {
    return this.request<"linted">({ type: "lint", id: ++this.asked });
  }

  /** The source compiled last with `patch`, a finding's fix, applied. */
  fix(patch: unknown[]): Promise<string> {
    return this.request<"fixed">({ type: "fix", id: ++this.asked, patch }).then(({ source }) => source);
  }

  /** `state` inspected, in `format` or on the deck's own canvas. */
  inspect(state: string, format?: string): Promise<Inspected> {
    return this.request<"inspected">({ type: "inspect", id: ++this.asked, state, format }).then(({ inspected }) => inspected);
  }

  /** Save the bundle with the deck `source` compiles to where it is kept, or into the
   * browser's storage (PLAN 2.4). The session goes on from the save. */
  save(source: string): Promise<Extract<FromWorker, { type: "saved" }>> {
    return this.request<"saved">({ type: "save", id: ++this.asked, source });
  }

  /** Save as (PLAN 2.12): `source` saved `to` a place of its own, where the bundle is kept from
   * then on. */
  saveAs(source: string, to: SaveTo): Promise<Extract<FromWorker, { type: "saved" }>> {
    return this.request<"saved">({ type: "saveAs", id: ++this.asked, source, to });
  }

  /** The bundle with the deck `source` compiles to, as a `.scaena` zip with its fonts subset. */
  zip(source: string): Promise<{ bytes: ArrayBuffer; subset: [string, number, number][] }> {
    return this.request<"zipped">({ type: "zip", id: ++this.asked, source });
  }

  /** Add a file dropped on the page to the bundle; resolves to its path there. */
  drop(name: string, bytes: ArrayBuffer): Promise<string> {
    return this.request<"dropped">({ type: "drop", id: ++this.asked, name, bytes }).then(({ path }) => path);
  }

  /** Ask the assistant `asking.text` about the deck `source` says, which must compile and
   * validate (PLAN 2.6). `hear` hears each step; the promise holds the last, `done` or
   * `failed`, and fails if the question could not be asked. */
  ask(source: string, asking: Asking, hear: (event: AssistantEvent) => void): Promise<AssistantEvent> {
    const id = ++this.asked;
    return new Promise((resolve, reject) => {
      this.hearing.set(id, (event) => {
        hear(event);
        if (event.kind === "done" || event.kind === "failed") {
          this.hearing.delete(id);
          this.waiting.delete(id);
          resolve(event);
        }
      });
      this.waiting.set(id, { resolve: () => {}, reject: (e) => (this.hearing.delete(id), reject(e)) });
      this.send({ type: "ask", id, source, ask: asking });
    });
  }

  /** Stop the assistant: the call it is in finishes, and it says no more. */
  stop() {
    this.send({ type: "stop" });
  }

  /** Start a new conversation with the assistant. */
  forget() {
    this.send({ type: "forget" });
  }

  /** The models `key` can use at `provider`, at its own address or `base`. */
  models(provider: ProviderId, key: string, base?: string): Promise<string[]> {
    return this.request<"models">({ type: "models", id: ++this.asked, provider, key, base }).then(({ models }) => models);
  }

  /** Stop the worker: the page opens another bundle. What was asked of it is never answered:
   * what asked it is closed too. */
  close() {
    this.worker.terminate();
    this.waiting.clear();
    this.hearing.clear();
  }

  private send(message: ToWorker) {
    this.worker.postMessage(message);
  }

  private request<T extends Reply["type"]>(message: Extract<ToWorker, { id: number }>): Promise<Extract<Reply, { type: T }>> {
    return new Promise((resolve, reject) => {
      this.waiting.set(message.id, { resolve: resolve as (reply: Reply) => void, reject });
      this.send(message);
    });
  }

  private listen(data: FromWorker) {
    switch (data.type) {
      case "shown":
      case "timeline":
      case "reading":
      case "source":
      case "edited":
      case "linted":
      case "fixed":
      case "inspected":
      case "saved":
      case "zipped":
      case "dropped":
      case "models":
      case "reloaded":
        this.waiting.get(data.id)?.resolve(data);
        this.waiting.delete(data.id);
        return;
      case "assistant":
        this.hearing.get(data.id)?.(data.event);
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
