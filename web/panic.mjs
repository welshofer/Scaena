// Lift-up STAB-2 check: the editor recovers from a panic in the engine, in headless Chromium
// (serve.mjs), the CPU painting, as in web/storage.mjs.
//
//   node web/panic.mjs     (after `just web`; from the repository's root)
//
// wasm32 does not unwind: a panic in the engine traps, and the session's borrow stays held, so
// every call after it fails, Save among them. The editor's module alone carries a panic asked
// for (`scaena.crash()`), which the walk asks for:
// - The revenue example, opened from its URL, edited, and a picture dropped on it; then the
//   panic. The worker stops, and the editor starts the engine again in a fresh worker from the
//   bundle's files as the old one held them, with the source as it stands: the status says so, in
//   plain words with the panic's, the state shown is shown again, the thumbnails are painted again,
//   and the edit and the picture are still there, as changes not saved.
// - Save then works: the deck goes into the browser's storage with the edit and the picture.
// - A second panic, after the save: the engine starts again kept where the deck is kept, and a
//   save writes there again.
// - A request made of the engine that stopped is refused, not left waiting.
// - The worker fails outside a message and holds one task 5 s (`scaena.die(5000)`): it lives, a
//   request made of it meanwhile is answered when the task ends, and nothing is started again.
// - The worker dies outside a message and answers nothing more, as one out of memory does
//   (`scaena.die()`, lift-up 10-24b STAB-2): a request waiting on it is refused within the 20 s it
//   is given to say anything, and the engine starts again from the deck as last saved, with the source as
//   it stands, the status naming the picture dropped since the save, which went with the worker.
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

/** Every file under the bundle `name` the browser keeps, by its path, with its size. */
const listing = (page, name) =>
  page.evaluate(async (name) => {
    let dir = await navigator.storage.getDirectory();
    for (const part of ["bundles", name]) dir = await dir.getDirectoryHandle(part);
    const out = {};
    const walk = async (d, prefix) => {
      for await (const [n, handle] of d.entries()) {
        if (handle.kind === "directory") await walk(handle, `${prefix}${n}/`);
        else out[prefix + n] = (await handle.getFile()).size;
      }
    };
    await walk(dir, "");
    return out;
  }, name);

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  // The panic asked for is said on the console by the engine's hook, as any panic is.
  page.on("console", (m) => m.type() === "error" && !m.text().includes("a panic asked for") && !m.text().includes("as asked, to test the editor's recovery") && failures.push(`console: ${m.text()}`));
  page.on("dialog", (d) => void d.accept());
  const ready = async () => {
    await page.waitForFunction(() => window.scaena?.last() || document.querySelector("#status")?.textContent.startsWith("Error"), null, { timeout: 120000 });
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
  const says = async (text) => {
    await page.waitForFunction((t) => document.querySelector("#status").textContent.includes(t), text, { timeout: 60000 }).catch(() => {});
    return (await status()).includes(text);
  };
  /** Once the strip has a thumbnail for each of `ids`, their digests. */
  const painted = async (ids) => {
    await page.waitForFunction((want) => want.every((id) => window.scaena.strip.digests()[id]), ids, { timeout: 60000, polling: 100 }).catch(() => {});
    return page.evaluate(() => window.scaena.strip.digests());
  };
  /** The engine panics: once the editor has started it again, the status. */
  const crash = async (n) => {
    await page.evaluate(() => window.scaena.crash());
    await page.waitForFunction((n) => window.scaena.restarts() === n, n, { timeout: 60000 }).catch(() => {});
    await says("was started again");
    return status();
  };

  await page.goto(editor("/docs/examples/revenue.deck.json"));
  await ready();
  const original = await source();
  const edited = original.replace('"Revenue doubled"', '"Revenue kept growing"');
  const png = await readFile("tests/fixtures/torture.scaena/assets/test-card.png");
  const asset = `assets/${createHash("sha256").update(png).digest("hex")}.png`;
  const node = '  card image  at:in(canvas) alt:"" z:-50\n';
  const withNode = edited.replace("state intro layout:title hold:4s\n", `state intro layout:title hold:4s\n${node}`);
  const at = withNode.indexOf(node) + "  card image ".length;
  await type(withNode);
  const before = await page.evaluate(() => window.scaena.trips().length);
  await page.evaluate(
    async ([b64, at]) => window.scaena.drop("Test Card.png", Uint8Array.from(atob(b64), (c) => c.charCodeAt(0)).buffer, at),
    [png.toString("base64"), at],
  );
  const lints = await page.evaluate(() => window.scaena.wholes().length);
  await page.waitForFunction((n) => window.scaena.trips().length > n && window.scaena.last().valid, before, { timeout: 60000 }).catch(() => {});
  const withImage = await source();
  check(withImage.includes(`card image "${asset}"`) && (await page.evaluate(() => window.scaena.last().valid)), "an edit, and a picture dropped: the deck validates");
  // Every state linted after the drop, and the thumbnails painted from that: what the engine
  // started again is held to.
  await page.waitForFunction((n) => window.scaena.wholes().length > n, lints, { timeout: 60000 }).catch(() => {});
  await page.evaluate(() => window.scaena.strip.painted());
  await page.locator('#strip li[data-state="mix"]').click();
  await page.waitForFunction(() => window.scaena.shown() === 2 && window.scaena.at().index === 2, null, { timeout: 30000 });
  await page.waitForTimeout(500);
  const thumbs = await painted(["intro", "revenue", "mix", "close"]);
  const shownBefore = await page.locator("#stage").screenshot();
  const linted = await page.evaluate(() => window.scaena.wholes().length);

  // The panic: the engine starts again, the deck as it stands.
  const said = await crash(1);
  check((await page.evaluate(() => window.scaena.restarts())) === 1, "a panic in the engine starts it again in a fresh worker");
  check(said.includes("The engine stopped on an error (a panic asked for, to test the editor's recovery)") && said.includes("nothing is lost. Save when you can."), `the status says so in plain words, with the panic's: ${said}`);
  check((await source()) === withImage && (await where()).dirty, "the source stands as it was, as changes not saved");
  await page.waitForFunction(() => window.scaena.shown() === 2 && window.scaena.at().index === 2, null, { timeout: 30000 }).catch(() => {});
  check((await page.evaluate(() => [window.scaena.shown(), window.scaena.at().index].join())) === "2,2", "the state shown is shown again");
  /** Whether the stage draws as it did before the panic, once it has, within 5 s. */
  const drawnAgain = async () => {
    for (const end = Date.now() + 5000; Date.now() < end; ) {
      if ((await page.locator("#stage").screenshot()).equals(shownBefore)) return true;
      await page.waitForTimeout(200);
    }
    return false;
  };
  check(await drawnAgain(), "and drawn as it was");
  const again = await painted(["intro", "revenue", "mix", "close"]);
  check(["intro", "revenue", "mix", "close"].every((id) => again[id] === thumbs[id]), "the thumbnails are painted again, the same drawings");
  await page.waitForFunction((n) => window.scaena.wholes().length > n, linted, { timeout: 60000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.wholes().length)) > linted && (await page.evaluate(() => window.scaena.last().valid)), "every state is linted again, and the deck validates: the picture came over");

  // Save works.
  const saved = await page.evaluate(() => window.scaena.save());
  check(saved?.where.kind === "opfs" && saved.where.name === "revenue" && !(await where()).dirty, `Save works after the panic: ${JSON.stringify(saved?.where)}`);
  const kept = await listing(page, "revenue");
  check(asset in kept && kept[asset] === png.length, "the picture dropped before the panic is saved with the deck");
  const deck = await page.evaluate(async () => {
    let dir = await navigator.storage.getDirectory();
    for (const part of ["bundles", "revenue"]) dir = await dir.getDirectoryHandle(part);
    return (await (await dir.getFileHandle("deck.json")).getFile()).text();
  });
  check(deck.includes("Revenue kept growing"), "and so is the edit");

  // A second panic, after the save: kept where the deck is kept.
  const afterSave = await source();
  await type(afterSave.replace("Revenue kept growing", "Revenue tripled"));
  const asked = page.evaluate(() => window.scaena.strip.paint().then(() => "painted", (e) => `refused: ${e.message}`));
  await crash(2);
  check((await page.evaluate(() => window.scaena.restarts())) === 2, "a second panic starts the engine again");
  const savedAgain = await page.evaluate(() => window.scaena.save());
  check(savedAgain?.where.kind === "opfs" && savedAgain.where.name === "revenue", `and a save writes where the deck is kept: ${JSON.stringify(savedAgain?.where)}`);
  check((await page.evaluate(async () => {
    let dir = await navigator.storage.getDirectory();
    for (const part of ["bundles", "revenue"]) dir = await dir.getDirectoryHandle(part);
    return (await (await dir.getFileHandle("deck.json")).getFile()).text();
  })).includes("Revenue tripled"), "with the edit made after the first panic");
  const answered = await asked;
  check(answered === "painted" || answered.startsWith("refused"), `a request in flight is answered or refused, not left waiting: ${answered}`);

  // The worker fails outside a message, then holds one task 5 s, as a large deck's whole lint can:
  // it lives, so a request made of it meanwhile is answered once the task ends, and nothing is
  // started again.
  const busy = await page.evaluate(async () => {
    window.scaena.die(5000);
    await new Promise((r) => setTimeout(r, 300));
    const asked = performance.now();
    const read = await window.scaena.reading().then(() => "read", (e) => `refused: ${e.message}`);
    return { read, after: Math.round(performance.now() - asked) };
  });
  check(busy.read === "read" && busy.after > 3000, `a worker busy 5 s after an error is alive: a request made meanwhile is answered when it can be, ${busy.after} ms on: ${busy.read}`);
  await page.waitForTimeout(500);
  check((await page.evaluate(() => window.scaena.restarts())) === 2, "and the engine is not started again");

  // The worker dies outside a message and answers nothing more, as one out of memory does (lift-up
  // 10-24b STAB-2): it hands nothing over, so the engine starts again from the deck as last saved,
  // with the source as it stands, and the picture dropped since the save is said to be lost. A
  // request waiting on the worker is refused, not left waiting.
  const map = await readFile("tests/fixtures/torture.scaena/assets/map-card.png");
  const mapAsset = `assets/${createHash("sha256").update(map).digest("hex")}.png`;
  const mapNode = '  mapped image  at:in(canvas) alt:"" z:-40\n';
  const withMap = (await source()).replace("Revenue tripled", "Revenue quadrupled").replace("state intro layout:title hold:4s\n", `state intro layout:title hold:4s\n${mapNode}`);
  await type(withMap);
  const mapAt = withMap.indexOf(mapNode) + "  mapped image ".length;
  const dropping = await page.evaluate(() => window.scaena.trips().length);
  await page.evaluate(
    async ([b64, at]) => window.scaena.drop("Map Card.png", Uint8Array.from(atob(b64), (c) => c.charCodeAt(0)).buffer, at),
    [map.toString("base64"), mapAt],
  );
  await page.waitForFunction((n) => window.scaena.trips().length > n && window.scaena.last().valid, dropping, { timeout: 60000 }).catch(() => {});
  check((await source()).includes(`mapped image "${mapAsset}"`) && (await page.evaluate(() => window.scaena.last().valid)), "a picture dropped after the save, and an edit");
  const waited = page.evaluate(async () => {
    window.scaena.die();
    // Once the worker has failed and stopped answering, a request of it.
    await new Promise((r) => setTimeout(r, 300));
    return window.scaena.reading().then(() => "read", (e) => `refused: ${e.message}`);
  });
  // While the stage waits for it to say anything, the status says so.
  const stalled = await page
    .waitForFunction(() => document.querySelector("#status").textContent.includes("stopped answering"), null, { timeout: 10000 })
    .then(() => page.locator("#status").textContent(), () => "");
  check(stalled.includes("waiting up to 20 seconds"), `while it waits, the status says the engine stopped answering: ${stalled}`);
  const late = await Promise.race([waited, new Promise((r) => setTimeout(() => r("waiting"), 30000))]);
  check(late.startsWith("refused: the engine stopped"), `a request waiting on a worker that died is refused, within the 20 s it is given to say anything: ${late}`);
  await page.waitForFunction(() => window.scaena.restarts() === 3, null, { timeout: 60000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.restarts())) === 3, "the engine starts again in a fresh worker");
  await says("Save when you can");
  const told = await status();
  check(
    told.includes("the worker died, as asked") && told.includes("from the deck as last saved, with the source as it stands") && told.includes("lost: Map Card.png"),
    `the status says so, naming the picture lost: ${told}`,
  );
  check((await source()).includes("Revenue quadrupled") && (await where()).dirty, "the source stands as it was, as changes not saved");
  await page.waitForFunction(() => window.scaena.last(), null, { timeout: 60000 }).catch(() => {});
  const reads = await page.evaluate(() => window.scaena.reading().then(() => "read", (e) => `refused: ${e.message}`));
  check(reads === "read", `the fresh worker answers: ${reads}`);
} finally {
  await browser.close();
  server.close();
}

if (failures.length) {
  console.error(`${failures.length} failure(s): ${failures.join("; ")}`);
  process.exit(1);
}
console.log("\nthe editor recovers from a panic in the engine, and saves after it");
