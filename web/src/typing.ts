// Text in place (PLAN 2.32, ADR-0013): a double click on a text puts a caret in it where the
// engine says the character is, and typing there makes `replace_text` patches, by the user,
// written where the text lives. The page lays nothing out: the caret, the selection, and where a
// caret goes up or down a line come from the engine's carets, read from the glyphs it set.
//
// - A textarea no one sees holds the text as written and takes the keys, the clipboard, and an
//   input method's composition; its selection is the caret's. Left and right, by a character or
//   a word, and deleting are the textarea's own, in the text's order; up, down, home, and end go
//   by the engine's lines.
// - Each change to it is one `replace_text`: the characters it replaced, from what the deck reads
//   to what the textarea holds. One is made at a time, and what is typed meanwhile goes in the
//   next. A burst of typing is one step to undo, and a run of it one change in the bundle's
//   history.
// - Escape, a click outside the text, or focus elsewhere leaves it, the node still selected;
//   focus in the inspector keeps it, so a look chosen there goes to the characters selected.
// - Characters selected take a look (PLAN 2.38): ⌘B makes them bold, or not, as the engine reads
//   the weight each is set in; ⌘I sets them in italic, or not, as each asks for it (PLAN 2.40); and
//   a role or a color chosen in the inspector gives them that. Each is one `style_text`, written
//   where the text lives, one step to undo.
import { type Key, MOD, SHIFT } from "./commands";
import { listen } from "./life";
import type { CaretLine, Carets, CodeKey, Edited, LinkTarget, ListMark, Map6, Rect } from "./protocol";
import { askWords } from "./notes";
import type { Stage } from "./stage";
import { quote, reaching, sentence } from "./words";

/** What a text typed in answers that no command runs by name (PLAN 2.65), as the keys sheet lists
 * it; the rest are the keys of any field typed in. */
export const typingKeys = (): Key[] => [
  { keys: `${MOD}B`, label: "In a text typed in: the characters selected bold, or not", group: "Type" },
  { keys: `${MOD}I`, label: "In italic, or not", group: "Type" },
  { keys: "↑ ↓ Home End", label: "Go up or down a line, or to its start or end, as the text is set", group: "Type" },
  { keys: "Enter", label: "In a list: a new item like it; in an empty item, the list ends", group: "Type" },
  { keys: `Tab, ${SHIFT}Tab`, label: "In a list: the items selected a level in, or out; elsewhere, the paragraph an item a level in", group: "Type" },
  { keys: "Backspace at an item's start", label: "Its marker off, or a level out; again, it joins the paragraph before", group: "Type" },
  { keys: `Tab, ${SHIFT}Tab`, label: "In code: spaces to the next stop, or the lines selected a stop in; a stop out", group: "Type" },
  { keys: "Enter", label: "In code: a new line indented as this one, a stop more after an opening bracket", group: "Type" },
  { keys: `${MOD}K`, label: "In a text typed in: link the characters selected to a web address or a slide", group: "Type" },
  { keys: "Escape", label: "Stop typing: the text stays selected", group: "Type" },
];

/** What typing asks of the canvas and the editor around it. */
export interface Around {
  /** The state shown, by its id and its slot's index. */
  shown(): { state: string; index: number } | undefined;
  format(): string | undefined;
  source(): string;
  /** Take `source` as typed: one step to undo with what was typed just before it (`joins`), or
   * the first of a burst of typing. */
  typed(source: string, edited: Edited, joins: boolean): void;
  undo(): void;
  redo(): void;
  say(text: string): void;
  /** Take back `text`, if the status still says it: `instead` in its place, or what was said
   * before it. */
  unsay?(text: string, instead?: string): void;
  /** `node` as a person names it (PLAN 2.99): a text by its words, quoted. */
  name(node: string): string;
  /** The state `state` as a person counts it: Slide 3, or Slide 3, step 2. */
  slide(state: string): string;
  /** The state a slide's number names, as a person types it: `3`, `slide 3`, `3, step 2`. */
  numbered(typed: string): string | undefined;
  /** Draw the canvas again: the caret moved, or the text changed. */
  draw(): void;
  /** Canvas units to a CSS pixel. */
  unit(): number;
  /** Where the preview's view begins, canvas units: its top left corner, the canvas's own until it
   * is zoomed in (PLAN 2.46). */
  origin(): [number, number];
  /** Where `node` stands in the state shown: its box. */
  box(node: string): Rect | undefined;
  /** Where its `transform` draws it (PLAN 2.51): the map from its box to the canvas. */
  map(node: string): Map6 | undefined;
  /** The characters selected in the text typed in are now `selected`, or none are. */
  chose(selected: Selected | undefined): void;
  /** Whether focus gone to `to` keeps typing on: the inspector, which gives the characters
   * selected a look. */
  keeps(to: EventTarget | null): boolean;
  /** Typing in `node` has stopped: a shape's words give what is selected back to their shape
   * (PLAN 2.108). */
  left?(node: string): void;
  /** The language `node` is written in, where it is code (PLAN 2.133). */
  code?(node: string): string | undefined;
}

/** Characters selected in a text typed in: from `from` to `to`, Unicode scalar values, as
 * `style_text` counts them, in `state`. */
export interface Selected {
  node: string;
  state: string;
  from: number;
  to: number;
  /** The characters themselves. */
  text: string;
}

/** A change to a text: the UTF-16 range of what it read that `text` takes the place of. */
interface Change {
  from: number;
  to: number;
  text: string;
}

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** `p` through `m`. */
const through = (m: Map6, [x, y]: [number, number]): [number, number] => [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];

/** `p`, a point on the canvas, read back through `m`: where it is in the text as laid out. */
function back(m: Map6 | undefined, p: [number, number]): [number, number] {
  if (!m) return p;
  const det = m[0] * m[3] - m[1] * m[2];
  if (!Number.isFinite(det) || Math.abs(det) < 1e-12) return p;
  const [x, y] = [p[0] - m[4], p[1] - m[5]];
  return [(m[3] * x - m[2] * y) / det, (m[0] * y - m[1] * x) / det];
}

/** The paragraphs of `text`, UTF-16 ranges without the break that ends each, as the deck counts
 * them (`scaena_core::lists::paragraphs`): `\n`, `\r`, `\r\n`, VT, FF, NEL, U+2028, and U+2029 end one. */
export function paragraphs(text: string): [number, number][] {
  const out: [number, number][] = [];
  let start = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (!"\n\r\v\f\u0085\u2028\u2029".includes(c)) continue;
    out.push([start, i]);
    if (c === "\r" && text[i + 1] === "\n") i++;
    start = i + 1;
  }
  out.push([start, text.length]);
  return out;
}

/** The paragraphs a selection from `from` to `to` (UTF-16) touches: the first's and the last's
 * index. */
export function touched(text: string, from: number, to: number): [number, number] {
  const p = paragraphs(text);
  const at = (o: number) => Math.max(0, p.findIndex(([, end]) => o <= end));
  return [at(from), Math.max(at(from), at(to))];
}

/** How long a pause between two keys ends a burst of typing, ms: one step to undo each. */
const BURST = 1000;

/** Whether a caret at `offset` can stand on `line`: not past the line break it ends with. */
const holds = (line: CaretLine, offset: number) =>
  line.start <= offset && (offset < line.end || (offset === line.end && !line.broken));

/** The line a caret at `offset` stands on: `on`, if it can stand there, else the last line that
 * holds it, so a caret where a line wraps starts the next one. */
export function lineOf(c: Carets, offset: number, on?: number): number {
  if (on !== undefined && c.lines[on] && holds(c.lines[on], offset)) return on;
  for (let l = c.lines.length - 1; l >= 0; l--) if (holds(c.lines[l], offset)) return l;
  return offset === 0 ? 0 : c.lines.length - 1;
}

/** Where a caret at `offset` stands: its line, and its x. Inside a character, before it. */
export function caretAt(c: Carets, offset: number, on?: number): { line: number; x: number } {
  const l = lineOf(c, offset, on);
  const line = c.lines[l];
  const chars = line.chars;
  let i = 0;
  while (i < chars.length && chars[i][0] <= offset) i++;
  if (i === 0) return { line: l, x: chars.length ? chars[0][1] : line.x };
  const [at, lead, trail] = chars[i - 1];
  return { line: l, x: at < offset && i === chars.length && offset >= line.end ? trail : lead };
}

/** The offset on line `l` whose caret stands nearest canvas `x`: before one of its characters, or
 * after the last, the first in the text's order where two are as near. */
export function onLine(c: Carets, l: number, x: number): number {
  const line = c.lines[l];
  if (!line) return c.text.length;
  let best: [number, number] = [Infinity, line.start];
  for (const [at, lead] of line.chars) if (Math.abs(lead - x) < best[0]) best = [Math.abs(lead - x), at];
  const last = line.chars.at(-1);
  if (last && holds(line, line.end) && Math.abs(last[2] - x) < best[0]) best = [Math.abs(last[2] - x), line.end];
  return best[1];
}

/** The caret nearest `point`: its offset, and the line it stands on. */
export function caretNear(c: Carets, [x, y]: [number, number]): [number, number] {
  let l = c.lines.findIndex((line) => y < line.bottom);
  if (l < 0) l = c.lines.length - 1;
  return [onLine(c, l, x), l];
}

/** What a selection from `from` to `to` covers: a rectangle per stretch of adjacent characters on
 * a line. */
export function covered(c: Carets, from: number, to: number): Rect[] {
  const out: Rect[] = [];
  for (const line of c.lines) {
    const spans = line.chars
      .filter(([at, lead, trail]) => from <= at && at < to && lead !== trail)
      .map(([, lead, trail]): [number, number] => [Math.min(lead, trail), Math.max(lead, trail)])
      .sort((a, b) => a[0] - b[0]);
    const merged: [number, number][] = [];
    for (const [l, r] of spans) {
      const last = merged.at(-1);
      if (last && l <= last[1] + 0.5) last[1] = Math.max(last[1], r);
      else merged.push([l, r]);
    }
    for (const [l, r] of merged) out.push([l, line.top, r - l, line.bottom - line.top]);
  }
  return out;
}

/** Where line `l` ends for a caret: before the line break it ends with, if it does. */
const lastOf = (line: CaretLine) => (line.broken && line.chars.length ? line.chars.at(-1)![0] : line.end);

/** What changed from `was` to `now`, UTF-16: what they share at either end stays. What was
 * replaced starts no later than the caret now (`caret`), nor than where the selection started
 * before any key since (`bounds`), and ends no earlier than where it ended, counted from the end. A
 * surrogate pair stays whole. */
export function changed(was: string, now: string, caret: number, bounds?: { start: number; fromEnd: number }): Change | undefined {
  if (was === now) return undefined;
  const max = Math.min(was.length, now.length);
  let start = 0;
  while (start < max && was.charCodeAt(start) === now.charCodeAt(start)) start++;
  start = Math.min(start, caret, bounds?.start ?? Infinity);
  let end = 0;
  while (end < max - start && was.charCodeAt(was.length - 1 - end) === now.charCodeAt(now.length - 1 - end)) end++;
  end = Math.min(end, bounds?.fromEnd ?? Infinity);
  const low = (s: string, i: number) => i < s.length && s.charCodeAt(i) >= 0xdc00 && s.charCodeAt(i) <= 0xdfff;
  if (start > 0 && low(was, start)) start--;
  if (end > 0 && low(was, was.length - end)) end--;
  return { from: start, to: was.length - end, text: now.slice(start, now.length - end) };
}

/** UTF-16 `units` of `text` as characters (Unicode scalar values), as `replace_text` counts. */
const points = (text: string, units: number) => [...text.slice(0, units)].length;

/** The text reads `now` where it read `was`, and `value` is what was typed into `was` since, with
 * `caret` and `bounds` as `changed` takes them: what the textarea holds now, and where its caret
 * goes. What was typed goes in where the change left it standing, unless the change is where it
 * was typed; the caret goes after it, or where the text changed. */
export function follow(
  was: string,
  now: string,
  value: string,
  caret: number,
  bounds?: { start: number; fromEnd: number },
): { value: string; at: number; bounds?: { start: number; fromEnd: number }; lost: boolean } {
  const change = changed(was, now, Infinity)!;
  const typed = changed(was, value, caret, bounds);
  // How far what was typed moves: past the change by what the change added, before it not at all.
  const by = !typed
    ? undefined
    : typed.from >= change.to
      ? change.text.length - (change.to - change.from)
      : typed.to <= change.from
        ? 0
        : undefined;
  if (!typed || by === undefined) return { value: now, at: change.from + change.text.length, lost: typed !== undefined };
  const [from, to] = [typed.from + by, typed.to + by];
  return {
    value: now.slice(0, from) + typed.text + now.slice(to),
    at: from + typed.text.length,
    bounds: { start: from, fromEnd: now.length - to },
    lost: false,
  };
}

/** Typing in place over `overlay`, the canvas's, on `stage`'s state shown. */
export function typing(stage: Stage, overlay: HTMLElement, around: Around) {
  const area = document.createElement("textarea");
  area.className = "typing";
  area.setAttribute("autocomplete", "off");
  area.setAttribute("autocorrect", "off");
  area.setAttribute("autocapitalize", "off");
  area.spellcheck = false;
  area.hidden = true;
  overlay.append(area);

  /** The text typed in: its node, the state it is typed in, and kept to that state (`fork`). */
  let open: { node: string; state: string; index: number; fork: boolean } | undefined;
  /** Where a caret stands in it, as the engine last laid it out. */
  let carets: Carets | undefined;
  /** The source `carets` were read from: a change is made on it. Where the source changed under
   * the text (an undo, an edit in the source, a change on disk), the text is read again first. */
  let read: string | undefined;
  /** The line a caret was put on, where it could stand on two: a line's end or the next's start. */
  let on: number | undefined;
  /** Where up and down aim, canvas x: the caret's x when they began. */
  let goal: number | undefined;
  /** Where a pointer dragging a selection began, as an offset. */
  let anchor: number | undefined;
  /** Where the keys since the last change was made began to edit. */
  let bounds: { start: number; fromEnd: number } | undefined;
  /** A change is with the worker, or the text is being read again; another waits for it. */
  let sending = false;
  /** Typing left while a change was with the worker, or the text was being read again, straight
   * after keys (Escape, or a press elsewhere): the text as it was typed then, and the text and
   * source what was typed is a change of: as the deck read them when typing was left, then as the
   * change came to them, or the text as it was read again. What was typed is made then (`finish`),
   * so leaving loses no key. */
  let finishing:
    | { now: NonNullable<typeof open>; value: string; end: number; bounds: typeof bounds; text?: string; read?: string }
    | undefined;
  /** The text was asked to be read again while one was: it is, once none is. */
  let behind = false;
  /** What waits for every change typed so far to be made: an undo, so that none is made after
   * it. */
  let waiting: (() => void)[] = [];
  const settled = () => !sending && (!open || !carets || area.value === carets.text);
  const idle = () => new Promise<void>((done) => (settled() ? done() : waiting.push(done)));
  const wake = () => {
    if (settled()) for (const done of waiting.splice(0)) done();
  };
  /** When the keys of the next change were typed: the first since a change was made, and the
   * last. */
  let keys: { first: number; last: number } | undefined;
  /** When the last key of the change made before it was typed, in this text. */
  let made: number | undefined;
  /** The source the last change typed in this text made: what was typed when typing was left
   * joins that change's step to undo only where the source is still what it made. Read again
   * because something else changed it, or with nothing made yet, it is a step of its own. */
  let ours: string | undefined;

  /** Show the caret at `from`–`to` in the textarea, on line `line` where it could stand on two. */
  function put(from: number, to = from, line?: number, backward = false) {
    area.setSelectionRange(from, to, backward ? "backward" : "forward");
    on = line;
    around.draw();
  }

  /** The caret's place in the textarea, clamped to the text the engine last laid out. */
  function caret(): { from: number; to: number; head: number } {
    const n = carets?.text.length ?? 0;
    const [from, to] = [Math.min(area.selectionStart, n), Math.min(area.selectionEnd, n)];
    return { from, to, head: area.selectionDirection === "backward" ? from : to };
  }

  /** Begin typing in `node`, in the state shown, at the caret nearest `point` (the end without
   * one, or with `all` every character selected); `fork` keeps what is typed to that state. False
   * for a node that is no text. */
  async function enter(node: string, point: [number, number] | undefined, fork: boolean, all = false): Promise<boolean> {
    const shown = around.shown();
    if (!shown) return false;
    const source = around.source();
    const found = await stage.carets(source, shown.state, node, around.format());
    if (!found) return false;
    // Typing began in this text while its carets were read: Insert types in a text it inserts
    // (PLAN 3.21), and a double click on it meanwhile waits its turn behind that. What is typed
    // there stands, so no key typed since is lost.
    if (open?.node === node && open.state === shown.state) return true;
    leave();
    open = { node, state: shown.state, index: shown.index, fork };
    carets = found;
    read = source;
    keys = made = ours = undefined;
    area.hidden = false;
    area.value = found.text;
    area.setAttribute("aria-label", `Typing in ${around.name(node)}, where it stands`);
    area.focus({ preventScroll: true });
    // A slot's prompt, still as it was put, is selected whole, so typing replaces it (PLAN 3.30).
    if (point && !found.prompt) {
      const [at, line] = caretNear(found, back(around.map(node), point));
      put(at, at, line);
    } else put(all || found.prompt ? 0 : found.text.length, found.text.length);
    void tellWhere();
    return true;
  }

  /** Say what typing changes: that it began, at once, then the states it reaches once the worker
   * says, in its place while the status still says it, so a line said meanwhile (⌘K's "No link
   * made") stands. */
  async function tellWhere() {
    const now = open;
    if (!now) return;
    const begun = `Typing in ${around.name(now.node)} · Escape leaves it`;
    typingSaid = begun;
    around.say(begun);
    const op = { op: "replace_text", node: now.node, state: now.state, from: 0, to: 0, text: "·", ...(now.fork ? { fork: true } : {}) };
    const states = await stage.reach([op]).catch(() => undefined);
    if (open !== now || !states) return;
    const n = states.length;
    const where = now.fork ? "kept to this slide" : `what is typed shows ${reaching(states, now.state, around.slide)}`;
    typingSaid = `Typing in ${around.name(now.node)} · ${where} · Escape leaves it${now.fork || n < 2 ? "" : "; Alt and a double click keep it to this slide"}`;
    if (around.unsay) around.unsay(begun, typingSaid);
    else around.say(typingSaid);
  }
  /** What the status was last told of typing, which it takes back once typing stops. */
  let typingSaid: string | undefined;

  /** The characters selected, if any are. */
  function selection(): Selected | undefined {
    if (!open || !carets) return undefined;
    const { from, to } = caret();
    if (from === to) return undefined;
    return { node: open.node, state: open.state, from: points(carets.text, from), to: points(carets.text, to), text: carets.text.slice(from, to) };
  }

  /** What the inspector was last told is selected, so it is told again only of a change. */
  let told: string | undefined;
  function tell() {
    const now = selection();
    const key = now && JSON.stringify(now);
    if (key === told) return;
    told = key;
    around.chose(now);
  }

  /** Give the characters selected `look` (`style_text`, PLAN 2.38), once what was typed is made:
   * one step to undo. Whether it was given. */
  async function style(look: Record<string, unknown>, words?: string): Promise<boolean> {
    await idle();
    const now = open;
    if (now && around.source() !== read) await sync();
    const chosen = selection();
    if (!now || !chosen || open !== now) {
      around.say("Select characters in a text to give them a look");
      return false;
    }
    const op = { op: "style_text", node: now.node, state: now.state, from: chosen.from, to: chosen.to, look, ...(now.fork ? { fork: true } : {}) };
    sending = true;
    try {
      const typed = await stage.type(around.source(), [op], { index: now.index, state: now.state, node: now.node }, around.format());
      around.typed(typed.source, typed.edited, false);
      made = undefined;
      if (open !== now) onto(now, typed.carets?.text, typed.source);
      else {
        if (!typed.carets) leave();
        else {
          [carets, read] = [typed.carets, typed.source];
          // A look that changes the characters (a quote's figure, PLAN 2.72): the area takes the
          // text, the figure still selected.
          if (typed.carets.text !== area.value) {
            const [start, end, length] = [area.selectionStart, area.selectionEnd, area.value.length];
            area.value = typed.carets.text;
            area.setSelectionRange(start, Math.max(start, end + area.value.length - length));
          }
        }
      }
      around.say(`${quote(chosen.text)} · ${words ?? "a new look"}`);
      return true;
    } catch (e) {
      around.say(`No look given: ${said(e)}`);
      return false;
    } finally {
      sending = false;
      around.draw();
      settle();
      // The inspector reads the characters' look again.
      told = undefined;
      tell();
    }
  }

  /** ⌘B: the characters selected bold, or, all bold already, not (PLAN 2.38), as the engine
   * reads the weight it sets each in. */
  async function bold(): Promise<boolean> {
    await idle();
    const now = open;
    const chosen = selection();
    if (!now || !chosen) {
      around.say("Select characters to make them bold");
      return false;
    }
    let look: Record<string, unknown>;
    try {
      look = await stage.bolding(around.source(), now.state, now.node, chosen.from, chosen.to, around.format());
    } catch (e) {
      around.say(`Not made bold: ${said(e)}`);
      return false;
    }
    if (open !== now) return false;
    return style(look, look["style/weight"] === 700 ? "bold" : "not bold");
  }

  /** ⌘I: the characters selected in italic, or, all italic already, not (PLAN 2.40), as each
   * asks for it: a family without an italic face sets them upright all the same (W231). */
  async function italic(): Promise<boolean> {
    await idle();
    const now = open;
    const chosen = selection();
    if (!now || !chosen) {
      around.say("Select characters to set them in italic");
      return false;
    }
    let look: Record<string, unknown>;
    try {
      look = await stage.italicizing(around.source(), now.state, now.node, chosen.from, chosen.to, around.format());
    } catch (e) {
      around.say(`Not set in italic: ${said(e)}`);
      return false;
    }
    if (open !== now) return false;
    return style(look, look["style/italic"] === true ? "italic" : "upright");
  }

  /** A list's keys (ADR-0018, PLAN 2.69), once what was typed is made: the paragraphs the selection
   * touches made items of `kind`, or out of the list, or moved `by` levels; one `list` patch, one
   * step to undo. `done` is what the status says. */
  async function list(how: { kind?: "bullet" | "number" | "none"; by?: number }, done: string): Promise<boolean> {
    await idle();
    const now = open;
    if (now && around.source() !== read) await sync();
    if (!now || !carets || open !== now) return false;
    const { from, to } = caret();
    const op = {
      op: "list",
      node: now.node,
      state: now.state,
      from: points(carets.text, from),
      to: points(carets.text, to),
      ...how,
      ...(now.fork ? { fork: true } : {}),
    };
    sending = true;
    try {
      const typed = await stage.type(around.source(), [op], { index: now.index, state: now.state, node: now.node }, around.format());
      around.typed(typed.source, typed.edited, false);
      made = undefined;
      if (open !== now) onto(now, typed.carets?.text, typed.source);
      else if (!typed.carets) leave();
      else [carets, read] = [typed.carets, typed.source];
      around.say(`${sentence(around.name(now.node))}: ${done}`);
      return true;
    } catch (e) {
      around.say(`Not listed: ${said(e)}`);
      return false;
    } finally {
      sending = false;
      around.draw();
      settle();
    }
  }

  /** The items of the paragraphs the selection touches, as the engine last set them. */
  function itemsSelected(): (ListMark | null)[] {
    if (!carets) return [];
    const { from, to } = caret();
    const [first, last] = touched(carets.text, from, to);
    return Array.from({ length: last - first + 1 }, (_, k) => carets?.items?.[first + k] ?? null);
  }

  /** ⌘⇧8 and ⌘⇧7: the paragraphs the selection touches bulleted, or numbered; all of that kind
   * already, out of the list. */
  function toggle(kind: "bullet" | "number"): Promise<boolean> {
    const all = itemsSelected().every((i) => i?.kind === kind);
    return list({ kind: all ? "none" : kind }, all ? "out of the list" : kind === "bullet" ? "bulleted" : "numbered");
  }

  /** Whether a field over the text asks for a link: focus there keeps typing on. */
  let asking = false;
  /** Where the characters from `from` to `to` (UTF-16) link to, as the engine last set them: the
   * one place every one of them goes; none where any goes nowhere, or they go to two. */
  function linked(from: number, to: number): LinkTarget | undefined {
    const over = (carets?.links ?? []).filter(([a, b]) => a < Math.max(to, from + 1) && b > from).sort((x, y) => x[0] - y[0]);
    let at = from;
    for (const [a, b] of over) {
      if (a > at) return undefined;
      at = Math.max(at, b);
    }
    const first = over[0] && JSON.stringify(over[0][2]);
    return at >= to && over.length && over.every(([, , t]) => JSON.stringify(t) === first) ? over[0][2] : undefined;
  }
  /** Where `typed` links to: a web address (`https://`, `http://`, `mailto:`, or a host typed
   * without its scheme, which takes `https://` as a presentation app gives it); a state by its
   * slide's number, or its id (`#` before it, or not); `null`, typed empty, the link taken away;
   * none where it names neither. */
  function target(typed: string): { link: LinkTarget | null; said: string } | undefined {
    const to = typed.trim();
    if (!to) return { link: null, said: "link removed" };
    if (/^(https?:\/\/|mailto:)\S+$/.test(to)) return { link: { href: to }, said: `linked to ${to}` };
    const numbered = around.numbered(to);
    if (numbered !== undefined) return { link: { state: numbered }, said: `linked to ${around.slide(numbered)}` };
    // A state's id, as the source names it: it holds no dot, as a host does.
    const state = to.replace(/^#/, "");
    if (/^[a-z][a-z0-9_-]{0,63}$/.test(state)) return { link: { state }, said: `linked to ${around.slide(state)}` };
    if (/^(?:[\p{L}\p{N}-]+\.)+\p{L}{2,}(?::\d+)?(?:[/?#]\S*)?$/u.test(to)) return { link: { href: `https://${to}` }, said: `linked to https://${to}` };
    return undefined;
  }
  /** ⌘K: link the characters selected (PLAN 2.70), to what is asked for in a field over them, which
   * shows where they link to already, if they do: a web address, or a slide. One `style_text` of
   * `link`, one step to undo; the field left as it was keeps the link, and left empty takes it
   * away. */
  async function link(): Promise<boolean> {
    await idle();
    const now = open;
    const chosen = selection();
    if (!now || !chosen || !carets) {
      around.say("Select characters to link them");
      return false;
    }
    const { from, to } = caret();
    const was = linked(from, to);
    const said = was && "state" in was ? around.slide(was.state) : undefined;
    const initial = !was ? "" : "href" in was ? was.href : said && /^Slide \d/.test(said) ? said : `#${was.state}`;
    // Over the first line of the characters, else under the last, so they stay in sight.
    const lines = covered(carets, from, to);
    const [x, y] = lines[0] ?? [caretAt(carets, from).x, carets.lines[caretAt(carets, from).line]?.top ?? 0];
    const last = lines.at(-1);
    const [left, top] = onPage([x + (lines[0]?.[2] ?? 0) / 2, y]);
    const below = last ? onPage([last[0], last[1] + last[3]])[1] : undefined;
    asking = true;
    const words = await askWords([left, top], initial, `Where ${quote(chosen.text)} links to: a web address, or a slide by its number`, {
      caption: was ? "Links to" : "Link to",
      placeholder: "https://… or a slide's number",
      below,
      check: (w) => (target(w) ? undefined : `“${w.trim()}” is no link: a web address, or a slide's number`),
    }).finally(() => (asking = false));
    if (open !== now) return false;
    area.focus({ preventScroll: true });
    if (words === undefined) {
      around.say(was ? `${quote(chosen.text)} · link kept` : "No link made");
      return false;
    }
    if (was && words.trim() === initial) {
      around.say(`${quote(chosen.text)} · link kept`);
      return false;
    }
    const made = target(words);
    if (!made) {
      around.say(`“${words.trim()}” is no link: type a web address (https://…) or a slide's number`);
      return false;
    }
    if (made.link === null && !was) {
      around.say("No link made");
      return false;
    }
    return style({ link: made.link }, made.said);
  }

  /** Where canvas point `p` of the text typed in stands on the page, client pixels: through its
   * `transform`, and the view the preview shows (PLAN 2.46, 2.51). */
  function onPage([x, y]: [number, number]): [number, number] {
    const r = overlay.getBoundingClientRect();
    const [ox, oy] = around.origin();
    const u = around.unit();
    const m = open && around.map(open.node);
    const [cx, cy] = m ? through(m, [x, y]) : [x, y];
    return [r.left + (cx - ox) / u, r.top + (cy - oy) / u];
  }

  /** Stop typing: the node stays selected. Keys typed after a change still with the worker are
   * made once it is (`finish`). */
  /** The text last typed in, its words, and the selection in it as typing stopped. */
  let last: { node: string; state: string; value: string; from: number; to: number } | undefined;

  function leave() {
    if (!open) return;
    const was = open.node;
    last = { node: open.node, state: open.state, value: area.value, from: area.selectionStart, to: area.selectionEnd };
    if (sending && carets && area.value !== carets.text)
      finishing = { now: open, value: area.value, end: area.selectionEnd, bounds, text: carets.text, read };
    open = carets = read = undefined;
    on = goal = anchor = bounds = undefined;
    behind = false;
    area.hidden = true;
    area.blur();
    wake();
    tell();
    around.draw();
    around.left?.(was);
    // However typing stopped (Escape, a press elsewhere, the focus gone on), the status says so.
    if (typingSaid !== undefined) around.unsay?.(typingSaid, "Typing done");
    typingSaid = undefined;
  }

  /** Make what was typed since the last change one more, on the source as it stands: one at a
   * time, and what is typed meanwhile in the next. */
  async function send() {
    if (sending || !open || !carets) return;
    const source = around.source();
    if (source !== read) return sync();
    const was = carets.text;
    const change = changed(was, area.value, area.selectionEnd, bounds);
    if (!change) return;
    bounds = undefined;
    // A burst of typing is one step to undo, however long each change takes to make: its keys
    // follow one another by less than a pause.
    const batch = keys;
    keys = undefined;
    const joins = made !== undefined && batch !== undefined && batch.first - made < BURST;
    if (batch) made = batch.last;
    const now = open;
    const op = {
      op: "replace_text",
      node: now.node,
      state: now.state,
      from: points(was, change.from),
      to: points(was, change.to),
      text: change.text,
      ...(now.fork ? { fork: true } : {}),
    };
    sending = true;
    try {
      const typed = await stage.type(source, [op], { index: now.index, state: now.state, node: now.node }, around.format());
      ours = typed.source;
      around.typed(typed.source, typed.edited, joins);
      if (finishing?.now === now && typed.carets) [finishing.text, finishing.read] = [typed.carets.text, typed.source];
      if (open !== now) return;
      if (!typed.carets) return leave();
      carets = typed.carets;
      read = typed.source;
    } catch (e) {
      around.say(`Not typed: ${said(e)}`);
      // What the deck reads stands: the textarea goes back to it, and what was typed after the
      // change, typing left meanwhile, is not made either.
      if (finishing?.now === now) finishing = undefined;
      if (open === now && carets) {
        const at = Math.min(area.selectionStart, carets.text.length);
        area.value = carets.text;
        area.setSelectionRange(at, at);
      }
    } finally {
      sending = false;
      around.draw();
      settle();
    }
  }

  /** Typing in `now` was left while its text was read again, or a look or a list was given it,
   * with keys typed meanwhile (`finishing`): what was typed goes onto the text as the deck reads
   * it now, `text` from `source`, as the textarea's own would (`follow`), to be made next
   * (`finish`). With the text gone, or changed where it was typed, it is not made, and the status
   * says so. */
  function onto(now: NonNullable<typeof open>, text: string | undefined, source: string) {
    const left = finishing;
    if (left?.now !== now || left.text === undefined) return;
    if (text === undefined) {
      finishing = undefined;
      return around.say(`Not typed in ${around.name(now.node)}: the deck changed as typing ended`);
    }
    if (text !== left.text) {
      const next = follow(left.text, text, left.value, left.end, left.bounds);
      if (next.lost) {
        finishing = undefined;
        return around.say("Not typed: the text changed where it was typed");
      }
      [left.value, left.end, left.bounds] = [next.value, next.at, next.bounds];
    }
    [left.text, left.read] = [text, source];
  }

  /** What was typed when typing was left with a change still with the worker, made once that change
   * is, as part of the burst it ended (`leave`): one step to undo with it. Where the source changed
   * meanwhile, it is not made, and the status says so. */
  async function finish() {
    const left = finishing;
    finishing = undefined;
    if (!left) return;
    const { now, value, end, text, read: source } = left;
    if (text === undefined || source === undefined) return;
    const change = changed(text, value, end, left.bounds);
    if (!change) return;
    if (source !== around.source()) return around.say(`Not typed in ${around.name(now.node)}: the deck changed as typing ended`);
    const op = {
      op: "replace_text",
      node: now.node,
      state: now.state,
      from: points(text, change.from),
      to: points(text, change.to),
      text: change.text,
      ...(now.fork ? { fork: true } : {}),
    };
    sending = true;
    try {
      const typed = await stage.type(source, [op], { index: now.index, state: now.state, node: now.node }, around.format());
      // Part of the burst it ended only where the change before it was this text's own: typing
      // left while the text was read again, before anything typed was made, is a step of its own,
      // so an undo goes back to what was there, a save included.
      const joins = ours !== undefined && ours === source;
      ours = typed.source;
      around.typed(typed.source, typed.edited, joins);
    } catch (e) {
      around.say(`Not typed: ${said(e)}`);
    } finally {
      sending = false;
      around.draw();
      settle();
    }
  }

  /** Read the text again from the source as it stands: an undo, the source edited, or the deck
   * shown again. The textarea takes what the deck reads, with what was typed since it was last
   * read where that still fits (`follow`). */
  async function sync() {
    const now = open;
    if (!now || !carets) return;
    if (sending) {
      behind = true;
      return;
    }
    const shown = around.shown();
    if (!shown || shown.state !== now.state) return leave();
    const source = around.source();
    sending = true;
    const found = await stage.carets(source, now.state, now.node, around.format()).catch(() => null);
    sending = false;
    if (open !== now) {
      onto(now, found?.text, source);
      return settle();
    }
    if (!found) return leave();
    const was = carets.text;
    // Something else changed the source: what is typed next is a step to undo of its own.
    if (source !== read) made = undefined;
    carets = found;
    read = source;
    if (found.text !== was) {
      const next = follow(was, found.text, area.value, area.selectionEnd, bounds);
      if (next.lost) around.say("Not typed: the text changed where it was typed");
      area.value = next.value;
      area.setSelectionRange(next.at, next.at);
      bounds = next.bounds;
      on = goal = undefined;
    }
    around.draw();
    settle();
  }

  /** A change made, or the text read: what was typed meanwhile goes in, the text is read again
   * if it was asked to be, and what waits for typing to settle goes on. */
  function settle() {
    if (open && carets && area.value !== carets.text) void send();
    else if (finishing && !sending) void finish();
    else if (behind) {
      behind = false;
      void sync();
    }
    wake();
  }

  /** Undo, or redo, once what was typed is made, so that none is made after it; then the text
   * is read where the deck now reads it, the caret where it changed. */
  function history(redo: boolean) {
    void idle().then(() => {
      if (redo) around.redo();
      else around.undo();
      made = undefined;
      return sync();
    });
  }

  /** Up or down a line (`by`), or to its start or end (`to`), by the engine's lines; with `extend`,
   * the selection's head moves and its anchor stays. */
  function move(e: KeyboardEvent, by: -1 | 1 | 0, to?: "start" | "end") {
    if (!carets) return;
    e.preventDefault();
    const { from, to: end, head } = caret();
    const tail = head === from ? end : from;
    const here = caretAt(carets, head, on);
    let next: number;
    let line = here.line;
    if (to) {
      next = to === "start" ? carets.lines[line].start : lastOf(carets.lines[line]);
      goal = undefined;
    } else {
      goal ??= here.x;
      line += by;
      if (line < 0) [next, line] = [0, 0];
      else if (line >= carets.lines.length) [next, line] = [carets.text.length, carets.lines.length - 1];
      else next = onLine(carets, line, goal);
    }
    if (e.shiftKey) put(Math.min(tail, next), Math.max(tail, next), line, next < tail);
    else put(next, next, line);
  }

  /** What was pressed in code while the engine answered a key before it (PLAN 2.139): taken in
   * order once it has, so a burst of keys types as it was pressed. */
  const queued: ({ key: CodeKey } | { text: string } | { back: boolean } | { done: true })[] = [];
  /** A code key is with the engine, and what is pressed waits behind it. */
  let keying = false;

  /** `text` typed in place of the selection, as the field types it. */
  function insert(text: string) {
    const [start, end] = [area.selectionStart, area.selectionEnd];
    const fromEnd = area.value.length - end;
    bounds = bounds ? { start: Math.min(bounds.start, start), fromEnd: Math.min(bounds.fromEnd, fromEnd) } : { start, fromEnd };
    area.setRangeText(text, start, end, "end");
    area.dispatchEvent(new InputEvent("input", { inputType: "insertText", data: text }));
  }

  /** The selection taken away, or the character before the caret (`back`) or after it. */
  function erase(back: boolean) {
    let [start, end] = [area.selectionStart, area.selectionEnd];
    if (start === end) {
      // A character as the string holds it: a surrogate pair is one.
      const pair = (at: number) => /[\uD800-\uDBFF]/.test(area.value[at] ?? "") && /[\uDC00-\uDFFF]/.test(area.value[at + 1] ?? "");
      if (back) start = Math.max(0, start - (start >= 2 && pair(start - 2) ? 2 : 1));
      else end = Math.min(area.value.length, end + (pair(end) ? 2 : 1));
    }
    if (start === end) return;
    area.setSelectionRange(start, end);
    insert("");
  }

  /** A key in code (PLAN 2.139), as the engine answers it (`code::keyed`): the text and selection
   * it gives, typed as any change is. */
  async function codeKey(key: CodeKey, code: { tab: number; language?: string }) {
    const before = area.value;
    const [start, end] = [area.selectionStart, area.selectionEnd];
    const keyed = await stage.keyed(before, start, end, key, code.tab, code.language);
    if (!keyed || area.value !== before) return;
    let head = 0;
    while (head < before.length && head < keyed.text.length && before[head] === keyed.text[head]) head++;
    let tail = 0;
    while (
      tail < before.length - head &&
      tail < keyed.text.length - head &&
      before[before.length - 1 - tail] === keyed.text[keyed.text.length - 1 - tail]
    )
      tail++;
    bounds = bounds ? { start: Math.min(bounds.start, head), fromEnd: Math.min(bounds.fromEnd, tail) } : { start: head, fromEnd: tail };
    area.setRangeText(keyed.text.slice(head, keyed.text.length - tail), head, before.length - tail);
    area.setSelectionRange(keyed.from, keyed.to);
    area.dispatchEvent(new InputEvent("input", { inputType: "insertText", data: null }));
  }

  /** What was queued behind a code key, taken in order. */
  async function keyOn() {
    if (keying) return;
    keying = true;
    try {
      for (let next = queued.shift(); next; next = queued.shift()) {
        if ("key" in next) await codeKey(next.key, carets?.code ?? { tab: 4, language: (open && around.code?.(open.node)) || undefined });
        else if ("text" in next) insert(next.text);
        else if ("back" in next) erase(next.back);
        else {
          queued.length = 0;
          leave();
          around.say("Typing done");
        }
      }
    } finally {
      keying = false;
    }
  }

  area.addEventListener("beforeinput", (e) => {
    // The browser's own undo (its Edit menu) is the deck's.
    if (e.inputType === "historyUndo" || e.inputType === "historyRedo") {
      e.preventDefault();
      return history(e.inputType === "historyRedo");
    }
    // Behind a code key the engine is answering, what is typed waits its turn.
    if (keying && !e.isComposing) {
      e.preventDefault();
      if (e.inputType === "deleteContentBackward" || e.inputType === "deleteContentForward") {
        queued.push({ back: e.inputType === "deleteContentBackward" });
      } else if (e.data) queued.push({ text: e.data });
      return;
    }
    const [start, end] = [area.selectionStart, area.selectionEnd];
    const fromEnd = area.value.length - end;
    bounds = bounds ? { start: Math.min(bounds.start, start), fromEnd: Math.min(bounds.fromEnd, fromEnd) } : { start, fromEnd };
  });
  // Code pasted into code (PLAN 2.133): tidied as a paste of code is, the indentation its lines
  // share taken off, then each line after the first indented as the line it is pasted into is.
  area.addEventListener("paste", (e) => {
    const language = open && around.code?.(open.node);
    const words = e.clipboardData?.getData("text/plain");
    if (!language || !words) return;
    e.preventDefault();
    const [start, end] = [area.selectionStart, area.selectionEnd];
    void stage.tidied(words, language).then((tidied) => {
      if (area.selectionStart !== start || area.selectionEnd !== end) return;
      const line = area.value.slice(area.value.lastIndexOf("\n", start - 1) + 1, start);
      const indent = /^[ \t]*/.exec(line)?.[0] ?? "";
      const text = tidied.replace(/\n/g, `\n${indent}`);
      const fromEnd = area.value.length - end;
      bounds = bounds ? { start: Math.min(bounds.start, start), fromEnd: Math.min(bounds.fromEnd, fromEnd) } : { start, fromEnd };
      area.setRangeText(text, start, end, "end");
      area.dispatchEvent(new InputEvent("input", { inputType: "insertFromPaste", data: text }));
    });
  });
  area.addEventListener("input", (e) => {
    keys = keys ? { ...keys, last: e.timeStamp } : { first: e.timeStamp, last: e.timeStamp };
    on = goal = undefined;
    around.draw();
    void send();
  });
  area.addEventListener("keydown", (e) => {
    const mod = e.metaKey || e.ctrlKey;
    const key = e.key.toLowerCase();
    // The canvas's keys are not the text's, but for its zoom, the deck's find, and Save, which
    // no text takes (PLAN 2.46, 2.47): Save waits for what was typed to be made.
    if (mod && !e.altKey && ["=", "+", "-", "_", "0", "f", "s"].includes(key)) return;
    e.stopPropagation();
    if (mod && (key === "z" || key === "y")) {
      e.preventDefault();
      return history(key === "y" || e.shiftKey);
    }
    if (e.key === "Escape") {
      e.preventDefault();
      // Behind a code key the engine is answering, typing ends once what was pressed is typed.
      if (keying) return void queued.push({ done: true });
      leave();
      return around.say("Typing done");
    }
    // Code (PLAN 2.139): set as written, so no characters of it take a look, a link, or a list's
    // marks; Tab, Shift+Tab, and Enter are a code editor's, as the engine answers them.
    const named = open && around.code?.(open.node);
    const code = carets?.code ?? (named ? { tab: 4, from: 1, language: named } : undefined);
    if (code && mod && !e.altKey && (["b", "i", "k"].includes(key) || (e.shiftKey && ["Digit8", "Digit7"].includes(e.code)))) {
      e.preventDefault();
      return around.say("Code is set as written: none of it takes a look, a link, or a list of its own");
    }
    if (code && (e.key === "Tab" || e.key === "Enter") && !mod && !e.altKey && !e.isComposing) {
      e.preventDefault();
      queued.push({ key: e.key === "Enter" ? "Return" : e.shiftKey ? "Shift+Tab" : "Tab" });
      return void keyOn();
    }
    if (mod && key === "b" && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      return void bold();
    }
    if (mod && key === "i" && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      return void italic();
    }
    // ⌘K links the characters selected (PLAN 2.70); outside a text, it is the palette's.
    if (mod && key === "k" && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      return void link();
    }
    // ⌘⇧8 bullets, ⌘⇧7 numbers, by the keys' places, as Shift makes them other characters.
    if (mod && e.shiftKey && !e.altKey && (e.code === "Digit8" || e.code === "Digit7")) {
      e.preventDefault();
      return void toggle(e.code === "Digit8" ? "bullet" : "number");
    }
    if (e.isComposing) return;
    // In a list (ADR-0018): Tab and Shift+Tab move the items selected a level; Enter in an empty
    // item ends the list there; Backspace at an item's start takes its marker off, a level out
    // first, and only then joins it to the paragraph before. Tab in a paragraph that is no item
    // makes it one a level in, as a presentation app's text box does. Elsewhere they are the
    // field's.
    if (["Tab", "Enter", "Backspace"].includes(e.key) && !mod && !e.altKey && carets && area.value === carets.text) {
      const { from, to } = caret();
      const [first] = touched(carets.text, from, to);
      const item = carets.items?.[first];
      if (e.key === "Tab" && (item || !e.shiftKey)) {
        e.preventDefault();
        return void list({ by: e.shiftKey ? -1 : 1 }, e.shiftKey ? "a level out" : "a level in");
      }
      const [start, end] = paragraphs(carets.text)[first] ?? [0, 0];
      if (item && e.key === "Enter" && !e.shiftKey && start === end && from === to) {
        e.preventDefault();
        return void list({ kind: "none" }, "the list ends");
      }
      if (item && e.key === "Backspace" && !e.shiftKey && from === to && from === start) {
        e.preventDefault();
        return void (item.level > 0 ? list({ by: -1 }, "a level out") : list({ kind: "none" }, "out of the list"));
      }
    }
    const mac = /Mac|iPhone|iPad/.test(navigator.platform);
    if (e.key === "ArrowUp" && !mod && !e.altKey) return move(e, -1);
    if (e.key === "ArrowDown" && !mod && !e.altKey) return move(e, 1);
    if ((e.key === "Home" && !mod) || (mac && e.metaKey && e.key === "ArrowLeft")) return move(e, 0, "start");
    if ((e.key === "End" && !mod) || (mac && e.metaKey && e.key === "ArrowRight")) return move(e, 0, "end");
    if (e.key.startsWith("Arrow") || e.key === "PageUp" || e.key === "PageDown") goal = on = undefined;
  });
  area.addEventListener("keyup", () => around.draw());
  /** Whether focus at `at` keeps typing on: the text, the canvas, or the inspector. */
  const keeping = (at: EventTarget | null) => asking || at === area || at === overlay || around.keeps(at);
  area.addEventListener("blur", () => {
    // Focus gone elsewhere (the source, another control) ends typing. On the canvas itself, its
    // pointer says: in the text, typing goes on; outside it, it ends.
    setTimeout(() => {
      if (open && !keeping(document.activeElement)) leave();
    });
  });
  // From the inspector, focus may go on elsewhere without the text's losing it.
  const wander = (e: FocusEvent) => {
    if (open && !keeping(e.target)) leave();
  };
  listen(document, "focusin", wander);
  // The canvas focused while a text is typed in, by the keyboard or the page rather than a press
  // on it (which puts the caret in the text, or ends typing): the text takes the focus, and the
  // keys with it.
  const onward = () => {
    if (open && document.activeElement === overlay) area.focus({ preventScroll: true });
  };
  listen(overlay, "focus", onward);
  const selecting = () => {
    if (!open || document.activeElement !== area) return;
    around.draw();
    tell();
  };
  listen(document, "selectionchange", selecting);

  return {
    enter,
    leave,
    sync,
    /** Once every change typed so far is made, typing left with one under way among them: what a
     * save waits for, so it holds what was typed. */
    idle,
    /** Whether what was typed is not yet all made. */
    busy: () => !settled() || finishing !== undefined || keying,
    style,
    bold,
    italic,
    /** Bullets or numbers on the paragraphs the selection touches, as ⌘⇧8 and ⌘⇧7 do. */
    toggle,
    /** Link the characters selected, as ⌘K does (PLAN 2.70). */
    link,
    /** The characters selected in the text typed in, if any are. */
    selection,
    /** Whether a text is typed in, and which. */
    node: () => open?.node,
    /** Where the caret was in the text last typed in, as typing stopped (UTF-16): what Focus on
     * These Lines reads once the keys have left the text (PLAN 2.140). */
    last: () => last,
    /** What is typed, for a test: the text, the caret, the lines the engine set it in, and
     * whether a change is with the worker. */
    now: () =>
      open && {
        node: open.node,
        value: area.value,
        from: area.selectionStart,
        to: area.selectionEnd,
        lines: carets?.lines.length ?? 0,
        line: carets ? caretAt(carets, caret().head, on).line : 0,
        sending,
      },
    /** A press at `point`, in the text typed in: the caret goes there, or with `extend` the
     * selection reaches it, and a drag from there selects. False outside the text. */
    down(given: [number, number], extend: boolean, clicks: number): boolean {
      const box = open && around.box(open.node);
      if (!open || !carets || !box) return false;
      const point = back(around.map(open.node), given);
      const [x, y, w, h] = box;
      const slop = 4 * around.unit();
      if (point[0] < x - slop || point[0] > x + w + slop || point[1] < y - slop || point[1] > y + h + slop) return false;
      const [at, line] = caretNear(carets, point);
      goal = anchor = undefined;
      if (clicks >= 3) {
        const l = carets.lines[line];
        put(l.start, lastOf(l), line);
      } else if (clicks === 2) {
        // A double click selects the word there.
        const words = new Intl.Segmenter(undefined, { granularity: "word" }).segment(carets.text);
        const word = words.containing(Math.min(at, Math.max(0, carets.text.length - 1)));
        if (word) put(word.index, word.index + word.segment.length, line);
      } else {
        // A drag from here selects from where the selection holds still: the press, or with
        // `extend`, the end of the selection the head is not at.
        const { from, to, head } = caret();
        anchor = extend ? (head === from ? to : from) : at;
        put(Math.min(anchor, at), Math.max(anchor, at), line, at < anchor);
      }
      area.focus({ preventScroll: true });
      return true;
    },
    /** The pointer moved with a press held: the selection reaches it. False unless selecting. */
    drag(given: [number, number]): boolean {
      if (!carets || anchor === undefined || !open) return false;
      const [at, line] = caretNear(carets, back(around.map(open.node), given));
      put(Math.min(anchor, at), Math.max(anchor, at), line, at < anchor);
      return true;
    },
    /** The press is let go. False unless selecting. */
    up(): boolean {
      if (anchor === undefined) return false;
      anchor = undefined;
      return true;
    },
    /** The caret, or the selection, as SVG in canvas units, `u` of them to a CSS pixel; and the
     * textarea moved under the caret, where an input method shows what it composes. */
    parts(u: number): string[] {
      if (!open || !carets) return [];
      const { from, to, head } = caret();
      const here = caretAt(carets, head, on);
      const line = carets.lines[here.line];
      const r = overlay.getBoundingClientRect();
      const [ox, oy] = around.origin();
      // A text its transform turns or scales is drawn through it, and so is its caret (PLAN 2.51).
      const m = around.map(open.node);
      const [cx, cy] = m ? through(m, [here.x, line.bottom]) : [here.x, line.bottom];
      area.style.left = `${Math.max(0, Math.min(r.width - 1, (cx - ox) / u))}px`;
      area.style.top = `${Math.max(0, Math.min(r.height - 1, (cy - oy) / u))}px`;
      const drawn = (parts: string[]) => (m ? [`<g transform="matrix(${m.join(" ")})">${parts.join("")}</g>`] : parts);
      if (from !== to) {
        return drawn(covered(carets, from, to).map(([x, y, w, h]) => `<rect class="text-selection" x="${x}" y="${y}" width="${w}" height="${h}"/>`));
      }
      const width = 2 * u;
      return drawn([`<rect class="caret" x="${here.x - width / 2}" y="${line.top}" width="${width}" height="${line.bottom - line.top}"/>`]);
    },
    /** Let the textarea go: the canvas is closed. */
    close() {
      leave();
      document.removeEventListener("selectionchange", selecting);
      document.removeEventListener("focusin", wander);
      overlay.removeEventListener("focus", onward);
      area.remove();
    },
  };
}
