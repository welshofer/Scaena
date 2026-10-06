// PLAN 2.61 check: the deck's theme edited in the editor's Theme tab, in headless Chromium
// (serve.mjs), the CPU painting, on the revenue example saved with a history (ADR-0016).
//
//   node web/theme-edit.mjs     (after `just web`; from the repository's root)
//
// - The tab shows the theme the deck names: its colors, each with the color roles that name it;
//   its type roles; and its spacing.
// - A color set by its value, and a role's size, are each one edit of the bundle's copy of the
//   theme: the deck is drawn in it, its source as it was, and the status says so.
// - ⌘Z in the source writes the theme back, the deck drawn as it was; ⇧⌘Z edits it again.
// - A value the theme's schema refuses is refused, with why, the theme as it was.
// - A save records each edit in the history, by the user, and nothing as changed outside Scaena.
// - axe-core finds nothing against WCAG 2.1 AA with the tab shown.
// Exits 1 on any failure.
import { execFileSync } from "node:child_process";
import { rm } from "node:fs/promises";
import { createRequire } from "node:module";
import { launch, serve } from "./serve.mjs";

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};
const scaena = (...args) => execFileSync("cargo", ["run", "-q", "-p", "scaena-cli", "--locked", "--", ...args], { stdio: "inherit" });

// The revenue example, its history begun.
const root = "target/web-theme";
const bundle = `${root}/revenue`;
await rm(root, { recursive: true, force: true });
scaena("save", "docs/examples/revenue.deck.json", "--to", bundle, "--history", "--keep-fonts");

const axe = createRequire(import.meta.url).resolve("axe-core/axe.min.js");
const site = await serve();
const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/${bundle}/deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid, null, { timeout: 120000 });
  // The revenue state, its chart and its headline on the theme's paper.
  await page.selectOption("#state", "revenue");
  await page.waitForFunction(() => window.scaena.at()?.index === 1, null, { timeout: 30000 }).catch(() => {});
  await page.click("#tab-theming");
  await page.waitForFunction(() => window.scaena.theme.theme() !== undefined, null, { timeout: 60000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.locator("#status").textContent();
  const settled = () => page.evaluate(() => window.scaena.theme.settled());
  const theme = () => page.evaluate(() => window.scaena.theme.theme());
  const shot = async () => {
    await page.waitForTimeout(400);
    return page.locator("#stage").screenshot();
  };

  // What the tab shows: the theme's colors, each with the roles that name it; its type roles; its
  // spacing.
  const summary = await page.locator("#theming [data-summary]").textContent();
  check(summary.startsWith("Dusk · themes/dusk.theme.json"), `the theme the deck names: ${summary}`);
  const colors = await page.locator("#theming [data-color]").evaluateAll((els) => els.map((e) => e.dataset.color));
  check(["ink", "paper", "accent"].every((c) => colors.includes(c)), `its colors: ${JSON.stringify(colors)}`);
  const paper = page.locator('#theming [data-color="paper"] input[type="text"]');
  const uses = await page.locator('#theming [data-color="paper"] .uses').textContent();
  check((await paper.inputValue()) === "#101014" && uses.includes("surface"), `paper, #101014, the surface: ${uses}`);
  const roles = await page.locator("#theming [data-roles] tbody tr").evaluateAll((els) => els.map((e) => e.dataset.role));
  check(["display", "headline", "body", "caption"].every((r) => roles.includes(r)), `its type roles: ${JSON.stringify(roles)}`);
  const size = page.getByLabel("headline size", { exact: true });
  const headline = Number(await size.inputValue());
  const family = await page.getByLabel("headline family", { exact: true }).inputValue();
  check(headline > 0 && family === "display", `the headline: ${family} at ${headline}`);
  const gutter = await page.getByLabel("gutter", { exact: true }).inputValue();
  const sides = await page.locator("#theming [data-spacing] label").allTextContents();
  check(gutter === "24" && sides.includes("margin, top and bottom") && sides.includes("space unit"), `its spacing: gutter ${gutter}; ${sides.join(", ")}`);

  // axe-core, with the tab shown.
  await page.addScriptTag({ path: axe });
  const found = await page.evaluate(async () => {
    const result = await window.axe.run(document, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  check(!found.length, `axe finds nothing against WCAG 2.1 AA with the theme shown${found.length ? `:\n    ${found.join("\n    ")}` : ""}`);

  // A color, by its value: one edit, the deck drawn in it, its source as it was.
  const was = await source();
  const before = await shot();
  await paper.fill("#0B0B10");
  await paper.press("Enter");
  await settled();
  await page.waitForFunction(() => window.scaena.theme.theme()?.tokens?.color?.paper === "#0B0B10", null, { timeout: 30000 }).catch(() => {});
  check((await status()).startsWith("theme: paper #0B0B10") && (await status()).includes("⌘Z undoes it"), `the edit, said: ${await status()}`);
  check((await theme())?.tokens?.color?.paper === "#0B0B10" && (await source()) === was, "the theme holds it, and the source is as it was");
  const darker = await shot();
  check(!darker.equals(before), "the deck is drawn in the theme as edited");

  // A role's size.
  await size.fill(String(headline - 4));
  await size.press("Enter");
  await settled();
  await page.waitForFunction((n) => window.scaena.theme.theme()?.type?.roles?.headline?.size === n, headline - 4, { timeout: 30000 }).catch(() => {});
  check((await theme())?.type?.roles?.headline?.size === headline - 4, `the headline's size, ${headline - 4}: ${await status()}`);

  // ⌘Z writes the theme back, an edit at a time; ⇧⌘Z edits it again.
  const press = async (keys, test, arg) => {
    await page.locator(".cm-content").focus();
    await page.keyboard.press(keys);
    await page.waitForFunction(test, arg, { timeout: 30000, polling: 50 }).catch(() => {});
    await settled();
  };
  await press("Control+z", (n) => window.scaena.theme.theme()?.type?.roles?.headline?.size === n, headline);
  check((await theme())?.type?.roles?.headline?.size === headline, "⌘Z puts the headline's size back");
  await press("Control+z", () => window.scaena.theme.theme()?.tokens?.color?.paper === "#101014");
  check((await theme())?.tokens?.color?.paper === "#101014" && (await paper.inputValue()) === "#101014", "⌘Z again, the paper, and the tab shows it");
  check((await shot()).equals(before), "the deck is drawn as it was");
  await press("Control+Shift+z", () => window.scaena.theme.theme()?.tokens?.color?.paper === "#0B0B10");
  check((await theme())?.tokens?.color?.paper === "#0B0B10", "⇧⌘Z edits it again");
  check((await shot()).equals(darker), "and the deck is drawn in it again");

  // A value the theme's schema refuses: refused, with why, the theme as it was.
  await paper.fill("a dark grey");
  await paper.press("Enter");
  await settled();
  const refused = await status();
  check(refused.startsWith("theme not edited: E106"), `a color the theme cannot read is refused, with why: ${refused}`);
  check((await theme())?.tokens?.color?.paper === "#0B0B10" && (await paper.inputValue()) === "#0B0B10", "and the theme, and the tab, are as they were");

  // A save records each edit, by the user, and nothing as changed outside Scaena.
  const saved = await page.evaluate(() => window.scaena.save());
  check(saved?.recorded === true, `the save records the edits in the history: ${await status()}`);
  await page.click("#tab-versions");
  await page.waitForFunction(() => window.scaena.versions.listed()?.length > 1, null, { timeout: 30000 }).catch(() => {});
  const versions = await page.evaluate(() => window.scaena.versions.listed());
  const said = (versions ?? []).map((v) => `${v.author}: ${v.message}`);
  check(said.includes("user: theme_edit: tokens/color/paper") && said.includes("user: theme_edit: type/roles/headline/size"), `the versions list each edit by the user: ${JSON.stringify(said)}`);
  check(!said.some((s) => s.startsWith("fs:")), "and nothing as changed outside Scaena: the editor wrote the theme");
  await context.close();
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  site.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "the editor edits the deck's theme, an edit at a time, each undone with the theme written back");
process.exit(failures.length ? 1 : 0);
