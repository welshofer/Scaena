// The editor's words (PLAN 2.99): what a person reads where the deck's format has names of its
// own, as the Mac's window says them (PLAN 3.18). The engine says them (`scaena_core::words`): each
// slot of the timeline its slide's number (`Slot.said`), each choice its label and its values
// (`Field.words`), and each state its objects by what they show and the theme's names said
// (`Player.words`, a `Lexicon`). What is here only puts those words together; a time is said in
// seconds, as `scaena_core::words::seconds` says it.
import type { Insert, Lexicon } from "./protocol";

/** Milliseconds said in seconds, to two places at most: `5.5 s`, `0.18 s`. */
export const seconds = (ms: number) => `${Number((ms / 1000).toFixed(2))} s`;

/** `n` of `what`, made plural where it is not one. */
export const counted = (n: number, what: string) => `${n} ${what}${n === 1 ? "" : "s"}`;

/** An object as a person names it, by what it shows (`words.objects`), else its kind; `quoted`
 * sets its words in quotation marks, as a sentence names it, and its kind in lower case after
 * "the": “Revenue doubled”, the chart. */
export function object(words: Lexicon | undefined, node: string, quoted = false): string {
  const said = words?.objects[node];
  const kind = words?.kinds[node];
  if (said === undefined) return quoted ? "it" : "Object";
  if (!quoted) return said;
  return said === kind ? `the ${kind.toLowerCase()}` : `“${said}”`;
}

/** Words a person wrote, as a sentence quotes them: their first line, cut at 40 characters, as
 * `scaena_core::words::object` names a text by its words. */
export function quote(words: string): string {
  const line = (words.trim().split("\n")[0] ?? "").trim();
  const chars = [...line];
  return `“${chars.length > 40 ? `${chars.slice(0, 39).join("").trimEnd()}…` : line}”`;
}

/** A finding as a person says it (PLAN 2.99): what it is about, then what it finds, in a sentence
 * (`said`, else the lexicon's for its code): “Revenue doubled”: The text doesn't fit its box. The
 * theme, where it is about the theme's file; the deck, where it names no object. */
export function finding(
  words: Lexicon | undefined,
  f: { code: string; message: string; node?: string; file?: string; said?: string },
): string {
  const said = f.said ?? words?.issues[f.code] ?? f.message;
  const about = f.node !== undefined && words?.objects[f.node] !== undefined ? sentence(object(words, f.node, true)) : f.file ? "The theme" : "The deck";
  return `${about}: ${said}`;
}

/** Several findings said, each once, as `finding` says one: the first `most`, then how many more. */
export function findings(words: Lexicon | undefined, all: Parameters<typeof finding>[1][], most = 2): string {
  const said = [...new Set(all.map((f) => finding(words, f)))];
  const more = said.length > most ? ` (and ${said.length - most} more)` : "";
  return `${said.slice(0, most).join(" ")}${more}`;
}

/** A name as the format writes it, in words, as `scaena_core::words::phrase` says it: `onSurface`
 * is On Surface, and `big-number` Big Number. */
export function phrase(name: string): string {
  const words: string[] = [];
  let word = "";
  for (const ch of name) {
    if ("-_. ".includes(ch)) {
      if (word) words.push(word);
      word = "";
    } else if (ch !== ch.toLowerCase() && /[\p{Ll}\p{N}]$/u.test(word)) {
      words.push(word);
      word = ch;
    } else word += ch;
  }
  if (word) words.push(word);
  return words.map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(" ");
}

/** One of the theme's names of a kind said: `onSurface` (a color) is Text; one the words lack, as
 * the format writes it in words. */
export const named = (words: Lexicon | undefined, kind: string, name: string) => words?.names[kind]?.[name] ?? phrase(name);

/** What a motion does, and what it moves one at a time: Enters Word by Word. */
export function moving(words: Lexicon | undefined, motion: string, split?: string | null): string {
  const does = words?.motions[motion] ?? motion;
  return split ? `${does} ${words?.splits[split] ?? split}` : does;
}

/** Where slot `index` stands among `slots`, as a player says it: `Slide 3 of 12`, or `Slide 3 of
 * 12, step 2` for a step after a slide's first (PLAN 2.99). */
export function place(slots: { state: string; slide?: string; said?: string }[], index: number): string {
  const said = slots[index]?.said ?? `${index + 1}`;
  const slides = new Set(slots.map((s) => s.slide ?? s.state)).size;
  return said.replace(/^Slide (\d+)/, `Slide $1 of ${slides}`);
}

/** `words` begun as a sentence begins: its first letter a capital. */
export const sentence = (words: string) => words.charAt(0).toUpperCase() + words.slice(1);

/** Each part of an image a fraction gives, said. */
export const PARTS: Record<string, string> = { x: "across", y: "down", w: "width", h: "height" };

/** An image's crop or focal point as a person says it, in shares of the picture: 80% wide, 100%
 * high, from 10% across and 0% down; 30% across, 60% down. */
export function fractions(names: string[], v: number[]): string {
  const pct = (k: number) => `${Math.round((v[k] ?? 0) * 100)}%`;
  if (names.join() === "x,y,w,h") return `${pct(2)} wide, ${pct(3)} high, from ${pct(0)} across and ${pct(1)} down`;
  return names.map((n, k) => `${pct(k)} ${PARTS[n] ?? n}`).join(", ");
}

/** The state a slide's number names, as a person types it (PLAN 2.99): `3`, `slide 3`, or `3, step
 * 2`, each slot of the timeline `said` as `Slide 3` or `Slide 3, step 2`. None for words that name
 * no slide there, as the Mac's link question reads them (`Slots.state(numbered:)`). */
export function numbered(slots: { state: string; said?: string }[], typed: string): string | undefined {
  const m = /^\s*(?:slide\s*)?(\d+)\s*(?:,?\s*step\s*(\d+))?\s*$/i.exec(typed);
  if (!m) return undefined;
  const [slide, step] = [Number(m[1]), Number(m[2] ?? 1)];
  const said = step > 1 ? `Slide ${slide}, step ${step}` : `Slide ${slide}`;
  return slots.find((s) => s.said === said)?.state;
}

/** The slides `states` are, as a person counts them (PLAN 2.99): this slide, where `shown` alone;
 * a slide by its number; 3 slides; 2 steps of Slide 4; or 3 slides, 5 steps in all. */
export function reaching(states: string[], shown: string, slide: (state: string) => string): string {
  if (states.length === 1) return states[0] === shown ? "on this slide" : `on ${slide(states[0])}`;
  const slides = new Set(states.map((s) => slide(s).replace(/, step \d+$/, "")));
  if (slides.size === states.length) return `on ${counted(slides.size, "slide")}`;
  if (slides.size === 1) return `on ${states.length} steps of ${[...slides][0]}`;
  return `on ${counted(slides.size, "slide")}, ${states.length} steps in all`;
}

/** What Insert offers, as a person names it (PLAN 2.99): a text by its role, a shape by its kind,
 * an effect by the theme's preset, and a picture or a source's chart by its name. */
export function inserting(words: Lexicon | undefined, insert: Insert): { kind: string; said: string } {
  const name = insert.label.includes(" · ") ? insert.label.split(" · ", 2)[1] : insert.label;
  const type = insert.node.type;
  const kind = words?.types[type] ?? type;
  if (type === "text") return { kind, said: named(words, "text-role", name) };
  if (type === "shape") return { kind, said: words?.shapes[name] ?? name };
  if (type === "shader") return { kind, said: named(words, "shader-preset", name) };
  return { kind, said: name };
}
