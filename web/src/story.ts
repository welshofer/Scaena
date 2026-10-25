// The deck's story (PLAN G.9, SPEC §7.7): how well it tells it, as Vzestup measures a talk, in a
// tab beside the versions. The worker reads the deck's narrative health by the generator's own
// module, loaded the first time (`scaena_ops::health`), so the engine's module carries none of it.
//
// - The score, and the arc the deck was written to; each measure with what it says.
// - The big idea the settled outline names, where it does, above the curve (lift-up 10-24b FUNC-1).
// - The arc: its curve, slide by slide, the intensity each slide declares on it, and what its
//   words read, a slide that bends the curve or has drifted from its beat marked. A press on a
//   slide's mark shows it.
// - What to look at, what to do first open (every error a slide's lint finds, then the warnings
//   whose measure scores lowest, five findings in all) and the rest folded under one "N more",
//   counted by kind (lift-up S1); a row for each slide, its headline beside it: what health finds
//   there, in a person's words; Slide N shows it; each fix, where one is safe, is one change that
//   ⌘Z takes back; and Ask puts its questions to the assistant, its tab shown and the question in
//   its box, to send or change.
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

/** How many findings stand open before the rest fold under one disclosure: every error, then the
 * warnings to see to first, up to this many in all (lift-up S1). */
export const OPEN = 5;

/** The measure a kind of finding speaks to, where one does: a warning there ranks by its score. */
const MEASURE: Partial<Record<HealthFinding["kind"], keyof Health>> = {
  tone: "tone",
  intensity: "intensity",
  arc: "fidelity",
  drift: "fidelity",
  turn: "coherence",
  claim: "coherence",
  bigIdea: "coherence",
  pacing: "pacing",
};

/** Where a warning ranks among the warnings: one no measure speaks to first, as it names one
 * slide's own trouble; then by its measure's score, lowest first; an unscored measure's last. */
function rank(f: HealthFinding, h: Health): number {
  const measure = MEASURE[f.kind];
  if (!measure) return -1;
  return (h[measure] as { score: number | null }).score ?? 101;
}

/** Health's findings as the panel leads with them (lift-up S1): what to do first open, every error
 * a slide's lint finds, then the warnings whose measure scores lowest, `OPEN` in all; and the rest,
 * the infos among them, folded, in the order the slides stand. Each finding once. */
export function triage(h: Health, open = OPEN): { open: HealthFinding[]; more: HealthFinding[] } {
  const errors = h.findings.filter((f) => f.severity === "error");
  const warnings = h.findings
    .map((f, i) => [f, i] as const)
    .filter(([f]) => f.severity === "warning")
    .sort(([a, i], [b, j]) => rank(a, h) - rank(b, h) || i - j)
    .map(([f]) => f);
  const first = [...errors, ...warnings.slice(0, Math.max(0, open - errors.length))];
  return { open: first, more: h.findings.filter((f) => !first.includes(f)) };
}

/** A kind of finding as the folded count names it. */
const KIND: Partial<Record<HealthFinding["kind"], string>> = {
  bigIdea: "big idea",
  layout: "broken slide",
  look: "change let go",
  label: "topic headline",
};

/** What the folded findings come to, by kind, the most first: "12 more: 7 tone, 5 arc". */
export function moreSaid(more: HealthFinding[]): string {
  const counts = new Map<string, number>();
  for (const f of more) counts.set(KIND[f.kind] ?? f.kind, (counts.get(KIND[f.kind] ?? f.kind) ?? 0) + 1);
  const kinds = [...counts].sort(([, a], [, b]) => b - a).map(([k, n]) => `${n} ${k}`);
  return `${more.length} more: ${kinds.join(", ")}`;
}

/** The findings a row for each slide, in the order they come: a slide's findings together, where
 * its first stands. */
function bySlide(findings: HealthFinding[]): HealthFinding[][] {
  const runs = new Map<number, HealthFinding[]>();
  for (const f of findings) {
    const run = runs.get(f.slide);
    if (run) run.push(f);
    else runs.set(f.slide, [f]);
  }
  return [...runs.values()];
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

/** The arc as the graph draws it, `width` by `height`: the arc's beats as bands under it, each
 * named where it has room; the curve; the intensity each slide declares and what its words read,
 * a mark for each slide; and the tone each declares, a thinner line from grave at the foot to
 * hopeful at the top, with what its words read (lift-up ATTR-4). */
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
  // Each run of slides written for one beat of the arc, a band half a step past its ends.
  const step = n > 1 ? (width - 2 * pad) / (n - 1) : width - 2 * pad;
  const runs: { part: number; from: number; to: number }[] = [];
  h.slides.forEach((s, i) => {
    if (s.part === undefined) return;
    const last = runs.at(-1);
    if (last && last.part === s.part && last.to === i - 1) last.to = i;
    else runs.push({ part: s.part, from: i, to: i });
  });
  const bands = runs
    .map((r) => {
      const left = Math.max(0, x(r.from) - step / 2);
      const right = Math.min(width, x(r.to) + step / 2);
      const name = h.beats[r.part] ?? "";
      // A name about 4.6 units a letter at the graph's size, shown where its band holds it.
      const named = name.length * 4.6 + 6 <= right - left;
      return (
        `<g class="beat" data-part="${r.part}"${r.part % 2 ? " data-odd" : ""}><title>${html(name)}</title>` +
        `<rect x="${left.toFixed(1)}" y="0" width="${(right - left).toFixed(1)}" height="${height}"/>` +
        (named ? `<text x="${((left + right) / 2).toFixed(1)}" y="8">${html(name)}</text>` : "") +
        `</g>`
      );
    })
    .join("");
  const curve = `<polyline class="curve" points="${line(h.slides.map((s) => s.curve))}"/>`;
  const toned = (t: number | undefined) => (t === undefined ? undefined : (t + 1) / 2);
  const tone = `<polyline class="tone" points="${line(h.slides.map((s) => toned(s.tone)))}"/>`;
  const declared = `<polyline class="declared" points="${line(h.slides.map((s) => s.declared))}"/>`;
  const marks = h.slides
    .map((s, i) => {
      const v = s.declared ?? s.curve;
      const read = s.read === undefined ? "" : `<circle class="read" cx="${x(i).toFixed(1)}" cy="${y(s.read).toFixed(1)}" r="2.5"/>`;
      const toneRead = toned(s.toneRead);
      const heard =
        toneRead === undefined ? "" : `<rect class="tone-read" x="${(x(i) - 1.75).toFixed(1)}" y="${(y(toneRead) - 1.75).toFixed(1)}" width="3.5" height="3.5"/>`;
      return (
        `<g data-slide="${i + 1}" data-state="${html(s.state)}" data-standing="${s.standing}">` +
        `<title>Slide ${i + 1}: ${html(s.title)}</title>` +
        `<rect class="hit" x="${(x(i) - (width - 2 * pad) / Math.max(n - 1, 1) / 2).toFixed(1)}" y="0" width="${((width - 2 * pad) / Math.max(n - 1, 1)).toFixed(1)}" height="${height}"/>` +
        `<circle class="mark" cx="${x(i).toFixed(1)}" cy="${y(v).toFixed(1)}" r="3.5"/>${read}${heard}</g>`
      );
    })
    .join("");
  return `<svg viewBox="0 0 ${width} ${height}" role="img" aria-label="The intensity and tone each slide declares against the curve of its arc, by the arc's beats">${bands}${curve}${tone}${declared}${marks}</svg>`;
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
    // What to do first, open; the rest folded under one "N more", open again as it was left.
    const { open, more } = triage(h);
    const unfolded = list.querySelector<HTMLDetailsElement>("[data-more] details")?.open ?? false;
    list.replaceChildren(...bySlide(open).map((run) => row(run, h)));
    if (more.length) {
      const li = document.createElement("li");
      li.dataset.more = "";
      const details = document.createElement("details");
      details.open = unfolded;
      const summary = document.createElement("summary");
      summary.textContent = moreSaid(more);
      const rest = document.createElement("ol");
      rest.append(...bySlide(more).map((run) => row(run, h)));
      details.append(summary, rest);
      li.append(details);
      list.append(li);
    }
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
    /** What it leads with and what it folds, as it last drew them. */
    triaged: () => (health ? triage(health) : undefined),
    /** What folded findings of these kinds would come to, as the panel names them. */
    said: (kinds: HealthFinding["kind"][]) => moreSaid(kinds.map((kind) => ({ kind }) as HealthFinding)),
  };
}
