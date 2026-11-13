// Editor dogfood 8: From a prompt… grounded in a PDF and a CSV, in the browser, against a scripted
// Anthropic, as web/generate.mjs scripts its provider: the path #1266's PDF reader and #1283's kept
// figures run in the generator module. A 10-minute Persuade talk for a water board on replacing
// Clearwater's lead service lines, made with an outline; then the Story worked down by its own
// buttons and fields (its fixes, the sourcing finding's "Cite …", Cite a source…, the claim's
// field) and by the assistant; a save, a slide typed over, and Restore This Slide; a rehearsal
// against the brief's slot; and Export… a PDF with notes.
// Run from the repository's root, after `just web` (report.pdf is written by report.py):
//   node docs/examples/editor-dogfood-8/walk.mjs [OUTDIR] [STAGES]   STAGES: make,story,restore,rehearse,export
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { answering, launch, serve, tab } from "../../../web/serve.mjs";

const here = new URL(".", import.meta.url).pathname;
const out = process.argv[2] ?? `${here}/run`;
const stages = new Set((process.argv[3] ?? "make,story,restore,rehearse,export").split(","));
await mkdir(out, { recursive: true });
const log = [];
const note = (...a) => {
  const line = a.map((x) => (typeof x === "string" ? x : JSON.stringify(x))).join(" ");
  console.log(line);
  log.push(line);
};

const IDEA = "Clearwater can replace every lead line by 2032, Eastgate first, and the grants pay for most of it.";
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
 * support, as a model drafts a board talk from a utility report. */
const DRAFT = {
  Problem: [
    { slideTitle: "Clearwater's lead pipes", subtitle: "A report to the Board of Water Commissioners, 2026", layoutSuggestion: "Title" },
    // A figure no source holds, for Cite a source….
    { slideTitle: "Lead in water harms children first", layoutSuggestion: "Statement", keyBullets: ["Children in homes with lead lines show 40% higher blood lead"], sourceRef: "report.pdf" },
  ],
  Impact: [
    { slideTitle: "31%", layoutSuggestion: "Big Number", keyBullets: ["31% of the lead lines that remain are in Eastgate, the city's oldest homes"], sourceRef: "report.pdf" },
  ],
  "Existing Approaches": [
    // Figures only lines.csv holds, cited to report.pdf: #1283's sourcing finding.
    { slideTitle: "At today's pace the work runs to 2034", layoutSuggestion: "Bullet List", keyBullets: ["2,100 lines replaced in 2025", "18,400 still to replace", "Finished in 2034, past the plan"], sourceRef: "report.pdf" },
  ],
  Solution: [
    { slideTitle: "Replace 3,700 lines a year and finish by 2032", layoutSuggestion: "Bullet List", keyBullets: ["Eastgate first, the oldest homes", "$9,800 a line, street side and home side", "Crews doubled for six years"], sourceRef: "report.pdf" },
  ],
  Proof: [
    {
      slideTitle: "Crews replace more lines every year",
      layoutSuggestion: "Line Chart",
      keyBullets: ["2,100 lines replaced in 2025, the most ever"],
      sourceRef: "lines.csv",
      chartData: { categories: ["2021", "2022", "2023", "2024", "2025"], series: [{ name: "Replaced", values: [620, 980, 1350, 2070, 2100] }] },
    },
    { slideTitle: "The grants pay for most of it", layoutSuggestion: "Bullet List", keyBullets: ["$42 million in federal and state grants through 2030", "The federal rule requires every line replaced by 2037"], sourceRef: "report.pdf" },
  ],
  "Call to Action": [{ slideTitle: "Fund the six-year plan this year", layoutSuggestion: "Closing", keyBullets: ["Questions?", "water@clearwater.example.gov"] }],
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
  "Commissioners, these numbers come from the department's own inventory.",
  "Every connection in the city was walked to count them.",
  "A lead line is the pipe from the main under the street to the meter at the house.",
  "Eastgate's homes are the oldest, and so are its pipes.",
  "Crews have grown each year, and the pace still falls short of the plan.",
  "The grants are there now, and they will not wait for us past 2030.",
  "Finishing in 2032 means no child in Clearwater drinks through lead after that.",
]
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
  else if (kind === "arc") answer = text({ selection: "Problem–Solution", reasoning: "A board is persuaded by a problem it can see and an answer it can fund.", candidates: [{ arc: "Problem–Solution", score: { overall: 0.9 } }] });
  else if (kind === "outline")
    answer = text({
      bigIdea: IDEA,
      beats: [
        "18,400 Clearwater homes still drink through a lead line.",
        "The lead is worst in Eastgate, where the oldest homes are.",
        "At today's pace the work runs to 2034, past the plan.",
        "Replace 3,700 lines a year, Eastgate first, and finish by 2032.",
        "The grants already set aside pay for most of it.",
        "Fund the six-year plan in this year's budget.",
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

  // 1. Made with From a prompt…, a PDF and a CSV, ten minutes, the outline shown first.
  await page.click("#new-prompt");
  await page.waitForFunction(() => document.querySelector("#prompting-goal").options.length > 0, null, { timeout: 60000 });
  await page.fill("#prompting-topic", "Clearwater's lead pipes: replace every lead service line by 2032");
  await page.fill("#prompting-audience", "Clearwater Board of Water Commissioners");
  const goals = await page.locator("#prompting-goal option").evaluateAll((o) => o.map((x) => x.value));
  await page.selectOption("#prompting-goal", goals.find((g) => /persuade/i.test(g)) ?? goals[0]);
  await page.fill("#prompting-minutes", "10");
  await page.selectOption("#prompting-pictures", "");
  for (const id of ["#prompting-see", "#prompting-notes", "#prompting-show-outline"]) if (!(await page.isChecked(id))) await page.check(id);
  await page.setInputFiles("#prompting-files", [`${here}/report.pdf`, `${here}/lines.csv`]);
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
        const words = f.ask?.match(/say “([^”]+)”/)?.[1] ?? "Eastgate holds a third of the lead";
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
      } else if (f.kind === "source" && (await row.locator("button[data-cite]").count())) {
        await row.locator("button[data-cite]").first().click();
        const pick = row.locator("[data-citing] button[data-source]").first();
        how = `Cite a source…, picked ${await pick.textContent()}`;
        await pick.click();
      } else if ((f.field === "claim" || f.field === "bigIdea") && (await row.locator("button[data-settle]").count())) {
        const headline = (await page.evaluate((k) => window.scaena.story.health().slides[k - 1]?.title, f.slide)) ?? "";
        how = `${f.field} field, settled as the slide's headline "${headline}"`;
        await row.locator("button[data-settle]").first().click();
        await row.locator("form[data-settling] input").first().fill(headline);
        await row.locator("form[data-settling] input").first().press("Enter");
      } else if (await row.locator("button[data-fold]").count()) {
        how = `fold "${await row.locator("button[data-fold]").first().textContent()}"`;
        await row.locator("button[data-fold]").first().click();
      } else {
        how = "asked the assistant";
        // A row has an Ask for each of its findings (#1251): the one whose question is this finding's.
        const asks = row.locator("button[data-ask]");
        let q = "";
        for (let j = 0; j < (await asks.count()); j++) {
          await page.fill("#question", "").catch(() => {});
          await tab(page, "story");
          await asks.nth(j).click();
          await page.waitForTimeout(800);
          q = await page.inputValue("#question");
          if (f.ask && q.includes(f.ask.slice(0, 40))) break;
        }
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
  // 3. A save, a slide's headline typed over, then Restore This Slide from the first version, and
  //    ⌘Z, as a person takes back an edit to one slide.
  if (stages.has("restore")) {
    // A made deck keeps no history in the browser: it is kept nowhere until a save, and keeps no
    // history until Keep a history begins one with a save.
    await tab(page, "versions");
    note("versions of a deck kept nowhere:", await page.textContent("#versions [data-summary]"), "| keep offered:", await page.locator("#versions [data-keep]").isVisible());
    note("first save:", await page.evaluate(() => window.scaena.save()).then((r) => r?.where ?? r), await page.textContent("#status"));
    await page.waitForTimeout(1500);
    note("versions before:", await page.textContent("#versions [data-summary]"));
    if (await page.locator("#versions [data-keep]").isVisible()) {
      await page.click("#versions [data-keep]");
      await page.waitForFunction(() => window.scaena.versions.listed()?.length > 0, null, { timeout: 60000 }).catch(() => note("no history begun"));
    }
    note("kept a history:", await page.textContent("#status"));
    const before = await page.evaluate(() => window.scaena.source());
    // The headline's own words, on the line that places it, not the beat's claim, which says the same.
    const typed = before.replace(/"Replace 3,700 lines a year and finish by 2032"( at:in\(header\))/, '"Replace the lines faster"$1');
    await page.evaluate((t) => window.scaena.type(t), typed);
    await page.waitForFunction((t) => window.scaena.source() === t && window.scaena.last()?.valid, typed, { timeout: 30000 }).catch(() => note("the typing did not take"));
    await page.evaluate(() => window.scaena.save());
    await tab(page, "versions");
    await page.waitForFunction(() => window.scaena.versions.listed()?.length > 1, null, { timeout: 30000 }).catch(() => {});
    const versions = await page.evaluate(() => window.scaena.versions.listed());
    note("versions:", versions?.map((v) => `${v.n} ${v.said} · ${v.by}`));
    await page.click(`#versions [data-id="${versions[0].id}"]`);
    await page.evaluate(() => window.scaena.versions.settled());
    note("version 1 against now:", await page.evaluate(() => window.scaena.versions.changes()));
    const options = await page.locator("#versions [data-state] option").evaluateAll((o) => o.map((x) => [x.value, x.textContent]));
    const solution = await page.evaluate(() => window.scaena.last().slots.find((s) => s.headline?.startsWith("Replace"))?.state);
    if (solution) await page.selectOption("#versions [data-state]", solution);
    await page.evaluate(() => window.scaena.versions.settled());
    note("Restore This Slide offers:", await page.locator("#versions [data-restore-slide]").getAttribute("aria-description"), "of", options.length, "states");
    await page.click("#versions [data-restore-slide]");
    await page.evaluate(() => window.scaena.versions.settled());
    await page.waitForTimeout(2000);
    note("restored:", await page.textContent("#status"), "| headline back:", /"Replace 3,700 lines a year and finish by 2032" at:in\(header\)/.test(await page.evaluate(() => window.scaena.source())));
    await page.locator(".cm-content").focus().catch(() => {});
    await page.keyboard.press("Control+z");
    await page.waitForTimeout(2000);
    note("⌘Z:", await page.textContent("#status"), "| as typed:", (await page.evaluate(() => window.scaena.source())) === typed);
    await page.screenshot({ path: `${out}/03-versions.png` });
  }

  // 4. Rehearsed from the first slide, a few seconds a state, against the brief's 10 minutes.
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
    await page.locator("#rehearsed").screenshot({ path: `${out}/04-rehearsed.png` }).catch(() => {});
    await page.locator("#rehearsed button[value=cancel]").click().catch(() => {});
    await page.waitForTimeout(500);
  }

  // Presented: Play, then the presenter view from the player.
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

  // 5. Exported: Export…, a PDF with the presenter's notes.
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
