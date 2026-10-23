// Rehearsal (PLAN 2.63): the deck played as presented, with the time spent in each state kept.
//
// - Rehearse plays the deck from its first state, in the format the canvas shows: each state's cue
//   as it comes, then the state at rest until the presenter goes on, as a presenter would play it.
//   → ↓ PageDown Space Enter, a click on the canvas, or On go on; ← ↑ PageUp Backspace, or Back, go
//   back; Escape, or End, stops, and so does going on from the last state.
// - The bar over the canvas says which state is shown, how long it has been shown, and how long the
//   rehearsal has run. Under it, the state's notes as a speaker reads them and the next slide's
//   name, as the presenter view shows them (SPEC §2.6, lift-up USE-3): a speaker rehearses the
//   talk, not the slides alone.
// - At the end, what each state took: a state's cue plays, then its hold, so the hold the time
//   keeps is the time less the cue's span, to a tenth of a second. A state shown twice keeps both
//   times. Keep makes each state reached hold that long, one `set_state` of `hold` each, as one
//   patch: one step to undo. A state not reached keeps its hold.
// - Nothing the rehearsal does changes the deck until Keep.
import type { Key } from "./commands";
import { listen } from "./life";
import type { Slot, SpeakerNotes } from "./protocol";
import { renderNotes } from "./speaker-notes";
import type { Stage } from "./stage";

/** What the rehearsal asks of the editor around it. */
export interface RehearsalEditor {
  /** The deck's states as its timeline places them, in the format the canvas shows; none where the
   * deck does not compile. */
  slots(): Promise<Slot[] | undefined>;
  /** The format the canvas shows, if not the deck's own. */
  format(): string | undefined;
  /** The notes a presenter reads for `state` (SPEC §2.6). */
  notes(state: string): Promise<SpeakerNotes>;
  /** Show state `index` at rest again, the rehearsal over. */
  show(index: number): void;
  /** Make `ops` one change of the deck, one step to undo; whether it was made. */
  keep(ops: unknown[], what: string): Promise<boolean>;
  say(text: string): void;
}

/** A state as the rehearsal kept it: how long it was shown, and the hold that keeps; `said` as a
 * person counts it. */
export interface Rehearsed {
  state: string;
  said: string;
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

/** A key as the keys sheet shows it. */
const shown = (key: string) => ({ ArrowRight: "→", ArrowLeft: "←", ArrowUp: "↑", ArrowDown: "↓", " ": "Space" })[key] ?? key;

/** What a rehearsal answers (PLAN 2.65), from the keys it goes on and back by, as the keys sheet
 * lists it. */
export const rehearsalKeys = (): Key[] => [
  { keys: [...ON].map(shown).join(" "), label: "Rehearsing: go on; from the last slide, stop", group: "Rehearse" },
  { keys: "Click the canvas", label: "Go on", group: "Rehearse" },
  { keys: [...BACK].map(shown).join(" "), label: "Go back", group: "Rehearse" },
  { keys: "Escape", label: "Stop, and see what each slide took", group: "Rehearse" },
];

/** `ms` as the bar and the table say it: `1:05`, or `4.2 s` under a minute. */
export const clock = (ms: number) => {
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)} s`;
  const m = Math.floor(s / 60);
  return `${m}:${String(Math.floor(s - m * 60)).padStart(2, "0")}`;
};

/** The rehearsal: its bar in `bar`, the notes and the next slide's name in `pane`, its table in
 * `dialog`, over `stage` and the canvas `preview`. */
export function rehearsal(
  stage: Stage,
  bar: HTMLElement,
  pane: HTMLElement,
  dialog: HTMLDialogElement,
  preview: HTMLElement,
  editor: RehearsalEditor,
) {
  const where = bar.querySelector<HTMLElement>("[data-where]")!;
  const nextName = pane.querySelector<HTMLElement>("[data-next]")!;
  const notes = pane.querySelector<HTMLElement>("[data-notes]")!;
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
    // The slide as a person counts it (PLAN 2.99).
    const said = slots[index]?.said ?? `${index + 1} of ${slots.length}`;
    where.textContent = `Rehearsing ${said} · ${clock(spent[index] + now - since)} here · ${clock(now - began)} in all`;
  }

  function play() {
    stage.run(index, 0, editor.format(), false, true);
    tell();
    noted();
  }

  /** The notes of the state shown, drawn as a speaker reads them, and the next slide's name, as
   * the presenter view shows them; an answer for a state no longer shown is dropped. */
  function noted() {
    const at = index;
    const next = slots[at + 1];
    nextName.textContent = next ? `Next: ${next.said ?? next.state}` : "Next: the end";
    editor.notes(slots[at].state).then(
      (read) => {
        if (!running || index !== at) return;
        renderNotes(notes, read.blocks);
        notes.scrollTop = 0;
      },
      () => renderNotes(notes, []),
    );
  }

  async function start() {
    if (running) return;
    const now = await editor.slots().catch(() => undefined);
    if (running || !now?.length) return editor.say("Not rehearsed: the source has an error to fix first");
    slots = now;
    spent = slots.map(() => 0);
    index = 0;
    began = since = performance.now();
    running = true;
    bar.hidden = false;
    pane.hidden = false;
    document.addEventListener("keydown", key, true);
    preview.addEventListener("pointerdown", click, true);
    ticking = setInterval(tell, 250);
    play();
    editor.say("Rehearsing: → goes on, ← back, Escape stops");
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
    pane.hidden = true;
    kept = slots.map((slot, i) => ({
      state: slot.state,
      said: slot.said ?? slot.state,
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
        const cells = [k.spent ? clock(k.spent) : "Not reached", clock(k.span), k.keeps === undefined ? "As it is" : `After ${clock(k.keeps)}`, k.hold ? `After ${clock(k.hold)}` : "On click"];
        tr.append(Object.assign(document.createElement("th"), { scope: "row", textContent: k.said }));
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

  listen(bar.querySelector<HTMLElement>("[data-on]")!, "click", () => go(1));
  listen(bar.querySelector<HTMLElement>("[data-back]")!, "click", () => go(-1));
  listen(bar.querySelector<HTMLElement>("[data-end]")!, "click", () => stop());
  listen(dialog, "close", async () => {
    if (dialog.returnValue !== "keep") return editor.say("Rehearsal: nothing kept");
    const ops = kept.filter((k) => k.keeps !== undefined && k.keeps !== k.hold).map((k) => ({ op: "set_state", id: k.state, prop: "hold", value: k.keeps }));
    if (!ops.length) return editor.say("Rehearsal: every timing is as it was");
    await editor.keep(ops, `${ops.length} timing${ops.length === 1 ? "" : "s"} from the rehearsal`);
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
