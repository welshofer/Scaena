// The web player's page (PLAN 2.1–2.2, SPEC §9.2): the player (`player.ts`) on a bundle by
// its address. `?bundle=` is a bundle's directory or its deck file (the torture deck by
// default), or `opfs:NAME`, a bundle the browser keeps (PLAN 2.4); `?painter=gpu` or `cpu`
// chooses who paints (WebGPU where the browser has an adapter, else the CPU painter, by
// default); `?state=` is the state to open on; `?view=presenter`, the presenter view.
import "./player.css";
import { sourceOf } from "./bundle";
import { start } from "./player";
import type { Painter } from "./protocol";
import { worker } from "./spawn";

const params = new URLSearchParams(location.search);
const bundle = params.get("bundle") ?? "../../tests/fixtures/torture.scaena";

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
