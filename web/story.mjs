// lift-up S1 check: the Story tab leads with what to do first, in headless Chromium (serve.mjs), on
// the higher-ed example, whose story health finds 25 things to look at.
//
//   node web/story.mjs     (after `just web`; from the repository's root)
//
// - The curve's legend keeps each mark with its words on one line (editor dogfood 2).
// - Each finding says what to do in its row, under what it found: the question the assistant
//   would be asked, so a presenter with no key is not sent to an Assistant tab to read it
//   (editor dogfood 3).
// - At most five findings stand open, none of them an info: the warnings whose measure scores
//   lowest. The rest, the infos among them, fold under one disclosure that counts them by kind
//   ("20 more: 12 arc, …"), shut until it is opened, and opened it shows each of them, so every
//   finding is there once.
// - A turn's bridge written in the Format tab's Notes field, as its finding asks: the field shows
//   the notes the presenter reads (the beat's, on this made deck), the presenter then reads the
//   bridge before them, and the turn is answered (editor dogfood 3).
// - A slide that lint finds broken leads: its support given more words than its place holds, the
//   Story tab's first row is that slide's error, the disclosure left open as it was; the row
//   offers lint's fix and Show it, which opens the slide's mark on the canvas, in focus, and the
//   fix taken mends the slide (editor dogfood 4).
// - A row names its slide by the headline the deck has now: retyped, the row says it, though
//   health still names the slide by its beat's claim (editor dogfood 4).
// - A figure no source backs, in a deck made from sources (W1's dogfood 5): its row's Cite a
//   source… offers the deck's sources, the one chosen joins the slide's beat's evidence, one
//   patch, the finding goes, the status says it, and ⌘Z takes it back.
// - On the revenue example: a new headline typed over the cover's, which says the opening beat's
//   claim, is offered beside the status once the burst of typing ends ("Slide 1's story still says
//   “…”. Update it to “…”?"), with Update and Keep, and the offer stands whatever the status says
//   after it, Escape's words and what lint finds; Update restates the claim and the offer goes; an
//   undo of the typing takes the offer away; Keep lets it be and changes nothing (lift-up FUNC-2).
// - On dogfood 9's keynote, every slide a row's words name is a button that shows it, the pacing row's
//   "slides 1, 5 and 11" each; and a read again after slide 5's notes are deepened keeps the open
//   rows where they stood, the pacing row's though it moves to another slide, what is new after
//   them (editor dogfood 6).
// - A made deck's plan (dogfood 9's keynote, made with no call): a slide planned hotter than its
//   place is asked by its Intensity; the Format tab offers its beat's Intensity and Tone, each said
//   in the engine's words as it is held, nothing made; let go, one patch, the bend answered; ⌘Z
//   takes it back (editor dogfood 3). A slide there that lint finds broken caps the score: the
//   score says so and what it would be, the slide a button that shows it (W1's dogfood 5).
// Exits 1 on any failure.
import { execFileSync } from "node:child_process";
import { mkdir, rm, writeFile } from "node:fs/promises";
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

  // The curve's legend: each mark with its words on one line, a narrow column wrapping between
  // keys, never between a mark and what it means (editor dogfood 2).
  const keys = await page.evaluate(() =>
    [...document.querySelectorAll("#story [data-legend] > span")].map((k) => ({ words: k.textContent.trim(), lines: new Set([...k.getClientRects()].map((r) => Math.round(r.top))).size })),
  );
  check(keys.length === 9 && keys.every((k) => k.lines === 1), `the legend's keys each stand on one line: ${JSON.stringify(keys.filter((k) => k.lines !== 1).map((k) => k.words))} of ${keys.length}`);
  // Each in a person's words: a slide off the arc's curve, and one outside its part of the talk
  // (W2's editor dogfood).
  const legend = keys.map((k) => k.words);
  check(legend.includes("off the curve") && legend.includes("outside its part of the talk"), `the legend says what a mark means in a person's words: ${JSON.stringify(legend)}`);

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

  // Ask the assistant keeps a question already drafted in its box, the finding's put after it
  // (lift-up 20261110 deferred).
  await tab(page, "assistant");
  await page.fill("#question", "My own question first.");
  await tab(page, "story");
  await page.locator("#story [data-findings] li[data-slide] [data-ask]").first().click();
  // Its slide is shown first, then the question put in the box.
  await page.waitForFunction(() => document.querySelector("#question").value !== "My own question first.", null, { timeout: 10000 }).catch(() => {});
  const drafted = await page.inputValue("#question");
  check(/^My own question first\.\n\n\S/.test(drafted), `Ask the assistant keeps the question drafted, its own after it: ${JSON.stringify(drafted.slice(0, 60))}`);
  // With no key kept, Ask says what to do: the key's field takes the focus and the status says
  // so, the question waiting; pressing Ask then says the same, keeping it (editor dogfood 5).
  check(await page.evaluate(() => !document.querySelector("#key").value), "no key is kept here");
  check(await page.evaluate(() => document.activeElement?.id === "key"), `the key's field takes the focus: ${await page.evaluate(() => document.activeElement?.id)}`);
  check(
    await page.evaluate(() => document.querySelector("#status").textContent.startsWith("Paste a key to ask this")),
    `the status says what to do: ${await page.evaluate(() => document.querySelector("#status").textContent)}`,
  );
  await page.click("#ask");
  const failed = await page
    .waitForFunction(() => document.querySelector("#transcript li.failed:last-child")?.textContent, null, { timeout: 10000 })
    .then((h) => h.jsonValue())
    .catch(() => "");
  check(/^Paste a key to ask this: paste \w+'s above\.$/.test(failed), `pressing Ask with no key says the same: ${failed}`);
  check((await page.inputValue("#question")) === drafted, "and keeps the question");
  check(await page.evaluate(() => document.activeElement?.id === "key"), "the key's field in focus");
  await page.fill("#question", "");
  await tab(page, "story");

  // Read again with nothing changed, the score's live region is left as it was, so nothing is
  // said again (lift-up 20261110 deferred).
  await page.evaluate(() => {
    window.__said = 0;
    new MutationObserver(() => window.__said++).observe(document.querySelector("#story [data-score]"), { childList: true, subtree: true, characterData: true });
  });
  for (let i = 0; i < 2; i++) {
    await page.evaluate(() => {
      window.__read = window.scaena.story.health();
      window.scaena.story.refresh();
    });
    await page.waitForFunction(() => window.scaena.story.health() !== window.__read, null, { timeout: 30000 }).catch(() => {});
  }
  check((await page.evaluate(() => window.__said)) === 0, `read again unchanged, the score is not said again: ${await page.evaluate(() => window.__said)} changes`);

  // A source that does not compile for a moment keeps the story as last read, said to be so
  // (lift-up 20261110 deferred); generate.mjs holds its fixes, on a deck that offers one.
  const compiled = await page.evaluate(() => window.scaena.source());
  const moved = compiled;
  const shown = await page.locator("#story [data-findings] li[data-slide]").count();
  await page.evaluate((s) => window.scaena.type(`${s}\nstate {{{`), moved);
  // The editor's own compile of it lands first, as it does on a slower machine: the worker then
  // holds the source among those compiled, and must still read no story of it.
  await page.waitForFunction(() => window.scaena.last()?.valid === false, null, { timeout: 30000 }).catch(() => {});
  // Shown again, the Story reads the source at once, as it does a pause after a burst of typing
  // that ends mid-word.
  await tab(page, "inspector");
  await tab(page, "story");
  await page.waitForFunction(() => !document.querySelector("#story > p[data-stale]").hidden, null, { timeout: 60000 }).catch(() => {});
  const asRead = await page.evaluate(() => ({
    said: document.querySelector("#story > p[data-stale]").textContent,
    rows: document.querySelectorAll("#story [data-findings] li[data-slide]").length,
    curve: !!document.querySelector("#story [data-arc] svg"),
    label: document.querySelector("#story [data-findings]").getAttribute("aria-label"),
  }));
  check(
    asRead.said === "Showing the story as last read: the source has an error to fix" && asRead.rows === shown && asRead.curve && asRead.label.endsWith(", as last read"),
    `a source that does not compile keeps the story as last read, and says so: ${JSON.stringify(asRead)}`,
  );
  await page.evaluate((s) => window.scaena.type(s), compiled);
  await page.waitForFunction(() => document.querySelector("#story > p[data-stale]").hidden, null, { timeout: 60000 }).catch(() => {});
  check(await page.evaluate(() => document.querySelector("#story > p[data-stale]").hidden), "compiled again, it reads anew");
  await page.evaluate(() => {
    const more = document.querySelector("#story [data-more] details");
    if (more) more.open = true;
  });
  await tab(page, "story");
  // Each finding says what to do under what it found, the question the assistant would be asked,
  // so a presenter with no key reads it in its row (editor dogfood 3).
  const todos = await page.evaluate(() =>
    [...document.querySelectorAll("#story [data-findings] li[data-slide] p[data-kind]")].map((p) => {
      const next = p.nextElementSibling;
      return { slide: p.closest("li").dataset.slide, kind: p.dataset.kind, todo: next?.matches("p[data-todo]") ? next.textContent : null, shown: !!next?.checkVisibility?.() };
    }),
  );
  // In a person's words: each figure health gives the assistant said by its word, as the row says it.
  const spoken = await page.evaluate((fs) => fs.map((f) => window.scaena.story.spoken(f.ask)), findings);
  const asks = findings.map((f, i) => `${f.slide} ${f.kind} To do: ${spoken[i]}`).sort();
  check(todos.every((t) => !/\d\.\d\d/.test(t.todo ?? "")), "each in a person's words, no figure");
  check(todos.every((t) => t.todo && t.shown), `each finding says what to do under it: ${JSON.stringify(todos.filter((t) => !t.todo || !t.shown).slice(0, 2))}`);
  check(
    JSON.stringify(todos.map((t) => `${t.slide} ${t.kind} ${t.todo}`).sort()) === JSON.stringify(asks),
    `what it says to do is each finding's ask, its figures said by their words, once: ${todos.length} of ${findings.length}`,
  );

  // A turn's bridge, written in the Format tab's Notes field as its finding asks: the field shows
  // the notes the presenter reads, the beat's on this made deck, and what is typed there is what
  // the presenter reads and what health judges, so the turn is answered (editor dogfood 3).
  const turn = findings.find((f) => f.kind === "turn");
  check(!!turn, `a turn its words do not make: ${turn?.says}`);
  const read = await page.evaluate((s) => window.scaena.notes(s), turn.state);
  check(read.text.length > 0, `the presenter reads its beat's notes: ${read.text.slice(0, 40)}`);
  const before = await page.evaluate((k) => window.scaena.story.health().slides[k - 2].title, turn.slide);
  await page.click(`#story [data-findings] li[data-slide="${turn.slide}"] [data-show]`);
  await page.waitForTimeout(1000);
  await tab(page, "inspector");
  const field = page.locator('textarea[data-prop="notes"]');
  await field.waitFor({ timeout: 30000 });
  check((await field.inputValue()) === read.text, `the Notes field shows what the presenter reads: ${JSON.stringify((await field.inputValue()).slice(0, 40))}`);
  const bridge = `From ${before}, we turn.`;
  await field.fill(`${bridge}\n${read.text}`);
  await field.blur();
  await page.waitForFunction((t) => window.scaena.notes(t.state).then((n) => n.text.startsWith(t.bridge)), { state: turn.state, bridge }, { timeout: 30000 }).catch(() => {});
  const after = await page.evaluate((s) => window.scaena.notes(s), turn.state);
  check(after.text === `${bridge}\n${read.text}`, `the presenter reads the bridge, then the beat's notes: ${after.text.slice(0, 60)}`);
  // Notes draw on no slide: the status says the edit is this slide's, never "on 0 slides" (W1's
  // dogfood 5).
  const noted = await page.textContent("#status");
  check(/on this slide/.test(noted) && !/on 0 /.test(noted), `the notes edit is said as this slide's: ${noted}`);
  await tab(page, "story");
  await page
    .waitForFunction((s) => !window.scaena.story.health()?.findings.some((f) => f.kind === "turn" && f.state === s), turn.state, { timeout: 60000 })
    .catch(() => {});
  const still = await page.evaluate((s) => window.scaena.story.health().findings.filter((f) => f.kind === "turn" && f.state === s), turn.state);
  check(still.length === 0, `and the turn is answered: ${JSON.stringify(still.map((f) => f.says))}`);

  // A row names its slide by its headline as the deck reads now, not by its beat's claim, which
  // health names it by: a headline retyped is the row's at once (editor dogfood 4).
  const named = await page.evaluate(() => {
    const row = document.querySelector("#story [data-findings] li[data-slide]");
    const slide = Number(row.dataset.slide);
    const state = window.scaena.story.health().slides[slide - 1].state;
    return { slide, headline: window.scaena.last().slots.find((s) => s.state === state)?.headline, shown: row.querySelector(".headline").textContent };
  });
  check(named.headline && named.shown === named.headline, `a row is named by its slide's headline: ${JSON.stringify(named)}`);
  const retitled = `${named.headline} Today`;
  const was = await page.evaluate(() => window.scaena.source());
  const at = was.indexOf(JSON.stringify(named.headline));
  check(at >= 0, `the headline is found in the source: ${named.headline}`);
  await page.evaluate((s) => window.scaena.type(s), was.slice(0, at) + JSON.stringify(retitled) + was.slice(at + JSON.stringify(named.headline).length));
  const renamed = await page
    .waitForFunction(
      ([slide, words]) => document.querySelector(`#story [data-findings] li[data-slide="${slide}"] .headline`)?.textContent === words,
      [named.slide, retitled],
      { timeout: 60000 },
    )
    .then(() => true)
    .catch(() => false);
  check(renamed, `the headline retyped, its row says it: ${retitled}`);
  await page.evaluate((s) => window.scaena.type(s), was);
  await page.waitForFunction((s) => window.scaena.source() === s && window.scaena.last()?.valid, was, { timeout: 60000 }).catch(() => {});

  // A slide that lint finds broken leads.
  const source = await page.evaluate(() => window.scaena.source());
  // Long enough to run past its place, short enough that lint's fix shrinks it to fit.
  const long = "Comprehensive was the aspiration, and every campus grew to meet it. ".repeat(5).trim();
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
  // Its severity is said in a word and drawn as a shape, not by its colour alone (lift-up 20261110
  // USE-1, WCAG 1.4.1): its button is named "Error: Slide N", and each severity's shape differs.
  const marked = await page.evaluate(() =>
    [...document.querySelectorAll("#story [data-findings] li[data-slide]")].map((li) => {
      const show = li.querySelector("[data-show]");
      return { severity: li.dataset.severity, name: show.ariaLabel, text: show.textContent, shape: getComputedStyle(show, "::before").content };
    }),
  );
  const word = { error: "Error", warning: "Warning", info: "Note" };
  check(
    marked.every((m) => m.name === `${word[m.severity]}: ${m.text}` && m.shape !== "none" && m.shape !== "normal"),
    `each finding's slide is named by its severity, and drawn with a shape: ${JSON.stringify(marked.slice(0, 3))}`,
  );
  const shapes = new Map(marked.map((m) => [m.severity, m.shape]));
  check(new Set(shapes.values()).size === shapes.size && shapes.size >= 2, `each severity its own shape: ${JSON.stringify([...shapes])}`);
  check(await page.evaluate(() => document.querySelector("#story [data-more] details")?.open === true), "the disclosure stays open as it was left");
  // The broken slide's row offers what its mark on the canvas offers (editor dogfood 4): lint's fix,
  // and Show it, which opens the slide with its problem's mark open.
  const offers = await page.evaluate(() => {
    const row = document.querySelector("#story [data-findings] > li[data-slide]");
    return {
      fixes: [...row.querySelectorAll("[data-lint-fix]")].map((b) => [b.dataset.lintFix, b.textContent]),
      show: Boolean(row.querySelector("[data-lint-show]")),
    };
  });
  check(offers.fixes.length > 0 && offers.show, `the broken slide's row offers lint's fix and Show it: ${JSON.stringify(offers)}`);
  // A source that stops compiling holds lint's fix with the rest, the story kept as last read.
  await page.evaluate((s) => window.scaena.type(`${s}\nstate {{{`), broken);
  // The editor's own compile of it lands first, as it does on a slower machine: the worker then
  // holds the source among those compiled, and must still read no story of it.
  await page.waitForFunction(() => window.scaena.last()?.valid === false, null, { timeout: 30000 }).catch(() => {});
  await tab(page, "inspector");
  await tab(page, "story");
  await page.waitForFunction(() => !document.querySelector("#story > p[data-stale]").hidden, null, { timeout: 60000 }).catch(() => {});
  const heldFix = await page.evaluate(() => document.querySelector("#story [data-findings] > li[data-slide] [data-lint-fix]")?.disabled);
  check(heldFix === true, `while the source does not compile, lint's fix is held: ${heldFix}`);
  // Taken back by ⌘Z, the deck is as it was, its fix offered again.
  await page.focus("#overlay");
  await page.keyboard.press("Control+z");
  await page.waitForFunction((s) => window.scaena.source() === s, broken, { timeout: 30000 }).catch(() => {});
  await page.waitForFunction(() => document.querySelector("#story > p[data-stale]").hidden, null, { timeout: 60000 }).catch(() => {});
  // The Story's read may come before every state is linted: it is read again once that lint
  // lands, so nothing more than waiting brings the fix back (lift-up 20261110 deferred).
  await page
    .waitForFunction(() => document.querySelector("#story [data-findings] > li[data-slide] [data-lint-fix]")?.disabled === false, null, { timeout: 120000 })
    .catch(() => {});
  check(
    await page.evaluate(() => document.querySelector("#story [data-findings] > li[data-slide] [data-lint-fix]")?.disabled === false),
    "taken back, lint's fix is offered again",
  );
  await page.click("#story [data-findings] > li[data-slide] [data-lint-show]");
  const popped = await page
    .waitForFunction(() => document.querySelector("#marked")?.matches(":popover-open") && document.querySelector("#marked").textContent, null, { timeout: 30000 })
    .then((h) => h.jsonValue())
    .catch(() => "");
  check(/fit|room|box/i.test(popped), `Show it opens the slide's mark, saying its problem: ${popped.slice(0, 120)}`);
  const markFixes = await page.evaluate(() => [...document.querySelectorAll("#marked [data-fix]")].map((b) => b.textContent));
  check(
    markFixes.length > 0 && markFixes.every((f) => offers.fixes.some(([, says]) => says === f)),
    `the row offers the fix its mark offers: ${JSON.stringify(markFixes)} in ${JSON.stringify(offers.fixes)}`,
  );
  check(
    await page.evaluate(() => document.querySelector("#marked")?.contains(document.activeElement)),
    "the mark it opens takes the focus",
  );
  await page.keyboard.press("Escape");
  await page.click(`#story [data-findings] > li[data-slide] [data-lint-fix]`);
  await page
    .waitForFunction(() => !window.scaena.story.health()?.findings.some((f) => f.kind === "layout"), null, { timeout: 120000 })
    .catch(() => {});
  const mended = await page.evaluate(() => window.scaena.story.health()?.findings.filter((f) => f.kind === "layout").map((f) => f.says));
  check(mended?.length === 0, `its fix taken, the slide stands: ${JSON.stringify(mended)}`);

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

    // The next change to the source takes the offer away: the typing that left the claim behind,
    // undone, the headline saying the claim again.
    const before = await source();
    await retitle("This quarter rebuilt the business.");
    check(await offered(), "a headline typed again over the claim's words is offered again");
    await page.keyboard.press("Escape");
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
    await page.waitForFunction((s) => window.scaena.source() === s, before, { timeout: 30000, polling: 50 }).catch(() => {});
    check((await source()) === before, "an undo takes the typing back");
    const undone = await offer();
    check(!undone.shown && !undone.update && undone.keep === undefined, `and the offer with it: ${JSON.stringify(undone)}`);

    // Keep lets the claim be: the offer goes, the source as it was. The headline and the claim then
    // part, so the next edit of the headline offers nothing: only a claim the slide said is offered.
    await retitle("This quarter remade the business.");
    check(await offered(), "a third headline over the claim's words is offered too");
    await page.keyboard.press("Escape");
    const kept = await source();
    await page.click("#keep-claim");
    await page.waitForTimeout(500);
    const gone = await offer();
    check(!gone.shown && !gone.update && gone.keep === undefined, `Keep lets it be, and the offer goes: ${JSON.stringify(gone)}`);
    check((await source()) === kept, "Keep changes nothing in the source");
  })();

  // A made deck's plan, editable (editor dogfood 3): dogfood 9's keynote, made from its slides
  // with no call, plans slide 2 hotter than its place. Its finding names the Intensity; with
  // nothing selected the Format tab offers the beat's plan, its Intensity and Tone, each said in
  // the engine's words as it is held, nothing made; let go, one patch, the bend answered and the
  // score read again; ⌘Z takes it back.
  await (async () => {
    const bundle = "target/web-story/keynote";
    await rm(bundle, { recursive: true, force: true });
    execFileSync(
      "cargo",
      ["run", "-q", "-p", "scaena-cli", "--locked", "--", "generate", bundle, "Taking the library to where people are",
        "--slides", "docs/examples/dogfood-9/keynote.slides.json", "--from-outline", "docs/examples/dogfood-9/keynote.outline.json",
        "--no-look", "--pictures", "none", "--audience", "the library board and city council", "--goal", "Persuade",
        "--length", "11-15 slides", "--modality", "keynote"],
      { stdio: "inherit" },
    );
    const page = await context.newPage();
    page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
    page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
    await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&source=open&bundle=/${bundle}/deck.json`);
    await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });
    await tab(page, "story");
    await page.waitForFunction(() => window.scaena.story.health(), null, { timeout: 120000 });
    const bend = await page.evaluate(() => window.scaena.story.health().findings.find((f) => f.kind === "arc" && f.says.includes("planned hotter")));
    check(!!bend && bend.ask.includes("by its Intensity"), `a slide planned hotter than its place, asked by its Intensity: ${bend?.ask}`);
    // Its row says the plan, and the place's call, by the ladder's words, never a figure.
    const todo = await page.evaluate((a) => window.scaena.story.spoken(a), bend.ask);
    check(todo.includes("planned at Peak") && /\((Calm|Steady|Building|Intense|Peak)\)/.test(todo) && !/\d\.\d\d/.test(todo), `its to-do says the plan by its word: ${todo}`);
    const place = await page.evaluate((k) => window.scaena.story.health().slides[k - 1].curve, bend.slide);
    await page.click("#story [data-more] summary").catch(() => {});
    await page.click(`#story [data-findings] li[data-slide="${bend.slide}"] [data-show]`);
    await page.waitForTimeout(1000);
    await tab(page, "inspector");
    const slider = page.locator("#look-plan-intensity");
    await slider.waitFor({ timeout: 30000 });
    const readout = page.locator("#look-plan-intensity").locator("xpath=../following-sibling::span[1]");
    check((await slider.inputValue()) === "0.8" && (await readout.textContent()) === "Peak", `its Intensity, planned: ${await slider.inputValue()}, ${await readout.textContent()}`);
    check((await page.locator("#look-plan-tone").count()) === 1, "and its Tone");
    const was = await page.evaluate(() => window.scaena.source());
    const to = (Math.round(place * 20) / 20).toFixed(2);
    await slider.evaluate((el, v) => {
      el.value = v;
      el.dispatchEvent(new Event("input", { bubbles: true }));
    }, to);
    const held = await readout.textContent();
    check(held === "Building" && (await slider.getAttribute("aria-valuetext")) === held, `held at ${to}, it says its word: ${held}`);
    check((await page.evaluate(() => window.scaena.source())) === was, "and nothing is made while it is held");
    await slider.evaluate((el) => el.dispatchEvent(new Event("change", { bubbles: true })));
    await page.waitForFunction((w) => window.scaena.source() !== w, was, { timeout: 30000 }).catch(() => {});
    const said = await page.locator("#status").textContent();
    check(said.startsWith(`Slide ${bend.slide}: Intensity Building`) && said.includes("⌘Z"), `let go, one patch, said: ${said}`);
    await tab(page, "story");
    await page
      .waitForFunction((s) => !window.scaena.story.health()?.findings.some((f) => f.kind === "arc" && f.state === s), bend.state, { timeout: 60000 })
      .catch(() => {});
    const after = await page.evaluate((k) => window.scaena.story.health().slides[k - 1].declared, bend.slide);
    const still = await page.evaluate((s) => window.scaena.story.health().findings.some((f) => f.kind === "arc" && f.state === s), bend.state);
    check(Math.abs(after - Number(to)) < 1e-9 && !still, `planned for its place (${after}), it bends no more`);
    await page.locator(".cm-content").focus();
    await page.keyboard.press("Control+z");
    await page.waitForFunction((w) => window.scaena.source() === w, was, { timeout: 30000, polling: 50 }).catch(() => {});
    check((await page.evaluate(() => window.scaena.source())) === was, "⌘Z takes the plan back");

    // Every slide a row's words name is a button that shows it (editor dogfood 6): the pacing row's
    // "Deepen the notes of slides 1, 5 and 11" reaches each of the three, not its own slide alone.
    await tab(page, "story");
    // Read again since the plan came back: its bend is found again.
    await page.waitForFunction((st) => window.scaena.story.health()?.findings.some((f) => f.kind === "arc" && f.state === st), bend.state, { timeout: 60000 }).catch(() => {});
    await page.click("#story [data-more] summary").catch(() => {});
    const pacing = await page.evaluate(() => window.scaena.story.health().findings.find((f) => f.kind === "pacing"));
    const named = [...(pacing?.ask.match(/slides ([\d, and]+), the shortest/)?.[1] ?? "").matchAll(/\d+/g)].map((m) => Number(m[0]));
    const linked = await page.evaluate(
      (k) => [...document.querySelectorAll(`#story [data-findings] li[data-slide="${k}"] p[data-todo="pacing"] [data-to-slide]`)].map((b) => [b.textContent, b.dataset.toSlide]),
      pacing?.slide,
    );
    const states = await page.evaluate(() => window.scaena.story.health().slides.map((s) => s.state));
    check(
      named.length === 3 && JSON.stringify(linked) === JSON.stringify(named.map((n) => [String(n), states[n - 1]])),
      `the pacing row's to-do names each of its slides by a button: ${JSON.stringify(linked)} of ${named}`,
    );
    const last = named[named.length - 1];
    await page.click(`#story [data-findings] li[data-slide="${pacing.slide}"] p[data-todo="pacing"] [data-to-slide="${states[last - 1]}"]`);
    await page.waitForFunction((st) => window.scaena.canvas.boxed() === st, states[last - 1], { timeout: 30000 }).catch(() => {});
    check(await page.evaluate((st) => window.scaena.canvas.boxed() === st, states[last - 1]), `a press on slide ${last} shows it`);

    // The row a person works stays where it stood (editor dogfood 6): slide 5's notes deepened as
    // the pacing row asks move that row to the next shortest beat's slide, and the words told bring
    // a finding new to the Story ("behind" read as grave); the open rows read again keep their
    // places, the pacing row's among them, and what is new comes after them.
    const rows = () =>
      page.evaluate(() =>
        [...document.querySelectorAll("#story [data-findings] > li[data-slide]")].map((li) => ({
          slide: Number(li.dataset.slide),
          kinds: [...li.querySelectorAll("p[data-kind]")].map((p) => p.dataset.kind),
        })),
      );
    const stood = await rows();
    const fifth = states[pacing.slide - 1];
    await page.click(`#story [data-findings] li[data-slide="${pacing.slide}"] [data-show]`);
    await page.waitForFunction((st) => window.scaena.canvas.boxed() === st, fifth, { timeout: 30000 }).catch(() => {});
    await tab(page, "inspector");
    const notes = page.locator('textarea[data-prop="notes"]');
    await notes.waitFor({ timeout: 30000 });
    const told = await notes.inputValue();
    const deepened = await page.evaluate(() => window.scaena.source());
    await notes.fill(`${told}\nI was behind a woman at the laundromat on Fifth who had never had a card; she left with two. The man behind her asked for one for his son.`);
    await notes.blur();
    await page.waitForFunction((w) => window.scaena.source() !== w, deepened, { timeout: 30000 }).catch(() => {});
    await tab(page, "story");
    await page
      .waitForFunction((k) => window.scaena.story.health()?.findings.find((f) => f.kind === "pacing")?.slide !== k, pacing.slide, { timeout: 60000 })
      .catch(() => {});
    const moved = await page.evaluate(() => window.scaena.story.health().findings.find((f) => f.kind === "pacing")?.slide);
    check(moved !== undefined && moved !== pacing.slide, `deepened, slide ${pacing.slide}'s notes move the pacing row to slide ${moved}`);
    const now = await rows();
    const pacingAt = (list) => list.findIndex((r) => r.kinds.includes("pacing"));
    const kept = (r) => stood.some((s) => r.kinds.some((k) => s.kinds.includes(k) && (k === "pacing" || s.slide === r.slide)));
    const firstNew = now.findIndex((r) => !kept(r));
    check(
      pacingAt(now) === pacingAt(stood) && (firstNew === -1 || now.slice(firstNew).every((r) => !kept(r))),
      `read again, the rows keep their places, the pacing row's at ${pacingAt(stood)}, and what is new comes after: ${JSON.stringify(stood)} → ${JSON.stringify(now)}`,
    );
    await page.locator(".cm-content").focus();
    await page.keyboard.press("Control+z");
    await page.waitForFunction((w) => window.scaena.source() === w, deepened, { timeout: 30000, polling: 50 }).catch(() => {});
    check((await page.evaluate(() => window.scaena.source())) === deepened, "⌘Z takes the notes back");
    // A slide lint finds broken caps the score: the score says so, and what it would be, the slide
    // a button that shows it (W1's dogfood 5).
    const sound = await page.evaluate(() => window.scaena.source());
    const words = '"adults in our city has never had a library card"';
    check(sound.includes(words), "the big number's words are found in the source");
    const long = "adults in our city has never had a library card, nor been asked to hold one. ".repeat(8).trim();
    await page.evaluate((s) => window.scaena.type(s), sound.replace(words, JSON.stringify(long)));
    await tab(page, "story");
    await page.waitForFunction(() => document.querySelector("#story [data-score] [data-capped]"), null, { timeout: 120000 }).catch(() => {});
    const capped = await page.evaluate(() => ({ capped: window.scaena.story.health()?.capped, said: document.querySelector("#story [data-score] [data-capped]")?.textContent }));
    check(
      capped.capped?.slides?.length === 1 && capped.capped.score > capped.capped.cap &&
        capped.said === `Capped at ${capped.capped.cap} while slide ${capped.capped.slides[0]} has an error; ${capped.capped.score} without.`,
      `the score says what caps it, and what it would be: ${JSON.stringify(capped)}`,
    );
    const cappedState = await page.evaluate((n) => window.scaena.story.health().slides[n - 1]?.state, capped.capped?.slides?.[0]);
    await page.click("#story [data-score] [data-capped-slide]").catch((e) => failures.push(`no capped slide to press: ${e.message}`));
    await page.waitForFunction((st) => window.scaena.canvas.boxed() === st, cappedState, { timeout: 30000 }).catch(() => {});
    check(await page.evaluate((st) => window.scaena.canvas.boxed() === st, cappedState), `a press on its slide shows it: ${cappedState}`);
    await page.close();
  })();

  // A figure no source backs, in a deck made from sources (W1's dogfood 5): its row offers the
  // deck's sources, and the one cited joins the slide's beat's evidence, one patch, the finding
  // gone; ⌘Z takes it back.
  await (async () => {
    const bundle = "target/web-story/grant";
    await rm(bundle, { recursive: true, force: true });
    await mkdir("target/web-story", { recursive: true });
    const slide = (title, layout, bullets, beat, name, src) => ({
      slideTitle: title, subtitle: "", layoutSuggestion: layout, keyBullets: bullets, speakerNotes: "",
      beatIndex: beat, beatName: name, purpose: "", intent: "", expectedIntensity: 0.5, expectedValence: 0.3,
      ...(src ? { sourceRef: src } : {}),
    });
    const slides = {
      executiveSummary: "The season came in under budget.",
      slides: [
        slide("High Country Trails", "Title", [], 0, "Opening"),
        slide("We came in $6,600 under budget", "Statement", ["Every line but one came in under plan"], 1, "Cost", "trails-budget.csv"),
        slide("Trail use rose 27% on rebuilt segments", "Big Number", ["27%", "more hikers on rebuilt segments"], 1, "Cost"),
        slide("Crews rebuilt the Bear Creek saddle", "Statement", ["Eleven weekends of work after the storms"], 1, "Cost"),
        slide("Renew the grant", "Closing", ["Thank you"], 2, "Ask"),
      ],
    };
    await writeFile("target/web-story/grant.slides.json", JSON.stringify(slides));
    execFileSync(
      "cargo",
      ["run", "-q", "-p", "scaena-cli", "--locked", "--", "generate", bundle, "A grant renewal for a panel that checks every figure",
        "--slides", "target/web-story/grant.slides.json", "--source", "docs/examples/data/trails-budget.csv",
        "--no-look", "--pictures", "none", "--audience", "a grant review panel", "--goal", "Persuade", "--length", "5-10 slides"],
      { stdio: "inherit" },
    );
    const page = await context.newPage();
    page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
    page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
    await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&bundle=/${bundle}/deck.json`);
    await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });
    await tab(page, "story");
    await page.waitForFunction(() => window.scaena.story.health()?.findings.some((f) => f.kind === "source"), null, { timeout: 120000 }).catch(() => {});
    const unsourced = await page.evaluate(() => window.scaena.story.health()?.findings.find((f) => f.kind === "source"));
    check(unsourced?.figure === "27%", `the 27% no source backs is found: ${JSON.stringify(unsourced)}`);
    const row = `#story [data-findings] li[data-slide="${unsourced?.slide}"]`;
    // While the source does not compile, Cite a source… is held with the row's fixes (#944).
    const sound = await page.evaluate(() => window.scaena.source());
    await page.evaluate((s) => window.scaena.type(`${s}\nstate {{{`), sound);
    await page.waitForFunction(() => window.scaena.last()?.valid === false, null, { timeout: 30000 }).catch(() => {});
    await tab(page, "inspector");
    await tab(page, "story");
    await page.waitForFunction(() => !document.querySelector("#story > p[data-stale]").hidden, null, { timeout: 60000 }).catch(() => {});
    const held = await page.evaluate((r) => [...document.querySelectorAll(`${r} [data-cite], ${r} [data-citing] [data-source]`)].map((b) => b.disabled), row);
    check(held.length === 2 && held.every(Boolean), `while the source does not compile, Cite a source… is held: ${JSON.stringify(held)}`);
    await page.focus("#overlay");
    await page.keyboard.press("Control+z");
    await page.waitForFunction((s) => window.scaena.source() === s, sound, { timeout: 30000 }).catch(() => {});
    await page.waitForFunction(() => document.querySelector("#story > p[data-stale]").hidden, null, { timeout: 60000 }).catch(() => {});
    await page.waitForFunction((r) => document.querySelector(`${r} [data-cite]`)?.disabled === false, row, { timeout: 60000 }).catch(() => {});
    check(await page.evaluate((r) => document.querySelector(`${r} [data-cite]`)?.disabled === false, row), "taken back, Cite a source… is offered again");
    await page.click(`${row} [data-cite]`).catch((e) => failures.push(`no Cite a source…: ${e.message}`));
    // Read again, as once every state is linted, the sources stay open as they were left.
    await page.evaluate(() => {
      window.__read = window.scaena.story.health();
      window.scaena.story.refresh();
    });
    await page.waitForFunction(() => window.scaena.story.health() !== window.__read, null, { timeout: 30000 }).catch(() => {});
    const kept = await page.evaluate((r) => ({
      open: document.querySelector(`${r} [data-citing]`)?.hidden === false,
      expanded: document.querySelector(`${r} [data-cite]`)?.ariaExpanded,
      focus: document.activeElement?.dataset?.source,
    }), row);
    check(kept.open && kept.expanded === "true" && kept.focus === "trails-budget.csv", `read again, the sources to cite stay open, the focus on them: ${JSON.stringify(kept)}`);
    const offered = await page.evaluate((r) => [...document.querySelectorAll(`${r} [data-citing] [data-source]`)].map((b) => b.textContent), row);
    check(offered.join() === "trails-budget.csv", `Cite a source… offers the deck's sources: ${JSON.stringify(offered)}`);
    const was = await page.evaluate(() => window.scaena.source());
    await page.click(`${row} [data-citing] [data-source="trails-budget.csv"]`).catch(() => {});
    await page.waitForFunction((w) => window.scaena.source() !== w, was, { timeout: 30000 }).catch(() => {});
    await page
      .waitForFunction(() => !window.scaena.story.health()?.findings.some((f) => f.kind === "source"), null, { timeout: 60000 })
      .catch(() => {});
    const cited = await page.evaluate(() => window.scaena.story.health()?.findings.filter((f) => f.kind === "source").map((f) => f.says));
    check(cited?.length === 0, `cited, the figure is sourced: ${JSON.stringify(cited)}`);
    check(/source:trails-budget\.csv/.test(await page.evaluate(() => window.scaena.source())), "the beat cites it as its evidence");
    check(/cites trails-budget\.csv/.test(await page.textContent("#status")), `and the status says so: ${await page.textContent("#status")}`);
    await page.locator(".cm-content").focus();
    await page.keyboard.press("Control+z");
    await page.waitForFunction((w) => window.scaena.source() === w, was, { timeout: 30000, polling: 50 }).catch(() => {});
    check((await page.evaluate(() => window.scaena.source())) === was, "⌘Z takes the citation back in one step");
    await page.close();
  })();
} catch (e) {
  failures.push(`error: ${e.message}`);
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "the Story tab leads with what to do first, and folds the rest; a claim an edit leaves behind is offered restated until the source changes again");
process.exit(failures.length ? 1 : 0);
