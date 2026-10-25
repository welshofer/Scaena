// The engine's worker (PLAN 2.1–2.5, SPEC §9.2): the bundle, the WASM engine, and the canvas
// the page handed over. It paints with vello on WebGPU where the browser has an adapter, and
// otherwise with vello_cpu, whose frames go onto the canvas by its 2D context. It keeps the
// deck's clock: a run plays the global timeline (SPEC §2.4), cue by cue and hold by hold. For
// the editor it compiles `.scn` as it is typed, lints it, fixes it, and inspects a state; it
// saves the bundle where it is kept, zips it, and takes files dropped on the page; and the
// assistant works here, with the bundle's tools (PLAN 2.6).
//
// A single-file export (PLAN 2.5) builds it as a classic script, against the player's module
// alone, which its page hands over compiled with the bundle's files: its page is a file, and
// a browser starts no module worker from a file's page.
//
// The CPU painter shares a shader's rows with helpers, each a worker started as this one was,
// holding the engine's module and no deck (PLAN 2.28): a worker whose first message is `help`
// is one.
import { codeTables } from "@scaena/code-tables";
import { hyphenation, verse, weatherIcons } from "@scaena/hyphenation";
import init, {
  Canvas,
  Player,
  engineModule,
  setCodeTables,
  setHyphenation,
  setIcons,
  setVerse,
  shaderRows,
} from "@scaena/wasm";
import { deleteKept, keptBundle, newBundle, readAll, remove, renameKept, write } from "./folders";
import type { Decision, Outlined } from "./generate";
import { finding } from "./words";
import type {
  SlideWritten,
  Added,
  Asking,
  AssistantEvent,
  Attached,
  Carets,
  Cells,
  Choices,
  DataEdited,
  DataSource,
  Drawn,
  Edited,
  Export,
  Finding,
  FromHelper,
  FromWorker,
  Generate,
  Insert,
  Keyed,
  Layer,
  LayoutSuggestion,
  Backgrounds,
  BackgroundOption,
  Thumb,
  Opened,
  SpeakerNotes,
  Painter,
  Pasted,
  Picture,
  Look,
  Put,
  BundleFile,
  Grounded,
  MapRefresh,
  MapTarget,
  Mapped,
  Compared,
  Restored,
  Rewritten,
  Grid,
  Grouped,
  Line,
  RowEdit,
  Section,
  Sheet,
  Slot,
  Snapped,
  Arrange,
  Arranged,
  Source,
  Starter,
  StateChoices,
  Lexicon,
  Themed,
  ThemeEdited,
  ThemeText,
  CodeColor,
  Themes,
  ToHelper,
  ToWorker,
  Version,
  Waiting,
  Worded,
  Connected,
  Duplicated,
  Diagrammed,
  Where,
  Writing,
  Written,
  Serving,
} from "./protocol";

const post = (message: FromWorker, transfer: Transferable[] = []) => self.postMessage(message, transfer);
const said = (e: unknown) => (e instanceof Error ? e.message : String(e));

let player: Player;
/** The bundle's files as the session holds them (lift-up STAB-2), kept in step after each message
 * by the session's edition of them: what a fresh worker goes on from when the engine panics.
 * wasm32 does not unwind: a panic traps, and the session is not used again. */
let mirror = new Map<string, Uint8Array>();
let mirrored: { of: Player; edition: number } | undefined;
/** The panic's words, once the engine has stopped: every request after is refused with them. */
let stopped: string | undefined;
/** The last panic the engine's hook said, which the console hears first. */
let panicked: string | undefined;
const consoleError = console.error.bind(console);
console.error = (...args: unknown[]) => {
  const text = String(args[0] ?? "");
  if (text.startsWith("panicked at")) panicked = text.split("\n").slice(1).join(" ").trim() || text;
  consoleError(...args);
};
self.addEventListener("error", (e) => {
  if (e.error instanceof WebAssembly.RuntimeError) crashed(e.error);
});
self.addEventListener("unhandledrejection", (e) => {
  if (e.reason instanceof WebAssembly.RuntimeError) crashed(e.reason);
});

/** The mirror of the session's files, kept in step: every file copied again when their edition
 * changed, or the session is another. */
function mirrorFiles() {
  if (!player || stopped !== undefined) return;
  const edition = player.filesEdition();
  if (mirrored?.of === player && mirrored.edition === edition) return;
  const next = new Map<string, Uint8Array>();
  for (const path of player.files()) {
    const bytes = player.file(path);
    if (bytes) next.set(path, bytes);
  }
  mirror = next;
  mirrored = { of: player, edition };
}

/** The engine stopped on a panic: the page hears, with the bundle's files as they were, and every
 * request after is refused. */
function crashed(e: unknown) {
  if (stopped !== undefined) return;
  stopped = panicked ?? said(e);
  asking?.abort();
  const files: Record<string, ArrayBuffer> = {};
  const transfer: ArrayBuffer[] = [];
  for (const [path, bytes] of mirror) {
    const whole = bytes.byteOffset === 0 && bytes.byteLength === bytes.buffer.byteLength;
    const buffer = whole ? (bytes.buffer as ArrayBuffer) : (bytes.slice().buffer as ArrayBuffer);
    files[path] = buffer;
    transfer.push(buffer);
  }
  mirror = new Map();
  post({ type: "crashed", message: stopped, name, files }, transfer);
}
/** The bundle's name, and where it is kept, if anywhere: a folder on disk, one in the
 * browser's storage, or the folder `scaena serve` serves it from. */
let name = "deck";
let home: Home | undefined;
/** Where the bundle was opened from, which `reload` reads again. */
let from: Source;
let canvas: OffscreenCanvas;
/** WebGPU, when it paints. */
let gpu: Canvas | undefined;
/** Where the CPU painter's frames go, when it paints. */
let raster: OffscreenCanvasRenderingContext2D | undefined;
/** The format the engine lays frames out in now (`undefined`: the deck's own canvas), and
 * the deck's timeline in it. */
let format: string | undefined;
let slots: Slot[] = [];
/** Counts the requests that paint: a run stops when a newer one comes. */
let latest = 0;
/** The round of layouts being judged (PLAN 2.92): one asked for while another is judged ends it. */
let judging = 0;
/** The states a single-file export plays, in its order; every state without it. */
let only: string[] | undefined;

/** The words of the state last asked for (PLAN 2.99), kept while only reads come: the inspector,
 * the layers, and the canvas each ask for them after an edit. */
let worded: { state: string; words: Lexicon } | undefined;
/** What only reads the deck and its theme. Any other message may change either, and the words
 * kept go, before it and after it. */
const READS = new Set<ToWorker["type"]>([
  "boxes",
  "calloutAt",
  "characterChoices",
  "choices",
  "framing",
  "grid",
  "hit",
  "inserts",
  "inspect",
  "layers",
  "layout",
  "linkAt",
  "markAt",
  "marksIn",
  "marksOf",
  "noteAt",
  "outline",
  "read",
  "seek",
  "show",
  "stateChoices",
  "targets",
  "themes",
  "thumbnails",
  "timeline",
  "view",
  "waiting",
  "words",
  "writing",
  "written",
  "slideAsk",
]);

self.onmessage = async ({ data }: MessageEvent<ToWorker>) => {
  const id = "id" in data ? data.id : undefined;
  const reads = READS.has(data.type);
  if (!reads) worded = undefined;
  if (stopped !== undefined) {
    if (id !== undefined) post({ type: "error", id, message: `the engine stopped: ${stopped}` });
    return;
  }
  // A request that carries the editor's source waits out an edit in flight, so nothing compiles
  // over the deck an edit compiled while its frame paints: it lints that deck and no other.
  while (editing && typeof (data as { source?: unknown }).source === "string") await editing;
  // Taken now: what it carries is read against the deck as it stands.
  const carried = (data as { seen?: unknown }).seen;
  if (typeof carried === "number") asked = carried;
  try {
    switch (data.type) {
      case "open":
        return await open(data.source, data.painter, data.canvas, data.engine);
      case "trap":
        return player.trap();
      case "briefing": {
        await init(data.engine && { module_or_path: data.engine });
        const { choices } = await import("@scaena/generate");
        return post({ type: "briefing", choices: await choices() });
      }
      case "generate":
        return await generating(data.generate, data.engine);
      // The person's decision on the outline shown (narrative FUNC-1): the generation waits on it.
      case "storySettled":
        deciding?.({ settled: data.outline });
        deciding = undefined;
        return;
      case "outlineAgain":
        deciding?.({ arc: data.arc });
        deciding = undefined;
        return;
      case "reload": {
        if (!("url" in from)) throw new Error("only a bundle read from a URL is read again");
        latest++;
        const was = player;
        player = await fetched(from.url, from.serve);
        await library();
        // A frame in flight finishes on the engine it began with.
        await painting;
        was.free();
        format = undefined;
        slots = opening();
        sized(true);
        return post({ type: "reloaded", id: data.id, ...opened() });
      }
      case "show": {
        latest++;
        const start = performance.now();
        layOut(data.format);
        await paint(data.state, data.t ?? Infinity);
        return answerShown(data.id, start);
      }
      case "timeline":
        layOut(data.format);
        return post({ type: "timeline", id: data.id, slots });
      case "read":
        layOut(data.format);
        return post({ type: "reading", id: data.id, html: player.reading(data.state) });
      case "linkAt": {
        layOut(data.format);
        const [w, h] = player.canvasSize();
        return post({ type: "linked", id: data.id, link: JSON.parse(player.linkAt(data.state, data.at[0] * w, data.at[1] * h)) });
      }
      case "run":
        layOut(data.format);
        return run(data.index, data.t, ++latest, data.still, data.alone);
      case "seek": {
        latest++;
        layOut(data.format);
        const slot = slots[data.index];
        if (!slot) throw new Error(`the deck has no slot ${data.index}: it has ${slots.length}`);
        shown = { index: data.index, format: data.format };
        const t = data.t ?? slot.span;
        await paint(slot.state, t);
        return post({ type: "at", id: data.id, index: data.index, t, global: slot.start + t, playing: false });
      }
      case "pause":
        latest++;
        return;
      case "source":
        return post({ type: "source", id: data.id, source: player.source() });
      case "edit": {
        latest++;
        shown = { index: data.index, format: data.format };
        // An edit no newer than the deck held (the linter's, of the source the page had while a
        // patch was made on it) is answered from that deck, never compiled back over it.
        const source = stale(data.source) ? player.source() : data.source;
        return post({ type: "edited", id: data.id, ...(await edit(source, data.index, data.format)) });
      }
      case "lint": {
        const start = performance.now();
        const now = Math.floor(Date.now() / 1000);
        const linted = JSON.parse(player.lint(undefined, now)) as { findings: Finding[]; laid: boolean; whole: boolean };
        return post({ type: "linted", id: data.id, ...linted, ms: performance.now() - start });
      }
      case "fix":
        return post({ type: "fixed", id: data.id, source: player.fix(JSON.stringify(data.patch)) });
      case "inspect":
        layOut(data.format);
        return post({ type: "inspected", id: data.id, inspected: JSON.parse(player.inspect(data.state)) });
      case "boxes": {
        layOut(data.format);
        const [width, height] = player.canvasSize();
        return post({ type: "boxes", id: data.id, boxes: JSON.parse(player.boxes(data.state)), size: [width, height] });
      }
      case "hit":
        layOut(data.format);
        return post({ type: "hits", id: data.id, hits: JSON.parse(player.hit(data.state, ...data.point)) });
      case "markAt": {
        layOut(data.format);
        const mark = JSON.parse(player.markAt(data.state, ...data.point));
        return post({ type: "marked", id: data.id, marks: mark ? [mark] : [] });
      }
      case "marksOf":
        layOut(data.format);
        return post({ type: "marked", id: data.id, marks: JSON.parse(player.marksOf(data.state, data.source, Uint32Array.from(data.rows))) });
      case "noteAt":
        layOut(data.format);
        return post({ type: "noted", id: data.id, note: JSON.parse(player.noteAt(data.state, ...data.point)) });
      case "layout":
        layOut(data.format);
        return post({ type: "laidOut", id: data.id, layout: JSON.parse(player.layout(data.state)) });
      case "marksIn":
        layOut(data.format);
        return post({ type: "markedIn", id: data.id, found: JSON.parse(player.marksIn(data.state, data.node)) });
      case "framing":
        layOut(data.format);
        return post({ type: "framed", id: data.id, framing: JSON.parse(player.framing(data.state, data.node)) });
      case "outline":
        layOut(data.format);
        return post({ type: "outlined", id: data.id, outline: JSON.parse(player.outline(data.state, data.node)) });
      case "calloutAt":
        layOut(data.format);
        return post({ type: "calledOut", id: data.id, at: JSON.parse(player.calloutAt(data.state, data.node, ...data.point)) });
      case "view": {
        viewing = data.view ? Float32Array.from(data.view) : null;
        if (lastPainted) await paint(lastPainted.state, lastPainted.t, "keep");
        return post({ type: "viewed", id: data.id });
      }
      case "fit": {
        room = data.room;
        if (lastPainted) await paint(lastPainted.state, lastPainted.t, "keep");
        else sized(true);
        return post({ type: "fitted", id: data.id, size: [canvas.width, canvas.height] });
      }
      case "find":
        current(data.source);
        return post({ type: "found", id: data.id, found: JSON.parse(player.find(JSON.stringify(data.query))) });
      case "replacing": {
        current(data.source);
        const one = data.one ? Uint32Array.from(data.one) : undefined;
        return post({ type: "replacement", id: data.id, ops: JSON.parse(player.replacing(JSON.stringify(data.query), data.with, one)) });
      }
      case "focalAt":
        current(data.source);
        layOut(data.format);
        return post({ type: "focal", id: data.id, at: JSON.parse(player.focalAt(data.state, data.node, ...data.point)) });
      case "targets":
        layOut(data.format);
        return post({ type: "targets", id: data.id, targets: JSON.parse(player.targets(data.state, data.node)) });
      case "grid":
        layOut(data.format);
        return post({ type: "grid", id: data.id, grid: JSON.parse(player.grid()) as Grid });
      case "drag":
        layOut(data.format);
        return post({ type: "dragged", id: data.id, ...(await drag(data)) });
      case "arrange":
        layOut(data.format);
        return post({ type: "arranged", id: data.id, arranged: arranging(data.state, data.nodes, data.how, data.fork) });
      case "rest": {
        latest++;
        layOut(data.format);
        player.setMoving([], 0, 0);
        player.preview(undefined);
        const start = performance.now();
        await paint(data.state, Infinity);
        return answerShown(data.id, start);
      }
      case "make":
        return post({ type: "made", id: data.id, ...(await make(data.source, data.ops, data.index, data.format)) });
      case "choices":
        return post({ type: "choices", id: data.id, choices: JSON.parse(player.choices(data.state, data.node)) as Choices });
      case "stateChoices":
        return post({ type: "stateChoices", id: data.id, choices: JSON.parse(player.stateChoices(data.state)) as StateChoices });
      case "writing":
        return post({ type: "writing", id: data.id, writing: JSON.parse(player.writing(JSON.stringify(data.ask))) as Writing });
      case "written":
        return post({ type: "written", id: data.id, written: JSON.parse(player.written(JSON.stringify(data.ask), data.answer)) as Written });
      case "words":
        if (worded?.state !== data.state) worded = { state: data.state, words: JSON.parse(player.words(data.state)) as Lexicon };
        return post({ type: "words", id: data.id, words: worded.words });
      case "notes":
        return post({ type: "notes", id: data.id, notes: JSON.parse(player.notes(data.state)) as SpeakerNotes });
      case "layoutSuggestions": {
        layOut(data.format);
        // A layout at a time, each a lint of the state in every format and a frame: what else the
        // page asks is answered between them. An edit meanwhile ends the round, an error, and so
        // does a round asked for after it, which is the one judged.
        const round = ++judging;
        player.layoutsBegin(data.state);
        do {
          await new Promise((next) => setTimeout(next));
          if (round !== judging) throw new Error("the layouts are judged again, as asked since");
        } while (player.layoutsStep());
        const listed = JSON.parse(player.layoutSuggestions(data.height)) as Omit<LayoutSuggestion, "pixels">[];
        const suggestions = listed.map((s, i) => ({ ...s, pixels: player.layoutPixels(i).buffer as ArrayBuffer }));
        return post({ type: "layoutSuggestions", id: data.id, suggestions }, suggestions.map((s) => s.pixels));
      }
      case "backgrounds": {
        layOut(data.format);
        const listed = JSON.parse(player.backgroundsPainted(data.state, data.height)) as Omit<Backgrounds, "options"> & {
          options: Omit<BackgroundOption, "pixels">[];
        };
        const options = listed.options.map((o, i) => ({ ...o, pixels: player.backgroundPixels(i, 0).buffer as ArrayBuffer }));
        return post({ type: "backgrounds", id: data.id, backgrounds: { ...listed, options } }, options.map((o) => o.pixels));
      }
      case "backgroundPatch": {
        const patch = JSON.parse(player.backgroundPatch(data.state, data.preset)) as unknown[];
        return post({ type: "backgroundPatch", id: data.id, patch });
      }
      case "backgroundFrames": {
        const pixels: ArrayBuffer[] = [];
        for (let i = 0; ; i++) {
          const painted = player.backgroundPixels(i, data.t);
          if (!painted.length) break;
          pixels.push(painted.buffer as ArrayBuffer);
        }
        return post({ type: "backgroundFrames", id: data.id, pixels }, pixels);
      }
      case "preview": {
        latest++;
        layOut(data.format);
        player.setMoving([], 0, 0);
        player.preview(JSON.stringify(data.ops));
        const start = performance.now();
        // The formats beside show the deck as it is: nothing is made.
        await paint(data.state, Infinity, "keep");
        return answerShown(data.id, start);
      }
      case "look":
        current(data.source);
        return post({ type: "look", id: data.id, look: JSON.parse(player.look(data.state, data.node)) as Look });
      case "putting": {
        current(data.source);
        const put = JSON.parse(player.putting(data.state, JSON.stringify(data.look), data.nodes)) as Put;
        return post({ type: "put", id: data.id, put });
      }
      case "reach":
        return post({ type: "reached", id: data.id, states: JSON.parse(player.reach(JSON.stringify(data.ops))) as string[] });
      case "inserts":
        return post({ type: "inserts", id: data.id, inserts: JSON.parse(player.inserts()) as Insert[] });
      case "layers":
        return post({ type: "layers", id: data.id, layers: JSON.parse(player.layers(data.state)) as Layer[] });
      case "inserting":
        current(data.source);
        layOut(data.format);
        return post({
          type: "adding",
          id: data.id,
          added: JSON.parse(player.inserting(data.state, data.n, ...data.at, data.named, data.with && JSON.stringify(data.with))) as Added,
        });
      case "insertingIcon":
        current(data.source);
        layOut(data.format);
        return post({ type: "adding", id: data.id, added: JSON.parse(player.insertingIcon(data.state, JSON.stringify(data.icon), ...data.at)) as Added });
      case "drawing":
        current(data.source);
        layOut(data.format);
        return post({ type: "adding", id: data.id, added: JSON.parse(player.drawing(data.state, data.n, ...data.from, ...data.to, data.free)) as Added });
      case "inking":
        current(data.source);
        layOut(data.format);
        return post({ type: "adding", id: data.id, added: JSON.parse(player.inking(data.state, new Float32Array(data.points), data.onto)) as Added });
      case "duplicating":
        current(data.source);
        layOut(data.format);
        return post({ type: "adding", id: data.id, added: JSON.parse(player.duplicating(data.state, data.node)) as Added });
      case "duplicatingAll":
        current(data.source);
        layOut(data.format);
        return post({ type: "duplicated", id: data.id, duplicated: JSON.parse(player.duplicatingAll(data.state, data.nodes)) as Duplicated });
      case "wording":
        current(data.source);
        return post({ type: "worded", id: data.id, worded: JSON.parse(player.wording(data.state, data.shape)) as Worded });
      case "connecting":
        current(data.source);
        return post({
          type: "connected",
          id: data.id,
          connected: JSON.parse(player.connecting(data.state, data.from, data.to, data.arrow)) as Connected,
        });
      case "diagramming":
        current(data.source);
        layOut(data.format);
        return post({
          type: "diagrammed",
          id: data.id,
          diagrammed: JSON.parse(player.diagramming(data.state, data.node, data.kind)) as Diagrammed,
        });
      case "grouping":
        current(data.source);
        return post({ type: "grouped", id: data.id, grouped: JSON.parse(player.grouping(data.state, data.nodes)) as Grouped });
      case "copying":
        current(data.source);
        layOut(data.format);
        return post({ type: "copied", id: data.id, clip: player.copying(data.state, data.nodes) });
      case "pasting":
        current(data.source);
        layOut(data.format);
        return post({ type: "pasted", id: data.id, pasted: JSON.parse(player.pasting(data.clip, data.state, ...data.at, JSON.stringify(data.hints ?? {}))) as Pasted });
      case "thumbnails": {
        layOut(data.format);
        const each = data.each ? (thumb: Thumb) => post({ type: "thumbed", id: data.id, thumb }, thumb.pixels ? [thumb.pixels] : []) : undefined;
        const thumbs = await thumbnails(data.height, data.known, data.states, each);
        return post({ type: "thumbnails", id: data.id, thumbs }, thumbs.flatMap((t) => (t.pixels ? [t.pixels] : [])));
      }
      case "addingState":
        current(data.source);
        return post({ type: "addingState", id: data.id, added: JSON.parse(player.addingState(data.state, data.what)) as { id: string; patch: unknown[] } });
      case "starters": {
        current(data.source);
        if (data.listed) {
          const offered = JSON.parse(player.starters()) as Omit<Starter, "pixels" | "width" | "height">[];
          return post({ type: "starters", id: data.id, starters: offered.map((s) => ({ ...s, width: 0, height: 0, pixels: new ArrayBuffer(0) })) });
        }
        layOut(data.format);
        const only = data.layouts ? JSON.stringify(data.layouts) : undefined;
        const listed = JSON.parse(player.startersPainted(data.state, data.height, only)) as Omit<Starter, "pixels">[];
        const starters = listed.map((s, i) => ({ ...s, pixels: player.starterPixels(i).buffer as ArrayBuffer }));
        return post({ type: "starters", id: data.id, starters }, starters.map((s) => s.pixels));
      }
      case "starting":
        current(data.source);
        return post({ type: "starting", id: data.id, added: JSON.parse(player.starting(data.state, data.layout)) as { id: string; patch: unknown[] } });
      case "waiting":
        layOut(data.format);
        return post({ type: "waiting", id: data.id, waiting: JSON.parse(player.waiting(data.state)) as Waiting[] });
      case "filling":
        current(data.source);
        return post({ type: "filling", id: data.id, filled: JSON.parse(player.filling(data.state, data.slot)) as { id: string; patch: unknown[] } });
      case "deleting":
        current(data.source);
        return post({ type: "deleting", id: data.id, patch: JSON.parse(player.deleting(data.state, data.nodes, data.everywhere)) as unknown[] });
      case "carets": {
        // The source as it stands, where the deck shown is not yet the one it compiles to: an
        // undo, or the source changed under the text.
        const behind = !held(data.source);
        if (behind) saveable(data.source);
        layOut(data.format, behind);
        const carets = JSON.parse(player.carets(data.state, data.node)) as Carets | null;
        // Words still as their slot's prompt put them, which typing replaces whole (PLAN 3.30).
        if (carets && player.prompted(data.state, data.node)) carets.prompt = true;
        return post({ type: "carets", id: data.id, carets });
      }
      case "themes":
        return post({ type: "themes", id: data.id, themes: JSON.parse(player.themes()) as Themes });
      case "retheme":
        return post({ type: "rethemed", id: data.id, ...(await retheme(data.source, data.theme, data.index, data.format)) });
      case "characterChoices": {
        const choices = JSON.parse(player.characterChoices(data.state, data.node, data.from, data.to)) as Choices;
        return post({ type: "characterChoices", id: data.id, choices });
      }
      case "tidied":
        return post({ type: "tidied", id: data.id, text: player.tidied(data.text, data.language) });
      case "keyed": {
        const keyed = JSON.parse(Player.keyed(data.text, data.from, data.to, data.key, data.tab, data.language)) as Keyed | null;
        return post({ type: "keyed", id: data.id, keyed });
      }
      // Make Code, Not Code, and Tidy Code (PLAN 2.139): each the patch the page then makes.
      case "coding":
      case "uncoding":
      case "tidying": {
        const behind = !player.compiledFrom(data.source);
        if (behind) saveable(data.source);
        const patch =
          data.type === "coding"
            ? player.coding(data.node, data.language)
            : data.type === "uncoding"
              ? player.uncoding(data.node)
              : player.tidying(data.state, data.node);
        return post({ type: data.type, id: data.id, patch: JSON.parse(patch) as unknown[] });
      }
      case "codeWords": {
        const behind = !held(data.source);
        if (behind) saveable(data.source);
        layOut(data.format, behind);
        return post({ type: "codeWords", id: data.id, words: player.codeWords(data.state, data.node) });
      }
      case "bolding": {
        const behind = !held(data.source);
        if (behind) saveable(data.source);
        layOut(data.format, behind);
        const look = JSON.parse(player.bolding(data.state, data.node, data.from, data.to)) as Record<string, unknown>;
        return post({ type: "bolding", id: data.id, look });
      }
      case "italicizing": {
        const behind = !held(data.source);
        if (behind) saveable(data.source);
        layOut(data.format, behind);
        const look = JSON.parse(player.italicizing(data.state, data.node, data.from, data.to)) as Record<string, unknown>;
        return post({ type: "italicizing", id: data.id, look });
      }
      case "type":
        return post({ type: "typed", id: data.id, ...(await type(data.source, data.ops, data.index, data.format, data.state, data.node)) });
      case "save":
        saveable(data.source);
        if (data.keep) player.keepHistory();
        return post({ type: "saved", id: data.id, ...(await save()) });
      case "saveAs": {
        saveable(data.source);
        // Kept where it was, if the save does not go through.
        const was = { home, name };
        home = "folder" in data.to ? keptIn(data.to.folder, "folder") : keptIn(await newBundle(data.to.opfs), "opfs");
        name = home.where.name;
        try {
          return post({ type: "saved", id: data.id, ...(await save()) });
        } catch (e) {
          ({ home, name } = was);
          throw e;
        }
      }
      case "rekeep": {
        const open = home?.where.kind === "opfs" && home.where.name === data.from;
        if (data.to === undefined) {
          if (open) throw new Error(`${data.from} is the deck open here: open another to delete it`);
          await deleteKept(data.from);
        } else {
          await renameKept(data.from, data.to);
          if (open) {
            home = keptIn(await keptBundle(data.to), "opfs");
            name = data.to;
          }
        }
        return post({ type: "rekept", id: data.id, where: home?.where });
      }
      case "zip": {
        saveable(data.source);
        await subsetFonts();
        const saved = player.save(new Date().toISOString(), true, await history());
        try {
          const bytes = saved.zip().buffer as ArrayBuffer;
          const { subset } = JSON.parse(saved.summary()) as { subset: [string, number, number][] };
          return post({ type: "zipped", id: data.id, bytes, subset }, [bytes]);
        } finally {
          saved.free();
        }
      }
      case "export": {
        saveable(data.source);
        const bytes = (await exporting(data.as)).slice().buffer as ArrayBuffer;
        return post({ type: "exported", id: data.id, bytes }, [bytes]);
      }
      case "drop": {
        const bytes = new Uint8Array(data.bytes);
        const path = Player.place(data.name, bytes);
        player.dropping(data.name, path, bytes);
        return post({ type: "dropped", id: data.id, path });
      }
      case "picture":
        return post({ type: "pictured", id: data.id, picture: JSON.parse(Player.picture(new Uint8Array(data.bytes))) as Picture });
      case "attaching": {
        current(data.source);
        const bytes = new Uint8Array(data.bytes);
        const path = player.placing(data.name, bytes);
        player.addFile(path, bytes);
        const made = JSON.parse(player.attaching(path, data.schema && JSON.stringify(data.schema))) as { path: string; data: string; attached: Attached | null; patch: unknown[] };
        return post({ type: "attached", id: data.id, ...made });
      }
      case "cells":
        current(data.source);
        return post({ type: "cells", id: data.id, cells: JSON.parse(player.cells(data.text)) as Cells | null });
      case "sheet":
        return post({ type: "sheet", id: data.id, ...sheet(data.source, data.name) });
      case "dataEdit":
        return post({ type: "dataEdited", id: data.id, ...(await dataEdit(data.source, data.name, data.edits, data.index, data.format)) });
      case "dataUndo":
        return post({ type: "dataUndone", id: data.id, ...(await dataUndo(data.source, data.redo, data.index, data.format)) });
      case "bundleFiles":
        current(data.source);
        return post({
          type: "bundleFiles",
          id: data.id,
          files: JSON.parse(player.bundleFiles()) as BundleFile[],
          grounded: JSON.parse(player.grounded()) as Grounded[],
        });
      case "health": {
        current(data.source);
        const deck = deckRead();
        if (!deck) throw new Error("the deck does not read");
        const { health } = await import("@scaena/generate");
        // Read beside what the last lint found, a slide lint finds broken first (lift-up 10-24b STAB-1).
        return post({ type: "health", id: data.id, health: await health(JSON.stringify(deck), player.findings()) });
      }
      case "makePicture": {
        // The theme frames are drawn in, as its JSON: its palette colors the picture.
        const named = player.themeText();
        if (!named) throw new Error("the deck names no theme to color the picture");
        const { picture } = await import("@scaena/generate");
        const made = await picture(data.picture, (JSON.parse(named) as ThemeText).text);
        const bytes = made.bytes.slice().buffer as ArrayBuffer;
        return post({ type: "pictureMade", id: data.id, bytes, ext: made.ext, alt: made.alt, model: made.model }, [bytes]);
      }
      case "mapTarget":
        current(data.source);
        return post({ type: "mapTargeted", id: data.id, target: JSON.parse(player.mapTarget(data.state, data.node, data.slot, JSON.stringify(data.rect))) as MapTarget });
      case "fetchMap": {
        const { fetchMap } = await import("@scaena/generate");
        const fetched = await fetchMap(data.ask, data.key);
        if (!fetched.map) return post({ type: "mapFetched", id: data.id, places: fetched.places });
        return post({ type: "mapFetched", id: data.id, map: fetched.map }, [fetched.map.png]);
      }
      case "mapping":
        current(data.source);
        return post({
          type: "mapped",
          id: data.id,
          mapped: JSON.parse(player.mapping(JSON.stringify(data.target), JSON.stringify(data.fetched), JSON.stringify(data.pin), data.label, new Uint8Array(data.png))) as Mapped,
        });
      case "mapRefreshing":
        current(data.source);
        return post({
          type: "mapRefresh",
          id: data.id,
          refresh: JSON.parse(player.mapRefreshing(data.state, data.node, data.zoom, data.center?.lat, data.center?.lon)) as MapRefresh,
        });
      case "removeFile":
      case "putBack": {
        compiles(data.source);
        if (data.type === "putBack") player.putBack(data.path);
        else player.removeFile(data.path);
        latest++;
        shown = { index: data.index, format: data.format };
        return post({ type: "removed", id: data.id, edited: await edit(player.source(), data.index, data.format) });
      }
      case "versions": {
        const module = await history();
        if (!module) return post({ type: "versions", id: data.id, versions: null });
        const changes = JSON.parse(module.changes(player.file(HISTORY)!)) as Change[];
        const at = (s: number) => (s > 0 ? new Date(s * 1000).toISOString() : null);
        const listed = changes.map((c, i) => ({ n: i + 1, id: c.id, author: c.author, message: c.message, at: at(c.timestamp), ops: c.ops }));
        // Each change in a person's words, as the engine says them on every platform (PLAN 2.99).
        const versions = JSON.parse(Player.versionsSaid(JSON.stringify(listed))) as Version[];
        return post({ type: "versions", id: data.id, versions });
      }
      case "version": {
        const held = await version(data.version);
        const states = JSON.parse(player.viewVersion(held)) as string[];
        const slides = JSON.parse(player.versionSlides()) as string[];
        const state = data.state !== undefined && states.includes(data.state) ? data.state : states[0];
        // A version that cannot be drawn in the bundle as it is still lists its states, and why.
        let png: ArrayBuffer | undefined;
        let why: string | undefined;
        try {
          png = player.versionPng(state, data.width).slice().buffer as ArrayBuffer;
        } catch (e) {
          why = said(e);
        }
        return post({ type: "version", id: data.id, states, said: slides, state, png, why }, png ? [png] : []);
      }
      case "compareVersions": {
        const from = await version(data.from);
        const to = data.to === undefined ? undefined : await version(data.to);
        // The deck now is the one the source compiles to.
        if (to === undefined) compiles(data.source);
        return post({ type: "compared", id: data.id, compared: JSON.parse(player.compareVersions(from, to)) as Compared });
      }
      case "restoreVersion": {
        // The deck before is the source's, or, where it does not compile, the last that did.
        try {
          compiles(data.source);
        } catch {
          // Restoring a version is a way back from a source that does not compile.
        }
        const held = await version(data.version.id);
        const done = JSON.parse(player.restoreVersion(held, JSON.stringify(data.version), "user", new Date().toISOString())) as {
          restored: Restored;
          files: Rewritten[];
        };
        if (!done.restored.applied) return post({ type: "restored", id: data.id, restored: done.restored, files: [] });
        latest++;
        shown = { index: data.index, format: data.format };
        const next = player.source();
        return post({ type: "restored", id: data.id, ...done, source: next, edited: await edit(next, data.index, data.format) });
      }
      case "codeColors":
        return post({ type: "codeColors", id: data.id, colors: JSON.parse(player.codeColors()) as CodeColor[] });
      case "themeText": {
        const text = player.themeText();
        return post({ type: "themeText", id: data.id, theme: text ? (JSON.parse(text) as ThemeText) : null });
      }
      case "themeEdit": {
        // The deck the theme is edited under is the source's, which must compile and validate.
        saveable(data.source);
        player.setMoving([], 0, 0);
        player.preview(undefined);
        const asked = data.photo ? { photo: data.photo } : { ops: data.ops };
        const done = JSON.parse(player.themeEdit(JSON.stringify(asked), false, "user", new Date().toISOString())) as {
          edited: ThemeEdited;
          files: Rewritten[];
        };
        if (!done.edited.applied) return post({ type: "themeEdited", id: data.id, result: done.edited, files: [] });
        latest++;
        shown = { index: data.index, format: data.format };
        // An inline theme is the deck's: its source changes. A theme file's edit leaves it as it is.
        const next = player.source();
        return post({ type: "themeEdited", id: data.id, result: done.edited, files: done.files, source: next, edited: await edit(next, data.index, data.format) });
      }
      case "writeFiles": {
        // A file kept as bytes, a recovery copy's picture (lift-up REL-4), goes in as it is.
        player.writeFiles(JSON.stringify(data.files.filter((f) => !f.bytes).map(({ path, text }) => ({ path, text }))));
        for (const f of data.files) if (f.bytes) player.addFile(f.path, new Uint8Array(f.bytes));
        if (!data.edit) return post({ type: "filesWritten", id: data.id });
        latest++;
        shown = { index: data.edit.index, format: data.edit.format };
        return post({ type: "filesWritten", id: data.id, edited: await edit(data.edit.source, data.edit.index, data.edit.format) });
      }
      case "file": {
        const bytes = player.file(data.path);
        const buffer = bytes ? (bytes.slice().buffer as ArrayBuffer) : null;
        return post({ type: "file", id: data.id, bytes: buffer }, buffer ? [buffer] : []);
      }
      case "ask":
        return await ask(data.id, data.source, data.ask, data.drawn);
      case "stop":
        return asking?.abort();
      case "forget":
        return (await import("@scaena/assistant")).forget();
      case "models": {
        const { providers } = await import("@scaena/assistant");
        return post({ type: "models", id: data.id, models: await providers[data.provider].models(data.key, data.base) });
      }
      case "slideAsk": {
        const { oneAsked } = await import("@scaena/generate");
        const read = player.tool("deck_read", "{}");
        let deck: string;
        try {
          if (read.error) throw new Error((JSON.parse(read.json) as { message?: string }).message ?? read.json);
          deck = JSON.stringify((JSON.parse(read.json) as { deck: unknown }).deck);
        } finally {
          read.free();
        }
        // The files its data sources read, for the rows of a chart the slide draws.
        const sources = Object.values((JSON.parse(deck) as { data?: Record<string, { source?: unknown }> }).data ?? {});
        const files = sources.flatMap(({ source }) => {
          const bytes = typeof source === "string" ? player.file(source) : undefined;
          return bytes ? [[source as string, bytes] as [string, Uint8Array]] : [];
        });
        return post({ type: "slideAsked", id: data.id, asked: await oneAsked(deck, data.place, data.ask, files) });
      }
      case "slideWrite": {
        const { oneAnswered } = await import("@scaena/generate");
        const { slideWrite } = await import("@scaena/assistant");
        const slide = await oneAnswered(data.answer);
        const [json, error] = await slideWrite(player, { slide, ...data.place }, data.author);
        if (error) throw new Error((JSON.parse(json) as { message?: string }).message ?? json);
        const written = JSON.parse(json) as SlideWritten;
        if (!written.applied) throw new Error(`the slide was refused: ${written.left.map((f) => f.message).join("; ")}`);
        const source = player.source();
        return post({ type: "slideWritten", id: data.id, written, source, edited: await edit(source, data.index, data.format) });
      }
      case "write": {
        const { write } = await import("@scaena/assistant");
        const { text, usage } = await write(data.call);
        return post({ type: "wrote", id: data.id, text, usage });
      }
      case "helpers":
        await Promise.all(data.ports.map(join));
        return;
      case "besides": {
        besides = data.besides.map(({ format, canvas }) => ({ format, canvas, context: canvas.getContext("2d")! }));
        besideHeight = Math.max(1, Math.round(data.height));
        // A frame in flight finishes first: the formats show what the canvas shows.
        await painting;
        if (lastPainted) paintBesides(lastPainted.state, lastPainted.t, true);
        return;
      }
      case "help":
        return help(data.port);
    }
  } catch (e) {
    if (e instanceof WebAssembly.RuntimeError) crashed(e);
    post({ type: "error", id, message: said(e) });
  } finally {
    if (!reads) worded = undefined;
    try {
      mirrorFiles();
    } catch (e) {
      if (e instanceof WebAssembly.RuntimeError) crashed(e);
    }
  }
};

/** Language `code`'s hyphenation patterns, which the editor's module leaves out (ADR-0015). The
 * engine asks the first time a text hyphenates in it, in the middle of a layout, so they are
 * fetched at once, as a worker may. The player's module has every language, and never asks. */
function patterns(code: string): Uint8Array | undefined {
  return fetchedNow(hyphenation[code]);
}

/** File `name` of the tables the code scanners read, which the editor's module leaves out (PLAN
 * 2.132). The engine asks the first time a code text in a language that reads it lays out, in the
 * middle of a layout, so it is fetched at once, as the patterns are. The player's module has every
 * table, and never asks. */
function tables(name: string): Uint8Array | undefined {
  return fetchedNow(codeTables[name]);
}

/** A forecast's icons, which the editor's module leaves out too (ADR-0030): fetched as the
 * patterns are, the first time a forecast lays out. */
function icons(): Uint8Array | undefined {
  return fetchedNow(weatherIcons);
}

/** How long a fetch made at once may hold the engine's thread (lift-up REL-3): one that never
 * answers, on a captive or a flaky network, would hold every edit, frame, and lint behind it.
 * A worker's synchronous request may have a timeout, and one that runs out is none. */
const WAIT_MS = 10_000;

/** The bytes at `url`, fetched at once, as a worker may: none where there is no such file, or
 * where it does not come within `WAIT_MS`. */
function fetchedNow(url: string | undefined): Uint8Array | undefined {
  if (!url) return undefined;
  const request = new XMLHttpRequest();
  request.open("GET", url, false);
  request.responseType = "arraybuffer";
  request.timeout = WAIT_MS;
  try {
    request.send();
  } catch {
    return undefined;
  }
  return request.status === 200 ? new Uint8Array(request.response as ArrayBuffer) : undefined;
}

/** The syllable table (PLAN O.4a), which the editor's module leaves out: asked for the first
 * time a line of verse counts, as lint judges a haiku (I404), so fetched at once. */
function syllables(): string | undefined {
  if (!verse) return undefined;
  const request = new XMLHttpRequest();
  request.open("GET", verse, false);
  request.timeout = WAIT_MS;
  try {
    request.send();
  } catch {
    return undefined;
  }
  return request.status === 200 ? request.responseText : undefined;
}

interface DeckFiles {
  meta?: { title?: string };
  /** A theme file's path, or the theme itself, written inline (SPEC §3.1). */
  theme: string | object;
  fonts?: { file: string }[];
  data?: Record<string, { source?: unknown }>;
  states: { id: string }[];
  spine?: { sections?: { title?: string; beats?: { claim?: string; states?: string[] }[] }[] };
}

async function open(source: Source, painter: Painter, target: OffscreenCanvas, engine?: WebAssembly.Module) {
  await init(engine && { module_or_path: engine });
  setHyphenation(patterns);
  setCodeTables(tables);
  setVerse(syllables);
  setIcons(icons);
  canvas = target;
  if (painter !== "cpu") {
    // Ask for an adapter before WebGPU takes the canvas: a canvas WebGPU holds takes no other
    // painter. Both come before the bundle, so a page that starts over on the CPU painter
    // opens it once.
    const adapter = "gpu" in navigator ? await navigator.gpu.requestAdapter().catch(() => null) : null;
    if (adapter) {
      try {
        gpu = await Canvas.attachOffscreen(canvas);
      } catch (e) {
        return post({ type: "error", message: `WebGPU: ${said(e)}`, webgpu: true });
      }
    } else if (painter === "gpu") throw new Error("this browser has no WebGPU adapter");
  }
  if (!gpu) {
    raster = canvas.getContext("2d") ?? undefined;
    if (!raster) throw new Error("the canvas has no 2D context");
  }
  from = source;
  await load(source);
  // A deck made from a theme the page carries has its library already, and one recovered from a
  // worker's files has it as that worker brought it in.
  if (!("create" in source || "recovered" in source)) await library();
  slots = opening();
  // The page hears `ready` first; it asks for the fit it wants, and hears the size then.
  sized(false);
  post({ type: "ready", ...opened() });
}

/** The layout library the theme ships now, brought into the bundle's older copy of it (PLAN 3.31):
 * a deck whose bundle took a theme that ships before its library grew opens with every layout, by
 * section, each drawn as it is used, and every slide as it was. A page with no themes beside it,
 * as a single file has, opens the deck as it is, and so does one that cannot fetch the theme. */
async function library() {
  const { themes, aliases } = await import("@scaena/themes");
  if (!Object.keys(themes).length) return;
  try {
    const text = player.themeText();
    const name = text ? (JSON.parse((JSON.parse(text) as ThemeText).text) as { name?: unknown }).name : undefined;
    // A copy named as the theme shipped before is its copy too: Gallery's is Salon's (PLAN T.0d).
    const shipped = typeof name === "string" ? (themes[name] ?? themes[aliases[name] ?? ""]) : undefined;
    if (!shipped) return;
    const response = await fetch(shipped.url);
    if (response.ok) player.library(await response.text(), new Date().toISOString());
  } catch {
    // The deck opens in the theme it has.
  }
}

/** The open bundle, as the page hears of it. */
function opened(): Opened {
  const deck = JSON.parse(new TextDecoder().decode(player.file("deck.json"))) as DeckFiles;
  return {
    name,
    title: deck.meta?.title || undefined,
    where: home?.where,
    states: slots.map((slot) => slot.state),
    formats: player.formats(),
    own: (JSON.parse(player.sizes()) as { own: string | null }).own ?? undefined,
    // The engine's, the same the Mac's presenter and the remote read (SPEC §2.6), as written
    // and as the blocks a speaker reads.
    notes: slots.map((slot) => JSON.parse(player.notes(slot.state)) as SpeakerNotes),
    outline: outline(deck),
    painter: gpu ? "webgpu" : "cpu",
    adapter: gpu?.adapter ?? "vello_cpu",
  };
}

/** Where a bundle is kept: how a save writes and removes its files there. */
interface Home {
  where: Where;
  write(path: string, bytes: Uint8Array): Promise<void>;
  remove(path: string): Promise<void>;
}

/** A bundle kept in `dir`: a folder on disk, or one in the browser's storage. */
function keptIn(dir: FileSystemDirectoryHandle, kind: "folder" | "opfs"): Home {
  return {
    where: { kind, name: dir.name },
    write: (path, bytes) => write(dir, path, bytes),
    remove: (path) => remove(dir, path),
  };
}

/** The folder `scaena serve` serves the bundle whose deck is at `deck` from (PLAN 2.11): a save
 * writes it back, as the page `serve.client`, so the page does not hear its own save as a change,
 * with the token its address carried (lift-up 10-24b SEC-5). */
function served(deck: string, serve: Serving, folder: string): Home {
  const sent = async (method: "PUT" | "DELETE", path: string, bytes?: Uint8Array) => {
    const url = new URL(path.split("/").map(encodeURIComponent).join("/"), deck);
    const body = bytes as Uint8Array<ArrayBuffer> | undefined;
    const headers = { "X-Scaena-Client": serve.client, "X-Scaena-Token": serve.token };
    const response = await fetch(url, { method, body, headers });
    // A file not there to remove is removed, as in a folder.
    if (method === "DELETE" && response.status === 404) return;
    if (!response.ok) throw new Error(`${path}: ${(await response.text()).trim() || response.status}`);
  };
  return { where: { kind: "serve", name: folder }, write: (path, bytes) => sent("PUT", path, bytes), remove: (path) => sent("DELETE", path) };
}

/** Open the bundle at `source` (SPEC §3.1). A zip is copied into the browser's storage, and
 * kept there from then on. */
async function load(source: Source) {
  if ("url" in source) {
    player = await fetched(source.url, source.serve);
    name = nameOf(source.url);
    if (source.serve) {
      // The folder's own name, as `scaena serve` says it.
      const state = await fetch(new URL("/scaena/state", source.url), { headers: { "X-Scaena-Token": source.serve.token } }).then(
        (r) => r.json() as Promise<{ name?: string }>,
      );
      name = state.name || name;
      home = served(source.url, source.serve, name);
    }
  } else if ("create" in source) {
    player = await made(source.create.theme, source.create.title);
    name = nameFor(source.create.title);
  } else if ("files" in source) {
    player = bundleOf(new Map(Object.entries(source.files).map(([path, bytes]) => [path, new Uint8Array(bytes)])));
    name = source.name;
    only = source.states;
  } else if ("recovered" in source) {
    // The files a worker held when its engine stopped (lift-up STAB-2), kept where it kept them.
    player = bundleOf(new Map(Object.entries(source.recovered).map(([path, bytes]) => [path, new Uint8Array(bytes)])));
    name = source.name;
    const kept = source.kept;
    if (kept && "opfs" in kept) home = keptIn(await keptBundle(kept.opfs), "opfs");
    else if (kept && "folder" in kept) home = keptIn(kept.folder, "folder");
    else if (kept) {
      home = served(kept.url, kept.serve, name);
      // A served bundle is read again from its URL.
      from = { url: kept.url, serve: kept.serve };
    }
  } else if ("zip" in source) {
    player = Player.fromZip(new Uint8Array(source.zip));
    const dir = await newBundle(source.name);
    for (const path of player.files()) await write(dir, path, player.file(path)!);
    home = keptIn(dir, "opfs");
    name = dir.name;
  } else {
    const dir = "opfs" in source ? await keptBundle(source.opfs) : source.folder;
    player = bundleOf(await readAll(dir));
    home = keptIn(dir, "opfs" in source ? "opfs" : "folder");
    name = dir.name;
  }
}

/** The bundle whose deck file is at `deck`: every file is where the deck names it, from the
 * deck's directory (SPEC §3.1). A saved bundle's manifest lists the rest: licenses, the
 * history. `scaena serve` reads each only with the token its address carried (lift-up R1). */
async function fetched(deck: string, serve?: Serving): Promise<Player> {
  const headers: Record<string, string> = serve ? { "X-Scaena-Token": serve.token } : {};
  const get = async (path: string) => {
    const response = await fetch(new URL(path, deck), { headers });
    if (!response.ok) throw new Error(`${path}: HTTP ${response.status}`);
    return response;
  };
  const json = await (await get(deck)).text();
  const files = JSON.parse(json) as DeckFiles;
  const theme = typeof files.theme === "string" ? await (await get(files.theme)).text() : JSON.stringify(files.theme);
  const fetchedPlayer = new Player(json, theme);
  const data = Object.values(files.data ?? {}).flatMap(({ source }) => (typeof source === "string" ? [source] : []));
  const paths = new Set([...(files.fonts ?? []).map((f) => f.file), ...data, ...fetchedPlayer.imageFiles()]);
  if (new URL(deck).pathname.endsWith("/deck.json")) {
    const manifest = await fetch(new URL("manifest.json", deck), { headers }).catch(() => undefined);
    if (manifest?.ok) {
      const bytes = new Uint8Array(await manifest.arrayBuffer());
      fetchedPlayer.addFile("manifest.json", bytes);
      const listed = (JSON.parse(new TextDecoder().decode(bytes)) as { files?: Record<string, string> }).files;
      for (const path of Object.keys(listed ?? {})) paths.add(path);
    }
  }
  if (typeof files.theme === "string") paths.delete(files.theme);
  const bytes = await Promise.all(
    [...paths].map(async (path) => [path, new Uint8Array(await (await get(path)).arrayBuffer())] as const),
  );
  for (const [path, file] of bytes) fetchedPlayer.addFile(path, file);
  return fetchedPlayer;
}

/** The theme that ships as `theme`, which the page carries (`themes.ts`), fetched now: its file's
 * name, its text, and the fonts it names, by the paths its families give them. */
async function shipped(theme: string): Promise<{ file: string; text: string; fonts: Map<string, Uint8Array> }> {
  const { themes, aliases, fonts } = await import("@scaena/themes");
  const chosen = themes[theme] ?? themes[aliases[theme] ?? ""];
  if (!chosen) throw new Error(`no theme ships as ${theme}: ${Object.keys(themes).join(", ") || "none here"}`);
  const fetched = async (url: string) => {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`${url}: ${response.status}`);
    return response;
  };
  const text = await (await fetched(chosen.url)).text();
  type Face = { file?: string };
  const families = (JSON.parse(text) as { type?: { families?: Record<string, Face & { italic?: Face }> } }).type?.families;
  const given = new Map<string, Uint8Array>();
  // Each family's font, and its italic's (PLAN 2.40).
  for (const file of Object.values(families ?? {}).flatMap((f) => [f.file, f.italic?.file])) {
    if (!file || given.has(file)) continue;
    if (!fonts[file]) throw new Error(`${theme} names a font the page does not carry: ${file}`);
    given.set(file, new Uint8Array(await (await fetched(fonts[file])).arrayBuffer()));
  }
  return { file: chosen.file, text, fonts: given };
}

/** The deck `job` asks for, made from a prompt (PLAN G.5) in this worker, which holds no deck: in
 * the theme that ships as `job.theme`, saying how far it has come, then its files, its name, and
 * what it came to. The page opens it as it opens any bundle's files. */
/** What the person decides of the outline shown, once they do (narrative FUNC-1). */
let deciding: ((decision: Decision) => void) | undefined;

async function generating(job: Generate, engine?: WebAssembly.Module) {
  await init(engine && { module_or_path: engine });
  // Lint lays the deck's texts out, which can hyphenate them and color their code, counts a haiku, and lays out its forecasts.
  setHyphenation(patterns);
  setCodeTables(tables);
  setVerse(syllables);
  setIcons(icons);
  const { generate } = await import("@scaena/generate");
  const theme = await shipped(job.theme);
  // The outline shown to the person (narrative FUNC-1), the generation waiting on what they decide
  // of it: `settled` or `outlineAgain`, which the page sends.
  const settle = (outlined: Outlined) => {
    post({ type: "storyOutlined", ...outlined });
    return new Promise<Decision>((resolve) => {
      deciding = resolve;
    });
  };
  const { files, made, title } = await generate(job, theme, (event) => post({ type: "generating", event }), settle);
  const out: Record<string, ArrayBuffer> = {};
  for (const [path, bytes] of files) out[path] = (bytes as Uint8Array).slice().buffer as ArrayBuffer;
  post({ type: "generated", files: out, name: nameFor(title), made }, Object.values(out));
}

/** A new deck titled `title` (PLAN 2.12), as `deck_create` makes one: the theme that ships as
 * `theme` and the fonts it names. Its one state, with nothing on it, is made a title slide in
 * the theme's `title` layout, its words to type over and the theme's backdrop in its picture's
 * place, as the New dialog draws the theme and the Mac's new deck opens (PLAN 3.30, `titling`):
 * made before the page reads the deck, so no undo goes back to the empty state. A theme with no
 * such layout leaves it. */
async function made(theme: string, title: string): Promise<Player> {
  const { file, text, fonts } = await shipped(theme);
  const created = Player.create(file, text, title, fonts);
  const [blank, ...more] = created.states();
  if (blank === undefined || more.length) return created;
  let started: { id: string; patch: unknown[] };
  try {
    started = JSON.parse(created.titling(blank)) as { id: string; patch: unknown[] };
  } catch {
    return created;
  }
  const ops = [...started.patch, { op: "remove_state", id: blank }];
  const titled = created.tool("deck_patch", JSON.stringify({ ops }), "user", new Date().toISOString());
  titled.free();
  return created;
}

/** The deck `source` compiles to, in another theme, by the user (PLAN 2.39), as `theme --apply`
 * re-themes it: one that ships, fetched with its fonts, or one the bundle holds. Refused, it says
 * why and changes nothing; else the deck's source after is compiled, shown at slot `index`, and
 * linted, as an edit of it is. */
async function retheme(
  source: string,
  theme: { ships: string } | { path: string },
  index: number,
  at: string | undefined,
): Promise<{ themed: Themed; source?: string; edited?: Edited }> {
  saveable(source);
  player.setMoving([], 0, 0);
  player.preview(undefined);
  const when = new Date().toISOString();
  let themed: Themed;
  if ("ships" in theme) {
    const { file, text, fonts } = await shipped(theme.ships);
    themed = JSON.parse(player.retheme(`themes/${file}`, text, fonts, when)) as Themed;
  } else themed = JSON.parse(player.retheme(theme.path, undefined, new Map(), when)) as Themed;
  if (themed.refused) return { themed };
  latest++;
  shown = { index, format: at };
  const next = player.source();
  return { themed, source: next, edited: await edit(next, index, at) };
}

/** A bundle's name for a deck titled `title`: lowercase, its words joined by `-`. */
function nameFor(title: string): string {
  const words = title.normalize("NFKD").toLowerCase().replace(/['’]/g, "").match(/[\p{L}\p{N}]+/gu) ?? [];
  return words.join("-").slice(0, 64) || "untitled";
}

/** A bundle's files, by their paths inside it, opened. */
function bundleOf(files: Map<string, Uint8Array>): Player {
  const text = (path: string) => {
    const bytes = files.get(path);
    if (!bytes) throw new Error(`the bundle has no ${path}`);
    return new TextDecoder().decode(bytes);
  };
  const deck = text("deck.json");
  const theme = (JSON.parse(deck) as { theme?: unknown }).theme;
  const session = new Player(deck, typeof theme === "string" ? text(theme) : JSON.stringify(theme));
  for (const [path, bytes] of files) if (path !== "deck.json" && path !== theme) session.addFile(path, bytes);
  return session;
}

/** A bundle's name, from its deck file's URL: its directory's, or the deck file's own, without
 * `.scaena`, `.deck.json`, or `.json`. */
function nameOf(deck: string): string {
  const parts = new URL(deck).pathname.split("/").filter(Boolean);
  const file = parts.at(-1) === "deck.json" ? parts.at(-2) : parts.at(-1);
  return decodeURIComponent(file ?? "").replace(/\.scaena$|(\.deck)?\.json$/, "") || "deck";
}

/** Compile `source`, the editor's as it stands, for a save: one that does not compile, or
 * whose deck does not validate, is not saved, and the error says why and where. */
function saveable(source: string) {
  const compiled = JSON.parse(player.compile(source)) as { error?: Finding; findings: Finding[]; valid: boolean };
  const why = compiled.error ?? (compiled.valid ? undefined : compiled.findings.find((f) => f.severity === "error"));
  if (!why) return noted(source);
  const where = why.at ? ` (line ${why.at.line})` : "";
  const what = compiled.error ? "the source does not compile" : "the deck does not validate";
  throw new Error(`${what}: ${why.message}${where}`);
}

/** Subset each font a save subsets to the characters the deck can draw, with the subsetter's
 * own module, loaded the first time (PLAN 2.4): the engine's module leaves it out. */
async function subsetFonts() {
  const { chars, fonts } = JSON.parse(player.subsetting()) as { chars: string; fonts: string[] };
  const subsetter = await import("@scaena/subset");
  await subsetter.default();
  for (const font of fonts) {
    let subset: Uint8Array;
    try {
      subset = subsetter.subset(player.file(font)!, chars);
    } catch (e) {
      // A damaged font can stop the subsetter: a panic in its module is a trap here.
      throw new Error(`${font} could not be subset (${said(e)}): the font file may be damaged`);
    }
    player.addSubset(font, chars, subset);
  }
}

/** What `export` asks for (PLAN 2.54), each as `scaena export` writes it. The PDF's own
 * module draws a PDF, loaded the first time one is asked for: the engine's module lays the
 * pages out and leaves the PDF writer out (SPEC §15). A single file's fonts are subset first,
 * as a download's are. */
async function exporting(what: Export): Promise<Uint8Array> {
  switch (what.kind) {
    case "png":
      layOut(what.format);
      return player.png(what.state, what.width);
    case "slides":
      layOut(what.format);
      return player.slidesPng(what.width, what.name);
    case "pdf": {
      const laid = player.pdfLaidOut(what.notes ?? false);
      const module = await import("@scaena/pdf");
      await module.default();
      try {
        return module.pdf(laid);
      } catch (e) {
        // A damaged font can stop krilla: a panic in the module is a trap here.
        throw new Error(`the PDF could not be drawn (${said(e)}): a font file may be damaged`);
      }
    }
    case "html":
      await subsetFonts();
      return new TextEncoder().encode(player.standalone(what.page, what.name));
  }
}

/** Where a bundle keeps its history (SPEC §8). */
const HISTORY = "history/deck.loro";

/** A change as the history's module lists it (PLAN 2.60). */
interface Change {
  id: string;
  author?: string | null;
  message?: string | null;
  timestamp: number;
  ops: number;
}

/** Versions read, by their ids, the last read last: a version is the deck as it was after one
 * change, which no later change alters, and its id names that change in any history. */
const versionsRead = new Map<string, string>();

/** Version `id` of the deck, as the history's module reads it: JSON `{ deck, files }`. */
async function version(id: string): Promise<string> {
  const read = versionsRead.get(id);
  if (read !== undefined) {
    versionsRead.delete(id);
    versionsRead.set(id, read);
    return read;
  }
  const module = await history();
  if (!module) throw new Error("the bundle keeps no history: Keep a history, in Versions, begins one with a save");
  const held = module.at(player.file(HISTORY)!, id);
  versionsRead.set(id, held);
  for (const old of versionsRead.keys()) if (versionsRead.size > 8) versionsRead.delete(old);
  return held;
}

/** The module that keeps a bundle's history (PLAN 2.9), loaded the first time a save, or the
 * versions (PLAN 2.60), needs it: the bundle keeps one (`history/deck.loro`). The engine's
 * module leaves the CRDT out. */
async function history(begins = false) {
  if (!player.files().includes(HISTORY) && !(begins && player.keepsHistory())) return undefined;
  const module = await import("@scaena/history");
  await module.default();
  return module;
}

/** Save the bundle with the deck shown, fonts kept whole, where it is kept, or into the
 * browser's storage: the files first, the deck and its manifest last, so the deck never names
 * a file not yet written; then the files the save renamed go. A bundle that keeps a history
 * has the save recorded in it. The session goes on from the save. */
async function save(): Promise<{ where: Where; renamed: [string, string][]; files: number; recorded: boolean }> {
  if (!home) {
    const dir = await newBundle(name);
    home = keptIn(dir, "opfs");
    name = dir.name;
  }
  // A history the person asked to keep begins with this save (PLAN 2.87).
  const recorder = await history(true);
  const saved = player.save(new Date().toISOString(), false, recorder);
  try {
    const paths = saved.paths();
    const last = ["deck.json", "manifest.json"];
    for (const path of [...paths.filter((p) => !last.includes(p)), ...last.filter((p) => paths.includes(p))])
      await home.write(path, saved.file(path)!);
    const written = new Set(paths);
    for (const path of saved.replaced()) if (!written.has(path)) await home.remove(path);
    const { renamed } = JSON.parse(saved.summary()) as { renamed: [string, string][] };
    player.adopt(saved);
    return { where: home.where, renamed, files: paths.length, recorded: recorder !== undefined };
  } finally {
    saved.free();
  }
}

/** The assistant's question being answered, to stop. */
let asking: AbortController | undefined;
/** Where the editor shows the deck: the slot and format it last showed or edited, where the
 * assistant's edits are shown. */
let shown: { index: number; format?: string } = { index: 0 };

/** Ask the assistant `question` about the deck `source` says (PLAN 2.6), which must compile
 * and validate: the assistant's tools work on that deck, and each edit they make comes back to
 * the editor as source. The assistant's code and what it reads load the first time. */
async function ask(id: number, source: string, question: Asking, drawn?: Drawn) {
  saveable(source);
  // Made before the assistant's code loads, so a stop that comes meanwhile stops this question.
  asking?.abort();
  const stop = (asking = new AbortController());
  const emit = (event: AssistantEvent, transfer: Transferable[] = []) => post({ type: "assistant", id, event }, transfer);
  // Each edit the assistant makes is compiled, shown, and linted here, as the editor's edit of
  // its source would be, before its next call: the editor takes the source and what the edit
  // came to, and sends nothing back that the assistant has moved past. With it go the nodes the
  // question has changed so far, which the editor selects (PLAN 2.52), and each state the edit
  // drew otherwise, drawn now, before the next call can change it again (PLAN 2.93).
  const before = deckRead();
  const known = { ...drawn?.known };
  const edited = async (source: string, files: Rewritten[]) => {
    latest++;
    const touched = changed(before, deckRead());
    const event: AssistantEvent = { kind: "edited", source, edited: await edit(source, shown.index, shown.format), touched, files };
    if (drawn) Object.assign(event, redrawn(drawn.height, known));
    emit(event, event.drawn?.flatMap((t) => (t.pixels ? [t.pixels] : [])));
  };
  try {
    const assistant = await import("@scaena/assistant");
    await assistant.ask(player, Player.toolNames(), question, emit, edited, stop.signal);
  } catch (e) {
    emit({ kind: "failed", message: said(e) });
  } finally {
    if (asking === stop) asking = undefined;
  }
}

/** The deck, as the assistant's tools read it: what a question's edits are told apart by. */
interface Read {
  nodes?: Record<string, unknown>;
  states?: { id: string; props?: Record<string, unknown> }[];
  overrides?: Record<string, unknown>;
}

function deckRead(): Read | undefined {
  const read = player.tool("deck_read", "{}");
  try {
    return read.error ? undefined : (JSON.parse(read.json) as { deck?: Read }).deck;
  } catch {
    return undefined;
  } finally {
    read.free();
  }
}

/** The nodes `now` changes from `was` (PLAN 2.52), in its order: those it adds, and those whose
 * own props, the deck's overrides of them, or a state's delta for them differ. A state is told
 * apart by its id; one taken away changes nothing it showed. */
function changed(was: Read | undefined, now: Read | undefined): string[] {
  if (!was || !now) return [];
  const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
  const states = new Map((was.states ?? []).map((s) => [s.id, s.props ?? {}]));
  return Object.keys(now.nodes ?? {}).filter(
    (node) =>
      !same(was.nodes?.[node], now.nodes?.[node]) ||
      !same(was.overrides?.[node], now.overrides?.[node]) ||
      (now.states ?? []).some((s) => !same(states.get(s.id)?.[node], s.props?.[node])),
  );
}

/** The spine as a reader goes through it (SPEC §3.11–3.12): its sections in order, each
 * titled by its title or its first beat's claim, with each of its beats' states and the
 * beat's claim; then the states no beat names. A state two beats name stands at the first. */
function outline(files: DeckFiles): Section[] {
  const placed = new Set<string>();
  const sections: Section[] = [];
  for (const section of files.spine?.sections ?? []) {
    const states = (section.beats ?? []).flatMap((beat) =>
      (beat.states ?? []).filter((s) => !placed.has(s) && placed.add(s)).map((state) => ({ state, claim: beat.claim })),
    );
    const title = section.title ?? section.beats?.find((b) => b.claim)?.claim;
    if (states.length) sections.push({ title, states });
  }
  const rest = files.states.filter((s) => !placed.has(s.id)).map((s) => ({ state: s.id }));
  if (sections.length && rest.length) sections.push({ states: rest });
  return sections;
}

/** The deck's timeline in the format frames are laid out in: the slots of the states it
 * plays, in the order it plays them. */
function timeline(): Slot[] {
  const all = JSON.parse(player.timeline()) as Slot[];
  return only ? only.flatMap((state) => all.filter((slot) => slot.state === state)) : all;
}

/** The timeline of a bundle just opened; where the engine cannot lay the deck out to time it, as
 * when a data file holds a cell its column does not read (E103), each state at an instant. The
 * editor opens on such a bundle, to mend it (PLAN 2.55), and the player says why when it paints. */
function opening(): Slot[] {
  try {
    return timeline();
  } catch {
    const states = player.states();
    return (only ? only.filter((state) => states.includes(state)) : states).map((state) => ({ state, start: 0, span: 0, hold: 0 }));
  }
}

/** The room frames are painted to fill, in device pixels, as the page last said (lift-up
 * ATTR-2); `null` paints them at the canvas's own size, as the editor's preview is. */
let room: [number, number] | null = null;

/** Whether the CPU painter paints a slide larger than its canvas: it does not. A slide larger on
 * the screen is painted at the canvas's own size and scaled up by the browser, so its frames cost
 * what they cost before and keep their pace (gate 2's bar: 60 a second, 30 where a cue draws a
 * shader), where WebGPU paints every pixel of the screen. Measured in headless Chromium
 * (web/fps.mjs, the trails example, fullscreen at 2560 × 1440 at 2×): painted at the canvas's
 * size, a cue's frame takes 5–6 ms, as at 1080p; painted at half again as many pixels, 22 ms,
 * and the cues fall to 30 a second. */
const CPU_LARGER = false;

/** The most a frame is on a side: WebGPU's texture limit, which the adapter is asked for. */
const MAX_SIDE = 8192;

/** The canvas at `size()`, where it is not that yet: its attributes, WebGPU's surface, and, where
 * `tell`, the page told, which shows the canvas at the slide's size on it (lift-up ATTR-2). */
function sized(tell: boolean) {
  const [width, height] = size();
  if (canvas.width === width && canvas.height === height) return;
  [canvas.width, canvas.height] = [width, height];
  gpu?.resize(width, height);
  if (tell) post({ type: "sized", size: [width, height] });
}

/** The canvas frames are painted on now, in whole pixels: the canvas frames are laid out on, or
 * that canvas as large as fits in the room, keeping its aspect, its height worked out from its
 * width as the CPU painter works it out (`raster_size`), so a frame fills the canvas to the last
 * row. The CPU painter paints it no larger than the canvas (`CPU_LARGER`). */
function size(): [number, number] {
  const [w, h] = player.canvasSize();
  if (!room) return [Math.round(w), Math.round(h)];
  // What a painter's raster holds at most (`scaena_paint::MAX_PIXELS`), and how much larger than
  // the canvas the CPU painter paints: not at all.
  const most = 1 << 25;
  const larger = gpu || CPU_LARGER ? Infinity : 1;
  const scale = Math.min(room[0] / w, room[1] / h, MAX_SIDE / w, MAX_SIDE / h, Math.sqrt(most / (w * h)), larger);
  let width = Math.max(1, Math.round(w * scale));
  let height = Math.max(1, Math.round((h * width) / w));
  // Rounded up past the room, or past what a raster holds: a pixel narrower.
  while (width > 1 && (height > Math.round(room[1]) || width * height > most)) {
    width -= 1;
    height = Math.max(1, Math.round((h * width) / w));
  }
  return [width, height];
}

/** Lay frames out in `next` from now on; `again` after the deck has changed. */
/** Thumbnails are asked for again: the last one asked stops the one before it, the strip's of
 * every state and the light table's of some apart (PLAN 2.97). */
const thumbing = { all: 0, some: 0 };

/** Each state of the timeline at rest, `height` pixels high, for the state strip (PLAN 2.35):
 * pixels for each whose drawing (its display list's digest) is not the one `known` holds. A
 * state at a time, the worker answering what else is asked between them; a newer request stops
 * it with what it has. A state the deck no longer has, changed meanwhile, is left out. With
 * `each`, each painted goes to it at once, so the first are seen while the rest are painted,
 * and the answer holds the rest. */
async function thumbnails(height: number, known: Record<string, string>, only?: string[], each?: (thumb: Thumb) => void): Promise<Thumb[]> {
  const kind = only ? "some" : "all";
  const mine = ++thumbing[kind];
  const [w, h] = player.canvasSize();
  const width = Math.max(1, Math.round((height * w) / h));
  const thumbs: Thumb[] = [];
  for (const { state } of slots) {
    if (mine !== thumbing[kind]) break;
    if (only && !only.includes(state)) continue;
    let digest: string | undefined;
    let thumb: Thumb;
    try {
      digest = player.digest(state);
      if (known[state] === digest) {
        thumbs.push({ state, digest });
        continue;
      }
      const pixels = player.pixels(state, Infinity, width);
      thumb = { state, digest, width, height: pixels.length / 4 / width, pixels: pixels.buffer as ArrayBuffer };
    } catch (e) {
      thumb = unpainted(state, digest, e);
    }
    if (each) each(thumb);
    else thumbs.push(thumb);
    await new Promise((go) => setTimeout(go, 0));
  }
  return thumbs;
}

/** A state whose thumbnail could not be painted (lift-up STAB-4): said to the page, which marks
 * its tile and keeps its drawing's digest, so it is painted again only once its drawing changes,
 * and the cause kept in the console. One whose drawing cannot be had is tried again each time. */
function unpainted(state: string, digest: string | undefined, e: unknown): Thumb {
  const why = said(e);
  console.warn(`The thumbnail of ${state} could not be painted: ${why}`);
  return { state, digest: digest ?? "", failed: why };
}

/** Each state whose drawing is not the one `known` holds, at rest, `height` pixels high, as the
 * strip draws it, and each `known` holds that the deck no longer has (PLAN 2.93); `known` takes
 * the drawings as they are now. All at once: an assistant's next call waits for it. */
function redrawn(height: number, known: Record<string, string>): { drawn: Thumb[]; gone: string[] } {
  const [w, h] = player.canvasSize();
  const width = Math.max(1, Math.round((height * w) / h));
  const drawn: Thumb[] = [];
  for (const { state } of slots) {
    let digest: string | undefined;
    try {
      digest = player.digest(state);
      if (known[state] === digest) continue;
      const pixels = player.pixels(state, Infinity, width);
      drawn.push({ state, digest, width, height: pixels.length / 4 / width, pixels: pixels.buffer as ArrayBuffer });
    } catch (e) {
      drawn.push(unpainted(state, digest, e));
    }
    if (digest !== undefined) known[state] = digest;
  }
  const gone = Object.keys(known).filter((state) => !slots.some((s) => s.state === state));
  for (const state of gone) delete known[state];
  return { drawn, gone };
}

/** The deck the editor's `source` compiles to: compiled first where the deck shown is not yet that
 * one, after an undo or while the source is typed in. */
function current(source: string) {
  if (held(source)) return;
  saveable(source);
  layOut(format, true);
}

/** The sources compiled as the deck, the latest first and the eight before it, as the page
 * keeps eight for an undo. A request that carries one of the eight is older than the deck held:
 * posted before the edit the deck came from was answered, and taken while that edit's paint
 * yielded. It is answered from the deck held, never by compiling its source over the newer
 * one, which set the deck a track behind the source (#469's canvas red: two arrow presses in a
 * row, and the Theme panel's `bundleFiles` between them). */
const compiledSources: string[] = [];
/** The page's version of the source (`version`, which counts every change of it, an undo
 * included) that the request being answered carries, and the one the deck held was compiled at:
 * a request no newer than the deck held is answered from it. An undo's source is one of the
 * eight, but newer: it is compiled, never answered from the deck before the undo. */
let asked: number | undefined;
let heldAt: number | undefined;
function noted(source: string) {
  const was = compiledSources.indexOf(source);
  if (was >= 0) compiledSources.splice(was, 1);
  compiledSources.unshift(source);
  compiledSources.length = Math.min(compiledSources.length, 9);
  if (asked !== undefined) heldAt = asked;
}
/** Whether the deck held answers a request that carries `source`: compiled from it, or from an
 * edit made after it, which is one of the eight, and, where the request carries a version, no
 * newer than the deck's. */
function held(source: string): boolean {
  if (player.compiledFrom(source)) return true;
  if (!compiledSources.includes(source)) return false;
  return asked === undefined || heldAt === undefined || asked <= heldAt;
}
/** Whether a request carrying `source` and a version is older than the deck held: a source
 * compiled before, at a version no newer than the deck's, and not the deck's own. A source never
 * compiled (a fix's, which the page edits before it takes it) is newer whatever its version. */
function stale(source: string): boolean {
  return asked !== undefined && heldAt !== undefined && asked <= heldAt && !player.compiledFrom(source) && compiledSources.includes(source);
}

function layOut(next: string | undefined, again = false) {
  if (next === format && !again) return;
  player.setFormat(next);
  format = next;
  slots = timeline();
}

/** Compile `source`. Once it validates, repaint slot `index` at rest from the new deck, then
 * lint it: the editor's round trip (PLAN 2.3), each step timed. Nothing else compiles meanwhile
 * (`editing`), so the lint is of the deck compiled from `source`. */
async function edit(source: string, index: number, at: string | undefined, read?: () => void): Promise<Edited> {
  // One edit at a time, each through its lint, whoever asked for it.
  while (editing) await editing;
  let edited!: () => void;
  editing = new Promise<void>((settle) => (edited = settle));
  try {
    return await editOf(source, index, at, read);
  } finally {
    editing = undefined;
    edited();
  }
}

/** The edit in flight, while its frame paints: what a request that carries a source waits for. */
let editing: Promise<void> | undefined;

/** [`edit`], on its turn. */
async function editOf(source: string, index: number, at: string | undefined, read?: () => void): Promise<Edited> {
  const start = performance.now();
  const compiled = JSON.parse(player.compile(source)) as {
    error?: Finding;
    findings: Finding[];
    states: [string, number][];
    valid: boolean;
  };
  noted(source);
  const compiledAt = performance.now();
  let where: Edited["at"];
  let shown: string | undefined;
  if (compiled.valid) {
    // A format the deck no longer lists falls back to its own canvas.
    layOut(at !== undefined && player.formats().includes(at) ? at : undefined, true);
  }
  // What the caller reads of the deck as compiled, before the frame's wait lets another message
  // compile another source.
  read?.();
  if (compiled.valid) {
    const i = Math.min(index, slots.length - 1);
    const slot = slots[i];
    if (slot) {
      await paint(slot.state, slot.span);
      where = { index: i, t: slot.span, global: slot.start + slot.span, playing: false };
      shown = slot.state;
    }
  }
  const paintedAt = performance.now();
  // The state shown, laid out alone: the rest waits for typing to stop.
  const linted = compiled.error
    ? undefined
    : (JSON.parse(player.lint(shown, Math.floor(Date.now() / 1000))) as { findings: Finding[]; laid: boolean; whole: boolean });
  const lintedAt = performance.now();
  return {
    error: compiled.error,
    findings: linted?.findings ?? compiled.findings,
    states: compiled.states,
    valid: compiled.valid,
    laid: linted?.laid ?? false,
    whole: linted?.whole ?? true,
    slots,
    formats: player.formats(),
    own: (JSON.parse(player.sizes()) as { own: string | null }).own ?? undefined,
    at: where,
    ms: { compile: compiledAt - start, paint: paintedAt - compiledAt, lint: lintedAt - paintedAt },
  };
}

/** The states the patch a drag last snapped to changes, by the patch: most moves of a drag land
 * where the one before did. */
let reached: { patch: string; states: string[] } | undefined;

/** A drag's move on the editor's canvas (ADR-0013): where its box would land and the states that
 * patch changes, and the state painted with the node moved, laying nothing out, or, when a resize
 * pauses, as the patch would make it. */
async function drag(d: Extract<ToWorker, { type: "drag" }>): Promise<{ snapped?: Snapped | null; states?: string[] }> {
  let snapped: Snapped | null | undefined;
  let states: string[] | undefined;
  const nodes = [d.node, ...(d.with ?? [])];
  if (d.together) {
    // Several, moved together (PLAN 2.42): the first's box lands as a drag of it alone does, and
    // the box around them meets others (PLAN 2.57).
    const { by, free, fork, reach } = d.together;
    const moved = JSON.parse(player.together(d.state, nodes, by[0], by[1], free, fork, reach ?? 0)) as (Arranged & { guides?: Line[] }) | null;
    snapped = moved && { cell: moved.landed[0]?.cell ?? [0, 0, 0, 0], patch: moved.patch, landed: moved.landed, guides: moved.guides };
  } else if (d.snap) {
    const [x, y, w, h] = d.snap.to;
    snapped = JSON.parse(player.snap(d.state, d.node, d.snap.how, x, y, w, h, d.snap.fork, d.snap.reach ?? 0)) as Snapped | null;
  }
  if (d.together || d.snap) {
    states = [];
    if (snapped?.patch.length) {
      const patch = JSON.stringify(snapped.patch);
      if (reached?.patch !== patch) reached = { patch, states: JSON.parse(player.reach(patch)) as string[] };
      states = reached.states;
    }
  }
  if (d.by || d.preview) {
    latest++;
    const [dx, dy] = d.by ?? [0, 0];
    player.setMoving(d.by ? nodes : [], dx, dy);
    player.preview(d.preview && snapped?.patch.length ? JSON.stringify(snapped.patch) : undefined);
    // The formats beside show the deck as it is until the drag ends in a patch.
    await paint(d.state, Infinity, "keep");
  }
  return { snapped, states };
}

/** `nodes` arranged `how` in `state` at rest (PLAN 2.42): where each lands and the patch. */
function arranging(state: string, nodes: string[], how: Arrange, fork: boolean): Arranged | null {
  return JSON.parse(player.arranging(state, nodes, JSON.stringify(how), fork)) as Arranged | null;
}

/** Make `ops`, the patch a gesture on the editor's canvas or a choice in its inspector ended in,
 * by the user, on the deck `source` compiles to, which must validate (ADR-0013). The deck's
 * source after is compiled, shown at slot `index`, and linted, as an edit of it is. A patch the
 * deck refuses, or one that changes nothing, is an error that says why. */
async function make(source: string, ops: unknown[], index: number, at: string | undefined): Promise<{ source: string; edited: Edited }> {
  saveable(source);
  player.setMoving([], 0, 0);
  player.preview(undefined);
  const made = player.tool("deck_patch", JSON.stringify({ ops }), "user", new Date().toISOString());
  const [json, failed, changed] = [made.json, made.error, made.edited];
  made.free();
  if (failed) throw new Error((JSON.parse(json) as { message?: string }).message ?? json);
  if (!changed) {
    const why = (JSON.parse(json) as { added?: Finding[] }).added?.find((f) => f.severity === "error");
    if (!why) throw new Error("it is there already");
    // Said as the findings are, in a person's words, of the object it is about (PLAN 2.99).
    const state = slots[index]?.state;
    let words: Lexicon | undefined;
    try {
      words = state === undefined ? undefined : (JSON.parse(player.words(state)) as Lexicon);
    } catch {
      words = undefined;
    }
    throw new Error(`the deck would not open with it. ${finding(words, why)}`);
  }
  latest++;
  shown = { index, format: at };
  const next = player.source();
  return { source: next, edited: await edit(next, index, at) };
}

/** Compile `source` into the deck a data edit works on (PLAN 2.55), which need not validate: a
 * cell its column does not read (E103) leaves the deck invalid, and the edit is what fixes it. */
function compiles(source: string) {
  if (held(source)) return;
  const compiled = JSON.parse(player.compile(source)) as { error?: Finding };
  if (!compiled.error) return noted(source);
  const where = compiled.error.at ? ` (line ${compiled.error.at.line})` : "";
  throw new Error(`the source does not compile: ${compiled.error.message}${where}`);
}

/** The deck `source` compiles to's data sources, and source `name` as a sheet (PLAN 2.55): the
 * first without it; or why it does not read as one. */
function sheet(source: string, name: string | undefined): { sources: DataSource[]; name?: string; sheet?: Sheet; file?: string; why?: string } {
  compiles(source);
  const sources = JSON.parse(player.dataSources()) as DataSource[];
  const shown = sources.find((s) => s.name === name)?.name ?? sources[0]?.name;
  if (shown === undefined) return { sources };
  try {
    const { sheet, file } = JSON.parse(player.dataSheet(shown)) as { sheet: Sheet; file?: string | null };
    return { sources, name: shown, sheet, file: file ?? undefined };
  } catch (e) {
    return { sources, name: shown, why: said(e) };
  }
}

/** `edits` of data source `name`, by the user (PLAN 2.55), on the deck `source` compiles to: one
 * write of its file, or one patch of rows written inline. Where they wrote, the deck's source
 * after is compiled, shown at slot `index`, and linted, as an edit of it is. A value its column
 * does not read is an error that says why; edits the deck refuses say so in what they did. */
async function dataEdit(
  source: string,
  name: string,
  edits: RowEdit[],
  index: number,
  at: string | undefined,
): Promise<{ result: DataEdited; source?: string; edited?: Edited }> {
  compiles(source);
  player.setMoving([], 0, 0);
  player.preview(undefined);
  const { result, wrote } = JSON.parse(player.dataEdit(JSON.stringify({ source: name, edits }), "user", new Date().toISOString())) as {
    result: DataEdited;
    wrote: boolean;
  };
  if (!wrote) return { result };
  latest++;
  shown = { index, format: at };
  const next = player.source();
  return { result, source: next, edited: await edit(next, index, at) };
}

/** The last edit of a data file undone, or with `redo` made again (PLAN 2.55): its file written as
 * it was, by the user, then the deck shown at slot `index` and linted, as an edit is. */
async function dataUndo(source: string, redo: boolean, index: number, at: string | undefined): Promise<{ name?: string; edited?: Edited }> {
  compiles(source);
  const name = redo ? player.dataRedo("user", new Date().toISOString()) : player.dataUndo("user", new Date().toISOString());
  if (name === undefined) return {};
  latest++;
  shown = { index, format: at };
  return { name, edited: await edit(player.source(), index, at) };
}

/** Make `ops`, text typed on the editor's canvas (PLAN 2.32), by the user, on the deck `source`
 * compiles to, which must validate: validated as a patch is, but not linted. The deck's source
 * after is compiled, shown at slot `index`, and linted, as an edit of it is; with it, where a
 * caret stands in `node`'s text in `state` now. */
async function type(
  source: string,
  ops: unknown[],
  index: number,
  at: string | undefined,
  state: string,
  node: string,
): Promise<{ source: string; edited: Edited; carets: Carets | null }> {
  saveable(source);
  player.setMoving([], 0, 0);
  player.preview(undefined);
  if (!player.typed(JSON.stringify(ops), new Date().toISOString())) throw new Error("it reads so already");
  latest++;
  shown = { index, format: at };
  const next = player.source();
  // Where a caret stands in the text as typed, read once it is compiled and before the frame is
  // painted: what the page asks meanwhile may compile another source, and the next change is made
  // on these.
  let carets: Carets | null = null;
  const edited = await edit(next, index, at, () => (carets = JSON.parse(player.carets(state, node)) as Carets | null));
  return { source: next, edited, carets };
}

/** What the preview last painted: what a new view paints again (PLAN 2.46). */
let lastPainted: { state: string; t: number } | undefined;
/** The part of the canvas the editor's preview shows zoomed in, `[x, y, w, h]` canvas units, or
 * `null` for all of it; `undefined` until the editor first asks, as the player's page never does
 * (PLAN 2.46). Each paint sets it on the engine, whichever the bundle is now: a reload makes another. */
let viewing: Float32Array | null | undefined;

/** The CPU painter's frame in flight: the next waits for it, since the module holds one frame
 * for its shaders at a time, and a reload waits for it before it lets the engine go. */
let painting: Promise<void> = Promise.resolve();

/** How a frame of the canvas takes the formats beside it along (PLAN 2.62): `paint`ed with it,
 * at the `pace` `BESIDE_MS` allows while a cue plays, or `keep`ing what they show, where only the
 * canvas changes (a drag, a zoom). */
type Beside = "paint" | "pace" | "keep";

/** `state` `t` ms into its cue, on the canvas, sized to the format first, then in each format
 * beside it as `beside` says. Past its span, it is at rest; its shaders keep the timeline's time
 * (SPEC §3.8). */
/** Request `id` answered, a frame painted since `start`, once the GPU has done the frame: a
 * page that screenshots the canvas when it hears gets the frame asked for, not the last one the
 * compositor had whole (a large picture's first upload, on a slow adapter, outlasting the page's
 * next frames). `ms` is the painting alone. */
async function answerShown(id: number, start: number) {
  const ms = performance.now() - start;
  await gpu?.settled();
  post({ type: "shown", id, size: [canvas.width, canvas.height], ms });
}

async function paint(state: string, t: number, beside: Beside = "paint") {
  lastPainted = { state, t };
  sized(true);
  const [width, height] = [canvas.width, canvas.height];
  if (viewing !== undefined) player.setView(viewing);
  if (gpu) {
    player.paint(gpu, state, t);
    if (beside !== "keep") paintBesides(state, t, beside === "paint");
    return;
  }
  const before = painting;
  let done = () => {};
  painting = new Promise((resolve) => (done = resolve));
  try {
    await before;
    await painted(player, state, t, width);
    if (beside !== "keep") paintBesides(state, t, beside === "paint");
  } finally {
    done();
  }
}

/** The formats painted beside the canvas (PLAN 2.62): each its own canvas, the deck's own
 * canvas where `format` is unset, all `besideHeight` pixels high. */
let besides: { format?: string; canvas: OffscreenCanvas; context: OffscreenCanvasRenderingContext2D }[] = [];
let besideHeight = 1;
/** When they were last painted, by the worker's clock. */
let besidesAt = -Infinity;
/** How often they are painted at most while a cue plays: the canvas's own frames come first. */
const BESIDE_MS = 1000 / 30;

/** `state` `t` ms into its cue in each format beside the canvas, as the canvas would show it
 * there, by the CPU painter (`Player.pixelsIn`): each format lays a state out once. `force`: this
 * frame, however soon after the last; else none sooner than `BESIDE_MS` after it. A format the
 * deck cannot be drawn in keeps what it showed. */
function paintBesides(state: string, t: number, force: boolean) {
  if (!besides.length) return;
  const now = performance.now();
  if (!force && now - besidesAt < BESIDE_MS) return;
  besidesAt = now;
  for (const { format, canvas, context } of besides) {
    try {
      const pixels = player.pixelsIn(format, state, t, besideHeight);
      const width = pixels.length / 4 / besideHeight;
      if (canvas.width !== width || canvas.height !== besideHeight) [canvas.width, canvas.height] = [width, besideHeight];
      context.putImageData(new ImageData(pixels as Uint8ClampedArray<ArrayBuffer>, width, besideHeight), 0, 0);
    } catch (e) {
      console.warn(`the ${format ?? "deck's own"} format could not be drawn: ${said(e)}`);
    }
  }
  post({ type: "besides", state, t });
}

/** The workers a shader's rows are spread over, this one among them (PLAN 2.28): the cores the
 * browser says it has, up to the 8 SPEC §15's budgets allow, as `scaena_core::shader::cores`
 * counts them natively. */
const cores = Math.min(8, Math.max(1, self.navigator?.hardwareConcurrency ?? 1));
/** The helpers that work out shaders' rows beside this worker, once they are ready. */
let helpers: Helper[] = [];
/** Whether the page was asked for them: once, the first time a frame drew a shader worth
 * splitting. */
let askedForHelpers = false;
/** How long a band may take on a helper before this worker works it out itself, and lets the
 * helper go: a helper that stopped would otherwise stop the deck. */
const BAND_MS = 10_000;

/** `state` `t` ms into its cue, painted by `engine`'s CPU painter `width` pixels wide and put on
 * the canvas, its shaders' rows in bands (PLAN 2.28): the first worked out here, the others on
 * the helpers, each from the shader's spec, which make the same job and so the same bytes. The
 * engine's module has no threads, and a full-canvas shader is most of a frame. The frame goes
 * onto the canvas from the module's memory, copied once, by the canvas (ADR-0004 finding 15). */
async function painted(engine: Player, state: string, t: number, width: number) {
  const count = engine.shading(state, t, width);
  const sharing = helpers.slice();
  // Each band settles, worked out on its helper or here, before the frame is painted or fails:
  // a band that came in late would land in the next frame held.
  const theirs: Promise<void>[] = [];
  let failed: unknown;
  for (let i = 0; i < count; i++) {
    if (!askedForHelpers && cores > 1 && engine.shaderBands(i, cores).length > 2) {
      askedForHelpers = true;
      post({ type: "helpers", count: cores - 1 });
    }
    // Each band's first row, then its rows.
    const bands = engine.shaderBands(i, sharing.length + 1);
    const spec = bands.length > 2 ? engine.shaderSpec(i) : undefined;
    for (let n = 2; n < bands.length; n += 2) {
      const [first, rows, helper] = [bands[n], bands[n + 1], sharing[n / 2 - 1]];
      const own = (e: unknown) => {
        console.warn(`a shader helper failed, and its rows are worked out here: ${said(e)}`);
        helpers = helpers.filter((h) => h !== helper);
        helper.close();
        engine.shade(i, first, rows);
      };
      const taken = (bytes: ArrayBuffer) => engine.takeRows(i, first, new Uint8Array(bytes));
      theirs.push(helper.rows(spec!, first, rows, BAND_MS).then(taken, own).catch((e) => void (failed ??= e)));
    }
    engine.shade(i, bands[0], bands[1]);
  }
  await Promise.all(theirs);
  if (failed !== undefined) throw failed;
  engine.putShaded(raster!);
}

/** A worker that works out shaders' rows for this one, over the port to it (PLAN 2.28). */
class Helper {
  private waiting = new Map<number, { resolve: (bytes: ArrayBuffer) => void; reject: (error: Error) => void }>();
  private asked = 0;

  constructor(private port: MessagePort) {
    port.onmessage = ({ data }: MessageEvent<FromHelper>) => {
      if (data.type === "ready" || data.id === undefined) return;
      const waiting = this.waiting.get(data.id);
      this.waiting.delete(data.id);
      if (data.type === "rows") waiting?.resolve(data.bytes);
      else waiting?.reject(new Error(data.message));
    };
  }

  /** Rows `first` to `first + rows` of the shader whose spec is `spec`, or an error once `ms`
   * have gone by without them. */
  rows(spec: Uint8Array, first: number, rows: number, ms: number): Promise<ArrayBuffer> {
    const id = ++this.asked;
    return new Promise((resolve, reject) => {
      const late = setTimeout(() => {
        this.waiting.delete(id);
        reject(new Error(`no rows after ${ms} ms`));
      }, ms);
      const settled = <T>(then: (value: T) => void) => (value: T) => (clearTimeout(late), then(value));
      this.waiting.set(id, { resolve: settled(resolve), reject: settled(reject) });
      this.port.postMessage({ type: "rows", id, spec, first, rows } satisfies ToHelper);
    });
  }

  close() {
    this.port.close();
    for (const { reject } of this.waiting.values()) reject(new Error("the helper was let go"));
    this.waiting.clear();
  }
}

/** Take on the helper at the other end of `port` once it holds the engine's module: this
 * worker's own, compiled, or, where a module cannot cross to it, one it loads itself. One that
 * cannot start is left out, and this worker works its rows out itself. */
async function join(port: MessagePort) {
  const ready = new Promise<void>((resolve, reject) => {
    port.onmessage = ({ data }: MessageEvent<FromHelper>) =>
      data.type === "ready" ? resolve() : data.type === "error" ? reject(new Error(data.message)) : undefined;
  });
  try {
    port.postMessage({ type: "module", module: engineModule() as WebAssembly.Module } satisfies ToHelper);
  } catch {
    port.postMessage({ type: "module" } satisfies ToHelper);
  }
  try {
    await ready;
    helpers.push(new Helper(port));
  } catch (e) {
    console.warn(`a shader helper could not start: ${said(e)}`);
    port.close();
  }
}

/** Be a helper (PLAN 2.28): work out shaders' rows for the engine's worker at the other end of
 * `port`, which first hands over the engine's module. A helper holds no deck. */
function help(port: MessagePort) {
  const answer = (message: FromHelper, transfer: Transferable[] = []) => port.postMessage(message, transfer);
  port.onmessage = async ({ data }: MessageEvent<ToHelper>) => {
    try {
      if (data.type === "module") {
        await init(data.module && { module_or_path: data.module });
        return answer({ type: "ready" });
      }
      const bytes = shaderRows(data.spec, data.first, data.rows);
      answer({ type: "rows", id: data.id, bytes: bytes.buffer as ArrayBuffer }, [bytes.buffer as ArrayBuffer]);
    } catch (e) {
      answer({ type: "error", id: data.type === "rows" ? data.id : undefined, message: said(e) });
    }
  };
}

/** The deck from slot `index`, `t` ms in, a frame each time the display takes one. A state
 * that holds, short of the last, gives way to the next when its cue and hold are over; one
 * that does not hold, and the last, comes to rest and waits. `still`: each cue is a cut, the
 * state painted at rest once, and the clock waits out its cue and hold without frames.
 * `alone`: slot `index`'s cue alone, which comes to rest and waits. */
function run(index: number, t: number, run: number, still = false, alone = false) {
  const goesOn = (i: number) => !alone && slots[i].hold > 0 && i < slots.length - 1;
  const start = performance.now();
  /** Where the clock started, from the slot it is in now. */
  let offset = t;
  /** The slot painted at rest, when still. */
  let rested = -1;
  /** When the run's last frame was painted, by the display's clock. */
  let last: number | undefined;
  /** The slot whose cue came to rest and laid out the next one's (lift-up PERF-1). */
  let prepared = -1;
  const frame = async (now: number) => {
    if (run !== latest) return;
    let t = offset + Math.max(0, now - start);
    while (goesOn(index) && t >= slots[index].span + slots[index].hold) {
      offset -= slots[index].span + slots[index].hold;
      t -= slots[index].span + slots[index].hold;
      index++;
    }
    const slot = slots[index];
    const playing = goesOn(index) || (!still && t < slot.span);
    const shown = still || !playing ? slot.span : t;
    if (rested !== index) {
      const began = performance.now();
      try {
        // The formats beside the canvas follow it as often as they can, and come to rest with it.
        await paint(slot.state, shown, playing ? "pace" : "paint");
      } catch (e) {
        return post({ type: "error", message: said(e) });
      }
      // A newer request came while the CPU painter painted: its frame is the one to report.
      if (run !== latest) return;
      const timing = still ? undefined : { paint: performance.now() - began, interval: last === undefined ? undefined : now - last };
      last = now;
      post({ type: "at", index, t: shown, global: slot.start + shown, playing, frame: timing });
      if (still) rested = index;
    }
    // Its cue over, held or waiting, the next one's is laid out meanwhile; a cut never samples one.
    if (!still && t >= slot.span && prepared !== index) {
      prepared = index;
      ahead(index + 1, run);
    }
    if (!playing) return;
    if (still) setTimeout(() => frame(performance.now()), slot.span + slot.hold - t);
    else next(frame);
  };
  next(frame);
}

/** Lay out the cue into slot `index` while the one before it rests or holds, so going on draws its
 * first frame at once (lift-up PERF-1): once the worker has answered what waits, and only while run
 * `run` is the one shown. */
function ahead(index: number, run: number) {
  const slot = slots[index];
  if (!slot || slot.span <= 0) return;
  setTimeout(() => {
    if (run !== latest) return;
    try {
      player.prepare(slot.state);
    } catch (e) {
      console.warn(`the next cue could not be laid out ahead: ${said(e)}`);
    }
  }, 0);
}

const next = (frame: (now: number) => void) => {
  if ("requestAnimationFrame" in self) self.requestAnimationFrame(frame);
  else setTimeout(() => frame(performance.now()), 1000 / 60);
};
