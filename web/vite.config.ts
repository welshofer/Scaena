import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
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

/** Each theme that ships drawn small by the CPU painter (PLAN T.0c), its title slide as New starts
 * one, into `target/themes/<slug>.png`, which `src/themes.ts` imports: drawn once as the build or
 * the dev server starts, by `scaena-cli`'s `theme_thumbnails` example. */
const thumbnails: Plugin = {
  name: "scaena-theme-thumbnails",
  buildStart() {
    execFileSync("cargo", ["run", "-q", "--locked", "-p", "scaena-cli", "--example", "theme_thumbnails", "--", join(repo, "target/themes")], {
      cwd: repo,
      stdio: "inherit",
    });
  },
};

/** The cache of upstream fonts by sha256 that `scripts/build_bundle_fonts.py --fetch --shipped`
 * fills (PLAN T.0b, T.0c), where `src/themes.ts` finds each font git does not hold. */
const fontCache = process.env.SCAENA_FONT_CACHE ?? join(homedir(), ".cache/scaena-fonts");

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

/** What the player's and the editor's pages may load and run (lift-up SEC-4): a Content-Security-Policy
 * written into each page the build writes, as the single file's page carries its own
 * (`standalone.html`).
 * - Scripts are the site's own files and the engine's WebAssembly, never one written into a page, so
 *   a deck's words or a theme's names that slip into markup run nothing. Workers are the site's,
 *   or a `blob:`.
 * - Styles may be inline: the pages and the overlay set them on what they draw.
 * - Pictures and fonts are the site's, or made in the page (`data:`, `blob:`).
 * - `connect-src` is the site, `data:` and `blob:`, and any HTTPS host. The assistant's providers
 *   alone (api.anthropic.com, api.openai.com, generativelanguage.googleapis.com) would not do: a
 *   deck opens from any address that answers CORS (`?bundle=`), and a deck made from a prompt reads
 *   the pages, repositories, and maps its sources name (`scaena_sources::fetch`, Geoapify). So the
 *   policy keeps a script from running; it does not keep one that runs from reaching a host.
 * - Nothing else: no plugins, no frames, no `<base>`, no form posted.
 * The development server goes without, as its client writes scripts of its own. */
const policy = [
  "default-src 'self'",
  "script-src 'self' 'wasm-unsafe-eval'",
  "worker-src 'self' blob:",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' data: blob:",
  "font-src 'self' data: blob:",
  "media-src 'self' data: blob:",
  "connect-src 'self' data: blob: https:",
  "manifest-src 'self'",
  "object-src 'none'",
  "frame-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
].join("; ");

const secured: Plugin = {
  name: "scaena-content-security-policy",
  apply: "build",
  transformIndexHtml: {
    order: "pre",
    handler: (html) => {
      const charset = '<meta charset="utf-8" />';
      if (!html.includes(charset)) throw new Error(`a page with no \`${charset}\` to write the policy after`);
      return html.replace(charset, `${charset}\n    <meta http-equiv="Content-Security-Policy" content="${policy}" />`);
    },
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
// - The site's files, every one the build wrote, are kept as the worker installs, each fetched past
//   the browser's HTTP cache (lift-up REL-2), so a page an older build left there is never kept
//   beside this build's modules; and answered from the cache, a query aside
//   (\`editor.html?bundle=…\`).
// - A newer build's worker installs beside this one and waits: the pages keep this build whole
//   until one asks for the new one (\`skipWaiting\`, from the page's "A new version is ready"),
//   and each page then loads again (\`offline.ts\`). Pages never run one build's pages with
//   another's modules.
// - Anything else under the site (a deck's files) is asked of the network, and the answer kept for
//   when there is none; one that cannot be kept (the quota full) is answered all the same. Requests
//   elsewhere, and a page's events (\`scaena serve\`), pass by.
const version = ${JSON.stringify(version)};
const files = ${JSON.stringify(files)};
const site = "scaena-site-" + version;
const kept = "scaena-kept";

self.addEventListener("install", (e) => {
  e.waitUntil(caches.open(site).then((cache) => cache.addAll(files.map((f) => new Request(f, { cache: "reload" })))));
});

self.addEventListener("message", (e) => {
  if (e.data === "skipWaiting") self.skipWaiting();
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
    if (response.ok) {
      try {
        await cache.put(request, response.clone());
      } catch {
        // Not kept, the quota full or the response too large to keep: the network's answer stands.
      }
    }
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
  plugins: [thumbnails, bundles, singleFile, secured, offline],
  // The WASM engine and its glue, as `just wasm` builds them (PLAN 0.8); the font subsetter,
  // which the worker loads only to download a bundle (PLAN 2.4); the history, which it loads
  // only to save a bundle that keeps one (PLAN 2.9); and the assistant, with what it reads,
  // which the worker loads the first time it is asked something (PLAN 2.6); the themes a new
  // deck starts from, with their fonts, which it loads only to make one (PLAN 2.12); the
  // PDF painter, which it loads the first time a PDF is exported (PLAN 2.54); the generator,
  // which a worker of its own loads to make a deck from a prompt (PLAN G.5); the hyphenation
  // patterns the engine's module leaves out, each fetched the first time a text hyphenates in its
  // language (ADR-0015); and the code scanners' tables it leaves out too, each fetched the first
  // time a code text in a language that reads it lays out (PLAN 2.132); and the fonts of the
  // themes that ship that git does not hold, from the sha256 cache (PLAN T.0c).
  resolve: {
    alias: {
      "@scaena/wasm": join(repo, "crates/scaena-wasm/www/pkg/scaena_wasm.js"),
      "@scaena/subset": join(repo, "crates/scaena-subset/pkg/scaena_subset.js"),
      "@scaena/history": join(repo, "crates/scaena-history/pkg/scaena_history.js"),
      "@scaena/resources": join(repo, "crates/scaena-resources/pkg/scaena_resources.js"),
      "@scaena/pdf": join(repo, "crates/scaena-pdf/pkg/scaena_pdf.js"),
      "@scaena/generator": join(repo, "crates/scaena-generator/pkg/scaena_generator.js"),
      "@scaena/generate": join(web, "src/generate.ts"),
      "@scaena/assistant": join(web, "src/assistant/index.ts"),
      "@scaena/themes": join(web, "src/themes.ts"),
      "@scaena/font-cache": fontCache,
      "@scaena/hyphenation": join(web, "src/hyphenation.ts"),
      "@scaena/code-tables": join(web, "src/code-tables.ts"),
    },
  },
  worker: { format: "es" },
  server: { fs: { allow: [repo, fontCache] } },
  build: {
    target: "es2022",
    // The player (PLAN 2.1–2.2) and the source editor (PLAN 2.3), sharing the engine's worker.
    rollupOptions: { input: { player: join(web, "index.html"), editor: join(web, "editor.html") } },
  },
});
