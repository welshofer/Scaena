// Find and replace across the deck's texts, in every state (PLAN 2.47, ADR-0013): a bar above the
// preview, opened by ⌘F (Ctrl+F) anywhere but the source, or by Find. The page finds nothing
// itself: the engine says which texts hold what is sought, once for each place each is written
// (the node's own, a state's delta, or the deck's overrides), with the states that show it, and
// makes the patch that replaces it, a `replace_text` where each text lives.
//
// - Enter, or ↓, goes to the next match, Shift+Enter, or ↑, to the one before: its state shown, its
//   node selected, and its characters marked on the canvas.
// - Replace replaces the match shown, then goes on to the next after what it put in; Replace All
//   replaces every match in one patch, one step to undo, and says how many, in how many texts.
// - Match case and Whole words, as asked. Escape, or ×, closes the bar.
import type { Edited, Found, Query } from "./protocol";
import type { Stage } from "./stage";

/** What the find bar asks of the editor around it. */
export interface FindEditor {
  /** The state shown, by its id and its slot's index; none while the source does not compile. */
  shown(): { state: string; index: number } | undefined;
  /** The deck's states, by id, in order. */
  states(): string[];
  format(): string | undefined;
  source(): string;
  /** Take `source`, a patch's, as one change: one step to undo. */
  apply(source: string, edited: Edited): void;
  say(text: string): void;
  /** Show `state`, then mark characters `from`..`to` (UTF-16) of `node`'s text there, and select
   * it. */
  reveal(state: string, node: string, from: number, to: number): Promise<void>;
  /** Take the mark away. */
  unmark(): void;
}

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
/** UTF-16 offset of character `chars` (Unicode scalar values, as the engine counts) in `text`. */
const utf16 = (text: string, chars: number) => [...text].slice(0, chars).join("").length;
const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

export function finder(stage: Stage, bar: HTMLElement, editor: FindEditor) {
  const field = bar.querySelector<HTMLInputElement>('input[name="find"]')!;
  const replacement = bar.querySelector<HTMLInputElement>('input[name="replace"]')!;
  const matchCase = bar.querySelector<HTMLInputElement>('input[name="case"]')!;
  const wholeWords = bar.querySelector<HTMLInputElement>('input[name="words"]')!;
  const count = bar.querySelector<HTMLOutputElement>("output")!;
  /** What the query finds, as last asked. */
  let found: Found[] = [];
  /** The match shown, as its place among all of them, in order; none before the first step. */
  let at: number | undefined;
  /** A search under way: the next waits for it, and a stale one is dropped. */
  let asked = 0;

  const query = (): Query => ({ find: field.value, case: matchCase.checked, words: wholeWords.checked });
  /** Every match, in order, as `[text, match]` into `found`. */
  const all = () => found.flatMap((f, i) => f.matches.map((_, k): [number, number] => [i, k]));
  const total = () => found.reduce((n, f) => n + f.matches.length, 0);

  function tell() {
    const n = total();
    count.value = !field.value ? "" : n === 0 ? "no match" : at === undefined ? plural(n, "match", "matches") : `${at + 1} of ${n}`;
    for (const b of bar.querySelectorAll<HTMLButtonElement>("[data-find=prev],[data-find=next],[data-find=replace],[data-find=all]")) b.disabled = n === 0;
  }

  /** Ask again what the query finds in the source as it stands; the match shown stays at its place
   * among them, or the last. */
  async function search() {
    const mine = ++asked;
    const q = query();
    const next = q.find ? await stage.find(editor.source(), q).catch((e) => (editor.say(`not found: ${said(e)}`), [])) : [];
    if (mine !== asked) return;
    found = next;
    const n = total();
    if (at !== undefined) at = n ? Math.min(at, n - 1) : undefined;
    tell();
  }

  /** Show match `n`, among all of them: its state, its node selected, its characters marked. */
  async function show(n: number) {
    const matches = all();
    if (!matches.length) return editor.unmark();
    at = ((n % matches.length) + matches.length) % matches.length;
    tell();
    const [i, k] = matches[at];
    const f = found[i];
    const [from, to] = f.matches[k];
    await editor.reveal(f.state, f.node, utf16(f.text, from), utf16(f.text, to));
    const elsewhere = f.states.length > 1 ? `, and ${plural(f.states.length - 1, "state")} more that show it so` : "";
    editor.say(`${f.node} in ${f.state}${elsewhere}: “${f.text}”`);
  }

  const step = (by: 1 | -1) => show(at === undefined ? (by > 0 ? 0 : -1) : at + by);

  /** Make `ops` on the source as it stands: one change to undo, after which state `then` is shown,
   * or the state shown now. */
  async function make(ops: unknown[], done: string, then?: string) {
    const shown = editor.shown();
    if (!shown || !ops.length) return false;
    const next = then === undefined ? -1 : editor.states().indexOf(then);
    try {
      const { source, edited } = await stage.make(editor.source(), ops, next >= 0 ? next : shown.index, editor.format());
      editor.apply(source, edited);
      editor.say(done);
      return true;
    } catch (e) {
      editor.say(`not replaced: ${said(e)}`);
      return false;
    }
  }

  /** Replace the match shown, or the first, then show the next after what was put in its place,
   * so that a replacement the query still finds ("Rev" for "REV", case alike) is not found again. */
  async function replaceOne() {
    if (!total()) return;
    if (at === undefined) return void (await show(0));
    const before = all();
    const [i, k] = before[at];
    const f = found[i];
    const past = f.matches[k][0] + [...replacement.value].length;
    const ops = await stage.replacing(editor.source(), query(), replacement.value, [i, k]).catch(() => []);
    const reach = f.states.length > 1 ? ` in ${plural(f.states.length, "state")}` : "";
    // A replacement changes only its own text, so the next match is the one after it now, and the
    // deck after the patch shows that one's state.
    const then = found[before[(at + 1) % before.length][0]].state;
    if (!(await make(ops, `replaced in ${f.node}${reach}`, then))) return;
    await search();
    const matches = all();
    if (!matches.length) return editor.unmark();
    // The text replaced in, if the query still finds it, else the one now where it stood.
    const same = found.findIndex((g) => g.node === f.node && g.lives === f.lives);
    const next = matches.findIndex(([j, m]) => (same < 0 ? j >= i : j > same || (j === same && found[j].matches[m][0] >= past)));
    await show(Math.max(next, 0));
  }

  /** Replace every match in one patch. */
  async function replaceAll() {
    const [n, texts] = [total(), found.length];
    if (!n) return;
    const ops = await stage.replacing(editor.source(), query(), replacement.value).catch(() => []);
    if (!(await make(ops, `replaced ${plural(n, "match", "matches")} in ${plural(texts, "text")}, one step to undo`))) return;
    at = undefined;
    await search();
    editor.unmark();
  }

  function open() {
    bar.hidden = false;
    field.focus();
    field.select();
    void search();
  }

  function close() {
    bar.hidden = true;
    at = undefined;
    editor.unmark();
  }

  /** The query changed: what it finds is asked again, from the first. */
  const again = () => {
    at = undefined;
    void search();
  };
  field.oninput = matchCase.onchange = wholeWords.onchange = again;
  bar.onkeydown = (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      return close();
    }
    if (e.key === "Enter" && e.target === field) {
      e.preventDefault();
      return void step(e.shiftKey ? -1 : 1);
    }
    if (e.key === "Enter" && e.target === replacement) {
      e.preventDefault();
      return void ((e.metaKey || e.ctrlKey) ? replaceAll() : replaceOne());
    }
    if ((e.key === "ArrowDown" || e.key === "ArrowUp") && e.target === field) {
      e.preventDefault();
      return void step(e.key === "ArrowDown" ? 1 : -1);
    }
  };
  bar.onclick = (e) => {
    const what = (e.target as Element).closest<HTMLElement>("[data-find]")?.dataset.find;
    if (what === "next" || what === "prev") void step(what === "next" ? 1 : -1);
    else if (what === "replace") void replaceOne();
    else if (what === "all") void replaceAll();
    else if (what === "close") close();
  };

  return {
    open,
    close,
    /** The source changed: what the query finds is asked again, while the bar is open. */
    changed: () => {
      if (!bar.hidden) void search();
    },
    /** What the bar finds now, and the match shown, for a test. */
    found: () => found,
    at: () => at,
  };
}
