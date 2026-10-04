// PLAN 2.12 check: the editor in headless Chromium (serve.mjs) starts a deck and saves it
// somewhere new. The CPU paints, as in web/editor.mjs.
//
//   node web/new.mjs     (after `just web`; from the repository's root)
//
// - New, from its dialog: a title and a theme that ships. The deck is the theme, its fonts, and
//   one state with nothing on it, as `deck_create` makes one; it compiles and lints with no
//   error, kept nowhere, named for its title, and Play waits for a save. Each theme makes one.
// - Its first save goes into the browser's storage under its name; Play then plays it.
// - Save as, from its dialog, keeps it in the browser's storage under another name, and the
//   address names that. Save as a folder (any directory handle; here one in the browser's
//   storage) writes it there and keeps it there: a save then writes there, and the page,
//   reloaded, opens it from there.
// - A single-file export's page carries no themes or fonts to make a deck with.
// Exits 1 on any failure.
import { readFile } from "node:fs/promises";
import { launch, serve } from "./serve.mjs";

const server = await serve();
const editor = (bundle) => `${server.origin}/web/dist/editor.html?painter=cpu${bundle ? `&bundle=${bundle}` : ""}`;
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

/** The files under `path` in the origin-private file system, by their paths inside it. */
const listing = (page, path) =>
  page.evaluate(async (path) => {
    let dir = await navigator.storage.getDirectory();
    for (const part of path.split("/")) dir = await dir.getDirectoryHandle(part);
    const out = [];
    const walk = async (d, prefix) => {
      for await (const [name, handle] of d.entries()) {
        if (handle.kind === "directory") await walk(handle, `${prefix}${name}/`);
        else out.push(prefix + name);
      }
    };
    await walk(dir, "");
    return out.sort();
  }, path);

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  page.on("dialog", (d) => d.accept());
  /** The editor once it has compiled and linted the bundle named `name`. */
  const ready = async (name) => {
    await page.waitForFunction(
      (name) => (window.scaena?.last() && window.scaena.where().name === name) || document.querySelector("#status")?.textContent.startsWith("error"),
      name,
      { timeout: 120000 },
    );
    const status = await page.evaluate(() => (window.scaena?.last() ? "" : document.querySelector("#status").textContent));
    if (status) throw new Error(status);
  };
  const where = () => page.evaluate(() => window.scaena.where());
  const errors = () => page.evaluate(() => window.scaena.last().findings.filter((f) => f.severity === "error").map((f) => f.code));
  const type = async (text) => {
    const before = await page.evaluate(() => window.scaena.trips().length);
    await page.evaluate((t) => window.scaena.type(t), text);
    await page.waitForFunction((n) => window.scaena.trips().length > n, before, { timeout: 60000, polling: 50 });
    return page.evaluate(() => window.scaena.last());
  };

  await page.goto(editor("/docs/examples/revenue.deck.json"));
  await ready("revenue");
  const revenue = await page.locator("#stage").screenshot();

  // New, from its dialog.
  const themes = await page.locator("#making-theme option").allTextContents();
  check(themes.join() === "Dusk,Daybreak,Ember", `New offers the themes that ship: ${themes}`);
  await page.click("#new-deck");
  await page.fill("#making-name", "Field notes: what’s next");
  await page.selectOption("#making-theme", "Daybreak");
  await page.click("#making-create");
  await ready("field-notes-whats-next");
  const made = await where();
  check(made.where === undefined && !made.dirty, `a new deck is kept nowhere, named for its title: ${JSON.stringify(made)}`);
  const source = await page.evaluate(() => window.scaena.source());
  check(source.includes('deck "Field notes: what’s next"') && source.includes('theme:"themes/daybreak.theme.json"'), `its source has its title and its theme: ${source.split("\n")[0]}`);
  check(/state start\b/.test(source) && !/^\s*\w+ (text|shape|chart|table|image)\b/m.test(source), "one state, with nothing on it");
  check((await page.evaluate(() => window.scaena.last().valid)) && (await errors()).length === 0, `it compiles and lints with no error: ${await errors()}`);
  check(await page.locator("#play").isHidden(), "Play waits for a save");
  check(!(await page.locator("#stage").screenshot()).equals(revenue), "the preview shows it");

  // A title on its state, then its first save: into the browser's storage under its name.
  const titled = source.replace(/^state start\b.*$/m, 'state start layout:title\n  title text role:display "Field notes" at:in(title)');
  const typed = await type(titled);
  check(typed.valid && (await where()).dirty, `an edit compiles and marks it changed: ${typed.error?.message ?? ""}`);
  const saved = await page.evaluate(() => window.scaena.save());
  check(saved?.where.kind === "opfs" && saved.where.name === "field-notes-whats-next", `its first save keeps it in the browser: ${JSON.stringify(saved?.where)}`);
  const kept = await listing(page, "bundles/field-notes-whats-next");
  check(
    kept.includes("deck.json") && kept.includes("manifest.json") && kept.includes("themes/daybreak.theme.json") && kept.filter((p) => /^fonts\/[A-Za-z]+-[0-9a-f]{16}\.ttf$/.test(p)).length === 3,
    `it holds its deck, manifest, theme, and three fonts named by their content: ${kept}`,
  );
  check(new URL(page.url()).searchParams.get("bundle") === "opfs:field-notes-whats-next", `the address names it: ${page.url()}`);
  check(await page.locator("#play").isVisible(), "Play plays it now");

  // Save as, from its dialog: another name in the browser.
  await page.click("#save-as");
  check((await page.inputValue("#saving-as-name")) === "field-notes-whats-next", "Save as starts from its name");
  await page.fill("#saving-as-name", "season copy");
  await page.click("#saving-as-opfs");
  await page.waitForFunction(() => window.scaena.where().where?.name === "season copy" && !window.scaena.where().dirty, null, { timeout: 60000 });
  check((await listing(page, "bundles/season copy")).includes("deck.json"), "Save as keeps a copy in the browser under the new name");
  check(new URL(page.url()).searchParams.get("bundle") === "opfs:season copy", `the address names the copy: ${page.url()}`);
  const kinds = await page.locator("#kept option").allTextContents();
  check(kinds.includes("season copy") && kinds.includes("field-notes-whats-next"), `the browser keeps both: ${kinds}`);

  // Save as a folder: written there, kept there.
  const asFolder = await page.evaluate(async () => {
    const folder = await (await navigator.storage.getDirectory()).getDirectoryHandle("on-disk", { create: true });
    return window.scaena.saveAs({ folder });
  });
  check(asFolder?.where.kind === "folder" && asFolder.where.name === "on-disk", `Save as a folder keeps it there: ${JSON.stringify(asFolder?.where)}`);
  check((await listing(page, "on-disk")).includes("deck.json"), "the folder holds the bundle");
  const again = (await page.evaluate(() => window.scaena.source())).replace('"Field notes"', '"Field notes, again"');
  await type(again);
  await page.evaluate(() => window.scaena.save());
  await page.goto(editor("folder:on-disk"));
  await ready("on-disk");
  check((await page.evaluate(() => window.scaena.source())).includes('"Field notes, again"'), "a save writes the folder, and the page opens it from there");

  // Every theme that ships makes a deck that lints with no error.
  for (const theme of themes) {
    await page.evaluate((theme) => window.scaena.open({ create: { theme, title: `A ${theme} deck` } }), theme);
    await ready(`a-${theme.toLowerCase()}-deck`);
    const text = await page.evaluate(() => window.scaena.source());
    check(text.includes(`themes/${theme.toLowerCase()}.theme.json`) && (await errors()).length === 0, `${theme} makes a deck with no error: ${await errors()}`);
  }

  const standalone = await readFile("crates/scaena-export/player/standalone.html", "utf8");
  check(!standalone.includes("daybreak.theme.json") && !standalone.includes("Fraunces-VF"), "a single-file export's page carries no themes to make a deck with");
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "the editor starts a deck and saves it somewhere new");
process.exit(failures.length ? 1 : 0);
