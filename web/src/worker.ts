// The engine's worker (PLAN 2.1–2.3, SPEC §9.2): the bundle, the WASM engine, and the canvas
// the page handed over. It paints with vello on WebGPU where the browser has an adapter, and
// otherwise with vello_cpu, whose frames reach the canvas as ImageBitmaps. It keeps the
// deck's clock: a run plays the global timeline (SPEC §2.4), cue by cue and hold by hold. For
// the editor it compiles `.scn` as it is typed, lints it, fixes it, and inspects a state.
import init, { Canvas, Player } from "@scaena/wasm";
import type { Edited, Finding, FromWorker, Painter, Slot, ToWorker } from "./protocol";

const post = (message: FromWorker) => self.postMessage(message);
const said = (e: unknown) => (e instanceof Error ? e.message : String(e));

let player: Player;
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
        return await open(data.deck, data.painter, data.canvas);
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

async function open(deck: string, painter: Painter, target: OffscreenCanvas) {
  await init();
  // Every file is where the deck names it, from the deck's directory: the bundle (SPEC §3.1).
  const get = async (path: string) => {
    const response = await fetch(new URL(path, deck));
    if (!response.ok) throw new Error(`${path}: HTTP ${response.status}`);
    return response;
  };
  const bytes = async (path: string) => [path, new Uint8Array(await (await get(path)).arrayBuffer())] as const;
  const json = await (await get(deck)).text();
  const files = JSON.parse(json) as DeckFiles;
  player = new Player(json, await (await get(files.theme)).text());
  const data = Object.values(files.data ?? {}).flatMap(({ source }) => (typeof source === "string" ? [source] : []));
  const [fonts, tables, images] = await Promise.all(
    [(files.fonts ?? []).map((f) => f.file), data, player.imageFiles()].map((paths) => Promise.all(paths.map(bytes))),
  );
  for (const [path, font] of fonts) player.addFont(path, font);
  for (const [path, table] of tables) player.addData(path, table);
  for (const [path, image] of images) player.addImage(path, image);
  slots = JSON.parse(player.timeline()) as Slot[];
  canvas = target;
  [canvas.width, canvas.height] = size();
  if (painter !== "cpu") {
    // Ask for an adapter before WebGPU takes the canvas: a canvas WebGPU holds takes no other painter.
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
  post({
    type: "ready",
    states: player.states(),
    formats: player.formats(),
    notes: notes(files),
    painter: gpu ? "webgpu" : "cpu",
    adapter: gpu?.adapter ?? "vello_cpu",
  });
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
