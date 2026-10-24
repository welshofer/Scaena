// PLAN G.5 check: New from a prompt in the editor, in headless Chromium (serve.mjs), Anthropic
// played by a scripted server that answers as the recorded answers do
// (crates/scaena-narrato/tests/answers): the arc, then the slides, then the texts asked for
// shorter, each cut to the length asked.
//
//   node web/generate.mjs     (after `just web`; from the repository's root)
//
// - The dialog offers Narrato's goals, lengths, and arcs, read from the generator's module as it
//   opens, and says who writes the deck: the assistant's provider and model, with the user's key.
// - What a browser cannot read says so, and where it is read: an SEC filing, and a page whose
//   site lets no other page read it. A provider that refuses the key says so in the dialog,
//   which stays open to try again.
// - Cancel stops the worker making the deck, and the deck open stays open.
// - A deck in a script the theme's fonts do not draw takes the script's font (PLAN G.22), before
//   the provider is asked anything: Japanese's, which cannot be fetched here, says so; Hindi's is
//   fetched from where google/fonts serves it, once, and kept by the browser for the next deck.
// - A deck grounded in a Markdown file, a CSV, and a web page: what each gave is in the slides'
//   prompt, the key went in Anthropic's header with its opt-in for a browser, and the deck opens
//   in the editor kept nowhere, its slides the recorded answer's, the CSV a data source of it,
//   and no error lint finds.
// - Its pictures (PLAN G.10) made by OpenAI's best image model the OpenAI key lists, played here
//   too: two of five slides, the cover and the ending, each asked for with no word in it, the
//   OpenAI key in OpenAI's header alone, and each shown in its slide's picture place.
// - The deck shown to the model as it is drawn (PLAN G.13): each slide a JPEG after what is said
//   of it, and the cover's title as the model tightened it.
// - Its story (PLAN G.9): the Story tab reads it as Vzestup measures a talk, by the generator's
//   module: its score and the arc it was written to, the six measures, coherence by words
//   unscored, a mark on the arc for each slide. No fix rewrites the plan's intensity or tone. A
//   slide moved out of its beat's order has drifted: a finding's slide is shown at a press; its
//   fix is one change, which ⌘Z takes back; and its question goes into the assistant's box.
// Exits 1 on any failure.
import { mkdir, readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { launch, serve, tab } from "./serve.mjs";

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

const answers = "crates/scaena-narrato/tests/answers";
const ARC = await readFile(`${answers}/anthropic-arc.json`, "utf8");
const SLIDES = await readFile(`${answers}/anthropic-slides.json`, "utf8");
/** The recorded answer, its slides two lists whose items each name an icon (PLAN G.23). */
const ICONED = (() => {
  const answer = JSON.parse(SLIDES);
  const three = ["Fast: a crossing in a day", "Safe: checked at every port", "Shared: one ledger for all"];
  const four = ["Plan: the route set a week out", "Track: each crate seen", "Agree: one price", "Arrive: on time"];
  answer.content.find((c) => c.type === "text").text = JSON.stringify({
    executiveSummary: "Cargo that pays for its carbon.",
    slides: [
      { slideTitle: "Tidewater", subtitle: "Series A", layoutSuggestion: "Title", keyBullets: [] },
      { slideTitle: "Why Tidewater wins", layoutSuggestion: "Bullet List", keyBullets: three, icons: ["lightning", "shield", "users"] },
      { slideTitle: "Cargo can pay for the carbon it moves", layoutSuggestion: "Statement", keyBullets: [] },
      { slideTitle: "How we work", layoutSuggestion: "Bullet List", keyBullets: four, icons: ["compass", "chart", "handshake", "flag"] },
      { slideTitle: "Sail with us", layoutSuggestion: "Closing", keyBullets: [] },
    ],
  });
  return JSON.stringify(answer);
})();
/** The same answer, the first list's second item naming its icon by a dash, as a model writes for
 * an item it cannot picture: a word with no letter, which names no icon. */
const DASHED = ICONED.replace(String.raw`[\"lightning\",\"shield\",\"users\"]`, String.raw`[\"lightning\",\"—\",\"users\"]`);
const NOTES = "# Tidewater field notes\n\nPort calls doubled in Q3, and every berth ran on shore power.";
/** A picture, as OpenAI's Images API gives one: a JPEG from the decoder's fixtures. */
const PICTURE = (await readFile("tests/fixtures/jpeg/baseline-422.jpg")).toString("base64");
/** The font a deck in Hindi is set in, as google/fonts serves it (PLAN G.22). */
const DEVANAGARI = await readFile("tests/fixtures/scripts/NotoSansDevanagari-VF.ttf");
/** What the image API was asked. */
const painted = [];
const PAGE = "<h1>Tidewater in the press</h1><p>The Gulf's first carbon-negative cargo line opened its third route.</p>";

/** Anthropic, scripted: what each request asks, and the state the walk sets (`slow`, `fail`, and
 * the `slides` it answers with, the recorded ones where none is set). */
const requests = [];
const state = { slow: 0, fail: false, slides: undefined };
const provider = createServer(async (req, res) => {
  const cors = {
    "access-control-allow-origin": req.headers.origin ?? "*",
    "access-control-allow-headers": req.headers["access-control-request-headers"] ?? "*",
    "access-control-allow-methods": "GET, POST, OPTIONS",
  };
  const path = new URL(req.url, "http://x").pathname;
  if (req.method === "OPTIONS") return res.writeHead(204, cors).end();
  const reply = (status, body) =>
    res.writeHead(status, { ...cors, "content-type": "application/json" }).end(typeof body === "string" ? body : JSON.stringify(body));
  // OpenAI's image models, and a picture for each one asked for.
  if (req.method === "GET" && path === "/v1/models") return reply(200, { data: [{ id: "gpt-4o" }, { id: "dall-e-3" }, { id: "gpt-image-1" }] });
  if (req.method === "POST" && path === "/v1/images/generations") {
    let asked = "";
    for await (const chunk of req) asked += chunk;
    painted.push({ headers: req.headers, body: JSON.parse(asked) });
    return reply(200, { data: [{ b64_json: PICTURE }] });
  }
  if (req.method !== "POST" || path !== "/v1/messages") return reply(404, { error: { message: `no ${req.method} ${path}` } });
  let text = "";
  for await (const chunk of req) text += chunk;
  const body = JSON.parse(text);
  const prompt = body.messages[0].content.find((c) => c.type === "text")?.text ?? "";
  const system = (body.system ?? []).map((s) => s.text).join("\n");
  const asked = system.includes("tightening the words")
    ? "fit"
    : system.includes("art director")
      ? "look"
      : system.includes("planning a presentation before its slides")
        ? "outline"
        : prompt.includes("=== CATALOG ===")
          ? "arc"
          : "slides";
  requests.push({ headers: req.headers, body, prompt, asked });
  if (state.slow) await new Promise((ok) => setTimeout(ok, state.slow));
  if (state.fail) return reply(401, { type: "error", error: { type: "authentication_error", message: "invalid x-api-key" } });
  if (asked === "arc") return reply(200, ARC);
  // The outline before the slides (narrative FUNC-1): the big idea and a claim for each of the
  // arc's six beats.
  if (asked === "outline") {
    const beats = [1, 2, 3, 4, 5, 6].map((beat) => ({ beat, claim: `Claim ${beat}: what this beat's slides leave behind.` }));
    return reply(200, { content: [{ type: "text", text: JSON.stringify({ bigIdea: "Cargo can pay for its carbon.", beats }) }], stop_reason: "end_turn" });
  }
  if (asked === "slides") return reply(200, state.slides ?? SLIDES);
  if (asked === "look") {
    const look = { slides: [{ slide: 1, layout: "", title: "Carbon-negative cargo, ready to scale", subtitle: "", bullets: [], why: "One idea, one line." }] };
    return reply(200, { content: [{ type: "text", text: JSON.stringify(look) }], stop_reason: "end_turn" });
  }
  // Each text asked for shorter, cut to the most characters it may have.
  const texts = [...prompt.matchAll(/id: (.+)\nat most: (\d+) characters\ntext:\n([^\n]*)/g)].map(([, id, most, said]) => ({
    id,
    text: said.slice(0, Math.max(1, Number(most) - 1)).trim(),
  }));
  reply(200, { content: [{ type: "text", text: JSON.stringify({ texts }) }], stop_reason: "end_turn" });
}).listen(0, "127.0.0.1");
await new Promise((ok) => provider.once("listening", ok));
const origin = `http://127.0.0.1:${provider.address().port}`;

const site = await serve();
const browser = await launch();
try {
  const page = await browser.newPage({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && !/Failed to (load resource|fetch)|CORS/.test(m.text()) && failures.push(`console: ${m.text()}`));
  page.on("dialog", (d) => d.accept());
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last() && window.scaena.where().name === "revenue", null, { timeout: 120000 });
  // The assistant's settings, as its tab keeps them: Anthropic at the scripted server, a model,
  // and a key for this tab.
  await page.evaluate((origin) => {
    localStorage.setItem(
      "scaena.assistant",
      JSON.stringify({ provider: "anthropic", models: { anthropic: "scripted" }, bases: { anthropic: origin, openai: origin } }),
    );
    sessionStorage.setItem("scaena.key.anthropic", "sk-walk");
    sessionStorage.setItem("scaena.key.openai", "sk-pictures");
  }, origin);

  /** The dialog, open, with Narrato's choices in it. */
  const opened = async () => {
    await page.click("#new-prompt");
    await page.waitForFunction(() => document.querySelector("#prompting-goal").options.length > 0, null, { timeout: 60000 });
  };
  /** The progress lines the dialog shows. */
  const lines = () => page.locator("#prompting-progress li").allTextContents();
  /** Once the dialog has said it failed, its last line. */
  const failed = async () => {
    await page.waitForSelector("#prompting-progress li.failed", { timeout: 60000 });
    return page.locator("#prompting-progress li.failed").last().textContent();
  };
  const link = async (l) => {
    await page.fill("#prompting-link", l);
    await page.click("#prompting-add-link");
  };
  const removeAll = async () => {
    while (await page.locator("#prompting-sources button").count()) await page.locator("#prompting-sources button").first().click();
  };

  await opened();
  const goals = await page.locator("#prompting-goal option").allTextContents();
  check(goals.join() === "Inform,Persuade,Inspire,Sell,Educate,Update Status", `the goals are Narrato's: ${goals}`);
  check((await page.$eval("#prompting-length", (s) => s.value)) === "5-10 slides", "the length is Narrato's default");
  // Shown as a person writes a range, with an en dash, as the Mac shows it (PLAN 3.18); and a link
  // asked for by an example in words, not a syntax.
  const length = await page.$eval("#prompting-length", (s) => s.selectedOptions[0]?.textContent);
  check(length === "5–10 slides", `the length reads with an en dash: ${length}`);
  const asking = await page.getAttribute("#prompting-link", "placeholder");
  check(asking?.startsWith("A web address") && !asking.includes("github:"), `a link is asked for in words: ${asking}`);
  const arcs = await page.locator("#prompting-arc option").allTextContents();
  check(arcs[0] === "The model's choice" && arcs.includes("Problem–Solution"), `the arcs are offered, the model's choice first: ${arcs.length}`);
  const who = await page.textContent("#prompting-who");
  check(who.includes("Anthropic's scripted") && who.includes("your key"), `it says who writes the deck: ${who}`);
  const paints = await page.textContent("#prompting-painter");
  check((await page.$eval("#prompting-pictures", (s) => s.value)) === "openai" && paints.includes("OpenAI"), `pictures by OpenAI, whose key is kept: ${paints}`);
  await page.fill("#prompting-topic", "Pitch a Series A round for our carbon-negative cargo platform.");
  await page.fill("#prompting-audience", "Investors");
  await page.selectOption("#prompting-goal", "Persuade");

  // What a browser cannot read says where it is read.
  await link("sec:AAPL");
  await page.click("#prompting-go");
  const sec = await failed();
  check(sec.includes("--source sec:TICKER") && sec.includes("Mac"), `SEC says the Mac and the CLI read it: ${sec}`);
  check(await page.locator("#prompting").evaluate((d) => d.open), "the dialog stays open");
  await removeAll();
  // A web source is a public https address: one on this machine is refused before it is asked.
  await link(`${origin}/press.html`);
  await page.click("#prompting-go");
  const local = await failed();
  check(local.includes("a web source is a public https address"), `a link to this machine is refused: ${local}`);
  await removeAll();
  // The press's site, a public address. A page it lets no other page read fails the fetch as
  // CORS fails it (a response fulfilled here skips that check, so the request is aborted).
  const press = "https://press.example.com";
  await page.route(`${press}/**`, (route) =>
    route.request().url().endsWith("/press.html")
      ? route.fulfill({ status: 200, body: PAGE, headers: { "content-type": "text/html; charset=utf-8", "access-control-allow-origin": "*" } })
      : route.abort("accessdenied"),
  );
  await link(`${press}/closed.html`);
  await page.click("#prompting-go");
  const closed = await failed();
  check(closed.includes("could not be read from this page"), `a site that lets no other page read it says so: ${closed}`);
  await removeAll();

  // A deck in a script the theme's fonts do not draw takes the script's font, fetched from
  // google/fonts by the worker and kept by the browser, before anything is asked of the
  // provider; a font that cannot be fetched says so, in a person's words (PLAN G.22).
  const fetched = [];
  await page.route("https://raw.githubusercontent.com/google/fonts/**", (route) => {
    const url = route.request().url();
    fetched.push(url);
    const headers = { "access-control-allow-origin": "*" };
    if (url.includes("NotoSansDevanagari")) return route.fulfill({ status: 200, body: DEVANAGARI, headers: { ...headers, "content-type": "font/ttf" } });
    return route.fulfill({ status: 404, body: "404: Not Found", headers });
  });
  requests.length = 0;
  await page.click("#prompting details > summary");
  await page.fill("#prompting-language", "Japanese");
  await page.click("#prompting-go");
  const unfetched = await failed();
  check(
    unfetched.startsWith("a deck in Japanese is set in Noto Sans JP, which could not be fetched from https://raw.githubusercontent.com/google/fonts/") && !unfetched.includes("PLAN"),
    `a script's font that cannot be fetched stops the deck first: ${unfetched}`,
  );
  check(requests.length === 0, `nothing is asked of the provider: ${requests.length}`);
  // Hindi's is fetched and checked, then the provider is asked, which refuses the key here.
  state.fail = true;
  await page.fill("#prompting-language", "Hindi");
  await page.click("#prompting-go");
  const hindi = await failed();
  const fetching = (await lines()).find((l) => l.startsWith("Fetching"));
  check(fetching === "Fetching Noto Sans Devanagari, the font a deck in Hindi is set in (0.6 MB)…", `the progress says the font is fetched, and how large: ${fetching}`);
  check(hindi.includes("invalid x-api-key") && requests.length > 0, `then the provider is asked: ${hindi}`);
  const devanagari = () => fetched.filter((u) => u.includes("NotoSansDevanagari")).length;
  check(devanagari() === 1, `the font is fetched once: ${devanagari()}`);
  // Kept by the browser: the next deck in Hindi fetches nothing.
  await page.waitForFunction(() => !document.querySelector("#prompting-go").disabled, null, { timeout: 30000 });
  await page.click("#prompting-go");
  await failed();
  check(devanagari() === 1 && !(await lines()).some((l) => l.startsWith("Fetching")), "kept, it is not fetched for the next deck");
  state.fail = false;
  await page.fill("#prompting-language", "");
  await page.click("#prompting details > summary");

  // A provider that refuses the key says so, and Generate is there to try again.
  state.fail = true;
  await page.click("#prompting-go");
  const refused = await failed();
  check(refused.includes("invalid x-api-key"), `the provider's refusal in its words: ${refused}`);
  await page.waitForFunction(() => !document.querySelector("#prompting-go").disabled, null, { timeout: 30000 });
  check(true, "Generate can be pressed again");
  state.fail = false;

  // Cancel stops it; the deck open stays open.
  state.slow = 4000;
  await page.click("#prompting-go");
  await page.waitForFunction(() => [...document.querySelectorAll("#prompting-progress li")].some((l) => l.textContent.startsWith("Choosing the arc")), null, { timeout: 60000 });
  await page.click("#prompting-cancel");
  check(!(await page.locator("#prompting").evaluate((d) => d.open)), "Cancel closes the dialog");
  await page.waitForTimeout(5000);
  check((await page.evaluate(() => window.scaena.where().name)) === "revenue", "the deck open stays open");
  state.slow = 0;

  // Grounded in a Markdown file, a CSV, and a web page, each slide naming the source its facts
  // come from (narrative FUNC-3): the notes, the page for the second.
  requests.length = 0;
  state.slides = (() => {
    const answer = JSON.parse(SLIDES);
    const text = answer.content.find((c) => c.type === "text");
    const written = JSON.parse(text.text);
    written.slides.forEach((s, i) => (s.sourceRef = i === 1 ? `${press}/press.html` : "notes.md"));
    text.text = JSON.stringify(written);
    return JSON.stringify(answer);
  })();
  await opened();
  await page.setInputFiles("#prompting-files", [
    { name: "notes.md", mimeType: "text/markdown", buffer: Buffer.from(NOTES) },
    { name: "q3.csv", mimeType: "text/csv", buffer: Buffer.from("region,revenue\nGulf,4.1\nAtlantic,2.2\n") },
  ]);
  await link(`${press}/press.html`);
  const listed = await page.locator("#prompting-sources li").allTextContents();
  check(listed.length === 3, `the sources are listed: ${listed.map((l) => l.replace(/\s*Remove$/, "")).join(", ")}`);
  check(await page.isChecked("#prompting-show-outline"), "the outline is shown first by default");
  await page.click("#prompting-go");
  // The outline before the slides (narrative FUNC-1): the arc and why, the big idea, and each beat's
  // claim, to change before the slides are written; the slides' call waits on it.
  await page.waitForSelector("#prompting-outline:not([hidden])", { timeout: 120000 });
  const arcSaid = await page.textContent("#prompting-outline-arc");
  check(arcSaid.startsWith("Arc: Problem–Solution."), `the outline names its arc and why: ${arcSaid}`);
  check((await page.inputValue("#prompting-big-idea")) === "Cargo can pay for its carbon.", "the big idea, to type over");
  const claims = await page.locator("#prompting-beats .claim").evaluateAll((all) => all.map((c) => c.value));
  check(claims.length === 6 && claims[0] === "Claim 1: what this beat's slides leave behind.", `a claim for each of the six beats: ${claims.length}`);
  const names = await page.locator("#prompting-beats .name").evaluateAll((all) => all.map((n) => n.value));
  check(names[0].startsWith("Problem:") && names[5].startsWith("Call to Action:"), `each beat named as the arc names it: ${names.join(" | ")}`);
  check(requests.filter((r) => r.asked === "slides").length === 0, "no slides are written while the outline is read");
  check(!(await page.isDisabled("#prompting-write")), "Write the slides can be pressed while the rest of the brief waits");
  const progress1 = await lines();
  check(progress1.some((l) => l.startsWith("Outline: 6 beats.")), `the progress says the outline is there to read: ${progress1.at(-1)}`);
  // Changed: the second beat's claim rewritten, the third moved first, the last removed, and a beat
  // of the person's own added.
  await page.locator("#prompting-beats .claim").nth(1).fill("No carrier can cut it alone.");
  await page.locator("#prompting-beats li").nth(2).locator("button[aria-label='Move the beat up']").click();
  await page.locator("#prompting-beats li").nth(1).locator("button[aria-label='Move the beat up']").click();
  await page.locator("#prompting-beats li").nth(5).locator("button[aria-label='Remove the beat']").click();
  await page.click("#prompting-add-beat");
  await page.locator("#prompting-beats li").last().locator(".name").fill("Thanks");
  await page.locator("#prompting-beats li").last().locator(".claim").fill("Thank you for listening.");
  await page.fill("#prompting-big-idea", "Cargo pays for its carbon.");
  await mkdir("target/web-generate", { recursive: true });
  await page.locator("#prompting").screenshot({ path: "target/web-generate/outline.png" });
  await page.click("#prompting-write");
  await page.waitForFunction(() => window.scaena.where().name !== "revenue" && window.scaena.last(), null, { timeout: 180000 });
  const outlined = requests.filter((r) => r.asked === "outline");
  check(outlined.length === 1 && outlined[0].prompt.includes("Beat 1 \"Problem: Define core challenge and stakes\" → 1 slide"), `the outline was asked once, each beat with its slides: ${outlined[0]?.prompt.slice(0, 120)}`);
  const toOutline = requests.find((r) => r.asked === "slides")?.prompt ?? "";
  const block = toOutline.slice(toOutline.indexOf("**Outline (Scaena)**"));
  check(toOutline.includes("**Slot shapes (Scaena)**") && block.length > 0, "the slides' prompt carries Narrato's text, then the outline's block");
  check(block.includes("\"Cargo pays for its carbon.\""), "with the big idea as typed");
  check(/Beat 1 "Existing Approaches[^\n]*\n\s+Beat 2 "Problem/.test(block), `the beats in the order settled: ${block.slice(0, 400)}`);
  check(block.includes(": No carrier can cut it alone.\n") && block.includes("Beat 6 \"Thanks\"") && !block.includes("Claim 6:"), "the claim rewritten, the beat added, the beat removed");
  check(await page.locator("#prompting").evaluate((d) => !d.open), "the deck made, the dialog closed");
  const made = await page.evaluate(() => window.scaena.where());
  check(made.where === undefined, `the deck made is kept nowhere until it is saved: ${JSON.stringify(made)}`);
  const status = await page.textContent("#status");
  check(/Made: 5 slides on the arc Problem–Solution, 2 pictures/.test(status), `the status says what was made: ${status}`);
  // Two pictures, the cover's and the ending's, by the best image model the key lists.
  check(painted.length === 2, `two pictures asked for: ${painted.length}`);
  check(
    painted.every((p) => p.headers.authorization === "Bearer sk-pictures" && p.body.model === "gpt-image-1"),
    "each by gpt-image-1, the OpenAI key in OpenAI's header",
  );
  check(painted.every((p) => p.body.prompt.includes("NO text in the image") && p.body.prompt.includes("Tidewater") === false), "each asked for with no word in it");
  check(painted.some((p) => p.body.prompt.includes("SUBJECT: A container ship at dawn")), "the cover's from its visual concept");
  check(!requests.some((r) => JSON.stringify(r.headers).includes("sk-pictures")), "the OpenAI key went to OpenAI alone");
  const arc = requests.find((r) => r.asked === "arc");
  check(arc?.headers["x-api-key"] === "sk-walk" && arc.headers["anthropic-dangerous-direct-browser-access"] === "true", "the key goes in Anthropic's header, with its opt-in for a browser");
  const slides = requests.find((r) => r.asked === "slides");
  check(
    slides?.prompt.includes("=== SOURCE: notes.md ===") && slides.prompt.includes("Port calls doubled in Q3"),
    "the Markdown file is in the slides' prompt",
  );
  check(slides?.prompt.includes(`=== SOURCE: ${press}/press.html ===`) && slides.prompt.includes("third route"), "the web page is in the slides' prompt");
  check(slides?.prompt.includes("=== SOURCE: q3.csv ==="), "the CSV is in the slides' prompt");
  const look = requests.find((r) => r.asked === "look");
  const drawn = look?.body.messages[0].content.filter((c) => c.type === "image") ?? [];
  check(
    drawn.length === 5 && drawn.every((d) => d.source.media_type === "image/jpeg" && d.source.data.startsWith("/9j/")),
    `each slide shown to the model as a JPEG: ${drawn.length}`,
  );
  check(look?.prompt.startsWith("Slide 1 of 5, drawn as"), "each after what is said of it");
  const source = await page.evaluate(() => window.scaena.source());
  const states = (source.match(/^state /gm) ?? []).length;
  check(states === 5, `the deck has the recorded answer's five slides: ${states}`);
  check(source.includes("data/q3.csv"), "the CSV is a data source of the deck's");
  check(source.includes("Carbon-negative cargo, ready to scale"), "the cover's title as the model tightened it");
  check(source.includes("assets/picture-01.jpg") && source.includes("assets/picture-05.jpg"), "the pictures are the cover's and the ending's");
  const last = await page.evaluate(() => window.scaena.last());
  const errors = last.findings.filter((f) => f.severity === "error").map((f) => f.code);
  check(last.valid && errors.length === 0, `it lints with no error: ${errors}`);
  // Kept nowhere yet, it plays all the same: Play keeps it in this browser first (PLAN 2.12).
  check(await page.locator("#play").isVisible(), "Play is there before a save");
  check((await page.getAttribute("#play", "title")) === "The player, on the deck kept in this browser first", "and says it keeps the deck in this browser first");

  // Its story.
  await tab(page, "story");
  await page.waitForFunction(() => document.querySelector("#story [data-score] b"), null, { timeout: 60000 });
  const scored = (await page.textContent("#story [data-score]")).trim();
  check(/^\d+ of 100, written to the Problem–Solution arc$/.test(scored), `its score, and the arc it was written to: ${scored}`);
  const named = await page.locator("#story [data-measures] dt").allTextContents();
  check(named.join() === "Pacing,Balance,Coherence,Arc,Intensity,Tone", `the six measures: ${named}`);
  const coherence = await page.locator("#story [data-measures] dd").nth(2).textContent();
  check(coherence.startsWith("—"), `coherence by words, unscored: ${coherence}`);
  const marks = await page.locator("#story [data-arc] [data-slide]").count();
  check(marks === 5, `a mark on the arc for each slide: ${marks}`);
  const said = await page.locator("#story [data-findings] li").allTextContents();
  // Each slide names what its words came from, and its row says so (narrative FUNC-3).
  const cited = await page.evaluate(() => window.scaena.story.health().slides.map((s) => (s.sources ?? []).join()));
  check(slides?.prompt.includes("`sourceRef`"), "the slides' prompt asks each slide for its source");
  check(cited[0] === "notes.md" && cited[1] === `${press}/press.html` && cited.every((c) => c), `each slide cites its source: ${cited.join(" | ")}`);
  const from = await page.evaluate(() => [...document.querySelectorAll("#story [data-findings] li")].map((li) => [li.dataset.slide, li.querySelector("[data-sources]")?.textContent]));
  check(from.length > 0 && from.every(([n, f]) => f === `From ${cited[Number(n) - 1]}`), `each row says what its slide came from: ${JSON.stringify(from)}`);
  const rows = await page.evaluate(() => [...document.querySelectorAll("#story [data-findings] li")].map((li) => li.dataset.slide));
  const looked = await page.evaluate(() => [...new Set(window.scaena.story.health().findings.map((f) => String(f.slide)))]);
  check(rows.join() === looked.join(), `a row for each slide with something to look at, in order: ${rows}`);
  check(!said.some((s) => /\d\.\d\d/.test(s)), `in a person's words, no figure: ${said.join(" | ")}`);
  // The words are to carry the plan's intensity and tone: a gap is asked of the assistant, and
  // no fix declares what the words read in the plan's place.
  const planned = await page.evaluate(() => window.scaena.story.health().findings.filter((f) => (f.kind === "intensity" || f.kind === "tone") && f.fix).length);
  check(planned === 0, `no fix rewrites the plan's intensity or tone: ${planned}`);
  // A slide moved out of its beat's order has drifted, and its fix makes it part of the beat it
  // now stands in.
  const second = await page.evaluate(() => window.scaena.last().states[1][0]);
  await page.locator(`#strip li[data-state="${second}"]`).click();
  const unmoved = await page.evaluate(() => window.scaena.source());
  await page.keyboard.press("Alt+ArrowRight");
  await page.waitForFunction((was) => window.scaena.source() !== was, unmoved, { timeout: 30000 }).catch(() => {});
  await page
    .waitForFunction(() => window.scaena.story.health()?.findings.some((f) => f.kind === "drift"), null, { timeout: 30000 })
    .catch(() => {});
  const fixable = page.locator("#story [data-findings] li:has([data-fix])").first();
  check((await fixable.count()) === 1, `a finding offers its fix: ${said.join(" | ")}`);
  const slide = Number((await fixable.locator("[data-show]").textContent()).replace("Slide ", ""));
  await fixable.locator("[data-show]").click();
  await page.waitForFunction((n) => window.scaena.shown() === n - 1, slide, { timeout: 30000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.shown())) === slide - 1, `its slide is shown: slide ${slide}`);
  const kind = await fixable.locator("[data-fix]").first().getAttribute("data-kind");
  const unfixed = await page.evaluate(() => window.scaena.source());
  await fixable.locator("[data-fix]").first().click();
  await page.waitForFunction((was) => window.scaena.source() !== was, unfixed, { timeout: 30000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.source())) !== unfixed, `slide ${slide}'s fix is made`);
  await page.waitForFunction(
    ([n, k]) => !window.scaena.story.health()?.findings.some((f) => f.slide === n && f.kind === k),
    [slide, kind],
    { timeout: 30000 },
  ).catch(() => {});
  const after = await page.evaluate(() => window.scaena.story.health()?.findings.map((f) => `${f.slide} ${f.kind}`));
  check(!after.includes(`${slide} ${kind}`), `the finding is gone: ${after}`);
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  await page.waitForFunction((was) => window.scaena.source() === was, unfixed, { timeout: 30000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.source())) === unfixed, "⌘Z takes the fix back");
  // Read again after the undo, the finding is back.
  await page.waitForFunction(
    ([n, k]) => window.scaena.story.health()?.findings.some((f) => f.slide === n && f.kind === k),
    [slide, kind],
    { timeout: 30000 },
  ).catch(() => {});
  const back = await page.evaluate(() => window.scaena.story.health()?.findings.map((f) => `${f.slide} ${f.kind}`));
  check(back.includes(`${slide} ${kind}`), `undone, the finding is back: ${back}`);
  // A row's questions are asked together.
  const asked = page.locator("#story [data-findings] li [data-ask]").first();
  const question = await page.evaluate(() => {
    const { findings } = window.scaena.story.health();
    return findings.filter((f) => f.slide === findings[0].slide).map((f) => f.ask).join(" ");
  });
  await asked.click();
  check((await page.getAttribute("#tab-assistant", "aria-selected")) === "true", "a question opens the assistant's tab");
  check((await page.inputValue("#question")) === question, `the question is in its box: ${question}`);
  // The Files tab lists what the deck was grounded in, each with the slides citing it (FUNC-3).
  await tab(page, "files");
  await page.waitForSelector("#files [data-grounded] li", { timeout: 30000 }).catch(() => {});
  const grounded = await page.locator("#files [data-grounded] li").allTextContents();
  check(
    grounded.length === 3 && grounded.some((g) => g.startsWith("notes.md") && /cited by Slide 1, Slide 3/.test(g)) && grounded.some((g) => g.includes("press.html") && /cited by Slide 2\b/.test(g)),
    `the Files tab lists what the deck was grounded in, each with its slides: ${grounded.join(" | ")}`,
  );

  // Items that each name an icon are set under the icons the worker finds in the set that ships,
  // fetched for them (PLAN G.23): each a path on its view, named by the icon.
  state.slides = ICONED;
  await opened();
  // Slides at once, the outline not asked for: a person who wants them now gets them.
  await page.uncheck("#prompting-show-outline");
  await page.click("#prompting-go");
  // Made, opened, and compiled: the new deck's first compile is its `last`.
  await page
    .waitForFunction(() => window.scaena.source().includes("alt:Lightning") && window.scaena.last(), null, { timeout: 180000 })
    .catch(() => {});
  const iconed = await page.evaluate(() => window.scaena.source());
  check(
    ["alt:Lightning", "alt:Shield", "alt:Users", "alt:Compass", "alt:Handshake"].every((a) => iconed.includes(a)) && iconed.includes("view:[0, 0, 256, 256]"),
    "items that each name an icon are set under it",
  );
  const iconedLast = await page.evaluate(() => window.scaena.last());
  const iconedErrors = (iconedLast?.findings ?? []).filter((f) => f.severity === "error").map((f) => f.code);
  check(iconedLast?.valid && iconedErrors.length === 0, `it lints with no error: ${iconedLast ? iconedErrors : "not compiled"}`);

  // A dash for an item's icon names none: that list keeps its words, under no icon, and no item
  // is set under the set's first icon.
  state.slides = DASHED;
  check(DASHED !== ICONED, "the dashed answer differs from the iconed one");
  await opened();
  await page.uncheck("#prompting-show-outline");
  await page.click("#prompting-go");
  await page
    .waitForFunction((was) => window.scaena.source() !== was && window.scaena.source().includes("alt:Compass") && window.scaena.last(), iconed, { timeout: 180000 })
    .catch(() => {});
  const dashed = await page.evaluate(() => window.scaena.source());
  check(dashed.includes("alt:Compass"), "the other list is still set under its icons");
  check(!dashed.includes("alt:Acorn") && !dashed.includes("alt:Lightning"), "the dashed list keeps its words, under no icon");
  state.slides = undefined;
} catch (e) {
  failures.push(String(e?.stack ?? e));
} finally {
  await browser.close();
  site.close();
  provider.close();
}
console.log(failures.length ? `${failures.length} failure(s):\n${failures.join("\n")}` : "New from a prompt makes a deck in the editor");
process.exit(failures.length ? 1 : 0);
