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

/** Data source `name` as a person names it (PLAN 3.18), never by the id the deck writes it with,
 * as `scaena_core::words::source` says it: by the name of its `file` in words,
 * `data/trails-funding.csv` as Trails Funding; rows written in the deck, by its own name in words. */
export function source(name: string, file?: string): string {
  if (file) {
    const leaf = file.split("/").pop() ?? file;
    const dot = leaf.lastIndexOf(".");
    const said = phrase(dot < 0 ? leaf : leaf.slice(0, dot));
    if (said) return said;
  }
  return phrase(name);
}

/** One of the deck's data sources as a person names it: as the lexicon says it (`words.sources`),
 * by its file; one the words lack, by its own name in words. */
export const sourced = (words: Lexicon | undefined, name: string) => words?.sources?.[name] ?? source(name);

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

/** `words` after the article it takes, as a vowel's sound asks: a rectangle, an oval, an arrow. */
export const an = (words: string) => `${/^[aeiou]/i.test(words) ? "an" : "a"} ${words}`;

/** `words` begun as a sentence begins: its first letter a capital. */
export const sentence = (words: string) => words.charAt(0).toUpperCase() + words.slice(1);

/** `words` as a whole sentence: begun with a capital, and ended with a stop where it has none; with
 * no words, what to do, as the Mac's `Why.sentence` says it. */
export function ended(words: string): string {
  const said = sentence(words.trim());
  if (!said) return "Something went wrong. Try again.";
  return /[.!?…]$/.test(said) ? said : `${said}.`;
}

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
 * an effect by the theme's preset, a source's chart or table by the source's file, as the Mac's
 * Insert menu names it (PLAN 3.18), and a picture by its name. */
export function inserting(words: Lexicon | undefined, insert: Insert): { kind: string; said: string } {
  const name = insert.label.includes(" · ") ? insert.label.split(" · ", 2)[1] : insert.label;
  const type = insert.node.type;
  const kind = words?.types[type] ?? type;
  if (type === "text") return { kind, said: named(words, "text-role", name) };
  if (type === "shape") return { kind, said: words?.shapes[name] ?? name };
  if (type === "shader") return { kind, said: named(words, "shader-preset", name) };
  // A chart or a table of a few sample rows, where the deck has no data, says so (the QA pass's J3).
  if ((type === "chart" || type === "table") && insert.sample) return { kind, said: `${kind} (sample data)` };
  if (type === "chart" || type === "table") return { kind, said: sourced(words, name) };
  return { kind, said: name };
}

/** What an answer that is no answer means for the person who asked (PLAN 3.18), as the Mac's
 * `Why.Answer` reads it: `status` 0 where none came. A key not taken is said by its status alone;
 * else the answer's words decide where they say more than the status does, so a quota spent is no
 * credit whatever status it came with. */
export type Unanswered = "notAccepted" | "noCredit" | "noAccess" | "unsafe" | "tooLong" | "busy" | "down" | "unreached" | "unread" | "refused";

export function unanswered(status: number, body: string): Unanswered {
  const said = body.toLowerCase();
  const names = (words: string[]) => words.some((w) => said.includes(w));
  if (status === 0) return "unreached";
  // Answered with nothing in it: withheld by the safety system, or an answer that does not read.
  if (status >= 200 && status < 300) return names(['safety"', "prohibited_content", "blocklist", '"refusal"']) ? "unsafe" : "unread";
  if (status === 401) return "notAccepted";
  if (status === 402 || names(["credit", "billing", "payment", "insufficient_quota", "exceeded your current quota"])) return "noCredit";
  if (
    status === 413 ||
    names(["too long", "too large", "context length", "context window", "maximum context", "too many tokens", "maximum number of tokens"])
  )
    return "tooLong";
  if (names(["safety", "moderation", "content_policy", "content policy", "usage policy", "flagged"])) return "unsafe";
  if (status === 403) {
    // Refused with no words, or of the key itself: not accepted. Else what the key may use.
    if (!/\p{L}/u.test(said)) return "notAccepted";
    if (names(["permission", "access", "not allowed", "region", "country", "territory", "verified"])) return "noAccess";
    return names(["key"]) ? "notAccepted" : "noAccess";
  }
  // Gemini answers a key it does not take with a 400 that says so.
  if (status === 400 && names(["api key", "api_key", "x-api-key"])) return "notAccepted";
  if (status === 404 && names(["model"])) return "noAccess";
  if (status === 408 || status === 429) return "busy";
  if (status >= 500) return "down";
  return "refused";
}

/** That `provider` could not be reached, in a person's words. */
export const unreached = (provider: string) => `${provider} can't be reached. Check your internet connection.`;

/** Why `provider`'s answer of `status`, its body `body`, is no answer, in a person's words, as the
 * Mac's `Why.answered` says it: no status, no code, and none of the provider's own words. */
export function answered(provider: string, status: number, body: string): string {
  switch (unanswered(status, body)) {
    case "notAccepted":
      return `${provider} didn't accept your key. Check it, or replace it.`;
    case "noCredit":
      return `Your ${provider} account is out of credit, or past its limit. Check its plan and billing.`;
    case "noAccess":
      return `Your ${provider} account can't use these models. Check its plan, or choose another model.`;
    case "unsafe":
      return `${provider}'s safety system turned that down. Ask in other words.`;
    case "tooLong":
      return `That is more than ${provider}'s model takes at once. Ask about less, or start again.`;
    case "busy":
      return `${provider} is busy. Try again in a moment.`;
    case "down":
      return `${provider} couldn't answer just now. Try again in a moment.`;
    case "unreached":
      return unreached(provider);
    case "unread":
      return `${provider}'s answer couldn't be read. Try again in a moment.`;
    case "refused":
      return `${provider} couldn't answer that. Try again in a moment.`;
  }
}

/** The answer an ask that came to nothing carries: its status and its words, as the assistant's
 * adapters throw them (`STATUS TEXT: words`, or `not JSON: …` for one that does not read; `json`
 * in assistant/providers.ts), status 0 where `fetch` found no way to the provider, as each browser
 * words that. None for an error that is no provider's answer: the page's own, said as no answer. */
export function answerOf(e: unknown): { status: number; words: string } | undefined {
  const message = e instanceof Error ? e.message : String(e);
  const answer = /^([1-5]\d\d)\b[^:]*:\s*([\s\S]*)$/.exec(message);
  if (answer) return { status: Number(answer[1]), words: answer[2] };
  if (message.startsWith("not JSON: ")) return { status: 200, words: message.slice("not JSON: ".length) };
  if (/failed to fetch|networkerror|load failed|fetch failed/i.test(message)) return { status: 0, words: "" };
  return undefined;
}

/** Why asking `provider` came to nothing, from what the ask threw, in a person's words, as the Mac's
 * `Why.said` says it: the provider's answer by what it means, no way to it, or stopped. */
export function unasked(provider: string, e: unknown): string {
  if (e instanceof Error && e.name === "AbortError") return "Stopped.";
  const answer = answerOf(e);
  return answer ? answered(provider, answer.status, answer.words) : `${provider} couldn't answer that. Try again in a moment.`;
}
