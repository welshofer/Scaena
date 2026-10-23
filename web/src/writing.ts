// Writing tools (PLAN 2.101): a text's words proofread, tightened, or rewritten, a slide's notes
// drafted, an issue explained, a picture described from itself (PLAN 2.119), and a slide's words
// checked against its beat's claim, its headline rewritten to carry it (narrative FUNC-4), by the
// model the assistant uses, with the user's own key. The
// engine says what to ask, from the deck as it stands (`Player.writing`, `scaena_core::writing`),
// in the words the Mac and the iPad ask in; the worker asks it once, with nothing to call
// (`write`); and the answer is offered in a dialog, its words to edit before they are taken. Use
// it takes them as one patch, written where the text or the notes live, one step to undo; a text
// keeps the looks of the words the answer keeps, run by run (`Player.written`). Leave it changes
// nothing. With no key, nothing is asked: the dialog says where one goes.
import { listen } from "./life";
import type { Edited, WritingAsk, WritingKind, Written } from "./protocol";
import type { Stage } from "./stage";
import { quote, unasked } from "./words";

/** Who writes: the assistant's provider, by its name, at its own address or `base`, with the
 * user's key, and the model they picked. */
export interface Who {
  provider: "anthropic" | "openai" | "gemini";
  name: string;
  model: string;
  key: string;
  base?: string;
}

/** What the writing tools ask of the editor around them. */
export interface WritingHost {
  /** The assistant's provider, model, and key; or why there is none to ask. */
  who(): Who | string;
  /** The state shown, by its id and its slot's index. */
  shown(): { state: string; index: number } | undefined;
  format(): string | undefined;
  source(): string;
  /** Take `source`, the patch made: one step to undo. */
  apply(source: string, edited: Edited, said?: string): void;
  say(text: string): void;
  /** Show the assistant, its key in focus: where a key goes. */
  keys(): void;
  /** `state` as a person counts it: `Slide 3`. */
  slide(state: string): string;
}

/** What each tool does, as the status says it once its answer is taken. */
const TOOK: Record<WritingKind, (what: string) => string> = {
  proofread: (what) => `Proofread ${what}`,
  tighten: (what) => `Tightened ${what}`,
  rewrite: (what) => `Rewrote ${what}`,
  notes: (what) => `Drafted the notes of ${what}`,
  explain: (what) => `Explained ${what}`,
  describe: (what) => `Described ${what}`,
  claim: (what) => `Rewrote the headline of ${what} to carry its claim`,
};

/** What each tool is called before the engine names it, and where nothing is asked. */
export const TOOLS: Record<WritingKind, string> = {
  proofread: "Proofread",
  tighten: "Tighten",
  rewrite: "Rewrite",
  notes: "Draft Notes",
  explain: "Explain",
  describe: "Describe",
  claim: "Check the Claim",
};

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
const $ = <T extends HTMLElement>(selector: string) => document.querySelector<T>(selector)!;

export function writer(stage: Stage, host: WritingHost) {
  const dialog = $<HTMLDialogElement>("#writing");
  const title = $("#writing-title");
  const was = $("#writing-was");
  const thinking = $("#writing-thinking");
  const answer = $<HTMLTextAreaElement>("#writing-answer");
  const read = $("#writing-said");
  const by = $("#writing-by");
  const leave = $<HTMLButtonElement>("#writing-leave");
  const use = $<HTMLButtonElement>("#writing-use");
  const key = $<HTMLButtonElement>("#writing-key");
  /** The question being answered: a later one, or the dialog closed, drops an earlier answer. */
  let turn = 0;
  /** The answer offered, and what it answers, until it is taken or left. */
  let offered: { ask: WritingAsk; written: Written } | undefined;
  /** The last question asked, settled: what a test waits for. */
  let settled: Promise<void> = Promise.resolve();

  function reset(name: string) {
    title.textContent = name;
    for (const part of [was, answer, read, key, use]) part.hidden = true;
    thinking.hidden = false;
    thinking.textContent = "";
    by.textContent = "";
    answer.value = "";
    leave.textContent = "Leave it";
    offered = undefined;
  }

  /** Ask for `ask` and offer the answer. */
  function offer(ask: WritingAsk): Promise<void> {
    settled = asking(ask);
    return settled;
  }

  async function asking(ask: WritingAsk) {
    const mine = ++turn;
    reset(TOOLS[ask.kind]);
    const who = host.who();
    if (typeof who === "string") {
      thinking.textContent = who;
      key.hidden = false;
      leave.textContent = "Close";
      if (!dialog.open) dialog.showModal();
      key.focus();
      return;
    }
    let writing;
    try {
      writing = await stage.writing(ask);
    } catch (e) {
      host.say(`Nothing to ask: ${said(e)}`);
      return;
    }
    if (mine !== turn) return;
    title.textContent = writing.title;
    // The words it works on, as they were: a text's, a picture's description, or the headline a
    // claim check may rewrite.
    const before = writing.task.text ?? writing.task.headline;
    if (before !== undefined) {
      was.textContent = before;
      was.hidden = false;
    }
    thinking.textContent = `Asking ${who.model}…`;
    by.textContent = `Written by ${who.model}, with your ${who.name} key: what it is asked goes to ${who.name} and nowhere else.`;
    if (!dialog.open) dialog.showModal();
    leave.focus();
    let text: string;
    try {
      ({ text } = await stage.write({
        provider: who.provider,
        model: who.model,
        key: who.key,
        base: who.base,
        instructions: writing.instructions,
        prompt: writing.prompt,
        picture: writing.picture,
      }));
    } catch (e) {
      // Why, in a person's words (PLAN 3.18), as the Mac's writing tools say it: never the
      // provider's own, nor its status.
      if (mine === turn) thinking.textContent = unasked(who.name, e);
      return;
    }
    if (mine !== turn || !dialog.open) return;
    let written: Written;
    try {
      written = await stage.written(ask, text);
    } catch (e) {
      thinking.textContent = `The answer could not be offered: ${said(e)}`;
      return;
    }
    if (mine !== turn || !dialog.open) return;
    // No answer, as a model that refuses or runs out of room gives: nothing to take.
    if (!written.text.trim()) {
      thinking.textContent = `No answer from ${who.name}: the model wrote nothing. Ask again, or try another model.`;
      leave.textContent = "Close";
      return;
    }
    thinking.hidden = true;
    if (written.patch === null) {
      read.textContent = written.text;
      read.hidden = false;
      leave.textContent = "Done";
      leave.focus();
      return;
    }
    offered = { ask, written };
    answer.value = written.text;
    answer.hidden = false;
    use.hidden = false;
    use.disabled = false;
    answer.focus();
  }

  // An answer with no words left in it is nothing to take, as no answer is: Use it waits for words.
  answer.addEventListener("input", () => {
    use.disabled = !answer.value.trim();
  });

  /** Take the answer as it stands in the dialog: one patch, one step to undo. */
  async function take() {
    const now = host.shown();
    if (!offered || !now) return;
    const { ask, written } = offered;
    offered = undefined;
    try {
      // Words edited in the dialog are taken as they are edited.
      const taking = answer.value === written.text ? written : await stage.written(ask, answer.value);
      // A text by the words it now reads, as the canvas shows it; notes by their slide.
      const what = ask.kind === "describe" ? "the picture" : ask.node !== undefined ? quote(taking.text) : host.slide(ask.state ?? now.state);
      if (!taking.patch?.length) return void host.say(`Nothing to change: ${what} already reads so`);
      const { source, edited } = await stage.make(host.source, taking.patch, now.index, host.format());
      host.apply(source, edited, TOOK[ask.kind](what));
      host.say(`${TOOK[ask.kind](what)} · ⌘Z undoes it`);
    } catch (e) {
      host.say(`Not changed: ${said(e)}`);
    }
  }

  // An answer is being taken from the press of Use on: the dialog says it closed a task later,
  // and what a test waits for is the answer taken, not the close.
  let taken: (() => void) | undefined;
  listen(use, "click", () => {
    settled = new Promise<void>((done) => (taken = done));
  });
  listen(dialog, "close", () => {
    // An answer still on its way is dropped.
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
    offer,
    /** The last question asked and its answer taken, settled: what a test waits for. */
    settled: () => settled,
  };
}
