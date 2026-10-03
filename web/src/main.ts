// The player page (PLAN 2.1, SPEC §9.2): it hands its canvas to the engine's worker and asks
// for frames. `?bundle=` is a bundle's directory or its deck file (the torture deck by
// default); `?painter=gpu` or `cpu` chooses who paints (WebGPU where the browser has an
// adapter, else the CPU painter, by default); `?state=` is the state to open on.
import type { FromWorker, Opened, Painter, Slot, ToWorker } from "./protocol";

const params = new URLSearchParams(location.search);
const deck = deckFile(new URL(params.get("bundle") ?? "../../tests/fixtures/torture.scaena", location.href));
const painter = (params.get("painter") ?? "auto") as Painter;

const status = document.querySelector<HTMLElement>("#status")!;
const states = document.querySelector<HTMLSelectElement>("#state")!;
const formats = document.querySelector<HTMLSelectElement>("#format")!;

/** A bundle's deck file: the bundle's `deck.json`, or a deck file named outright, whose
 * directory is then the bundle (SPEC §3.1). */
function deckFile(bundle: URL): string {
  if (bundle.pathname.endsWith(".json")) return bundle.href;
  const dir = new URL(bundle);
  if (!dir.pathname.endsWith("/")) dir.pathname += "/";
  return new URL("deck.json", dir).href;
}

type Reply = Extract<FromWorker, { type: "shown" | "timeline" }>;
/** Requests not yet answered, by id. */
const waiting = new Map<number, { resolve: (reply: Reply) => void; reject: (error: Error) => void }>();
let asked = 0;

let worker: Worker;
let opened: Opened;

/** Open the bundle in a new worker, which paints into a new canvas: a canvas WebGPU has
 * held takes no other painter, so falling back to the CPU painter starts over. */
async function start(painter: Painter): Promise<Opened> {
  const stage = document.createElement("canvas");
  stage.id = "stage";
  document.querySelector("#stage")!.replaceWith(stage);
  worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
  const reply = new Promise<FromWorker>((resolve, reject) => {
    worker.onmessage = ({ data }: MessageEvent<FromWorker>) => resolve(data);
    worker.onerror = (e) => reject(new Error(e.message || "the worker failed to start"));
  });
  const canvas = stage.transferControlToOffscreen();
  worker.postMessage({ type: "open", deck, painter, canvas } satisfies ToWorker, [canvas]);
  const first = await reply;
  if (first.type === "ready") {
    worker.onmessage = listen;
    worker.onerror = (e) => fail(new Error(e.message));
    const { type: _, ...rest } = first;
    return rest;
  }
  worker.terminate();
  const message = first.type === "error" ? first.message : `the worker said ${first.type} first`;
  if (first.type === "error" && first.webgpu && painter === "auto") {
    console.warn(`${message}; the CPU painter paints instead`);
    return start("cpu");
  }
  throw new Error(message);
}

function listen({ data }: MessageEvent<FromWorker>) {
  switch (data.type) {
    case "shown":
    case "timeline":
      waiting.get(data.id)?.resolve(data);
      return waiting.delete(data.id);
    case "rested":
      return void (status.textContent = label(data.state));
    case "error":
      if (data.id !== undefined && waiting.has(data.id)) {
        waiting.get(data.id)!.reject(new Error(data.message));
        return waiting.delete(data.id);
      }
      return fail(new Error(data.message));
  }
}

const fail = (e: unknown) => void (status.textContent = `error: ${e instanceof Error ? e.message : String(e)}`);

function ask<T extends Reply["type"]>(message: Extract<ToWorker, { id: number }>): Promise<Extract<Reply, { type: T }>> {
  return new Promise((resolve, reject) => {
    waiting.set(message.id, { resolve: resolve as (reply: Reply) => void, reject });
    worker.postMessage(message);
  });
}

/** Paint `state` `t` ms into its cue (at rest without `t`), in `format` or on the deck's own
 * canvas. Resolves once the frame is on the canvas. */
const show = (state: string, t?: number, format?: string) =>
  ask<"shown">({ type: "show", id: ++asked, state, t, format }).then(({ size, ms }) => ({ size, ms }));

/** The deck's timeline in `format`, or on its own canvas. */
const timeline = (format?: string): Promise<Slot[]> =>
  ask<"timeline">({ type: "timeline", id: ++asked, format }).then(({ slots }) => slots);

const format = () => formats.value || undefined;
const label = (state: string) =>
  [state, format(), opened.painter === "webgpu" ? "WebGPU" : "CPU painter"].filter(Boolean).join(" · ");

try {
  opened = await start(painter);
  for (const id of opened.states) states.add(new Option(id, id));
  for (const id of opened.formats) formats.add(new Option(id, id));
  states.value = params.get("state") ?? opened.states[0];
  // Choosing a state plays the cue into it, then rests; another format lays the deck out again.
  states.onchange = () => {
    status.textContent = `${states.value} · playing`;
    worker.postMessage({ type: "play", state: states.value, format: format() } satisfies ToWorker);
  };
  formats.onchange = () =>
    void show(states.value, undefined, format()).then(() => (status.textContent = label(states.value)), fail);
  await show(states.value);
  status.textContent = label(states.value);
  // For tests and the console: the open bundle, its frames, and its timeline.
  Object.assign(window, { scaena: { ...opened, show, timeline } });
} catch (e) {
  fail(e);
}
