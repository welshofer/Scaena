// PLAN 2.38 check: characters selected in a text typed in on the editor's canvas take a look, in
// headless Chromium (serve.mjs), the CPU painting, as in web/typing.mjs.
//
//   node web/runs.mjs     (after `just web`; from the repository's root)
//
// On the revenue example, typing in the title in `revenue`, which sets its text:
// - "doubled" selected, the inspector offers its look: role, emphasis, family, weight, color.
// - ⌘B makes it bold: one `style_text`, the title's runs where `revenue` sets its text, and the
//   selection drawn wider, as the engine sets bold wider. Typing stays where it was.
// - A color chosen in the inspector is the selected characters', and the text stays typed in.
// - ⌘B again, the characters bold by a weight of their own, takes that weight away; × takes the
//   color away, and the runs, all reading as the title does, are its text again.
// - Each is one step to undo. Escape stops typing, and the inspector shows the title again.
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
  const context = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(e.message));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const heading = () => page.evaluate(() => document.querySelector("#look h2")?.textContent ?? "");
  const typing = () => page.evaluate(() => window.scaena.canvas.typing());
  const client = ([x, y]) =>
    page.evaluate(
      ([x, y]) => {
        const r = document.querySelector("#overlay").getBoundingClientRect();
        const [w, h] = window.scaena.canvas.size();
        return [r.left + (x / w) * r.width, r.top + (y / h) * r.height];
      },
      [x, y],
    );
  /** How wide the selection is drawn, canvas units. */
  const drawn = () =>
    page.evaluate(() => [...document.querySelectorAll("#overlay rect.text-selection")].reduce((w, r) => w + Number(r.getAttribute("width")), 0));
  /** Once the source holds `text`, and nothing is with the worker. */
  const reads = async (text, what) => {
    const ok = await page
      .waitForFunction((t) => window.scaena.source().includes(t) && !window.scaena.canvas.typed()?.sending, text, { timeout: 30000, polling: 50 })
      .then(() => true)
      .catch(() => false);
    check(ok, `${what}: ${ok ? text : (await source()).split("\n").find((l) => l.includes("Revenue")) ?? ""}`);
    await page.waitForTimeout(200);
  };
  const showing = (words) =>
    page.waitForFunction((w) => document.querySelector("#look h2")?.textContent.includes(w), words, { timeout: 30000 }).then(
      () => true,
      () => false,
    );

  // The title in `revenue`, typed in.
  const at = (await source()).indexOf("state revenue") + "state ".length;
  await page.evaluate((offset) => window.scaena.cursor(offset), at);
  await page.waitForFunction(() => window.scaena.shown() === 1 && window.scaena.canvas.boxed() === "revenue" && window.scaena.canvas.boxes().length > 0, null, {
    timeout: 30000,
  });
  const [x, y, w, h] = await page.evaluate(() => window.scaena.canvas.boxes().find((b) => b.node === "title").rect);
  const [cx, cy] = await client([x + w / 2, y + h / 2]);
  await page.mouse.dblclick(cx, cy);
  await page.waitForFunction(() => window.scaena.canvas.typing() === "title", null, { timeout: 30000 });
  const original = await source();

  // "doubled": the end, and seven characters back.
  await page.keyboard.press("End");
  for (let i = 0; i < 7; i++) await page.keyboard.press("Shift+ArrowLeft");
  check(await showing("characters 9–15"), `the inspector offers the characters selected: ${await heading()}`);
  const props = await page.evaluate(() => [...document.querySelectorAll("#look [data-prop]")].map((e) => e.dataset.prop));
  check(
    ["role", "emphasis", "style/family", "style/weight", "style/color"].every((p) => props.includes(p)),
    `a run's role, emphasis, family, weight, and color: ${props.join(", ")}`,
  );
  const plain = await drawn();

  // ⌘B: bold, as runs where `revenue` sets the title's text.
  await page.keyboard.press("Control+b");
  await reads('runs:[{text: "Revenue "}, {text: doubled, style: {weight: 700}}]', "⌘B makes the characters bold");
  check((await status()).includes("bold"), `the status says so: ${await status()}`);
  const bold = await drawn();
  check(bold > plain, `the selection is drawn wider, as bold is set wider: ${plain.toFixed(1)} → ${bold.toFixed(1)}`);
  check((await typing()) === "title", "the title is still typed in");
  const sel = await page.evaluate(() => window.scaena.canvas.typed());
  check(sel?.from === 8 && sel?.to === 15, `the characters stay selected: ${sel?.from}–${sel?.to}`);
  const own = (text) => text.split("\n").find((l) => l.trimStart().startsWith("title text"));
  check(own(await source()) === own(original), `the title's own text is as it was: ${own(await source())?.trim()}`);

  // A color from the inspector, kept typing in.
  await page.waitForFunction(() => document.querySelector("#look-style-weight")?.value === "700", null, { timeout: 30000 }).catch(() => {});
  check((await page.evaluate(() => document.querySelector("#look-style-weight")?.value)) === "700", "the inspector shows the weight they take");
  await page.selectOption("#look-style-color", "accent");
  await reads("style: {weight: 700, color: accent}", "a color chosen in the inspector is theirs");
  check((await typing()) === "title", "choosing in the inspector keeps the title typed in");

  // ⌘B again: their own weight goes, and with the color gone too they read as the title does.
  await page.locator("textarea.typing").focus();
  await page.keyboard.press("Control+b");
  await reads("{text: doubled, style: {color: accent}}", "⌘B on bold characters takes their weight away");
  await page.waitForFunction(() => document.querySelector('#look [data-away="style/color"]'), null, { timeout: 30000 }).catch(() => {});
  await page.click('#look [data-away="style/color"]');
  const back = await page
    .waitForFunction(() => !window.scaena.source().includes("runs:") && !window.scaena.canvas.typed()?.sending, null, { timeout: 30000 })
    .then(() => true)
    .catch(() => false);
  check(back && (await source()).includes('"Revenue doubled"'), "× takes the color away, and the runs are the title's text again");

  // Each look a step to undo.
  await page.locator("textarea.typing").focus();
  await page.keyboard.press("Control+z");
  await reads("{text: doubled, style: {color: accent}}", "undo brings the color back");
  await page.keyboard.press("Control+z");
  await reads("style: {weight: 700, color: accent}", "and the weight");
  await page.keyboard.press("Control+z");
  await page.keyboard.press("Control+z");
  const undone = await page
    .waitForFunction((s) => window.scaena.source() === s, original, { timeout: 30000 })
    .then(() => true)
    .catch(() => false);
  check(undone, "four undos make the source what it was");

  // Escape: the title, not its characters.
  await page.keyboard.press("Escape");
  check(await showing("title · text"), `the inspector shows the title again: ${await heading()}`);
} catch (e) {
  failures.push(`error: ${e.message}`);
} finally {
  await browser.close();
  server.close();
}
if (failures.length) {
  console.log(`\n${failures.length} failed:\n${failures.map((f) => `  ${f}`).join("\n")}`);
  process.exit(1);
}
