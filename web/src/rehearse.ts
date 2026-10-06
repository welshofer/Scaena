// Rehearsal (PLAN 2.63): the deck played as presented, with the time spent in each state kept.
//
// - Rehearse plays the deck from its first state, in the format the canvas shows: each state's cue
//   as it comes, then the state at rest until the presenter goes on, as a presenter would play it.
//   → ↓ PageDown Space Enter, a click on the canvas, or On go on; ← ↑ PageUp Backspace, or Back, go
//   back; Escape, or End, stops, and so does going on from the last state.
// - The bar over the canvas says which state is shown, how long it has been shown, and how long the
//   rehearsal has run.
// - At the end, what each state took: a state's cue plays, then its hold, so the hold the time
//   keeps is the time less the cue's span, to a tenth of a second. A state shown twice keeps both
//   times. Keep makes each state reached hold that long, one `set_state` of `hold` each, as one
//   patch: one step to undo. A state not reached keeps its hold.
// - Nothing the rehearsal does changes the deck until Keep.
import type { Slot } from "./protocol";
import type { Stage } from "./stage";

/** What the rehearsal asks of the editor around it. */
export interface RehearsalEditor {
  /** The deck's states as its timeline places them, in the format the canvas shows; none where the
   * deck does not compile. */
  slots(): Promise<Slot[] | undefined>;
  /** The format the canvas shows, if not the deck's own. */
  format(): string | undefined;
  /** Show state `index` at rest again, the rehearsal over. */
  show(index: number): void;
  /** Make `ops` one change of the deck, one step to undo; whether it was made. */
  keep(ops: unknown[], what: string): Promise<boolean>;
  say(text: string): void;
}

/** A state as the rehearsal kept it: how long it was shown, and the hold that keeps. */
export interface Rehearsed {
  state: string;
  /** How long it was shown, ms; 0 for a state not reached. */
  spent: number;
  /** Its cue's span, ms. */
  span: number;
  /** Its hold now, ms. */
  hold: number;
  /** The hold the time spent keeps, ms: none for a state not reached. */
  keeps?: number;
}

const ON = new Set(["ArrowRight", "ArrowDown", "PageDown", " ", "Enter"]);
const BACK = new Set(["ArrowLeft", "ArrowUp", "PageUp", "Backspace"]);

/** `ms` as the bar and the table say it: `1:05`, or `4.2 s` under a minute. */
export const clock = (ms: number) => {
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)} s`;
  const m = Math.floor(s / 60);
  return `${m}:${String(Math.floor(s - m * 60)).padStart(2, "0")}`;
};

/** The rehearsal: its bar in `bar`, its table in `dialog`, over `stage` and the canvas `preview`. */
export function rehearsal(stage: Stage, bar: HTMLElement, dialog: HTMLDialogElement, preview: HTMLElement, editor: RehearsalEditor) {
  const where = bar.querySelector<HTMLElement>("[data-where]")!;
  const rows = dialog.querySelector<HTMLTableSectionElement>("tbody")!;
  const total = dialog.querySelector<HTMLElement>("[data-total]")!;
  /** The states as the rehearsal began, and the time each was shown. */
  let slots: Slot[] = [];
  let spent: number[] = [];
  /** The state shown, and since when, by the page's clock. */
  let index = 0;
  let since = 0;
  let began = 0;
  let running = false;
  let ticking: ReturnType<typeof setInterval> | undefined;
  /** What the last rehearsal kept, for the table and a test. */
  let kept: Rehearsed[] = [];

  /** Count the time since the state shown came up toward it. */
  function count() {
    const now = performance.now();
    spent[index] += now - since;
    since = now;
  }

  function tell() {
    const now = performance.now();
    const state = slots[index]?.state ?? "";
    where.textContent = `${state} (${index + 1} of ${slots.length}) · ${clock(spent[index] + now - since)} here · ${clock(now - began)} in all`;
  }

  function play() {
    stage.run(index, 0, editor.format(), false, true);
    tell();
  }

  async function start() {
    if (running) return;
    const now = await editor.slots().catch(() => undefined);
    if (running || !now?.length) return editor.say("not rehearsed: the deck does not compile");
    slots = now;
    spent = slots.map(() => 0);
    index = 0;
    began = since = performance.now();
    running = true;
    bar.hidden = false;
    document.addEventListener("keydown", key, true);
    preview.addEventListener("pointerdown", click, true);
    ticking = setInterval(tell, 250);
    play();
    editor.say("rehearsing: → goes on, ← back, Escape stops");
  }

  /** Go `by` states on, or back; on from the last, stop. */
  function go(by: number) {
    if (!running) return;
    count();
    const next = index + by;
    if (next >= slots.length) return stop();
    if (next < 0) return;
    index = next;
    play();
  }

  function stop() {
    if (!running) return;
    count();
    running = false;
    clearInterval(ticking);
    document.removeEventListener("keydown", key, true);
    preview.removeEventListener("pointerdown", click, true);
    stage.pause();
    bar.hidden = true;
    kept = slots.map((slot, i) => ({
      state: slot.state,
      spent: spent[i],
      span: slot.span,
      hold: slot.hold,
      keeps: spent[i] > 0 ? Math.max(0, Math.round((spent[i] - slot.span) / 100) * 100) : undefined,
    }));
    const last = index;
    table();
    dialog.returnValue = "";
    dialog.showModal();
    editor.show(last);
  }

  function table() {
    const all = kept.reduce((n, k) => n + k.spent, 0);
    total.textContent = clock(all);
    rows.replaceChildren(
      ...kept.map((k) => {
        const tr = document.createElement("tr");
        tr.dataset.state = k.state;
        const cells = [k.spent ? clock(k.spent) : "not reached", clock(k.span), k.keeps === undefined ? "as it is" : clock(k.keeps), k.hold ? clock(k.hold) : "none"];
        tr.append(Object.assign(document.createElement("th"), { scope: "row", textContent: k.state }));
        for (const text of cells) tr.append(Object.assign(document.createElement("td"), { textContent: text }));
        return tr;
      }),
    );
  }

  function key(e: KeyboardEvent) {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (ON.has(e.key)) go(1);
    else if (BACK.has(e.key)) go(-1);
    else if (e.key === "Escape") stop();
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  /** A press on the canvas goes on, and selects nothing. */
  function click(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    go(1);
  }

  bar.querySelector("[data-on]")?.addEventListener("click", () => go(1));
  bar.querySelector("[data-back]")?.addEventListener("click", () => go(-1));
  bar.querySelector("[data-end]")?.addEventListener("click", () => stop());
  dialog.addEventListener("close", async () => {
    if (dialog.returnValue !== "keep") return editor.say("rehearsal: nothing kept");
    const ops = kept.filter((k) => k.keeps !== undefined && k.keeps !== k.hold).map((k) => ({ op: "set_state", id: k.state, prop: "hold", value: k.keeps }));
    if (!ops.length) return editor.say("rehearsal: every hold is as it was");
    await editor.keep(ops, `${ops.length} hold${ops.length === 1 ? "" : "s"} from the rehearsal`);
  });

  return {
    start,
    stop,
    /** Whether a rehearsal is running. */
    running: () => running,
    /** What the last rehearsal kept, for a test. */
    kept: () => kept,
  };
}
