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
//   thumbnail the same drawing, and it is shown. One undo takes it back.
// - + Slide opens the gallery of Dusk's layouts (PLAN 3.30): a slide drawn for each of its 47, by
//   its sections, then a blank one. Bullets starts a slide after `mix` with its headline and its
//   points in their slots, one undo; Photo one whose picture, edge to edge, the canvas outlines
//   with its words, where it outlines nothing on `intro`, a title slide the deck opened with no
//   picture and no kicker. Blank adds an empty slide after `mix`. Alt with an arrow key moves it, and so
//   does a drag; Delete removes it, and the source is as it was.
// - F2 renames a state; a state another builds on is not removed, and the status says why.
// - The state shown, renamed and taken away, leaves no error on the page, and ⌘Z and ⇧⌘Z with the
//   strip focused undo and redo (PLAN 2.77).
// Exits 1 on any failure.
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
  await page.goto(url("/docs/examples/revenue.deck.json"));
  await page.waitForFunction(() => window.scaena?.last() || document.querySelector("#status")?.textContent.startsWith("error"), null, {
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
  check(await says("revenue-2 added after revenue"), `the status says so: ${await status()}`);
  check(await shows(2), "and shows it");
  const after = await painted(["revenue-2"]);
  check(after["revenue-2"] === after.revenue, "its thumbnail is revenue's drawing, digest for digest");
  await undo();
  check(await back(original), "one undo takes it back");
  check(await holds(["intro", "revenue", "mix", "close"]), "and the strip with it");

  // + Slide opens the gallery of the theme's layouts, each a slide started in it (PLAN 3.30).
  const gallery = async () => {
    await page.click("#strip [data-add=slide]");
    await page.waitForSelector("#starting[open] button[data-start]", { timeout: 60000 });
  };
  const tile = (name) => page.locator("#starting").getByRole("button", { name, exact: true });
  await item("revenue").click();
  await shows(1);
  await gallery();
  const offered = await page.evaluate(() => ({
    tiles: document.querySelectorAll("#starting button[data-start]").length,
    sections: [...document.querySelectorAll("#starting h3")].map((h) => h.textContent),
    names: [...document.querySelectorAll("#starting button[data-start] span")].map((s) => s.textContent),
    drawn: [...document.querySelectorAll("#starting canvas")].filter((c) => c.width === 0 || c.height === 0).length,
  }));
  check(offered.tiles === 48 && offered.drawn === 0, `a slide drawn for each of Dusk's 47 layouts and a blank one: ${offered.tiles}`);
  check(offered.sections.join() === "Titles,Words,Pictures,Charts,Numbers,Blank", `by its sections: ${offered.sections.join(", ")}`);
  const wanted = ["Big number", "Photo", "Two up", "Three up", "Four up", "Cards", "Funnel"];
  check(wanted.every((n) => offered.names.includes(n)), `among them ${wanted.join(", ")}`);
  await tile("Bullets").click();
  check(await holds(["intro", "revenue", "mix", "bullets", "close"]), `Bullets starts a slide after mix: ${await order()}`);
  const started = await source();
  check(started.includes("state bullets mode:absolute layout:bullets"), "absolute, in its layout");
  check(started.includes('"What this slide says"') && started.includes("The first point"), "its headline and its points in their slots");
  check(await shows(3), "and shows it");
  await undo();
  check(await back(original), "one undo takes it back");

  // Photo: its picture waits, edge to edge, outlined with its words.
  await item("revenue").click();
  await shows(1);
  await gallery();
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

  // F2 renames; what another state builds on stays.
  await item("close").focus();
  await page.keyboard.press("F2");
  await page.keyboard.press("Control+a");
  await page.keyboard.type("end");
  await page.keyboard.press("Enter");
  check(await holds(["intro", "revenue", "mix", "end"]), `F2 renames close: ${await order()}`);
  check((await source()).includes("state end layout:title"), "in the source");
  await undo();
  check(await back(original), "one undo takes the name back");
  await item("revenue").focus();
  await page.keyboard.press("Delete");
  check(await says("not made"), `revenue, which mix builds on, stays: ${await status()}`);
  check(await holds(["intro", "revenue", "mix", "close"]), "the strip as it was");
  check(await page.evaluate(() => window.scaena.last().valid), "the source still compiles and validates");

  // The state shown, renamed then taken away, from the strip: what the canvas was asking of it
  // lets go, with no error on the page; and ⌘Z with the strip focused undoes each (PLAN 2.77).
  await item("close").click();
  check(await shows(3), "close shown");
  await page.keyboard.press("F2");
  await page.keyboard.press("Control+a");
  await page.keyboard.type("end");
  await page.keyboard.press("Enter");
  check(await holds(["intro", "revenue", "mix", "end"]), `the state shown renamed: ${await order()}`);
  await item("end").focus();
  await page.keyboard.press("Delete");
  check(await holds(["intro", "revenue", "mix"]), `and taken away: ${await order()}`);
  await page.waitForTimeout(1500);
  check(!failures.some((f) => f.includes("unknown state")), "the page asks nothing of the state it took away");
  await item("mix").focus();
  await page.keyboard.press("Control+z");
  check(await holds(["intro", "revenue", "mix", "end"]), `⌘Z in the strip puts it back: ${await order()}`);
  await page.keyboard.press("Control+z");
  check(await back(original), "and again, the name it had");
  await page.keyboard.press("Control+Shift+z");
  check(await holds(["intro", "revenue", "mix", "end"]), `⇧⌘Z in the strip makes the rename again: ${await order()}`);
} finally {
  await browser.close();
  await server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "the strip shows, adds, moves, renames, and removes states, each a patch");
process.exit(failures.length ? 1 : 0);
