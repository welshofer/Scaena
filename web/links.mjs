// PLAN 2.70 check: links in a text, in the editor and the player in headless Chromium
// (serve.mjs), the CPU painting, on the torture deck's `links` case: `link-text`, whose runs link
// to a web address and to the `shapes` state.
//
//   node web/links.mjs     (after `just web`; from the repository's root)
//
// - The editor: characters selected in a text typed in, ⌘K asks where they go in a field over
//   them; a state's id, or a web address, links them, one `style_text` of `link`, one step to
//   undo; words that are neither make nothing and say so; nothing takes a link away. The keys
//   sheet lists ⌘K.
// - The player: a click on the state's link shows that state; a click on the web address's
//   opens it in a window of its own; a click off every link goes on, as before. The reading
//   names each link and where it goes.
// - axe-core finds nothing in the player on the case.
// Exits 1 on any failure.
import { createRequire } from "node:module";
import { launch, serve } from "./serve.mjs";

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

const axe = createRequire(import.meta.url).resolve("axe-core/axe.min.js");
const site = await serve();
const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`editor: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`editor console: ${m.text()}`));
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/tests/fixtures/torture.scaena/deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid, null, { timeout: 120000 });
  await page.selectOption("#state", "links");
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "links", null, { timeout: 30000 }).catch(() => {});

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const says = async (text) => {
    await page.waitForFunction((t) => document.querySelector("#status").textContent.includes(t), text, { timeout: 30000 }).catch(() => {});
    return (await status()).includes(text);
  };
  const comes = (text) =>
    page
      .waitForFunction((t) => window.scaena.source().includes(t) && !window.scaena.canvas.typed()?.sending, text, { timeout: 30000, polling: 50 })
      .then(() => true)
      .catch(() => false);
  const back = (to) =>
    page
      .waitForFunction((s) => window.scaena.source() === s && !window.scaena.canvas.typed()?.sending, to, { timeout: 30000, polling: 50 })
      .then(() => true)
      .catch(() => false);
  /** Select characters `from` to `to` of the text typed in. */
  const choose = (from, to) =>
    page.evaluate(
      ([from, to]) => {
        const area = document.querySelector("textarea.typing");
        area.setSelectionRange(from, to);
        document.dispatchEvent(new Event("selectionchange"));
      },
      [from, to],
    );
  /** ⌘K, and `words` typed into the field it shows, then Enter. */
  const link = async (words) => {
    await page.keyboard.press("Control+k");
    const field = await page.waitForSelector("input.note-words", { timeout: 10000 }).catch(() => null);
    if (!field) return false;
    if (words) await page.keyboard.type(words);
    await page.keyboard.press("Enter");
    return true;
  };

  const original = await source();
  // Typed in; "Read" chosen.
  await page.evaluate(() => window.scaena.canvas.select("link-text"));
  await page.locator("#overlay").focus();
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => window.scaena.canvas.typing() === "link-text", null, { timeout: 30000 });
  await choose(0, 4);
  check(await link("#shapes"), "⌘K asks where the characters go, in a field over them");
  check(await says("linked to the state shapes"), `the status says so: ${await status()}`);
  check(await comes('{text: Read, link: {state: shapes}}'), "a state's id links them to it");
  check((await page.evaluate(() => window.scaena.canvas.typing())) === "link-text", "typing goes on");
  await page.keyboard.press("Control+z");
  check(await back(original), "one undo takes it back");

  // A web address.
  await choose(0, 4);
  await link("https://example.com/read");
  check(await comes('{text: Read, link: {href: "https://example.com/read"}}'), "a web address links them to it");
  // Nothing takes it away.
  await choose(0, 4);
  await link("");
  check(await back(original), "nothing takes the link away");
  // Words that are no link make nothing.
  await choose(0, 4);
  const before = await source();
  await link("not a link");
  check(await says("is no link"), `words that are no link make nothing, and say so: ${await status()}`);
  check((await source()) === before, "and the deck is as it was");
  await page.keyboard.press("Escape");

  // The keys sheet.
  await page.locator("#overlay").focus();
  await page.keyboard.press("Escape");
  await page.keyboard.press("?");
  await page.waitForSelector("#keys[open]", { timeout: 10000 }).catch(() => {});
  const sheet = await page.evaluate(() => [...document.querySelectorAll("#keys tr")].map((tr) => tr.textContent));
  check(sheet.some((l) => l.includes("link the characters selected")), "the keys sheet lists ⌘K in a text");

  // The player, on the case.
  const player = await context.newPage();
  player.on("pageerror", (e) => failures.push(`player: ${e.message}`));
  await player.goto(`${site.origin}/web/dist/?painter=cpu&bundle=/tests/fixtures/torture.scaena/deck.json`);
  await player.waitForFunction(() => window.scaena?.at, null, { timeout: 120000 });
  const states = await page.evaluate(() => window.scaena.opened.states);
  await player.evaluate((i) => window.scaena.seek(i), states.indexOf("links"));
  await player.waitForFunction((i) => window.scaena.at().index === i, states.indexOf("links"), { timeout: 30000 });
  // Where each link is: the canvas scanned, row by row.
  const found = await player.evaluate(async () => {
    const out = {};
    for (let y = 0.15; y < 0.5; y += 0.01) {
      for (let x = 0.03; x < 0.5; x += 0.01) {
        const l = await window.scaena.linkAt([x, y]);
        const key = l ? ("href" in l ? "href" : "state") : undefined;
        if (key && !out[key]) out[key] = { at: [x, y], link: l };
      }
    }
    return out;
  });
  check(found.state?.link.state === "shapes" && found.href?.link.href === "https://example.com/scaena/method", `both links are found where they are drawn: ${JSON.stringify(found)}`);
  const click = async ([fx, fy]) => {
    const r = await player.locator("#stage").boundingBox();
    await player.mouse.click(r.x + fx * r.width, r.y + fy * r.height);
  };
  // Reading: each link an `a`.
  const reading = await player
    .waitForFunction(() => document.querySelector('#reading [data-node="link-text"] a[data-state]'), null, { timeout: 30000 })
    .then(() => player.evaluate(() => document.querySelector('#reading [data-node="link-text"]').innerHTML))
    .catch(() => "");
  check(reading.includes('<a href="https://example.com/scaena/method">') && reading.includes('data-state="shapes" title="To Shapes, in this deck"'), `the reading names each link and where it goes: ${reading}`);
  await player.addScriptTag({ path: axe });
  const violations = await player.evaluate(async () => {
    const result = await window.axe.run(document, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  check(!violations.length, `axe finds nothing in the player on the case${violations.length ? `:\n    ${violations.join("\n    ")}` : ""}`);
  // The web address opens in a window of its own; the deck stays where it is.
  if (found.href) {
    // No network here: the web address answers from the test.
    await context.route("https://example.com/**", (r) => r.fulfill({ contentType: "text/html", body: "<title>method</title>" }));
    const opened = context.waitForEvent("page", { timeout: 10000 }).catch(() => null);
    await click(found.href.at);
    const popup = await opened;
    if (popup) await popup.waitForEvent("framenavigated", { timeout: 5000 }).catch(() => {});
    const url = popup ? popup.url() : "";
    check(popup !== null && url.includes("example.com/scaena/method"), `a click on the web address opens it in a window of its own: ${popup ? url : "none"}`);
    await popup?.close().catch(() => {});
    check((await player.evaluate(() => window.scaena.at().index)) === states.indexOf("links"), "and the deck stays on the case");
  }
  // The state's link shows the state.
  if (found.state) {
    await click(found.state.at);
    const shown = await player
      .waitForFunction((i) => window.scaena.at().index === i, states.indexOf("shapes"), { timeout: 30000 })
      .then(() => true)
      .catch(() => false);
    check(shown, "a click on the state's link shows that state");
  }
  // Off every link, a click goes on.
  await player.evaluate((i) => window.scaena.seek(i), states.indexOf("links"));
  await player.waitForFunction((i) => window.scaena.at().index === i, states.indexOf("links"), { timeout: 30000 });
  await click([0.9, 0.9]);
  const on = await player
    .waitForFunction((i) => window.scaena.at().index === i + 1, states.indexOf("links"), { timeout: 30000 })
    .then(() => true)
    .catch(() => false);
  check(on || states.indexOf("links") === states.length - 1, "a click off every link goes on");
} finally {
  await browser.close();
  await site.close();
}

if (failures.length) {
  console.log(`\n${failures.length} failed:\n  ${failures.join("\n  ")}`);
  process.exit(1);
}
console.log("\nall passed");
