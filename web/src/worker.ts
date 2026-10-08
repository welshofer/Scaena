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
import { hyphenation } from "@scaena/hyphenation";
import init, { Canvas, Player, engineModule, setHyphenation, shaderRows } from "@scaena/wasm";
import { keptBundle, newBundle, readAll, remove, write } from "./folders";
import type {
  Added,
  Asking,
  AssistantEvent,
  Attached,
  Carets,
  Choices,
  DataEdited,
  DataSource,
  Drawn,
  Edited,
  Export,
  Finding,
  FromHelper,
  FromWorker,
  Insert,
  Layer,
  LayoutSuggestion,
  Thumb,
  Opened,
  Painter,
  Pasted,
  Look,
  Put,
  BundleFile,
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
  StateChoices,
  Themed,
  ThemeEdited,
  ThemeText,
  Themes,
  ToHelper,
  ToWorker,
  Where,
} from "./protocol";

const post = (message: FromWorker, transfer: Transferable[] = []) => self.postMessage(message, transfer);
const said = (e: unknown) => (e instanceof Error ? e.message : String(e));

let player: Player;
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
/** The states a single-file export plays, in its order; every state without it. */
let only: string[] | undefined;

self.onmessage = async ({ data }: MessageEvent<ToWorker>) => {
  const id = "id" in data ? data.id : undefined;
  try {
    switch (data.type) {
      case "open":
        return await open(data.source, data.painter, data.canvas, data.engine);
      case "reload": {
        if (!("url" in from)) throw new Error("only a bundle read from a URL is read again");
        latest++;
        const was = player;
        player = await fetched(from.url);
        // A frame in flight finishes on the engine it began with.
        await painting;
        was.free();
        format = undefined;
        slots = opening();
        [canvas.width, canvas.height] = size();
        gpu?.resize(canvas.width, canvas.height);
        return post({ type: "reloaded", id: data.id, ...opened() });
      }
      case "show": {
        latest++;
        const start = performance.now();
        layOut(data.format);
        await paint(data.state, data.t ?? Infinity);
        return post({ type: "shown", id: data.id, size: [canvas.width, canvas.height], ms: performance.now() - start });
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
      case "edit":
        latest++;
        shown = { index: data.index, format: data.format };
        return post({ type: "edited", id: data.id, ...(await edit(data.source, data.index, data.format)) });
      case "lint": {
        const start = performance.now();
        const linted = JSON.parse(player.lint(undefined)) as { findings: Finding[]; laid: boolean; whole: boolean };
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
        return post({ type: "shown", id: data.id, size: [canvas.width, canvas.height], ms: performance.now() - start });
      }
      case "make":
        return post({ type: "made", id: data.id, ...(await make(data.source, data.ops, data.index, data.format)) });
      case "choices":
        return post({ type: "choices", id: data.id, choices: JSON.parse(player.choices(data.state, data.node)) as Choices });
      case "stateChoices":
        return post({ type: "stateChoices", id: data.id, choices: JSON.parse(player.stateChoices(data.state)) as StateChoices });
      case "layoutSuggestions": {
        layOut(data.format);
        // A layout at a time, each a lint of the state in every format and a frame: what else the
        // page asks is answered between them. An edit meanwhile ends the round, an error.
        player.layoutsBegin(data.state);
        do {
          await new Promise((next) => setTimeout(next));
        } while (player.layoutsStep());
        const listed = JSON.parse(player.layoutSuggestions(data.height)) as Omit<LayoutSuggestion, "pixels">[];
        const suggestions = listed.map((s, i) => ({ ...s, pixels: player.layoutPixels(i).buffer as ArrayBuffer }));
        return post({ type: "layoutSuggestions", id: data.id, suggestions }, suggestions.map((s) => s.pixels));
      }
      case "preview": {
        latest++;
        layOut(data.format);
        player.setMoving([], 0, 0);
        player.preview(JSON.stringify(data.ops));
        const start = performance.now();
        // The formats beside show the deck as it is: nothing is made.
        await paint(data.state, Infinity, "keep");
        return post({ type: "shown", id: data.id, size: [canvas.width, canvas.height], ms: performance.now() - start });
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
        return post({ type: "adding", id: data.id, added: JSON.parse(player.inserting(data.state, data.n, ...data.at, data.named)) as Added });
      case "drawing":
        current(data.source);
        layOut(data.format);
        return post({ type: "adding", id: data.id, added: JSON.parse(player.drawing(data.state, data.n, ...data.from, ...data.to, data.free)) as Added });
      case "duplicating":
        current(data.source);
        layOut(data.format);
        return post({ type: "adding", id: data.id, added: JSON.parse(player.duplicating(data.state, data.node)) as Added });
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
        return post({ type: "pasted", id: data.id, pasted: JSON.parse(player.pasting(data.clip, data.state, ...data.at)) as Pasted });
      case "thumbnails": {
        layOut(data.format);
        const thumbs = await thumbnails(data.height, data.known);
        return post({ type: "thumbnails", id: data.id, thumbs }, thumbs.flatMap((t) => (t.pixels ? [t.pixels] : [])));
      }
      case "addingState":
        current(data.source);
        return post({ type: "addingState", id: data.id, added: JSON.parse(player.addingState(data.state, data.what)) as { id: string; patch: unknown[] } });
      case "deleting":
        current(data.source);
        return post({ type: "deleting", id: data.id, patch: JSON.parse(player.deleting(data.state, data.node, data.everywhere)) as unknown[] });
      case "carets": {
        // The source as it stands, where the deck shown is not yet the one it compiles to: an
        // undo, or the source changed under the text.
        const behind = !player.compiledFrom(data.source);
        if (behind) saveable(data.source);
        layOut(data.format, behind);
        return post({ type: "carets", id: data.id, carets: JSON.parse(player.carets(data.state, data.node)) as Carets | null });
      }
      case "themes":
        return post({ type: "themes", id: data.id, themes: JSON.parse(player.themes()) as Themes });
      case "retheme":
        return post({ type: "rethemed", id: data.id, ...(await retheme(data.source, data.theme, data.index, data.format)) });
      case "characterChoices": {
        const choices = JSON.parse(player.characterChoices(data.state, data.node, data.from, data.to)) as Choices;
        return post({ type: "characterChoices", id: data.id, choices });
      }
      case "bolding": {
        const behind = !player.compiledFrom(data.source);
        if (behind) saveable(data.source);
        layOut(data.format, behind);
        const look = JSON.parse(player.bolding(data.state, data.node, data.from, data.to)) as Record<string, unknown>;
        return post({ type: "bolding", id: data.id, look });
      }
      case "italicizing": {
        const behind = !player.compiledFrom(data.source);
        if (behind) saveable(data.source);
        layOut(data.format, behind);
        const look = JSON.parse(player.italicizing(data.state, data.node, data.from, data.to)) as Record<string, unknown>;
        return post({ type: "italicizing", id: data.id, look });
      }
      case "type": {
        const typed = await type(data.source, data.ops, data.index, data.format);
        const carets = JSON.parse(player.carets(data.state, data.node)) as Carets | null;
        return post({ type: "typed", id: data.id, ...typed, carets });
      }
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
        player.addFile(path, bytes);
        return post({ type: "dropped", id: data.id, path });
      }
      case "attaching": {
        current(data.source);
        const bytes = new Uint8Array(data.bytes);
        const path = player.placing(data.name, bytes);
        player.addFile(path, bytes);
        const made = JSON.parse(player.attaching(path)) as { path: string; data: string; attached: Attached | null; patch: unknown[] };
        return post({ type: "attached", id: data.id, ...made });
      }
      case "sheet":
        return post({ type: "sheet", id: data.id, ...sheet(data.source, data.name) });
      case "dataEdit":
        return post({ type: "dataEdited", id: data.id, ...(await dataEdit(data.source, data.name, data.edits, data.index, data.format)) });
      case "dataUndo":
        return post({ type: "dataUndone", id: data.id, ...(await dataUndo(data.source, data.redo, data.index, data.format)) });
      case "bundleFiles":
        current(data.source);
        return post({ type: "bundleFiles", id: data.id, files: JSON.parse(player.bundleFiles()) as BundleFile[] });
      case "removeFile": {
        compiles(data.source);
        player.removeFile(data.path);
        latest++;
        shown = { index: data.index, format: data.format };
        return post({ type: "removed", id: data.id, edited: await edit(player.source(), data.index, data.format) });
      }
      case "versions": {
        const module = await history();
        if (!module) return post({ type: "versions", id: data.id, versions: null });
        const changes = JSON.parse(module.changes(player.file(HISTORY)!)) as Change[];
        const at = (s: number) => (s > 0 ? new Date(s * 1000).toISOString() : null);
        const versions = changes.map((c, i) => ({ n: i + 1, id: c.id, author: c.author, message: c.message, at: at(c.timestamp), ops: c.ops }));
        return post({ type: "versions", id: data.id, versions });
      }
      case "version": {
        const held = await version(data.version);
        const states = JSON.parse(player.viewVersion(held)) as string[];
        const state = data.state !== undefined && states.includes(data.state) ? data.state : states[0];
        // A version that cannot be drawn in the bundle as it is still lists its states, and why.
        let png: ArrayBuffer | undefined;
        let why: string | undefined;
        try {
          png = player.versionPng(state, data.width).slice().buffer as ArrayBuffer;
        } catch (e) {
          why = said(e);
        }
        return post({ type: "version", id: data.id, states, state, png, why }, png ? [png] : []);
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
        player.writeFiles(JSON.stringify(data.files));
        if (!data.edit) return post({ type: "filesWritten", id: data.id });
        latest++;
        shown = { index: data.edit.index, format: data.edit.format };
        return post({ type: "filesWritten", id: data.id, edited: await edit(data.edit.source, data.edit.index, data.edit.format) });
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
    post({ type: "error", id, message: said(e) });
  }
};

/** Language `code`'s hyphenation patterns, which the editor's module leaves out (ADR-0015). The
 * engine asks the first time a text hyphenates in it, in the middle of a layout, so they are
 * fetched at once, as a worker may. The player's module has every language, and never asks. */
function patterns(code: string): Uint8Array | undefined {
  const url = hyphenation[code];
  if (!url) return undefined;
  const request = new XMLHttpRequest();
  request.open("GET", url, false);
  request.responseType = "arraybuffer";
  try {
    request.send();
  } catch {
    return undefined;
  }
  return request.status === 200 ? new Uint8Array(request.response as ArrayBuffer) : undefined;
}

interface DeckFiles {
  theme: string;
  fonts?: { file: string }[];
  data?: Record<string, { source?: unknown }>;
  states: { id: string; notes?: string }[];
  spine?: { sections?: { title?: string; beats?: { claim?: string; states?: string[]; notes?: string }[] }[] };
}

async function open(source: Source, painter: Painter, target: OffscreenCanvas, engine?: WebAssembly.Module) {
  await init(engine && { module_or_path: engine });
  setHyphenation(patterns);
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
  slots = opening();
  [canvas.width, canvas.height] = size();
  gpu?.resize(canvas.width, canvas.height);
  post({ type: "ready", ...opened() });
}

/** The open bundle, as the page hears of it. */
function opened(): Opened {
  const deck = JSON.parse(new TextDecoder().decode(player.file("deck.json"))) as DeckFiles;
  const noted = notes(deck);
  return {
    name,
    where: home?.where,
    states: slots.map((slot) => slot.state),
    formats: player.formats(),
    notes: slots.map((slot) => noted.get(slot.state) ?? ""),
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
 * writes it back, as the page `id`, so the page does not hear its own save as a change. */
function served(deck: string, id: string, folder: string): Home {
  const sent = async (method: "PUT" | "DELETE", path: string, bytes?: Uint8Array) => {
    const url = new URL(path.split("/").map(encodeURIComponent).join("/"), deck);
    const body = bytes as Uint8Array<ArrayBuffer> | undefined;
    const response = await fetch(url, { method, body, headers: { "X-Scaena-Client": id } });
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
    player = await fetched(source.url);
    name = nameOf(source.url);
    if (source.serve) {
      // The folder's own name, as `scaena serve` says it.
      const state = await fetch(new URL("/scaena/state", source.url)).then((r) => r.json() as Promise<{ name?: string }>);
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
 * history. */
async function fetched(deck: string): Promise<Player> {
  const get = async (path: string) => {
    const response = await fetch(new URL(path, deck));
    if (!response.ok) throw new Error(`${path}: HTTP ${response.status}`);
    return response;
  };
  const json = await (await get(deck)).text();
  const files = JSON.parse(json) as DeckFiles;
  const fetchedPlayer = new Player(json, await (await get(files.theme)).text());
  const data = Object.values(files.data ?? {}).flatMap(({ source }) => (typeof source === "string" ? [source] : []));
  const paths = new Set([...(files.fonts ?? []).map((f) => f.file), ...data, ...fetchedPlayer.imageFiles()]);
  if (new URL(deck).pathname.endsWith("/deck.json")) {
    const manifest = await fetch(new URL("manifest.json", deck)).catch(() => undefined);
    if (manifest?.ok) {
      const bytes = new Uint8Array(await manifest.arrayBuffer());
      fetchedPlayer.addFile("manifest.json", bytes);
      const listed = (JSON.parse(new TextDecoder().decode(bytes)) as { files?: Record<string, string> }).files;
      for (const path of Object.keys(listed ?? {})) paths.add(path);
    }
  }
  paths.delete(files.theme);
  const bytes = await Promise.all(
    [...paths].map(async (path) => [path, new Uint8Array(await (await get(path)).arrayBuffer())] as const),
  );
  for (const [path, file] of bytes) fetchedPlayer.addFile(path, file);
  return fetchedPlayer;
}

/** The theme that ships as `theme`, which the page carries (`themes.ts`), fetched now: its file's
 * name, its text, and the fonts it names, by the paths its families give them. */
async function shipped(theme: string): Promise<{ file: string; text: string; fonts: Map<string, Uint8Array> }> {
  const { themes, fonts } = await import("@scaena/themes");
  const chosen = themes[theme];
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

/** A new deck titled `title` (PLAN 2.12), as `deck_create` makes one: the theme that ships as
 * `theme` and the fonts it names, and one state with nothing on it. */
async function made(theme: string, title: string): Promise<Player> {
  const { file, text, fonts } = await shipped(theme);
  return Player.create(file, text, title, fonts);
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
  if (!why) return;
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
    case "pdf": {
      const laid = player.pdfLaidOut();
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
  const assistant = await import("@scaena/assistant");
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

/** Each state's notes, by its id: its own, else those of the first beat that names it. */
function notes(files: DeckFiles): Map<string, string> {
  const beats = new Map<string, string>();
  for (const section of files.spine?.sections ?? [])
    for (const beat of section.beats ?? [])
      for (const state of beat.states ?? []) if (!beats.has(state)) beats.set(state, beat.notes ?? "");
  return new Map(files.states.map((s) => [s.id, s.notes ?? beats.get(s.id) ?? ""]));
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

/** The canvas frames are laid out on now, in whole pixels. */
const size = () => Array.from(player.canvasSize(), Math.round) as [number, number];

/** Lay frames out in `next` from now on; `again` after the deck has changed. */
/** The strip's thumbnails are asked for again: the last one asked stops. */
let thumbing = 0;

/** Each state of the timeline at rest, `height` pixels high, for the state strip (PLAN 2.35):
 * pixels for each whose drawing (its display list's digest) is not the one `known` holds. A
 * state at a time, the worker answering what else is asked between them; a newer request stops
 * it with what it has. A state the deck no longer has, changed meanwhile, is left out. */
async function thumbnails(height: number, known: Record<string, string>): Promise<Thumb[]> {
  const mine = ++thumbing;
  const [w, h] = player.canvasSize();
  const width = Math.max(1, Math.round((height * w) / h));
  const thumbs: Thumb[] = [];
  for (const { state } of slots) {
    if (mine !== thumbing) break;
    try {
      const digest = player.digest(state);
      if (known[state] === digest) {
        thumbs.push({ state, digest });
        continue;
      }
      const pixels = player.pixels(state, Infinity, width);
      thumbs.push({ state, digest, width, height: pixels.length / 4 / width, pixels: pixels.buffer as ArrayBuffer });
    } catch {
      continue;
    }
    await new Promise((go) => setTimeout(go, 0));
  }
  return thumbs;
}

/** Each state whose drawing is not the one `known` holds, at rest, `height` pixels high, as the
 * strip draws it, and each `known` holds that the deck no longer has (PLAN 2.93); `known` takes
 * the drawings as they are now. All at once: an assistant's next call waits for it. */
function redrawn(height: number, known: Record<string, string>): { drawn: Thumb[]; gone: string[] } {
  const [w, h] = player.canvasSize();
  const width = Math.max(1, Math.round((height * w) / h));
  const drawn: Thumb[] = [];
  for (const { state } of slots) {
    try {
      const digest = player.digest(state);
      if (known[state] === digest) continue;
      const pixels = player.pixels(state, Infinity, width);
      drawn.push({ state, digest, width, height: pixels.length / 4 / width, pixels: pixels.buffer as ArrayBuffer });
      known[state] = digest;
    } catch {
      continue;
    }
  }
  const gone = Object.keys(known).filter((state) => !slots.some((s) => s.state === state));
  for (const state of gone) delete known[state];
  return { drawn, gone };
}

/** The deck the editor's `source` compiles to: compiled first where the deck shown is not yet that
 * one, after an undo or while the source is typed in. */
function current(source: string) {
  if (player.compiledFrom(source)) return;
  saveable(source);
  layOut(format, true);
}

function layOut(next: string | undefined, again = false) {
  if (next === format && !again) return;
  player.setFormat(next);
  format = next;
  slots = timeline();
}

/** Compile `source`. Once it validates, repaint slot `index` at rest from the new deck, then
 * lint it: the editor's round trip (PLAN 2.3), each step timed. */
async function edit(source: string, index: number, at: string | undefined): Promise<Edited> {
  const start = performance.now();
  const compiled = JSON.parse(player.compile(source)) as {
    error?: Finding;
    findings: Finding[];
    states: [string, number][];
    valid: boolean;
  };
  const compiledAt = performance.now();
  let where: Edited["at"];
  let shown: string | undefined;
  if (compiled.valid) {
    // A format the deck no longer lists falls back to its own canvas.
    layOut(at !== undefined && player.formats().includes(at) ? at : undefined, true);
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
    : (JSON.parse(player.lint(shown)) as { findings: Finding[]; laid: boolean; whole: boolean });
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
    throw new Error(why ? `the deck refuses it: ${why.message}` : "it is there already");
  }
  latest++;
  shown = { index, format: at };
  const next = player.source();
  return { source: next, edited: await edit(next, index, at) };
}

/** Compile `source` into the deck a data edit works on (PLAN 2.55), which need not validate: a
 * cell its column does not read (E103) leaves the deck invalid, and the edit is what fixes it. */
function compiles(source: string) {
  if (player.compiledFrom(source)) return;
  const compiled = JSON.parse(player.compile(source)) as { error?: Finding };
  if (!compiled.error) return;
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
 * after is compiled, shown at slot `index`, and linted, as an edit of it is. */
async function type(source: string, ops: unknown[], index: number, at: string | undefined): Promise<{ source: string; edited: Edited }> {
  saveable(source);
  player.setMoving([], 0, 0);
  player.preview(undefined);
  if (!player.typed(JSON.stringify(ops), new Date().toISOString())) throw new Error("it reads so already");
  latest++;
  shown = { index, format: at };
  const next = player.source();
  return { source: next, edited: await edit(next, index, at) };
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
async function paint(state: string, t: number, beside: Beside = "paint") {
  lastPainted = { state, t };
  const [width, height] = size();
  if (canvas.width !== width || canvas.height !== height) {
    [canvas.width, canvas.height] = [width, height];
    gpu?.resize(width, height);
  }
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
    if (!playing) return;
    if (still) setTimeout(() => frame(performance.now()), slot.span + slot.hold - t);
    else next(frame);
  };
  next(frame);
}

const next = (frame: (now: number) => void) => {
  if ("requestAnimationFrame" in self) self.requestAnimationFrame(frame);
  else setTimeout(() => frame(performance.now()), 1000 / 60);
};
