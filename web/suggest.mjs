// PLAN 2.92 check: the layouts the editor's inspector suggests for the state shown, in headless
// Chromium (serve.mjs), the CPU painting, as in web/inspector.mjs.
//
//   node web/suggest.mjs     (after `just web`; from the repository's root)
//
// On the trails example, with nothing selected:
// - `budget` is drawn in each layout it may take, best first by what lint finds there: `figure`,
//   the one it takes now, which `poster` and `full` draw alike, then `narrow-figure`, which sets
//   its note over the table.
// - `storm`: a pointer over `narrow-figure` shows the state laid out in it on the canvas, nothing
//   made, and the canvas is as it was once the pointer leaves. A click gives the state that
//   layout: one `set_state` by the user, written where the layout lives, one step to undo; the
//   suggestions are judged again, the new one pressed. By keys: the focus on one shows it, Enter
//   chooses it, and the focus leaving lets it go.
// - A state every layout draws alike (`agenda`), or that places no node in a slot (`change`, its
//   cards on the grid), has nothing drawn to choose between.
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
  await page.goto(url("/docs/examples/trails.deck.json"));
  await page.waitForFunction(() => window.scaena?.last() || document.querySelector("#status")?.textContent.startsWith("error"), null, {
    timeout: 120000,
  });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const line = async (state) => (await source()).split("\n").find((l) => l.startsWith(`state ${state} `)) ?? "";
  const suggested = () => page.evaluate(() => window.scaena.look.suggested() ?? []);
  const buttons = () =>
    page.evaluate(() =>
      [...document.querySelectorAll("#look .layouts [data-layout]")].map((b) => ({
        layout: b.dataset.layout,
        pressed: b.getAttribute("aria-pressed"),
        says: [...b.querySelectorAll(".name, .verdict, .alike")].map((s) => s.textContent).join(" · "),
      })),
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
  const previewed = () => page.evaluate(() => window.scaena.look.previewed());
  const previewing = (layout) =>
    page.waitForFunction((l) => window.scaena.look.previewed().then((p) => p === (l ?? undefined)), layout ?? null, { timeout: 30000 }).catch(() => {});
  /** Wait for the inspector to have suggested layouts for `state`, the first `first`. */
  const suggestedFor = async (state, first) => {
    await page
      .waitForFunction(
        ([s, f]) => {
          const offered = window.scaena.look.offered();
          const all = window.scaena.look.suggested();
          return offered?.state === s && all?.[0]?.layout === f && document.querySelector("#look .layouts [data-layout]")?.dataset.layout === f;
        },
        [state, first],
        { timeout: 60000 },
      )
      .catch(() => {});
    await page.evaluate(() => window.scaena.look.settled());
  };
  const undo = async () => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
  };
  const showState = async (state) => {
    const at = (await source()).indexOf(`state ${state} `) + "state ".length;
    await page.evaluate((offset) => window.scaena.cursor(offset), at);
    await page.waitForFunction((s) => window.scaena.canvas.boxed() === s && window.scaena.canvas.boxes().length > 0, state, { timeout: 30000 });
    await page.evaluate(() => window.scaena.canvas.select(undefined));
    await page.waitForFunction((s) => window.scaena.look.offered()?.state === s && !("node" in window.scaena.look.offered()), state, {
      timeout: 30000,
    });
  };
  const original = await source();

  // budget: each layout that draws it otherwise, drawn, best first.
  await showState("budget");
  await suggestedFor("budget", "figure");
  const shown = await buttons();
  check(shown.map((b) => b.layout).join() === "figure,narrow-figure", `best first: ${shown.map((b) => b.layout).join(", ")}`);
  check(shown[0]?.pressed === "true" && shown[1]?.pressed === "false", "the layout it takes now is pressed");
  check(shown[0]?.says === "figure · nothing found · also poster, full", `poster and full draw it as figure does: ${shown[0]?.says}`);
  const last = (await suggested()).at(-1);
  check(last?.errors === 3 && shown[1]?.says === "narrow-figure · 3 errors", `narrow-figure says its errors: ${shown[1]?.says}`);
  check(
    last?.patch.length === 1 && last.patch[0].op === "set_state" && last.patch[0].value === "narrow-figure" && last.reach.join() === "budget",
    "it is one set_state that changes budget alone",
  );
  const drawn = await page.evaluate(() =>
    [...document.querySelectorAll("#look .layouts canvas")].map((c) => {
      const data = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
      let lit = 0;
      for (let i = 0; i < data.length; i += 4) if (data[i] + data[i + 1] + data[i + 2] > 60) lit++;
      return { width: c.width, height: c.height, css: c.getBoundingClientRect().height, lit, sum: data.reduce((a, b) => (a * 31 + b) % 1000000007, 0) };
    }),
  );
  check(drawn.length === 2 && drawn.every((d) => d.height === 72 && d.width === 128 && d.css === 72), `each is drawn 72 pixels high: ${JSON.stringify(drawn.map((d) => [d.width, d.height]))}`);
  check(drawn.every((d) => d.lit > 200) && drawn[0].sum !== drawn[1].sum, `each draws the slide its own way: ${drawn.map((d) => d.lit).join(", ")} pixels lit`);

  // storm: a pointer over narrow-figure shows it on the canvas, nothing made; leaving lets it go.
  await showState("storm");
  await suggestedFor("storm", "full");
  const storm = await buttons();
  check(storm.map((b) => b.says).join(" | ") === "full · nothing found · also poster, figure | narrow-figure · nothing found", `storm: ${storm.map((b) => b.says).join(" | ")}`);
  const before = await shot();
  await page.hover('#look .layouts [data-layout="narrow-figure"]');
  await previewing("narrow-figure");
  check((await previewed()) === "narrow-figure", "a pointer over narrow-figure previews it");
  check(!(await shot()).equals(before), "the canvas shows storm laid out in narrow-figure");
  check((await source()) === original, "nothing is made");
  await page.mouse.move(5, 5);
  await previewing(undefined);
  check((await previewed()) === undefined, "leaving lets it go");
  check((await shot()).equals(before), "the canvas is as it was");

  // A click gives storm the layout.
  await page.click('#look .layouts [data-layout="narrow-figure"]');
  await page.evaluate(() => window.scaena.look.settled());
  await page.waitForFunction(() => window.scaena.source().split("\n").find((l) => l.startsWith("state storm "))?.includes("layout:narrow-figure"), null, {
    timeout: 30000,
  }).catch(() => {});
  check((await line("storm")).includes("layout:narrow-figure"), `a click writes the layout where it lives: ${await line("storm")}`);
  check((await status()).includes("storm's layout: narrow-figure · in this state"), `the status says so: ${await status()}`);
  await suggestedFor("storm", "narrow-figure");
  const again = await buttons();
  check(again[0]?.layout === "narrow-figure" && again[0]?.pressed === "true", `judged again, narrow-figure is pressed: ${again.map((b) => `${b.layout} ${b.pressed}`).join(", ")}`);
  await page.mouse.move(5, 5);
  await previewing(undefined);
  check(!(await shot()).equals(before), "the canvas shows the layout chosen");
  await undo();
  await page.waitForFunction((s) => window.scaena.source() === s, original, { timeout: 30000 }).catch(() => {});
  check((await source()) === original, "one undo takes it back");

  // By keys: the focus shows one, Enter chooses it, and the focus leaving lets it go.
  await showState("storm");
  await suggestedFor("storm", "full");
  await page.focus('#look .layouts [data-layout="narrow-figure"]');
  await previewing("narrow-figure");
  check((await previewed()) === "narrow-figure", "the focus on narrow-figure previews it");
  await page.keyboard.press("Enter");
  await page.evaluate(() => window.scaena.look.settled());
  await page.waitForFunction(() => window.scaena.source().split("\n").find((l) => l.startsWith("state storm "))?.includes("layout:narrow-figure"), null, {
    timeout: 30000,
  }).catch(() => {});
  check((await line("storm")).includes("layout:narrow-figure"), `Enter chooses it: ${await line("storm")}`);
  await suggestedFor("storm", "narrow-figure");
  const kept = await page.evaluate(() => document.activeElement?.dataset.layout);
  check(kept === "narrow-figure", `the focus stays on it, drawn again: ${kept}`);
  // The other drawing is named for the first in the theme's order of the layouts that draw it.
  const other = (await buttons()).find((b) => b.pressed === "false")?.layout;
  check(other === "poster", `the other is poster, which full and figure draw alike: ${other}`);
  await page.focus(`#look .layouts [data-layout="${other}"]`);
  await previewing(other);
  check((await previewed()) === other, `the focus on ${other} previews it`);
  await page.locator("#overlay").focus();
  await previewing(undefined);
  check((await previewed()) === undefined, "the focus leaving lets the preview go");
  await page.keyboard.press("Control+z");
  await page.waitForFunction((s) => window.scaena.source() === s, original, { timeout: 30000 }).catch(() => {});
  check((await source()) === original, "and one undo takes it back");

  // Nothing drawn to choose between: every layout draws the agenda alike, and change's cards stand
  // on the grid, in no slot.
  for (const [state, why] of [
    ["agenda", "every layout draws it alike"],
    ["change", "it places no node in a slot"],
  ]) {
    await showState(state);
    await page
      .waitForFunction((s) => (window.scaena.look.suggested()?.length ?? 2) < 2 && window.scaena.look.offered()?.state === s, state, { timeout: 60000 })
      .catch(() => {});
    const none = await page.evaluate(() => ({
      count: window.scaena.look.suggested()?.length,
      hidden: document.querySelector("#look .layouts")?.hidden ?? true,
      offered: [...document.querySelectorAll("#look-layout option")].length - 1,
    }));
    check(none.hidden && none.count < 2 && none.offered > 1, `${state}: nothing drawn, as ${why} (${none.count} suggested, ${none.offered} in its layout field)`);
  }
  check(await page.evaluate(() => window.scaena.last().valid), "the source still compiles and validates");
} finally {
  await browser.close();
  await server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "a state's layouts are drawn, judged, previewed, and chosen");
process.exit(failures.length ? 1 : 0);
