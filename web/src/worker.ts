// The engine's worker (PLAN 2.1–2.4, SPEC §9.2): the bundle, the WASM engine, and the canvas
// the page handed over. It paints with vello on WebGPU where the browser has an adapter, and
// otherwise with vello_cpu, whose frames reach the canvas as ImageBitmaps. It keeps the
// deck's clock: a run plays the global timeline (SPEC §2.4), cue by cue and hold by hold. For
// the editor it compiles `.scn` as it is typed, lints it, fixes it, and inspects a state; it
// saves the bundle where it is kept, zips it, and takes files dropped on the page.
import init, { Canvas, Player } from "@scaena/wasm";
import { keptBundle, newBundle, readAll, remove, write } from "./folders";
import type { Edited, Finding, FromWorker, Painter, Slot, Source, ToWorker, Where } from "./protocol";

const post = (message: FromWorker, transfer: Transferable[] = []) => self.postMessage(message, transfer);
const said = (e: unknown) => (e instanceof Error ? e.message : String(e));

let player: Player;
/** The bundle's name, and where it is kept, if anywhere: a folder on disk, or one in the
 * browser's storage. */
let name = "deck";
let home: { dir: FileSystemDirectoryHandle; where: Where } | undefined;
let canvas: OffscreenCanvas;
/** WebGPU, when it paints. */
let gpu: Canvas | undefined;
/** Where the CPU painter's frames go, when it paints. */
let bitmaps: ImageBitmapRenderingContext | undefined;
/** The format the engine lays frames out in now (`undefined`: the deck's own canvas), and
 * the deck's timeline in it. */
let format: string | undefined;
let slots: Slot[] = [];
/** Counts the requests that paint: a run stops when a newer one comes. */
let latest = 0;

self.onmessage = async ({ data }: MessageEvent<ToWorker>) => {
  const id = "id" in data ? data.id : undefined;
  try {
    switch (data.type) {
      case "open":
        return await open(data.source, data.painter, data.canvas);
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
      case "run":
        layOut(data.format);
        return run(data.index, data.t, ++latest);
      case "seek": {
        latest++;
        layOut(data.format);
        const slot = slots[data.index];
        if (!slot) throw new Error(`the deck has no slot ${data.index}: it has ${slots.length}`);
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
      case "save":
        saveable(data.source);
        return post({ type: "saved", id: data.id, ...(await save()) });
      case "zip": {
        saveable(data.source);
        await subsetFonts();
        const saved = player.save(new Date().toISOString(), true);
        try {
          const bytes = saved.zip().buffer as ArrayBuffer;
          const { subset } = JSON.parse(saved.summary()) as { subset: [string, number, number][] };
          return post({ type: "zipped", id: data.id, bytes, subset }, [bytes]);
        } finally {
          saved.free();
        }
      }
      case "drop": {
        const bytes = new Uint8Array(data.bytes);
        const path = Player.place(data.name, bytes);
        player.addFile(path, bytes);
        return post({ type: "dropped", id: data.id, path });
      }
    }
  } catch (e) {
    post({ type: "error", id, message: said(e) });
  }
};

interface DeckFiles {
  theme: string;
  fonts?: { file: string }[];
  data?: Record<string, { source?: unknown }>;
  states: { id: string; notes?: string }[];
  spine?: { sections?: { beats?: { states?: string[]; notes?: string }[] }[] };
}

async function open(source: Source, painter: Painter, target: OffscreenCanvas) {
  await init();
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
    bitmaps = canvas.getContext("bitmaprenderer") ?? undefined;
    if (!bitmaps) throw new Error("the canvas takes no ImageBitmap");
  }
  await load(source);
  slots = JSON.parse(player.timeline()) as Slot[];
  [canvas.width, canvas.height] = size();
  gpu?.resize(canvas.width, canvas.height);
  const deck = JSON.parse(new TextDecoder().decode(player.file("deck.json"))) as DeckFiles;
  post({
    type: "ready",
    name,
    where: home?.where,
    states: player.states(),
    formats: player.formats(),
    notes: notes(deck),
    painter: gpu ? "webgpu" : "cpu",
    adapter: gpu?.adapter ?? "vello_cpu",
  });
}

/** Open the bundle at `source` (SPEC §3.1). A zip is copied into the browser's storage, and
 * kept there from then on. */
async function load(source: Source) {
  if ("url" in source) {
    player = await fetched(source.url);
    name = nameOf(source.url);
  } else if ("zip" in source) {
    player = Player.fromZip(new Uint8Array(source.zip));
    const dir = await newBundle(source.name);
    for (const path of player.files()) await write(dir, path, player.file(path)!);
    home = { dir, where: { kind: "opfs", name: dir.name } };
    name = dir.name;
  } else {
    const dir = "opfs" in source ? await keptBundle(source.opfs) : source.folder;
    player = opened(await readAll(dir));
    home = { dir, where: { kind: "opfs" in source ? "opfs" : "folder", name: dir.name } };
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

/** A bundle's files, by their paths inside it, opened. */
function opened(files: Map<string, Uint8Array>): Player {
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
  for (const font of fonts) player.addSubset(font, chars, subsetter.subset(player.file(font)!, chars));
}

/** Save the bundle with the deck shown, fonts kept whole, where it is kept, or into the
 * browser's storage: the files first, the deck and its manifest last, so the deck never names
 * a file not yet written; then the files the save renamed go. The session goes on from the
 * save. */
async function save(): Promise<{ where: Where; renamed: [string, string][]; files: number }> {
  if (!home) {
    const dir = await newBundle(name);
    home = { dir, where: { kind: "opfs", name: dir.name } };
    name = dir.name;
  }
  const saved = player.save(new Date().toISOString(), false);
  try {
    const paths = saved.paths();
    const last = ["deck.json", "manifest.json"];
    for (const path of [...paths.filter((p) => !last.includes(p)), ...last.filter((p) => paths.includes(p))])
      await write(home.dir, path, saved.file(path)!);
    const written = new Set(paths);
    for (const path of saved.replaced()) if (!written.has(path)) await remove(home.dir, path);
    const { renamed } = JSON.parse(saved.summary()) as { renamed: [string, string][] };
    player.adopt(saved);
    return { where: home.where, renamed, files: paths.length };
  } finally {
    saved.free();
  }
}

/** Each state's notes: its own, else those of the first beat that names it. */
function notes(files: DeckFiles): string[] {
  const beats = new Map<string, string>();
  for (const section of files.spine?.sections ?? [])
    for (const beat of section.beats ?? [])
      for (const state of beat.states ?? []) if (!beats.has(state)) beats.set(state, beat.notes ?? "");
  return files.states.map((s) => s.notes ?? beats.get(s.id) ?? "");
}

/** The canvas frames are laid out on now, in whole pixels. */
const size = () => Array.from(player.canvasSize(), Math.round) as [number, number];

/** Lay frames out in `next` from now on; `again` after the deck has changed. */
function layOut(next: string | undefined, again = false) {
  if (next === format && !again) return;
  player.setFormat(next);
  format = next;
  slots = JSON.parse(player.timeline()) as Slot[];
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
    at: where,
    ms: { compile: compiledAt - start, paint: paintedAt - compiledAt, lint: lintedAt - paintedAt },
  };
}

/** `state` `t` ms into its cue, on the canvas, sized to the format first. Past its span, it
 * is at rest; its shaders keep the timeline's time (SPEC §3.8). */
async function paint(state: string, t: number) {
  const [width, height] = size();
  if (canvas.width !== width || canvas.height !== height) {
    [canvas.width, canvas.height] = [width, height];
    gpu?.resize(width, height);
  }
  if (gpu) return player.paint(gpu, state, t);
  // A copy out of the module's memory, on an ArrayBuffer of its own.
  const frame = new ImageData(player.pixels(state, t, width) as Uint8ClampedArray<ArrayBuffer>, width);
  bitmaps!.transferFromImageBitmap(await createImageBitmap(frame));
}

/** The deck from slot `index`, `t` ms in, a frame each time the display takes one. A state
 * that holds, short of the last, gives way to the next when its cue and hold are over; one
 * that does not hold, and the last, comes to rest and waits. */
function run(index: number, t: number, run: number) {
  const goesOn = (i: number) => slots[i].hold > 0 && i < slots.length - 1;
  const start = performance.now();
  /** Where the clock started, from the slot it is in now. */
  let offset = t;
  const frame = async (now: number) => {
    if (run !== latest) return;
    let t = offset + Math.max(0, now - start);
    while (goesOn(index) && t >= slots[index].span + slots[index].hold) {
      offset -= slots[index].span + slots[index].hold;
      t -= slots[index].span + slots[index].hold;
      index++;
    }
    const slot = slots[index];
    const playing = goesOn(index) || t < slot.span;
    if (!playing) t = slot.span;
    try {
      await paint(slot.state, t);
    } catch (e) {
      return post({ type: "error", message: said(e) });
    }
    // A newer request came while the CPU painter painted: its frame is the one to report.
    if (run !== latest) return;
    post({ type: "at", index, t, global: slot.start + t, playing });
    if (playing) next(frame);
  };
  next(frame);
}

const next = (frame: (now: number) => void) => {
  if ("requestAnimationFrame" in self) self.requestAnimationFrame(frame);
  else setTimeout(() => frame(performance.now()), 1000 / 60);
};
