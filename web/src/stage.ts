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
  DataEdited,
  DataMark,
  DataSource,
  Edited,
  Export,
  Found,
  FromWorker,
  Grid,
  Grouped,
  Hit,
  Insert,
  Inspected,
  Layer,
  LayoutSuggestion,
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
  Themes,
  Thumb,
  Rewritten,
  ToWorker,
  Carets,
  Choices,
  StateChoices,
  Look,
  Put,
  BundleFile,
  Compared,
  Version,
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
      | "exported"
      | "sheet"
      | "dataEdited"
      | "dataUndone"
      | "dropped"
      | "attached"
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
      | "look"
      | "put"
      | "bundleFiles"
      | "removed"
      | "versions"
      | "version"
      | "compared"
      | "restored"
      | "filesWritten"
      | "themeText"
      | "themeEdited"
      | "carets"
      | "characterChoices"
      | "bolding"
      | "italicizing"
      | "themes"
      | "rethemed"
      | "reached"
      | "typed"
      | "inserts"
      | "layers"
      | "adding"
      | "deleting"
      | "copied"
      | "pasted"
      | "thumbnails"
      | "addingState"
      | "layoutSuggestions";
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
  /** The formats beside the canvas are painted: `state`, `t` ms into its cue (PLAN 2.62). */
  onBesides: (state: string, t: number) => void = () => {};
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
  make(source: string, ops: unknown[], index: number, format?: string): Promise<{ source: string; edited: Edited }> {
    return this.request<"made">({ type: "make", id: ++this.asked, source, ops, index, format }).then(({ source, edited }) => {
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
        return { result, source, edited };
      },
    );
  }

  /** The bundle's images, fonts, and data, in the deck `source` compiles to (PLAN 2.59): each with
   * what names it, and the nodes drawn from it in the states that show them so. */
  bundleFiles(source: string): Promise<BundleFile[]> {
    return this.request<"bundleFiles">({ type: "bundleFiles", id: ++this.asked, source }).then(({ files }) => files);
  }

  /** `path` taken out of the bundle, by the user (PLAN 2.59): one nothing names. `dataUndo` puts it
   * back. What the deck came to, shown and linted as an edit is. */
  removeFile(source: string, path: string, index: number, format?: string): Promise<Edited> {
    return this.request<"removed">({ type: "removeFile", id: ++this.asked, source, path, index, format }).then(({ edited }) => {
      this.moved(edited);
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
      return done;
    });
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
      return done;
    });
  }

  /** Files written back, as an undo or a redo of a restore or a theme edit has them; with `edit`,
   * the deck its source compiles to shown and linted again, as an edit is. */
  writeFiles(files: { path: string; text: string | null }[], edit?: { source: string; index: number; format?: string }): Promise<Edited | undefined> {
    return this.request<"filesWritten">({ type: "writeFiles", id: ++this.asked, files, edit }).then(({ edited }) => {
      if (edited) this.moved(edited);
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

  /** The layouts `state` may take, best first, in the deck shown (PLAN 2.92), as `stateChoices`
   * offers them: each judged by lint, with the patch that gives it, and painted at rest `height`
   * pixels high. The deck the worker holds: a request on a timer never takes it back to a source
   * an edit has moved past. */
  layoutSuggestions(state: string, height: number, format?: string): Promise<LayoutSuggestion[]> {
    return this.request<"layoutSuggestions">({ type: "layoutSuggestions", id: ++this.asked, state, height, format }).then(
      ({ suggestions }) => suggestions,
    );
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
  inserting(source: string, state: string, n: number, at: [number, number], format?: string, named?: string): Promise<Added> {
    return this.request<"adding">({ type: "inserting", id: ++this.asked, source, state, n, at, format, named }).then(({ added }) => added);
  }

  /** The patch that draws what `inserts` offers `n`th, entering in `state`, in the box a drag
   * from `from` to `to` covers (canvas units): snapped to the theme's grid as a resize snaps, or,
   * `free`, where it was drawn (PLAN 2.48); on the deck `source` compiles to. */
  drawing(source: string, state: string, n: number, [from, to]: [[number, number], [number, number]], free: boolean, format?: string): Promise<Added> {
    return this.request<"adding">({ type: "drawing", id: ++this.asked, source, state, n, from, to, free, format }).then(({ added }) => added);
  }

  /** The patch that copies `node` beside it in `state`, on the deck `source` compiles to. */
  duplicating(source: string, state: string, node: string, format?: string): Promise<Added> {
    return this.request<"adding">({ type: "duplicating", id: ++this.asked, source, state, node, format }).then(({ added }) => added);
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
   * browser's storage (PLAN 2.4). The session goes on from the save. With `keep`, a bundle that
   * keeps no history begins one with it (PLAN 2.87). */
  save(source: string, keep = false): Promise<Extract<FromWorker, { type: "saved" }>> {
    return this.request<"saved">({ type: "save", id: ++this.asked, source, keep });
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

  /** The deck `source` compiles to, exported `as` asks (PLAN 2.54): the file's bytes. */
  export(source: string, as: Export): Promise<ArrayBuffer> {
    return this.request<"exported">({ type: "export", id: ++this.asked, source, as }).then(({ bytes }) => bytes);
  }

  /** Add a file dropped on the page to the bundle; resolves to its path there. */
  drop(name: string, bytes: ArrayBuffer): Promise<string> {
    return this.request<"dropped">({ type: "drop", id: ++this.asked, name, bytes }).then(({ path }) => path);
  }

  /** A data file dropped on the canvas, into the bundle at `path`, as the source `data` of the
   * deck `source` compiles to: one it declares already (no `attached`), or one declared as
   * `data_attach` declares it, by the patch, which `make` applies; empty where it is refused. */
  attaching(source: string, name: string, bytes: ArrayBuffer): Promise<{ path: string; data: string; attached: Attached | null; patch: unknown[] }> {
    return this.request<"attached">({ type: "attaching", id: ++this.asked, source, name, bytes }).then(({ path, data, attached, patch }) => ({ path, data, attached, patch }));
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
      case "exported":
      case "dropped":
      case "attached":
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
      case "look":
      case "put":
      case "bundleFiles":
      case "removed":
      case "versions":
      case "version":
      case "compared":
      case "restored":
      case "filesWritten":
      case "themeText":
      case "themeEdited":
      case "carets":
      case "characterChoices":
      case "bolding":
      case "italicizing":
      case "themes":
      case "rethemed":
      case "reached":
      case "typed":
      case "inserts":
      case "layers":
      case "adding":
      case "deleting":
      case "copied":
      case "pasted":
      case "thumbnails":
      case "addingState":
      case "layoutSuggestions":
      case "sheet":
      case "dataEdited":
      case "dataUndone":
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
      case "besides":
        return this.onBesides(data.state, data.t);
      case "ready":
        return;
      default: {
        // A reply the switch does not name would leave its request waiting: the compiler says so.
        const unheard: never = data;
        return unheard;
      }
    }
  }
}
