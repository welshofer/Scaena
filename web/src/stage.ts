// One canvas and the engine's worker that paints it (PLAN 2.1–2.4): the page's side of
// `protocol.ts`. The player shows one; the presenter view, two; the editor, one.
import type {
  Added,
  AnnotationAt,
  Arrange,
  Arranged,
  Asking,
  AssistantEvent,
  Attached,
  At,
  Cells,
  CodeKey,
  DataEdited,
  DataMark,
  DataSource,
  Drawn,
  Edited,
  Export,
  Found,
  FromWorker,
  Grid,
  Grouped,
  Health,
  Restated,
  Hit,
  Insert,
  Inspected,
  Keyed,
  Layer,
  LayoutSuggestion,
  Backgrounds,
  LayoutSlots,
  LinkTarget,
  Linted,
  NodeBox,
  NoteMark,
  Opened,
  Framing,
  Outline,
  Painter,
  Pasted,
  Picture,
  ProviderId,
  Query,
  Rect,
  RowEdit,
  SaveTo,
  Sheet,
  Slot,
  Snapped,
  SnapMode,
  Source,
  Targets,
  Themed,
  ThemeEdited,
  ThemeText,
  CodeColor,
  Themes,
  Thumb,
  Rewritten,
  ToWorker,
  Carets,
  Choices,
  StateChoices,
  Lexicon,
  SpeakerNotes,
  Look,
  Put,
  BundleFile,
  Grounded,
  Compared,
  Starter,
  Version,
  Waiting,
  Where,
  Worded,
  Connected,
  Duplicated,
  Diagram,
  Diagrammed,
  Writing,
  WritingAsk,
  WritingCall,
  Written,
  SlidePlace,
  SlideWritten,
  PictureJob,
  MapAsk,
  MapPlace,
  MapRefresh,
  MapTarget,
  Mapped,
  FetchedMap,
  PasteHints,
} from "./protocol";
import type { Icon } from "./icon-set";

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
      | "rekept"
      | "zipped"
      | "exported"
      | "sheet"
      | "dataEdited"
      | "dataUndone"
      | "dropped"
      | "pictured"
      | "attached"
      | "cells"
      | "models"
      | "reloaded"
      | "boxes"
      | "hits"
      | "marked"
      | "noted"
      | "calledOut"
      | "outlined"
      | "framed"
      | "markedIn"
      | "linked"
      | "laidOut"
      | "viewed"
      | "fitted"
      | "focal"
      | "found"
      | "replacement"
      | "targets"
      | "grid"
      | "dragged"
      | "arranged"
      | "grouped"
      | "made"
      | "choices"
      | "stateChoices"
      | "writing"
      | "written"
      | "wrote"
      | "slideAsked"
      | "slideWritten"
      | "words"
      | "notes"
      | "look"
      | "put"
      | "bundleFiles"
      | "health"
      | "restated"
      | "pictureMade"
      | "mapTargeted"
      | "mapFetched"
      | "mapped"
      | "mapRefresh"
      | "removed"
      | "versions"
      | "version"
      | "compared"
      | "restored"
      | "filesWritten"
      | "file"
      | "themeText"
      | "codeColors"
      | "themeEdited"
      | "carets"
      | "characterChoices"
      | "bolding"
      | "codeWords"
      | "tidied"
      | "keyed"
      | "coding"
      | "uncoding"
      | "tidying"
      | "focusing"
      | "walking"
      | "italicizing"
      | "themes"
      | "rethemed"
      | "reached"
      | "typed"
      | "inserts"
      | "layers"
      | "adding"
      | "duplicated"
      | "worded"
      | "connected"
      | "diagrammed"
      | "deleting"
      | "copied"
      | "pasted"
      | "thumbnails"
      | "addingState"
      | "starters"
      | "starting"
      | "waiting"
      | "filling"
      | "layoutSuggestions"
      | "backgrounds"
      | "backgroundFrames"
      | "backgroundPatch"
      | "pong";
  }
>;

/** The most helpers a stage starts for the CPU painter (PLAN 2.28): the page's `?helpers=`, a
 * whole number (0: the engine's worker works out every row itself), or as many as it asks. */
const mostHelpers = (() => {
  const n = Number(new URLSearchParams(location.search).get("helpers") ?? Number.NaN);
  return Number.isInteger(n) && n >= 0 ? n : Number.POSITIVE_INFINITY;
})();

/** How long a worker that failed outside a message has to say anything at all before it is taken
 * to have died (lift-up 10-24b STAB-2), ms: one task of a live worker, a large deck's whole lint
 * or a PDF's preparation, holds it for seconds. */
export const PROBE = 20000;

/** How a page starts the engine: a worker, and the engine's module, compiled, if the page
 * carries it, as a single-file export does (PLAN 2.5). A worker without it loads its own. */
export interface Engine {
  spawn: () => Worker;
  module?: WebAssembly.Module;
}

/** What a request waiting on a worker that stopped is refused with, a panic or a death (lift-up
 * STAB-2): the page that starts the engine again says so, so a caller that does not catch it
 * leaves nothing to say. */
export class EngineStopped extends Error {}

/** Whether `listen` hears anything within `within` ms: `listen` is handed what to call on each
 * thing heard (lift-up 10-24b STAB-2). */
export function lives(within: number, listen: (heard: () => void) => void): Promise<boolean> {
  return new Promise((resolve) => {
    const late = setTimeout(() => resolve(false), within);
    listen(() => (clearTimeout(late), resolve(true)));
  });
}

export class Stage {
  /** Where the deck is, as the worker last said. */
  at: At = { index: 0, t: 0, global: 0, playing: false };
  /** Called as the deck moves: each frame of a run, and each seek. */
  onAt: (at: At) => void = () => {};
  /** Called when the worker fails outside a request. */
  onError: (error: Error) => void = () => {};
  /** Called when the engine stopped on a panic (lift-up STAB-2), the worker stopped and every
   * request waiting on it refused: the bundle's files as the worker held them, with its name, for
   * the page to start the stage again from (`restart`). A page that does not hears an error. */
  onCrashed: (crash: { message: string; name: string; files: Record<string, ArrayBuffer> }) => void = (crash) =>
    this.onError(new Error(`the engine stopped: ${crash.message}`));
  /** Called when the worker failed outside a message and then answered nothing for `PROBE` ms
   * (lift-up 10-24b STAB-2), as one out of memory does: it is stopped and every request waiting
   * on it refused, and it hands nothing over. `lost`: the bundle's files written since the save,
   * which went with it, each by the name it was dropped as, where it was. A page that does not
   * start the stage again (`restart`) hears an error. */
  onDied: (death: { message: string; lost: string[] }) => void = (death) => this.onError(new Error(`the engine stopped: ${death.message}`));
  /** Called when a worker that failed outside a message is given `PROBE` ms to say anything
   * (lift-up 10-24b STAB-2), with what failed: the page says it is waiting, before it hears the
   * error the worker lived through (`onError`) or the stage stopped (`onDied`). */
  onStalled: (message: string) => void = () => {};
  /** Called on each message the worker sends, while a worker that failed is asked whether it
   * lives. */
  private heard?: () => void;
  /** The formats beside the canvas are painted: `state`, `t` ms into its cue (PLAN 2.62). */
  onBesides: (state: string, t: number) => void = () => {};
  /** Called as the canvas's attributes change: `size`, which frames are painted at from then
   * on, fitted to the room (`fit`), the format shown, or the bundle read again (lift-up ATTR-2). */
  onSized: (size: [number, number]) => void = () => {};
  /** Whether a worker that failed is being asked whether it still answers. */
  private probing = false;
  /** Requests not yet answered, by id. */
  private waiting = new Map<number, { resolve: (reply: Reply) => void; reject: (error: Error) => void }>();
  /** What hears each step of the assistant's answer to a question, by the question's id. */
  private hearing = new Map<number, (event: AssistantEvent) => void>();
  /** What takes each thumbnail as it is painted, by the request's id. */
  private thumbing = new Map<number, (thumb: Thumb) => void>();
  private asked = 0;
  /** The name each file dropped had, by its path in the bundle, which names it by its content. */
  private dropped = new Map<string, string>();
  /** The patch being made, which the next waits for (`make`). */
  private making: Promise<void> = Promise.resolve();

  /** The bundle's files written since the last save, as the page heard of them (lift-up REL-4),
   * each with how many times, and those taken out: what a recovery copy keeps beside the source. A
   * save clears both. */
  private writtenSince = new Map<string, number>();
  private removedSince = new Set<string>();

  /** Workers that work out shaders' rows for the engine's worker, when it asks (PLAN 2.28). */
  private helpers: Worker[] = [];

  private constructor(
    public canvas: HTMLCanvasElement,
    private worker: Worker,
    /** The bundle as the worker opened it, or last read it again. */
    public opened: Opened,
    /** How the engine's worker was started: its helpers start the same way. */
    private engine: Engine,
    /** Who paints, as the page asked. */
    private painter: Painter,
  ) {
    this.hear(worker);
  }

  private hear(worker: Worker) {
    worker.onmessage = ({ data }: MessageEvent<FromWorker>) => {
      if (worker === this.worker) this.heard?.();
      this.listen(data);
    };
    worker.onerror = (e) => {
      // Heard here, and said or recovered from: not the page's uncaught error.
      e.preventDefault();
      void this.failed(worker, e.message || "the worker failed");
    };
    // A message that could not be read leaves the request it answered waiting: each waiting is
    // refused, and the worker, which still answers, is kept.
    worker.onmessageerror = () => this.refuse(new Error("the engine's answer could not be read"));
  }

  /** The worker failed outside a message (lift-up 10-24b STAB-2): an error it lives through is
   * said, and one it died of, which leaves it answering nothing, stops it as a panic does. */
  private async failed(worker: Worker, message: string) {
    if (worker !== this.worker || this.probing) return;
    this.probing = true;
    this.onStalled(message);
    const alive = await this.probe();
    this.probing = false;
    if (worker !== this.worker) return;
    if (alive) return this.onError(new Error(message));
    const lost = [...this.writtenSince.keys()].map((path) => this.dropped.get(path) ?? path);
    // They went with it: none is there to keep beside the source or to save.
    this.writtenSince.clear();
    this.halt(new EngineStopped(`the engine stopped: ${message}`));
    this.onDied({ message, lost });
  }

  /** Whether the worker says anything within `PROBE` ms: an answer to anything, or to a `ping`,
   * which it answers once the task it is in ends. */
  private probe(): Promise<boolean> {
    const id = ++this.asked;
    return lives(PROBE, (heard) => {
      this.heard = heard;
      this.waiting.set(id, { resolve: heard, reject: heard });
      this.send({ type: "ping", id });
    }).finally(() => {
      this.heard = undefined;
      this.waiting.delete(id);
    });
  }

  /** Open the bundle at `source` in a new worker of `engine`'s, which paints into a new
   * canvas in `canvas`'s place: a canvas WebGPU has held takes no other painter, so falling
   * back to the CPU painter starts over. A bundle that does not open puts `canvas` back, as
   * whoever painted it left it. */
  static async open(canvas: HTMLCanvasElement, source: Source, painter: Painter, engine: Engine): Promise<Stage> {
    const started = await Stage.start(canvas, source, painter, engine);
    return new Stage(started.canvas, started.worker, started.opened, engine, painter);
  }

  /** Start the stage again on `source`, the bundle's files as its worker held them (lift-up
   * STAB-2): a fresh worker of its own, painting into a fresh canvas in this one's place; the
   * requests waiting on the old one are refused. The bundle as the fresh worker opened it. */
  async restart(source: Source): Promise<Opened> {
    this.halt(new EngineStopped("the engine stopped"));
    const started = await Stage.start(this.canvas, source, this.painter, this.engine);
    this.canvas = started.canvas;
    this.worker = started.worker;
    this.opened = started.opened;
    this.hear(started.worker);
    return started.opened;
  }

  /** A worker of `engine`'s opened on `source`, painting into a fresh canvas in `canvas`'s
   * place: the canvas, the worker, and the bundle as it opened it. */
  private static async start(
    canvas: HTMLCanvasElement,
    source: Source,
    painter: Painter,
    engine: Engine,
  ): Promise<{ canvas: HTMLCanvasElement; worker: Worker; opened: Opened }> {
    const fresh = canvas.cloneNode() as HTMLCanvasElement;
    canvas.replaceWith(fresh);
    const worker = engine.spawn();
    try {
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
        return { canvas: fresh, worker, opened };
      }
      worker.terminate();
      const message = first.type === "error" ? first.message : `the worker said ${first.type} first`;
      if (first.type === "error" && first.webgpu && painter === "auto") {
        console.warn(`${message}; the CPU painter paints instead`);
        return await Stage.start(fresh, source, "cpu", engine);
      }
      throw new Error(message);
    } catch (e) {
      worker.terminate();
      if (fresh.isConnected) fresh.replaceWith(canvas);
      throw e;
    }
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
  /** The link drawn at `at`, fractions of the canvas, in `state` at rest: where a click there goes
   * (PLAN 2.70); none off every link. */
  linkAt(state: string, at: [number, number], format?: string): Promise<LinkTarget | undefined> {
    return this.request<"linked">({ type: "linkAt", id: ++this.asked, state, at, format }).then(({ link }) => link ?? undefined);
  }

  reading(state: string, format?: string): Promise<string> {
    return this.request<"reading">({ type: "read", id: ++this.asked, state, format }).then(({ html }) => html);
  }

  /** Play from slot `index`, `t` ms into its cue, until a state that waits comes to rest:
   * `still`, each cue a cut to its state at rest, at the deck's pace (PLAN 2.8); `alone`, that
   * slot's cue alone, coming to rest at its end (PLAN 2.44). */
  run(index: number, t = 0, format?: string, still?: boolean, alone?: boolean) {
    this.send({ type: "run", index, t, format, still, alone });
  }

  /** Show slot `index` `t` ms into its cue (at rest without `t`), still. Resolves once it is
   * on the canvas. */
  seek(index: number, t?: number, format?: string): Promise<At> {
    return this.request<"at">({ type: "seek", id: ++this.asked, index, t, format }).then(() => this.at);
  }

  pause() {
    this.send({ type: "pause" });
  }

  /** Paint the state shown in each of `besides`' formats on its canvas, `height` pixels high,
   * after each of the canvas's frames, as it plays (PLAN 2.62); none stops it. Each canvas is
   * handed to the worker, which paints it from then on. */
  besides(besides: { format?: string; canvas: HTMLCanvasElement }[], height: number) {
    const offscreen = besides.map(({ format, canvas }) => ({ format, canvas: canvas.transferControlToOffscreen() }));
    this.worker.postMessage(
      { type: "besides", besides: offscreen, height } satisfies ToWorker,
      offscreen.map(({ canvas }) => canvas),
    );
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

  /** The chart mark or table row drawn at `point` in `state` at rest, with the rows of its source
   * it was made from; none where the topmost node there is no chart or table, or between its marks
   * (PLAN 2.64). */
  markAt(state: string, point: [number, number], format?: string): Promise<DataMark | undefined> {
    return this.request<"marked">({ type: "markAt", id: ++this.asked, state, point, format }).then(({ marks }) => marks[0]);
  }

  /** What `rows` of data source `source` draw in `state` at rest: each chart mark and table row
   * made from any of them, in paint order (PLAN 2.64). */
  marksOf(state: string, source: string, rows: number[], format?: string): Promise<DataMark[]> {
    return this.request<"marked">({ type: "marksOf", id: ++this.asked, state, source, rows, format }).then(({ marks }) => marks);
  }

  /** The chart annotation drawn at `point` in `state` at rest, by its place among the chart's
   * `annotations`; none where the topmost node there is no chart, or off its annotations (PLAN 2.67). */
  noteAt(state: string, point: [number, number], format?: string): Promise<NoteMark | undefined> {
    return this.request<"noted">({ type: "noteAt", id: ++this.asked, state, point, format }).then(({ note }) => note ?? undefined);
  }

  /** The layout `state` uses and its slots in the format shown (PLAN 2.71); none for a state
   * that names no layout. */
  layout(state: string, format?: string): Promise<LayoutSlots | undefined> {
    return this.request<"laidOut">({ type: "layout", id: ++this.asked, state, format }).then(({ layout }) => layout ?? undefined);
  }

  /** Chart `node`'s marks, in data order, and its annotations, in the order it writes them, in
   * `state` at rest; none for a node that is no chart (PLAN 2.75). */
  marksIn(state: string, node: string, format?: string): Promise<{ marks: DataMark[]; notes: NoteMark[] } | undefined> {
    return this.request<"markedIn">({ type: "marksIn", id: ++this.asked, state, node, format }).then(({ found }) => found ?? undefined);
  }

  /** Image `node`'s framing in `state` at rest: where its whole and the part that shows are drawn,
   * its crop, and its focal point; none for a node that is no image, or not drawn there (PLAN 2.74). */
  framing(state: string, node: string, format?: string): Promise<Framing | undefined> {
    return this.request<"framed">({ type: "framing", id: ++this.asked, state, node, format }).then(({ framing }) => framing ?? undefined);
  }

  /** Shape `node`'s outline in `state` at rest: its points and a rect's corners, with the theme's
   * radius steps; none for a node that is no shape, or not drawn there (PLAN 2.68). */
  outline(state: string, node: string, format?: string): Promise<Outline | undefined> {
    return this.request<"outlined">({ type: "outline", id: ++this.asked, state, node, format }).then(({ outline }) => outline ?? undefined);
  }

  /** Where a callout of chart `node` dropped at `point` in `state` at rest would stand: on the mark
   * there, or at the category or x nearest across and the value there (PLAN 2.67). */
  calloutAt(state: string, node: string, point: [number, number], format?: string): Promise<AnnotationAt | undefined> {
    return this.request<"calledOut">({ type: "calloutAt", id: ++this.asked, state, node, point, format }).then(({ at }) => at ?? undefined);
  }

  /** Paint the preview through `view`, `[x, y, w, h]` canvas units: the part of the canvas a
   * zoomed editor shows, at the size shown; the whole canvas with none (PLAN 2.46). Resolves once
   * what is shown is painted so. */
  view(view: [number, number, number, number] | undefined): Promise<void> {
    return this.request<"viewed">({ type: "view", id: ++this.asked, view: view ?? null }).then(() => {});
  }

  /** Each text of the deck `source` compiles to that `query` matches, once for each place it is
   * written, with the states that show it from there (PLAN 2.47). */
  find(source: string, query: Query): Promise<Found[]> {
    return this.request<"found">({ type: "find", id: ++this.asked, source, query }).then(({ found }) => found);
  }

  /** The patch that replaces what `query` matches in the deck `source` compiles to with `with`:
   * every match, a `replace_text` where each text lives, or with `one`, `[text, match]` into
   * what `find` gives, that match alone (PLAN 2.47). */
  replacing(source: string, query: Query, with_: string, one?: [number, number]): Promise<unknown[]> {
    return this.request<"replacement">({ type: "replacing", id: ++this.asked, source, query, with: with_, one }).then(({ ops }) => ops);
  }

  /** The point of image `node` under `point` in `state` at rest, in the deck `source` compiles
   * to: fractions of its crop, which a focal point picked there names; `null` off the image
   * (PLAN 2.45). */
  focalAt(source: string, state: string, node: string, point: [number, number], format?: string): Promise<[number, number] | null> {
    return this.request<"focal">({ type: "focalAt", id: ++this.asked, source, state, node, point, format }).then(({ at }) => at);
  }

  /** Where `node` may go in `state` at rest. */
  targets(state: string, node: string, format?: string): Promise<Targets> {
    return this.request<"targets">({ type: "targets", id: ++this.asked, state, node, format }).then(({ targets }) => targets);
  }
  /** Paint frames at the size of the slide as large as fits in `room`, device pixels, keeping
   * the canvas's aspect, so a slide fills the screen as sharp as the screen is (lift-up ATTR-2);
   * at the canvas's own size with `null`. Resolves with the canvas's size from now on, once what
   * is shown is painted again so. */
  fit(room: [number, number] | null): Promise<[number, number]> {
    return this.request<"fitted">({ type: "fit", id: ++this.asked, room }).then(({ size }) => size);
  }

  /** The theme's grid in `format`, as the canvas's guides draw it (PLAN 2.57). */
  grid(format?: string): Promise<Grid> {
    return this.request<"grid">({ type: "grid", id: ++this.asked, format }).then(({ grid }) => grid);
  }


  /** A drag's move: `node` painted `by` from where it stands, where its box would land `snap`ped,
   * and the states that patch changes; or, to `preview`, the state as the patch would make it. */
  drag(
    state: string,
    node: string,
    move: {
      by?: [number, number];
      snap?: { how: SnapMode; to: Rect; fork: boolean; reach?: number };
      with?: string[];
      together?: { by: [number, number]; free: boolean; fork: boolean; reach?: number };
      preview?: boolean;
    },
    format?: string,
  ): Promise<{ snapped?: Snapped | null; states?: string[] }> {
    return this.request<"dragged">({ type: "drag", id: ++this.asked, state, node, ...move, format });
  }

  /** `nodes`, children of one container, arranged `how` in `state` at rest (PLAN 2.42): where each
   * lands and the patch that puts them there, kept to the state to `fork` it; `null` where nothing
   * moves them so. */
  arranging(state: string, nodes: string[], how: Arrange, fork: boolean, format?: string): Promise<Arranged | null> {
    return this.request<"arranged">({ type: "arrange", id: ++this.asked, state, nodes, how, fork, format }).then(({ arranged }) => arranged);
  }

  /** A drag is over and changes nothing: `state` at rest as it stands. */
  rest(state: string, format?: string): Promise<void> {
    return this.request<"shown">({ type: "rest", id: ++this.asked, state, format }).then(() => {});
  }

  /** Make `ops` by the user on the deck `source` compiles to, and show slot `index` from it: the
   * deck's source after, and what the edit came to. */
  make(source: () => string, ops: unknown[], index: number, format?: string): Promise<{ source: string; edited: Edited }> {
    // One patch at a time for the whole page (lift-up REL-5): each made on the source as it stands
    // when its turn comes, after the last one was taken, and made again on the source as it stands
    // if something else changed it meanwhile (a keystroke, a data edit), so that two panels' edits
    // in flight compose instead of the second overwriting the first.
    const turn = this.making.then(async () => {
      for (let tries = 1; ; tries++) {
        const from = source();
        const made = await this.request<"made">({ type: "make", id: ++this.asked, source: from, ops, index, format });
        if (source() === from || tries === 3) return made;
      }
    });
    // The next turn waits a task more, so the caller has taken this one (each takes its patch as
    // soon as it is made) before the next reads the source.
    const taken = () => new Promise<void>((settle) => setTimeout(settle));
    this.making = turn.then(taken, taken);
    return turn.then(({ source, edited }) => {
      if (edited.at) {
        this.at = edited.at;
        this.onAt(edited.at);
      }
      return { source, edited };
    });
  }

  /** The deck's data sources, and source `name` as a sheet (PLAN 2.55): the first source without
   * it; or why it does not read as one. */
  sheet(source: string, name?: string): Promise<{ sources: DataSource[]; name?: string; sheet?: Sheet; file?: string; why?: string }> {
    return this.request<"sheet">({ type: "sheet", id: ++this.asked, source, name }).then(({ type: _, id: __, ...got }) => got);
  }

  /** `edits` of data source `name`, by the user (PLAN 2.55): what they did, and where they wrote,
   * the deck's source after and what the edit of it came to. */
  dataEdit(
    source: string,
    name: string,
    edits: RowEdit[],
    index: number,
    format?: string,
  ): Promise<{ result: DataEdited; source?: string; edited?: Edited }> {
    return this.request<"dataEdited">({ type: "dataEdit", id: ++this.asked, source, name, edits, index, format }).then(
      ({ result, source, edited }) => {
        this.moved(edited);
        if (result.file && !result.refused) this.wrote([result.file]);
        return { result, source, edited };
      },
    );
  }

  /** The bundle's images, fonts, and data, in the deck `source` compiles to (PLAN 2.59): each with
   * what names it, and the nodes drawn from it in the states that show them so. */
  /** The narrative health of the deck `source` compiles to (PLAN G.9): its scores, each slide as
   * read, and what to look at, read by the generator's module, which the worker loads the first
   * time. */
  health(source: string): Promise<Health> {
    return this.request<"health">({ type: "health", id: ++this.asked, source }).then(({ health }) => health);
  }

  /** The beats whose claim a slide said in the deck `before` compiles to and says no longer in
   * the deck `source` compiles to, each with its fix (lift-up FUNC-2). */
  restated(source: string, before: string): Promise<Restated[]> {
    return this.request<"restated">({ type: "restated", id: ++this.asked, source, before }).then(({ restated }) => restated);
  }

  /** A picture made by the user's image model in the deck's theme (PLAN 2.107), by the
   * generator's module, which the worker loads the first time: its bytes, its file's kind, what
   * it shows, and the model that made it. */
  /** Where a map goes in `state` (ADR-0030, PLAN O.1c), as the canvas found the place. */
  mapTarget(source: string, state: string, place: { node?: string; slot?: string; rect: [number, number, number, number] }): Promise<MapTarget> {
    return this.request<"mapTargeted">({ type: "mapTarget", id: ++this.asked, source, state, ...place }).then(({ target }) => target);
  }

  /** A map fetched from Geoapify with `key` by the generator's plan, each GET made by `fetch`: the
   * places a name matched, to pick one of, or the map. */
  fetchMap(ask: MapAsk, key: string): Promise<{ places?: MapPlace[]; map?: FetchedMap }> {
    return this.request<"mapFetched">({ type: "fetchMap", id: ++this.asked, ask, key }).then(({ places, map }) => ({ places, map }));
  }

  /** A map fetched put where `target` says: its picture joins the bundle, and the ops are the
   * editor's to make one patch of. */
  mapping(source: string, target: MapTarget, map: FetchedMap, label?: string): Promise<Mapped> {
    return this.request<"mapped">({ type: "mapping", id: ++this.asked, source, target, fetched: map.fetched, pin: map.pin, label, png: map.png }).then(
      ({ mapped }) => mapped,
    );
  }

  /** What fetching image `node`'s map again in `state` asks for: at `zoom`, or about `center`. */
  mapRefreshing(source: string, state: string, node: string, zoom?: number, center?: { lat: number; lon: number }): Promise<MapRefresh> {
    return this.request<"mapRefresh">({ type: "mapRefreshing", id: ++this.asked, source, state, node, zoom, center }).then(({ refresh }) => refresh);
  }

  makePicture(picture: PictureJob): Promise<{ bytes: ArrayBuffer; ext: string; alt: string; model: string }> {
    return this.request<"pictureMade">({ type: "makePicture", id: ++this.asked, picture }).then(({ bytes, ext, alt, model }) => ({
      bytes,
      ext,
      alt,
      model,
    }));
  }

  bundleFiles(source: string): Promise<BundleFile[]> {
    return this.listing(source).then(({ files }) => files);
  }

  /** The bundle's files, and the sources its deck was grounded in (narrative FUNC-3). */
  listing(source: string): Promise<{ files: BundleFile[]; grounded: Grounded[] }> {
    return this.request<"bundleFiles">({ type: "bundleFiles", id: ++this.asked, source });
  }

  /** `path` taken out of the bundle, by the user (PLAN 2.59): one nothing names. `dataUndo` puts it
   * back. What the deck came to, shown and linted as an edit is. */
  removeFile(source: string, path: string, index: number, format?: string): Promise<Edited> {
    return this.request<"removed">({ type: "removeFile", id: ++this.asked, source, path, index, format }).then(({ edited }) => {
      this.moved(edited);
      this.wrote([], [path]);
      return edited;
    });
  }

  /** `path`, taken out of the bundle, put back as it was, by its path (PLAN 2.59): the undo of its
   * removal in the editor's one undo, whatever came after it. What the deck came to, shown and
   * linted as an edit is. */
  putBack(source: string, path: string, index: number, format?: string): Promise<Edited> {
    return this.request<"removed">({ type: "putBack", id: ++this.asked, source, path, index, format }).then(({ edited }) => {
      this.moved(edited);
      this.wrote([path]);
      return edited;
    });
  }

  /** The bundle's versions, oldest first (PLAN 2.60); none where it keeps no history. */
  versions(): Promise<Version[] | null> {
    return this.request<"versions">({ type: "versions", id: ++this.asked }).then(({ versions }) => versions);
  }

  /** Version `version` shown read-only: its states, and `state` of it at rest as a PNG `width`
   * pixels wide. */
  version(version: string, state: string | undefined, width: number) {
    return this.request<"version">({ type: "version", id: ++this.asked, version, state, width });
  }

  /** What changed from version `from` to version `to`, or without it to the deck now. */
  compareVersions(source: string, from: string, to?: string): Promise<Compared> {
    return this.request<"compared">({ type: "compareVersions", id: ++this.asked, source, from, to }).then(({ compared }) => compared);
  }

  /** `version` made the deck again, with its data files, as one change by the user. */
  restoreVersion(source: string, version: Version, index: number, format?: string) {
    return this.request<"restored">({ type: "restoreVersion", id: ++this.asked, source, version, index, format }).then((done) => {
      if (done.edited) this.moved(done.edited);
      this.rewrote(done.files);
      return done;
    });
  }

  /** Each kind of code token as the theme draws it (PLAN 2.144). */
  codeColors(): Promise<CodeColor[]> {
    return this.request<"codeColors">({ type: "codeColors", id: ++this.asked }).then(({ colors }) => colors);
  }

  /** The theme frames are drawn in, as its text (PLAN 2.61); null where the deck names none. */
  themeText(): Promise<ThemeText | null> {
    return this.request<"themeText">({ type: "themeText", id: ++this.asked }).then(({ theme }) => theme);
  }

  /** The theme the deck names edited by `ops` (RFC 6902), or to the colors of the image `photo`
   * the bundle holds (PLAN 2.94), as one change by the user, the deck `source` compiles to drawn in
   * it at slot `index` (PLAN 2.61, ADR-0016): what it did, the theme file it wrote, before and
   * after, the source after, and what the edit came to. */
  themeEdit(
    source: string,
    ops: unknown[],
    index: number,
    format?: string,
    photo?: string,
  ): Promise<{ result: ThemeEdited; files: Rewritten[]; source?: string; edited?: Edited }> {
    return this.request<"themeEdited">({ type: "themeEdit", id: ++this.asked, source, ops, photo, index, format }).then((done) => {
      if (done.edited) this.moved(done.edited);
      this.rewrote(done.files);
      return done;
    });
  }

  /** Files written back, as an undo or a redo of a restore or a theme edit has them; with `edit`,
   * the deck its source compiles to shown and linted again, as an edit is. */
  writeFiles(files: { path: string; text: string | null; bytes?: ArrayBuffer }[], edit?: { source: string; index: number; format?: string }): Promise<Edited | undefined> {
    return this.request<"filesWritten">({ type: "writeFiles", id: ++this.asked, files, edit }).then(({ edited }) => {
      if (edited) this.moved(edited);
      this.rewrote(files.map(({ path, text, bytes }) => ({ path, after: bytes ? "" : text })));
      return edited;
    });
  }

  /** The last edit of a data file undone, or with `redo` made again (PLAN 2.55): the source whose
   * file it wrote, and what the edit came to; nothing where there was nothing to do. */
  dataUndo(source: string, redo: boolean, index: number, format?: string): Promise<{ name?: string; edited?: Edited }> {
    return this.request<"dataUndone">({ type: "dataUndo", id: ++this.asked, source, redo, index, format }).then(({ name, edited }) => {
      this.moved(edited);
      return { name, edited };
    });
  }

  /** Where an edit left the deck shown, if it says. */
  /** Files `written` since the save, as the page heard, and `removed` from the bundle (lift-up
   * REL-4): a file written is no longer removed, and one removed no longer written. */
  wrote(written: Iterable<string>, removed: Iterable<string> = []) {
    for (const path of written) {
      this.writtenSince.set(path, (this.writtenSince.get(path) ?? 0) + 1);
      this.removedSince.delete(path);
    }
    for (const path of removed) {
      this.writtenSince.delete(path);
      this.removedSince.add(path);
    }
  }

  /** Files rewritten, each to its text `after`, none taking it out. */
  private rewrote(files: { path: string; after: string | null }[]) {
    this.wrote(
      files.filter((f) => f.after !== null).map((f) => f.path),
      files.filter((f) => f.after === null).map((f) => f.path),
    );
  }

  /** A save `done`: every file is where the bundle is kept. */
  private saved<T>(done: T): T {
    this.writtenSince.clear();
    this.removedSince.clear();
    return done;
  }

  /** The files written since the last save, each with how many times, and those taken out. */
  unsaved(): { written: Map<string, number>; removed: string[] } {
    return { written: new Map(this.writtenSince), removed: [...this.removedSince] };
  }

  /** The file at `path` in the bundle, as the worker holds it; null where it holds none. */
  file(path: string): Promise<ArrayBuffer | null> {
    return this.request<"file">({ type: "file", id: ++this.asked, path }).then(({ bytes }) => bytes);
  }

  private moved(edited: Edited | undefined) {
    if (!edited?.at) return;
    this.at = edited.at;
    this.onAt(edited.at);
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

  /** `text` with `from` to `to` (UTF-16) selected after `key` in code whose tab stops are `tab`
   * columns apart, in `language`, as a code editor answers it (PLAN 2.139, `code::keyed`): a
   * Makefile's stop a tab; none for a key code does not answer. */
  keyed(text: string, from: number, to: number, key: CodeKey, tab: number, language?: string): Promise<Keyed | null> {
    const asked = { type: "keyed" as const, id: ++this.asked, text, from, to, key, tab, language };
    return this.request<"keyed">(asked).then(({ keyed }) => keyed);
  }

  /** Make Code's patch of `node` (PLAN 2.139): `code` in `language`, else in the language its words
   * read as, and the theme's code role, what code does not take taken away. */
  coding(source: string, node: string, language?: string): Promise<unknown[]> {
    return this.request<"coding">({ type: "coding", id: ++this.asked, source, node, language }).then(({ patch }) => patch);
  }

  /** Not Code's patch of `node` (PLAN 2.139): `code` taken away everywhere, the text in `body`. */
  uncoding(source: string, node: string): Promise<unknown[]> {
    return this.request<"uncoding">({ type: "uncoding", id: ++this.asked, source, node }).then(({ patch }) => patch);
  }

  /** Tidy Code's patch of `node` as `state` shows it (PLAN 2.139): `[]` where it is tidy. */
  tidying(source: string, state: string, node: string): Promise<unknown[]> {
    const asked = { type: "tidying" as const, id: ++this.asked, source, state, node };
    return this.request<"tidying">(asked).then(({ patch }) => patch);
  }

  /** The patch that sets code text `node`'s lines in focus in `state` to those `said` names in its
   * gutter's numbers, "2–4, 7"; nothing said clears it (PLAN 2.140): Focus on These Lines, Lines in
   * Focus, and Clear Focus. The states after it stay as they were; `[]` where it shows them so. */
  focusing(source: string, state: string, node: string, said: string): Promise<unknown[]> {
    const asked = { type: "focusing" as const, id: ++this.asked, source, state, node, said };
    return this.request<"focusing">(asked).then(({ patch }) => patch);
  }

  /** Walk Through Lines…'s patch of code text `node` from `state` (PLAN 2.140): `state` shows the
   * lines the first of `steps` names, and a step after it each of the rest's. */
  walking(source: string, state: string, node: string, steps: string[]): Promise<unknown[]> {
    const asked = { type: "walking" as const, id: ++this.asked, source, state, node, steps };
    return this.request<"walking">(asked).then(({ patch }) => patch);
  }

  /** `text` tidied as code in `language` is when it is pasted (PLAN 2.133, `code::tidy`). */
  tidied(text: string, language: string): Promise<string> {
    return this.request<"tidied">({ type: "tidied", id: ++this.asked, text, language }).then(({ text }) => text);
  }

  /** The words of code text `node` as `state` shows them: what Copy Code copies (PLAN 2.133). */
  codeWords(source: string, state: string, node: string, format?: string): Promise<string> {
    const asked = { type: "codeWords" as const, id: ++this.asked, source, state, node, format };
    return this.request<"codeWords">(asked).then(({ words }) => words);
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

  /** What to ask a model for the person editing (PLAN 2.101): the task, its title, and the
   * instructions and prompt it answers, from the deck as it stands. */
  writing(ask: WritingAsk): Promise<Writing> {
    return this.request<"writing">({ type: "writing", id: ++this.asked, ask }).then(({ writing }) => writing);
  }

  /** A model's `answer` to `ask` as it is offered, and the patch that takes it (PLAN 2.101). */
  written(ask: WritingAsk, answer: string): Promise<Written> {
    return this.request<"written">({ type: "written", id: ++this.asked, ask, answer }).then(({ written }) => written);
  }

  /** Ask a model once, with the user's key and nothing to call (PLAN 2.101): its words. */
  /** What to ask a model for one slide at `place`, with the person's `ask` (lift-up FUNC-1). */
  slideAsk(place: SlidePlace, ask: string): Promise<{ system: string; prompt: string }> {
    return this.request<"slideAsked">({ type: "slideAsk", id: ++this.asked, place, ask }).then(({ asked }) => asked);
  }

  /** The slide a model's `answer` holds, written at `place` by `author` (lift-up FUNC-1): what it
   * did, the deck's source now, and what the edit came to for state `index` in `format`. */
  slideWrite(place: SlidePlace, answer: string, author: string, index: number, format?: string): Promise<{ written: SlideWritten; source: string; edited: Edited }> {
    return this.request<"slideWritten">({ type: "slideWrite", id: ++this.asked, place, answer, author, index, format }).then(({ written, source, edited }) => ({
      written,
      source,
      edited,
    }));
  }

  /** Ask a model once, with the user's key and nothing to call (PLAN 2.101): its words. `signal`,
   * aborted, stops the request in the worker (lift-up PERF-4). */
  write(call: WritingCall, signal?: AbortSignal): Promise<{ text: string; usage?: { input: number; output: number } }> {
    const id = ++this.asked;
    const stop = () => this.send({ type: "stopWrite", id });
    signal?.addEventListener("abort", stop, { once: true });
    return this.request<"wrote">({ type: "write", id, call })
      .then(({ text, usage }) => ({ text, usage }))
      .finally(() => signal?.removeEventListener("abort", stop));
  }

  /** The words an editor shows for `state` (PLAN 2.99): each object it shows named by what it
   * shows and by its kind, and each of the theme's names said. */
  words(state: string): Promise<Lexicon> {
    return this.request<"words">({ type: "words", id: ++this.asked, state }).then(({ words }) => words);
  }

  /** The notes a presenter reads for `state` (SPEC §2.6): its own, else its first beat's, as
   * written and as the blocks a speaker reads; what a rehearsal shows beside the slide. */
  notes(state: string): Promise<SpeakerNotes> {
    return this.request<"notes">({ type: "notes", id: ++this.asked, state }).then(({ notes }) => notes);
  }

  /** The layouts `state` may take, best first, in the deck shown (PLAN 2.92), as `stateChoices`
   * offers them: each judged by lint, with the patch that gives it, and painted at rest `height`
   * pixels high. The deck the worker holds: a request on a timer never takes it back to a source
   * an edit has moved past. */
  layoutSuggestions(state: string, height: number, format?: string): Promise<LayoutSuggestion[]> {
    return this.request<"layoutSuggestions">({ type: "layoutSuggestions", id: ++this.asked, state, height, format }).then(
      ({ suggestions }) => suggestions,
    );
  }

  /** What `state`'s slide may take as its background (PLAN 2.148): None and each of the theme's
   * shader presets, each with the patch that gives the slide it, drawn at rest `height` pixels
   * high in the deck shown. */
  backgrounds(state: string, height: number, format?: string): Promise<Backgrounds> {
    return this.request<"backgrounds">({ type: "backgrounds", id: ++this.asked, state, height, format }).then(
      ({ backgrounds }) => backgrounds,
    );
  }

  /** The patch that gives `state`'s slide `preset` as its background, or none, each text too faint over it
   * set in a color that reads there: what choosing one makes (PLAN 2.148). */
  backgroundPatch(state: string, preset?: string): Promise<unknown[]> {
    return this.request<"backgroundPatch">({ type: "backgroundPatch", id: ++this.asked, state, preset }).then(({ patch }) => patch);
  }

  /** The backgrounds `backgrounds` drew last, each painted again with its effect's clock at `t` seconds:
   * a swatch's frame (PLAN 2.148). */
  backgroundFrames(t: number): Promise<ArrayBuffer[]> {
    return this.request<"backgroundFrames">({ type: "backgroundFrames", id: ++this.asked, t }).then(({ pixels }) => pixels);
  }

  /** `state` at rest as the patch `ops` would make it, nothing made, until `rest` (PLAN 2.92). */
  preview(state: string, ops: unknown[], format?: string): Promise<void> {
    return this.request<"shown">({ type: "preview", id: ++this.asked, state, ops, format }).then(() => {});
  }

  /** `node`'s look as `state` shows it, in the deck `source` compiles to (PLAN 2.58): what ⌥⌘C
   * picks up. */
  look(source: string, state: string, node: string): Promise<Look> {
    return this.request<"look">({ type: "look", id: ++this.asked, source, state, node }).then(({ look }) => look);
  }

  /** The patch that puts `look` on `nodes` in `state`, in the deck `source` compiles to (PLAN
   * 2.58): one `choose` for each property a node shows otherwise, written where its own value
   * lives; and which nodes it changes, which look so already, and which take none of it. */
  putting(source: string, state: string, look: Look, nodes: string[]): Promise<Put> {
    return this.request<"put">({ type: "putting", id: ++this.asked, source, state, look, nodes }).then(({ put }) => put);
  }

  /** The states `ops` (a patch) would change, by id, with nothing made. */
  reach(ops: unknown[]): Promise<string[]> {
    return this.request<"reached">({ type: "reach", id: ++this.asked, ops }).then(({ states }) => states);
  }

  /** `state`'s layers (PLAN 2.50): its nodes in paint order, nested as their containers and groups
   * hold them, with those it does not show that leave in it or that another state of its slide
   * shows. */
  layers(state: string): Promise<Layer[]> {
    return this.request<"layers">({ type: "layers", id: ++this.asked, state }).then(({ layers }) => layers);
  }

  /** What may be inserted in the deck (PLAN 2.34): a text in each of the theme's roles, each kind
   * of shape, each image in the bundle, and each shader preset. */
  inserts(): Promise<Insert[]> {
    return this.request<"inserts">({ type: "inserts", id: ++this.asked }).then(({ inserts }) => inserts);
  }

  /** The patch that inserts what `inserts` offers `n`th, entering in `state` about `at` (canvas
   * units), snapped to the theme's grid as a drop snaps, on the deck `source` compiles to. */
  inserting(source: string, state: string, n: number, at: [number, number], format?: string, named?: string, with_?: Record<string, unknown>): Promise<Added> {
    return this.request<"adding">({ type: "inserting", id: ++this.asked, source, state, n, at, format, named, with: with_ }).then(({ added }) => added);
  }

  /** The patch that inserts `icon` (PLAN 2.104), entering in `state` about `at` (canvas units),
   * on the deck `source` compiles to: a path shape on its view, in the theme's accent. */
  insertingIcon(source: string, state: string, icon: Icon, at: [number, number], format?: string): Promise<Added> {
    return this.request<"adding">({ type: "insertingIcon", id: ++this.asked, source, state, icon, at, format }).then(({ added }) => added);
  }

  /** The patch that draws what `inserts` offers `n`th, entering in `state`, in the box a drag
   * from `from` to `to` covers (canvas units): snapped to the theme's grid as a resize snaps, or,
   * `free`, where it was drawn (PLAN 2.48); on the deck `source` compiles to. */
  drawing(source: string, state: string, n: number, [from, to]: [[number, number], [number, number]], free: boolean, format?: string): Promise<Added> {
    return this.request<"adding">({ type: "drawing", id: ++this.asked, source, state, n, from, to, free, format }).then(({ added }) => added);
  }

  /** The patch that draws a stroke of the pen in `state` (PLAN 2.114), `points` in canvas units as
   * the pointer gave them: one more stroke of the drawing `onto` where the state shows it as one,
   * else a new drawing where the stroke was drawn; on the deck `source` compiles to. */
  inking(source: string, state: string, points: [number, number][], onto?: string, format?: string): Promise<Added> {
    return this.request<"adding">({ type: "inking", id: ++this.asked, source, state, points: points.flat(), onto, format }).then(({ added }) => added);
  }

  /** The patch that copies `node` beside it in `state`, on the deck `source` compiles to. */
  duplicating(source: string, state: string, node: string, format?: string): Promise<Added> {
    return this.request<"adding">({ type: "duplicating", id: ++this.asked, source, state, node, format }).then(({ added }) => added);
  }

  /** The patch that copies each of `nodes` beside it in `state`, on the deck `source` compiles to
   * (PLAN 2.42): the copies' ids, and the connectors among them copied to join their copies. */
  duplicatingAll(source: string, state: string, nodes: string[], format?: string): Promise<Duplicated> {
    return this.request<"duplicated">({ type: "duplicatingAll", id: ++this.asked, source, state, nodes, format }).then(
      ({ duplicated }) => duplicated,
    );
  }

  /** The words of `shape` to type in in `state` (PLAN 2.108): the text that holds them, and the
   * patch that gives it one where it holds none, or brings them back where the state does not
   * show them. */
  wording(source: string, state: string, shape: string): Promise<Worded> {
    return this.request<"worded">({ type: "wording", id: ++this.asked, source, state, shape }).then(({ worded }) => worded);
  }

  /** The patch that joins `from` to `to` in `state` with an arrow, or a line (PLAN 2.109). */
  connecting(source: string, state: string, from: string, to: string, arrow: boolean): Promise<Connected> {
    return this.request<"connected">({ type: "connecting", id: ++this.asked, source, state, from, to, arrow }).then(
      ({ connected }) => connected,
    );
  }

  /** The patch that draws text `node`'s lines in `state` as a diagram of `kind` in its place, in
   * `format` (PLAN 2.110). */
  diagramming(source: string, state: string, node: string, kind: Diagram, format?: string): Promise<Diagrammed> {
    return this.request<"diagrammed">({ type: "diagramming", id: ++this.asked, source, state, node, kind, format }).then(
      ({ diagrammed }) => diagrammed,
    );
  }

  /** The patch that puts `nodes`, children of one container as `state` shows them, in a new group
   * where they stand, on the deck `source` compiles to (PLAN 2.43): its id and the patch. */
  grouping(source: string, state: string, nodes: string[]): Promise<Grouped> {
    return this.request<"grouped">({ type: "grouping", id: ++this.asked, source, state, nodes }).then(({ grouped }) => grouped);
  }

  /** What the clipboard holds of `nodes` as `state` shows them, on the deck `source` compiles to
   * (PLAN 2.37, 2.42): the clip, as JSON text. The first is the node copied. */
  copying(source: string, state: string, nodes: string[], format?: string): Promise<string> {
    return this.request<"copied">({ type: "copying", id: ++this.asked, source, state, nodes, format }).then(({ clip }) => clip);
  }

  /** The patch that pastes `clip`, the clipboard's text, entering in `state` about `at` (canvas
   * units), on the deck `source` compiles to (PLAN 2.37). The files it carries are in the bundle
   * once it answers. */
  pasting(source: string, state: string, clip: string, at: [number, number], format?: string, hints?: PasteHints): Promise<Pasted> {
    return this.request<"pasted">({ type: "pasting", id: ++this.asked, source, state, clip, at, format, hints }).then(({ pasted }) => pasted);
  }

  /** Each state at rest, `height` pixels high, in `format`, for the state strip (PLAN 2.35): with
   * pixels where its drawing is not the one `known` holds. With `states`, those alone, for the
   * light table (PLAN 2.97); a request for some does not stop one for all. With `each`, each is
   * handed to it as it is painted, first to last, and the answer holds the rest. */
  thumbnails(height: number, known: Record<string, string>, format?: string, states?: string[], each?: (thumb: Thumb) => void): Promise<Thumb[]> {
    const id = ++this.asked;
    if (each) this.thumbing.set(id, each);
    return this.request<"thumbnails">({ type: "thumbnails", id, height, known, format, states, each: each !== undefined })
      .then(({ thumbs }) => thumbs)
      .finally(() => this.thumbing.delete(id));
  }

  /** The patch that adds a state after `state`, a `step` of its slide or a `slide` of its own, on
   * the deck `source` compiles to (PLAN 2.35). */
  addingState(source: string, state: string, what: "step" | "slide"): Promise<{ id: string; patch: unknown[] }> {
    return this.request<"addingState">({ type: "addingState", id: ++this.asked, source, state, what }).then(({ added }) => added);
  }

  /** The slides a person may start after `state`'s slide, on the deck `source` compiles to (PLAN
   * 3.30): one in each of the theme's layouts, then a blank one, or those `layouts` names, a null
   * the blank one (PLAN 3.31), each as its layout's picture shows it, painted `height` pixels high. */
  starters(source: string, state: string, height: number, format?: string, layouts?: (string | null)[]): Promise<Starter[]> {
    return this.request<"starters">({ type: "starters", id: ++this.asked, source, state, height, format, layouts }).then(({ starters }) => starters);
  }

  /** The slides a person may start, on the deck `source` compiles to, as a picker lists them before
   * it draws any (PLAN 3.31): each of the theme's layouts, in its order, then a blank one. */
  startersListed(source: string): Promise<Starter[]> {
    return this.request<"starters">({ type: "starters", id: ++this.asked, source, state: "", height: 0, listed: true }).then(({ starters }) => starters);
  }

  /** The patch that starts a slide in `layout`, or a blank one with none, after `state`'s slide, on
   * the deck `source` compiles to (PLAN 3.30). */
  starting(source: string, state: string, layout?: string): Promise<{ id: string; patch: unknown[] }> {
    return this.request<"starting">({ type: "starting", id: ++this.asked, source, state, layout }).then(({ added }) => added);
  }

  /** The patch that fills `slot` of `state`'s layout, which waits for words, as a new slide in the
   * layout fills it, on the deck `source` compiles to (PLAN 3.30): the slot's prompt's words in its
   * role, or a list's items. */
  filling(source: string, state: string, slot: string): Promise<{ id: string; patch: unknown[] }> {
    return this.request<"filling">({ type: "filling", id: ++this.asked, source, state, slot }).then(({ filled }) => filled);
  }

  /** The slots of `state`'s layout, in `format`, with nothing placed in them, each waiting for
   * what its prompt says goes there (PLAN 3.30). */
  waits(state: string, format?: string): Promise<Waiting[]> {
    return this.request<"waiting">({ type: "waiting", id: ++this.asked, state, format }).then(({ waiting }) => waiting);
  }

  /** The patch that takes `nodes`, with what they hold, out of `state` and the states after it,
   * or, `everywhere`, out of the deck, on the deck `source` compiles to: one patch, a connector
   * going with an end that goes. */
  deleting(source: string, state: string, nodes: string[], everywhere: boolean): Promise<unknown[]> {
    return this.request<"deleting">({ type: "deleting", id: ++this.asked, source, state, nodes, everywhere }).then(({ patch }) => patch);
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
   * browser's storage (PLAN 2.4). The session goes on from the save. With `keep`, a bundle that
   * keeps no history begins one with it (PLAN 2.87). */
  save(source: string, keep = false): Promise<Extract<FromWorker, { type: "saved" }>> {
    return this.request<"saved">({ type: "save", id: ++this.asked, source, keep }).then((done) => this.saved(done));
  }

  /** Save as (PLAN 2.12): `source` saved `to` a place of its own, where the bundle is kept from
   * then on. */
  saveAs(source: string, to: SaveTo): Promise<Extract<FromWorker, { type: "saved" }>> {
    return this.request<"saved">({ type: "saveAs", id: ++this.asked, source, to }).then((done) => this.saved(done));
  }

  /** Rename the bundle the browser keeps as `from` to `to`, or with no `to` delete it (PLAN
   * 2.124); where the open bundle is kept then. */
  rekeep(from: string, to?: string): Promise<Where | undefined> {
    return this.request<"rekept">({ type: "rekeep", id: ++this.asked, from, to }).then(({ where }) => where);
  }

  /** The bundle with the deck `source` compiles to, as a `.scaena` zip with its fonts subset. */
  zip(source: string): Promise<{ bytes: ArrayBuffer; subset: [string, number, number][] }> {
    return this.request<"zipped">({ type: "zip", id: ++this.asked, source });
  }

  /** The deck `source` compiles to, exported `as` asks (PLAN 2.54): the file's bytes. */
  export(source: string, as: Export): Promise<ArrayBuffer> {
    return this.request<"exported">({ type: "export", id: ++this.asked, source, as }).then(({ bytes }) => bytes);
  }

  /** Add a file dropped on the page to the bundle; resolves to its path there. A picture an image
   * could not show, one too large among them, is refused and kept out of the bundle. */
  drop(name: string, bytes: ArrayBuffer): Promise<string> {
    return this.request<"dropped">({ type: "drop", id: ++this.asked, name, bytes }).then(({ path }) => {
      this.dropped.set(path, name);
      this.wrote([path]);
      return path;
    });
  }

  /** The name the file at `path` had when it was dropped on this page, where it was. */
  droppedAs(path: string): string | undefined {
    return this.dropped.get(path);
  }

  /** What the PNG or JPEG `bytes` are as a picture, read as the engine reads it before it is
   * dropped, so one too large goes in as a smaller copy; refused where it is no picture an
   * image shows. */
  picture(bytes: ArrayBuffer): Promise<Picture> {
    return this.request<"pictured">({ type: "picture", id: ++this.asked, bytes }).then(({ picture }) => picture);
  }

  /** A data file dropped on the canvas, into the bundle at `path`, as the source `data` of the
   * deck `source` compiles to: one it declares already (no `attached`), or one declared as
   * `data_attach` declares it, by the patch, which `make` applies; empty where it is refused. */
  attaching(source: string, name: string, bytes: ArrayBuffer, schema?: Record<string, string>): Promise<{ path: string; data: string; attached: Attached | null; patch: unknown[] }> {
    return this.request<"attached">({ type: "attaching", id: ++this.asked, source, name, bytes, schema }).then(({ path, data, attached, patch }) => {
      if (attached) this.wrote([path]);
      return { path, data, attached, patch };
    });
  }

  /** `text` pasted on the canvas as a sheet's cells, read in the language of the deck `source`
   * compiles to (PLAN 2.96): the data source they would be, or null where `text` is not cells. */
  cells(source: string, text: string): Promise<Cells | null> {
    return this.request<"cells">({ type: "cells", id: ++this.asked, source, text }).then(({ cells }) => cells);
  }

  /** Ask the assistant `asking.text` about the deck `source` says, which must compile and
   * validate (PLAN 2.6). `hear` hears each step; the promise holds the last, `done` or
   * `failed`, and fails if the question could not be asked. */
  ask(source: string, asking: Asking, hear: (event: AssistantEvent) => void, drawn?: Drawn): Promise<AssistantEvent> {
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
      this.send({ type: "ask", id, source, ask: asking, drawn });
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
    this.thumbing.clear();
  }

  /** Stop the worker, and refuse every request waiting on it with `error`: it answers no more. */
  private halt(error: Error) {
    const waiting = [...this.waiting.values()];
    this.close();
    for (const { reject } of waiting) reject(error);
  }

  /** Refuse every request waiting on the worker with `error`, and keep it. */
  private refuse(error: Error) {
    const waiting = [...this.waiting.values()];
    this.waiting.clear();
    this.hearing.clear();
    this.thumbing.clear();
    for (const { reject } of waiting) reject(error);
  }

  /** Fail outside a message and answer nothing more, as a worker out of memory does, or nothing
   * for `busy` ms, as a live one in a long task does, to test the page's recovery (lift-up 10-24b
   * STAB-2). */
  die(busy?: number) {
    this.send({ type: "die", busy });
  }

  /** Panic in the engine, to test the page's recovery (lift-up STAB-2): the editor's module alone
   * carries it. */
  trap() {
    this.send({ type: "trap" });
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

  /** The page's version of the source (the editor's `version`): sent with each request that
   * carries a source, so the worker answers a request no newer than its deck from that deck. */
  sourceVersion: () => number | undefined = () => undefined;

  private send(message: ToWorker) {
    const version = "source" in message && typeof message.source === "string" ? this.sourceVersion() : undefined;
    this.worker.postMessage(version === undefined ? message : { ...message, seen: version });
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
      case "rekept":
      case "zipped":
      case "exported":
      case "dropped":
      case "pictured":
      case "attached":
      case "cells":
      case "models":
      case "reloaded":
      case "boxes":
      case "hits":
      case "marked":
      case "noted":
      case "calledOut":
      case "outlined":
      case "framed":
      case "markedIn":
      case "linked":
      case "laidOut":
      case "viewed":
      case "fitted":
      case "focal":
      case "found":
      case "replacement":
      case "targets":
      case "grid":
      case "dragged":
      case "arranged":
      case "grouped":
      case "made":
      case "choices":
      case "stateChoices":
      case "writing":
      case "written":
      case "wrote":
      case "slideAsked":
      case "slideWritten":
      case "words":
      case "notes":
      case "look":
      case "put":
      case "bundleFiles":
      case "health":
      case "restated":
      case "pictureMade":
      case "mapTargeted":
      case "mapFetched":
      case "mapped":
      case "mapRefresh":
      case "removed":
      case "versions":
      case "version":
      case "compared":
      case "restored":
      case "filesWritten":
      case "file":
      case "themeText":
      case "codeColors":
      case "themeEdited":
      case "carets":
      case "characterChoices":
      case "bolding":
      case "codeWords":
      case "tidied":
      case "keyed":
      case "coding":
      case "uncoding":
      case "tidying":
      case "focusing":
      case "walking":
      case "italicizing":
      case "themes":
      case "rethemed":
      case "reached":
      case "typed":
      case "inserts":
      case "layers":
      case "adding":
      case "duplicated":
      case "worded":
      case "connected":
      case "diagrammed":
      case "deleting":
      case "copied":
      case "pasted":
      case "thumbnails":
      case "addingState":
      case "starters":
      case "starting":
      case "waiting":
      case "filling":
      case "layoutSuggestions":
      case "backgrounds":
      case "backgroundFrames":
      case "backgroundPatch":
      case "sheet":
      case "dataEdited":
      case "dataUndone":
        this.waiting.get(data.id)?.resolve(data);
        this.waiting.delete(data.id);
        return;
      case "assistant":
        this.hearing.get(data.id)?.(data.event);
        return;
      case "thumbed":
        this.thumbing.get(data.id)?.(data.thumb);
        return;
      case "pong":
        this.waiting.get(data.id)?.resolve(data);
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
      case "crashed": {
        this.halt(new EngineStopped(`the engine stopped: ${data.message}`));
        this.onCrashed({ message: data.message, name: data.name, files: data.files });
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
      case "besides":
        return this.onBesides(data.state, data.t);
      case "sized":
        return this.onSized(data.size);
      case "ready":
        return;
      // What a worker that makes a deck from a prompt says (PLAN G.5): a stage's never does.
      case "briefing":
      case "generating":
      case "storyOutlined":
      case "generated":
        return;
      default: {
        // A reply the switch does not name would leave its request waiting: the compiler says so.
        const unheard: never = data;
        return unheard;
      }
    }
  }
}
