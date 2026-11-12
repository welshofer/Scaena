// W1 dogfood 14: the browser's From a prompt… end to end, against a scripted Anthropic.
// An English 15-minute Inform talk for a hospital board, grounded in a PDF and a CSV; pictures
// off, the look and the notes on. Run from the repository's root:
//   node /path/to/walk.mjs OUTDIR
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { answering, launch, serve, tab } from "../../../web/serve.mjs";

const here = new URL(".", import.meta.url).pathname;
const out = process.argv[2] ?? `${here}/run`;
await mkdir(out, { recursive: true });
const log = [];
const note = (...a) => {
  const line = a.map((x) => (typeof x === "string" ? x : JSON.stringify(x))).join(" ");
  console.log(line);
  log.push(line);
};

const IDEA = "Sepsis care at Lakeside is safer than a year ago, and nights are where the next gain is.";
const CLAIMS = [
  "Sepsis kills more of our patients than any other condition we treat, so the board should track it every quarter.",
  "Bundle compliance rose from 61% to 83% in five quarters, and mortality fell with it.",
  "The rapid-response nurse pilot cut time to antibiotics to 52 minutes on two wards.",
  "Night shifts still lag days by 17 points, and two sepsis nurse posts are open.",
  "We ask the board to fund the two open posts and extend the pilot to every ward.",
];
const requests = [];
const state = { slow: 0 };

/** The beats the slides' prompt settled, as `Beat N "name" → slides a–b: claim`. */
const beatsIn = (prompt) =>
  [...prompt.matchAll(/Beat (\d+) "([^"]+)" → (?:slide (\d+)|slides (\d+)–(\d+)): (.*)/g)].map(([, i, name, one, a, b, claim]) => ({
    index: Number(i) - 1,
    name,
    count: one ? 1 : Number(b) - Number(a) + 1,
    claim,
  }));

const slidesFor = (prompt) => {
  const beats = beatsIn(prompt);
  const slides = [];
  beats.forEach((beat, k) => {
    for (let j = 0; j < beat.count; j++) {
      const s = { beatIndex: beat.index, beatName: beat.name, subtitle: "", keyBullets: [], expectedIntensity: 0.5, expectedValence: 0.2 };
      const last = k === beats.length - 1 && j === beat.count - 1;
      if (k === 0 && j === 0) Object.assign(s, { slideTitle: "Sepsis care at Lakeside General", subtitle: "A report to the board, Q1 2026", layoutSuggestion: "Title" });
      else if (last) Object.assign(s, { slideTitle: "Thank you", layoutSuggestion: "Closing", keyBullets: ["Questions?", "quality@lakeside.example.org"], expectedIntensity: 0.4 });
      else if (beat.name.startsWith("Night")) {
        // A beat written off its claim: its slides share none of its words.
        Object.assign(s, { slideTitle: "How the cafeteria menu changed", layoutSuggestion: "Bullet List", keyBullets: ["Soup: three kinds daily", "Salad bar: open till 8 pm", "Coffee: free for night staff"] });
      } else if (j === 0) Object.assign(s, { slideTitle: beat.claim.replace(/\.$/, ""), layoutSuggestion: "Statement", keyBullets: ["Source: Sepsis Quality Report, Q1 2026"], sourceRef: "report.pdf" });
      else if (beat.name.startsWith("Data")) {
        Object.assign(s, {
          slideTitle: "Bundle compliance rose every quarter",
          layoutSuggestion: "Line Chart",
          keyBullets: ["Compliance reached 83% in Q1 2026", "Night shifts are not broken out here"],
          sourceRef: "sepsis.csv",
          chartData: { categories: ["2025-Q1", "2025-Q2", "2025-Q3", "2025-Q4", "2026-Q1"], series: [{ name: "Bundle compliance", values: [61, 68, 74, 79, 83] }] },
        });
      } else if (beat.name.startsWith("Insight")) {
        Object.assign(s, { slideTitle: "Where the minutes go", layoutSuggestion: "Venn Diagram", keyBullets: ["Triage: 14 minutes", "Cultures drawn: 19 minutes", "Antibiotics given: 52 minutes"], sourceRef: "report.pdf" });
      } else {
        Object.assign(s, { slideTitle: "What the funding buys", layoutSuggestion: "Bullet List", keyBullets: ["Two sepsis nurses: $240,000 a year", "Pilot on every ward: $90,000 once", "Night audit: quarterly"], sourceRef: "report.pdf" });
      }
      slides.push(s);
    }
  });
  return { executiveSummary: IDEA, slides };
};

const lookFor = (prompt) => {
  const slides = [];
  for (const m of prompt.matchAll(/Slide (\d+) of \d+, drawn as [^\n]*\n<data[^>]*>\nTitle: ([^\n]*)/g)) {
    const [n, title] = [Number(m[1]), m[2]];
    if (title === "What the funding buys") slides.push({ slide: n, layout: "", title: "Two nurses and a wider pilot cost $330,000", subtitle: "", bullets: [], why: "State the ask." });
    if (title.startsWith("Bundle compliance rose from 61%")) slides.push({ slide: n, layout: "", title: "Compliance is up", subtitle: "", bullets: [], why: "Shorter." });
  }
  return { slides };
};

const text = (t) => ({ content: [{ type: "text", text: typeof t === "string" ? t : JSON.stringify(t) }], stop_reason: "end_turn" });
const provider = createServer(async (req, res) => {
  const cors = {
    "access-control-allow-origin": req.headers.origin ?? "*",
    "access-control-allow-headers": req.headers["access-control-request-headers"] ?? "*",
    "access-control-allow-methods": "GET, POST, OPTIONS",
  };
  const path = new URL(req.url, "http://x").pathname;
  if (req.method === "OPTIONS") return res.writeHead(204, cors).end();
  const reply = (status, body) => res.writeHead(status, { ...cors, "content-type": "application/json" }).end(JSON.stringify(body));
  if (req.method === "GET" && path === "/v1/models") return reply(200, { data: [{ id: "scripted", display_name: "Scripted" }] });
  if (req.method !== "POST" || path !== "/v1/messages") return reply(404, { error: { message: `no ${req.method} ${path}` } });
  let raw = "";
  for await (const chunk of req) raw += chunk;
  const body = JSON.parse(raw);
  const prompt = body.messages[0].content.filter((c) => c.type === "text").map((c) => c.text).join("\n");
  const system = (body.system ?? []).map((s) => s.text).join("\n");
  const asked = system.includes("tightening the words")
    ? "fit"
    : system.includes("speechwriter")
      ? "notes"
      : system.includes("art director")
        ? "look"
        : system.includes("revising one beat")
          ? "claim"
          : system.includes("planning a presentation before its slides")
            ? "outline"
            : prompt.includes("=== CATALOG ===")
              ? "arc"
              : "slides";
  requests.push({ asked, at: Date.now(), prompt, pdf: body.messages[0].content.some((c) => c.type === "document") });
  await writeFile(`${out}/req-${String(requests.length).padStart(2, "0")}-${asked}.txt`, `SYSTEM: ${system.slice(0, 400)}\n\n${prompt}`);
  if (asked === "slides" && state.slow) await new Promise((ok) => setTimeout(ok, state.slow));
  if (asked === "arc")
    return reply(200, text({ selection: "Data Storytelling", reasoning: "A board update informs: what the numbers show, then what they mean.", candidates: [{ arc: "Data Storytelling", score: { overall: 0.9 } }] }));
  if (asked === "outline") return reply(200, text({ bigIdea: IDEA, beats: CLAIMS.map((claim, i) => ({ beat: i + 1, claim })) }));
  if (asked === "claim") return reply(200, text({ claim: "Asked again." }));
  if (asked === "slides") return reply(200, text(slidesFor(prompt)));
  if (asked === "look") return reply(200, text(lookFor(prompt)));
  if (asked === "notes") {
    const notes = [...prompt.matchAll(/Slide (\d+) of \d+: about (\d+) words\./g)].map(([, n, w]) => ({
      slide: Number(n),
      notes: Array.from({ length: Number(w) }, (_, k) => ["board", "patients", "quarter", "nurses", "wards"][k % 5]).join(" ") + ".",
    }));
    return reply(200, text({ notes }));
  }
  // The fit: each text asked shorter, cut to its most.
  const texts = [...prompt.matchAll(/id: (.+)\nat most: (\d+) characters\ntext:\n<data label="text">\n([^\n]*)/g)].map(([, id, most, said]) => ({
    id,
    text: said.slice(0, Math.max(1, Number(most) - 1)).trim(),
  }));
  reply(200, text({ texts }));
}).listen(0, "127.0.0.1");
await new Promise((ok) => provider.once("listening", ok));
const origin = `http://127.0.0.1:${provider.address().port}`;

const site = await serve();
const browser = await launch();
const lines = (page) => page.locator("#prompting-progress li").evaluateAll((all) => all.map((li) => li.firstChild?.textContent ?? ""));
try {
  const page = await browser.newPage({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  page.on("pageerror", (e) => note("PAGE ERROR", e.message));
  page.on("console", (m) => m.type() === "error" && note("CONSOLE", m.text()));
  const dialogs = [];
  page.on("dialog", (d) => {
    dialogs.push(d.message());
    void d.accept();
  });
  await answering(page);
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last() && window.scaena.where().name === "revenue", null, { timeout: 180000 });
  await page.evaluate((origin) => {
    localStorage.setItem("scaena.assistant", JSON.stringify({ provider: "anthropic", models: { anthropic: "scripted" }, bases: { anthropic: origin } }));
    sessionStorage.setItem("scaena.key.anthropic", "sk-ant-dummy");
  }, origin);

  const brief = async () => {
    await page.click("#new-prompt");
    await page.waitForFunction(() => document.querySelector("#prompting-goal").options.length > 0, null, { timeout: 60000 });
    await page.fill("#prompting-topic", "Sepsis care at Lakeside General: where we stand after a year of the bundle");
    await page.fill("#prompting-audience", "The hospital board");
    const goals = await page.locator("#prompting-goal option").evaluateAll((o) => o.map((x) => x.value));
    await page.selectOption("#prompting-goal", goals.find((g) => /inform/i.test(g)) ?? goals[0]);
    await page.fill("#prompting-minutes", "15");
    await page.selectOption("#prompting-pictures", "");
    if (!(await page.isChecked("#prompting-see"))) await page.check("#prompting-see");
    if (!(await page.isChecked("#prompting-notes"))) await page.check("#prompting-notes");
    if (!(await page.isChecked("#prompting-show-outline"))) await page.check("#prompting-show-outline");
    await page.setInputFiles("#prompting-files", [`${here}/report.pdf`, `${here}/sepsis.csv`]);
    note("goals offered:", goals);
  };

  /** Settle the outline by hand: retype a claim, move a beat, add one. */
  const settle = async () => {
    await page.waitForSelector("#prompting-outline:not([hidden])", { timeout: 180000 });
    const before = await page.locator("#prompting-beats li").evaluateAll((all) =>
      all.map((li) => ({ name: li.querySelector(".name").value, claim: li.querySelector(".claim").value, slides: li.querySelector(".slides")?.value })),
    );
    note("outline asked:", before);
    note("total:", await page.textContent("#prompting-total"));
    // Retype beat 2's claim.
    await page.locator("#prompting-beats .claim").nth(1).fill("Bundle compliance rose from 61% to 83% in five quarters, while sepsis mortality fell from 18.2% to 12.6% across the same wards and shifts.");
    // Give the Data beat a second slide, for its chart.
    await page.locator("#prompting-beats li").nth(1).locator(".slides").fill("2");
    // Move beat 4 (Implication) up above beat 3.
    await page.locator("#prompting-beats li").nth(3).locator("button[data-act=up]").click();
    // Add a beat of the person's own, before the last.
    await page.click("#prompting-add-beat");
    await page.locator("#prompting-beats li").last().locator(".name").fill("Night shifts");
    await page.locator("#prompting-beats li").last().locator(".claim").fill("Nights are where the next gain is: compliance after 11 pm is 71%, against 88% by day.");
    // Before the call to action, which stays last.
    await page.locator("#prompting-beats li").last().locator("button[data-act=up]").click();
    const after = await page.locator("#prompting-beats li").evaluateAll((all) =>
      all.map((li) => ({ name: li.querySelector(".name").value, claim: li.querySelector(".claim").value, slides: li.querySelector(".slides")?.value })),
    );
    note("outline settled:", after);
    note("total:", await page.textContent("#prompting-total"));
    await page.locator("#prompting").screenshot({ path: `${out}/outline.png` });
    return after;
  };

  // 1. The first run: settled, then cancelled mid-slides.
  await brief();
  await page.click("#prompting-go");
  const settled = await settle();
  state.slow = 15000;
  await page.click("#prompting-write");
  await page.waitForFunction(() => [...document.querySelectorAll("#prompting-progress li")].some((li) => li.textContent.startsWith("Writing the slides")), null, { timeout: 60000 });
  note("progress before cancel:", await lines(page));
  await page.click("#prompting-cancel");
  await page.waitForTimeout(1000);
  note("dialogs asked:", dialogs);
  note("dialog open after cancel:", await page.locator("#prompting").evaluate((d) => d.open));
  state.slow = 0;
  const firstRun = requests.map((r) => r.asked);
  note("first run asked:", firstRun);

  // 2. Reopened: is Go on from what was made offered?
  await page.click("#new-prompt");
  await page.waitForFunction(() => document.querySelector("#prompting-goal").options.length > 0, null, { timeout: 60000 });
  const reopened = await page.evaluate(() => ({
    goOn: !document.querySelector("#prompting-go-on").hidden,
    kept: document.querySelector("#prompting-kept").textContent,
    topic: document.querySelector("#prompting-topic").value,
    sources: [...document.querySelectorAll("#prompting-sources li")].map((li) => li.textContent),
    beats: document.querySelectorAll("#prompting-beats li").length,
  }));
  note("reopened after cancel:", reopened);
  await page.locator("#prompting").screenshot({ path: `${out}/reopened.png` });

  // 3. Made again: if Go on is offered take it; else Generate, settling again.
  const from = requests.length;
  if (reopened.goOn) await page.click("#prompting-go-on");
  else {
    if (!reopened.topic) await page.click("#prompting-cancel").then(brief);
    else if (!reopened.sources.length) await page.setInputFiles("#prompting-files", [`${here}/report.pdf`, `${here}/sepsis.csv`]);
    await page.click("#prompting-go");
    await settle();
    await page.click("#prompting-write");
  }
  await page.waitForFunction(() => window.scaena.where().name !== "revenue" && window.scaena.last(), null, { timeout: 300000 });
  const second = requests.slice(from).map((r) => r.asked);
  note("second run asked:", second);
  note("progress:", await lines(page).catch(() => []));
  const film = await page.locator("#prompting-film li").count().catch(() => 0);
  note("film tiles:", film);
  await page.waitForTimeout(3000);
  note("status:", await page.textContent("#status"));
  note("where:", await page.evaluate(() => window.scaena.where()));
  note("story tab selected:", await page.getAttribute("#tab-story", "aria-selected"));
  await page.waitForTimeout(4000);
  await page.screenshot({ path: `${out}/landed.png` });
  const story = await page.evaluate(() => document.querySelector("#story")?.innerText ?? "");
  await writeFile(`${out}/story.txt`, story);
  const source = await page.evaluate(() => window.scaena.source());
  await writeFile(`${out}/deck.scn`, source);
  // The Story's first fix: pressed, the deck changes; ⌘Z takes it back.
  const slidesNow = () => page.evaluate(() => (window.scaena.source().match(/^state /gm) ?? []).length);
  const before = await slidesNow();
  const fix = page.locator("#story button", { hasText: /^Fold into slide/ }).first();
  if (await fix.count()) {
    const label = await fix.textContent();
    await fix.click();
    await page.waitForTimeout(4000);
    const folded = await slidesNow();
    note("story fix:", label, "states", before, "->", folded, "status:", await page.textContent("#status"));
    await page.keyboard.press("Control+z");
    await page.waitForTimeout(3000);
    note("undone, states:", await slidesNow());
  } else note("no Fold fix offered");
  await writeFile(`${out}/log.txt`, log.join("\n"));

  // 4. The recovery copy: reload, is the deck offered?
  // The copy is kept a few seconds after the deck lands; wait for it, then reload.
  await page.waitForTimeout(8000);
  await page.reload();
  await page.locator("#leftovers[open]").waitFor({ timeout: 30000 }).catch(() => {});
  note("leftovers:", await page.locator("#leftovers li").allTextContents().catch(() => []));
  const recovering = await page.evaluate(() => ({
    open: document.querySelector("#recovering")?.open ?? null,
    said: document.querySelector("#recovering")?.innerText ?? "",
  }));
  note("after reload, recovery:", recovering);
  await page.screenshot({ path: `${out}/recovery.png` });
} finally {
  await writeFile(`${out}/log.txt`, log.join("\n"));
  await browser.close();
  site.close?.();
  provider.close();
}
