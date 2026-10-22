// PLAN 2.148 check: a slide's background, chosen in the editor's inspector, in headless Chromium
// (serve.mjs), the CPU painting, as in web/suggest.mjs.
//
//   node web/backgrounds.mjs     (after `just web`; from the repository's root)
//
// On the trails example, with nothing selected:
// - `cover` offers None, then the theme's presets, Backdrop and Texture, each a swatch of the slide
//   with it, its kind in words: Backdrop, its mesh, is the cover's now. The swatches move while the
//   row shows, and hold still under `prefers-reduced-motion`.
// - A pointer over Texture shows the cover with it on the canvas, nothing made, and leaving lets it
//   go. A click gives the cover Texture: one patch by the user, one step to undo, after which the
//   source is as it was. By keys, the row is a listbox: the arrows move the focus, which previews,
//   and Enter chooses.
// - The cover's mesh selected, the inspector edits its settings in a person's words: Drift, one
//   `choose` on the mesh, one step to undo.
// - `budget`, a table, says why backgrounds stay off it: each preset dimmed and refused with that
//   reason, the source as it was.
// - `promise`, with none, takes Backdrop; None takes it away again.
// Exits 1 on any failure.
import { launch, serve } from "./serve.mjs";

const server = await serve();
const url = `${server.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/trails.deck.json`;
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

const browser = await launch();
try {
  const open = async (options) => {
    const context = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1, ...options });
    const page = await context.newPage();
    page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
    page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
    await page.goto(url);
    await page.waitForFunction(() => window.scaena?.last() || document.querySelector("#status")?.textContent.startsWith("Error"), null, {
      timeout: 120000,
    });
    return page;
  };
  const page = await open({});

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const options = () =>
    page.evaluate(() =>
      [...document.querySelectorAll("#look .backgrounds [role=option]")].map((o) => ({
        name: o.querySelector(".name")?.textContent,
        kind: o.querySelector(".kind")?.textContent ?? "",
        selected: o.getAttribute("aria-selected"),
        disabled: o.getAttribute("aria-disabled"),
        tabindex: o.tabIndex,
      })),
    );
  /** Each swatch's pixels, summed: what tells one drawing from another. */
  const swatches = (p = page) =>
    p.evaluate(() =>
      [...document.querySelectorAll("#look .backgrounds canvas")].map((c) => {
        const data = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
        return { height: c.height, sum: data.reduce((a, b) => (a * 31 + b) % 1000000007, 0) };
      }),
    );
  /** The canvas as painted, without the guides and the findings' marks over it. */
  const shot = async () => {
    const hide = (v) => {
      document.querySelector("#overlay svg").style.visibility = v;
      document.querySelector("#marks").style.visibility = v;
    };
    await page.evaluate(hide, "hidden");
    const png = await page.locator("#stage").screenshot();
    await page.evaluate(hide, "");
    return png;
  };
  const previewing = (i) =>
    page
      .waitForFunction((want) => window.scaena.look.backgrounds.previewed().then((p) => p === (want ?? undefined)), i ?? null, { timeout: 30000 })
      .catch(() => {});
  const previewed = () => page.evaluate(() => window.scaena.look.backgrounds.previewed());
  const undo = async () => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
  };
  const showState = async (p, state) => {
    const at = (await p.evaluate(() => window.scaena.source())).indexOf(`state ${state} `) + "state ".length;
    await p.evaluate((offset) => window.scaena.cursor(offset), at);
    await p.waitForFunction((s) => window.scaena.canvas.boxed() === s && window.scaena.canvas.boxes().length > 0, state, { timeout: 30000 });
    await p.evaluate(() => window.scaena.canvas.select(undefined));
    await p.waitForFunction((s) => window.scaena.look.offered()?.state === s && !("node" in window.scaena.look.offered()), state, {
      timeout: 30000,
    });
  };
  /** Wait for the row to offer the state shown's backgrounds, `selected` the one it has. */
  const offeredWith = (p, selected) =>
    p
      .waitForFunction(
        (want) => {
          const all = window.scaena.look.backgrounds.offered();
          const chosen = document.querySelector('#look .backgrounds [aria-selected="true"] .name')?.textContent;
          return all !== undefined && chosen === want;
        },
        selected,
        { timeout: 60000 },
      )
      .catch(() => {});
  const original = await source();

  // cover: None, Backdrop, Texture; Backdrop its own.
  await showState(page, "cover");
  await offeredWith(page, "Backdrop");
  const cover = await options();
  check(
    cover.map((o) => `${o.name}${o.kind ? ` (${o.kind})` : ""}`).join(", ") === "None, Backdrop (Mesh Gradient), Texture (Noise)",
    `the cover offers ${cover.map((o) => `${o.name} ${o.kind}`).join(", ")}`,
  );
  check(cover.map((o) => o.selected).join() === "false,true,false", "Backdrop is the cover's");
  check(cover.map((o) => o.tabindex).join() === "-1,0,-1", "the focus roves: the one chosen takes it");
  check((await page.locator("#look .backgrounds [role=listbox]").count()) === 1, "the row is a listbox");
  const first = await swatches();
  check(first.length === 3 && first.every((s) => s.height === 54), `each swatch is 54 pixels high: ${first.map((s) => s.height)}`);
  check(new Set(first.map((s) => s.sum)).size === 3, "each draws the cover its own way");
  // The swatches move while the row shows.
  await page.waitForFunction(() => window.scaena.look.backgrounds.clock() >= 1, null, { timeout: 30000 }).catch(() => {});
  const later = await swatches();
  check(later[1]?.sum !== first[1]?.sum, "Backdrop's swatch moves");
  check(later[0]?.sum === first[0]?.sum, "None's holds still: nothing on the slide moves");

  // A pointer over Texture previews it, nothing made; leaving lets it go.
  const before = await shot();
  await page.hover("#look .backgrounds #background-2");
  await previewing(2);
  check((await previewed()) === 2, "a pointer over Texture previews it");
  check(!(await shot()).equals(before), "the canvas shows the cover with Texture");
  check((await source()) === original, "nothing is made");
  await page.mouse.move(5, 5);
  await previewing(undefined);
  check((await shot()).equals(before), "leaving lets it go: the canvas is as it was");

  // A click gives the cover Texture: one step to undo.
  await page.click("#look .backgrounds #background-2");
  await offeredWith(page, "Texture");
  const textured = await source();
  check(textured !== original && /preset:texture/.test(textured), "a click gives the cover Texture");
  check((await status()).includes("background Texture"), `the status says it: ${await status()}`);
  await undo();
  await page.waitForFunction((s) => window.scaena.source() === s, original, { timeout: 30000 }).catch(() => {});
  check((await source()) === original, "one undo gives the cover back as it was");

  // By keys: the arrows move the focus, which previews; Enter chooses.
  await showState(page, "cover");
  await offeredWith(page, "Backdrop");
  await page.focus("#look .backgrounds #background-1");
  await page.keyboard.press("ArrowRight");
  check((await page.evaluate(() => document.activeElement?.id)) === "background-2", "ArrowRight moves the focus to Texture");
  await previewing(2);
  check((await previewed()) === 2, "the focus on Texture previews it");
  await page.keyboard.press("Enter");
  await offeredWith(page, "Texture");
  check(/preset:texture/.test(await source()), "Enter chooses it");
  await undo();
  await page.waitForFunction((s) => window.scaena.source() === s, original, { timeout: 30000 }).catch(() => {});
  check((await source()) === original, "and undo takes it back");

  // The mesh selected: its settings in a person's words, each one choose.
  await showState(page, "cover");
  await page.evaluate(() => window.scaena.canvas.select("cover-bg"));
  await page.waitForFunction(() => window.scaena.look.offered()?.node === "cover-bg", null, { timeout: 30000 }).catch(() => {});
  await page.waitForSelector("#look-params-drift", { timeout: 30000 }).catch(() => {});
  const labels = await page.evaluate(() => [...document.querySelectorAll("#look .fields label")].map((l) => l.textContent.trim()));
  check(
    ["Effect", "Colors", "Color Points", "Drift", "Softness", "Grain"].every((l) => labels.includes(l)),
    `the mesh's settings are labeled in words: ${labels.join(", ")}`,
  );
  check(!labels.some((l) => l.includes("params")), "no setting is labeled by its path");
  await page.fill("#look-params-drift", "0.3");
  // Let go, as a person leaves a field: its change is made once.
  await page.locator("#look-params-drift").blur();
  await page.waitForFunction((s) => window.scaena.source() !== s, original, { timeout: 30000 }).catch(() => {});
  check(/drift: ?0\.3/.test(await source()), "Drift is set on the mesh");
  await undo();
  await page.waitForFunction((s) => window.scaena.source() === s, original, { timeout: 30000 }).catch(() => {});
  check((await source()) === original, "one undo takes it back");

  // budget, a table: each preset dimmed, and why.
  await showState(page, "budget");
  await offeredWith(page, "None");
  const budget = await options();
  check(budget.map((o) => o.disabled).join() === ",true,true", `budget's presets are dimmed: ${budget.map((o) => o.disabled)}`);
  const held = await page.evaluate(() => document.querySelector("#look .backgrounds .held")?.textContent ?? "");
  check(held.startsWith("Backgrounds stay off slides with a chart or a table"), `it says why: ${held}`);
  // Dimmed, as aria-disabled says, and pressed all the same, as a person may.
  await page.click("#look .backgrounds #background-1", { force: true });
  await page.waitForTimeout(500);
  check((await source()) === original, "a preset there is refused");
  check((await status()).includes("Backgrounds stay off"), `with the reason: ${await status()}`);

  // promise, with none: Backdrop given, then None takes it away.
  await showState(page, "promise");
  await offeredWith(page, "None");
  await page.click("#look .backgrounds #background-1");
  await offeredWith(page, "Backdrop");
  const given = await source();
  check(given !== original && /promise-background/.test(given), "promise takes Backdrop");
  await page.click("#look .backgrounds #background-0");
  await page.waitForFunction(() => !/promise-background/.test(window.scaena.source()), null, { timeout: 30000 }).catch(() => {});
  await offeredWith(page, "None");
  check(!/promise-background/.test(await source()), `None takes it away: ${await status()}`);
  await page.screenshot({ path: "target/web-smoke/backgrounds.png" }).catch(() => {});

  // Less motion: the swatches hold still.
  const still = await open({ reducedMotion: "reduce" });
  await showState(still, "cover");
  await offeredWith(still, "Backdrop");
  const a = await swatches(still);
  await still.waitForTimeout(1500);
  const b = await swatches(still);
  check(a.length === 3 && a.every((s, i) => s.sum === b[i].sum), "under prefers-reduced-motion the swatches hold still");
} finally {
  await browser.close();
  server.close();
}

if (failures.length) {
  console.error(`${failures.length} failure(s):\n${failures.join("\n")}`);
  process.exit(1);
}
