import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { join, normalize, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";

const repo = fileURLToPath(new URL("..", import.meta.url));
const web = fileURLToPath(new URL(".", import.meta.url));

/** The page a single-file export fills in (PLAN 2.5), as `vite.standalone.config.ts` builds it, beside
 * the editor, which fills it in to export one (PLAN 2.54): that build goes first. */
const standalone = join(repo, "crates/scaena-export/player/standalone.html");
const singleFile: Plugin = {
  name: "scaena-single-file-page",
  generateBundle() {
    let source: Buffer;
    try {
      source = readFileSync(standalone);
    } catch {
      throw new Error(`no ${standalone}: build it first, with \`vite build --config vite.standalone.config.ts\``);
    }
    this.emitFile({ type: "asset", fileName: "standalone.html", source });
  },
  configureServer(server) {
    server.middlewares.use("/standalone.html", (_, res, next) => {
      try {
        res.setHeader("Content-Type", "text/html");
        res.end(readFileSync(standalone));
      } catch {
        next();
      }
    });
  },
};

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

/** The site, installed (PLAN 2.73): `sw.js`, a service worker that keeps every file the build
 * writes, so the pages, the engine, and each module they load later work with no network; and the
 * web app manifest and its icon, by which a browser installs the site. The pages register it only
 * in the static site's build (`VITE_BUNDLE`, `src/offline.ts`). Its cache is named for what the
 * files hold: another build is another cache, taken once no page uses the last. */
const offline: Plugin = {
  name: "scaena-offline",
  enforce: "post",
  generateBundle(_, bundle) {
    const icon = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><rect width="512" height="512" rx="96" fill="#14121c"/><path d="M128 352 L256 160 L384 352 Z" fill="none" stroke="#e8b04b" stroke-width="36" stroke-linejoin="round"/><circle cx="256" cy="300" r="34" fill="#e8b04b"/></svg>\n`;
    const manifest = `${JSON.stringify(
      {
        name: "Scaena",
        short_name: "Scaena",
        description: "Decks as a timeline of states: play them, and edit them, with or without a network.",
        start_url: "./",
        scope: "./",
        display: "standalone",
        background_color: "#14121c",
        theme_color: "#14121c",
        icons: [{ src: "icon.svg", sizes: "any", type: "image/svg+xml", purpose: "any" }],
      },
      null,
      2,
    )}\n`;
    this.emitFile({ type: "asset", fileName: "icon.svg", source: icon });
    this.emitFile({ type: "asset", fileName: "manifest.webmanifest", source: manifest });
    const files = Object.keys(bundle).filter((f) => f !== "sw.js" && !f.endsWith(".map")).sort();
    const hash = createHash("sha256");
    for (const f of files) {
      const out = bundle[f];
      hash.update(f).update(out.type === "chunk" ? out.code : out.source);
    }
    const version = hash.digest("hex").slice(0, 16);
    this.emitFile({ type: "asset", fileName: "sw.js", source: worker(version, ["./", ...files]) });
  },
};

/** The service worker's script: `files`, the site's, kept in a cache named for `version`. */
function worker(version: string, files: string[]): string {
  return `// Scaena's service worker (PLAN 2.73), written by the build: the site with no network.
// - The site's files, every one the build wrote, are kept as the worker installs, and answered
//   from the cache, a query aside (\`editor.html?bundle=…\`).
// - Anything else under the site (a deck's files) is asked of the network, and the answer kept for
//   when there is none. Requests elsewhere, and a page's events (\`scaena serve\`), pass by.
const version = ${JSON.stringify(version)};
const files = ${JSON.stringify(files)};
const site = "scaena-site-" + version;
const kept = "scaena-kept";

self.addEventListener("install", (e) => {
  e.waitUntil(caches.open(site).then((cache) => cache.addAll(files)));
});

self.addEventListener("activate", (e) => {
  e.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k.startsWith("scaena-site-") && k !== site).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (e) => {
  const request = e.request;
  if (request.method !== "GET" || !request.url.startsWith(self.registration.scope)) return;
  if ((request.headers.get("accept") ?? "").includes("text/event-stream")) return;
  e.respondWith(answer(request));
});

async function answer(request) {
  const url = new URL(request.url);
  url.search = "";
  url.hash = "";
  const built = await (await caches.open(site)).match(url.href);
  if (built) return built;
  const cache = await caches.open(kept);
  try {
    const response = await fetch(request);
    if (response.ok) await cache.put(request, response.clone());
    return response;
  } catch (error) {
    const was = await cache.match(request);
    if (was) return was;
    throw error;
  }
}
`;
}

export default defineConfig({
  // Relative paths: the build plays from any directory.
  base: "./",
  plugins: [bundles, singleFile, offline],
  // The WASM engine and its glue, as `just wasm` builds them (PLAN 0.8); the font subsetter,
  // which the worker loads only to download a bundle (PLAN 2.4); the history, which it loads
  // only to save a bundle that keeps one (PLAN 2.9); and the assistant, with what it reads,
  // which the worker loads the first time it is asked something (PLAN 2.6); the themes a new
  // deck starts from, with their fonts, which it loads only to make one (PLAN 2.12); the
  // PDF painter, which it loads the first time a PDF is exported (PLAN 2.54); and the
  // hyphenation patterns the engine's module leaves out, each fetched the first time a text
  // hyphenates in its language (ADR-0015).
  resolve: {
    alias: {
      "@scaena/wasm": join(repo, "crates/scaena-wasm/www/pkg/scaena_wasm.js"),
      "@scaena/subset": join(repo, "crates/scaena-subset/pkg/scaena_subset.js"),
      "@scaena/history": join(repo, "crates/scaena-history/pkg/scaena_history.js"),
      "@scaena/resources": join(repo, "crates/scaena-resources/pkg/scaena_resources.js"),
      "@scaena/pdf": join(repo, "crates/scaena-pdf/pkg/scaena_pdf.js"),
      "@scaena/assistant": join(web, "src/assistant/index.ts"),
      "@scaena/themes": join(web, "src/themes.ts"),
      "@scaena/hyphenation": join(web, "src/hyphenation.ts"),
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
