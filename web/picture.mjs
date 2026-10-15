// PLAN 2.107 picture check: Make a picture in the editor, in headless Chromium (serve.mjs), the
// image model played by a scripted server that speaks OpenAI's Images API. On the revenue example:
//
// - With no OpenAI or Gemini key kept, nothing is asked: the dialog says where a key goes, and
//   Add a key… opens the assistant on the key of the provider it chose, whatever the assistant
//   asked before.
// - On a slide started in Photo, with nothing selected, the picture fills the empty place for one,
//   framed wide as that place is: asked once, the key in OpenAI's header alone, of the subject in
//   the person's words, in the theme's palette. It joins the bundle, says what it shows, and is
//   selected; one undo takes it back, and the place waits again.
// - The picture selected, Make a picture to replace it (its right click) asks again with what it
//   shows written in, and the same image shows the new picture whole and says what it shows, the
//   old one's crop taken away: one patch, which one undo takes back.
// - A picture locked, selected, is not replaced: with no empty place on the slide, the picture
//   goes where the pointer last pressed, asked for square, and lands in its own proportions.
// - A provider that refuses says why in the dialog, and nothing changes.
//
//   node web/picture.mjs     (after `just web`; from the repository's root)
//
// Exits 1 on any failure.
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { launch, serve } from "./serve.mjs";

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

/** Pictures, as OpenAI's Images API gives them: JPEGs from the decoder's fixtures, one for each
 * picture asked for, in turn. */
const PICTURES = await Promise.all(["baseline-422", "baseline-444"].map(async (f) => (await readFile(`tests/fixtures/jpeg/${f}.jpg`)).toString("base64")));

/** OpenAI's Images API, scripted, on a port of its own: each picture it was asked for. A subject
 * that asks for a refusal is refused, as a provider's safety system refuses one. */
async function provider() {
  const painted = [];
  const server = createServer(async (req, res) => {
    const cors = {
      "access-control-allow-origin": req.headers.origin ?? "*",
      "access-control-allow-headers": req.headers["access-control-request-headers"] ?? "*",
      "access-control-allow-methods": "GET, POST, OPTIONS",
    };
    if (req.method === "OPTIONS") return res.writeHead(204, cors).end();
    const path = new URL(req.url, "http://x").pathname;
    const reply = (status, body) => res.writeHead(status, { ...cors, "content-type": "application/json" }).end(JSON.stringify(body));
    if (req.method === "GET" && path === "/v1/models") return reply(200, { data: [{ id: "gpt-4o" }, { id: "dall-e-3" }, { id: "gpt-image-1" }] });
    if (req.method !== "POST" || path !== "/v1/images/generations") return reply(404, { error: { message: `no ${req.method} ${path}` } });
    let text = "";
    for await (const chunk of req) text += chunk;
    const body = JSON.parse(text);
    painted.push({ headers: req.headers, body });
    if (body.prompt.includes("SUBJECT: refuse")) {
      return reply(400, { error: { message: "Your request was rejected by the safety system.", code: "moderation_blocked" } });
    }
    reply(200, { data: [{ b64_json: PICTURES[painted.length % PICTURES.length] }] });
  }).listen(0, "127.0.0.1");
  await new Promise((ok) => server.once("listening", ok));
  return { origin: `http://127.0.0.1:${server.address().port}`, painted, close: () => server.close() };
}

const site = await serve();
const scripted = await provider();
const browser = await launch();
try {
  const page = await browser.newPage({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1 });
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && !/Failed to load resource/.test(m.text()) && failures.push(`console: ${m.text()}`));
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid && window.scaena.canvas.boxes()?.length, null, { timeout: 120000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const says = async (text) => {
    await page.waitForFunction((t) => document.querySelector("#status").textContent.includes(t), text, { timeout: 30000 }).catch(() => {});
    return (await status()).includes(text);
  };
  const back = async (to) => {
    await page.waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 }).catch(() => {});
    return (await source()) === to;
  };
  const undo = async () => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
  };
  const dialog = () =>
    page.evaluate(() => ({
      open: document.querySelector("#picturing").open,
      subject: document.querySelector("#picturing-subject").value,
      where: document.querySelector("#picturing-where").textContent,
      who: document.querySelector("#picturing-who").textContent,
      status: document.querySelector("#picturing-status").textContent,
      key: !document.querySelector("#picturing-key").hidden,
      make: !document.querySelector("#picturing-make").disabled,
    }));
  /** The Insert menu's Make a picture…, and the dialog open. */
  const fromInsert = async () => {
    await page.selectOption("#insert", "make-picture");
    await page.waitForFunction(() => document.querySelector("#picturing").open, null, { timeout: 10000 }).catch(() => {});
    return dialog();
  };
  /** Once the deck has an image that says it shows `alt`: its id, its box, whether it is
   * selected, where its slot is, and the pictures the deck's images show. */
  const imageShowing = (alt) =>
    page
      .waitForFunction(
        (a) => {
          const text = window.scaena.source();
          const id = text.match(/^\s+(\S+) image\s*$/m)?.[1];
          const b = id && window.scaena.canvas.boxes()?.find((x) => x.node === id);
          if (!text.includes(`alt:"${a}"`) || !b) return undefined;
          return {
            id,
            rect: b.rect,
            selected: window.scaena.canvas.selected() === id,
            srcs: [...text.matchAll(/"(assets\/[^"]+)"/g)].map((m) => m[1]),
            art: text.includes(`alt:"${a}" at:in(art)`),
          };
        },
        alt,
        { timeout: 30000 },
      )
      .then((h) => h.jsonValue(), () => undefined);

  const original = await source();

  // 1. No key: nothing is asked, and the dialog says where one goes.
  const keyless = await fromInsert();
  check(keyless.open && keyless.key && !keyless.make, `with no key, the dialog offers Add a key… and makes nothing: ${JSON.stringify(keyless)}`);
  check(/No OpenAI key is kept: keep one in the Assistant tab/.test(keyless.who), `and says where one goes: ${keyless.who}`);
  await page.click("#picturing-key");
  await page.waitForFunction(() => document.activeElement?.id === "key", null, { timeout: 10000 }).catch(() => {});
  check(
    await page.evaluate(() => !document.querySelector("#picturing").open && document.activeElement?.id === "key"),
    "Add a key… opens the assistant on its key",
  );
  const keyFor = await page.evaluate(() => document.querySelector("#provider").value);
  check(keyFor === "openai", `the key it opens on is OpenAI's, the provider the dialog chose, not the assistant's own: ${keyFor}`);
  check(scripted.painted.length === 0, "nothing was asked");

  // An OpenAI key, at the scripted server, pasted where Add a key… put the focus.
  await page.fill("#base", scripted.origin);
  await page.fill("#key", "picture-test-key");
  await page.waitForFunction(() => document.querySelector("#model").options.length > 0, null, { timeout: 30000 });

  // 2. A slide started in Photo: the picture fills its empty place, framed wide as it is.
  await page.locator('#strip li[data-state="mix"]').click();
  await page.waitForFunction(() => window.scaena.at()?.index === 2, null, { timeout: 30000 }).catch(() => {});
  await page.click("#strip [data-add=slide]");
  await page.waitForSelector("#starting[open] button[data-start]", { timeout: 60000 });
  await page.locator("#starting").getByRole("tab", { name: "Pictures", exact: true }).click();
  await page.locator("#starting").getByRole("button", { name: "Photo", exact: true }).click();
  await page.waitForFunction(() => window.scaena.last()?.states[window.scaena.shown()]?.[0] === "photo" && window.scaena.canvas.boxes(), null, { timeout: 30000 }).catch(() => {});
  const started = await source();
  check(started.includes("state photo"), "Photo starts a slide");
  const asking = await fromInsert();
  check(asking.open && asking.make && !asking.key, `with a key kept, the dialog asks: ${JSON.stringify(asking)}`);
  check(asking.where === "It fills the empty place for a picture on this slide, wide as that place is.", `and says where it goes: ${asking.where}`);
  check(/Made by OpenAI's best image model your key lists/.test(asking.who), `and who makes it: ${asking.who}`);
  await page.fill("#picturing-subject", "A lighthouse at dusk");
  await page.selectOption("#picturing-look", "photo");
  await page.click("#picturing-make");
  await page.evaluate(() => window.scaena.picturing.settled());
  const [asked] = scripted.painted;
  check(scripted.painted.length === 1, `asked once: ${scripted.painted.length}`);
  check(asked?.headers.authorization === "Bearer picture-test-key", "the key in OpenAI's header");
  check(asked?.body.model === "gpt-image-1" && asked.body.size === "1536x1024", `by the best image model the key lists, wide: ${asked?.body.model} ${asked?.body.size}`);
  check(asked?.body.prompt.includes("SUBJECT: A lighthouse at dusk") && asked.body.prompt.includes("Editorial photography"), "of what it shows, as a photograph");
  check(asked?.body.prompt.includes("Landscape composition"), "composed wide");
  check(asked?.body.prompt.includes("#FF6A3D"), "in the theme's palette");
  check(await says("A picture of a lighthouse at dusk, made by gpt-image-1, inserted on this slide"), `the status says so: ${await status()}`);
  const made = await imageShowing("A lighthouse at dusk");
  check(made?.art && made.selected && made.rect.join() === "0,0,1920,1080", `it fills the place, edge to edge, selected: ${JSON.stringify(made)}`);
  check(made?.srcs.length === 1 && /^assets\/[0-9a-f]{64}\.jpg$/.test(made.srcs[0]), `named in the bundle by its SHA-256: ${made?.srcs}`);
  check(await page.evaluate(() => !window.scaena.canvas.waiting().length), "the place waits no more");
  const pictured = await source();
  await undo();
  check(await back(started), "one undo takes it back");
  await page.keyboard.press("Control+Shift+z");
  check(await back(pictured), "and redo puts it there again");
  // Redone before the undo's deck came back, the slide draws the picture again, its place
  // filled: the worker compiles the redone source, not the patch's deck it no longer holds.
  await page.waitForTimeout(1000);
  check(
    await page
      .waitForFunction(() => window.scaena.canvas.boxes()?.some((b) => b.node === "a-lighthouse-at-dusk") && !window.scaena.canvas.waiting().length, null, { timeout: 10000 })
      .then(() => true, () => false),
    "and the slide draws it again, its place filled",
  );

  // 3. The picture selected: its right click makes another in its place, whole, though the old
  // one showed a crop of its picture: the middle half of it.
  const cropped = pictured.replace('alt:"A lighthouse at dusk" at:in(art)', 'alt:"A lighthouse at dusk" at:in(art) crop:[0.25, 0, 0.5, 1]');
  await page.evaluate((text) => window.scaena.type(text), cropped);
  await page.waitForFunction(() => window.scaena.last()?.valid && /crop:\[0\.25/.test(window.scaena.source()), null, { timeout: 30000 }).catch(() => {});
  const first = await imageShowing("A lighthouse at dusk");
  await page.evaluate((id) => window.scaena.canvas.select(id), first?.id);
  const [cx, cy] = await page.evaluate(() => {
    const r = document.querySelector("#overlay").getBoundingClientRect();
    return [r.left + r.width / 2, r.top + r.height / 2];
  });
  await page.mouse.click(cx, cy, { button: "right" });
  await page.locator(".context-menu [role=menuitem]", { hasText: "Make a picture to replace it…" }).first().click();
  await page.waitForFunction(() => document.querySelector("#picturing").open, null, { timeout: 10000 }).catch(() => {});
  const replacing = await dialog();
  check(replacing.where === "It takes the place of the picture selected, wide as its place is.", `it takes the place of the picture selected: ${replacing.where}`);
  check(replacing.subject === "A lighthouse at dusk", `what it shows, written in: ${replacing.subject}`);
  await page.fill("#picturing-subject", "A harbor at night");
  await page.selectOption("#picturing-look", "illustration");
  await page.click("#picturing-make");
  await page.evaluate(() => window.scaena.picturing.settled());
  check(scripted.painted.at(-1)?.body.prompt.includes("SUBJECT: A harbor at night") && scripted.painted.at(-1).body.prompt.includes("editorial illustration"), "asked again, as an illustration");
  const second = await imageShowing("A harbor at night");
  check(second?.id === first?.id && second?.rect.join() === first?.rect.join(), `the same image shows it, where it stood: ${JSON.stringify(second)}`);
  check(second?.srcs.length === 1 && second.srcs[0] !== first?.srcs[0] && !(await source()).includes('alt:"A lighthouse at dusk"'), "the new picture, and what it shows, in place of the old");
  check(!(await source()).includes("crop:"), "shown whole, framed for its box: the old picture's crop taken away");
  check(await says("Replaced “A lighthouse at dusk” with a picture of a harbor at night, made by gpt-image-1"), `the status says so: ${await status()}`);
  await undo();
  check(await back(cropped), "one undo takes the new picture back, what it said it showed, and its crop");
  await undo();
  check(await back(pictured), "and the crop set before it goes with the next");

  // A picture locked (PLAN 2.95), selected from its layers, is not replaced: with no empty place
  // for one, the picture goes where the pointer last pressed, asked for square, and lands in a box
  // of the picture's own proportions, as Insert gives a picture (the scripted one is 67 × 45).
  const locked = await imageShowing("A lighthouse at dusk");
  await page.evaluate((id) => window.scaena.canvas.lock([id], true), locked?.id);
  await page.waitForFunction((id) => window.scaena.canvas.boxes()?.find((b) => b.node === id)?.locked, locked?.id, { timeout: 30000 }).catch(() => {});
  const lockedSource = await source();
  await page.evaluate((id) => window.scaena.canvas.select(id), locked?.id);
  // The palette offers no picture to replace it.
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+k");
  await page.waitForFunction(() => document.querySelector("#palette").open, null, { timeout: 10000 }).catch(() => {});
  await page.keyboard.type("picture");
  const offers = await page.evaluate(() => window.scaena.commands.shown());
  check(offers.includes("Make a picture…") && !offers.includes("Make a picture to replace it…"), `the palette offers no picture in place of the one locked: ${offers.join(" · ")}`);
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => !document.querySelector("#palette").open, null, { timeout: 10000 }).catch(() => {});
  const elsewhere = await fromInsert();
  check(elsewhere.where === "It goes on this slide where you last clicked, square.", `the picture locked is not replaced, and the picture is framed square: ${elsewhere.where}`);
  await page.fill("#picturing-subject", "A brass compass");
  await page.click("#picturing-make");
  await page.evaluate(() => window.scaena.picturing.settled());
  check(scripted.painted.at(-1)?.body.size === "1024x1024", `asked for square: ${scripted.painted.at(-1)?.body.size}`);
  const compass = await page
    .waitForFunction(() => window.scaena.source().includes('alt:"A brass compass"') && window.scaena.canvas.boxes()?.find((b) => b.node.startsWith("a-brass-compass"))?.rect, null, { timeout: 30000 })
    .then((h) => h.jsonValue(), () => undefined);
  const ratio = compass ? compass[2] / compass[3] : 0;
  check(Math.abs(ratio / (67 / 45) - 1) <= 0.05, `and lands in a box of the picture's own proportions: ${compass}`);
  check((await source()).includes('alt:"A lighthouse at dusk"'), "the picture locked stays as it was");
  await undo();
  check(await back(lockedSource), "one undo takes the picture back");
  await undo();
  check(await back(pictured), "and another the lock");

  // 4. Refused: the dialog says why, and nothing changes.
  const asked4 = scripted.painted.length;
  await page.evaluate(() => window.scaena.canvas.select(undefined));
  await fromInsert();
  await page.fill("#picturing-subject", "refuse this");
  await page.click("#picturing-make");
  await page.evaluate(() => window.scaena.picturing.settled());
  const refused = await dialog();
  check(refused.open && /^No picture: .*rejected by the safety system/.test(refused.status), `a refusal is said in the dialog: ${refused.status}`);
  check(!/^No picture: slide/.test(refused.status), "and names no slide of a made deck: the picture was for the slide shown");
  check(refused.make, "which asks again");
  check(scripted.painted.length === asked4 + 1, "asked once");
  check((await source()) === pictured, "and nothing changed");
  await page.click("#picturing-cancel");
  check(await page.evaluate(() => !document.querySelector("#picturing").open), "Cancel closes it");
  await undo();
  await undo();
  check(await back(original), "the deck is as it opened");
} finally {
  await browser.close();
  scripted.close();
  site.close();
}

if (failures.length) {
  console.error(`\n${failures.length} failed`);
  process.exit(1);
}
