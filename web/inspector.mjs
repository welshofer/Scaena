// PLAN 2.33 inspector check: a node's look chosen in the editor's inspector, in headless Chromium
// (serve.mjs), the CPU painting, as in web/canvas.mjs.
//
//   node web/inspector.mjs     (after `just web`; from the repository's root)
//
// On the revenue example:
// - Selecting the title shows what the theme offers for it: its role among the theme's roles, the
//   value `revenue` shows, and where that value lives.
// - Choosing a role is one `choose` patch, by the user, written where the role lives (`revenue`'s
//   delta), and the status says which states it changes; one undo takes it back.
// - "Only in revenue" keeps a choice to the state: the note's role goes into its delta there, and
//   the node keeps its own.
// - A color written out is an override: it goes in the deck's `overrides` and shows as one; a
//   theme name chosen then is written there too; the × takes it away.
// - What cannot be kept to one state (a value written out) is refused, and says why.
// - An inspection that offers the same, as each edit's does, leaves the controls in place.
// - With nothing selected, the inspector edits the state shown (PLAN 2.36): `mix`'s layout, from
//   the theme's layouts with a slot for each node, written in `revenue`, where it lives, or kept to
//   `mix`; its transition, which a cut takes away; its hold, in seconds; and its notes. Each is one
//   `set_state` patch, one step to undo.
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
  /** What the inspector offers for `prop` of the node it shows: its value and where it lives. */
  const field = (prop) => page.evaluate((p) => window.scaena.look.offered()?.fields.find((f) => f.prop === p), prop);
  /** Once the source holds `text`, the choice is made, and the inspector shows the deck again. */
  const reads = async (text) => {
    await page.waitForFunction((t) => window.scaena.source().includes(t), text, { timeout: 30000, polling: 50 }).catch(() => {});
    await page.evaluate(() => window.scaena.look.settled());
    await page.waitForTimeout(300);
    return (await source()).includes(text);
  };
  const back = async (to) => {
    await page.waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 }).catch(() => {});
    // The inspector is drawn again once the edit is shown: a control written to before then is
    // drawn over.
    await page.evaluate(() => window.scaena.look.settled());
    await page.waitForTimeout(300);
    return (await source()) === to;
  };
  /** Select `node` in the state shown, and wait for the inspector to show it. */
  const select = async (node) => {
    await page.evaluate((n) => window.scaena.canvas.select(n), node);
    await page.waitForFunction((n) => window.scaena.look.offered()?.node === n, node, { timeout: 30000 });
  };

  const at = (await source()).indexOf("state revenue") + "state ".length;
  await page.evaluate((offset) => window.scaena.cursor(offset), at);
  await page.waitForFunction(() => window.scaena.shown() === 1 && window.scaena.at().index === 1, null, { timeout: 30000 });
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "revenue" && window.scaena.canvas.boxes().length > 0, null, { timeout: 30000 });
  const original = await source();

  // Selecting the title shows what the theme offers for it, and where its role lives.
  await select("title");
  const heading = await page.evaluate(() => document.querySelector("#look h2")?.textContent);
  check(heading === "title · text", `the inspector shows the title: ${heading}`);
  const roles = await page.evaluate(() => [...document.querySelectorAll("#look-role option")].map((o) => o.textContent));
  check(["display", "headline", "title", "caption"].every((r) => roles.includes(r)), `the role takes the theme's roles: ${roles.join(", ")}`);
  check((await page.inputValue("#look-role")) === "headline", "it shows the role `revenue` sets");
  const where = await page.evaluate(() => document.querySelector("#look-role")?.closest(".control")?.nextElementSibling?.textContent);
  check(where?.includes("set in revenue"), `and says where it lives: ${where}`);

  // Choosing a role: one patch, written where the role lives, and the states it reaches.
  await page.selectOption("#look-role", "title");
  check(await reads('"Revenue doubled" role:title'), "choosing a role writes it where it lives, `revenue`'s delta");
  const said = await status();
  check(said.includes("title's role: title") && said.includes("in 2 states"), `the status says what changed and where: ${said}`);
  check((await source()).includes('title text role:display "Q3 Review"'), "the node keeps its own role");
  check((await field("role"))?.value === "title", "the inspector shows the role chosen");
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  check(await back(original), "one undo takes the choice back");

  // Kept to the state: the note's role goes into `revenue`'s delta, and `mix` takes it from there.
  await select("note");
  await page.check("#look [data-keep]");
  await page.selectOption("#look-role", "body");
  await page.evaluate(() => window.scaena.look.settled());
  await page.waitForFunction(() => window.scaena.look.offered()?.fields.find((f) => f.prop === "role")?.value === "body", null, { timeout: 30000 }).catch(() => {});
  const kept = await field("role");
  check(kept?.value === "body" && kept?.lives?.state === "revenue", `only in revenue: the note's role lives in its delta: ${JSON.stringify(kept)}`);
  check((await source()).includes('note text role:caption "Revenue in $M.'), "and the node keeps its own");
  // A color written out cannot be kept to one state: it goes in the overrides, which hold in every one.
  await page.evaluate(() => {
    const input = document.querySelector("#look-style-color-written");
    input.value = "#ff3366";
    input.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await page.evaluate(() => window.scaena.look.settled());
  const refused = await status();
  check(refused.startsWith("not chosen") && refused.includes("cannot be kept to `revenue`"), `kept to a state, a color written out is refused: ${refused}`);
  await page.uncheck("#look [data-keep]");
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  check(await back(original), "and undo takes the kept choice back");

  // A color written out is an override, in every state, and shows as one.
  await select("title");
  // An inspection that offers the same, as each edit's does, leaves the controls in place: a
  // change on its way to one, or a color picker open on it, still lands.
  const stays = await page.evaluate(async () => {
    const input = document.querySelector("#look-style-color-written");
    await window.scaena.look.show("title");
    return input?.isConnected;
  });
  check(stays, "an inspection that offers the same leaves the controls in place");
  await page.evaluate(() => {
    const input = document.querySelector("#look-style-color-written");
    input.value = "#ff3366";
    input.dispatchEvent(new Event("change", { bubbles: true }));
  });
  check(await reads("override title"), "a color written out goes in the deck's overrides");
  // The inspector is drawn again from the deck the edit made, which takes the worker a while.
  await page
    .waitForFunction(
      () =>
        window.scaena.look.offered()?.fields.find((f) => f.prop === "style/color")?.lives === "overrides" &&
        document.querySelector("#look-style-color")?.closest(".control")?.nextElementSibling?.textContent.includes("override"),
      null,
      { timeout: 30000 },
    )
    .catch(() => {});
  const red = await field("style/color");
  check(red?.value === "#ff3366" && red?.lives === "overrides", `the inspector shows it as an override: ${JSON.stringify(red)}`);
  const flagged = await page.evaluate(() => document.querySelector("#look-style-color")?.closest(".control")?.nextElementSibling?.textContent);
  check(flagged?.includes("override"), `and flags it: ${flagged}`);
  check(!(await page.evaluate(() => window.scaena.last().findings.some((f) => f.code === "W300"))), "in the overrides, no W300");
  // A theme color chosen now goes there too, since the overrides win in every state.
  await page.selectOption("#look-style-color", "accent");
  await page.evaluate(() => window.scaena.look.settled());
  await page.waitForFunction(() => window.scaena.look.offered()?.fields.find((f) => f.prop === "style/color")?.value === "accent", null, { timeout: 30000 }).catch(() => {});
  check((await field("style/color"))?.lives === "overrides", "a theme color chosen over an override is written there");
  // The × takes it away where it lives: the overrides go, and the deck is as it was.
  await page.click('#look [data-away="style/color"]');
  check(await back(original), "the × takes the override away");
  await page.waitForFunction(() => !window.scaena.look.offered()?.fields.find((f) => f.prop === "style/color")?.value, null, { timeout: 30000 }).catch(() => {});
  check((await field("style/color"))?.value === undefined, "and the role's color shows again");
  check((await page.evaluate(() => window.scaena.last().valid)), "the source still compiles and validates");

  // With nothing selected, the inspector edits the state shown.
  const stated = (prop) => page.evaluate((p) => window.scaena.look.offered()?.fields.find((f) => f.prop === p), prop);
  const mixLine = async () => (await source()).split("\n").find((l) => l.startsWith("state mix")) ?? "";
  const slot = () => page.evaluate(() => window.scaena.last().slots.find((s) => s.state === "mix"));
  const undo = async () => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
  };
  await page.evaluate((offset) => window.scaena.cursor(offset), (await source()).indexOf("state mix") + "state ".length);
  await page.waitForFunction(() => window.scaena.shown() === 2 && window.scaena.at().index === 2, null, { timeout: 30000 });
  await page.evaluate(() => window.scaena.canvas.select(undefined));
  await page.waitForFunction(() => window.scaena.look.offered()?.state === "mix" && !("node" in window.scaena.look.offered()), null, {
    timeout: 30000,
  });
  const titled = await page.evaluate(() => document.querySelector("#look h2")?.textContent);
  check(titled === "mix · state", `with nothing selected, the inspector shows the state: ${titled}`);
  const layouts = await page.evaluate(() => [...document.querySelectorAll("#look-layout option")].slice(1).map((o) => o.textContent));
  check(layouts.join() === "full,figure,narrow-figure", `its layout takes the theme's layouts with a slot for each node: ${layouts}`);
  check((await page.inputValue("#look-layout")) === "figure", "it shows the layout `mix` takes");
  const from = await page.evaluate(() => document.querySelector("#look-layout")?.closest(".control")?.nextElementSibling?.textContent);
  check(from?.includes("set in revenue"), `from revenue, which sets it: ${from}`);

  // A layout chosen is written where it lives, and reaches the states that take it from there.
  await page.selectOption("#look-layout", "full");
  check(await reads("state revenue layout:full"), "a layout chosen in mix is written in revenue, where it lives");
  const told = await status();
  check(told.includes("mix's layout: full") && told.includes("in 2 states"), `the status says what changed and where: ${told}`);
  await undo();
  check(await back(original), "one undo takes it back");
  // Kept to `mix`, it is its own.
  await page.check("#look [data-keep]");
  await page.selectOption("#look-layout", "narrow-figure");
  await page.evaluate(() => window.scaena.look.settled());
  await page.waitForFunction(() => window.scaena.source().includes("layout:narrow-figure"), null, { timeout: 30000 }).catch(() => {});
  check((await mixLine()).includes("layout:narrow-figure") && (await source()).includes("state revenue layout:figure"), `layout only in mix: ${await mixLine()}`);
  await page.uncheck("#look [data-keep]");
  await undo();
  check(await back(original), "and one undo takes it back");

  // Its transition, its own: a duration from the theme, then an ease, then none, a cut.
  const slow = (await slot()).span;
  await page.selectOption("#look-transition-duration", "fast");
  check(await reads("state mix slide:revenue transition:fast"), `a duration chosen: ${await mixLine()}`);
  check((await slot()).span < slow, `the cue is shorter: ${(await slot()).span} ms, from ${slow}`);
  await page.selectOption("#look-transition-ease", "out");
  check(await reads("ease: out"), `an ease makes the transition an object: ${await mixLine()}`);
  check((await stated("transition/ease"))?.lives?.state === "mix", "set in mix");
  await page.click('#look [data-away="transition/ease"]');
  await page.evaluate(() => window.scaena.look.settled());
  await page.click('#look [data-away="transition/duration"]');
  await page.evaluate(() => window.scaena.look.settled());
  await page.waitForFunction(() => !window.scaena.source().split("\n").find((l) => l.startsWith("state mix"))?.includes("transition"), null, {
    timeout: 30000,
  }).catch(() => {});
  check(!(await mixLine()).includes("transition"), `taken away, the state cuts: ${await mixLine()}`);
  await page.waitForFunction(() => window.scaena.last().slots.find((s) => s.state === "mix").span === 0, null, { timeout: 30000 }).catch(() => {});
  check((await slot()).span === 0, "its cue is a cut");
  const cut = await page.evaluate(() => document.querySelector("#look-transition-duration option")?.textContent);
  check(cut === "cut", `the duration says so: ${cut}`);

  // Its hold, in seconds, and its notes.
  await page.locator("#look-hold").evaluate((input) => {
    input.value = "4.5";
    input.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await page.evaluate(() => window.scaena.look.settled());
  await page.waitForFunction(() => window.scaena.last().slots.find((s) => s.state === "mix").hold === 4500, null, { timeout: 30000 }).catch(() => {});
  check((await slot()).hold === 4500, `a hold in seconds holds the state: ${(await slot()).hold} ms`);
  check((await status()).includes("mix's hold: 4.5 s"), `the status says it in seconds: ${await status()}`);
  await page.fill("#look-notes", "Let the stack land.");
  await page.locator("#overlay").focus();
  check(await reads('notes "Let the stack land."'), "notes written in the inspector are the state's");
  for (let i = 0; i < 6; i++) await undo();
  check(await back(original), "an undo a change takes the state back as it was");
  check(await page.evaluate(() => window.scaena.last().valid), "the source still compiles and validates");
} finally {
  await browser.close();
  await server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "a node's look and a state's are chosen from the theme, each a patch");
process.exit(failures.length ? 1 : 0);
