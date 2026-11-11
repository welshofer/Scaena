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
//   counted by kind (lift-up S1); a row for each slide, its headline as the deck reads now beside
//   it, never its beat's claim, which health names it by (editor dogfood 4): what health finds
//   there, in a person's words; Slide N shows it; each fix, where one is safe, is one change that
//   ⌘Z takes back; a slide lint finds broken offers lint's fixes as its marks on the canvas do,
//   and Show it, the slide with its problem's mark open (editor dogfood 4); a figure no source
//   backs, in a deck made from sources, cites one of them from the row (W1's dogfood 5); and Ask
//   puts its questions to the assistant, its tab shown and the question in its box, to send or
//   change.
// - It is read again whenever the deck changes while the tab is shown, the last answer winning.
import { observing } from "./life";
import type { CiteChoice, Finding, Health, HealthFinding } from "./protocol";
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
  /** The words that head the slide whose first state is `slide`, as the deck reads now, where a
   * text heads it: what its row names it by, since health names a slide by its beat's claim
   * (editor dogfood 4). */
  headline(slide: string): string | undefined;
  /** What lint last found wrong on the slide whose first state is `slide`, any step of it: what
   * its marks on the canvas stand for (editor dogfood 4). */
  issues(slide: string): Finding[];
  /** What lint last found in the deck, a state's or every state's: what the worker reads health
   * beside. */
  findings(): Finding[];
  /** Take `f`'s fix, as its mark on the canvas does: one change to undo. */
  fix(f: Finding): Promise<void>;
  /** Show `f`'s state, its mark on the canvas open. */
  mark(f: Finding): Promise<void>;
  /** The sources the deck was grounded in, by name (`meta.narrative.sources`): none for a deck
   * made from none (W1's dogfood 5). */
  sources(): Promise<string[]>;
  /** What the beat that tells `state` cites, where one does. */
  cites(state: string): Promise<CiteChoice | undefined>;
  say(text: string): void;
  /** The words the editor shows for the state shown, the plan's ladders among them. */
  words(): Ladders | undefined;
}

/** The findings about what a slide claims, whose rows offer Check the claim: its claim not carried,
 * its headline a topic, the big idea not landed, or a turn its words do not make. */
const CLAIMED = new Set<HealthFinding["kind"]>(["claim", "label", "bigIdea", "turn"]);

/** How many of lint's fixes a broken slide's row offers, the worst first. */
const FIXES = 3;

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
  source: "unsourced figure",
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

/** How a plan's intensity and tone are said, each word from the least it says (the session's
 * `Lexicon`, `scaena_core::words::INTENSITIES` and `TONES`). */
export interface Ladders {
  intensities: [number, string][];
  tones: [number, string][];
}

/** `value` said by `ladder`: the last word whose least it reaches. */
const worded = (ladder: [number, string][], value: number) =>
  [...ladder].reverse().find(([least]) => value >= least)?.[1] ?? ladder[0]?.[1] ?? "";

/** A finding's question as its row says what to do, in a person's words: each figure health gives
 * the assistant, which reads the deck's (`scaena health` prints them too), said by its word, a
 * tone (signed, −1 grave to +1 hopeful) by `ladders.tones` and an intensity (0 calm to 1 intense)
 * by `ladders.intensities`, so "carry the intensity it declares (0.90)" reads "… (Peak)" and
 * "they read −0.19" reads "they read Even" (editor dogfood 3). Without the ladders, the figures
 * go. The assistant is asked the question whole. */
export function spoken(ask: string, ladders?: Ladders): string {
  if (ladders?.intensities.length && ladders.tones.length) {
    return ask.replace(/[+−-]?\d\.\d\d/g, (figure) => {
      const signed = /^[+−-]/.test(figure);
      const value = Number(figure.replace("−", "-"));
      return worded(signed ? ladders.tones : ladders.intensities, value);
    });
  }
  return ask
    .replace(/\s*\([+−-]?\d\.\d\d(?:–[+−-]?\d\.\d\d)?\)/g, "")
    .replace(/;\s*they read [+−-]?\d\.\d\d/g, "")
    .replace(/ (at|read) [+−-]?\d\.\d\d,/g, (_, w: string) => (w === "read" ? " read" : ""))
    .replace(/\s?[+−-]?\d\.\d\d/g, "");
}

/** A finding as its slide's row says it: "Slide 4 runs hotter…" as "Runs hotter…", since the row
 * names its slide. */
function sentence(f: HealthFinding): string {
  const rest = f.says.replace(new RegExp(`^Slide ${f.slide} `), "");
  return rest === f.says ? rest : rest.charAt(0).toUpperCase() + rest.slice(1);
}

const SEVERITY = ["info", "warning", "error"] as const;
/** Each severity's word, which names a finding's slide button beside the shape the page draws
 * before it, so no finding is told by its colour alone (lift-up 20261110 USE-1, WCAG 1.4.1). */
const WORD: Record<(typeof SEVERITY)[number], string> = { error: "Error", warning: "Warning", info: "Note" };

/** The slides a row's words name ("slides 1, 5 and 11", "slide 4's", "slides 10–11"), each a
 * button that shows it where health reads that slide, the words as they were (editor dogfood 6):
 * a to-do about several slides reaches each of them, as the cap line's slides do. */
export function slidesShown(text: string, h: Health): (string | HTMLButtonElement)[] {
  const parts: (string | HTMLButtonElement)[] = [];
  const named = /\b([Ss]lides?) (\d+(?:(?:, | and |, and |–)\d+)*)/g;
  let from = 0;
  for (const m of text.matchAll(named)) {
    parts.push(text.slice(from, m.index), `${m[1]} `);
    for (const piece of m[2].split(/(, and |, | and |–)/)) {
      const n = Number(piece);
      const state = /^\d+$/.test(piece) ? h.slides[n - 1]?.state : undefined;
      if (!state) {
        parts.push(piece);
        continue;
      }
      const go = document.createElement("button");
      go.type = "button";
      go.dataset.toSlide = state;
      go.textContent = piece;
      go.ariaLabel = `Show slide ${n}`;
      parts.push(go);
    }
    from = m.index + m[0].length;
  }
  parts.push(text.slice(from));
  return parts.filter((p) => p !== "");
}

/** Where a finding's row stood when last drawn, and where each stands now: a finding is told by
 * its kind and slide, or, where the deck had and has one finding of its kind, by the kind alone,
 * as pacing's, whose slide moves to the shortest beat as notes are deepened (editor dogfood 6). */
function places(findings: HealthFinding[]) {
  const kinds = new Map<string, number>();
  for (const f of findings) kinds.set(f.kind, (kinds.get(f.kind) ?? 0) + 1);
  const alone = (f: HealthFinding) => kinds.get(f.kind) === 1;
  return {
    /** Where `f` stood in `stood`, else none. */
    at: (stood: Map<string, number>, f: HealthFinding) => stood.get(`${f.kind} ${f.slide}`) ?? (alone(f) ? stood.get(f.kind) : undefined),
    /** The keys `f` is found by at the next read, standing at `i`. */
    keys: (f: HealthFinding) => (alone(f) ? [`${f.kind} ${f.slide}`, f.kind] : [`${f.kind} ${f.slide}`]),
  };
}

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

/** What holds the score down, where a slide lint finds broken caps it (W1's dogfood 5): "Capped at
 * 72 while slides 7 and 10 have an error; 84 without", each slide a button that shows it. */
function cappedSaid(h: Health): string {
  const c = h.capped;
  if (!c?.slides.length) return "";
  const slide = (n: number) => {
    const state = h.slides[n - 1]?.state;
    const said = `slide ${n}`;
    return state ? `<button type="button" data-capped-slide="${html(state)}">${said}</button>` : said;
  };
  const named = c.slides.map(slide);
  const listed = named.length < 3 ? named.join(" and ") : `${named.slice(0, -1).join(", ")} and ${named[named.length - 1]}`;
  const have = c.slides.length === 1 ? "has" : "have";
  return `<span data-capped>Capped at ${c.cap} while ${listed} ${have} an error; ${c.score} without.</span>`;
}

/** The panel in `into`, over `stage`. */
export function storyPanel(stage: Stage, into: HTMLElement, editor: StoryEditor) {
  const score = into.querySelector<HTMLElement>("[data-score]")!;
  // A slide the cap names shows it (W1's dogfood 5): the score's line is drawn anew as it changes,
  // so its buttons are heard here.
  score.addEventListener("click", (e) => {
    const state = (e.target as Element).closest<HTMLElement>("[data-capped-slide]")?.dataset.cappedSlide;
    if (state) void editor.show(state);
  });
  const measures = into.querySelector<HTMLElement>("[data-measures]")!;
  // A slide a row's words name shows it (editor dogfood 6): the rows are drawn anew at each read,
  // so their buttons are heard here.
  into.querySelector<HTMLElement>("[data-findings]")!.addEventListener("click", (e) => {
    const state = (e.target as Element).closest<HTMLElement>("[data-to-slide]")?.dataset.toSlide;
    if (state) void editor.show(state);
  });
  const arc = into.querySelector<HTMLElement>("[data-arc]")!;
  const idea = into.querySelector<HTMLElement>("[data-big-idea]")!;
  const list = into.querySelector<HTMLOListElement>("[data-findings]")!;
  /** The line that says the story shown is as last read, while the source does not compile. */
  const stale = Object.assign(document.createElement("p"), { hidden: true });
  stale.dataset.stale = "";
  stale.setAttribute("role", "status");
  score.before(stale);
  const listed = list.getAttribute("aria-label") ?? "";
  /** The score as last said, so the live region speaks only when it changes. */
  let scored = "";
  /** Show the story as it was last read, or as read now (lift-up 20261110 deferred): a source that
   * does not compile for a moment, mid-typing, keeps the last read, dimmed and said to be so, its
   * fixes held, since each was made against a deck that no longer compiles. */
  const asLastRead = (on: boolean) => {
    if (on) into.dataset.stale = "";
    else delete into.dataset.stale;
    stale.hidden = !on;
    if (on && !stale.textContent) stale.textContent = "Showing the story as last read: the source has an error to fix";
    if (!on) stale.textContent = "";
    list.setAttribute("aria-label", on ? `${listed}, as last read` : listed);
    for (const fix of list.querySelectorAll<HTMLButtonElement>("[data-fix], [data-lint-fix], [data-cite], [data-citing] [data-source]")) fix.disabled = on;
  };
  /** What the worker last read. */
  let health: Health | undefined;
  /** Where each open row stood when last drawn, by its findings' places (`places`): a read again
   * keeps a row where it stood, so the row a person is working stays under them and a finding new
   * to the Story comes after, an error before (editor dogfood 6). */
  let stood = new Map<string, number>();
  /** The sources the deck was grounded in, as last read with its health. */
  let grounded: string[] = [];
  /** Readings asked for: the last answer wins. */
  let asking = 0;

  /** The findings the last read was taken beside, by what tells them apart: a lint of every state
   * that finds otherwise reads the story again (lift-up 20261110 deferred). */
  let readWith = "";
  const keyed = (findings: Finding[]) =>
    findings.map((f) => `${f.code} ${f.severity} ${f.state ?? ""} ${f.node ?? ""} ${f.fixable ? 1 : 0}`).join("\n");

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
    readWith = keyed(editor.findings());
    try {
      const [got, sources] = await Promise.all([stage.health(editor.source()), editor.sources().catch(() => [])]);
      if (asked !== asking) return;
      health = got;
      grounded = sources;
      asLastRead(false);
      draw();
    } catch (e) {
      if (asked !== asking) return;
      if (health) return asLastRead(true);
      health = undefined;
      stood = new Map();
      score.textContent = "";
      measures.replaceChildren();
      arc.replaceChildren();
      idea.hidden = true;
      list.replaceChildren();
      score.textContent = `Its story is not read: ${said(e)}`;
      scored = "";
    }
  }

  function draw() {
    const h = health;
    if (!h) return;
    // Said again only where it changed: the live region speaks each change once.
    const scoring =
      `<b class="${grade(h.score)}">${h.score}</b> of 100, ` +
      (h.arc ? `written to the ${html(h.arc)} arc` : "no arc named: read against a talk's curve") +
      cappedSaid(h);
    if (scoring !== scored) score.innerHTML = scored = scoring;
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
    // A slide's sources to cite, open, stay open as they were left, the focus where it was: a read
    // again after every state is linted draws the rows again (lift-up 20261110 deferred).
    const choosing = list.querySelector<HTMLElement>("[data-citing]:not([hidden])");
    const citing = choosing?.closest<HTMLElement>("li[data-slide]")?.dataset.slide;
    const picked = choosing?.contains(document.activeElement) ? (document.activeElement as HTMLElement).dataset.source : undefined;
    const place = places(h.findings);
    const at = (run: HealthFinding[]) => Math.min(...run.map((f) => place.at(stood, f) ?? Infinity));
    const runs = bySlide(open)
      .map((run, i) => ({ run, i, at: at(run) }))
      .map((r) => ({ ...r, at: r.at === Infinity && r.run.some((f) => f.severity === "error") ? -1 : r.at }))
      .sort((a, b) => a.at - b.at || a.i - b.i)
      .map((r) => r.run);
    stood = new Map(runs.flatMap((run, i) => run.flatMap((f) => place.keys(f).map((k) => [k, i] as const))));
    list.replaceChildren(...runs.map((run) => row(run, h)));
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
    if (citing) {
      const cite = list.querySelector<HTMLButtonElement>(`li[data-slide="${citing}"] [data-cite]`);
      const offered = cite?.nextElementSibling as HTMLElement | null;
      if (cite && offered) {
        offered.hidden = false;
        cite.ariaExpanded = "true";
        if (picked) [...offered.querySelectorAll<HTMLButtonElement>("[data-source]")].find((b) => b.dataset.source === picked)?.focus();
      }
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
    const severity = SEVERITY[Math.max(...run.map((f) => SEVERITY.indexOf(f.severity)))];
    li.dataset.severity = severity;
    const head = document.createElement("div");
    const show = document.createElement("button");
    show.type = "button";
    show.dataset.show = "";
    show.textContent = `Slide ${slide}`;
    show.ariaLabel = `${WORD[severity]}: Slide ${slide}`;
    show.title = WORD[severity];
    show.addEventListener("click", () => void editor.show(state));
    const title = document.createElement("span");
    title.className = "headline";
    // The slide as it reads now: its headline, else health's title, its beat's claim, which an
    // edit of the headline leaves as it was (editor dogfood 4).
    title.textContent = editor.headline(state) ?? h.slides[slide - 1]?.title ?? "";
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
    // What it found, then what to do about it, each in a sentence: the question the assistant
    // would be asked says what to do, so a presenter with no key, or none to spend, reads it
    // here, not in an Assistant tab that asks for a key first.
    for (const f of run) {
      const p = document.createElement("p");
      p.dataset.kind = f.kind;
      p.append(...slidesShown(sentence(f), h));
      li.append(p);
      if (f.ask) {
        const todo = document.createElement("p");
        todo.dataset.todo = f.kind;
        const label = document.createElement("span");
        label.textContent = "To do: ";
        todo.append(label, ...slidesShown(spoken(f.ask, editor.words()), h));
        li.append(todo);
      }
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
    // A slide lint finds broken offers what its marks on the canvas offer: each fix lint has, and
    // the slide shown with its first problem's mark open, where the fix or the words are
    // (editor dogfood 4).
    if (run.some((f) => f.kind === "layout")) {
      const issues = editor.issues(state).filter((f) => f.severity === "error");
      const fixable = issues.filter((f) => f.fix && f.fixable);
      const offered = new Set<string>();
      for (const f of fixable) {
        const says = f.fixing ?? "Fix";
        if (offered.size >= FIXES || offered.has(says + (f.node ?? ""))) continue;
        offered.add(says + (f.node ?? ""));
        const button = document.createElement("button");
        button.type = "button";
        button.dataset.lintFix = f.code;
        button.textContent = says;
        button.title = f.said ?? f.message;
        button.addEventListener("click", async () => {
          try {
            await editor.fix(f);
            editor.say(`Slide ${slide}: ${says.toLowerCase()} · ⌘Z undoes it`);
          } catch (e) {
            editor.say(`Not fixed: ${said(e)}`);
          }
        });
        actions.append(button);
      }
      const first = issues.find((f) => f.shown) ?? issues[0];
      if (first) {
        const button = document.createElement("button");
        button.type = "button";
        button.dataset.lintShow = "";
        button.textContent = "Show it";
        button.title = "The slide shown, its problem's mark open on the canvas";
        button.addEventListener("click", () => void editor.mark(first));
        actions.append(button);
      }
    }
    // A figure no source backs, in a deck made from sources, cites one from the row (W1's dogfood
    // 5): the deck's sources offered by name, the one chosen added to the slide's beat's evidence,
    // one patch, one step to undo.
    if (grounded.length && run.some((f) => f.kind === "source")) {
      const cite = document.createElement("button");
      cite.type = "button";
      cite.dataset.cite = "";
      cite.textContent = "Cite a source…";
      cite.ariaExpanded = "false";
      const offered = document.createElement("div");
      offered.dataset.citing = "";
      offered.hidden = true;
      offered.setAttribute("role", "group");
      offered.ariaLabel = `The sources slide ${slide} may cite`;
      for (const name of grounded) {
        const pick = document.createElement("button");
        pick.type = "button";
        pick.dataset.source = name;
        pick.textContent = name;
        pick.addEventListener("click", async () => {
          try {
            const cites = await editor.cites(state);
            if (!cites) return editor.say(`Not cited: no beat of the story tells slide ${slide}`);
            const value = `source:${name}`;
            if (cites.evidence.includes(value)) return editor.say(`Slide ${slide} cites ${name} already`);
            const op = cites.evidence.length
              ? { op: "add", path: `${cites.at}/-`, value }
              : { op: "add", path: cites.at, value: [value] };
            await editor.make([op], `Slide ${slide} cites ${name} · ⌘Z undoes it`);
          } catch (e) {
            editor.say(`Not cited: ${said(e)}`);
          }
        });
        offered.append(pick);
      }
      cite.addEventListener("click", () => {
        offered.hidden = !offered.hidden;
        cite.ariaExpanded = String(!offered.hidden);
        if (!offered.hidden) offered.querySelector<HTMLButtonElement>("button")?.focus();
      });
      actions.append(cite, offered);
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
    // Offered where the row is about what the slide claims, never on one about its words' count
    // or its notes (W1's dogfood 7).
    if (run.some((f) => CLAIMED.has(f.kind))) {
      const check = document.createElement("button");
      check.type = "button";
      check.dataset.check = "";
      check.title = "Whether its words carry the claim its beat makes, judged by the assistant's model, which offers the headline rewritten to carry it";
      check.textContent = "Check the claim";
      check.addEventListener("click", () => editor.check(state));
      actions.append(check);
    }
    li.append(actions);
    return li;
  }

  observing(new MutationObserver(() => refresh(true))).observe(into, { attributes: true, attributeFilter: ["hidden"] });
  return {
    /** Read the deck again once edits stop. */
    refresh: () => refresh(),
    /** Every state is linted: read again at once, where the read shown was taken beside other
     * findings, as a read before that lint was. Typing that lints nothing new reads nothing more. */
    linted: (findings: Finding[]) => {
      if (keyed(findings) !== readWith) refresh(true);
    },
    /** What the panel last read. */
    health: () => health,
    /** How many times it has asked the worker to read the deck. */
    reads: () => reads,
    /** A finding's question as its row says what to do (editor dogfood 3). */
    spoken: (ask: string) => spoken(ask, editor.words()),
    /** What it leads with and what it folds, as it last drew them. */
    triaged: () => (health ? triage(health) : undefined),
    /** What folded findings of these kinds would come to, as the panel names them. */
    said: (kinds: HealthFinding["kind"][]) => moreSaid(kinds.map((kind) => ({ kind }) as HealthFinding)),
  };
}
