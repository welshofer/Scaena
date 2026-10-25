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
} catch (e) {
  failures.push(`error: ${e.message}`);
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "the Story tab leads with what to do first, and folds the rest");
process.exit(failures.length ? 1 : 0);
