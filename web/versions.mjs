// PLAN 2.60 check: the deck's versions in the editor's Versions tab, in headless Chromium
// (serve.mjs), the CPU painting, on the revenue example kept with a history from its first save
// and changed by `scaena` four times since: its title's text, a data file's cell, a state added,
// and an image node added and taken out, the image's file with it.
//
//   node web/versions.mjs     (after `just web`; from the repository's root)
//
// - The tab lists the history's changes, the newest first, each by its author and when.
// - A version chosen shows a state of it at rest, read only, and what changed from it to the deck
//   now; or, chosen, to another version.
// - Restore makes it the deck again, with the data file it read, as one change: ⌘Z in the source
//   puts the deck and the data file back as they were, ⇧⌘Z restores them again, and a save
//   records the restore in the history, by the user.
// - A version whose deck names a file the bundle no longer holds is refused, and says why.
// - axe-core finds nothing against WCAG 2.1 AA with a version shown; a bundle that keeps no
//   history says so, and Keep a history begins one with a save (PLAN 2.87).
// Exits 1 on any failure.
import { execFileSync } from "node:child_process";
import { copyFile, mkdir, rm, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { join } from "node:path";
import { launch, serve } from "./serve.mjs";

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};
const scaena = (...args) => execFileSync("cargo", ["run", "-q", "-p", "scaena-cli", "--locked", "--", ...args], { stdio: "inherit" });

// The revenue example, its history begun, and four changes since.
const root = "target/web-versions";
const bundle = `${root}/revenue`;
await rm(root, { recursive: true, force: true });
scaena("save", "docs/examples/revenue.deck.json", "--to", bundle, "--history", "--keep-fonts");
const patch = async (ops) => {
  await writeFile(join(root, "ops.json"), JSON.stringify(ops));
  scaena("patch", bundle, "--ops", join(root, "ops.json"));
};
await patch([{ op: "set_text", node: "title", text: "Revenue tripled" }]);
await writeFile(join(root, "edits.json"), JSON.stringify([{ op: "set", row: 0, column: "revenue", value: "9" }]));
scaena("data", bundle, "q3", "--edits", join(root, "edits.json"));
await patch([{ op: "add_state", state: { id: "outro" } }]);
await mkdir(join(bundle, "assets"), { recursive: true });
await copyFile("tests/fixtures/torture.scaena/assets/test-card.png", join(bundle, "assets/card.png"));
await patch([{ op: "add_node", id: "photo", node: { type: "image", src: "assets/card.png" } }]);
await patch([{ op: "remove_node", id: "photo" }]);
scaena("files", bundle, "--remove", "assets/card.png");

const axe = createRequire(import.meta.url).resolve("axe-core/axe.min.js");
const site = await serve();
const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  /** The editor on `deck`, the Versions tab shown. */
  const open = async (deck) => {
    const page = await context.newPage();
    page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
    page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
    await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&source=open&bundle=${deck}`);
    await page.waitForFunction(() => window.scaena?.last()?.valid, null, { timeout: 120000 });
    await page.click("#tab-versions");
    await page.waitForFunction(() => window.scaena.versions.listed() === null || window.scaena.versions.listed().length > 0, null, { timeout: 60000 });
    return page;
  };

  // A bundle that keeps no history says so, and Keep a history begins one with a save, here into
  // the browser's storage: the deck as saved its first version, by the user (PLAN 2.87).
  {
    const page = await open("/docs/examples/revenue.deck.json");
    const said = await page.locator("#versions [data-summary]").textContent();
    const keep = page.locator("#versions [data-keep]");
    check(said.includes("keeps no history") && (await keep.isVisible()), `a bundle without a history says so, and offers to keep one: ${said}`);
    await keep.click();
    await page.waitForFunction(() => window.scaena.versions.listed()?.length > 0, null, { timeout: 60000 }).catch(() => {});
    const begun = await page.evaluate(() => window.scaena.versions.listed());
    check(
      begun?.length === 1 && begun[0].message === "history begins" && begun[0].author === "user" && !(await keep.isVisible()),
      `Keep a history saves the bundle, its first version the deck as saved: ${JSON.stringify(begun?.map((v) => `${v.message} · ${v.author}`))} · ${await page.locator("#status").textContent()}`,
    );
    await page.close();
  }

  const page = await open(`/${bundle}/deck.json`);
  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.locator("#status").textContent();
  const settled = () => page.evaluate(() => window.scaena.versions.settled());
  const changes = () => page.evaluate(() => window.scaena.versions.changes());

  // The history's changes, the newest first.
  const versions = await page.evaluate(() => window.scaena.versions.listed());
  const messages = versions.map((v) => v.message);
  check(
    JSON.stringify(messages) ===
      JSON.stringify(["history begins", "patch: set_text", "data_edit q3: revenue of row 0", "patch: add_state", "patch: add_node", "patch: remove_node"]),
    `the history's changes, oldest first: ${JSON.stringify(messages)}`,
  );
  check(versions.every((v, i) => v.n === i + 1 && v.author === "user" && v.at?.endsWith("Z")), "each numbered, by its author, and when");
  const rows = await page.locator("#versions [data-versions] button").allTextContents();
  check(rows.length === versions.length && rows[0].includes("patch: remove_node") && rows.at(-1).includes("history begins"), `listed the newest first: ${rows[0]} … ${rows.at(-1)}`);
  const summary = await page.locator("#versions [data-summary]").textContent();
  check(summary.startsWith(`${versions.length} versions`), `the summary counts them: ${summary}`);

  // The first version, read only: a state at rest, and what changed from it to the deck now.
  const first = versions[0];
  await page.click(`#versions [data-id="${first.id}"]`);
  await settled();
  const title = await page.locator("#versions [data-title]").textContent();
  check(title.startsWith("Version 1: history begins · user"), `the version shown says what it is: ${title}`);
  const states = await page.locator("#versions [data-state] option").allTextContents();
  check(JSON.stringify(states) === JSON.stringify(["intro", "revenue", "mix", "close"]), `its states, as it had them: ${JSON.stringify(states)}`);
  const drawn = async () =>
    page.evaluate(async () => {
      const img = document.querySelector("#versions img");
      await img.decode().catch(() => {});
      return { src: img.src, width: img.naturalWidth, alt: img.alt };
    });
  const intro = await drawn();
  check(intro.width >= 320 && intro.alt.includes("intro") && intro.alt.includes("version 1"), `a state of it drawn at rest: ${JSON.stringify(intro)}`);
  const now = await changes();
  check(
    now.some((l) => l.startsWith("state intro:") && l.includes('title: text "Revenue tripled"')) &&
      now.includes("state outro added") &&
      now.includes("data/q3-revenue.csv changed"),
    `what changed from it to the deck now: ${JSON.stringify(now)}`,
  );
  await page.selectOption("#versions [data-state]", "mix");
  await settled();
  const mix = await drawn();
  check(mix.src !== intro.src && mix.alt.includes("mix"), `another of its states, picked: ${mix.alt}`);

  // Compared with another version.
  await page.selectOption("#versions [data-against]", versions[1].id);
  await settled();
  const second = await changes();
  check(
    second.length === 1 && second[0].startsWith("state intro:") && second[0].includes('"Revenue tripled"'),
    `to version 2, the title's text alone: ${JSON.stringify(second)}`,
  );
  await page.selectOption("#versions [data-against]", versions[2].id);
  await settled();
  check((await changes()).includes("data/q3-revenue.csv changed"), "to version 3, the data file too");
  await page.selectOption("#versions [data-against]", "");
  await settled();

  // axe-core, with a version shown.
  await page.addScriptTag({ path: axe });
  const found = await page.evaluate(async () => {
    const result = await window.axe.run(document, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  check(!found.length, `axe finds nothing against WCAG 2.1 AA with a version shown${found.length ? `:\n    ${found.join("\n    ")}` : ""}`);

  /** The data file's first row's revenue, as the Data tab shows it once it reads `expected`, or
   * as it reads after a while. */
  const revenue = async (expected) => {
    await page.click("#tab-data");
    await page
      .waitForFunction((v) => window.scaena.data.shown()?.sheet?.rows[0]?.[2] === v, expected, { timeout: 30000, polling: 100 })
      .catch(() => {});
    const value = await page.evaluate(() => window.scaena.data.shown()?.sheet?.rows[0]?.[2]);
    await page.click("#tab-versions");
    await settled();
    return value;
  };
  const later = await source();
  check((await revenue("9")) === "9", "the data file as the deck reads it now: the edit's 9");

  // Restore: the deck and its data file as they were, as one change.
  await page.click(`#versions [data-id="${first.id}"]`);
  await settled();
  await page.click("#versions [data-restore]");
  await settled();
  await page.waitForFunction(() => window.scaena.last()?.valid && window.scaena.source().includes('"Q3 Review"'), null, { timeout: 60000 }).catch(() => {});
  const restored = await source();
  check(restored.includes('"Q3 Review"') && !restored.includes("Revenue tripled") && !/state outro/.test(restored), "Restore makes the first version the deck again");
  check((await status()).startsWith("version 1 restored, with data/q3-revenue.csv"), `and says so: ${await status()}`);
  check((await changes())[0]?.startsWith("nothing: version 1 draws and reads as the deck now does"), `the version and the deck now are alike: ${JSON.stringify(await changes())}`);
  check((await revenue("18.2")) === "18.2", "the data file as the version read it");

  // ⌘Z puts back the deck and its data file; ⇧⌘Z restores them again.
  const press = async (keys, to) => {
    await page.locator(".cm-content").focus();
    await page.keyboard.press(keys);
    await page.waitForFunction((s) => window.scaena.source() === s && window.scaena.last()?.valid, to, { timeout: 30000, polling: 50 }).catch(() => {});
    return (await source()) === to;
  };
  check(await press("Control+z", later), "⌘Z puts the deck back as it was before the restore");
  check((await revenue("9")) === "9", "and its data file");
  check(await press("Control+Shift+z", restored), "⇧⌘Z restores the version again");
  check((await revenue("18.2")) === "18.2", "and its data file");

  // A save records the restore, by the user.
  const saved = await page.evaluate(() => window.scaena.save());
  check(saved?.recorded === true, `the save records it in the history: ${await status()}`);
  await page.waitForFunction((n) => window.scaena.versions.listed()?.length > n, versions.length, { timeout: 30000 }).catch(() => {});
  const after = await page.evaluate(() => window.scaena.versions.listed());
  const recorded = after.slice(versions.length).map((v) => `${v.author}: ${v.message}`);
  check(recorded.includes(`user: history --restore ${first.id}`), `the versions list the restore after the save: ${JSON.stringify(recorded)}`);
  check(!recorded.some((r) => r.startsWith("fs:")), "and nothing as changed outside Scaena: the restore wrote the data file");

  // A version whose deck names a file the bundle no longer holds is refused, with why.
  const photo = after.find((v) => v.message === "patch: add_node");
  await page.click(`#versions [data-id="${photo.id}"]`);
  await settled();
  const why = await page.locator("#versions [data-why]").textContent();
  check(
    (await page.locator("#versions img").isHidden()) && why.includes("assets/card.png, which the bundle no longer holds"),
    `a version that names a file taken out since is not drawn, and says why: ${why}`,
  );
  // The image node no state shows draws nothing: what changed since is what draws and reads.
  check((await changes()).includes("state outro taken out"), `and what changed since: ${JSON.stringify(await changes())}`);
  const before = await source();
  await page.click("#versions [data-restore]");
  await settled();
  check((await status()).startsWith(`version ${photo.n} not restored: E102`), `a version whose image is gone is refused, with why: ${await status()}`);
  check((await source()) === before, "and the deck stays as it is");
  await context.close();
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  site.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "the editor lists the deck's versions, shows, compares, and restores them");
process.exit(failures.length ? 1 : 0);
