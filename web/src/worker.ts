// The engine's worker (PLAN 2.1, SPEC §9.2): the bundle, the WASM engine, and the canvas the
// page handed over. It paints with vello on WebGPU where the browser has an adapter, and
// otherwise with vello_cpu, whose frames reach the canvas as ImageBitmaps.
import init, { Canvas, Player } from "@scaena/wasm";
import type { FromWorker, Painter, Slot, ToWorker } from "./protocol";

const post = (message: FromWorker) => self.postMessage(message);
const said = (e: unknown) => (e instanceof Error ? e.message : String(e));

let player: Player;
let canvas: OffscreenCanvas;
/** WebGPU, when it paints. */
let gpu: Canvas | undefined;
/** Where the CPU painter's frames go, when it paints. */
let bitmaps: ImageBitmapRenderingContext | undefined;
/** The format the engine lays frames out in now; `undefined` is the deck's own canvas. */
let format: string | undefined;
/** Counts the requests that paint: a cue playing stops when a newer one comes. */
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
      case "play":
        layOut(data.format);
        return play(data.state, ++latest);
      case "timeline":
        layOut(data.format);
        return post({ type: "timeline", id: data.id, slots: JSON.parse(player.timeline()) as Slot[] });
    }
  } catch (e) {
    post({ type: "error", id, message: said(e) });
  }
};

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
  const files = JSON.parse(json) as { theme: string; fonts?: { file: string }[]; data?: Record<string, { source?: unknown }> };
  player = new Player(json, await (await get(files.theme)).text());
  const data = Object.values(files.data ?? {}).flatMap(({ source }) => (typeof source === "string" ? [source] : []));
  const [fonts, tables, images] = await Promise.all(
    [(files.fonts ?? []).map((f) => f.file), data, player.imageFiles()].map((paths) => Promise.all(paths.map(bytes))),
  );
  for (const [path, font] of fonts) player.addFont(path, font);
  for (const [path, table] of tables) player.addData(path, table);
  for (const [path, image] of images) player.addImage(path, image);
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
    painter: gpu ? "webgpu" : "cpu",
    adapter: gpu?.adapter ?? "vello_cpu",
  });
}

/** The canvas frames are laid out on now, in whole pixels. */
const size = () => Array.from(player.canvasSize(), Math.round) as [number, number];

/** Lay frames out in `next` from now on. */
function layOut(next: string | undefined) {
  if (next === format) return;
  player.setFormat(next);
  format = next;
}

/** `state` `t` ms into its cue, on the canvas, sized to the format first. */
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

/** The cue into `state`, a frame each time the display takes one, then its rest. */
function play(state: string, run: number) {
  const span = player.duration(state);
  const start = performance.now();
  const frame = async (now: number) => {
    if (run !== latest) return;
    const t = now - start;
    try {
      await paint(state, t < span ? t : Infinity);
    } catch (e) {
      return post({ type: "error", message: said(e) });
    }
    if (t < span) next(frame);
    else post({ type: "rested", state });
  };
  next(frame);
}

const next = (frame: (now: number) => void) => {
  if ("requestAnimationFrame" in self) self.requestAnimationFrame(frame);
  else setTimeout(() => frame(performance.now()), 1000 / 60);
};
