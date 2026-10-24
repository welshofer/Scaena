// Lift-up 10-24 REL-1 check: a deck kept nowhere is a deck not saved, in headless Chromium
// (serve.mjs), the CPU painting, as in web/new.mjs.
//
//   node web/unkept.mjs     (after `just web`; from the repository's root)
//
// A deck New makes, or one made from a prompt (both kept nowhere until their first save), opens
// with nothing edited, yet nothing of it is kept anywhere: closing the tab, a crash, or opening
// another deck would lose it. So, before any edit:
// - the line under the slide says it is not saved yet;
// - the recovery copy (lift-up REL-4) keeps it, rather than taking it away for having nothing to
//   save;
// - opening another deck over it asks first, and declined, leaves it open;
// - leaving the page asks first.
// Exits 1 on any failure.
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
  /** Each dialog the page opened, by its kind and its words; a confirm is declined while `decline`. */
  const asked = [];
  let decline = false;
  page.on("dialog", (d) => {
    asked.push({ type: d.type(), message: d.message() });
    void (decline && d.type() === "confirm" ? d.dismiss() : d.accept());
  });
  /** The editor once it has compiled and linted the bundle named `name`. */
  const ready = async (name) => {
    await page.waitForFunction(
      (name) => (window.scaena?.last() && window.scaena.where().name === name) || document.querySelector("#status")?.textContent.startsWith("Error"),
      name,
      { timeout: 120000 },
    );
    const status = await page.evaluate(() => (window.scaena?.last() ? "" : document.querySelector("#status").textContent));
    if (status) throw new Error(status);
  };
  const where = () => page.evaluate(() => window.scaena.where());

  await page.goto(editor("/docs/examples/revenue.deck.json"));
  await ready("revenue");

  // New: a deck kept nowhere, nothing edited.
  await page.click("#file-open").then(() => page.click("#new-deck"));
  await page.fill("#making-name", "Kept nowhere");
  await page.click("#making-create");
  await ready("kept-nowhere");
  const made = await where();
  check(made.where === undefined && !made.dirty, `New makes a deck kept nowhere, nothing edited: ${JSON.stringify(made)}`);

  // The line under the slide says so.
  const line = await page.locator("#where").textContent();
  check(line === "Kept nowhere · Not saved yet", `the line under the slide says it is not saved yet: ${line}`);

  // The recovery copy keeps it.
  const copied = await page.evaluate(async () => {
    await window.scaena.keepCopy();
    const key = window.scaena.copyKey();
    const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(key));
    const hashed = [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
    const root = await navigator.storage.getDirectory();
    try {
      const dir = await (await root.getDirectoryHandle("recovery")).getDirectoryHandle(hashed);
      const copy = JSON.parse(await (await (await dir.getFileHandle("copy.json")).getFile()).text());
      const source = await (await (await dir.getFileHandle("source.scn")).getFile()).text();
      return { key, name: copy.name, same: source === window.scaena.source() };
    } catch (e) {
      return { key, missing: String(e) };
    }
  });
  check(copied.same === true, `the recovery copy keeps it, its source as it stands: ${JSON.stringify(copied)}`);

  // Opening another deck over it asks first; declined, it stays open.
  decline = true;
  asked.length = 0;
  await page.evaluate(() => window.scaena.open({ url: "/docs/examples/revenue.deck.json" }));
  const confirmed = asked.find((a) => a.type === "confirm");
  check(Boolean(confirmed), `opening another deck over it asks first: ${confirmed?.message ?? JSON.stringify(asked)}`);
  const stays = await page.locator("#where").textContent();
  check(stays === "Kept nowhere · Not saved yet", `declined, the deck kept nowhere stays open: ${stays}`);
  decline = false;

  // Leaving the page asks first.
  asked.length = 0;
  await page.close({ runBeforeUnload: true });
  await new Promise((r) => setTimeout(r, 1000));
  check(
    asked.some((a) => a.type === "beforeunload"),
    `leaving the page asks first: ${JSON.stringify(asked)}`,
  );
} finally {
  await browser.close();
  server.close();
}

if (failures.length) {
  console.error(`${failures.length} failure(s): ${failures.join("; ")}`);
  process.exit(1);
}
console.log("a deck kept nowhere is a deck not saved");
