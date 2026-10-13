// PLAN 2.103 furniture check: a slide's number and footer set from the editor's inspector
// (ADR-0028), in headless Chromium (serve.mjs), the CPU painting. On the revenue example in Dusk:
//
// - With nothing selected, the inspector offers Slide Number and Footer: no number, no footer,
//   and the slide as its layout has them.
// - Number: Each slide's is one `furnish` patch: the deck's header says so, the slide draws its
//   number at the right of its foot and nothing else moves, and the status says what changed.
// - A footer typed is one `furnish` patch, its words in the header, drawn from the left of the foot.
// - This slide: Hides them is a `set_state` of `furniture` on the slide's first state, chosen
//   from its step too, and the slide draws as it did; As its layout has them takes it away.
// - The title slide, which Dusk leaves them off, shows them when it asks.
// - Neither is an object: the canvas lists no box for them, and lint finds nothing under them.
// - ⌘Z takes each back, a step at a time.
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
  check((await shows()).lives === "Shown", `the inspector says so: ${(await shows()).lives}`);

  // Each is one step to undo.
  for (let i = 0; i < 5; i++) await undo();
  await until((s) => window.scaena.source() === s, original);
  check((await source()) === original, "five undos take the deck back as it was");
  check(await page.evaluate(() => window.scaena.last().valid), "the source still compiles and validates");
} finally {
  await browser.close();
  await server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "a slide's number and footer are set from the inspector, each a patch");
process.exit(failures.length ? 1 : 0);
