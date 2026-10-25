// lift-up FUNC-2 check: the offer to restate a beat's claim an edit on the canvas left behind, in
// headless Chromium (serve.mjs), the CPU painting, on the revenue example, whose cover's headline
// (`subtitle`, a text with `semantic: claim`) says the opening beat's claim.
//
//   node web/restate.mjs     (after `just web`; from the repository's root)
//
// - A new headline typed over the cover's, once the burst of typing ends, is offered beside the
//   status: "Slide 1's story still says “…”. Update it to “…”?", with Update and Keep.
// - The offer stands whatever the status says after it: Escape's "Typing done", and what lint
//   finds once typing stops. It was the status's own words, which anything said next took away.
// - Update restates the claim in the slide's words, and the offer goes.
// - Keep lets the claim be: the offer goes, and the source is as it was.
// - The next change to the source takes the offer away: an undo of the typing that left the claim
//   behind.
// Exits 1 on any failure.
import { launch, serve } from "./serve.mjs";

const server = await serve();
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const offer = () =>
    page.evaluate(() => ({
      shown: !document.querySelector("#offer").hidden,
      says: document.querySelector("#offer").textContent,
      update: !document.querySelector("#restate").hidden,
      keep: document.querySelector("#keep-claim").hidden ? undefined : document.querySelector("#keep-claim").textContent,
    }));
  const offered = () =>
    page.waitForFunction(() => !document.querySelector("#offer").hidden, null, { timeout: 30000, polling: 50 }).then(
      () => true,
      () => false,
    );
  const client = ([x, y]) =>
    page.evaluate(
      ([x, y]) => {
        const r = document.querySelector("#overlay").getBoundingClientRect();
        const [w, h] = window.scaena.canvas.size();
        return [r.left + (x / w) * r.width, r.top + (y / h) * r.height];
      },
      [x, y],
    );
  /** The cover's headline, double clicked at its middle and typed over whole with `words`, once
   * the source holds them and the worker is done with what was typed. */
  const retitle = async (words) => {
    await page.waitForFunction(() => window.scaena.canvas.boxes().some((b) => b.node === "subtitle"), null, { timeout: 30000 });
    const [x, y, w, h] = await page.evaluate(() => window.scaena.canvas.boxes().find((b) => b.node === "subtitle").rect);
    const [cx, cy] = await client([x + w / 2, y + h / 2]);
    await page.mouse.dblclick(cx, cy);
    await page.waitForFunction(() => window.scaena.canvas.typing() === "subtitle", null, { timeout: 30000 }).catch(() => {});
    await page.keyboard.press("Control+a");
    await page.keyboard.type(words);
    await page.waitForFunction(
      (t) => window.scaena.source().includes(t) && !window.scaena.canvas.typed()?.sending,
      words,
      { timeout: 30000, polling: 50 },
    );
  };

  // The cover is the first state, the opening beat's.
  await page
    .waitForFunction(() => window.scaena.shown() === 0 && window.scaena.canvas.boxed() === "intro", null, { timeout: 30000 })
    .catch(() => {});
  check(await page.evaluate(() => window.scaena.canvas.boxed() === "intro"), "the editor opens on the cover");
  const claim = "This quarter changed the shape of the business.";
  check((await source()).includes(claim), "the opening beat claims what the cover's headline says");

  // A new headline, typed: once the burst ends, the claim it left behind is offered restated.
  await retitle("This quarter reshaped the business.");
  check(await offered(), "the claim the headline left behind is offered restated, beside the status");
  const first = await offer();
  check(
    first.says === `Slide 1's story still says “${claim}”. Update it to “This quarter reshaped the business.”?`,
    `the offer says it in a person's words: ${first.says}`,
  );
  check(first.update && first.keep === "Keep", `with Update and Keep: ${JSON.stringify(first)}`);

  // What the status says next leaves it standing: Escape's words, and what lint finds after.
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => document.querySelector("#status").textContent.startsWith("Typing done"), null, { timeout: 30000 }).catch(() => {});
  check((await status()).startsWith("Typing done"), `Escape says typing is done: ${await status()}`);
  await page
    .waitForFunction(() => document.querySelector("#status").textContent.startsWith("Typing done — "), null, { timeout: 15000 })
    .catch(() => {});
  await page.waitForTimeout(500);
  const after = await offer();
  check(after.shown && after.says === first.says && after.update, `the offer stands over what the status says after it (${await status()}): ${JSON.stringify(after)}`);

  // Update restates the claim in the slide's words, and the offer goes.
  await page.click("#restate");
  await page.waitForFunction((c) => !window.scaena.source().includes(c), claim, { timeout: 30000, polling: 50 }).catch(() => {});
  const restated = await source();
  check(!restated.includes(claim) && restated.split("This quarter reshaped the business.").length === 3, "Update restates the claim in the headline's words");
  await page.waitForFunction(() => document.querySelector("#status").textContent.startsWith("The story says what the slide says"), null, { timeout: 30000 }).catch(() => {});
  check((await status()).startsWith("The story says what the slide says"), `and says so: ${await status()}`);
  const updated = await offer();
  check(!updated.shown && !updated.update && updated.keep === undefined, `and the offer goes: ${JSON.stringify(updated)}`);

  // Keep lets the claim be: the offer goes, the source as it was.
  await retitle("This quarter remade the business.");
  check(await offered(), "a headline typed again over the claim's words is offered again");
  await page.keyboard.press("Escape");
  const kept = await source();
  await page.click("#keep-claim");
  await page.waitForTimeout(500);
  const gone = await offer();
  check(!gone.shown && !gone.update && gone.keep === undefined, `Keep lets it be, and the offer goes: ${JSON.stringify(gone)}`);
  check((await source()) === kept, "Keep changes nothing in the source");

  // The next change to the source takes the offer away: the typing that left the claim behind,
  // undone.
  const before = await source();
  await retitle("This quarter rebuilt the business.");
  check(await offered(), "a third headline is offered too");
  await page.keyboard.press("Escape");
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  await page.waitForFunction((s) => window.scaena.source() === s, before, { timeout: 30000, polling: 50 }).catch(() => {});
  check((await source()) === before, "an undo takes the typing back");
  const undone = await offer();
  check(!undone.shown && !undone.update && undone.keep === undefined, `and the offer with it: ${JSON.stringify(undone)}`);
} catch (e) {
  failures.push(`error: ${e.message}`);
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "a claim an edit leaves behind is offered restated until the source changes again");
process.exit(failures.length ? 1 : 0);
