// One canvas and the engine's worker that paints it (PLAN 2.1–2.4): the page's side of
// `protocol.ts`. The player shows one; the presenter view, two; the editor, one.
import type {
  Added,
  Asking,
  AssistantEvent,
  At,
  Edited,
  FromWorker,
  Hit,
  Insert,
  Inspected,
  Linted,
  NodeBox,
  Opened,
  Painter,
  Pasted,
  ProviderId,
  Rect,
  SaveTo,
  Slot,
  Snapped,
  SnapMode,
  Source,
  Targets,
  Themed,
  Themes,
  Thumb,
  ToWorker,
  Carets,
  Choices,
  StateChoices,
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
      | "made"
      | "choices"
      | "stateChoices"
      | "carets"
      | "characterChoices"
      | "bolding"
      | "italicizing"
      | "themes"
      | "rethemed"
      | "reached"
      | "typed"
      | "inserts"
      | "adding"
      | "deleting"
      | "copied"
      | "pasted"
      | "thumbnails"
      | "addingState";
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
  make(source: string, ops: unknown[], index: number, format?: string): Promise<{ source: string; edited: Edited }> {
    return this.request<"made">({ type: "make", id: ++this.asked, source, ops, index, format }).then(({ source, edited }) => {
      if (edited.at) {
        this.at = edited.at;
        this.onAt(edited.at);
      }
      return { source, edited };
    });
  }

  /** Where a caret stands in `node`'s text in `state` at rest, in the deck `source` compiles to;
   * `null` for a node that is no text. */
  carets(source: string, state: string, node: string, format?: string): Promise<Carets | null> {
    return this.request<"carets">({ type: "carets", id: ++this.asked, source, state, node, format }).then(({ carets }) => carets);
  }

  /** What an inspector offers for `node` as `state` shows it (PLAN 2.33): each property it edits,
   * the value shown and where it lives, and the theme's names for it. */
  choices(state: string, node: string): Promise<Choices> {
    return this.request<"choices">({ type: "choices", id: ++this.asked, state, node }).then(({ choices }) => choices);
  }

  /** What an inspector offers for the characters `from` to `to` (Unicode scalar values) of
   * `node`'s text as `state` shows it (PLAN 2.38): the looks a run takes, with the first
   * character's, which `style_text` sets. */
  characterChoices(state: string, node: string, from: number, to: number): Promise<Choices> {
    const asked = { type: "characterChoices" as const, id: ++this.asked, state, node, from, to };
    return this.request<"characterChoices">(asked).then(({ choices }) => choices);
  }

  /** What ⌘B gives the characters `from` to `to` (Unicode scalar values) of `node`'s text in
   * `state`, on the deck the editor's `source` compiles to (PLAN 2.38): `style_text`'s `look`. */
  bolding(source: string, state: string, node: string, from: number, to: number, format?: string): Promise<Record<string, unknown>> {
    const asked = { type: "bolding" as const, id: ++this.asked, source, state, node, from, to, format };
    return this.request<"bolding">(asked).then(({ look }) => look);
  }

  /** What ⌘I gives the characters `from` to `to` of `node`'s text (PLAN 2.40): `style_text`'s
   * `look`, from whether each of them asks for italic. */
  italicizing(source: string, state: string, node: string, from: number, to: number, format?: string): Promise<Record<string, unknown>> {
    const asked = { type: "italicizing" as const, id: ++this.asked, source, state, node, from, to, format };
    return this.request<"italicizing">(asked).then(({ look }) => look);
  }

  /** The theme the deck names, and the theme files the bundle holds (PLAN 2.39). */
  themes(): Promise<Themes> {
    return this.request<"themes">({ type: "themes", id: ++this.asked }).then(({ themes }) => themes);
  }

  /** The deck `source` compiles to in another theme (PLAN 2.39): one that ships, by its name, or
   * one the bundle holds, by its path. What it did, and unless it was refused, the deck's source
   * now and what the edit came to. */
  retheme(
    source: string,
    theme: { ships: string } | { path: string },
    index: number,
    format?: string,
  ): Promise<{ themed: Themed; source?: string; edited?: Edited }> {
    const asked = { type: "retheme" as const, id: ++this.asked, source, theme, index, format };
    return this.request<"rethemed">(asked).then(({ themed, source, edited }) => {
      if (edited?.at) {
        this.at = edited.at;
        this.onAt(edited.at);
      }
      return { themed, source, edited };
    });
  }

  /** What an inspector offers for `state` itself (PLAN 2.36): its layout, each key of its
   * transition, its hold, and its notes, each with its value and where it lives. */
  stateChoices(state: string): Promise<StateChoices> {
    return this.request<"stateChoices">({ type: "stateChoices", id: ++this.asked, state }).then(({ choices }) => choices);
  }

  /** The states `ops` (a patch) would change, by id, with nothing made. */
  reach(ops: unknown[]): Promise<string[]> {
    return this.request<"reached">({ type: "reach", id: ++this.asked, ops }).then(({ states }) => states);
  }

  /** What may be inserted in the deck (PLAN 2.34): a text in each of the theme's roles, each kind
   * of shape, each image in the bundle, and each shader preset. */
  inserts(): Promise<Insert[]> {
    return this.request<"inserts">({ type: "inserts", id: ++this.asked }).then(({ inserts }) => inserts);
  }

  /** The patch that inserts what `inserts` offers `n`th, entering in `state` about `at` (canvas
   * units), snapped to the theme's grid as a drop snaps, on the deck `source` compiles to. */
  inserting(source: string, state: string, n: number, at: [number, number], format?: string): Promise<Added> {
    return this.request<"adding">({ type: "inserting", id: ++this.asked, source, state, n, at, format }).then(({ added }) => added);
  }

  /** The patch that copies `node` beside it in `state`, on the deck `source` compiles to. */
  duplicating(source: string, state: string, node: string, format?: string): Promise<Added> {
    return this.request<"adding">({ type: "duplicating", id: ++this.asked, source, state, node, format }).then(({ added }) => added);
  }

  /** What the clipboard holds of `node` as `state` shows it, on the deck `source` compiles to
   * (PLAN 2.37): the clip, as JSON text. */
  copying(source: string, state: string, node: string, format?: string): Promise<string> {
    return this.request<"copied">({ type: "copying", id: ++this.asked, source, state, node, format }).then(({ clip }) => clip);
  }

  /** The patch that pastes `clip`, the clipboard's text, entering in `state` about `at` (canvas
   * units), on the deck `source` compiles to (PLAN 2.37). The files it carries are in the bundle
   * once it answers. */
  pasting(source: string, state: string, clip: string, at: [number, number], format?: string): Promise<Pasted> {
    return this.request<"pasted">({ type: "pasting", id: ++this.asked, source, state, clip, at, format }).then(({ pasted }) => pasted);
  }

  /** Each state at rest, `height` pixels high, in `format`, for the state strip (PLAN 2.35): with
   * pixels where its drawing is not the one `known` holds. */
  thumbnails(height: number, known: Record<string, string>, format?: string): Promise<Thumb[]> {
    return this.request<"thumbnails">({ type: "thumbnails", id: ++this.asked, height, known, format }).then(({ thumbs }) => thumbs);
  }

  /** The patch that adds a state after `state`, a `step` of its slide or a `slide` of its own, on
   * the deck `source` compiles to (PLAN 2.35). */
  addingState(source: string, state: string, what: "step" | "slide"): Promise<{ id: string; patch: unknown[] }> {
    return this.request<"addingState">({ type: "addingState", id: ++this.asked, source, state, what }).then(({ added }) => added);
  }

  /** The patch that takes `node`, with what it holds, out of `state` and the states after it, or,
   * `everywhere`, out of the deck, on the deck `source` compiles to. */
  deleting(source: string, state: string, node: string, everywhere: boolean): Promise<unknown[]> {
    return this.request<"deleting">({ type: "deleting", id: ++this.asked, source, state, node, everywhere }).then(({ patch }) => patch);
  }

  /** Make `ops`, typed on the canvas, by the user on the deck `source` compiles to, and show slot
   * `index` from it: the deck's source after, what the edit came to, and where a caret stands in
   * `node`'s text in `state` now. */
  type(
    source: string,
    ops: unknown[],
    at: { index: number; state: string; node: string },
    format?: string,
  ): Promise<{ source: string; edited: Edited; carets: Carets | null }> {
    const asked = { type: "type" as const, id: ++this.asked, source, ops, ...at, format };
    return this.request<"typed">(asked).then(({ source, edited, carets }) => {
      if (edited.at) {
        this.at = edited.at;
        this.onAt(edited.at);
      }
      return { source, edited, carets };
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
      case "made":
      case "choices":
      case "stateChoices":
      case "carets":
      case "characterChoices":
      case "bolding":
      case "italicizing":
      case "themes":
      case "rethemed":
      case "reached":
      case "typed":
      case "inserts":
      case "adding":
      case "deleting":
      case "copied":
      case "pasted":
      case "thumbnails":
      case "addingState":
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
