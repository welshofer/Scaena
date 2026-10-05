// One canvas and the engine's worker that paints it (PLAN 2.1–2.4): the page's side of
// `protocol.ts`. The player shows one; the presenter view, two; the editor, one.
import type {
  Asking,
  AssistantEvent,
  At,
  Edited,
  FromWorker,
  Hit,
  Inspected,
  Linted,
  NodeBox,
  Opened,
  Painter,
  ProviderId,
  Rect,
  SaveTo,
  Slot,
  Snapped,
  SnapMode,
  Source,
  Targets,
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
      | "reloaded"
      | "boxes"
      | "hits"
      | "targets"
      | "dragged"
      | "placed";
  }
>;

/** The most helpers a stage starts for the CPU painter (PLAN 2.28): the page's `?helpers=`, a
 * whole number (0: the engine's worker works out every row itself), or as many as it asks. */
const mostHelpers = (() => {
  const n = Number(new URLSearchParams(location.search).get("helpers") ?? Number.NaN);
  return Number.isInteger(n) && n >= 0 ? n : Number.POSITIVE_INFINITY;
})();

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

  /** Workers that work out shaders' rows for the engine's worker, when it asks (PLAN 2.28). */
  private helpers: Worker[] = [];

  private constructor(
    readonly canvas: HTMLCanvasElement,
    private worker: Worker,
    /** The bundle as the worker opened it, or last read it again. */
    public opened: Opened,
    /** How the engine's worker was started: its helpers start the same way. */
    private engine: Engine,
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
      return new Stage(fresh, worker, opened, engine);
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

  /** Each visible node's box in `state` at rest, in `format`, and the canvas's size, canvas
   * units (ADR-0013). */
  boxes(state: string, format?: string): Promise<{ boxes: NodeBox[]; size: [number, number] }> {
    return this.request<"boxes">({ type: "boxes", id: ++this.asked, state, format });
  }

  /** The nodes that draw at `point` in `state` at rest, topmost first. */
  hit(state: string, point: [number, number], format?: string): Promise<Hit[]> {
    return this.request<"hits">({ type: "hit", id: ++this.asked, state, point, format }).then(({ hits }) => hits);
  }

  /** Where `node` may go in `state` at rest. */
  targets(state: string, node: string, format?: string): Promise<Targets> {
    return this.request<"targets">({ type: "targets", id: ++this.asked, state, node, format }).then(({ targets }) => targets);
  }

  /** A drag's move: `node` painted `by` from where it stands, where its box would land `snap`ped,
   * and the states that patch changes; or, to `preview`, the state as the patch would make it. */
  drag(
    state: string,
    node: string,
    move: { by?: [number, number]; snap?: { how: SnapMode; to: Rect; fork: boolean }; preview?: boolean },
    format?: string,
  ): Promise<{ snapped?: Snapped | null; states?: string[] }> {
    return this.request<"dragged">({ type: "drag", id: ++this.asked, state, node, ...move, format });
  }

  /** A drag is over and changes nothing: `state` at rest as it stands. */
  rest(state: string, format?: string): Promise<void> {
    return this.request<"shown">({ type: "rest", id: ++this.asked, state, format }).then(() => {});
  }

  /** Make `ops` by the user on the deck `source` compiles to, and show slot `index` from it: the
   * deck's source after, and what the edit came to. */
  place(source: string, ops: unknown[], index: number, format?: string): Promise<{ source: string; edited: Edited }> {
    return this.request<"placed">({ type: "place", id: ++this.asked, source, ops, index, format }).then(({ source, edited }) => {
      if (edited.at) {
        this.at = edited.at;
        this.onAt(edited.at);
      }
      return { source, edited };
    });
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
    for (const helper of this.helpers) helper.terminate();
    this.helpers = [];
    this.waiting.clear();
    this.hearing.clear();
  }

  /** Start `count` workers to work out shaders' rows beside the engine's (PLAN 2.28), each as
   * the engine's worker was started, and hand it a port to each: no more than `?helpers=`
   * allows. One that fails to start is stopped: the engine's worker works its rows out itself. */
  private help(count: number) {
    const ports: MessagePort[] = [];
    for (let i = 0; i < Math.min(count, mostHelpers); i++) {
      const helper = this.engine.spawn();
      helper.onerror = (e) => {
        console.warn(`a shader helper stopped: ${e.message}`);
        helper.terminate();
      };
      const { port1, port2 } = new MessageChannel();
      helper.postMessage({ type: "help", port: port2 } satisfies ToWorker, [port2]);
      this.helpers.push(helper);
      ports.push(port1);
    }
    this.worker.postMessage({ type: "helpers", ports } satisfies ToWorker, ports);
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
      case "boxes":
      case "hits":
      case "targets":
      case "dragged":
      case "placed":
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
      case "helpers":
        return this.help(data.count);
      case "ready":
        return;
    }
  }
}
