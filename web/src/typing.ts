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
import type { CaretLine, Carets, Edited, ListMark, Map6, Rect } from "./protocol";
import { askWords } from "./notes";
import type { Stage } from "./stage";

/** What a text typed in answers that no command runs by name (PLAN 2.65), as the keys sheet lists
 * it; the rest are the keys of any field typed in. */
export const typingKeys = (): Key[] => [
  { keys: `${MOD}B`, label: "In a text typed in: the characters selected bold, or not", group: "Type" },
  { keys: `${MOD}I`, label: "In italic, or not", group: "Type" },
  { keys: "↑ ↓ Home End", label: "Go up or down a line, or to its start or end, as the text is set", group: "Type" },
  { keys: "Enter", label: "In a list: a new item like it; in an empty item, the list ends", group: "Type" },
  { keys: `Tab, ${SHIFT}Tab`, label: "In a list: the items selected a level in, or out", group: "Type" },
  { keys: `${MOD}K`, label: "In a text typed in: link the characters selected to a web address or a state", group: "Type" },
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
 * them (`scaena_core::lists::paragraphs`): `\n`, `\r`, `\r\n`, U+2028, and U+2029 end one. */
export function paragraphs(text: string): [number, number][] {
  const out: [number, number][] = [];
  let start = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c !== "\n" && c !== "\r" && c !== "\u2028" && c !== "\u2029") continue;
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
  /** Typing left while a change was with the worker, straight after keys (Escape, or a press
   * elsewhere): the text as it was typed then, and the text and source that change came to, once
   * it is made. What was typed after it is made then (`finish`), so leaving loses no key. */
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
    leave();
    open = { node, state: shown.state, index: shown.index, fork };
    carets = found;
    read = source;
    keys = made = undefined;
    area.hidden = false;
    area.value = found.text;
    area.setAttribute("aria-label", `The text of ${node}, typed where it stands`);
    area.focus({ preventScroll: true });
    if (point) {
      const [at, line] = caretNear(found, back(around.map(node), point));
      put(at, at, line);
    } else put(all ? 0 : found.text.length, found.text.length);
    void tellWhere();
    return true;
  }

  /** Say what typing changes: the states it reaches. */
  async function tellWhere() {
    const now = open;
    if (!now) return;
    const op = { op: "replace_text", node: now.node, state: now.state, from: 0, to: 0, text: "·", ...(now.fork ? { fork: true } : {}) };
    const states = await stage.reach([op]).catch(() => undefined);
    if (open !== now || !states) return;
    const n = states.length;
    const where = now.fork ? `kept to ${now.state}` : n === 1 && states[0] === now.state ? "in this state" : `in ${n} states`;
    around.say(`typing in ${now.node} · ${where} · Escape leaves it${now.fork || n < 2 ? "" : `; Alt and a double click keep it to ${now.state}`}`);
  }

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
      around.say("select characters in a text to give them a look");
      return false;
    }
    const op = { op: "style_text", node: now.node, state: now.state, from: chosen.from, to: chosen.to, look, ...(now.fork ? { fork: true } : {}) };
    sending = true;
    try {
      const typed = await stage.type(around.source(), [op], { index: now.index, state: now.state, node: now.node }, around.format());
      around.typed(typed.source, typed.edited, false);
      made = undefined;
      if (open === now) {
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
      const what = words ?? Object.entries(look).map(([k, v]) => (v === null ? `${k} taken away` : `${k} ${typeof v === "string" ? v : JSON.stringify(v)}`)).join(", ");
      around.say(`${now.node}, characters ${chosen.from + 1}–${chosen.to}: ${what}`);
      return true;
    } catch (e) {
      around.say(`no look given: ${said(e)}`);
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
      around.say("select characters to make them bold");
      return false;
    }
    let look: Record<string, unknown>;
    try {
      look = await stage.bolding(around.source(), now.state, now.node, chosen.from, chosen.to, around.format());
    } catch (e) {
      around.say(`not bold: ${said(e)}`);
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
      around.say("select characters to set them in italic");
      return false;
    }
    let look: Record<string, unknown>;
    try {
      look = await stage.italicizing(around.source(), now.state, now.node, chosen.from, chosen.to, around.format());
    } catch (e) {
      around.say(`not italic: ${said(e)}`);
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
      if (open === now) {
        if (!typed.carets) leave();
        else [carets, read] = [typed.carets, typed.source];
      }
      around.say(`${now.node}: ${done}`);
      return true;
    } catch (e) {
      around.say(`not listed: ${said(e)}`);
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
  /** ⌘K: link the characters selected (PLAN 2.70), to what is asked for in a field over them: a
   * web address (`https://`, `http://`, `mailto:`), or a state's id (`#` before it, or not).
   * Nothing takes the link away. One `style_text` of `link`, one step to undo. */
  async function link(): Promise<boolean> {
    await idle();
    const now = open;
    const chosen = selection();
    if (!now || !chosen) {
      around.say("select characters to link them");
      return false;
    }
    const r = area.getBoundingClientRect();
    asking = true;
    const words = await askWords([r.left, r.top], "", `Where ${chosen.text} links to: a web address, or a state's id`).finally(
      () => (asking = false),
    );
    if (open !== now) return false;
    area.focus({ preventScroll: true });
    if (words === undefined) {
      around.say("no link made");
      return false;
    }
    const to = words.trim();
    if (!to) return style({ link: null }, "the link taken away");
    if (/^(https?:\/\/|mailto:)\S+$/.test(to)) return style({ link: { href: to } }, `linked to ${to}`);
    const state = to.replace(/^#/, "");
    if (/^[a-z][a-z0-9_-]{0,63}$/.test(state)) return style({ link: { state } }, `linked to the state ${state}`);
    around.say(`${to} is no link: a web address (https://…, mailto:…) or a state's id`);
    return false;
  }

  /** Stop typing: the node stays selected. Keys typed after a change still with the worker are
   * made once it is (`finish`). */
  function leave() {
    if (!open) return;
    if (sending && carets && area.value !== carets.text) finishing = { now: open, value: area.value, end: area.selectionEnd, bounds };
    open = carets = read = undefined;
    on = goal = anchor = bounds = undefined;
    behind = false;
    area.hidden = true;
    area.blur();
    wake();
    tell();
    around.draw();
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
      around.typed(typed.source, typed.edited, joins);
      if (finishing?.now === now && typed.carets) [finishing.text, finishing.read] = [typed.carets.text, typed.source];
      if (open !== now) return;
      if (!typed.carets) return leave();
      carets = typed.carets;
      read = typed.source;
    } catch (e) {
      around.say(`not typed: ${said(e)}`);
      // What the deck reads stands: the textarea goes back to it.
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
    if (source !== around.source()) return around.say(`not typed in ${now.node}: the source changed as typing ended`);
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
      around.typed(typed.source, typed.edited, true);
    } catch (e) {
      around.say(`not typed: ${said(e)}`);
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
    if (open !== now) return settle();
    if (!found) return leave();
    const was = carets.text;
    // Something else changed the source: what is typed next is a step to undo of its own.
    if (source !== read) made = undefined;
    carets = found;
    read = source;
    if (found.text !== was) {
      const next = follow(was, found.text, area.value, area.selectionEnd, bounds);
      if (next.lost) around.say("not typed: the text changed where it was typed");
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

  area.addEventListener("beforeinput", (e) => {
    // The browser's own undo (its Edit menu) is the deck's.
    if (e.inputType === "historyUndo" || e.inputType === "historyRedo") {
      e.preventDefault();
      return history(e.inputType === "historyRedo");
    }
    const [start, end] = [area.selectionStart, area.selectionEnd];
    const fromEnd = area.value.length - end;
    bounds = bounds ? { start: Math.min(bounds.start, start), fromEnd: Math.min(bounds.fromEnd, fromEnd) } : { start, fromEnd };
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
    // The canvas's keys are not the text's, but for its zoom and the deck's find, which no text
    // takes (PLAN 2.46, 2.47).
    if (mod && !e.altKey && ["=", "+", "-", "_", "0", "f"].includes(key)) return;
    e.stopPropagation();
    if (mod && (key === "z" || key === "y")) {
      e.preventDefault();
      return history(key === "y" || e.shiftKey);
    }
    if (e.key === "Escape") {
      e.preventDefault();
      leave();
      return around.say("typing done");
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
    // item ends the list there. Elsewhere they are the field's.
    if ((e.key === "Tab" || e.key === "Enter") && !mod && !e.altKey && carets && area.value === carets.text) {
      const [first] = touched(carets.text, caret().from, caret().to);
      const item = carets.items?.[first];
      if (item && e.key === "Tab") {
        e.preventDefault();
        return void list({ by: e.shiftKey ? -1 : 1 }, e.shiftKey ? "a level out" : "a level in");
      }
      const [start, end] = paragraphs(carets.text)[first] ?? [0, 0];
      if (item && e.key === "Enter" && !e.shiftKey && start === end && caret().from === caret().to) {
        e.preventDefault();
        return void list({ kind: "none" }, "the list ends");
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
  document.addEventListener("focusin", wander);
  // The canvas focused while a text is typed in, by the keyboard or the page rather than a press
  // on it (which puts the caret in the text, or ends typing): the text takes the focus, and the
  // keys with it.
  const onward = () => {
    if (open && document.activeElement === overlay) area.focus({ preventScroll: true });
  };
  overlay.addEventListener("focus", onward);
  const selecting = () => {
    if (!open || document.activeElement !== area) return;
    around.draw();
    tell();
  };
  document.addEventListener("selectionchange", selecting);

  return {
    enter,
    leave,
    sync,
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
