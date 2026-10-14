// PLAN 2.103 furniture check: a slide's number and footer set from the editor's inspector
// (ADR-0028), in headless Chromium (serve.mjs), the CPU painting. On the revenue example in Dusk:
//
// - With nothing selected, the inspector offers Slide Number and Footer: no number, no footer,
//   and the slide as its layout has them.
// - Number: Each slide's is one `furnish` patch: the deck's header says so, the slide draws its
//   number at the right of its foot and nothing else moves, and the status says what changed.
// - A footer typed is one `furnish` patch, its words in the header, drawn from the left of the foot;
//   the same words with a space added are no edit; the field emptied takes it away, one `furnish`
//   of `null`.
// - This slide: Hides them is a `set_state` of `furniture` on the slide's first state, chosen
//   from its step too, and the slide draws as it did; As its layout has them takes it away.
// - The title slide, which Dusk leaves them off, shows them when it asks.
// - Neither is an object: the canvas lists no box for them, and lint finds nothing under them.
// - ⌘Z takes each back, a step at a time.
// - New layout from the title slide's leaves them off the copy, as Dusk leaves them off a title.
//
//   node web/furniture.mjs     (after `just web`; from the repository's root)
//
// Exits 1 on any failure.
import { launch, serve } from "./serve.mjs";

const server = await serve();
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1, acceptDownloads: true });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid && window.scaena.canvas.boxes()?.length, null, { timeout: 120000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  /** The deck's header, its lines joined: a long one goes on under it. */
  const header = async () => {
    const lines = (await source()).split("\n");
    const at = lines.findIndex((l) => l.startsWith("deck "));
    const end = lines.findIndex((l, i) => i > at && !l.startsWith("  "));
    return lines.slice(at, end).map((l) => l.trim()).join(" ");
  };
  const line = async (state) => (await source()).split("\n").find((l) => l.startsWith(`state ${state}`)) ?? "";
  /** The inspector's furniture, as it shows it. */
  const shows = () =>
    page.evaluate(() => {
      const value = (id) => document.querySelector(id)?.value;
      return {
        title: document.querySelector("#look .furniture-title")?.textContent,
        number: value("#look-number"),
        footer: value("#look-footer"),
        slide: value("#look-furniture"),
        lives: document.querySelector("#look-furniture")?.closest(".control")?.nextElementSibling?.textContent,
      };
    });
  /** Whether the inspector comes to say the slide's number and footer are `word`, Shown or
   * Hidden: it inspects again once the edit is applied and the source compiled, after the edit's
   * own promise settles. */
  const lives = (word) =>
    page
      .waitForFunction(
        (w) => document.querySelector("#look-furniture")?.closest(".control")?.nextElementSibling?.textContent === w,
        word,
        { timeout: 15000, polling: 50 },
      )
      .then(() => true, () => false);
  /** Once the source satisfies `test`, the edit is made and the inspector shows the deck again. */
  const until = async (test, arg) => {
    await page.waitForFunction(test, arg, { timeout: 30000, polling: 50 }).catch(() => {});
    await page.evaluate(() => window.scaena.look.settled());
    await page.waitForTimeout(300);
  };
  const undo = async () => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
  };
  /** Show `state` with nothing selected, and wait for the inspector to offer its choices. */
  const show = async (state, index) => {
    await page.evaluate((s) => window.scaena.cursor(window.scaena.source().indexOf(`state ${s}`) + "state ".length), state);
    await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.at().index === i, index, { timeout: 30000 });
    await page.evaluate(() => window.scaena.canvas.select(undefined));
    await page.waitForFunction((s) => window.scaena.look.offered()?.state === s && !("node" in window.scaena.look.offered()), state, {
      timeout: 30000,
    });
    await page.waitForSelector("#look .furniture-title", { timeout: 30000 });
  };
  /** The slide shown as the PNG export draws it, 960 wide, as RGBA. */
  const drawn = () =>
    page.evaluate(async () => {
      const shot = await window.scaena.exportAs("png", 960);
      const bitmap = await createImageBitmap(new Blob([shot.bytes], { type: "image/png" }));
      const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
      const g = canvas.getContext("2d");
      g.drawImage(bitmap, 0, 0);
      const { data, width, height } = g.getImageData(0, 0, bitmap.width, bitmap.height);
      return { data: Array.from(data), width, height };
    });
  /** Where two drawings differ: in the foot's margin (under Dusk's last row, 96 of 1080), at its
   * left and its right half, and above it. */
  const differ = (a, b) => {
    const foot = Math.floor(a.height * (1 - 96 / 1080));
    const out = { left: 0, right: 0, above: 0 };
    for (let y = 0; y < a.height; y++) {
      for (let x = 0; x < a.width; x++) {
        const i = (y * a.width + x) * 4;
        const d = Math.abs(a.data[i] - b.data[i]) + Math.abs(a.data[i + 1] - b.data[i + 1]) + Math.abs(a.data[i + 2] - b.data[i + 2]);
        if (d < 24) continue;
        if (y < foot) out.above++;
        else if (x < a.width / 2) out.left++;
        else out.right++;
      }
    }
    return out;
  };

  const original = await source();
  await show("revenue", 1);
  let seen = await shows();
  check(seen.title === "Slide Number and Footer", `with nothing selected, the inspector offers the slide's number and footer: ${seen.title}`);
  check(seen.number === "no" && seen.footer === "" && seen.slide === "", `none shown yet, the slide as its layout has them: ${JSON.stringify(seen)}`);
  const bare = await drawn();

  // The deck numbers its slides: one `furnish`, its header saying so.
  await page.selectOption("#look-number", "yes");
  await until(() => window.scaena.source().includes("furniture:{"));
  check(/furniture:\{number: true\}/.test(await header()), `the deck's header says it numbers its slides: ${await header()}`);
  check((await status()).startsWith("Slides numbered"), `the status says what changed: ${await status()}`);
  await lives("Shown");
  seen = await shows();
  check(seen.number === "yes" && seen.lives === "Shown", `the inspector shows them shown: ${JSON.stringify(seen)}`);
  const numbered = await drawn();
  const number = differ(bare, numbered);
  check(number.right > 20 && number.left === 0 && number.above === 0, `the slide draws its number at the right of its foot, and nothing else moves: ${JSON.stringify(number)}`);

  // A footer typed: one `furnish`, drawn from the left.
  await page.fill("#look-footer", "Q3 Review · Board");
  await page.locator("#overlay").focus();
  await until(() => window.scaena.source().includes('footer: "Q3 Review · Board"'));
  check((await header()).includes('footer: "Q3 Review · Board"'), `the footer's words are the deck's: ${await header()}`);
  check((await status()).includes("Q3 Review · Board"), `the status says the footer: ${await status()}`);
  const footed = differ(numbered, await drawn());
  check(footed.left > 20 && footed.right === 0 && footed.above === 0, `the footer is drawn from the left of the foot: ${JSON.stringify(footed)}`);
  // The same words with a space added change nothing: no patch, nothing refused, the field put back.
  const kept = await source();
  await page.fill("#look-footer", "Q3 Review · Board ");
  await page.locator("#overlay").focus();
  const refused = await page
    .waitForFunction(() => document.querySelector("#status").textContent.startsWith("Not changed"), null, { timeout: 3000, polling: 50 })
    .then(() => true, () => false);
  check(!refused, `a space added is no edit, and nothing is refused: ${await status()}`);
  check((await source()) === kept, "and the deck stays as it was");
  check((await page.inputValue("#look-footer")) === "Q3 Review · Board", `the field is put back: “${await page.inputValue("#look-footer")}”`);
  const findings = await page.evaluate(() => window.scaena.last().findings.filter((f) => f.code === "W303").length);
  check(findings === 0, `nothing on the example's slides stands under them: ${findings} W303`);
  const boxes = await page.evaluate(() => window.scaena.canvas.boxes().map((b) => b.node));
  check(boxes.length > 0 && !boxes.some((id) => id.startsWith(":")), `neither is an object the canvas selects: ${boxes}`);

  // The slide hides them: a `set_state` on its first state, chosen from its step.
  await show("mix", 2);
  await page.selectOption("#look-furniture", "no");
  await until(() => /^state revenue .*furniture:false/m.test(window.scaena.source()));
  check((await line("revenue")).includes("furniture:false"), `chosen from its step, it is written on the slide's first state: ${await line("revenue")}`);
  check(!(await line("mix")).includes("furniture"), `and not on the step: ${await line("mix")}`);
  check((await status()).startsWith("Slide 2: number and footer hidden"), `the status says so: ${await status()}`);
  await lives("Hidden");
  seen = await shows();
  check(seen.slide === "no" && seen.lives === "Hidden", `the step shows them hidden with its slide: ${JSON.stringify(seen)}`);
  await show("revenue", 1);
  const hidden = differ(bare, await drawn());
  check(hidden.left + hidden.right + hidden.above === 0, `the slide draws as it did with none: ${JSON.stringify(hidden)}`);
  await page.selectOption("#look-furniture", "");
  await until(() => !/^state revenue .*furniture/m.test(window.scaena.source()));
  check(!(await line("revenue")).includes("furniture"), `as its layout has them takes the say away: ${await line("revenue")}`);

  // The title slide, which Dusk leaves them off, asks for them.
  await show("intro", 0);
  seen = await shows();
  check(seen.slide === "" && seen.lives === "Hidden", `the title slide shows none, as its layout has them: ${JSON.stringify(seen)}`);
  await page.selectOption("#look-furniture", "yes");
  await until(() => /^state intro .*furniture:true/m.test(window.scaena.source()));
  check((await line("intro")).includes("furniture:true"), `asked for, they show on it: ${await line("intro")}`);
  check(await lives("Shown"), `the inspector says so: ${(await shows()).lives}`);

  // No words take the footer away: one `furnish` of `null`, the number as it was.
  await page.fill("#look-footer", "");
  await page.locator("#overlay").focus();
  await until(() => !window.scaena.source().includes("footer:"));
  check(!(await header()).includes("footer") && /furniture:\{number: true\}/.test(await header()), `no words take the footer away: ${await header()}`);
  check((await status()).startsWith("No footer"), `the status says so: ${await status()}`);

  // Each is one step to undo.
  for (let i = 0; i < 6; i++) await undo();
  await until((s) => window.scaena.source() === s, original);
  check((await source()) === original, "six undos take the deck back as it was");
  check(await page.evaluate(() => window.scaena.last().valid), "the source still compiles and validates");

  // A new layout from the title's leaves them off as the title's does: one theme edit.
  // The preview follows the caret to the title slide once the editor has taken the undos: the caret
  // leaves the slide's line and comes back until it does.
  for (let tries = 0; tries < 10 && (await page.evaluate(() => window.scaena.shown())) !== 0; tries++) {
    await page.evaluate(() => window.scaena.cursor(0));
    await page.waitForTimeout(500);
    await page.evaluate(() => window.scaena.cursor(window.scaena.source().indexOf("state intro") + "state ".length));
    await page.waitForFunction(() => window.scaena.shown() === 0, null, { timeout: 3000 }).catch(() => {});
  }
  await show("intro", 0);
  await page.selectOption("#look-number", "yes");
  await until(() => window.scaena.source().includes("furniture:{"));
  check(await lives("Hidden"), `numbered, the title slide shows none, as its layout has them: ${(await shows()).lives}`);
  const cover = await drawn();
  await page.evaluate(() => void window.scaena.canvas.newLayout());
  const field = await page.waitForSelector("input.note-words", { timeout: 10000 }).catch(() => null);
  check(field !== null, "New layout asks for its name");
  if (field) {
    await field.fill("Cover 2");
    await page.keyboard.press("Enter");
  }
  await until(() => /^state intro .*layout:cover-2/m.test(window.scaena.source()));
  check((await line("intro")).includes("layout:cover-2"), `the title slide takes the copy: ${await line("intro")}`);
  await page.locator("#overlay").focus();
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => !window.scaena.canvas.slotting(), null, { timeout: 10000 }).catch(() => {});
  await show("intro", 0);
  seen = await shows();
  check(seen.slide === "" && seen.lives === "Hidden", `the copy leaves them off, as the title's layout does: ${JSON.stringify(seen)}`);
  const copied = differ(cover, await drawn());
  check(copied.left + copied.right + copied.above === 0, `the title slide draws as it did, no number at its foot: ${JSON.stringify(copied)}`);
} finally {
  await browser.close();
  await server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "a slide's number and footer are set from the inspector, each a patch");
process.exit(failures.length ? 1 : 0);
