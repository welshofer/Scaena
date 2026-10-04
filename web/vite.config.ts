import { readFile } from "node:fs/promises";
import { join, normalize, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";

const repo = fileURLToPath(new URL("..", import.meta.url));
const web = fileURLToPath(new URL(".", import.meta.url));

/** In development, the repository's bundles at their paths in it (`/tests/fixtures/torture.scaena`),
 * as a static server at the repository's root serves them beside the built player. */
const bundles: Plugin = {
  name: "scaena-bundles",
  configureServer(server) {
    server.middlewares.use(async (req, res, next) => {
      const path = normalize(join(repo, decodeURIComponent(new URL(req.url ?? "/", "http://x").pathname)));
      if (![join(repo, "tests"), join(repo, "docs")].some((dir) => path.startsWith(dir + sep))) return next();
      try {
        res.end(await readFile(path));
      } catch {
        next();
      }
    });
  },
};

export default defineConfig({
  // Relative paths: the build plays from any directory.
  base: "./",
  plugins: [bundles],
  // The WASM engine and its glue, as `just wasm` builds them (PLAN 0.8); the font subsetter,
  // which the worker loads only to download a bundle (PLAN 2.4); the history, which it loads
  // only to save a bundle that keeps one (PLAN 2.9); and the assistant, with what it reads,
  // which the worker loads the first time it is asked something (PLAN 2.6); and the themes a
  // new deck starts from, with their fonts, which it loads only to make one (PLAN 2.12).
  resolve: {
    alias: {
      "@scaena/wasm": join(repo, "crates/scaena-wasm/www/pkg/scaena_wasm.js"),
      "@scaena/subset": join(repo, "crates/scaena-subset/pkg/scaena_subset.js"),
      "@scaena/history": join(repo, "crates/scaena-history/pkg/scaena_history.js"),
      "@scaena/resources": join(repo, "crates/scaena-resources/pkg/scaena_resources.js"),
      "@scaena/assistant": join(web, "src/assistant/index.ts"),
      "@scaena/themes": join(web, "src/themes.ts"),
    },
  },
  worker: { format: "es" },
  server: { fs: { allow: [repo] } },
  build: {
    target: "es2022",
    // The player (PLAN 2.1–2.2) and the source editor (PLAN 2.3), sharing the engine's worker.
    rollupOptions: { input: { player: join(web, "index.html"), editor: join(web, "editor.html") } },
  },
});
