// The deck's story (PLAN G.9, SPEC §7.7): how well it tells it, as Vzestup measures a talk, in a
// tab beside the versions. The worker reads the deck's narrative health by the generator's own
// module, loaded the first time (`scaena_ops::health`), so the engine's module carries none of it.
//
// - The score, and the arc the deck was written to; each measure with what it says.
// - The big idea the settled outline names, where it does, above the curve (lift-up 10-24b FUNC-1).
// - The arc: its curve, slide by slide, the intensity each slide declares on it, and what its
//   words read, a slide that bends the curve or has drifted from its beat marked. A press on a
//   slide's mark shows it.
// - What to look at, a row for each slide in the order they stand, its headline beside it: what
//   health finds there, in a person's words; Slide N shows it; each fix, where one is safe, is
//   one change that ⌘Z takes back; and Ask puts its questions to the assistant, its tab shown and
//   the question in its box, to send or change.
// - It is read again whenever the deck changes while the tab is shown, the last answer winning.
import { observing } from "./life";
import type { Health, HealthFinding } from "./protocol";
import type { Stage } from "./stage";

/** What the panel asks of the editor around it. */
export interface StoryEditor {
  source(): string;
  /** Show `state`. */
  show(state: string): Promise<void>;
  /** Make `ops` by the user, one change to undo; `done` is what to say once it is made. */
  make(ops: unknown[], done: string): Promise<void>;
  /** Put `question` to the assistant: its tab shown, the question in its box. */
  ask(question: string): void;
  /** Check `state`'s words against its beat's claim by the writing tools' model, which offers
   * the headline rewritten to carry it (narrative FUNC-4). */
  check(state: string): void;
  say(text: string): void;
}

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
const html = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

/** Each measure, as the panel names it. */
const MEASURES: [keyof Health, string][] = [
  ["pacing", "Pacing"],
  ["balance", "Balance"],
  ["coherence", "Coherence"],
  ["fidelity", "Arc"],
  ["intensity", "Intensity"],
  ["tone", "Tone"],
];

/** How a score reads: good from 80, fair from 60. */
const grade = (score: number) => (score >= 80 ? "good" : score >= 60 ? "fair" : "poor");

/** The findings, a run for each slide, in the order the slides stand (as health gives them). */
function bySlide(findings: HealthFinding[]): HealthFinding[][] {
  const runs: HealthFinding[][] = [];
  for (const f of findings) {
    const run = runs.at(-1);
    if (run && run[0].slide === f.slide) run.push(f);
    else runs.push([f]);
  }
  return runs;
}

/** A finding as its slide's row says it: "Slide 4 runs hotter…" as "Runs hotter…", since the row
 * names its slide. */
function sentence(f: HealthFinding): string {
  const rest = f.says.replace(new RegExp(`^Slide ${f.slide} `), "");
  return rest === f.says ? rest : rest.charAt(0).toUpperCase() + rest.slice(1);
}

const SEVERITY = ["info", "warning", "error"] as const;

/** How long the panel waits after an edit before it reads the deck again, in ms: the Mac's
 * Story panel's pause (lift-up PERF-1). */
const PAUSE = 700;

/** The arc as the graph draws it, `width` by `height`: the curve, what each slide declares, and
 * what its words read, a mark for each slide. */
export function arcGraph(h: Health, width = 300, height = 120): string {
  const n = h.slides.length;
  if (!n) return "";
  const pad = 10;
  const x = (i: number) => (n === 1 ? width / 2 : pad + (i * (width - 2 * pad)) / (n - 1));
  const y = (v: number) => height - pad - v * (height - 2 * pad);
  const line = (vs: (number | undefined)[]) =>
    vs
      .map((v, i) => (v === undefined ? undefined : `${x(i).toFixed(1)},${y(v).toFixed(1)}`))
      .filter((p): p is string => p !== undefined)
      .join(" ");
  const curve = `<polyline class="curve" points="${line(h.slides.map((s) => s.curve))}"/>`;
  const declared = `<polyline class="declared" points="${line(h.slides.map((s) => s.declared))}"/>`;
  const marks = h.slides
    .map((s, i) => {
      const v = s.declared ?? s.curve;
      const read = s.read === undefined ? "" : `<circle class="read" cx="${x(i).toFixed(1)}" cy="${y(s.read).toFixed(1)}" r="2.5"/>`;
      return (
        `<g data-slide="${i + 1}" data-state="${html(s.state)}" data-standing="${s.standing}">` +
        `<title>Slide ${i + 1}: ${html(s.title)}</title>` +
        `<rect class="hit" x="${(x(i) - (width - 2 * pad) / Math.max(n - 1, 1) / 2).toFixed(1)}" y="0" width="${((width - 2 * pad) / Math.max(n - 1, 1)).toFixed(1)}" height="${height}"/>` +
        `<circle class="mark" cx="${x(i).toFixed(1)}" cy="${y(v).toFixed(1)}" r="3.5"/>${read}</g>`
      );
    })
    .join("");
  return `<svg viewBox="0 0 ${width} ${height}" role="img" aria-label="The intensity each slide declares against the curve of its arc">${curve}${declared}${marks}</svg>`;
}

/** The panel in `into`, over `stage`. */
export function storyPanel(stage: Stage, into: HTMLElement, editor: StoryEditor) {
  const score = into.querySelector<HTMLElement>("[data-score]")!;
  const measures = into.querySelector<HTMLElement>("[data-measures]")!;
  const arc = into.querySelector<HTMLElement>("[data-arc]")!;
  const idea = into.querySelector<HTMLElement>("[data-big-idea]")!;
  const list = into.querySelector<HTMLOListElement>("[data-findings]")!;
  /** What the worker last read. */
  let health: Health | undefined;
  /** Readings asked for: the last answer wins. */
  let asking = 0;

  /** Whether the panel has read the deck once, its health or why not. */
  let read = false;
  /** How many times it has asked the worker. */
  let reads = 0;
  let pausing: ReturnType<typeof setTimeout> | undefined;
  /** Read the deck's health again: at once as the panel shows or before it has read anything,
   * else once edits stop for `PAUSE`, so a burst of typing asks the worker once, as the Mac's
   * Story panel waits (lift-up PERF-1). An edit made while it is read drops that answer. */
  function refresh(now = false) {
    clearTimeout(pausing);
    if (into.hidden) return;
    const asked = ++asking;
    if (now || !read) void reading(asked);
    else pausing = setTimeout(() => void reading(asked), PAUSE);
  }

  /** Read the deck's health, and show it, while the panel is shown and nothing newer is asked. */
  async function reading(asked: number) {
    if (asked !== asking || into.hidden) return;
    read = true;
    reads++;
    try {
      const got = await stage.health(editor.source());
      if (asked !== asking) return;
      health = got;
      draw();
    } catch (e) {
      if (asked !== asking) return;
      health = undefined;
      score.textContent = "";
      measures.replaceChildren();
      arc.replaceChildren();
      idea.hidden = true;
      list.replaceChildren();
      score.textContent = `Its story is not read: ${said(e)}`;
    }
  }

  function draw() {
    const h = health;
    if (!h) return;
    score.innerHTML =
      `<b class="${grade(h.score)}">${h.score}</b> of 100, ` +
      (h.arc ? `written to the ${html(h.arc)} arc` : "no arc named: read against a talk's curve");
    measures.innerHTML = MEASURES.map(([key, name]) => {
      const m = h[key] as { score: number | null; says: string };
      const value = m.score === null ? "—" : String(m.score);
      return `<dt>${name}</dt><dd><b class="${m.score === null ? "" : grade(m.score)}">${value}</b> ${html(m.says)}</dd>`;
    }).join("");
    // The one sentence the outline settled, above the curve it is to land on (lift-up 10-24b FUNC-1).
    idea.hidden = !h.bigIdea;
    idea.innerHTML = h.bigIdea ? `<span>The big idea</span>\u201c${html(h.bigIdea)}\u201d` : "";
    arc.innerHTML = arcGraph(h);
    for (const g of arc.querySelectorAll<SVGGElement>("[data-state]")) {
      g.addEventListener("click", () => void editor.show(g.dataset.state!));
    }
    list.replaceChildren(...bySlide(h.findings).map((run) => row(run, h)));
    if (!h.findings.length) {
      const none = document.createElement("li");
      none.dataset.none = "";
      none.textContent = "Nothing to look at: each slide tells its part.";
      list.append(none);
    }
  }

  /** A slide's row: its number, which shows it, and its headline; what health finds there; each
   * fix it offers; and its questions, asked together. */
  function row(run: HealthFinding[], h: Health): HTMLLIElement {
    const { slide, state } = run[0];
    const li = document.createElement("li");
    li.dataset.slide = String(slide);
    li.dataset.severity = SEVERITY[Math.max(...run.map((f) => SEVERITY.indexOf(f.severity)))];
    const head = document.createElement("div");
    const show = document.createElement("button");
    show.type = "button";
    show.dataset.show = "";
    show.textContent = `Slide ${slide}`;
    show.addEventListener("click", () => void editor.show(state));
    const title = document.createElement("span");
    title.className = "headline";
    title.textContent = h.slides[slide - 1]?.title ?? "";
    head.append(show, title);
    li.append(head);
    // What its words came from, of the sources the deck was grounded in (narrative FUNC-3).
    const sources = h.slides[slide - 1]?.sources ?? [];
    if (sources.length) {
      const from = document.createElement("p");
      from.dataset.sources = "";
      from.textContent = `From ${sources.join(", ")}`;
      li.append(from);
    }
    for (const f of run) {
      const p = document.createElement("p");
      p.dataset.kind = f.kind;
      p.textContent = sentence(f);
      li.append(p);
    }
    const actions = document.createElement("div");
    actions.className = "actions";
    for (const f of run) {
      if (!f.fix) continue;
      const fix = f.fix;
      const button = document.createElement("button");
      button.type = "button";
      button.dataset.fix = "";
      button.dataset.kind = f.kind;
      button.textContent = fix.says;
      button.addEventListener("click", async () => {
        try {
          await editor.make(fix.patch, `Slide ${slide}: ${fix.says.toLowerCase()}`);
        } catch (e) {
          editor.say(`Not fixed: ${said(e)}`);
        }
      });
      actions.append(button);
    }
    const ask = document.createElement("button");
    ask.type = "button";
    ask.dataset.ask = "";
    ask.textContent = "Ask the assistant";
    // The slide it is about is shown first: a question begins with what the editor shows, which
    // "this slide" means to the assistant (lift-up USE-5).
    ask.addEventListener("click", async () => {
      await editor.show(state);
      editor.ask(run.map((f) => f.ask).join(" "));
    });
    actions.append(ask);
    const check = document.createElement("button");
    check.type = "button";
    check.dataset.check = "";
    check.title = "Whether its words carry the claim its beat makes, judged by the assistant's model, which offers the headline rewritten to carry it";
    check.textContent = "Check the claim";
    check.addEventListener("click", () => editor.check(state));
    actions.append(check);
    li.append(actions);
    return li;
  }

  observing(new MutationObserver(() => refresh(true))).observe(into, { attributes: true, attributeFilter: ["hidden"] });
  return {
    /** Read the deck again once edits stop. */
    refresh: () => refresh(),
    /** What the panel last read. */
    health: () => health,
    /** How many times it has asked the worker to read the deck. */
    reads: () => reads,
  };
}
