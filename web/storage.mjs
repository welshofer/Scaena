// PLAN 2.4 storage check: the editor in headless Chromium (serve.mjs) opens bundles, saves
// them, downloads them, and takes dropped files. The CPU paints, as in web/editor.mjs.
//
//   node web/storage.mjs     (after `just web`; from the repository's root)
//
// - The revenue example, opened from its URL, edited, and saved, goes into the browser's
//   storage (the origin-private file system) as `revenue`: fonts named by their content, the
//   edit in its deck, the page's address naming it, and the source naming the fonts as the
//   save named them. Reloaded, the page opens it from there. The player plays it from there.
// - A source that does not compile is not saved.
// - A PNG dropped on the source joins the bundle as `assets/<sha256>.png`, its path where it
//   was dropped; an image node that shows it compiles and draws, and a save keeps it.
// - Download .scaena is a zip whose fonts the subsetter's own module subset. Opened, it is
//   copied into the browser's storage and edits as it was.
// - A folder (any directory handle; here one in the browser's storage) opens and saves in
//   place: the save names its fonts by their content and removes the old names.
// Exits 1 on any failure.
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { launch, serve } from "./serve.mjs";

const server = await serve();
const editor = (bundle) => `${server.origin}/web/dist/editor.html?painter=cpu&bundle=${bundle}`;
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

/** Every file under the directory `path` names in the origin-private file system, by its path
 * inside it, with its size; or the text of one file in it. */
const listing = (page, path, file) =>
  page.evaluate(
    async ([path, file]) => {
      let dir = await navigator.storage.getDirectory();
      for (const part of path.split("/")) dir = await dir.getDirectoryHandle(part);
      if (file) {
        let at = dir;
        const parts = file.split("/");
        const name = parts.pop();
        for (const part of parts) at = await at.getDirectoryHandle(part);
        return (await (await at.getFileHandle(name)).getFile()).text();
      }
      const out = {};
      const walk = async (d, prefix) => {
        for await (const [name, handle] of d.entries()) {
          if (handle.kind === "directory") await walk(handle, `${prefix}${name}/`);
          else out[prefix + name] = (await handle.getFile()).size;
        }
      };
      await walk(dir, "");
      return out;
    },
    [path, file],
  );

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1, acceptDownloads: true });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  page.on("dialog", (d) => d.accept());
  /** The editor once it has compiled and linted the bundle named `name`, if any. */
  const ready = async (name) => {
    await page.waitForFunction(
      (name) =>
        (window.scaena?.last() && (!name || window.scaena.where().name === name)) ||
        document.querySelector("#status")?.textContent.startsWith("error"),
      name,
      { timeout: 120000 },
    );
    const status = await page.evaluate(() => (window.scaena?.last() ? "" : document.querySelector("#status").textContent));
    if (status) throw new Error(status);
  };
  const type = async (text) => {
    const before = await page.evaluate(() => window.scaena.trips().length);
    await page.evaluate((t) => window.scaena.type(t), text);
    await page.waitForFunction((n) => window.scaena.trips().length > n, before, { timeout: 60000, polling: 50 });
    return page.evaluate(() => window.scaena.last());
  };
  const where = () => page.evaluate(() => window.scaena.where());

  // The revenue example, from its URL, edited and saved into the browser's storage.
  await page.goto(editor("/docs/examples/revenue.deck.json"));
  await ready();
  check((await where()).where === undefined && (await where()).name === "revenue", "a bundle from a URL is kept nowhere yet, named `revenue`");
  const source = await page.evaluate(() => window.scaena.source());
  const edited = source.replace('"Revenue doubled"', '"Revenue more than doubled"');
  check((await type(edited)).valid && (await where()).dirty, "an edit marks the bundle changed");

  const broken = edited.replace("state close layout:title", "state close layout:");
  await type(broken);
  const refused = await page.evaluate(() => window.scaena.save());
  const said = await page.locator("#status").textContent();
  check(refused === undefined && said.includes("not saved: the source does not compile"), `a source that does not compile is not saved: ${said}`);
  await type(edited);

  const saved = await page.evaluate(() => window.scaena.save());
  check(saved?.where.kind === "opfs" && saved.where.name === "revenue", `the save goes into the browser's storage as revenue: ${JSON.stringify(saved?.where)}`);
  check(new URL(page.url()).searchParams.get("bundle") === "opfs:revenue", `the address names it: ${page.url()}`);
  const kept = await listing(page, "bundles/revenue");
  const fonts = Object.keys(kept).filter((p) => p.startsWith("fonts/"));
  check(fonts.length === 3 && fonts.every((p) => /^fonts\/[A-Za-z]+-[0-9a-f]{16}\.ttf$/.test(p)), `its fonts are named by their content: ${fonts}`);
  check(["deck.json", "manifest.json", "data/q3-revenue.csv", "themes/dusk.theme.json"].every((p) => p in kept), `it holds its deck, manifest, data, and theme: ${Object.keys(kept)}`);
  check((await listing(page, "bundles/revenue", "deck.json")).includes("Revenue more than doubled"), "the edit is in its deck");
  const after = await page.evaluate(() => window.scaena.source());
  check(fonts.every((p) => after.includes(`"${p}"`)) && !after.includes("Inter-VF.ttf"), "the source names the fonts as the save named them");
  check(!(await where()).dirty, "saved, nothing is changed");

  await page.reload();
  await ready("revenue");
  check((await page.evaluate(() => window.scaena.source())) === after, "reloaded, the page opens it from the browser's storage as it was saved");

  const player = await context.newPage();
  await player.goto(`${server.origin}/web/dist/?painter=cpu&bundle=opfs:revenue`);
  await player.waitForFunction(() => window.scaena?.at || document.querySelector("#status")?.textContent.startsWith("error"), null, { timeout: 120000 });
  const played = await player.evaluate(() => window.scaena?.states ?? document.querySelector("#status").textContent);
  check(Array.isArray(played) && played.length === 4, `the player plays it from there: ${played}`);
  await player.close();

  // A PNG dropped where an image node's source goes.
  const png = await readFile("tests/fixtures/torture.scaena/assets/test-card.png");
  const asset = `assets/${createHash("sha256").update(png).digest("hex")}.png`;
  const node = '  card image  at:in(canvas) alt:"" z:-50\n';
  const withNode = after.replace("state intro layout:title hold:4s\n", `state intro layout:title hold:4s\n${node}`);
  const at = withNode.indexOf(node) + "  card image ".length;
  check((await type(withNode)).valid === false, "an image node with no source does not validate");
  await page.evaluate((at) => window.scaena.cursor(at), at);
  await page.waitForFunction(() => window.scaena.shown() === 0 && window.scaena.at().index === 0, null, { timeout: 15000 }).catch(() => {});
  const shownBefore = await page.locator("#stage").screenshot();
  const before = await page.evaluate(() => window.scaena.trips().length);
  const dropped = await page.evaluate(
    async ([b64]) => window.scaena.drop("Test Card.png", Uint8Array.from(atob(b64), (c) => c.charCodeAt(0)).buffer),
    [png.toString("base64")],
  );
  check(dropped?.[0] === asset, `a dropped PNG joins the bundle named by its SHA-256: ${dropped}`);
  await page.waitForFunction((n) => window.scaena.trips().length > n && window.scaena.last().valid, before, { timeout: 60000 }).catch(() => {});
  const withImage = await page.evaluate(() => window.scaena.source());
  check(withImage.includes(`card image "${asset}" at:in(canvas)`), "its path goes where it was dropped");
  check((await page.evaluate(() => window.scaena.last().valid)) === true, "the image node that shows it compiles");
  check((await page.evaluate(() => window.scaena.at().index)) === 0 && !(await page.locator("#stage").screenshot()).equals(shownBefore), "the preview draws it");
  await page.evaluate(() => window.scaena.save());
  check(asset in (await listing(page, "bundles/revenue")), "a save keeps it");

  // Download .scaena: fonts subset by the subsetter's module.
  const [download] = await Promise.all([page.waitForEvent("download"), page.click("#download")]);
  const zip = await readFile(await download.path());
  check(download.suggestedFilename() === "revenue.scaena" && zip[0] === 0x50 && zip[1] === 0x4b, `a download is a .scaena zip: ${download.suggestedFilename()}, ${zip.length} bytes`);
  const status = await page.locator("#status").textContent();
  check(/fonts subset from \d+ KB to \d+ KB/.test(status), `the page says how far its fonts were subset: ${status}`);

  // The download, opened: copied into the browser's storage.
  await page.evaluate(
    async ([b64]) => window.scaena.open({ zip: Uint8Array.from(atob(b64), (c) => c.charCodeAt(0)).buffer, name: "downloaded" }),
    [zip.toString("base64")],
  );
  await ready("downloaded");
  const opened = await where();
  check(opened.where?.kind === "opfs" && opened.where.name === "downloaded", `an opened .scaena is kept in the browser's storage: ${JSON.stringify(opened.where)}`);
  check((await page.evaluate(() => window.scaena.source())).includes("Revenue more than doubled"), "it edits as it was saved");
  const subset = Object.keys(await listing(page, "bundles/downloaded")).filter((p) => p.startsWith("fonts/"));
  // The repository's example fonts are subset to the examples' text already, so a subset is
  // about their size; it is other bytes, under another name.
  check(subset.length === 3 && subset.every((p) => !fonts.includes(p)), `its fonts are the subsetter's, named by their content: ${subset}`);

  // A folder, saved in place: the revenue example's files as they are in the repository.
  const deck = JSON.parse(await readFile("docs/examples/revenue.deck.json", "utf8"));
  const files = [["deck.json", "revenue.deck.json"], [deck.theme, deck.theme], ...deck.fonts.map((f) => [f.file, f.file])];
  for (const source of Object.values(deck.data)) files.push([source.source, source.source]);
  await page.evaluate(async ([files]) => {
    const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle("plain", { create: true });
    for (const [path, from] of files) {
      const bytes = new Uint8Array(await (await fetch(`/docs/examples/${from}`)).arrayBuffer());
      let at = dir;
      const parts = path.split("/");
      const name = parts.pop();
      for (const part of parts) at = await at.getDirectoryHandle(part, { create: true });
      const out = await (await at.getFileHandle(name, { create: true })).createWritable();
      await out.write(bytes);
      await out.close();
    }
    await window.scaena.open({ folder: dir });
  }, [files]);
  await ready("plain");
  check((await where()).where?.kind === "folder", "a folder opens as a folder");
  const inPlace = await page.evaluate(() => window.scaena.save());
  const plain = await listing(page, "plain");
  check(inPlace?.renamed.length === 3, `a save names its fonts by their content: ${JSON.stringify(inPlace?.renamed)}`);
  check(!("fonts/Inter-VF.ttf" in plain) && Object.keys(plain).some((p) => /^fonts\/Inter-[0-9a-f]{16}\.ttf$/.test(p)), `and removes the old names: ${Object.keys(plain)}`);
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "the editor opens, saves, downloads, and takes dropped files");
process.exit(failures.length ? 1 : 0);
