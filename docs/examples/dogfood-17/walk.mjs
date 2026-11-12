// W1 dogfood 17: the narrative loop end to end in the browser, against a scripted Anthropic.
// A 12-minute Persuade talk for a city council on Riverton's street-tree canopy, grounded in a
// PDF and a CSV, made with From a prompt…; then the Story's findings worked down through its
// asks to the assistant, its Set and Edit fields, and by hand; rehearsed against the brief's
// minutes; presented with the presenter view; and exported as a PDF with notes.
// Run from the repository's root, after `just web`:
//   node docs/examples/dogfood-17/walk.mjs [OUTDIR] [STAGES]   STAGES: make,story,rehearse,present,export,bundle
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { answering, launch, serve, tab } from "../../../web/serve.mjs";

const here = new URL(".", import.meta.url).pathname;
const out = process.argv[2] ?? `${here}/run`;
const stages = new Set((process.argv[3] ?? "make,story,rehearse,present,export").split(","));
await mkdir(out, { recursive: true });
const log = [];
const note = (...a) => {
  const line = a.map((x) => (typeof x === "string" ? x : JSON.stringify(x))).join(" ");
  console.log(line);
  log.push(line);
};

const IDEA = "Riverton is losing its shade faster than it plants it, and 1,500 trees a year wins it back by 2040.";
const requests = [];

// ---- the scripted generation ---------------------------------------------------------------

const beatsIn = (prompt) =>
  [...prompt.matchAll(/Beat (\d+) "([^"]+)" → (?:slide (\d+)|slides (\d+)–(\d+)): (.*)/g)].map(([, i, name, one, a, b, claim]) => ({
    index: Number(i) - 1,
    name,
    count: one ? 1 : Number(b) - Number(a) + 1,
    claim,
  }));

/** The slides a writer would draft for each beat: the first of a beat its claim, the rest its
 * support, as a model drafts a council talk from a forestry report. */
const DRAFT = {
  Problem: [
    { slideTitle: "Riverton's street trees", subtitle: "A report to the City Council, 2026", layoutSuggestion: "Title" },
    { slideTitle: "Riverton has lost a fifth of its shade in ten years", layoutSuggestion: "Statement", keyBullets: ["Canopy fell from 31.4% of the city in 2016 to 25.8% in 2025"], sourceRef: "report.pdf" },
  ],
  Impact: [
    {
      slideTitle: "Canopy cover, 2016–2025",
      layoutSuggestion: "Line Chart",
      keyBullets: ["Canopy fell every year, to 25.8% in 2025"],
      sourceRef: "canopy.csv",
      chartData: { categories: ["2016", "2017", "2018", "2019", "2020", "2021", "2022", "2023", "2024", "2025"], series: [{ name: "Canopy", values: [31.4, 30.9, 30.2, 29.6, 29.0, 28.3, 27.7, 27.0, 26.4, 25.8] }] },
    },
    { slideTitle: "9°F hotter", layoutSuggestion: "Big Number", keyBullets: ["9°F hotter on Eastside blocks than shaded Northgate on the hottest afternoon of July 2025"], sourceRef: "report.pdf" },
  ],
  "Existing Approaches": [
    { slideTitle: "Current planting efforts", layoutSuggestion: "Bullet List", keyBullets: ["About 450 trees planted a year", "Removals: 760 a year on average", "No watering after the first summer"], sourceRef: "report.pdf" },
  ],
  Solution: [
    { slideTitle: "Plant 1,500 trees a year for five years", layoutSuggestion: "Bullet List", keyBullets: ["$2.1 million a year, three years of watering included", "Eastside and Millbrook first, the hottest blocks", "One replacement for each tree utility work removes"], sourceRef: "report.pdf" },
  ],
  Proof: [
    {
      slideTitle: "Plantings have never caught up with removals",
      layoutSuggestion: "Bar Chart",
      keyBullets: ["2025: 640 planted, 910 removed"],
      sourceRef: "canopy.csv",
      chartData: { categories: ["2021", "2022", "2023", "2024", "2025"], series: [{ name: "Planted", values: [290, 450, 520, 610, 640] }, { name: "Removed", values: [760, 810, 840, 880, 910] }] },
    },
    { slideTitle: "Trees already save Riverton $6.2 million a year", layoutSuggestion: "Bullet List", keyBullets: ["410 tonnes of air pollution removed", "180 million gallons of stormwater caught"], sourceRef: "report.pdf" },
  ],
  "Call to Action": [{ slideTitle: "Thank you", layoutSuggestion: "Closing", keyBullets: ["Questions?", "forestry@riverton.example.gov"] }],
};

const slidesFor = (prompt) => {
  const beats = beatsIn(prompt);
  const slides = [];
  for (const beat of beats) {
    const key = Object.keys(DRAFT).find((k) => beat.name.startsWith(k)) ?? "Solution";
    const drafted = DRAFT[key];
    for (let j = 0; j < beat.count; j++) {
      const d = drafted[j] ?? { slideTitle: `More on ${key.toLowerCase()}`, layoutSuggestion: "Bullet List", keyBullets: ["One more point", "And another"] };
      slides.push({ beatIndex: beat.index, beatName: beat.name, subtitle: "", keyBullets: [], expectedIntensity: 0.5, expectedValence: 0.1, ...d });
    }
  }
  return { executiveSummary: IDEA, slides };
};

/** Notes as a speechwriter drafts them: sentences about the slide, to the words asked. */
const BANK = [
  "Councillors, the numbers here come from the division's own count.",
  "Every street in Riverton was walked for this assessment.",
  "The trees we lose are the big ones, the oaks and maples that shade a whole block.",
  "A sapling planted this spring takes most of a generation to shade that block.",
  "Eastside residents feel this every July, when the pavement holds the heat into the night.",
  "Watering is what keeps a young tree alive through its first summers.",
  "This is the part of the budget that pays for itself in stormwater and cooler streets.",
];
const notesFor = (words) => {
  const said = [];
  let n = 0;
  for (let k = 0; n < words; k++) {
    const s = BANK[k % BANK.length];
    said.push(s);
    n += s.split(/\s+/).length;
  }
  return said.join(" ");
};

const text = (t) => ({ content: [{ type: "text", text: typeof t === "string" ? t : JSON.stringify(t) }], stop_reason: "end_turn", usage: { input_tokens: 100, output_tokens: 10 } });

// ---- the scripted assistant ------------------------------------------------------------------

/** What the assistant did, ask by ask. */
const asked = [];
const unfence = (s) => s?.match(/^<data label="[a-z0-9-]+">\n([\s\S]*)\n<\/data>$/)?.[1] ?? s;

/** The patch a careful model makes for `question` on `deck`, the state shown `state`. */
function patchFor(question, deck, state) {
  const i = deck.states.findIndex((s) => s.id === state);
  const s = deck.states[i];
  const ops = [];
  const beats = (deck.spine?.sections ?? []).flatMap((x) => x.beats ?? []);
  const beat = beats.find((b) => (b.states ?? []).includes(state));
  const shown = Object.keys(s?.props ?? {});
  const headline = shown.find((id) => ["headline", "title", "display"].includes(s.props[id]?.role ?? deck.nodes[id]?.role));
  const words = (id) => {
    const p = { ...(deck.nodes[id] ?? {}), ...(s.props[id] ?? {}) };
    return p.text ?? (p.runs ?? []).map((r) => r.text).join("") ?? "";
  };
  // The slides a question names, by their first states, else the state shown.
  const firsts = deck.states.filter((x) => !x.slide).map((x) => x.id);
  const named = (question.match(/slides? ((?:\d+(?:, | and |, and )?)+)/)?.[1] ?? "").match(/\d+/g)?.map((n) => firsts[Number(n) - 1]).filter(Boolean);
  const opener = question.match(/such as “([^”]+)”/)?.[1];
  if (/notes/i.test(question))
    for (const id of named?.length ? named : [state])
      ops.push({ op: "set_state", id, prop: "notes", value: (opener ? `So we ${opener}, and here is the proof it works. ` : "") + notesFor(70) });
  const quoted = question.match(/say “([^”]+)”/)?.[1] ?? question.match(/big idea is “([^”]+)”/)?.[1] ?? question.match(/believing “([^”]+)”/)?.[1];
  if (/(headline|point it makes|state its claim|as its headline|carries it|landing it)/i.test(question) && headline) {
    const was = words(headline);
    const claim = (quoted ?? beat?.claim ?? was).replace(/\.$/, "");
    if (claim && claim !== was) ops.push({ op: "replace_text", node: headline, from: 0, to: [...was].length, text: claim, state });
  }
  if (/(intensity|calmer|hotter|its plan)/i.test(question) && beat) {
    const at = beats.indexOf(beat);
    const sec = (deck.spine?.sections ?? []);
    let k = 0;
    for (const [si, section] of sec.entries())
      for (const [bi] of (section.beats ?? []).entries()) {
        if (k++ === at) {
          const m = question.match(/(?:to|at) (0\.\d+)/);
          if (m) ops.push({ op: "add", path: `/spine/sections/${si}/beats/${bi}/media`, value: { ...(beat.media ?? {}), narrato: { ...(beat.media?.narrato ?? {}), intensity: Number(m[1]) } } });
        }
      }
  }
  return ops;
}

function assistantTurn(body) {
  const msgs = body.messages;
  const lastUser = msgs.findLastIndex((m) => m.role === "user" && (typeof m.content === "string" || m.content.some((c) => c.type === "text" && c.text.includes("[In the editor"))));
  const turn = msgs.slice(lastUser + 1);
  const round = turn.filter((m) => m.role === "assistant").length;
  const q = typeof msgs[lastUser].content === "string" ? msgs[lastUser].content : msgs[lastUser].content.filter((c) => c.type === "text").map((c) => c.text).join("\n");
  const state = q.match(/state ([^ ]+) shown/)?.[1];
  const results = (msgs.at(-1)?.content ?? []).filter?.((c) => c.type === "tool_result") ?? [];
  const call = (name, input) => ({ content: [{ type: "tool_use", id: `toolu_${requests.length}`, name, input }], stop_reason: "tool_use", usage: { input_tokens: 100, output_tokens: 10 } });
  if (round === 0) return call("deck_read", {});
  if (round === 1) {
    const read = unfence(results[0]?.content?.find((p) => p.type === "text")?.text);
    let deck;
    try {
      const v = JSON.parse(read);
      deck = v.deck ?? v;
      if (!deck?.states) throw new Error("no deck");
    } catch {
      asked.push({ q, state, read: read?.slice(0, 200), ops: null });
      return text("I could not read the deck.");
    }
    const ops = patchFor(q, deck, state);
    asked.push({ q, state, ops });
    if (!ops.length) return text("I read the slide, but I am not sure what to change: tell me what you would like it to say.");
    return call("deck_patch", { ops });
  }
  const r = unfence(results[0]?.content?.find((p) => p.type === "text")?.text);
  if (asked.length) asked.at(-1).result = r?.slice(0, 400);
  return text("Done: I made that change on this slide.");
}

/** An answer streamed as Anthropic streams it. */
function sse(answer) {
  const events = [];
  const event = (type, data) => events.push(`event: ${type}\ndata: ${JSON.stringify({ type, ...data })}\n\n`);
  event("message_start", { message: { id: "msg", type: "message", role: "assistant", content: [], usage: { input_tokens: 100, output_tokens: 0 } } });
  answer.content.forEach((block, index) => {
    if (block.type === "text") {
      event("content_block_start", { index, content_block: { type: "text", text: "" } });
      event("content_block_delta", { index, delta: { type: "text_delta", text: block.text } });
    } else {
      event("content_block_start", { index, content_block: { type: "tool_use", id: block.id, name: block.name, input: {} } });
      event("content_block_delta", { index, delta: { type: "input_json_delta", partial_json: JSON.stringify(block.input) } });
    }
    event("content_block_stop", { index });
  });
  event("message_delta", { delta: { stop_reason: answer.stop_reason }, usage: { output_tokens: 10 } });
  event("message_stop", {});
  return events.join("");
}

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
  const first = body.messages[0].content;
  const prompt = typeof first === "string" ? first : first.filter((c) => c.type === "text").map((c) => c.text).join("\n");
  const system = (Array.isArray(body.system) ? body.system : [{ text: body.system ?? "" }]).map((s) => s.text).join("\n");
  const kind = body.tools?.length
    ? "assistant"
    : system.includes("tightening the words")
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
  requests.push({ kind, at: Date.now() });
  await writeFile(`${out}/req-${String(requests.length).padStart(3, "0")}-${kind}.txt`, `SYSTEM: ${system.slice(0, 600)}\n\n${JSON.stringify(body.messages).slice(-6000)}`);
  let answer;
  if (kind === "assistant") answer = assistantTurn(body);
  else if (kind === "arc") answer = text({ selection: "Problem–Solution", reasoning: "A council is persuaded by a problem it can see and an answer it can fund.", candidates: [{ arc: "Problem–Solution", score: { overall: 0.9 } }] });
  else if (kind === "outline")
    answer = text({
      bigIdea: IDEA,
      beats: [
        "Riverton has lost a fifth of its shade in ten years.",
        "Losing shade costs the hottest neighborhoods most: Eastside runs 9°F hotter.",
        "Planting 450 trees a year cannot keep up with 760 removals.",
        "Plant 1,500 trees a year for five years, Eastside and Millbrook first.",
        "Trees already return $6.2 million a year, more than the $2.1 million plan costs.",
        "Fund the plan in the 2027 budget.",
      ].map((claim, i) => ({ beat: i + 1, claim })),
    });
  else if (kind === "claim") answer = text({ claim: "Asked again." });
  else if (kind === "slides") answer = text(slidesFor(prompt));
  else if (kind === "look") answer = text({ slides: [] });
  else if (kind === "notes") {
    const notes = [...prompt.matchAll(/Slide (\d+) of \d+: about (\d+) words\./g)].map(([, n, w]) => ({ slide: Number(n), notes: notesFor(Number(w)) }));
    answer = text({ notes });
  } else {
    const texts = [...prompt.matchAll(/id: (.+)\nat most: (\d+) characters\ntext:\n<data label="text">\n([^\n]*)/g)].map(([, id, most, said]) => ({ id, text: said.slice(0, Math.max(1, Number(most) - 1)).trim() }));
    answer = text({ texts });
  }
  if (!body.stream) return reply(200, answer);
  res.writeHead(200, { ...cors, "content-type": "text/event-stream", "cache-control": "no-cache" });
  res.end(sse(answer));
}).listen(0, "127.0.0.1");
await new Promise((ok) => provider.once("listening", ok));
const origin = `http://127.0.0.1:${provider.address().port}`;

// ---- the walk ----------------------------------------------------------------------------------

const site = await serve();
const browser = await launch();
const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1, acceptDownloads: true });
const page = await context.newPage();
page.on("pageerror", (e) => note("PAGE ERROR", e.message));
page.on("console", (m) => m.type() === "error" && note("CONSOLE", m.text()));
page.on("dialog", (d) => {
  note("DIALOG", d.message());
  void d.accept();
});
const health = () => page.evaluate(() => window.scaena.story.health());
const said = (h) => (h?.findings ?? []).map((f) => `${f.severity[0]} ${f.kind} s${f.slide}: ${f.says}${f.ask ? ` | ASK: ${f.ask}` : ""}${f.fix ? ` | FIX: ${f.fix.says}` : ""}${f.field ? ` | FIELD: ${f.field}` : ""}`);
const score = (h) => ({ score: h?.score, capped: h?.capped, pacing: h?.pacing?.score, balance: h?.balance?.score, coherence: h?.coherence?.score, fidelity: h?.fidelity?.score, intensity: h?.intensity?.score, tone: h?.tone?.score });
const reread = async (was) => {
  await page.waitForFunction((b) => JSON.stringify(window.scaena.story.health()) !== b, JSON.stringify(was), { timeout: 60000 }).catch(() => note("(health did not read again)"));
  await page.waitForTimeout(1500);
  return health();
};

try {
  await answering(page);
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last() && window.scaena.where().name === "revenue", null, { timeout: 180000 });
  await page.evaluate((origin) => {
    localStorage.setItem("scaena.assistant", JSON.stringify({ provider: "anthropic", models: { anthropic: "scripted" }, bases: { anthropic: origin } }));
    sessionStorage.setItem("scaena.key.anthropic", "sk-ant-dummy");
  }, origin);

  // 1. Made with From a prompt…, a PDF and a CSV, twelve minutes.
  await page.click("#new-prompt");
  await page.waitForFunction(() => document.querySelector("#prompting-goal").options.length > 0, null, { timeout: 60000 });
  await page.fill("#prompting-topic", "Riverton's street trees: we are losing shade faster than we plant it");
  await page.fill("#prompting-audience", "Riverton City Council");
  const goals = await page.locator("#prompting-goal option").evaluateAll((o) => o.map((x) => x.value));
  await page.selectOption("#prompting-goal", goals.find((g) => /persuade/i.test(g)) ?? goals[0]);
  await page.fill("#prompting-minutes", "12");
  await page.selectOption("#prompting-pictures", "");
  for (const id of ["#prompting-see", "#prompting-notes", "#prompting-show-outline"]) if (!(await page.isChecked(id))) await page.check(id);
  await page.setInputFiles("#prompting-files", [`${here}/report.pdf`, `${here}/canopy.csv`]);
  note("length chosen:", await page.inputValue("#prompting-length").catch(() => "?"));
  await page.click("#prompting-go");
  await page.waitForSelector("#prompting-outline:not([hidden])", { timeout: 180000 });
  note(
    "outline:",
    await page.locator("#prompting-beats li").evaluateAll((all) => all.map((li) => ({ name: li.querySelector(".name").value, claim: li.querySelector(".claim").value, slides: li.querySelector(".slides")?.value }))),
  );
  note("total:", await page.textContent("#prompting-total"));
  await page.click("#prompting-write");
  await page.waitForFunction(() => window.scaena.where().name !== "revenue" && window.scaena.last(), null, { timeout: 600000 });
  note("progress:", await page.locator("#prompting-progress li").evaluateAll((all) => all.map((li) => li.firstChild?.textContent ?? "")).catch(() => []));
  note("asked:", requests.map((r) => r.kind).join(" "));
  await page.waitForTimeout(5000);
  note("status:", await page.textContent("#status"));
  await page.screenshot({ path: `${out}/01-landed.png` });
  await writeFile(`${out}/01-deck.scn`, await page.evaluate(() => window.scaena.source()));
  let h = await health();
  for (let k = 0; !h && k < 30; k++) {
    await page.waitForTimeout(2000);
    h = await health();
  }
  note("health:", score(h));
  for (const l of said(h)) note("  ", l);
  await writeFile(`${out}/01-story.txt`, await page.evaluate(() => document.querySelector("#story")?.innerText ?? ""));
  await writeFile(`${out}/01-health.json`, JSON.stringify(h, null, 1));
  /** The bundle as the editor has it, downloaded as a `.scaena` into OUTDIR, named `name`. */
  const downloadBundle = async (name) => {
    const [download] = await Promise.all([page.waitForEvent("download", { timeout: 180000 }), page.click("#file-open").then(() => page.click("#download"))]);
    await writeFile(`${out}/${name}.scaena`, await readFile(await download.path()));
    note("downloaded:", `${name}.scaena`, download.suggestedFilename());
  };
  if (stages.has("bundle")) await downloadBundle("made");

  // 2. The Story's findings worked down, as a person would: a fix where one is offered, a field
  //    typed by hand where the row offers one, else the assistant asked.
  if (stages.has("story")) {
    await tab(page, "story");
    const tried = new Set();
    const journal = [];
    for (let k = 0; k < 14; k++) {
      h = await health();
      const open = (h?.findings ?? []).filter((f) => !tried.has(`${f.kind} ${f.state} ${f.says}`));
      if (!open.length) break;
      const f = open[0];
      tried.add(`${f.kind} ${f.state} ${f.says}`);
      const before = { ...score(h), findings: h.findings.length };
      const row = page.locator(`#story [data-findings] li[data-slide="${f.slide}"]`).first();
      await page.evaluate(() => document.querySelectorAll("#story [data-more] details").forEach((d) => (d.open = true)));
      let how;
      const was = await page.evaluate(() => window.scaena.source());
      try {
      if (f.fix && (await row.locator("button[data-fix]").count())) {
        how = `fix "${f.fix.says}"`;
        await row.locator("button[data-fix]").first().click();
      } else if (f.field === "headline" && (await row.locator('button[data-field="headline"]').count())) {
        const words = f.ask?.match(/say “([^”]+)”/)?.[1] ?? "We plant more than we lose";
        how = `field headline, typed "${words}"`;
        await row.locator('button[data-field="headline"]').first().click();
        await page.waitForTimeout(1500);
        await page.keyboard.type(words);
        await page.waitForTimeout(800);
        await page.keyboard.press("Escape");
      } else if (f.field === "notes" && f.kind === "turn" && (await row.locator('button[data-field="notes"]').count())) {
        const word = f.ask?.match(/such as “([^”]+)”/)?.[1] ?? "trees";
        how = `field notes, typed a bridge on "${word}"`;
        await row.locator('button[data-field="notes"]').first().click();
        await page.waitForTimeout(1500);
        await page.keyboard.press("Control+Home");
        await page.keyboard.type(`We have seen why we must ${word}; here is the proof it pays. `);
        await page.keyboard.press("Tab");
        await page.waitForTimeout(1000);
        await tab(page, "story");
      } else if (await row.locator("button[data-fold]").count()) {
        how = `fold "${await row.locator("button[data-fold]").first().textContent()}"`;
        await row.locator("button[data-fold]").first().click();
      } else {
        how = "asked the assistant";
        await row.locator("button[data-ask]").first().click();
        await page.waitForTimeout(800);
        const q = await page.inputValue("#question");
        await page.click("#ask");
        await page.waitForFunction(() => document.querySelector("#thinking")?.hidden !== false && !document.querySelector("#stop:not([hidden])"), null, { timeout: 120000 }).catch(() => {});
        await page.waitForTimeout(1500);
        const transcript = await page.locator("#transcript").innerText().catch(() => "");
        how += ` "${q.slice(0, 160)}" -> ${transcript.split("\n").slice(-4).join(" / ").slice(0, 300)}`;
        await tab(page, "story");
      }
      } catch (e) {
        how = `${how} FAILED: ${e.message.split("\n")[0]}`;
        await tab(page, "story").catch(() => {});
      }
      await page.waitForFunction((s) => window.scaena.source() !== s, was, { timeout: 20000 }).catch(() => {});
      await page.waitForTimeout(2000);
      const offered = await page.evaluate(() => { const o = document.querySelector("#offer"); return o && !o.hidden ? o.textContent : ""; });
      let restated = "";
      if (offered && (await page.locator("#restate:not([hidden])").count())) {
        await page.click("#restate");
        await page.waitForTimeout(1500);
        restated = await page.textContent("#status");
      }
      const changed = (await page.evaluate(() => window.scaena.source())) !== was;
      const status = await page.textContent("#status");
      const after = await reread(h);
      const gone = !(after?.findings ?? []).some((g) => g.kind === f.kind && g.state === f.state);
      const entry = { finding: `${f.kind} s${f.slide}: ${f.says}`, how, offered, restated, changed, gone, status, before, after: { ...score(after), findings: after?.findings.length }, answered: await page.locator("#story [data-answered]").textContent().catch(() => "") };
      journal.push(entry);
      note("WORKED", entry);
      for (const l of said(after)) note("   now", l);
    }
    await writeFile(`${out}/02-journal.json`, JSON.stringify(journal, null, 1));
    await writeFile(`${out}/02-deck.scn`, await page.evaluate(() => window.scaena.source()));
    await writeFile(`${out}/02-story.txt`, await page.evaluate(() => document.querySelector("#story")?.innerText ?? ""));
    await page.screenshot({ path: `${out}/02-story.png` });
    if (stages.has("bundle")) await downloadBundle("worked");
  }
  // 3. Rehearsed from the first slide, a few seconds a state, against the brief's 12 minutes.
  if (stages.has("rehearse")) {
    await page.locator("#strip li[data-state]").first().click();
    await page.waitForTimeout(1000);
    await page.click("#rehearse-open");
    await page.waitForFunction(() => !document.querySelector("#rehearsing").hidden, null, { timeout: 10000 }).catch(() => note("rehearsal bar did not show"));
    const states = (await page.evaluate(() => window.scaena.last().states.length)) ?? 0;
    for (let k = 0; k < states; k++) {
      await page.waitForTimeout(4000);
      if (k === 1) note("rehearsal bar:", await page.locator("#rehearsing").innerText().catch(() => ""), "| notes:", await page.locator("#rehearsal-notes").innerText().catch(() => ""));
      await page.keyboard.press("ArrowRight");
    }
    await page.waitForFunction(() => document.querySelector("#rehearsed")?.open, null, { timeout: 10000 }).catch(async () => {
      await page.keyboard.press("Escape");
      await page.waitForFunction(() => document.querySelector("#rehearsed")?.open, null, { timeout: 10000 }).catch(() => note("no rehearsal results"));
    });
    note("rehearsal results:", await page.locator("#rehearsed").innerText().catch(() => ""));
    await page.locator("#rehearsed").screenshot({ path: `${out}/03-rehearsed.png` }).catch(() => {});
    await page.locator("#rehearsed button[value=cancel]").click().catch(() => {});
    await page.waitForTimeout(500);
  }

  // 4. Presented: Play, then the presenter view from the player.
  if (stages.has("present")) {
    await page.locator("#strip li[data-state]").first().click();
    const [player] = await Promise.all([context.waitForEvent("page", { timeout: 60000 }), page.click("#play")]);
    player.on("pageerror", (e) => note("PLAYER ERROR", e.message));
    await player.waitForFunction(() => window.scaena?.at, null, { timeout: 120000 }).catch(() => note("player did not start"));
    await player.waitForTimeout(3000);
    note("editor status after Play:", await page.textContent("#status"));
    note("player at:", await player.evaluate(() => window.scaena.at?.()).catch(() => null), await player.textContent("#status").catch(() => ""));
    const [presenter] = await Promise.all([context.waitForEvent("page", { timeout: 60000 }), player.click("#present")]);
    presenter.on("pageerror", (e) => note("PRESENTER ERROR", e.message));
    await presenter.waitForSelector("#on", { timeout: 120000 });
    await presenter.waitForTimeout(2500);
    const view = async () =>
      presenter.evaluate(() => Object.fromEntries(["where", "story", "pace", "clock", "nextName", "notes"].map((id) => [id, document.querySelector(`#${id}`)?.textContent?.trim().slice(0, 300) ?? null])));
    note("presenter, slide 1:", await view());
    await presenter.screenshot({ path: `${out}/04-presenter-1.png` });
    for (let k = 0; k < 4; k++) {
      await presenter.click("#on");
      await presenter.waitForTimeout(2500);
    }
    note("presenter, four on:", await view());
    await presenter.screenshot({ path: `${out}/04-presenter-5.png` });
    await player.screenshot({ path: `${out}/04-player.png` });
    await presenter.close();
    await player.close();
  }

  // 5. Exported: the PDF with notes.
  if (stages.has("export")) {
    await page.bringToFront();
    await page.click("#export");
    await page.waitForFunction(() => document.querySelector("#exporting").open, null, { timeout: 10000 });
    await page.check("#exporting input[name=as][value=pdf]");
    await page.check("#exporting-with-notes").catch((e) => note("no notes switch", e.message));
    note("export note:", await page.textContent("#exporting-note").catch(() => ""));
    const [download] = await Promise.all([page.waitForEvent("download", { timeout: 300000 }), page.click("#exporting-go")]);
    const pdf = `${out}/${download.suggestedFilename()}`;
    await writeFile(pdf, await readFile(await download.path()));
    const words = execFileSync("pdftotext", ["-layout", pdf, "-"]).toString();
    await writeFile(`${out}/05-pdf.txt`, words);
    note("pdf:", download.suggestedFilename(), "pages:", (words.match(/\f/g) ?? []).length);
  }
} finally {
  await writeFile(`${out}/log.txt`, log.join("\n"));
  await writeFile(`${out}/asked.json`, JSON.stringify(asked, null, 1));
  await browser.close();
  site.close?.();
  provider.close();
}
