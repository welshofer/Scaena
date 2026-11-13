// Editor dogfood 7: a made deck opened in the editor as a person would, its Story worked end to
// end. The deck is the outlined calibration's `larder`, a seed pitch made by `scaena generate
// --slides --from-outline` with a brief that asks 10 minutes, its history begun by a save.
//   1. The Story read: its score, its findings, and what each row offers.
//   2. Show and Ask tried on the rows that offer them.
//   3. The findings worked down by their own buttons and fields, no assistant: a fix where one is
//      offered, a headline or notes field typed in, else left (only Ask is offered).
//   4. A save, then a slide put back from the first version by Restore This Slide, and ⌘Z.
//   5. A rehearsal from the first slide against the brief's 10 minutes.
// Run from the repository's root, after `just web`:
//   node docs/examples/editor-dogfood-7/walk.mjs [OUTDIR]
import { execFileSync } from "node:child_process";
import { mkdir, rm, writeFile } from "node:fs/promises";
import { launch, serve, tab } from "../../../web/serve.mjs";

const here = new URL(".", import.meta.url).pathname;
const out = process.argv[2] ?? `${here}/run`;
await mkdir(out, { recursive: true });
const log = [];
const note = (...a) => {
  const line = a.map((x) => (typeof x === "string" ? x : JSON.stringify(x))).join(" ");
  console.log(line);
  log.push(line);
};
const scaena = (...args) => execFileSync("cargo", ["run", "-q", "-p", "scaena-cli", "--locked", "--", ...args], { stdio: ["ignore", "pipe", "inherit"] }).toString();

// 0. The deck, made and its history begun.
const root = "target/editor-dogfood-7";
const bundle = `${root}/larder`;
const cal = "docs/examples/health-calibration-outlined";
await rm(root, { recursive: true, force: true });
await mkdir(root, { recursive: true });
scaena(
  "generate", bundle, "Larder seed pitch: meal planning from what is in the fridge",
  "--slides", `${cal}/larder.slides.json`, "--from-outline", `${cal}/larder.outline.json`,
  "--no-look", "--no-notes", "--pictures", "none",
  "--audience", "seed investors", "--goal", "Persuade", "--minutes", "10", "--persona", "investors",
);
scaena("save", bundle, "--history");

const site = await serve();
const browser = await launch();
const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
const page = await context.newPage();
page.on("pageerror", (e) => note("PAGE ERROR", e.message));
page.on("console", (m) => m.type() === "error" && note("CONSOLE", m.text()));
page.on("dialog", (d) => {
  note("DIALOG", d.message());
  void d.accept();
});
const health = () => page.evaluate(() => window.scaena.story.health());
const said = (h) => (h?.findings ?? []).map((f) => `${f.severity[0]} ${f.kind} s${f.slide}: ${f.says}${f.fix ? ` | FIX: ${f.fix.says}` : ""}${f.field ? ` | FIELD: ${f.field}` : ""}`);
const score = (h) => ({ score: h?.score, pacing: h?.pacing?.score, balance: h?.balance?.score, fidelity: h?.fidelity?.score, findings: h?.findings?.length });
const status = () => page.textContent("#status");
const reread = async (was) => {
  await page.waitForFunction((b) => JSON.stringify(window.scaena.story.health()) !== b, JSON.stringify(was), { timeout: 30000 }).catch(() => note("  (health did not read again)"));
  await page.waitForTimeout(1200);
  return health();
};
/** What each finding's row offers, by its buttons' words. */
const rows = () =>
  page.evaluate(() => {
    document.querySelectorAll("#story [data-more] details").forEach((d) => (d.open = true));
    return [...document.querySelectorAll("#story [data-findings] [data-slide]")].filter((li) => li.querySelector("button[data-ask], button[data-fix], button[data-field], button[data-show]")).map((li) => ({
      slide: li.dataset.slide,
      kind: li.dataset.kind,
      says: (li.querySelector("p, .says")?.textContent ?? li.textContent).trim().slice(0, 160),
      buttons: [...li.querySelectorAll("button")].map((b) => `${b.textContent.trim()}${Object.keys(b.dataset).length ? ` [${Object.keys(b.dataset).join(",")}]` : ""}`),
    }));
  });

try {
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/${bundle}/deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid, null, { timeout: 180000 });

  // 1. The Story read.
  await tab(page, "story");
  let h = await health();
  for (let k = 0; !h && k < 30; k++) {
    await page.waitForTimeout(1000);
    h = await health();
  }
  note("1. health:", score(h));
  for (const l of said(h)) note("   ", l);
  await writeFile(`${out}/01-story.txt`, await page.evaluate(() => document.querySelector("#story")?.innerText ?? ""));
  await page.screenshot({ path: `${out}/01-story.png` });
  const offered = await rows();
  note("1. rows:");
  for (const r of offered) note("   ", `s${r.slide} ${r.kind}:`, r.buttons.join(" · "));

  // 2. Show and Ask.
  const show = page.locator("#story [data-findings] li button[data-show]");
  const shows = await show.count();
  for (let k = 0; k < Math.min(shows, 3); k++) {
    const slide = await show.nth(k).evaluate((b) => b.closest("li")?.dataset.slide);
    await show.nth(k).click();
    await page.waitForTimeout(1200);
    const at = await page.evaluate(() => ({ index: window.scaena.at().index, tab: document.querySelector('[role="tab"][aria-selected="true"]')?.textContent?.trim() }));
    note(`2. Show on slide ${slide}'s row:`, at, "| status:", await status());
    if (k === 0) await page.screenshot({ path: `${out}/02-show.png` });
    await tab(page, "story");
  }
  const ask = page.locator("#story [data-findings] li button[data-ask]").first();
  if (await ask.count()) {
    await ask.click();
    await page.waitForTimeout(1000);
    note("2. Ask:", {
      question: (await page.inputValue("#question").catch(() => "")).slice(0, 300),
      focused: await page.evaluate(() => document.activeElement?.id),
      status: await status(),
    });
    await page.screenshot({ path: `${out}/02-ask.png` });
    await page.fill("#question", "").catch(() => {});
    await tab(page, "story");
  }

  // 3. The findings worked down by their own buttons and fields.
  const journal = [];
  const tried = new Set();
  for (let k = 0; k < 14; k++) {
    h = await health();
    const open = (h?.findings ?? []).filter((f) => !tried.has(`${f.kind} ${f.state} ${f.says}`));
    if (!open.length) break;
    const f = open[0];
    tried.add(`${f.kind} ${f.state} ${f.says}`);
    const before = score(h);
    await page.evaluate(() => document.querySelectorAll("#story [data-more] details").forEach((d) => (d.open = true)));
    // The finding's own row: the one under its slide that says its words.
    const words = f.says.slice(0, 40).replace(/[“”]/g, "");
    const rowsOf = page.locator(`#story [data-findings] [data-slide="${f.slide}"]`).filter({ hasText: words });
    const at = (await rowsOf.count()) ? rowsOf.first() : page.locator(`#story [data-findings] [data-slide="${f.slide}"]`).first();
    const was = await page.evaluate(() => window.scaena.source());
    let how;
    try {
      if (f.fix && (await at.locator("button[data-fix]").count())) {
        how = `fix "${f.fix.says}"`;
        await at.locator("button[data-fix]").first().click();
      } else if (await at.locator("button[data-lint-fix]").count()) {
        how = `lint fix "${(await at.locator("button[data-lint-fix]").first().textContent()).trim()}"`;
        await at.locator("button[data-lint-fix]").first().click();
      } else if (f.field === "headline" && (await at.locator('button[data-field="headline"]').count())) {
        const words = f.ask?.match(/say “([^”]+)”/)?.[1] ?? f.says.match(/“([^”]+)”/)?.[1] ?? "Families on Larder waste a third less food";
        how = `headline field, typed "${words}"`;
        await at.locator('button[data-field="headline"]').first().click();
        await page.waitForTimeout(1500);
        await page.keyboard.type(words);
        await page.waitForTimeout(800);
        await page.keyboard.press("Escape");
      } else if (await at.locator('button[data-field="notes"]').count()) {
        how = "notes field, typed a line of notes";
        await at.locator('button[data-field="notes"]').first().click();
        await page.waitForTimeout(1500);
        await page.keyboard.press("Control+End");
        await page.keyboard.type(" Families tell us the savings show on the first receipt, and that is why they stay.");
        await page.keyboard.press("Tab");
        await page.waitForTimeout(1000);
        await tab(page, "story");
      } else if (await at.locator("button[data-fold]").count()) {
        how = `fold "${(await at.locator("button[data-fold]").first().textContent()).trim()}"`;
        await at.locator("button[data-fold]").first().click();
      } else {
        how = `left: the row offers ${(await at.locator("button").allTextContents()).map((t) => t.trim()).join(" · ") || "nothing"}`;
      }
    } catch (e) {
      how = `${how} FAILED: ${e.message.split("\n")[0]}`;
      await tab(page, "story").catch(() => {});
    }
    if (!how.startsWith("left")) {
      await page.waitForFunction((s) => window.scaena.source() !== s, was, { timeout: 15000 }).catch(() => {});
      await page.waitForTimeout(1500);
    }
    const offer = await page.evaluate(() => {
      const o = document.querySelector("#offer");
      return o && !o.hidden ? o.textContent : "";
    });
    if (offer && (await page.locator("#restate:not([hidden])").count())) {
      await page.click("#restate");
      await page.waitForTimeout(1200);
    }
    const changed = (await page.evaluate(() => window.scaena.source())) !== was;
    const after = changed ? await reread(h) : h;
    const gone = !(after?.findings ?? []).some((g) => g.kind === f.kind && g.state === f.state);
    const entry = {
      finding: `${f.kind} s${f.slide}: ${f.says}`,
      how,
      offer,
      changed,
      gone,
      status: await status(),
      answered: await page.locator("#story [data-answered]").textContent().catch(() => ""),
      before,
      after: score(after),
    };
    journal.push(entry);
    note("3.", entry);
  }
  await writeFile(`${out}/03-journal.json`, JSON.stringify(journal, null, 1));
  await writeFile(`${out}/03-story.txt`, await page.evaluate(() => document.querySelector("#story")?.innerText ?? ""));
  await page.screenshot({ path: `${out}/03-story.png` });

  // 4. Saved, then a slide put back from the first version by Restore This Slide, and ⌘Z.
  const saved = await page.evaluate(() => window.scaena.save());
  note("4. saved:", saved?.recorded, "|", await status());
  await tab(page, "versions");
  await page.waitForFunction(() => window.scaena.versions.listed()?.length > 1, null, { timeout: 60000 }).catch(() => note("  (no second version)"));
  const versions = await page.evaluate(() => window.scaena.versions.listed());
  note("4. versions:", versions?.map((v) => `${v.n} ${v.said} · ${v.by}`));
  if (versions?.length) {
    await page.click(`#versions [data-id="${versions[0].id}"]`);
    await page.evaluate(() => window.scaena.versions.settled());
    const changes = await page.evaluate(() => window.scaena.versions.changes());
    note("4. version 1 against the deck now:", changes);
    // The first slide the changes name, else the second slide.
    const named = changes.map((l) => l.match(/^Slide (\d+)/)?.[1]).find(Boolean);
    const options = await page.locator("#versions [data-state] option").evaluateAll((o) => o.map((x) => ({ value: x.value, label: x.textContent })));
    const pick = options.find((o) => o.label === `Slide ${named}`) ?? options[1];
    await page.selectOption("#versions [data-state]", pick.value);
    await page.evaluate(() => window.scaena.versions.settled());
    note("4. Restore This Slide offers:", await page.locator("#versions [data-restore-slide]").getAttribute("aria-description"));
    const before = await page.evaluate(() => window.scaena.source());
    await page.click("#versions [data-restore-slide]");
    await page.evaluate(() => window.scaena.versions.settled());
    await page.waitForFunction((s) => window.scaena.source() !== s, before, { timeout: 20000 }).catch(() => {});
    await page.waitForTimeout(1000);
    note("4. restored:", await status());
    await writeFile(`${out}/04-before.scn`, before);
    await writeFile(`${out}/04-after.scn`, await page.evaluate(() => window.scaena.source()));
    await page.screenshot({ path: `${out}/04-restored.png` });
    await page.locator(".cm-content").focus();
    await page.keyboard.press("Control+z");
    await page.waitForFunction((s) => window.scaena.source() === s, before, { timeout: 20000 }).catch(() => {});
    note("4. ⌘Z:", (await page.evaluate(() => window.scaena.source())) === before ? "the deck as before the restore" : "NOT as before", "|", await status());
  }

  // 5. A rehearsal from the first slide against the brief's 10 minutes.
  await page.locator("#strip li[data-state]").first().click();
  await page.waitForTimeout(800);
  await page.click("#rehearse-open");
  await page.waitForFunction(() => !document.querySelector("#rehearsing").hidden, null, { timeout: 10000 }).catch(() => note("  (rehearsal bar did not show)"));
  const states = await page.evaluate(() => window.scaena.last().states.length);
  for (let k = 0; k < states; k++) {
    await page.waitForTimeout(2500);
    if (k === 1) note("5. bar:", await page.locator("#rehearsing").innerText().catch(() => ""), "| notes:", (await page.locator("#rehearsal-notes").innerText().catch(() => "")).slice(0, 200));
    await page.keyboard.press("ArrowRight");
  }
  await page.waitForFunction(() => document.querySelector("#rehearsed")?.open, null, { timeout: 10000 }).catch(async () => {
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => document.querySelector("#rehearsed")?.open, null, { timeout: 10000 }).catch(() => note("  (no rehearsal results)"));
  });
  note("5. plan:", await page.textContent("#rehearsed [data-plan]").catch(() => ""));
  await page.locator("#rehearsed").screenshot({ path: `${out}/05-rehearsed.png` }).catch(() => {});
  await page.locator("#rehearsed button[value=cancel]").click().catch(() => {});
} catch (e) {
  note("WALK FAILED:", e.message);
} finally {
  await writeFile(`${out}/walk.log`, log.join("\n") + "\n");
  await browser.close();
  site.close();
}
