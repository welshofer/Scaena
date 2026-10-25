// lift-up S1 check: the Story tab leads with what to do first, in headless Chromium (serve.mjs), on
// the higher-ed example, whose story health finds 25 things to look at.
//
//   node web/story.mjs     (after `just web`; from the repository's root)
//
// - At most five findings stand open, none of them an info: the warnings whose measure scores
//   lowest. The rest, the infos among them, fold under one disclosure that counts them by kind
//   ("20 more: 12 arc, …"), shut until it is opened, and opened it shows each of them, so every
//   finding is there once.
// - A slide that lint finds broken leads: its support given more words than its place holds, the
//   Story tab's first row is that slide's error, the disclosure left open as it was.
// - On the revenue example: a new headline typed over the cover's, which says the opening beat's
//   claim, is offered beside the status once the burst of typing ends ("Slide 1's story still says
//   “…”. Update it to “…”?"), with Update and Keep, and the offer stands whatever the status says
//   after it, Escape's words and what lint finds; Update restates the claim and the offer goes; Keep
//   lets it be and changes nothing; an undo of the typing takes the offer away (lift-up FUNC-2).
// Exits 1 on any failure.
import { launch, serve, tab } from "./serve.mjs";

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
  await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/higher-ed.deck.json`);
  await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });
  await tab(page, "story");
  await page.waitForFunction(() => window.scaena.story.health() && document.querySelector("#story [data-findings] li"), null, { timeout: 120000 });

  const findings = await page.evaluate(() => window.scaena.story.health().findings);
  check(findings.length > 5, `the deck has more to look at than five: ${findings.length}`);
  const opened = "#story [data-findings] > li[data-slide] p[data-kind]";
  const folded = "#story [data-findings] > li[data-more] li[data-slide] p[data-kind]";
  const open = await page.locator(opened).count();
  check(open === 5, `five findings stand open: ${open}`);
  const triaged = await page.evaluate(() => window.scaena.story.triaged());
  check(triaged.open.every((f) => f.severity !== "info"), `none of them an info: ${triaged.open.map((f) => `${f.slide} ${f.kind}`)}`);
  const more = await page.locator(folded).count();
  check(open + more === findings.length, `with the folded ones, every finding is there once: ${open} + ${more} of ${findings.length}`);
  // The count by kind, the most first.
  const counts = new Map();
  for (const f of triaged.more) counts.set(f.kind, (counts.get(f.kind) ?? 0) + 1);
  const expected = `${triaged.more.length} more: ${[...counts].sort(([, a], [, b]) => b - a).map(([k, n]) => `${n} ${k === "bigIdea" ? "big idea" : k}`).join(", ")}`;
  const summary = (await page.textContent("#story [data-more] summary")).trim();
  check(summary === expected, `the rest fold under one count by kind: ${summary}`);
  const first = page.locator("#story [data-more] li[data-slide] [data-show]").first();
  check(!(await first.isVisible()), "folded until it is opened");
  await page.click("#story [data-more] summary");
  check(await first.isVisible(), "and opened, it shows them");

  // A slide that lint finds broken leads.
  const source = await page.evaluate(() => window.scaena.source());
  const long = "Comprehensive was the aspiration, and every campus grew to meet it. ".repeat(12).trim();
  const broken = source.replace(
    '"Comprehensive was the aspiration: more programs, labs, campuses, teams, international students, and online degrees. One answer to every question."',
    `"${long}"`,
  );
  check(broken !== source, "the support's words are found in the source");
  await page.evaluate((s) => window.scaena.type(s), broken);
  await page
    .waitForFunction(() => window.scaena.story.health()?.findings.some((f) => f.severity === "error"), null, { timeout: 120000 })
    .catch(() => {});
  const lead = await page.evaluate(() => {
    const row = document.querySelector("#story [data-findings] > li[data-slide]");
    return { severity: row?.dataset.severity, kind: row?.querySelector("p[data-kind]")?.dataset.kind, slide: row?.dataset.slide };
  });
  check(lead.severity === "error" && lead.kind === "layout", `the broken slide's error leads: ${JSON.stringify(lead)}`);
  check(await page.evaluate(() => document.querySelector("#story [data-more] details")?.open === true), "the disclosure stays open as it was left");

  // A claim a headline typed on the canvas leaves behind (lift-up FUNC-2), on the revenue
  // example, whose cover's headline (`subtitle`, a text with `semantic: claim`) says the opening
  // beat's claim.
  await (async () => {
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
  })();
} catch (e) {
  failures.push(`error: ${e.message}`);
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "the Story tab leads with what to do first, and folds the rest; a claim an edit leaves behind is offered restated until the source changes again");
process.exit(failures.length ? 1 : 0);
