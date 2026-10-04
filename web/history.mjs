// PLAN 2.9 history check: the editor in headless Chromium (serve.mjs) records what it saves in
// a bundle's history, with the CRDT in a WASM module of its own that it loads only to save a
// bundle that keeps one. The CPU paints, as in web/editor.mjs.
//
//   node web/history.mjs     (after `just web`; from the repository's root)
//
// - The revenue example, saved by `scaena save --history`, opens from its URL without the
//   history's module. The user types an edit; the assistant, asked a question of a scripted
//   Anthropic server, renames the title; the user types again; and the save into the browser's
//   storage loads the module. After the history's first change, `scaena save`'s, it holds the
//   user's edit, the assistant's patch by `agent:scripted`, and the save by the user, each
//   stamped when it was made, as `scaena-history` reads it.
// - The history holds the deck as saved: copied out of the browser's storage, `scaena save`
//   finds nothing to take in as a change by `fs`.
// - A download records too: its history is the one kept, and the save that subset its fonts.
// - A bundle that keeps no history saves without the module, and keeps none.
// Exits 1 on any failure.
import { execFileSync } from "node:child_process";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { dirname, join } from "node:path";
import initHistory, { changes } from "../crates/scaena-history/pkg/scaena_history.js";
import { launch, serve } from "./serve.mjs";

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};
const scaena = (...args) => execFileSync("cargo", ["run", "-q", "-p", "scaena-cli", "--locked", "--", ...args], { stdio: "inherit" });

// The history's module, read here as the page's worker reads it.
await initHistory({ module_or_path: await readFile("crates/scaena-history/pkg/scaena_history_bg.wasm") });
/** Each change `history` holds, as `author: message`, and when. */
const log = (history) => JSON.parse(changes(history));
const said = (entries) => entries.map((c) => `${c.author}: ${c.message}`);

// The revenue example, keeping a history from its first save.
const bundle = "target/web-history/revenue";
await rm("target/web-history", { recursive: true, force: true });
scaena("save", "docs/examples/revenue.deck.json", "--to", bundle, "--history", "--keep-fonts");
const begun = log(await readFile(join(bundle, "history/deck.loro")));
check(begun.length === 1 && begun[0].message === "history begins", `scaena save --history begins it: ${JSON.stringify(said(begun))}`);

/** A scripted Anthropic server: it renames the title, then answers. */
const answer = "The title is the heading now.";
const scripted = createServer(async (req, res) => {
  const cors = {
    "access-control-allow-origin": req.headers.origin ?? "*",
    "access-control-allow-headers": req.headers["access-control-request-headers"] ?? "*",
    "access-control-allow-methods": "GET, POST, OPTIONS",
  };
  if (req.method === "OPTIONS") return res.writeHead(204, cors).end();
  const reply = (body) => res.writeHead(200, { ...cors, "content-type": "application/json" }).end(JSON.stringify(body));
  const path = new URL(req.url, "http://x").pathname;
  if (req.method === "GET" && path === "/v1/models") return reply({ data: [{ id: "scripted", type: "model" }] });
  let text = "";
  for await (const chunk of req) text += chunk;
  const answered = JSON.parse(text).messages.filter((m) => m.role === "assistant").length;
  const usage = { input_tokens: 1000, output_tokens: 20 };
  if (answered === 0) {
    const input = { ops: [{ op: "rename_node", id: "title", to: "heading" }] };
    return reply({ content: [{ type: "tool_use", id: "toolu_0", name: "deck_patch", input }], stop_reason: "tool_use", usage });
  }
  reply({ content: [{ type: "text", text: answer }], stop_reason: "end_turn", usage });
}).listen(0, "127.0.0.1");
await new Promise((ok) => scripted.once("listening", ok));
const provider = `http://127.0.0.1:${scripted.address().port}`;

/** Every file under `path` in the origin-private file system, by its path inside it, in base64. */
const kept = (page, path) =>
  page.evaluate(async (path) => {
    let dir = await navigator.storage.getDirectory();
    for (const part of path.split("/")) dir = await dir.getDirectoryHandle(part);
    const base64 = (bytes) => {
      let text = "";
      for (let i = 0; i < bytes.length; i += 0x8000) text += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
      return btoa(text);
    };
    const out = {};
    const walk = async (d, prefix) => {
      for await (const [name, handle] of d.entries()) {
        if (handle.kind === "directory") await walk(handle, `${prefix}${name}/`);
        else out[prefix + name] = base64(new Uint8Array(await (await handle.getFile()).arrayBuffer()));
      }
    };
    await walk(dir, "");
    return out;
  }, path);

const site = await serve();
const browser = await launch();
try {
  /** The editor on `bundle` in a browser of its own, and every address it asks for. */
  const open = async (bundle) => {
    const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1, acceptDownloads: true });
    const asked = [];
    context.on("request", (r) => asked.push(r.url()));
    const page = await context.newPage();
    page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
    page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
    await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=${bundle}`);
    await page.waitForFunction(() => window.scaena?.last() || document.querySelector("#status")?.textContent.startsWith("error"), null, { timeout: 120000 });
    const loaded = () => asked.some((u) => /\/scaena_history_bg-[^/]+\.wasm$/.test(u));
    return { context, page, loaded };
  };
  /** Put `text` in the editor and wait for it to compile. */
  const type = async (page, text) => {
    const before = await page.evaluate(() => window.scaena.trips().length);
    await page.evaluate((t) => window.scaena.type(t), text);
    await page.waitForFunction((n) => window.scaena.trips().length > n, before, { timeout: 60000, polling: 50 });
    return page.evaluate(() => window.scaena.last());
  };

  // A bundle that keeps no history saves without the history's module.
  {
    const { context, page, loaded } = await open("/docs/examples/revenue.deck.json");
    const source = await page.evaluate(() => window.scaena.source());
    check((await type(page, source.replace('"Revenue doubled"', '"Revenue grew"'))).valid, "an edit to a bundle that keeps no history");
    const saved = await page.evaluate(() => window.scaena.save());
    const files = Object.keys(await kept(page, "bundles/revenue"));
    check(saved?.where.kind === "opfs" && saved.recorded === false && !files.some((p) => p.startsWith("history/")), `saves, and keeps no history: ${files.length} files`);
    check(!loaded(), "without loading the history's module");
    await context.close();
  }

  const { context, page, loaded } = await open(`/${bundle}/deck.json`);
  check(!loaded(), "a bundle that keeps a history opens without the history's module");
  const started = Date.now() / 1000;
  const source = await page.evaluate(() => window.scaena.source());
  check((await type(page, source.replace('"Revenue doubled"', '"Revenue more than doubled"'))).valid, "the user types an edit");

  // The assistant renames the title.
  await page.click("#tab-assistant");
  await page.selectOption("#provider", "anthropic");
  await page.fill("#base", provider);
  await page.fill("#key", "anthropic-test-key");
  await page.waitForFunction(() => document.querySelector("#model").options.length > 0, null, { timeout: 30000 });
  const done = await page.evaluate(() => window.scaena.assistant.ask("Call the title the heading."));
  check(done.kind === "done" && done.stop === "end", `the assistant answers: ${JSON.stringify(done)}`);
  await page.waitForFunction(
    () => window.scaena.last()?.whole && window.scaena.source().includes("heading") && !document.querySelector("#code .cm-content[contenteditable=false]"),
    null,
    { timeout: 60000, polling: 100 },
  );
  const renamed = await page.evaluate(() => window.scaena.source());
  check(/^\s*heading text/m.test(renamed) && !/^\s*title text/m.test(renamed), "the source takes its rename");
  check((await type(page, renamed.replace("Q3 Review", "Third-quarter review"))).valid, "the user types again");
  check(!loaded(), "nothing has loaded the history's module yet");

  const saved = await page.evaluate(() => window.scaena.save());
  check(saved?.where.kind === "opfs" && saved.where.name === "revenue", `the save goes into the browser's storage: ${JSON.stringify(saved?.where)}`);
  check(loaded(), "and loads the history's module");
  const status = await page.locator("#status").textContent();
  check(saved?.recorded === true && status.endsWith("and recorded in its history"), `the page says it recorded the save: ${status}`);
  const files = await kept(page, "bundles/revenue");
  const history = log(Buffer.from(files["history/deck.loro"] ?? "", "base64"));
  const ours = history.slice(begun.length);
  check(
    JSON.stringify(said(ours)) === JSON.stringify(["user: edit", "agent:scripted: patch: rename_node", "user: save"]),
    `after scaena save's, the history holds the user's edit, the assistant's, and the save: ${JSON.stringify(said(ours))}`,
  );
  const now = Date.now() / 1000;
  check(ours.every((c) => c.timestamp >= Math.floor(started) && c.timestamp <= now + 1), `each stamped when it was made: ${ours.map((c) => c.timestamp)}`);
  check(said(history.slice(0, begun.length)).join() === said(begun).join(), "and what it held before, as it was");

  // Copied out of the browser's storage, the bundle's history holds its deck as saved.
  const out = "target/web-history/saved";
  for (const [path, b64] of Object.entries(files)) {
    await mkdir(dirname(join(out, path)), { recursive: true });
    await writeFile(join(out, path), Buffer.from(b64, "base64"));
  }
  scaena("save", out, "--keep-fonts");
  const after = log(await readFile(join(out, "history/deck.loro")));
  check(!after.some((c) => c.author === "fs"), `scaena save finds nothing to take in by fs: ${JSON.stringify(said(after.slice(history.length)))}`);

  // A download records too.
  const [download] = await Promise.all([page.waitForEvent("download"), page.click("#download")]);
  const zipped = join("target/web-history", "revenue.scaena");
  await writeFile(zipped, await readFile(await download.path()));
  const read = execFileSync("python3", [
    "-c",
    "import sys, zipfile; sys.stdout.buffer.write(zipfile.ZipFile(sys.argv[1]).read('history/deck.loro'))",
    zipped,
  ]);
  const downloaded = log(read);
  check(
    said(downloaded.slice(0, history.length)).join() === said(history).join() && said(downloaded.slice(history.length)).join() === "user: save",
    `a download's history is the one kept, and the save that subset its fonts: ${JSON.stringify(said(downloaded.slice(history.length)))}`,
  );
  await context.close();
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  site.close();
  scripted.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "the editor records what it saves in the bundle's history");
process.exit(failures.length ? 1 : 0);
