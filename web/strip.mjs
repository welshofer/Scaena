// PLAN 2.35 check: the editor's state strip, in headless Chromium (serve.mjs), the CPU painting,
// as in web/canvas.mjs.
//
//   node web/strip.mjs     (after `just web`; from the repository's root)
//
// On the revenue example:
// - The strip holds the deck's four states in order, each with its cue's length, and a thumbnail
//   the engine painted at rest, once every state is laid out.
// - A click on a state shows it; the arrow keys, Home, and End move among them. A state clicked
//   just after an edit on the canvas stays shown once the editor lints the source.
// - + Step adds `revenue-2` after `revenue`, in its slide: it shows what `revenue` shows, its
//   thumbnail the same drawing, and it is shown; the source, opened, opens at it. One undo takes
//   it back, its slide shown and named in the picker, and a slide picked there straight after the
//   undo is the one shown.
// - + Slide opens Dusk's layouts by their sections (PLAN 3.30, 3.31, S.1, S.2): the 16 down the side as
//   tabs, 173 layouts in all, Titles first, each drawn as it is used and named as a person calls it;
//   a tab clicked or the down arrow shows another, and the field finds a layout in any section, and
//   by another name people call it (Kanban finds Roadmap, ADR-0031).
//   Among the layouts, → goes to the next and ↓ to the one below, as they stand in the grid.
//   Bullets starts a slide after `mix` with its headline and its
//   points in their slots, one undo; a double click on its headline selects its words whole, so
//   typing replaces them; its headline deleted, a press on the header's words puts a headline back
//   in the slot. Photo starts one whose picture, edge to edge, the canvas outlines
//   with its words, and a press on them asks for a picture, which goes in that slot, one undo;
//   where it outlines nothing on `intro`, a title slide the deck opened with no picture and no
//   kicker. Bullets deleted from the strip takes its headline and points with it, the source as
//   it was, and the editor counts the states as the patch left them before the source compiles
//   again, as it does after the title is taken out of every slide that writes it and after the
//   undo that puts it back. Blank adds an empty slide after `mix`. Alt with an arrow key moves it,
//   and so does a drag; Delete removes it, and the source is as it was.
// - A deck whose bundle took Dusk before its library grew, 11 layouts in no section, opens with the
//   library Dusk ships now (PLAN 3.31): + Slide offers the 16 sections and 173 layouts.
// - Each is named as a person counts it (PLAN 2.99): Slide 2, Step 2; no id names one. A state
//   another builds on is not removed, and the status says why.
// - The state shown, taken away, leaves no error on the page, and ⌘Z and ⇧⌘Z with the strip
//   focused undo and redo (PLAN 2.77). An undo says what it took back, and a redo what it made
//   again, in the words the change was made in (lift-up USE-4).
// On the trails example, the strip draws each thumbnail as it is painted, the first before every
// state is linted. A slide whose thumbnail cannot be painted is marked so on its tile, said once in
// the console, and not painted again until its drawing changes (lift-up STAB-4).
// Exits 1 on any failure.
import { readFile } from "node:fs/promises";
import { crc32 } from "node:zlib";
import { launch, serve } from "./serve.mjs";

const server = await serve();
const url = (bundle) => `${server.origin}/web/dist/editor.html?painter=cpu&bundle=${bundle}`;
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
  // Each file picker the page opens. Playwright intercepts pickers once a listener is added, but
  // turns that on without waiting: a press made at once can open its picker first, and headless
  // Chromium cancels a picker no one intercepts. Heard from the start, none is missed.
  const choosers = [];
  page.on("filechooser", (c) => choosers.push(c));
  /** The picker the page opens after the `n`th, within `ms`. */
  const chooserAfter = async (n, ms = 10000) => {
    for (const end = Date.now() + ms; choosers.length <= n && Date.now() < end; ) await page.waitForTimeout(50);
    return choosers[n];
  };
  /** Where `part`, drawn in the overlay, stands on the page. The overlay draws its parts anew each
   * time it draws, so the part found can be gone before its box is read: it is asked again. */
  const boxOf = async (part) => {
    for (let i = 0; i < 100; i++) {
      const box = await part.boundingBox({ timeout: 1000 }).catch(() => null);
      if (box) return box;
      await page.waitForTimeout(50);
    }
    return null;
  };
  await page.goto(url("/docs/examples/revenue.deck.json"));
  await page.waitForFunction(() => window.scaena?.last() || document.querySelector("#status")?.textContent.startsWith("Error"), null, {
    timeout: 120000,
  });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  /** The strip's states, in order, and the one selected. */
  const strip = () =>
    page.evaluate(() => [...document.querySelectorAll("#strip li")].map((li) => ({ id: li.dataset.state, on: li.getAttribute("aria-selected") === "true" })));
  const order = async () => (await strip()).map((s) => s.id);
  const selected = async () => (await strip()).find((s) => s.on)?.id;
  /** Once the strip's states are `ids`. */
  const holds = async (ids) => {
    await page
      .waitForFunction((want) => [...document.querySelectorAll("#strip li")].map((li) => li.dataset.state).join() === want.join(), ids, {
        timeout: 30000,
        polling: 50,
      })
      .catch(() => {});
    return JSON.stringify(await order()) === JSON.stringify(ids);
  };
  const shows = async (index) => {
    await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.at()?.index === i, index, { timeout: 30000 }).catch(() => {});
    return (await page.evaluate(() => window.scaena.shown())) === index;
  };
  /** Once the status says `text`. */
  const says = async (text) => {
    await page.waitForFunction((t) => document.querySelector("#status").textContent.includes(t), text, { timeout: 30000 }).catch(() => {});
    return (await status()).includes(text);
  };
  const back = async (to) => {
    await page.waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 }).catch(() => {});
    await page.evaluate(() => window.scaena.strip.settled());
    await page.waitForTimeout(300);
    return (await source()) === to;
  };
  /** Once the strip has a thumbnail for each of `ids`. */
  const painted = async (ids) => {
    await page
      .waitForFunction((want) => want.every((id) => window.scaena.strip.digests()[id]), ids, { timeout: 60000, polling: 100 })
      .catch(() => {});
    return page.evaluate(() => window.scaena.strip.digests());
  };
  const item = (id) => page.locator(`#strip li[data-state="${id}"]`);
  const undo = async () => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
  };

  const original = await source();
  check(await holds(["intro", "revenue", "mix", "close"]), `the strip holds the deck's states in order: ${await order()}`);
  const cues = await page.evaluate(() => [...document.querySelectorAll("#strip li .cue")].map((c) => c.textContent));
  check(cues.length === 4 && cues.every((c) => /^\d+(\.\d)? s$/.test(c)), `each with its cue's length: ${cues}`);
  const digests = await painted(["intro", "revenue", "mix", "close"]);
  check(Object.keys(digests).length === 4, `each with a thumbnail the engine painted: ${Object.keys(digests)}`);
  const size = await page.evaluate(() => {
    const c = document.querySelector('#strip li[data-state="intro"] canvas');
    return [c.width, c.height];
  });
  check(size[1] === 54 && size[0] === 96, `painted 54 pixels high, in the canvas's aspect: ${size}`);
  const lit = await page.evaluate(() => {
    const c = document.querySelector('#strip li[data-state="intro"] canvas');
    const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
    let n = 0;
    for (let i = 0; i < d.length; i += 4) if (d[i] + d[i + 1] + d[i + 2] > 60) n++;
    return n / (d.length / 4);
  });
  check(lit > 0.2, `intro's thumbnail shows its mesh background: ${(lit * 100).toFixed(0)}% of it lit`);

  // A click shows a state; the keys move among them.
  await item("mix").click();
  check(await shows(2), "a click on mix shows it");
  check((await selected()) === "mix", "and the strip selects it");
  await page.keyboard.press("ArrowLeft");
  check(await shows(1), "← shows the state before it");
  await page.keyboard.press("End");
  check(await shows(3), "End shows the last");
  await page.keyboard.press("Home");
  check(await shows(0), "Home shows the first");

  // A state shown just after an edit on the canvas stays shown once the editor lints the source.
  // The lint takes the patch's own result, which names the state shown when it was made: a click
  // on another state between the two is a move, as it is while the worker compiles.
  await item("revenue").click();
  await shows(1);
  await page.waitForFunction(() => window.scaena.canvas.boxes().some((b) => b.node === "note"), null, { timeout: 30000 });
  const note = await page.evaluate(() => {
    const [x, y, w, h] = window.scaena.canvas.boxes().find((b) => b.node === "note").rect;
    const r = document.querySelector("#overlay").getBoundingClientRect();
    const [cw, ch] = window.scaena.canvas.size();
    return [r.left + ((x + w / 2) / cw) * r.width, r.top + ((y + h / 2) / ch) * r.height];
  });
  await page.mouse.click(note[0], note[1]);
  await page.waitForFunction(() => window.scaena.canvas.selected() === "note", null, { timeout: 30000 }).catch(() => {});
  // The moment the Delete is in the source, a click on mix, well inside the lint's 150 ms.
  const clicked = page.evaluate(
    (before) =>
      new Promise((done) => {
        const poll = () => {
          if (window.scaena.source() === before) return setTimeout(poll, 0);
          document.querySelector('#strip li[data-state="mix"]').click();
          done(window.scaena.shown());
        };
        poll();
      }),
    original,
  );
  await page.keyboard.press("Delete");
  const then = await clicked;
  await page.waitForTimeout(1000);
  const linted = await page.evaluate(() => window.scaena.shown());
  check(then === 2 && linted === 2 && (await selected()) === "mix", `mix, clicked just after a Delete on revenue, stays shown once the source is linted: ${then} then ${linted}`);
  await undo();
  check(await back(original), "one undo takes the Delete back");

  // + Step after revenue: in its slide, showing what it shows.
  await item("revenue").click();
  await shows(1);
  await page.click("#strip [data-add=step]");
  check(await holds(["intro", "revenue", "revenue-2", "mix", "close"]), `+ Step adds revenue-2 after revenue: ${await order()}`);
  check((await source()).includes("state revenue-2 slide:revenue"), "in revenue's slide");
  check(await says("A step added after Slide 2"), `the status says so: ${await status()}`);
  check(await shows(2), "and shows it");
  // The source, opened now, opens at the step shown, its lines from their starts: not where the
  // change began, which a pane folded scrolled to against no width.
  await page.click("#source-toggle");
  await page.waitForFunction(() => !document.querySelector("main").classList.contains("folded"), null, { timeout: 10000 });
  await page.waitForTimeout(300);
  const cursor = await page.evaluate(() => ({ line: document.querySelector(".cm-activeLine")?.textContent ?? "", left: document.querySelector(".cm-scroller").scrollLeft }));
  check(cursor.line.startsWith("state revenue-2") && cursor.left === 0, `the source opens at it, from the line's start: ${JSON.stringify(cursor)}`);
  await page.click("#source-toggle");
  await page.waitForFunction(() => document.querySelector("main").classList.contains("folded"), null, { timeout: 10000 });
  const after = await painted(["revenue-2"]);
  check(after["revenue-2"] === after.revenue, "its thumbnail is revenue's drawing, digest for digest");
  await undo();
  check(await says("Undone: A step added after Slide 2"), `the status says what the undo took back (lift-up USE-4): ${await status()}`);
  check(await back(original), "one undo takes it back");
  check(await holds(["intro", "revenue", "mix", "close"]), "and the strip with it");
  const label = () => page.evaluate(() => document.querySelector("#state").selectedOptions[0]?.textContent);
  check((await shows(1)) && (await label()) === "Slide 2", `its slide is shown, and the picker names it: ${await page.evaluate(() => window.scaena.shown())}, ${await label()}`);
  // A slide picked in the picker straight after the undo, before the source is compiled again, is
  // the one shown, and stays shown.
  await page.click("#strip [data-add=step]");
  await holds(["intro", "revenue", "revenue-2", "mix", "close"]);
  await shows(2);
  await undo();
  await page.selectOption("#state", "mix");
  await page.waitForTimeout(1500);
  const picked = () => page.evaluate(() => document.querySelector("#state").value);
  check(
    (await shows(2)) && (await selected()) === "mix" && (await picked()) === "mix" && (await label()) === "Slide 2, step 2",
    `mix, picked at once, is shown: ${await page.evaluate(() => window.scaena.shown())}, ${await selected()}, ${await picked()}, ${await label()}`,
  );
  check(await back(original), "and the step is undone");

  // ⌘D with the strip focused copies the slide of the state focused, just after itself (lift-up
  // FUNC-1): one `duplicate_slide`, as the light table copies one, the copy shown, one undo; and
  // the key is taken, so the browser does not bookmark the page. So does the state's right-click
  // menu's Duplicate this slide.
  await item("mix").click();
  await shows(2);
  await item("mix").focus();
  const taken = await page.evaluate(() => {
    const e = new KeyboardEvent("keydown", { key: "d", ctrlKey: true, bubbles: true, cancelable: true });
    document.activeElement.dispatchEvent(e);
    return e.defaultPrevented;
  });
  check(taken, "⌘D in the strip is taken from the browser");
  check(await holds(["intro", "revenue", "mix", "revenue-2", "mix-2", "close"]), `⌘D on a step copies its slide just after itself: ${await order()}`);
  check(await says("Slide 2 copied just after itself"), `the status says so: ${await status()}`);
  check(await shows(3), "and shows the copy");
  await undo();
  check(await back(original), "one undo takes the copy back");
  await item("intro").click();
  await shows(0);
  await item("intro").click({ button: "right" });
  const duplicating = page.locator(".context-menu [role=menuitem]", { hasText: "Duplicate this slide" });
  check((await duplicating.count()) === 1, "a right click on a slide offers Duplicate this slide");
  await duplicating.click();
  check(await holds(["intro", "intro-2", "revenue", "mix", "close"]), `which copies it just after itself: ${await order()}`);
  check(await shows(1), "and shows the copy");
  await undo();
  check(await back(original), "one undo takes it back");

  // + Slide opens the theme's layouts by their sections, each a slide started in it (PLAN 3.30,
  // 3.31): a section's tiles drawn as they are shown, a few at a time.
  const gallery = async () => {
    await page.click("#strip [data-add=slide]");
    await page.waitForSelector("#starting[open] button[data-start]", { timeout: 60000 });
  };
  const tile = (name) => page.locator("#starting").getByRole("button", { name, exact: true });
  const tab = (name) => page.locator("#starting").getByRole("tab", { name, exact: true });
  /** The names of the tiles shown, once every one is drawn: none where they are not. */
  const drawn = () =>
    page
      .waitForFunction(
        () => {
          const all = [...document.querySelectorAll("#starting .starters button[data-start]")];
          return all.length > 0 && all.every((b) => b.classList.contains("drawn")) && all.map((b) => b.querySelector("span").textContent);
        },
        null,
        { timeout: 60000 },
      )
      .then((h) => h.jsonValue(), () => []);
  await item("revenue").click();
  await shows(1);
  await gallery();
  const offered = await page.evaluate(() => ({
    sections: [...document.querySelectorAll("#starting [role=tab]")].map((t) => t.dataset.section),
    counted: [...document.querySelectorAll("#starting [role=tab] .count")].reduce((n, c) => n + Number(c.textContent), 0),
    selected: document.querySelector("#starting [role=tab][aria-selected=true]")?.dataset.section,
  }));
  const sections = "Titles,Statements,Lists,Comparison,Frameworks,Process,Time,Numbers,Charts,Pictures,People,Editorial,Diagrams,Briefing,Classroom,Closing";
  check(offered.sections.join() === sections, `the sections down the side: ${offered.sections.join(", ")}`);
  check(offered.counted === 173 && offered.selected === "Titles", `173 layouts, Titles shown first: ${offered.counted}, ${offered.selected}`);
  const titles = await drawn();
  const want = "Title,Section,Title over Photo,Title Only,Centered Title,Title beside Photo,Section and Photo,Section Number,Paper Title";
  check(titles.join() === want, `each drawn, by its name: ${titles.join(", ")}`);
  // The arrows go among the layouts as they stand in the grid: → the next, ↓ the one below.
  await tile("Title").focus();
  await page.keyboard.press("ArrowRight");
  const focused = () => page.evaluate(() => document.activeElement?.querySelector("span")?.textContent);
  check((await focused()) === "Section", `→ goes to the next layout: ${await focused()}`);
  const below = await page.evaluate(() => {
    const all = [...document.querySelectorAll("#starting .starters button[data-start]")];
    const across = all.filter((b) => b.offsetTop === all[0].offsetTop).length;
    return all[1 + across]?.querySelector("span").textContent;
  });
  await page.keyboard.press("ArrowDown");
  check(below !== undefined && (await focused()) === below, `↓ goes to the one below: ${await focused()}, ${below}`);
  await tab("Frameworks").click();
  const frameworks = await drawn();
  check(["SWOT", "OKRs", "RACI", "Venn Diagram", "Two-by-Two Matrix"].every((n) => frameworks.includes(n)), `Frameworks: ${frameworks.join(", ")}`);
  await page.keyboard.press("ArrowDown");
  const process = await drawn();
  const now = await page.evaluate(() => document.querySelector("#starting [role=tab][aria-selected=true]")?.dataset.section);
  check(now === "Process" && process.includes("How It Works"), `the down arrow shows the next: ${now}, ${process.join(", ")}`);
  await page.fill("#starting .find", "questions");
  const found = await drawn();
  check(found.includes("Q&A") && found.includes("FAQ"), `the field finds them in any section: ${found.join(", ")}`);
  await page.fill("#starting .find", "kanban");
  const kanban = await drawn();
  check(kanban.join() === "Roadmap", `the field finds a layout by another name people call it (ADR-0031): ${kanban.join(", ")}`);
  await page.fill("#starting .find", "disclosures");
  const disclosures = await drawn();
  check(disclosures.join() === "Fine Print", `and Fine Print by Disclosures (PLAN S.3): ${disclosures.join(", ")}`);
  await page.fill("#starting .find", "");
  await tab("Lists").click();
  await drawn();
  await tile("Bullets").click();
  check(await holds(["intro", "revenue", "mix", "bullets", "close"]), `Bullets starts a slide after mix: ${await order()}`);
  const started = await source();
  check(started.includes("state bullets mode:absolute layout:bullets"), "absolute, in its layout");
  check(started.includes('"What this slide says"') && started.includes("The first point"), "its headline and its points in their slots");
  check(await shows(3), "and shows it");
  // A double click on its headline types in it with its words selected whole, as a placeholder's
  // are: typing replaces them. One undo takes the typing back.
  const header0 = await page
    .waitForFunction(() => window.scaena.canvas.boxes()?.find((b) => b.node === "bullets-header")?.rect, null, { timeout: 30000 })
    .then((h) => h.jsonValue(), () => undefined);
  if (header0) {
    const [hx, hy] = await page.evaluate(([x, y, w, h]) => {
      const r = document.querySelector("#overlay").getBoundingClientRect();
      const [cw, ch] = window.scaena.canvas.size();
      return [r.left + ((x + w / 2) / cw) * r.width, r.top + ((y + h / 2) / ch) * r.height];
    }, header0);
    await page.mouse.dblclick(hx, hy);
    await page.waitForFunction(() => window.scaena.canvas.typing() === "bullets-header", null, { timeout: 30000 }).catch(() => {});
    const selected = await page.evaluate(() => {
      const a = document.activeElement;
      return a instanceof HTMLTextAreaElement ? [a.selectionStart, a.selectionEnd, a.value.length] : undefined;
    });
    check(selected?.[0] === 0 && selected[1] === selected[2] && selected[2] > 0, `a double click on its headline selects its words whole: ${JSON.stringify(selected)}`);
    await page.keyboard.type("Why it matters");
    await page.waitForFunction(() => window.scaena.source().includes('"Why it matters"') && !window.scaena.canvas.typed()?.sending, null, { timeout: 30000 }).catch(() => {});
    await page.keyboard.press("Escape");
    const retyped = await source();
    check(retyped.includes('"Why it matters"') && !retyped.includes('"What this slide says"'), "typing replaces them");
    await undo();
    check(await back(started), "one undo takes the typing back");
  }
  // Its headline deleted, the header's slot waits with its words; a press on them puts a text in
  // the slot's role back there, as a placeholder fills.
  await page.evaluate(() => window.scaena.canvas.select("bullets-header"));
  await page.locator("#overlay").focus();
  await page.keyboard.press("Delete");
  const header = await page
    .waitForFunction(() => window.scaena.canvas.waiting().find((w) => w.slot === "header"), null, { timeout: 30000 })
    .then((h) => h.jsonValue(), () => undefined);
  check(header?.typed === true && header?.role === "headline" && header?.words === "What this slide says", `its headline deleted, the header's slot waits with its words: ${JSON.stringify(header)}`);
  if (header) {
    const at = await boxOf(page.locator("#overlay [data-wait]").filter({ hasText: header.words }));
    if (at) await page.mouse.click(at.x + at.width / 2, at.y + at.height / 2);
    check(await says("The Header slot filled"), `a press on them puts its headline back: ${await status()}`);
    check((await source()).includes('"What this slide says"'), "its words, as the slide was started with them");
    const id = "bullets-header";
    const put = await page
      .waitForFunction((n) => {
        const b = window.scaena.canvas.boxes()?.find((x) => x.node === n);
        return b && !window.scaena.canvas.waiting().some((w) => w.slot === "header") ? b.rect : undefined;
      }, id, { timeout: 30000 })
      .then((h) => h.jsonValue(), () => undefined);
    check(Boolean(put) && Math.abs(put[0] - header.rect[0]) < 1 && Math.abs(put[1] - header.rect[1]) < 1, `in the header's slot: ${JSON.stringify(put)}, the slot ${JSON.stringify(header.rect)}`);
    await undo();
    await undo();
    check(await back(started), "two undos take the headline and the Delete back");
  }
  // Deleted from the strip, the slide takes what it alone showed with it, as a presentation app's
  // does: the source is as it was, and nothing is left that no slide shows.
  /** Where the editor has each state, read as the source changes from `was`, before it compiles
   * again: what the cursor follows meanwhile. */
  const counted = (was) =>
    page
      .waitForFunction((s) => window.scaena.source() !== s && window.scaena.last().states.map(([id]) => id).join(" "), was, { timeout: 30000, polling: "raf" })
      .then((h) => h.jsonValue(), () => undefined);
  await item("bullets").focus();
  const gone = counted(await source());
  await page.keyboard.press("Delete");
  check((await gone) === "intro revenue mix close", `the editor counts the states the patch left, before the source compiles: ${await gone}`);
  check(await holds(["intro", "revenue", "mix", "close"]), `Delete removes the Bullets slide: ${await order()}`);
  const left = (await source()).split("\n").filter((l) => l.includes("bullets-"));
  check(await back(original), `and its headline and points with it: ${left.join(" | ")}`);
  check(!(await status()).includes("note"), `with nothing left to note: ${await status()}`);
  await undo();
  check(await back(started), "one undo puts the slide back");
  await undo();
  check(await back(original), "one undo takes it back");
  // The title taken out of the deck goes from each slide that writes it, the first and the last,
  // one change across the slides between them: the editor counts every slide meanwhile.
  await page.evaluate(() => window.scaena.canvas.select("title"));
  await page.locator("#overlay").focus();
  const across = counted(await source());
  await page.keyboard.press("Shift+Delete");
  check((await across) === "intro revenue mix close", `and after a patch across several, before the source compiles: ${await across}`);
  // Its undo writes each slide's first line between them again: the editor still counts them.
  const restored = counted(await source());
  await undo();
  check((await restored) === "intro revenue mix close", `and after its undo, before the source compiles: ${await restored}`);
  check(await back(original), "one undo puts the title back");

  // Photo: its picture waits, edge to edge, outlined with its words.
  await item("revenue").click();
  await shows(1);
  await gallery();
  await tab("Pictures").click();
  await drawn();
  await tile("Photo").click();
  check(await holds(["intro", "revenue", "mix", "photo", "close"]), `Photo starts a slide after mix: ${await order()}`);
  await shows(3);
  const waits = await page.waitForFunction(() => {
    const w = window.scaena.canvas.waiting();
    return w.length ? w : undefined;
  }, null, { timeout: 30000 }).then((h) => h.jsonValue(), () => []);
  check(
    waits.length === 1 && waits[0].slot === "art" && waits[0].words === "A picture, edge to edge" && waits[0].rect.join() === "0,0,1920,1080",
    `the canvas outlines its picture's slot, edge to edge: ${JSON.stringify(waits)}`,
  );
  check((await page.locator("#overlay .waiting-words").textContent()) === "A picture, edge to edge", "with its words");
  // A press on the words asks for a picture, which goes in the slot, as a placeholder fills.
  const photoSlide = await source();
  const words = await boxOf(page.locator("#overlay .waiting-words"));
  const asked = choosers.length;
  if (words) await page.mouse.click(words.x + words.width / 2, words.y + words.height / 2);
  const chooser = await chooserAfter(asked);
  check(Boolean(chooser), "a press on its words asks for a picture");
  if (chooser) {
    await chooser.setFiles({ name: "card.png", mimeType: "image/png", buffer: await readFile("tests/fixtures/torture.scaena/assets/test-card.png") });
    check(await says("The picture card.png inserted on this slide"), `the picture goes in: ${await status()}`);
    const id = await page.evaluate(() => window.scaena.canvas.selected());
    const filled = await page
      .waitForFunction((n) => {
        const b = window.scaena.canvas.boxes()?.find((x) => x.node === n);
        return b && !window.scaena.canvas.waiting().length ? { node: b.node, rect: b.rect } : undefined;
      }, id, { timeout: 30000 })
      .then((h) => h.jsonValue(), () => undefined);
    check(filled?.rect?.join() === "0,0,1920,1080", `in the slot, edge to edge, outlined no more: ${JSON.stringify(filled)}`);
    await undo();
    check(await back(photoSlide), "one undo takes the picture back");
  }
  await undo();
  check(await back(original), "one undo takes it back");
  // intro, a title slide the deck opened with no picture and no kicker, outlines neither: only a
  // slide made here shows its layout's empty slots.
  await item("intro").click();
  await shows(0);
  await page.waitForFunction(() => window.scaena.canvas.boxes()?.some((b) => b.node === "title"), null, { timeout: 30000 }).catch(() => {});
  await page.waitForTimeout(500);
  const opened = await page.evaluate(() => ({ waits: window.scaena.canvas.waiting().length, words: document.querySelectorAll("#overlay .waiting-words").length }));
  check(opened.waits === 0 && opened.words === 0, `intro, a title slide as the deck opened, outlines no slot it left empty: ${JSON.stringify(opened)}`);

  // Blank: an empty slide after revenue's slide, after mix.
  await item("revenue").click();
  await shows(1);
  await gallery();
  await tile("Blank").click();
  check(await holds(["intro", "revenue", "mix", "slide", "close"]), `Blank adds an empty slide after mix: ${await order()}`);
  const declared = (await source()).split("\n").find((l) => l.startsWith("state slide"));
  check(Boolean(declared?.includes("mode:absolute") && !declared.includes("layout:")), `empty, in no layout: ${declared}`);
  check(await shows(3), "and shows it");

  // Alt with an arrow key moves it; so does a drag; Delete removes it.
  await item("slide").focus();
  await page.keyboard.press("Alt+ArrowRight");
  check(await holds(["intro", "revenue", "mix", "close", "slide"]), `Alt+→ moves it after close: ${await order()}`);
  await item("slide").dragTo(item("intro"), { targetPosition: { x: 4, y: 20 } });
  check(await holds(["slide", "intro", "revenue", "mix", "close"]), `a drag before intro moves it there: ${await order()}`);
  await item("slide").focus();
  await page.keyboard.press("Delete");
  check(await holds(["intro", "revenue", "mix", "close"]), `Delete removes it: ${await order()}`);
  check(await back(original), "and the source is as it was");

  // Each by its slide's number, a step by its place in its slide; no id, which nothing renames.
  const names = await page.evaluate(() => [...document.querySelectorAll("#strip li .id")].map((n) => n.textContent));
  check(names.join() === "Slide 1,Slide 2,Step 2,Slide 3", `each named as a person counts it: ${names}`);
  await item("close").focus();
  await page.keyboard.press("F2");
  check(!(await page.evaluate(() => document.querySelector("#strip input"))), "F2 opens no field to rename an id");
  // What another state builds on stays.
  await item("revenue").focus();
  await page.keyboard.press("Delete");
  check(await says("Not changed"), `revenue, which mix builds on, stays: ${await status()}`);
  check(await holds(["intro", "revenue", "mix", "close"]), "the strip as it was");
  check(await page.evaluate(() => window.scaena.last().valid), "the source still compiles and validates");

  // The same deck, its bundle holding Dusk as it was on 2026-10-06, before the library grew: the
  // worker brings in the library Dusk ships now, and + Slide offers all of it, by section.
  {
    const fresh = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1 });
    const held = await readFile("tests/themes/older/dusk-0.10.theme.json", "utf8");
    let served = 0;
    await fresh.route("**/docs/examples/themes/dusk.theme.json", (route) => {
      served++;
      return route.fulfill({ contentType: "application/json", body: held });
    });
    const older = await fresh.newPage();
    older.on("pageerror", (e) => failures.push(`older Dusk: ${e.message}`));
    await older.goto(url("/docs/examples/revenue.deck.json"));
    await older.waitForFunction(() => window.scaena?.last() || document.querySelector("#status")?.textContent.startsWith("Error"), null, { timeout: 120000 });
    await older.click("#strip [data-add=slide]");
    await older.waitForSelector("#starting[open] button[data-start]", { timeout: 60000 });
    const brought = await older.evaluate(() => ({
      sections: [...document.querySelectorAll("#starting [role=tab]")].map((t) => t.dataset.section),
      counted: [...document.querySelectorAll("#starting [role=tab] .count")].reduce((n, c) => n + Number(c.textContent), 0),
    }));
    check(served > 0, `the bundle's theme is the older Dusk, served ${served} time(s)`);
    check(brought.sections.join() === sections && brought.counted === 173, `an older Dusk opens with its library: ${brought.sections.length} sections, ${brought.counted} layouts`);
    await fresh.close();
  }

  // The state shown, taken away from the strip: what the canvas was asking of it lets go, with no
  // error on the page; and ⌘Z and ⇧⌘Z with the strip focused undo and redo it (PLAN 2.77).
  await item("close").click();
  check(await shows(3), "close shown");
  await item("close").focus();
  await page.keyboard.press("Delete");
  check(await holds(["intro", "revenue", "mix"]), `the state shown taken away: ${await order()}`);
  await page.waitForTimeout(1500);
  check(!failures.some((f) => f.includes("unknown state")), "the page asks nothing of the state it took away");
  await item("mix").focus();
  await page.keyboard.press("Control+z");
  check(await holds(["intro", "revenue", "mix", "close"]), `⌘Z in the strip puts it back: ${await order()}`);
  check(await says("Undone: Slide 3 removed"), `and the status says what it took back (lift-up USE-4): ${await status()}`);
  check(await back(original), "as it was");
  await item("mix").focus();
  await page.keyboard.press("Control+Shift+z");
  check(await holds(["intro", "revenue", "mix"]), `⇧⌘Z in the strip takes it away again: ${await order()}`);
  check(await says("Made again: Slide 3 removed"), `and the status says what it made again: ${await status()}`);

  // A deck of many states, opened: the strip draws its thumbnails as they are painted, the first
  // before every state is linted.
  const trails = await context.newPage();
  trails.on("pageerror", (e) => failures.push(`trails: ${e.message}`));
  await trails.goto(url("/docs/examples/trails.deck.json"));
  const first = await trails.evaluate(
    () =>
      new Promise((done) => {
        const poll = () => {
          const painted = Object.keys(window.scaena?.strip?.digests() ?? {}).length;
          if (painted) return done({ painted, linted: window.scaena.wholes().length });
          setTimeout(poll, 20);
        };
        poll();
        setTimeout(() => done({ painted: 0 }), 60000);
      }),
  );
  check(first.painted > 0 && first.linted === 0, `the first thumbnails are drawn before every state is linted: ${JSON.stringify(first)}`);

  // A slide whose thumbnail cannot be painted (lift-up STAB-4): the storm slide's photo, its
  // header whole and its pixels' stream damaged, lays out and draws no pixels. Its tile says so,
  // the cause goes to the console once, and it is not painted again until its drawing changes,
  // however often the strip paints.
  const photo = await readFile("docs/examples/assets/trails-ridge.png");
  const damaged = Buffer.from(photo);
  const idat = damaged.indexOf("IDAT");
  const length = damaged.readUInt32BE(idat - 4);
  damaged.fill(0x55, idat + 6, idat + 4 + length);
  damaged.writeUInt32BE(crc32(damaged.subarray(idat, idat + 4 + length)), idat + 4 + length);
  const broken = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1 });
  await broken.route("**/assets/trails-ridge.png", (r) => r.fulfill({ contentType: "image/png", body: damaged }));
  const unpaintable = await broken.newPage();
  unpaintable.on("pageerror", (e) => failures.push(`unpaintable: ${e.message}`));
  const warned = [];
  unpaintable.on("console", (m) => m.type() === "warning" && m.text().includes("could not be painted") && warned.push(m.text()));
  await unpaintable.goto(url("/docs/examples/trails.deck.json"));
  const states = await unpaintable.evaluate(() => window.scaena.slots().length).catch(() => 15);
  const settled = () =>
    unpaintable.waitForFunction((n) => Object.keys(window.scaena?.strip?.digests() ?? {}).length >= n, states, { timeout: 120000 }).then(
      () => true,
      () => false,
    );
  check(await settled(), "every slide's thumbnail is painted or marked");
  const stormTile = () =>
    unpaintable.evaluate(() => {
      const li = document.querySelector('#strip li[data-state="storm"]');
      const canvas = document.querySelector('#strip li[data-state="cover"] canvas');
      return { said: li?.querySelector(".unpainted")?.textContent ?? null, others: canvas ? canvas.width > 0 : false };
    });
  const stormMarked = await stormTile();
  check(stormMarked.said === "This slide couldn’t be drawn", `the storm slide's tile says it couldn't be drawn: ${JSON.stringify(stormMarked)}`);
  check(stormMarked.others, "and the slides around it are painted");
  check(warned.length === 1 && warned[0].includes("storm"), `the cause is in the console once: ${JSON.stringify(warned)}`);
  // The strip painted again, three times over: the storm slide is not tried again. (An edit would
  // paint the strip once every state is linted, which this deck's damaged photo stops.)
  for (let i = 0; i < 3; i++) await unpaintable.evaluate(() => window.scaena.strip.painted());
  check(warned.length === 1, `and the storm slide is not painted again, however often the strip paints: ${warned.length} warnings`);
  check((await stormTile()).said === "This slide couldn’t be drawn", "its tile still says so");
  await broken.close();
} finally {
  await browser.close();
  await server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "the strip shows, adds, moves, and removes states, each a patch, each named as a person counts it");
process.exit(failures.length ? 1 : 0);
