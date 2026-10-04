// PLAN 2.11 check: `scaena serve` (ADR-0012), the player and the editor on a bundle's folder on
// disk, in headless Chromium (serve.mjs's `launch`). The CPU paints, as in web/editor.mjs.
//
//   node web/live.mjs     (after `just web`, so the `scaena` it builds carries the pages; from the repository's root)
//
// - The revenue example, saved into a folder with its source beside it as `deck.scn`, is served.
// - The player opens on it, at a state. `deck.scn` saved on disk, as a text editor saves it: the
//   player reads the deck again and shows the new words, at the same state.
// - `deck.scn` broken on disk: the player says so, at its line, and keeps the deck. Mended, the
//   word goes.
// - The editor opens `deck.scn` as its source. Typed and saved, the folder's `deck.scn` and
//   `deck.json` are the editor's, and the player, in a tab of its own, hears of the save.
// - `deck.scn` saved on disk while the editor has nothing of its own not saved: the editor takes
//   it. Over changes not saved, it offers to take it, and keeps them until asked.
// Exits 1 on any failure.
import { execFileSync, spawn } from "node:child_process";
import { readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { launch } from "./serve.mjs";

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};
const cargo = (...args) => execFileSync("cargo", args, { stdio: "inherit" });

// The bundle: the revenue example saved into a folder, and its source beside it.
const bundle = "target/web-live/talk";
await rm("target/web-live", { recursive: true, force: true });
cargo("build", "-q", "-p", "scaena-cli", "--locked");
const scaena = "target/debug/scaena";
execFileSync(scaena, ["save", "docs/examples/revenue.deck.json", "--to", bundle, "--keep-fonts"]);
execFileSync(scaena, ["decompile", bundle, "-o", join(bundle, "deck.scn")]);
const source = await readFile(join(bundle, "deck.scn"), "utf8");
const deckOnDisk = () => readFile(join(bundle, "deck.json"), "utf8");
const scnOnDisk = () => readFile(join(bundle, "deck.scn"), "utf8");

// Served, on a free port; it says where on stdout, as one JSON value.
const server = spawn(scaena, ["serve", bundle, "--port", "0", "--json"], { stdio: ["ignore", "pipe", "inherit"] });
let said = "";
const where = await new Promise((resolve, reject) => {
  server.stdout.on("data", (chunk) => {
    said += chunk;
    try {
      resolve(JSON.parse(said));
    } catch {}
  });
  server.once("exit", (code) => reject(new Error(`scaena serve exited ${code}: ${said}`)));
});
check(where.player?.startsWith("http://localhost:") && where.editor === `${where.player}edit`, `served at ${where.player}`);
const at = (path) => new URL(path, where.player).href;

const browser = await launch();
try {
  const context = await browser.newContext();
  const player = await context.newPage();
  player.on("pageerror", (e) => check(false, `the player: ${e.message}`));
  // The address `/` sends a browser to, with the CPU painting.
  await player.goto(at("/index.html?bundle=/bundle/&serve&painter=cpu&state=revenue"));
  await player.waitForFunction(() => window.scaena?.at, null, { timeout: 120_000 });
  const reading = () => player.locator("#reading").innerText();
  await player.waitForFunction(() => document.querySelector("#reading")?.textContent?.includes("Revenue doubled"), null, { timeout: 30_000 });
  const state = () => player.evaluate(() => window.scaena.opened().states[window.scaena.at().index]);
  check((await state()) === "revenue", "the player opens on the bundle on disk, at `revenue`");

  // Saved on disk, as a text editor saves it.
  await writeFile(join(bundle, "deck.scn"), source.replace('"Revenue doubled"', '"Revenue more than doubled"'));
  await player.waitForFunction(() => window.scaena.reloads() > 0, null, { timeout: 30_000 });
  await player.waitForFunction(() => document.querySelector("#reading")?.textContent?.includes("more than doubled"), null, { timeout: 30_000 });
  check((await deckOnDisk()).includes("Revenue more than doubled"), "deck.scn saved on disk compiles into deck.json");
  check((await state()) === "revenue", `the player reads the deck again where it was: ${await state()}`);

  // Broken on disk: said at its line; the deck is kept.
  const kept = await deckOnDisk();
  const broken = `${source}\nstate broken\n  t text "unterminated\n`;
  const line = broken.split("\n").findIndex((l) => l.includes("unterminated")) + 1;
  await writeFile(join(bundle, "deck.scn"), broken);
  await player.waitForFunction(() => !document.querySelector("#served")?.hidden, null, { timeout: 30_000 });
  const alert = await player.locator("#served").innerText();
  check(alert.includes(`deck.scn:${line}:`) && alert.includes("this string is not closed"), `a broken deck.scn is said at its line: ${alert}`);
  check((await deckOnDisk()) === kept, "the deck stays as it was");
  await writeFile(join(bundle, "deck.scn"), source);
  await player.waitForFunction(() => document.querySelector("#served")?.hidden, null, { timeout: 30_000 });
  await player.waitForFunction(() => document.querySelector("#reading")?.textContent?.includes("Revenue doubled"), null, { timeout: 30_000 });
  check((await reading()).includes("Revenue doubled"), "mended, it compiles, and the word goes");

  // The editor, on deck.scn.
  const editor = await context.newPage();
  editor.on("pageerror", (e) => check(false, `the editor: ${e.message}`));
  editor.on("dialog", (d) => d.accept());
  await editor.goto(at("/editor.html?bundle=/bundle/&serve&painter=cpu"));
  await editor.waitForFunction(() => window.scaena?.source, null, { timeout: 120_000 });
  check((await editor.evaluate(() => window.scaena.source())) === source, "the editor opens deck.scn as its source");
  const told = await editor.locator("#where").innerText();
  check(told.includes("talk") && told.includes("served"), `it says where the bundle is: ${told}`);

  const reloads = await player.evaluate(() => window.scaena.reloads());
  const typed = source.replace('"Revenue doubled"', '"Revenue doubled again"');
  await editor.evaluate((text) => window.scaena.type(text), typed);
  await editor.waitForFunction(() => window.scaena.last()?.valid, null, { timeout: 30_000 });
  await editor.evaluate(() => window.scaena.save());
  check((await scnOnDisk()) === typed, "the editor's save writes deck.scn as it shows it");
  check((await deckOnDisk()).includes("Revenue doubled again"), "and deck.json, compiled");
  check(!(await editor.evaluate(() => window.scaena.where().dirty)), "and it is as saved");
  await player.waitForFunction((n) => window.scaena.reloads() > n, reloads, { timeout: 30_000 });
  await player.waitForFunction(() => document.querySelector("#reading")?.textContent?.includes("doubled again"), null, { timeout: 30_000 });
  check(true, "the player hears the editor's save and reads the deck again");

  // On disk, with nothing in the editor not saved: it takes it.
  const fromDisk = typed.replace('"Thank you"', '"Thanks for listening"');
  await writeFile(join(bundle, "deck.scn"), fromDisk);
  await editor.waitForFunction((text) => window.scaena.source() === text, fromDisk, { timeout: 30_000 });
  check(!(await editor.evaluate(() => window.scaena.where().dirty)), "the editor takes deck.scn as saved on disk");

  // Over changes not saved, it offers instead, and keeps them.
  const mine = fromDisk.replace('"Thanks for listening"', '"Thanks, all"');
  await editor.evaluate((text) => window.scaena.type(text), mine);
  await writeFile(join(bundle, "deck.scn"), fromDisk.replace('"Thanks for listening"', '"Questions?"'));
  await editor.waitForFunction(() => !document.querySelector("#again")?.hidden, null, { timeout: 30_000 });
  check((await editor.evaluate(() => window.scaena.source())) === mine, "over changes not saved, the editor keeps them");
  const offer = await editor.locator("#again").innerText();
  check(offer.includes("Changed on disk"), `and offers what changed on disk: ${offer}`);
  await editor.locator("#again").click();
  await editor.waitForFunction(() => window.scaena.source().includes('"Questions?"'), null, { timeout: 30_000 });
  check(!(await editor.evaluate(() => window.scaena.where().dirty)), "taken when asked");
} finally {
  await browser.close();
  server.kill();
}

if (failures.length) {
  console.error(`${failures.length} failed`);
  process.exit(1);
}
