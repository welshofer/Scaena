// Write This Slide Again… and Add a Slide About… (lift-up FUNC-1): one slide written by the model the
// assistant uses, with the user's own key, the way the deck's generator builds slides. The
// generator's module says what to ask, from the deck as it stands: its brief and arc, the part of
// the talk, the slides either side, and the slide's words where it is written again
// (`scaena_ops::narrative::one_asked`); the worker asks it once, with nothing to call (`write`); and
// the slide the answer holds is written as `slide_write` writes one, by `agent:` and the model's
// name, as one step to undo. An answer that does not read as a slide changes nothing, and says why.
import type { Edited, SlidePlace } from "./protocol";
import type { Stage } from "./stage";
import type { Who } from "./writing";
import { unasked } from "./words";

/** What the dialog asks of the editor around it. */
export interface SlideAskHost {
  /** The assistant's provider, model, and key; or why there is none to ask. */
  who(): Who | string;
  /** The state shown, by its id and its slot's index. */
  shown(): { state: string; index: number } | undefined;
  format(): string | undefined;
  /** Take `source`, the slide written: one step to undo. */
  apply(source: string, edited: Edited, said?: string): void;
  /** Show the state `id`. */
  show(id: string): void;
  say(text: string): void;
  /** Show the assistant, its key in focus: where a key goes. */
  keys(): void;
  /** `state` as a person counts it: `Slide 3`. */
  slide(state: string): string;
}

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
const $ = <T extends HTMLElement>(selector: string) => document.querySelector<T>(selector)!;

export function slideAsker(stage: Stage, host: SlideAskHost) {
  const dialog = $<HTMLDialogElement>("#slide-ask");
  const title = $("#slide-ask-title");
  const label = $("#slide-ask-label");
  const ask = $<HTMLTextAreaElement>("#slide-ask-text");
  const thinking = $("#slide-ask-thinking");
  const by = $("#slide-ask-by");
  const go = $<HTMLButtonElement>("#slide-ask-go");
  const key = $<HTMLButtonElement>("#slide-ask-key");
  /** Where the slide goes, while the dialog is open. */
  let place: SlidePlace | undefined;
  /** The question being answered: a later one, or the dialog closed, drops an earlier answer. */
  let turn = 0;
  /** The last slide asked for, settled: what a test waits for. */
  let settled: Promise<void> = Promise.resolve();
  /** What stops the request of the slide being asked for (lift-up PERF-4). */
  let stopping: AbortController | undefined;

  /** Open the dialog: the slide shown written again, or a new one after it. */
  function open(kind: "redo" | "add") {
    const now = host.shown();
    if (!now) return host.say("Nothing to write yet: the deck must compile first");
    place = kind === "redo" ? { replace: now.state } : { after: now.state };
    turn++;
    const slide = host.slide(now.state).replace(/, step \d+$/, "");
    title.textContent = kind === "redo" ? `Write ${slide} again` : `Add a slide after ${slide}`;
    label.textContent = kind === "redo" ? "What to change (or leave it empty)" : "What the new slide is about";
    ask.value = "";
    ask.required = kind === "add";
    thinking.textContent = "";
    go.disabled = false;
    go.textContent = kind === "redo" ? "Write it again" : "Write the slide";
    const who = host.who();
    key.hidden = typeof who !== "string";
    by.textContent =
      typeof who === "string" ? who : `Written by ${who.model}, with your ${who.name} key: what it is asked goes to ${who.name} and nowhere else.`;
    if (!dialog.open) dialog.showModal();
    ask.focus();
  }

  async function write() {
    const now = host.shown();
    const who = host.who();
    if (!place || !now) return;
    if (typeof who === "string") {
      thinking.textContent = who;
      return;
    }
    if (ask.required && !ask.value.trim()) return void ask.focus();
    const mine = ++turn;
    const where = place;
    go.disabled = true;
    thinking.textContent = `Asking ${who.model}…`;
    let asked: { system: string; prompt: string };
    try {
      asked = await stage.slideAsk(where, ask.value);
    } catch (e) {
      thinking.textContent = `Nothing to ask: ${said(e)}`;
      go.disabled = false;
      return;
    }
    let text: string;
    try {
      stopping?.abort();
      stopping = new AbortController();
      ({ text } = await stage.write(
        { provider: who.provider, model: who.model, key: who.key, base: who.base, instructions: asked.system, prompt: asked.prompt },
        stopping.signal,
      ));
    } catch (e) {
      if (mine === turn) {
        thinking.textContent = unasked(who.name, e);
        go.disabled = false;
      }
      return;
    }
    if (mine !== turn || !dialog.open) return;
    thinking.textContent = "Writing the slide…";
    try {
      const { written, source, edited } = await stage.slideWrite(where, text, `agent:${who.model}`, now.index, host.format());
      if (mine !== turn) return;
      dialog.close();
      const first = written.slide[0];
      host.apply(source, edited, where.replace ? `Wrote ${host.slide(where.replace)} again` : "Wrote a new slide");
      if (first) host.show(first);
      const left = written.left.length;
      host.say(`${where.replace ? "Wrote the slide again" : "Wrote a new slide"}${left ? `: ${left} ${left === 1 ? "issue" : "issues"} left to look at` : ""}`);
    } catch (e) {
      // Nothing changed: the answer did not read as a slide, or the deck would not take it.
      if (mine === turn) {
        thinking.textContent = `Nothing changed: ${said(e)}`;
        go.disabled = false;
      }
    }
  }

  go.addEventListener("click", (e) => {
    e.preventDefault();
    settled = write();
  });
  key.addEventListener("click", () => {
    dialog.close();
    host.keys();
  });
  dialog.addEventListener("close", () => {
    turn++;
    place = undefined;
    // The request of a slide still on its way is stopped (lift-up PERF-4).
    stopping?.abort();
    stopping = undefined;
  });

  return {
    open,
    /** The last slide asked for, settled. */
    settled: () => settled,
  };
}
