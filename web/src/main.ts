// The web player's page (PLAN 2.1–2.2, SPEC §9.2): the player (`player.ts`) on a bundle by
// its address. `?bundle=` is a bundle's directory or its deck file (by default the bundle the
// build names, the site's demo deck (PLAN 2.7), or else the torture deck), or `opfs:NAME`, a
// bundle the browser keeps (PLAN 2.4); `?painter=gpu` or `cpu` chooses who paints (WebGPU
// where the browser has an adapter, else the CPU painter, by default); `?state=` is the state
// to open on; `?view=presenter`, the presenter view. Edit opens the editor on the bundle.
import "./player.css";
import { sourceOf } from "./bundle";
import { start } from "./player";
import type { Painter } from "./protocol";
import { worker } from "./spawn";

const params = new URLSearchParams(location.search);
const bundle = params.get("bundle") ?? import.meta.env.VITE_BUNDLE ?? "../../tests/fixtures/torture.scaena";
const editing = new URLSearchParams({ bundle });
if (params.has("painter")) editing.set("painter", params.get("painter")!);
document.querySelector<HTMLAnchorElement>("#edit")!.href = `editor.html?${editing}`;

try {
  await start(sourceOf(bundle, bundle), {
    painter: (params.get("painter") ?? "auto") as Painter,
    engine: worker,
    channel: `scaena:${new URL(bundle, location.href).href}`,
    state: params.get("state"),
  });
} catch (e) {
  document.querySelector("#status")!.textContent = `error: ${e instanceof Error ? e.message : String(e)}`;
}
