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
//   copied into the browser's storage and edits as it was. A damaged font stops a download,
//   which names it, and once it is whole again the next download goes through.
// - A folder (any directory handle; here one in the browser's storage) opens and saves in
//   place: the save names its fonts by their content and removes the old names. Notes typed, then
//   ⌘S from their field: the save holds them.
// - A file that is no bundle, opened, is refused by its name, and the bundle open stays open, on
//   its own canvas, and saves.
// - Kept in this browser… lists the decks the browser keeps (PLAN 2.124), and axe-core finds
//   nothing in it against WCAG 2.1 AA: one is renamed in its row, every file of it under the new
//   name and none under the old; a name another deck has, or one with a slash, is refused, saying
//   why; one is deleted, asked first, and is gone. The deck open here is marked, is not deleted,
//   and renamed, goes on where it is kept now: the address names it, and a save writes there.
//   Escape in a name leaves it as it was, the list open.
// Exits 1 on any failure.
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { launch, serve } from "./serve.mjs";

const axe = createRequire(import.meta.url).resolve("axe-core/axe.min.js");

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
  /** What the page asked first, as `confirm` asks it. */
  const asked = [];
  /** Whether the next question is declined. */
  let declining = false;
  page.on("dialog", (d) => {
    asked.push(d.message());
    void (declining ? d.dismiss() : d.accept());
  });
  /** The editor once it has compiled and linted the bundle named `name`, if any. */
  const ready = async (name) => {
    await page.waitForFunction(
      (name) =>
        (window.scaena?.last() && (!name || window.scaena.where().name === name)) ||
        document.querySelector("#status")?.textContent.startsWith("Error"),
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
  check(refused === undefined && said.includes("Not saved: the source does not compile"), `a source that does not compile is not saved: ${said}`);
  await type(edited);

  const saved = await page.evaluate(() => window.scaena.save());
  check(saved?.where.kind === "opfs" && saved.where.name === "revenue", `the save goes into the browser's storage as revenue: ${JSON.stringify(saved?.where)}`);
  check(new URL(page.url()).searchParams.get("bundle") === "opfs:revenue", `the address names it: ${page.url()}`);
  const kept = await listing(page, "bundles/revenue");
  const fonts = Object.keys(kept).filter((p) => p.startsWith("fonts/"));
  // Each family's font, and its italic's (PLAN 2.40).
  check(fonts.length === 6 && fonts.every((p) => /^fonts\/[A-Za-z]+(-Italic)?-[0-9a-f]{16}\.ttf$/.test(p)) && fonts.filter((p) => p.includes("-Italic-")).length === 3, `its fonts are named by their content: ${fonts}`);
  check(["deck.json", "manifest.json", "data/q3-revenue.csv", "themes/dusk.theme.json"].every((p) => p in kept), `it holds its deck, manifest, data, and theme: ${Object.keys(kept)}`);
  check((await listing(page, "bundles/revenue", "deck.json")).includes("Revenue more than doubled"), "the edit is in its deck");
  const after = await page.evaluate(() => window.scaena.source());
  check(fonts.every((p) => after.includes(`"${p}"`)) && !after.includes("Inter-VF.ttf"), "the source names the fonts as the save named them");
  check(!(await where()).dirty, "saved, nothing is changed");

  await page.reload();
  await ready("revenue");
  // A save renames the fonts in the source as it was typed; reloaded, the page prints the deck
  // as canonical source, which wraps a line the new names made long. The same words, in order.
  const words = (text) => text.split(/\s+/).join(" ");
  const reopened = await page.evaluate(() => window.scaena.source());
  check(words(reopened) === words(after), "reloaded, the page opens it from the browser's storage as it was saved");

  const player = await context.newPage();
  await player.goto(`${server.origin}/web/dist/?painter=cpu&bundle=opfs:revenue`);
  await player.waitForFunction(() => window.scaena?.at || document.querySelector("#status")?.textContent.startsWith("Error"), null, { timeout: 120000 });
  const played = await player.evaluate(() => window.scaena?.states ?? document.querySelector("#status").textContent);
  check(Array.isArray(played) && played.length === 4, `the player plays it from there: ${played}`);
  await player.close();

  // Play opens the player on the slide shown (lift-up FUNC-2), and on the deck as the editor has
  // it: with changes not saved, it asks to save first, and saves; declined, it plays the deck as
  // last saved.
  const playing = async () => {
    // The state shown as Play is pressed: the preview follows the cursor, so an edit typed shows the
    // state it is in.
    const shown = await page.evaluate(() => window.scaena.last().states[window.scaena.shown()][0]);
    const [opened] = await Promise.all([context.waitForEvent("page"), page.click("#play")]);
    await opened.waitForFunction(() => window.scaena?.at || document.querySelector("#status")?.textContent.startsWith("Error"), null, { timeout: 120000 });
    const at = {
      shown,
      state: new URL(opened.url()).searchParams.get("state"),
      index: await opened.evaluate(() => window.scaena?.at?.().index),
      states: await opened.evaluate(() => window.scaena?.states),
    };
    await opened.close();
    return { ...at, on: at.state === shown && at.states?.[at.index] === shown };
  };
  await page.locator('#strip li[data-state="mix"]').click();
  await page.waitForFunction(() => window.scaena.shown() === 2 && window.scaena.at().index === 2, null, { timeout: 30000 });
  const onMix = await playing();
  check(onMix.on && onMix.state === "mix", `Play opens the player on the slide shown: ${JSON.stringify(onMix)}`);
  check(asked.length === 0, "with nothing to save, nothing is asked");
  const tripled = reopened.replace("Revenue more than doubled", "Revenue tripled");
  await type(tripled);
  check((await where()).dirty, "an edit not saved");
  const onEdited = await playing();
  check(asked.length === 1 && asked[0].includes("Save it first?"), `Play on a deck with changes not saved asks to save it first: ${asked}`);
  check(!(await where()).dirty && (await listing(page, "bundles/revenue", "deck.json")).includes("Revenue tripled"), "saved, so the player plays the deck as the editor has it");
  check(onEdited.on, `on the slide shown: ${JSON.stringify(onEdited)}`);
  await type(reopened);
  check((await where()).dirty, "an edit not saved again");
  declining = true;
  const declined = await playing();
  declining = false;
  check(asked.length === 2 && (await where()).dirty && (await listing(page, "bundles/revenue", "deck.json")).includes("Revenue tripled"), "declined, nothing is saved, and the player plays the deck as last saved");
  check(declined.on, `on the slide shown: ${JSON.stringify(declined)}`);
  await page.evaluate(() => window.scaena.save());
  check(!(await where()).dirty && (await listing(page, "bundles/revenue", "deck.json")).includes("Revenue more than doubled"), "saved again as it was");
  await page.locator('#strip li[data-state="intro"]').click();
  await page.waitForFunction(() => window.scaena.shown() === 0 && window.scaena.at().index === 0, null, { timeout: 30000 });

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
  check(/its fonts cut to what it draws, \d+ KB to \d+ KB/.test(status), `the page says how far its fonts were subset: ${status}`);

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
  check(subset.length === 6 && subset.every((p) => !fonts.includes(p)), `its fonts are the subsetter's, named by their content: ${subset}`);

  // A damaged font stops the subsetter's module, a panic in it a trap the worker catches: the
  // download says which font, and with the font whole again the next one goes through (PLAN
  // 2.25). Its `maxp` table is one byte long, where skera expects to read it.
  {
    const inter = subset.find((p) => p.includes("Inter"));
    const whole = await readFile("docs/examples/fonts/Inter-VF.ttf");
    const damaged = Buffer.from(whole);
    for (let i = 0; i < damaged.readUInt16BE(4); i++) {
      const record = 12 + 16 * i;
      if (damaged.toString("latin1", record, record + 4) === "maxp") damaged.writeUInt32BE(1, record + 12);
    }
    const asWas = await page.evaluate(() => window.scaena.source());
    // Dropped under its name, the font takes its place; the source is put back as it was.
    const put = (bytes) =>
      page.evaluate(
        async ([name, b64, source]) => {
          await window.scaena.drop(name, Uint8Array.from(atob(b64), (c) => c.charCodeAt(0)).buffer);
          window.scaena.type(source);
        },
        [inter.split("/").pop(), bytes.toString("base64"), asWas],
      );
    await put(damaged);
    await page.evaluate(() => window.scaena.download());
    const refused = await page.locator("#status").textContent();
    check(refused.startsWith("Not downloaded") && refused.includes(inter) && refused.includes("damaged"), `a damaged font stops a download, which names it: ${refused}`);
    await put(whole);
    const [again] = await Promise.all([page.waitForEvent("download"), page.click("#download")]);
    check(again.suggestedFilename() === "downloaded.scaena", `with the font whole again, the next download goes through: ${await page.locator("#status").textContent()}`);
  }

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
  check(inPlace?.renamed.length === 6, `a save names its fonts by their content: ${JSON.stringify(inPlace?.renamed)}`);
  check(!("fonts/Inter-VF.ttf" in plain) && Object.keys(plain).some((p) => /^fonts\/Inter-[0-9a-f]{16}\.ttf$/.test(p)), `and removes the old names: ${Object.keys(plain)}`);

  // Words typed in the presenter notes, then ⌘S with the caret still in the field: the save holds
  // them, as leaving the field takes them, and nothing is left to save.
  const notes = page.locator('textarea[data-prop="notes"]').first();
  await notes.waitFor({ timeout: 30000 });
  await notes.click();
  await page.keyboard.press("End");
  await page.keyboard.type(" Smile at the board.");
  await page.keyboard.press("Control+s");
  let noted = false;
  for (const end = Date.now() + 30000; !noted && Date.now() < end; await page.waitForTimeout(200))
    noted = (await listing(page, "plain", "deck.json")).includes("Smile at the board.");
  check(noted, "notes typed, then ⌘S from the field: the save holds them");
  await page.waitForFunction(() => !window.scaena.where().dirty, null, { timeout: 30000 }).catch(() => {});
  const focused = await page.evaluate(() => document.activeElement?.id);
  check(!(await where()).dirty && focused === "look-notes", `nothing is left to save, and the caret stays in the field: ${JSON.stringify(await where())}, ${focused}`);

  // A file that is no bundle: refused by its name, and the bundle open stays open, in use.
  await page.evaluate(() => (document.querySelector("#stage").kept = true));
  await page.locator("#open-file").setInputFiles({ name: "Board deck.scaena", mimeType: "application/octet-stream", buffer: Buffer.from("Not a zip, just words.\n".repeat(20)) });
  await page.waitForFunction(() => document.querySelector("#status").textContent.includes("can't be opened"), null, { timeout: 60000 }).catch(() => {});
  const why = await page.locator("#status").textContent();
  check(why.startsWith("Board deck.scaena can't be opened: it isn't a Scaena file, or it is damaged"), `a file that is no bundle is refused by its name: ${why}`);
  check((await where()).name === "plain" && (await page.evaluate(() => document.querySelector("#stage").kept === true)), "the bundle open stays open, on its own canvas");
  const still = await page.evaluate(() => Promise.race([window.scaena.save().then((d) => d?.where.name), new Promise((r) => setTimeout(() => r("no answer"), 20000))]));
  check(still === "plain", `and saves: ${still}`);

  // Kept in this browser…: each deck opened, renamed, or deleted from the list (PLAN 2.124).
  const keeps = () => page.evaluate(async () => {
    const names = [];
    for await (const [name, handle] of (await (await navigator.storage.getDirectory()).getDirectoryHandle("bundles")).entries()) if (handle.kind === "directory") names.push(name);
    return names.sort();
  });
  const listed = () => page.locator("#keeping li").evaluateAll((rows) => rows.map((li) => li.dataset.name));
  const row = (name) => page.locator(`#keeping li[data-name="${name}"]`);
  const told = () => page.evaluate(() => document.querySelector("#keeping [data-said]").textContent);
  /** Rename `from` to `to` in its row, as a person types it, then Enter. */
  const renamed = async (from, to) => {
    await page.evaluate(() => (document.querySelector("#keeping [data-said]").textContent = ""));
    await row(from).locator("[data-rename]").click();
    await page.keyboard.type(to);
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => document.querySelector("#keeping [data-said]").textContent, null, { timeout: 30000 }).catch(() => {});
  };
  await page.click("#kept");
  await page.waitForFunction(() => document.querySelector("#keeping").open && document.querySelectorAll("#keeping li").length, null, { timeout: 10000 }).catch(() => {});
  check(JSON.stringify(await listed()) === JSON.stringify(["downloaded", "revenue"]) && JSON.stringify(await keeps()) === JSON.stringify(["downloaded", "revenue"]), `Kept in this browser lists the decks it keeps: ${await listed()}`);
  check(await page.evaluate(() => document.querySelector("#keeping").contains(document.activeElement)), "the focus in the list");
  await page.addScriptTag({ path: axe });
  const found = await page.evaluate(async () => {
    const result = await window.axe.run(document, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  check(!found.length, `axe finds nothing against WCAG 2.1 AA with the list open${found.length ? `:\n    ${found.join("\n    ")}` : ""}`);
  await renamed("revenue", "Q3 board");
  check(JSON.stringify(await keeps()) === JSON.stringify(["Q3 board", "downloaded"]) && JSON.stringify(await listed()) === JSON.stringify(["Q3 board", "downloaded"]), `renamed in its row, it is kept under its new name alone: ${await keeps()} · ${await told()}`);
  const moved = await listing(page, "bundles/Q3 board");
  check(["deck.json", "manifest.json", "data/q3-revenue.csv"].every((p) => p in moved) && Object.keys(moved).filter((p) => p.startsWith("fonts/")).length === 6, `every file of it goes with it: ${Object.keys(moved)}`);
  await renamed("Q3 board", "downloaded");
  check((await told()).startsWith("Not renamed") && (await told()).includes("downloaded already") && JSON.stringify(await keeps()) === JSON.stringify(["Q3 board", "downloaded"]), `a name another deck has is refused: ${await told()}`);
  await renamed("Q3 board", "Q3/board");
  check((await told()).startsWith("Not renamed") && (await told()).includes("no slash") && JSON.stringify(await keeps()) === JSON.stringify(["Q3 board", "downloaded"]), `and so is one with a slash: ${await told()}`);
  await row("Q3 board").locator("[data-rename]").click();
  await page.keyboard.type("nothing");
  await page.keyboard.press("Escape");
  await page.waitForTimeout(300);
  check((await page.evaluate(() => document.querySelector("#keeping").open)) && JSON.stringify(await keeps()) === JSON.stringify(["Q3 board", "downloaded"]), "Escape in a name leaves it as it was, the list open");
  asked.length = 0;
  await row("downloaded").locator("[data-delete]").click();
  await page.waitForFunction(() => !document.querySelector('#keeping li[data-name="downloaded"]'), null, { timeout: 30000 }).catch(() => {});
  check(asked.length === 1 && asked[0].includes("Delete “downloaded”"), `Delete asks first: ${asked}`);
  check(JSON.stringify(await keeps()) === JSON.stringify(["Q3 board"]) && JSON.stringify(await listed()) === JSON.stringify(["Q3 board"]), `and the deck is gone: ${await keeps()} · ${await told()}`);
  // The deck open here: opened from the list, marked, kept from deletion, and renamed where it is.
  await row("Q3 board").locator("[data-open]").click();
  await ready("Q3 board");
  await page.click("#kept");
  await page.waitForFunction(() => document.querySelector("#keeping").open, null, { timeout: 10000 }).catch(() => {});
  const here = await page.evaluate(() => {
    const li = document.querySelector('#keeping li[data-name="Q3 board"]');
    return { marked: li?.querySelector(".here")?.textContent, deletable: !li?.querySelector("[data-delete]").disabled };
  });
  check(here.marked === "open here" && !here.deletable, `the deck open here is marked, and not deleted: ${JSON.stringify(here)}`);
  await renamed("Q3 board", "Board");
  check(JSON.stringify(await keeps()) === JSON.stringify(["Board"]) && (await where()).where?.name === "Board" && new URL(page.url()).searchParams.get("bundle") === "opfs:Board", `renamed, the deck open goes on where it is kept now: ${JSON.stringify((await where()).where)}, ${page.url()}`);
  await page.keyboard.press("Escape");
  await type((await page.evaluate(() => window.scaena.source())).replace("Revenue more than doubled", "Revenue tripled"));
  check((await page.evaluate(() => window.scaena.save()))?.where.name === "Board" && (await listing(page, "bundles/Board", "deck.json")).includes("Revenue tripled"), "and a save writes there");
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "the editor opens, saves, downloads, and takes dropped files");
process.exit(failures.length ? 1 : 0);
