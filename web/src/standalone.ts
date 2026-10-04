// A single-file export's page (PLAN 2.5, SPEC §10): the player (`player.ts`), the engine, and
// a bundle, in one HTML file that plays from a disk or a USB stick and reaches no network. The
// build carries the engine: the player's module alone, gzipped, in base64. `scaena export
// --format html` fills in the rest: the title and language, the bundle's files (each gzipped,
// in base64, by its path in the bundle), the states it plays, and how each reads.
//
// The page compiles the engine and hands it to the worker, which starts from this page's own
// code: a browser starts no worker from a file's address, nor a module worker from a blob.
// `?painter=` and `?state=` are the player's.
import "./player.css";
import { start } from "./player";
import type { Painter } from "./protocol";
import Inline from "./worker?worker&inline";

const $ = <T extends Element>(selector: string) => document.querySelector<T>(selector)!;
const params = new URLSearchParams(location.search);

/** Base64's bytes: by the browser's own decoder where it has one. */
const decoded = (text: string): Uint8Array<ArrayBuffer> => {
  const native = Uint8Array as { fromBase64?: (text: string) => Uint8Array<ArrayBuffer> };
  return native.fromBase64 ? native.fromBase64(text) : Uint8Array.from(atob(text), (c) => c.charCodeAt(0));
};

/** The bytes an element carries: gzipped, in base64. */
const unpacked = (element: Element): Promise<ArrayBuffer> =>
  new Response(new Blob([decoded(element.textContent ?? "")]).stream().pipeThrough(new DecompressionStream("gzip")))
    .arrayBuffer();

/** How each state the export wrote reads: a template of its own, each node an element of
 * its own (`data-node`). The player shows it in the live region as the deck comes to it. */
const readings = new Map(
  [...document.querySelectorAll<HTMLTemplateElement>("template[data-state]")].map((t) => [t.dataset.state!, t.innerHTML]),
);

try {
  const [module, files] = await Promise.all([
    unpacked($("#scaena-engine")).then((bytes) => WebAssembly.compile(bytes)),
    Promise.all(
      [...document.querySelectorAll("script[data-path]")].map(
        async (el) => [el.getAttribute("data-path")!, await unpacked(el)] as const,
      ),
    ),
  ]);
  const { name, states } = JSON.parse($("#scaena-deck").textContent!) as { name: string; states: string[] };
  await start(
    { files: Object.fromEntries(files), name, states },
    {
      painter: (params.get("painter") ?? "auto") as Painter,
      engine: { spawn: () => new Inline(), module },
      channel: `scaena:${location.href.split(/[?#]/)[0]}`,
      state: params.get("state"),
      read: (state) => readings.get(state) ?? "",
    },
  );
} catch (e) {
  $("#status").textContent = `error: ${e instanceof Error ? e.message : String(e)}`;
}
