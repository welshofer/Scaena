// The page a single-file export fills in (PLAN 2.5, SPEC §10): the player of `standalone.html`,
// its code, its styles, and the engine in one HTML file, which `scaena export --format html`
// carries (it is built in when `scaena` is built after this) and fills in with a bundle.
//
// - The worker is a classic script inside the page's code (`?worker&inline`): a browser
//   starts no worker from a file's address, nor a module worker from a blob.
// - The engine is the player's module alone (crates/scaena-wasm/player, `just wasm`),
//   gzipped, in base64, where the page has `<!--__SCAENA_ENGINE__-->`; the page compiles it
//   and hands it to the worker. The editor's operations and the font subsetter stay out.
// - The rest of the build goes: the engine's module as a file of its own, which the glue
//   names but the page never loads.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import { defineConfig, type Plugin } from "vite";

const repo = fileURLToPath(new URL("..", import.meta.url));
const web = fileURLToPath(new URL(".", import.meta.url));
const engine = join(repo, "crates/scaena-wasm/player/scaena_wasm_bg.wasm");

/** Code inside `<script>`: no `</script` or `<!--` in it ends the element early. Each is in a
 * string, a template, a regular expression, or a comment, where the escape means the same. */
const inScript = (code: string) => code.replace(/<\/(script)/gi, "<\\/$1").replace(/<!--/g, "<\\!--");

const singleFile: Plugin = {
  name: "scaena-single-file",
  enforce: "post",
  generateBundle(_, bundle) {
    const page = bundle["standalone.html"];
    if (page?.type !== "asset") throw new Error("the build made no standalone.html");
    let html = String(page.source);
    for (const [name, item] of Object.entries(bundle)) {
      if (item === page) continue;
      const at = (tag: string) => new RegExp(`<${tag}[^>]*(?:src|href)="[^"]*${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}"[^>]*>(?:</script>)?`);
      if (item.type === "chunk" && item.isEntry) {
        html = html.replace(at("script"), () => `<script type="module">${inScript(item.code)}</script>`);
      } else if (item.type === "asset" && name.endsWith(".css")) {
        html = html.replace(at("link"), () => `<style>${String(item.source)}</style>`);
      } else if (item.type === "chunk") {
        throw new Error(`the page's code split into ${name}: it must be one script`);
      }
      delete bundle[name];
    }
    const gz = gzipSync(readFileSync(engine), { level: 9 }).toString("base64");
    const marker = "<!--__SCAENA_ENGINE__-->";
    if (!html.includes(marker)) throw new Error(`standalone.html has no ${marker}`);
    html = html.replace(marker, () => `<script type="application/octet-stream" id="scaena-engine">${gz}</script>`);
    if (/<(script|link)[^>]*(src|href)="/.test(html)) throw new Error("standalone.html still loads a file");
    page.source = html;
  },
};

export default defineConfig({
  base: "./",
  plugins: [singleFile],
  resolve: {
    alias: {
      // The player's module alone, no subsetter, and no assistant: the page plays, it never
      // downloads or edits.
      "@scaena/wasm": join(repo, "crates/scaena-wasm/player/scaena_wasm.js"),
      "@scaena/subset": join(web, "src/no-subset.ts"),
      "@scaena/assistant": join(web, "src/no-assistant.ts"),
    },
  },
  worker: { format: "iife" },
  build: {
    target: "es2022",
    outDir: join(repo, "crates/scaena-export/player"),
    emptyOutDir: false,
    modulePreload: { polyfill: false },
    assetsInlineLimit: 0,
    rollupOptions: { input: { standalone: join(web, "standalone.html") } },
  },
});
