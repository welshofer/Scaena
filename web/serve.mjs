// What web/smoke.mjs and web/player.mjs share: the repository served from its root, so the
// built player at /web/dist/ reads bundles at their paths in the repository, and headless
// Chromium with WebGPU. Needs Playwright and its Chromium, as crates/scaena-wasm/www/smoke.mjs
// does; `playwright` resolves as `require` does, so a global install works with NODE_PATH.
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { extname, join, normalize } from "node:path";

const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".json": "application/json" };

/** The repository at `root`, served on a free port: `{ origin, close }`. */
export async function serve(root = process.cwd()) {
  const server = createServer(async (req, res) => {
    let path = normalize(join(root, decodeURIComponent(new URL(req.url, "http://x").pathname)));
    if (!path.startsWith(root)) return res.writeHead(403).end();
    if (path.endsWith("/")) path = join(path, "index.html");
    try {
      const body = await readFile(path);
      res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" }).end(body);
    } catch {
      res.writeHead(404).end();
    }
  }).listen(0, "127.0.0.1");
  await new Promise((ok) => server.once("listening", ok));
  return { origin: `http://127.0.0.1:${server.address().port}`, close: () => server.close() };
}

/** Headless Chromium with WebGPU, which leaves WebGPU canvases blank unless its compositor
 * runs on Vulkan. */
export function launch() {
  const { chromium } = createRequire(import.meta.url)("playwright");
  return chromium.launch({
    args: ["--enable-unsafe-webgpu", "--enable-features=Vulkan", "--use-vulkan=swiftshader", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"],
  });
}
