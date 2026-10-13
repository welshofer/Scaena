// The editor's words (PLAN 2.99): what a person reads where the deck's format has names of its
// own, as the Mac's window says them (PLAN 3.18). The engine says them (`scaena_core::words`): each
// slot of the timeline its slide's number (`Slot.said`), each choice its label and its values
// (`Field.words`), and each state its objects by what they show and the theme's names said
// (`Player.words`, a `Lexicon`). What is here only puts those words together; a time is said in
// seconds, as `scaena_core::words::seconds` says it.
import type { Lexicon } from "./protocol";

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

/** One of the theme's names of a kind said: `onSurface` (a color) is Text. */
export const named = (words: Lexicon | undefined, kind: string, name: string) => words?.names[kind]?.[name] ?? name;

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
