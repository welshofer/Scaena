// PLAN 2.54 check: export from the editor, in headless Chromium (serve.mjs), the CPU painting, held
// to what `scaena export` writes for the same bundle: the revenue example saved, with its manifest,
// so the editor holds every file the CLI reads (its fonts' licenses too), in target/web-export.
//
//   node web/export.mjs     (after `just web`; from the repository's root)
//
// `scaena` runs here by cargo, so it is built after the web build it carries the single-file page of.
// - Export… opens on a PNG of the state shown, its width the canvas's, and the note says its pixels.
//   axe-core finds nothing against WCAG 2.1 AA in it. Export downloads `<state>.png`: the size
//   asked, within SPEC §13.5 of `scaena export --format png --size` of that state. The CPU painter
//   runs in WASM SIMD in the browser and natively there (ADR-0004 finding 15).
// - At 960 pixels wide, the state is 960 × 540, and within §13.5 of the CLI's at that size.
// - The deck as a PDF, from the palette: the PDF's own module loads then, and not before, and the
//   PDF is the one `scaena export --format pdf` writes, byte for byte.
// - The deck as one HTML file, from Export…: the one `scaena export --format html` writes, byte for
//   byte. Opened from its address on disk with the network off, it plays.
// - A right click on a state in the strip offers Export it as a PNG…, which opens Export… on it.
// - A source that does not compile is not exported: the status says why, and nothing downloads.
// Exits 1 on any failure.
import { execFileSync } from "node:child_process";
import { createRequire } from "node:module";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { compare, decode, launch, serve } from "./serve.mjs";

const out = "target/web-export";
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const scaena = (...args) => execFileSync("cargo", ["run", "-q", "-p", "scaena-cli", "--locked", "--", ...args], { stdio: "inherit" });
const bundle = join(out, "revenue");
scaena("save", "docs/examples/revenue.deck.json", "--to", bundle, "--keep-fonts");

const server = await serve();
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};
const axe = createRequire(import.meta.url).resolve("axe-core/axe.min.js");

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1, acceptDownloads: true });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  const fetched = [];
  context.on("request", (r) => fetched.push(r.url()));
  await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&bundle=/${bundle}/`);
  await page.waitForFunction(() => window.scaena?.last()?.valid, null, { timeout: 120000 });
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  /** What `act` downloads: its name and bytes. */
  const downloaded = async (act) => {
    const [download] = await Promise.all([page.waitForEvent("download", { timeout: 120000 }), act()]);
    return { name: download.suggestedFilename(), bytes: readFileSync(await download.path()) };
  };
  const state = await page.evaluate(() => window.scaena.last().states[window.scaena.shown()][0]);

  // Export…, on a PNG of the state shown at the canvas's width.
  await page.click("#export");
  await page.waitForFunction(() => document.querySelector("#exporting").open, null, { timeout: 10000 });
  const opened = await page.evaluate(() => ({
    as: document.querySelector("#exporting input[name=as]:checked").value,
    width: document.querySelector("#exporting-width").value,
    note: document.querySelector("#exporting-note").textContent,
    focus: document.activeElement?.id,
  }));
  check(
    opened.as === "png" && opened.width === "1920" && opened.note.includes("1920 × 1080 pixels") && opened.focus === "exporting-width",
    `Export… opens on a PNG of the state shown, 1920 wide, its pixels said (${JSON.stringify(opened)})`,
  );
  await page.addScriptTag({ path: axe });
  const violations = await page.evaluate(async () => (await window.axe.run(document.querySelector("#exporting"), { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] })).violations);
  check(violations.length === 0, `axe finds nothing in Export…${violations.length ? `: ${violations.map((v) => `${v.id} (${v.nodes.length})`).join(", ")}` : ""}`);

  /** The CLI's PNG of `state` at `size`. */
  const cliPng = (size) => {
    const dir = join(out, `png-${size}`);
    scaena("export", bundle, "--format", "png", "--states", state, "--size", size, "--out", dir);
    return decode(readFileSync(join(dir, `${state}.png`)));
  };
  const big = await downloaded(() => page.click("#exporting-go"));
  const ours = decode(big.bytes);
  const theirs = cliPng("1920x1080");
  const same = compare(theirs, ours);
  check(big.name === `${state}.png` && ours.width === 1920 && ours.height === 1080, `Export downloads ${big.name}, ${ours.width} × ${ours.height}`);
  check(same.passes, `the PNG is within §13.5 of the CLI's: ${same.said}${Buffer.compare(theirs.rgba, ours.rgba) === 0 ? " (the same pixels)" : ""}`);
  check((await status()).startsWith(`downloaded ${state}.png`), `the status says what was downloaded: ${await status()}`);

  await page.click("#export");
  await page.fill("#exporting-width", "960");
  const note = await page.evaluate(() => document.querySelector("#exporting-note").textContent);
  check(note.includes("960 × 540 pixels"), `the note follows the width: ${note}`);
  const small = decode((await downloaded(() => page.click("#exporting-go"))).bytes);
  const narrow = compare(cliPng("960x540"), small);
  check(small.width === 960 && small.height === 540 && narrow.passes, `at 960 wide, ${small.width} × ${small.height}, within §13.5 of the CLI's: ${narrow.said}`);

  // The deck as a PDF, from the palette: its module loads then.
  const pdfModule = () => fetched.filter((u) => /scaena_pdf_bg[^/]*\.wasm$/.test(u)).length;
  check(pdfModule() === 0, "the PDF's module is not loaded before a PDF is asked for");
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+k");
  await page.keyboard.type("export pdf");
  const listed = await page.evaluate(() => window.scaena.commands.shown());
  check(listed[0] === "Export the deck as a PDF", `the palette finds it: ${JSON.stringify(listed)}`);
  const pdf = await downloaded(() => page.keyboard.press("Enter"));
  scaena("export", bundle, "--format", "pdf", "--out", join(out, "cli.pdf"));
  const cliPdf = readFileSync(join(out, "cli.pdf"));
  check(pdfModule() === 1, `the PDF's module loads for it, once (${pdfModule()})`);
  check(pdf.name === "revenue.pdf" && pdf.bytes.equals(cliPdf), `${pdf.name}, ${pdf.bytes.length} B, is the CLI's PDF byte for byte (${cliPdf.length} B)`);

  // The deck as one HTML file, from Export….
  await page.click("#export");
  await page.check("#exporting input[name=as][value=html]");
  const hidden = await page.evaluate(() => document.querySelector("#exporting-size").hidden);
  check(hidden, "the width goes for a file of the deck");
  const html = await downloaded(() => page.click("#exporting-go"));
  scaena("export", bundle, "--format", "html", "--out", join(out, "cli.html"));
  const cliHtml = readFileSync(join(out, "cli.html"));
  check(html.name === "revenue.html" && html.bytes.equals(cliHtml), `${html.name}, ${html.bytes.length} B, is the CLI's single file byte for byte (${cliHtml.length} B)`);
  const file = join(out, "editor.html");
  writeFileSync(file, html.bytes);
  const offline = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  await offline.setOffline(true);
  const played = await offline.newPage();
  played.on("pageerror", (e) => failures.push(`file: ${e.message}`));
  await played.goto(`${pathToFileURL(resolve(file)).href}?painter=cpu`);
  const plays = await played
    .waitForFunction(() => window.scaena?.at?.(), null, { timeout: 120000 })
    .then(() => played.evaluate(() => window.scaena.states))
    .catch((e) => String(e));
  check(JSON.stringify(plays) === '["intro","revenue","mix","close"]', `the file plays offline: ${JSON.stringify(plays)}`);
  await offline.close();

  // A right click on a state in the strip.
  const li = page.locator(`#strip li[data-state="${state}"]`);
  await li.click({ button: "right" });
  await page.waitForFunction(() => document.querySelector(".context-menu"), null, { timeout: 10000 });
  const offered = await page.evaluate(() => [...document.querySelectorAll(".context-menu [role=menuitem] span")].map((s) => s.textContent));
  check(offered.includes("Export it as a PNG…"), `a state's menu offers Export it as a PNG…: ${JSON.stringify(offered)}`);
  await page.locator(".context-menu [role=menuitem]", { hasText: "Export it as a PNG" }).click();
  const fromMenu = await page.evaluate(() => document.querySelector("#exporting").open && document.querySelector("#exporting input[name=as]:checked").value);
  check(fromMenu === "png", `it opens Export… on a PNG (${fromMenu})`);
  await page.keyboard.press("Escape");

  // A source that does not compile is not exported.
  const source = await page.evaluate(() => window.scaena.source());
  await page.evaluate(() => window.scaena.type(`${window.scaena.source()}\nstate broken {`));
  await page.waitForFunction(() => window.scaena.last() && !window.scaena.last().valid, null, { timeout: 30000 }).catch(() => {});
  let downloads = 0;
  page.on("download", () => downloads++);
  const refused = await page.evaluate(() => window.scaena.exportAs("pdf"));
  check(refused === undefined && downloads === 0 && (await status()).startsWith("not exported: the source does not compile"), `a source that does not compile is not exported: ${await status()}`);
  await page.evaluate((s) => window.scaena.type(s), source);
} finally {
  await browser.close();
  server.close();
}

if (failures.length) {
  console.error(`\n${failures.length} failed:\n${failures.map((f) => `  ${f}`).join("\n")}`);
  process.exit(1);
}
console.log("\nall export checks pass");
