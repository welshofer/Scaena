// Lift-up REL-4 check: the editor's recovery copy, in headless Chromium (serve.mjs), the CPU
// painting, as in web/storage.mjs.
//
//   node web/recovery.mjs     (after `just web`; from the repository's root)
//
// - An edit not saved is kept in the browser's storage, under `recovery/`, a couple of seconds
//   after it: the source as it stands, and when. Reloaded, the editor offers the changes: Recover
//   puts them back as one change, one undo, and says so; an undo back to what was saved takes the
//   copy away, and so does a save.
// - A picture dropped since the save is kept in the copy, and comes back with it, so the deck
//   validates again.
// - Discard leaves the deck as saved, and the copy is gone.
// - A copy older than the deck as saved is passed over, and gone.
// - The page hidden, the copy is kept at once, not when typing stops.
// - A new deck's changes are kept too, and offered when a deck of that title is made again after
//   the page went; another deck opened over it, its changes lost as agreed, takes its copy away.
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

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  // Leaving with changes not saved is confirmed, as is opening another deck over them.
  page.on("dialog", (d) => void d.accept());
  const ready = async (name) => {
    await page.waitForFunction(
      (name) => (window.scaena?.last() && (!name || window.scaena.where().name === name)) || document.querySelector("#status")?.textContent.startsWith("Error"),
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
  const source = () => page.evaluate(() => window.scaena.source());
  const where = () => page.evaluate(() => window.scaena.where());
  const status = () => page.locator("#status").textContent();
  /** The copies the browser keeps: each directory under `recovery/`, its files, and its record. A
   * save removes a copy while this reads it (`uncopied` polls as the save runs), so a directory
   * gone mid-walk is no copy, not an error. */
  const copies = () =>
    page.evaluate(async () => {
      const root = await navigator.storage.getDirectory();
      const all = await root.getDirectoryHandle("recovery").catch(() => null);
      if (!all) return [];
      const out = [];
      for await (const [name, dir] of all.entries()) {
        try {
          const files = [];
          const walk = async (d, prefix) => {
            for await (const [n, h] of d.entries()) {
              if (h.kind === "directory") await walk(h, `${prefix}${n}/`);
              else files.push(prefix + n);
            }
          };
          await walk(dir, "");
          const text = async (path) => (await (await dir.getFileHandle(path).catch(() => null))?.getFile())?.text();
          const record = await text("copy.json");
          out.push({ name, files: files.sort(), record: record ? JSON.parse(record) : null, source: await text("source.scn") });
        } catch (e) {
          if (e?.name !== "NotFoundError") throw e;
        }
      }
      return out;
    });
  /** Once one copy is kept, within `ms`: the copies. */
  const copied = async (ms = 10000) => {
    for (const end = Date.now() + ms; Date.now() < end; ) {
      const all = await copies();
      if (all.some((c) => c.record)) return all;
      await page.waitForTimeout(100);
    }
    return copies();
  };
  /** Whether no copy is kept, within `ms`. */
  const uncopied = async (ms = 10000) => {
    for (const end = Date.now() + ms; Date.now() < end; ) {
      if (!(await copies()).some((c) => c.record)) return true;
      await page.waitForTimeout(100);
    }
    return false;
  };
  const offered = () => page.locator("#recovering[open]");
  const undo = async () => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
  };

  // An edit not saved, kept a couple of seconds after it.
  await page.goto(editor("/docs/examples/revenue.deck.json"));
  await ready();
  check((await copies()).length === 0, "a deck opened holds no copy");
  const original = await source();
  const edited = original.replace('"Revenue doubled"', '"Revenue kept growing"');
  const started = Date.now();
  await type(edited);
  let kept = await copied();
  check(kept.length === 1 && kept[0].source?.includes("Revenue kept growing"), `an edit not saved is kept in the browser's storage: ${JSON.stringify(kept.map((c) => c.files))}`);
  check(kept[0]?.record && kept[0].record.at >= started - 1000 && kept[0].record.at <= Date.now() && kept[0].record.name === "revenue", `and when, and the deck's name: ${JSON.stringify(kept[0]?.record)}`);
  check(Date.now() - started < 8000, `within a few seconds: ${Date.now() - started} ms`);

  // Reloaded, the editor offers the changes: Recover puts them back.
  await page.reload();
  await ready();
  await offered().waitFor({ timeout: 10000 }).catch(() => {});
  check((await offered().count()) === 1, "reloaded, the editor offers the changes not saved");
  const said = await page.locator("#recovering [data-said]").textContent();
  check(said.includes("were not saved") && said.includes("Recover them, or discard them?"), `and says when they were made: ${said}`);
  check(!(await source()).includes("Revenue kept growing"), "the deck opens as saved meanwhile");
  await page.click("#recovering button[value=recover]");
  await page.waitForFunction(() => window.scaena.source().includes("Revenue kept growing"), null, { timeout: 30000 });
  check((await source()) === edited && (await where()).dirty, "Recover puts the changes back, as changes not saved");
  check((await status()).startsWith("Recovered the changes from"), `and says so: ${await status()}`);
  await undo();
  await page.waitForFunction((s) => window.scaena.source() === s, original, { timeout: 30000 });
  check((await source()) === original && !(await where()).dirty, "one undo takes them back");
  check(await uncopied(), "an undo back to what was saved takes the copy away");

  // A save takes it away.
  await type(edited);
  await copied();
  await page.evaluate(() => window.scaena.save());
  await page.waitForFunction(() => !window.scaena.where().dirty, null, { timeout: 30000 });
  check((await where()).where?.kind === "opfs" && (await uncopied()), "a save takes the copy away");

  // A picture dropped since the save is kept in the copy, and comes back with it.
  const png = await readFile("tests/fixtures/torture.scaena/assets/test-card.png");
  const asset = `assets/${createHash("sha256").update(png).digest("hex")}.png`;
  const saved = await source();
  const node = '  card image  at:in(canvas) alt:"" z:-50\n';
  const withNode = saved.replace("state intro layout:title hold:4s\n", `state intro layout:title hold:4s\n${node}`);
  const at = withNode.indexOf(node) + "  card image ".length;
  await type(withNode);
  const before = await page.evaluate(() => window.scaena.trips().length);
  await page.evaluate(
    async ([b64, at]) => window.scaena.drop("Test Card.png", Uint8Array.from(atob(b64), (c) => c.charCodeAt(0)).buffer, at),
    [png.toString("base64"), at],
  );
  await page.waitForFunction((n) => window.scaena.trips().length > n && window.scaena.last().valid, before, { timeout: 60000 }).catch(() => {});
  const withImage = await source();
  check(withImage.includes(`card image "${asset}"`) && (await page.evaluate(() => window.scaena.last().valid)), "a dropped PNG joins the bundle, and the deck validates");
  await page.evaluate(() => window.scaena.keepCopy());
  kept = await copies();
  check(kept.length === 1 && kept[0].files.includes(`files/${asset}`) && kept[0].record?.files.includes(asset), `the copy keeps the picture dropped since the save: ${JSON.stringify(kept.map((c) => c.files))}`);
  await page.reload();
  await ready("revenue");
  await offered().waitFor({ timeout: 10000 }).catch(() => {});
  check((await offered().count()) === 1 && (await page.evaluate(() => window.scaena.last().valid)), "reloaded, the deck as saved validates, and the changes are offered");
  await page.click("#recovering button[value=recover]");
  await page.waitForFunction((s) => window.scaena.source() === s, withImage, { timeout: 30000 });
  await page.waitForFunction(() => window.scaena.last()?.valid === true, null, { timeout: 30000 }).catch(() => {});
  check((await source()) === withImage && (await page.evaluate(() => window.scaena.last().valid)), "Recover puts the picture back with the source: the deck validates");
  check((await status()).includes("with 1 file"), `and says the file came with it: ${await status()}`);
  await page.evaluate(() => window.scaena.save());
  await page.waitForFunction(() => !window.scaena.where().dirty, null, { timeout: 30000 });
  check(await uncopied(), "saved, the copy is gone");

  // Discard leaves the deck as saved.
  const kept2 = await source();
  await type(kept2.replace("Revenue kept growing", "Revenue stalled"));
  await page.evaluate(() => window.scaena.keepCopy());
  await page.reload();
  await ready("revenue");
  await offered().waitFor({ timeout: 10000 }).catch(() => {});
  check((await offered().count()) === 1, "offered again");
  // The source as the deck opened, printed canonically, which wraps otherwise than it was typed.
  const asSaved = await source();
  await page.click("#recovering button[value=discard]");
  await page.waitForTimeout(500);
  check((await source()) === asSaved && !asSaved.includes("Revenue stalled") && !(await where()).dirty && (await uncopied()), "Discard leaves the deck as saved, and the copy is gone");

  // A copy older than the deck as saved is passed over.
  await page.evaluate(async () => {
    const key = window.scaena.copyKey();
    const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(key));
    const name = [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
    const root = await navigator.storage.getDirectory();
    const dir = await (await root.getDirectoryHandle("recovery", { create: true })).getDirectoryHandle(name, { create: true });
    const write = async (path, text) => {
      const out = await (await dir.getFileHandle(path, { create: true })).createWritable();
      await out.write(text);
      await out.close();
    };
    await write("source.scn", window.scaena.source().replace("Revenue kept growing", "Revenue of old"));
    await write("copy.json", JSON.stringify({ at: 1000, name: "revenue", files: [], removed: [] }));
  });
  await page.reload();
  await ready("revenue");
  await page.waitForTimeout(1000);
  check((await offered().count()) === 0 && (await uncopied(3000)), "a copy older than the deck as saved is passed over, and gone");

  // The page hidden, the copy is kept at once.
  await type(kept2.replace("Revenue kept growing", "Revenue hid"));
  await page.evaluate(() => {
    Object.defineProperty(document, "visibilityState", { value: "hidden", configurable: true });
    document.dispatchEvent(new Event("visibilitychange"));
  });
  kept = await copied(1200);
  check(kept.length === 1 && kept[0].source?.includes("Revenue hid"), "the page hidden, the copy is kept at once");
  await page.evaluate(() => {
    Object.defineProperty(document, "visibilityState", { value: "visible", configurable: true });
    document.dispatchEvent(new Event("visibilitychange"));
  });

  // A new deck's changes are kept too.
  await page.evaluate(() => window.scaena.open({ create: { theme: "Dusk", title: "Fresh" } }));
  await ready("fresh");
  check((await where()).where === undefined, "a new deck is kept nowhere");
  const fresh = await source();
  const titled = fresh.replace('"Fresh"', '"Fresh words"');
  check(titled !== fresh, "its title is in its source");
  await type(titled);
  await page.evaluate(() => window.scaena.keepCopy());
  const keptNew = (await copies()).find((c) => c.record?.name === "fresh");
  check(keptNew?.source?.includes("Fresh words"), `the new deck's changes are kept: ${JSON.stringify((await copies()).map((c) => c.record?.name))}`);
  // The page goes, as a crash or a tab closed takes it, and a deck of that title made again is
  // offered them.
  await page.reload();
  await ready();
  // The editor opened again lists it among the decks not saved; not now, it stays kept.
  await page.locator("#leftovers[open]").waitFor({ timeout: 30000 }).catch(() => {});
  const leftovers = await page.locator("#leftovers li span").allTextContents();
  check(leftovers.length === 1 && leftovers[0].startsWith("Fresh words, changed "), `the editor opened again lists the new deck not saved: ${leftovers.join(" | ")}`);
  await page.click("#leftovers button[value=later]");
  await page.evaluate(() => window.scaena.open({ create: { theme: "Dusk", title: "Fresh" } }));
  await ready("fresh");
  await offered().waitFor({ timeout: 10000 }).catch(() => {});
  check((await offered().count()) === 1, "a deck of that title made again is offered them");
  await page.click("#recovering button[value=recover]");
  await page.waitForFunction(() => window.scaena.source().includes("Fresh words"), null, { timeout: 30000 });
  check((await source()).includes("Fresh words") && (await where()).dirty, "and Recover puts them back");
  // Another deck opened over it, its changes lost as the person agreed: no copy keeps them, and a
  // deck of that title made again is offered nothing.
  await page.evaluate(() => window.scaena.keepCopy());
  check((await copies()).some((c) => c.record?.name === "fresh"), "kept again before it is closed");
  await page.evaluate((url) => window.scaena.open({ url }), `${server.origin}/docs/examples/revenue.deck.json`);
  await ready("revenue");
  check(!(await copies()).some((c) => c.record?.name === "fresh"), "opened over, its changes lost as agreed, its copy goes with them");
  await page.evaluate(() => window.scaena.open({ create: { theme: "Dusk", title: "Fresh" } }));
  await ready("fresh");
  await page.waitForTimeout(1000);
  check((await offered().count()) === 0, "and a deck of that title made again is offered nothing");
} finally {
  await browser.close();
  server.close();
}

if (failures.length) {
  console.error(`${failures.length} failure(s): ${failures.join("; ")}`);
  process.exit(1);
}
console.log("\nthe editor keeps a recovery copy of what is not saved, and offers it");
