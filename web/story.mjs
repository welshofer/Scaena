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
//   Story tab's first row is that slide's error, the disclosure left open as it was.
// - On the revenue example: a new headline typed over the cover's, which says the opening beat's
//   claim, is offered beside the status once the burst of typing ends ("Slide 1's story still says
//   “…”. Update it to “…”?"), with Update and Keep, and the offer stands whatever the status says
//   after it, Escape's words and what lint finds; Update restates the claim and the offer goes; an
//   undo of the typing takes the offer away; Keep lets it be and changes nothing (lift-up FUNC-2).
// - A made deck's plan (dogfood 9's keynote, made with no call): a slide planned hotter than its
//   place is asked by its Intensity; the Format tab offers its beat's Intensity and Tone, each said
//   in the engine's words as it is held, nothing made; let go, one patch, the bend answered; ⌘Z
//   takes it back (editor dogfood 3).
// Exits 1 on any failure.
import { execFileSync } from "node:child_process";
import { rm } from "node:fs/promises";
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
  await page.fill("#question", "");
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
  await tab(page, "story");
  await page
    .waitForFunction((s) => !window.scaena.story.health()?.findings.some((f) => f.kind === "turn" && f.state === s), turn.state, { timeout: 60000 })
    .catch(() => {});
  const still = await page.evaluate((s) => window.scaena.story.health().findings.filter((f) => f.kind === "turn" && f.state === s), turn.state);
  check(still.length === 0, `and the turn is answered: ${JSON.stringify(still.map((f) => f.says))}`);

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
