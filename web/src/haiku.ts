// Insert › Haiku… (PLAN O.4f, ADR-0030 decisions 26–29): a haiku about a topic, the slide shown,
// or the whole talk, written by the model the assistant uses, with the user's own key. The engine
// says what to ask (`Player.haiku`), counts what comes back, and asks once more where a poem is off
// 5-7-5 (`Player.haikuTaken`); the worker asks with the assistant's adapters (`write`), as the
// writing tools do. The poem is offered to edit, each line's count beside it (`Player.verseCounts`),
// to write again, leave, or use: one patch, one step to undo, on a new Haiku slide after the slide
// shown or on this one. Which model wrote it goes in the slide's notes, never on the slide, and the
// attribution is never filled in for the person.
import type { Key } from "./commands";
import { MOD } from "./commands";
import { listen } from "./life";
import type { Edited, HaikuAsk, HaikuTaken, HaikuWriting, Checked, Poem } from "./protocol";
import type { Stage } from "./stage";
import { unasked } from "./words";
import type { Who } from "./writing";

/** Where a haiku goes on the slide shown, as the canvas finds it: over the verse text selected, or
 * in the slide's empty verse place; else where the pointer last pressed. */
export interface HaikuTarget {
  node?: string;
  waiting: boolean;
  at: [number, number];
}

/** What the dialog asks of the editor around it. */
export interface HaikuHost {
  who(): Who | string;
  shown(): { state: string; index: number } | undefined;
  format(): string | undefined;
  source(): string;
  target(): Promise<HaikuTarget | undefined>;
  apply(source: string, edited: Edited): void;
  say(text: string): void;
  keys(): void;
}

/** The keys the dialog answers, as the keys sheet lists them (PLAN 2.65). */
export const haikuKeys = (): Key[] => [
  { keys: `${MOD}Return`, label: "Use the haiku written, in its dialog", group: "Draw and insert" },
];

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
const $ = <T extends HTMLElement>(selector: string) => document.querySelector<T>(selector)!;

/** A line's count as a person says it: `5`, or `6 or 7`. */
const counted = (c: Checked["lines"][number] | undefined) =>
  !c?.count ? "?" : c.count.low === c.count.high ? String(c.count.low) : `${c.count.low} or ${c.count.high}`;

export function haikuer(stage: Stage, host: HaikuHost) {
  const dialog = $<HTMLDialogElement>("#haiku");
  const topic = $<HTMLInputElement>("#haiku-topic");
  const where = $<HTMLFieldSetElement>("#haiku-where");
  const attribution = $<HTMLInputElement>("#haiku-attribution");
  const status = $("#haiku-status");
  const poem = $("#haiku-poem");
  const note = $("#haiku-note");
  const by = $("#haiku-by");
  const write = $<HTMLButtonElement>("#haiku-write");
  const another = $<HTMLButtonElement>("#haiku-another");
  const use = $<HTMLButtonElement>("#haiku-use");
  const key = $<HTMLButtonElement>("#haiku-key");
  const lines = [0, 1, 2].map((i) => $<HTMLInputElement>(`#haiku-line-${i}`));
  const counts = [0, 1, 2].map((i) => $(`#haiku-count-${i}`));
  /** The question being answered: a later one, or the dialog closed, drops an earlier answer. */
  let turn = 0;
  /** What the poem offered answers, and who wrote it. */
  let offered: { ask: HaikuAsk; who: Who } | undefined;
  let target: HaikuTarget | undefined;
  let settled: Promise<void> = Promise.resolve();

  const about = (): HaikuAsk["about"] =>
    (dialog.querySelector<HTMLInputElement>('input[name="haiku-about"]:checked')?.value as HaikuAsk["about"]) ?? "topic";
  const placing = () =>
    where.hidden ? "slide" : (dialog.querySelector<HTMLInputElement>('input[name="haiku-to"]:checked')?.value ?? "new");

  function show(c: Checked) {
    c.lines.forEach((line, i) => {
      if (!counts[i]) return;
      counts[i].textContent = counted(line);
      // A line off its count stands out; one that cannot be counted does not.
      const off = line.count !== undefined && line.target !== undefined && (line.target < line.count.low || line.target > line.count.high);
      lines[i].classList.toggle("off", off);
      counts[i].classList.toggle("off", off);
      lines[i].setAttribute("aria-description", off ? `counts ${counted(line)}, not ${line.target}` : `counts ${counted(line)}`);
    });
  }

  function offer(p: Poem, n: string | undefined) {
    poem.hidden = false;
    lines.forEach((input, i) => (input.value = p.lines[i] ?? ""));
    show(p.checked);
    note.textContent = n ?? "";
    note.hidden = !n;
    status.textContent = "";
    another.hidden = false;
    use.hidden = false;
    use.disabled = false;
    lines[0].focus();
  }

  /** Ask the model, once or twice, and offer what it wrote. */
  async function asking(ask: HaikuAsk) {
    const mine = ++turn;
    const who = host.who();
    if (typeof who === "string") {
      status.textContent = who;
      key.hidden = false;
      key.focus();
      return;
    }
    let writing: HaikuWriting;
    try {
      writing = await stage.haiku<HaikuWriting>("haiku", ask);
    } catch (e) {
      status.textContent = `Nothing asked: ${said(e)}`;
      return;
    }
    const call = (prompt: string) =>
      stage.write({ provider: who.provider, model: who.model, key: who.key, base: who.base, instructions: writing.instructions, prompt });
    status.textContent = `Asking ${who.model}…`;
    by.textContent = `Written by ${who.model}, with your ${who.name} key: what it is asked goes to ${who.name} and nowhere else.`;
    try {
      let { text } = await call(writing.prompt);
      if (mine !== turn || !dialog.open) return;
      let taken = await stage.haiku<HaikuTaken>("haikuTaken", { ask, answer: text });
      if (taken.next === "again") {
        // Off 5-7-5: asked once more, with each line's count, the first poem handed back with the
        // second answer.
        status.textContent = `Asking ${who.model} once more, for 5, 7, and 5…`;
        const first = taken.poem;
        ({ text } = await call(taken.prompt));
        if (mine !== turn || !dialog.open) return;
        taken = await stage.haiku<HaikuTaken>("haikuTaken", { ask, answer: text, first });
      }
      if (mine !== turn || !dialog.open || taken.next !== "offer") return;
      offered = { ask, who };
      offer(taken.poem, taken.note);
    } catch (e) {
      if (mine === turn) status.textContent = unasked(who.name, e);
    }
  }

  function begin(ask: HaikuAsk) {
    settled = asking(ask);
    return settled;
  }

  /** Open the dialog for the state shown; `again`, Write Another from a verse's right click. */
  async function open(again?: string[]) {
    const now = host.shown();
    if (!now) return host.say("Nothing to write on yet: the deck must compile first");
    target = await host.target();
    // Where it goes is not asked where a verse is selected or the slide waits for one.
    where.hidden = target?.node !== undefined || target?.waiting === true;
    for (const part of [poem, note, another, use, key]) part.hidden = true;
    status.textContent = "";
    by.textContent = "";
    offered = undefined;
    attribution.value = "";
    if (!dialog.open) dialog.showModal();
    if (again?.length) {
      const slide = dialog.querySelector<HTMLInputElement>('input[name="haiku-about"][value="slide"]');
      if (slide) slide.checked = true;
      return begin({ state: now.state, about: "slide", another: again });
    }
    topic.focus();
  }

  write.onclick = () => {
    const now = host.shown();
    if (!now) return;
    const asked = about();
    if (asked === "topic" && !topic.value.trim()) {
      status.textContent = "Say what the haiku is about, or choose this slide or the whole talk.";
      return void topic.focus();
    }
    void begin({ state: now.state, about: asked, topic: asked === "topic" ? topic.value.trim() : undefined });
  };
  another.onclick = () => {
    const now = host.shown();
    if (!now || !offered) return;
    const not = lines.map((l) => l.value.trim()).filter(Boolean);
    void begin({ ...offered.ask, state: now.state, another: not });
  };
  // Each line's count follows an edit.
  for (const input of lines) {
    listen(input, "input", () => {
      const now = lines.map((l) => l.value);
      use.disabled = !now.some((l) => l.trim());
      void stage.haiku<Checked>("verseCounts", now).then(show, () => {});
    });
  }

  /** Take the poem as it stands in the dialog: one patch, one step to undo. */
  async function take() {
    const now = host.shown();
    if (!offered || !now) return;
    const { ask, who } = offered;
    offered = undefined;
    try {
      const date = new Date().toLocaleDateString("en-US", { year: "numeric", month: "long", day: "numeric" });
      const taken = await stage.haiku<HaikuTaken>("haikuTaken", {
        ask,
        lines: lines.map((l) => l.value.trim()),
        to: placing(),
        node: target?.node,
        at: target?.at,
        attribution: attribution.value.trim() || undefined,
        by: { model: who.model, provider: who.name },
        date,
      });
      if (taken.next !== "offer" || !taken.placed) return host.say("Not placed: the haiku has nowhere to go");
      const { source, edited } = await stage.make(host.source(), taken.placed.patch, now.index, host.format());
      host.apply(source, edited);
      host.say(`Placed a haiku${placing() === "new" ? " on a new slide" : ""} · ${MOD}Z undoes it`);
    } catch (e) {
      host.say(`Not placed: ${said(e)}`);
    }
  }

  let taken: (() => void) | undefined;
  listen(use, "click", () => {
    settled = new Promise<void>((done) => (taken = done));
  });
  listen(dialog, "keydown", (e) => {
    const k = e as KeyboardEvent;
    if (k.key === "Enter" && (k.metaKey || k.ctrlKey) && !use.hidden && !use.disabled) {
      k.preventDefault();
      use.click();
    }
  });
  listen(dialog, "close", () => {
    turn++;
    const done = taken;
    taken = undefined;
    if (dialog.returnValue === "use") settled = take().finally(() => done?.());
    else {
      offered = undefined;
      done?.();
    }
    dialog.returnValue = "";
  });
  key.onclick = () => {
    dialog.close();
    host.keys();
  };

  return {
    open,
    /** The last question asked, and the poem taken, settled: what a test waits for. */
    settled: () => settled,
  };
}
