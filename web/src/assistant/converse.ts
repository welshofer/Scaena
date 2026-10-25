// The assistant's loop (PLAN 2.6): the model answers, its calls run on the bundle, their
// results go back, and so on until it answers without a call, the user stops it, or it has
// called tools as many rounds as one question allows.
import type { AssistantEvent } from "../protocol";
import { answerOf, unanswered, unasked } from "../words";
import type { Call, Message, Provider, Result, Tool, Turn } from "./providers";
import { owing } from "./cut";
import { fenced } from "./prompt";

/** What the page hears as the assistant works. */
export type Event = AssistantEvent;

export interface Options {
  provider: Provider;
  model: string;
  key: string;
  base?: string;
  system: string;
  tools: Tool[];
  /** The conversation so far, the user's question last; the loop adds to it. */
  conversation: Message[];
  /** Run a call on the bundle: its result, a line saying what it came to, and whether it did
   * what its words say. */
  call(call: Call): Promise<{ result: Result; summary: string; made: boolean }>;
  emit(event: Event): void;
  signal: AbortSignal;
  /** The most rounds of calls one question may take. */
  rounds?: number;
  /** The most frames the model is sent again: older ones are named, not shown. */
  frames?: number;
  /** How long an answer, once it has begun, may go without a word from the provider before it is
   * given up and asked again (narrative REL-3); two minutes without it. */
  timeoutMs?: number;
  /** How many times a turn a busy, down, or silent provider is asked again; three without it. */
  retries?: number;
}

/** How long an answer that has begun may go with nothing more from the provider. */
const SILENCE_MS = 120_000;
/** How long a turn may wait for the first piece of its answer: ten minutes, since a model that
 * reasons first may say nothing for longer than two. */
const FIRST_MS = 600_000;
/** The waits before asking again, in seconds, where the provider names none. */
const WAITS = [2, 4, 8];

/** Whether a turn that failed so is worth asking again (narrative REL-3): the provider busy
 * (408, 429), down (5xx, Anthropic's 529), or silent past its deadline; never a key refused, a
 * question too long, or the safety system. */
function again(e: unknown, silent: boolean): boolean {
  if (silent) return true;
  const answer = answerOf(e);
  if (!answer) return false;
  const why = unanswered(answer.status, answer.words);
  return why === "busy" || why === "down";
}

/** `ms` passed, or `signal` aborted. */
function pause(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    const timer = setTimeout(done, ms);
    function done() {
      clearTimeout(timer);
      signal.removeEventListener("abort", done);
      resolve();
    }
    signal.addEventListener("abort", done, { once: true });
  });
}

/** One turn of `o`'s provider, asked again after a wait when it is busy, down, or silent, up to
 * `o.retries` times, the panel told of each wait; the turn, or what the last ask threw. */
async function turn(o: Options, messages: Message[]): Promise<Turn> {
  const retries = o.retries ?? WAITS.length;
  for (let attempt = 0; ; attempt++) {
    // A deadline on silence, counted once the answer has begun: each piece that arrives puts it
    // off again. Until the first, the turn waits as long as a model that reasons first may take.
    const silence = new AbortController();
    let timer: ReturnType<typeof setTimeout> | undefined;
    let begun = false;
    const alive = (ms: number) => {
      clearTimeout(timer);
      timer = setTimeout(() => silence.abort(), ms);
    };
    alive(FIRST_MS);
    const signal = AbortSignal.any([o.signal, silence.signal]);
    try {
      return await o.provider.turn({
        model: o.model,
        key: o.key,
        base: o.base,
        system: o.system,
        tools: o.tools,
        messages,
        signal,
        // The words as they are written, which the panel shows before the answer is whole.
        onText: (text) => o.emit({ kind: "saying", text }),
        onChunk: () => {
          begun = true;
          alive(o.timeoutMs ?? SILENCE_MS);
        },
      });
    } catch (e) {
      const silent = silence.signal.aborted && !o.signal.aborted;
      if (o.signal.aborted || attempt >= retries || !again(e, silent)) throw silent ? new Error(`504 Silent: the provider ${begun ? "stopped in the middle of its answer" : "sent nothing for ten minutes"}`) : e;
      const asked = (e as { retryAfter?: number }).retryAfter;
      const seconds = Math.min(60, Math.max(1, Math.ceil(asked ?? WAITS[Math.min(attempt, WAITS.length - 1)])));
      const message = silent ? `${o.provider.name} went quiet.` : unasked(o.provider.name, e).replace(/ Try again in a moment\.$/, "");
      o.emit({ kind: "waiting", message, seconds, attempt: attempt + 1, of: retries });
      await pause(seconds * 1000, o.signal);
      if (o.signal.aborted) throw e;
    } finally {
      clearTimeout(timer);
    }
  }
}

/** About how much of tools' results, in bytes, the model is sent again (lift-up PERF-3): each
 * round re-sends the conversation, so a long session's `deck_read`s and `deck_health`s would grow
 * it until the provider refuses it. `scaena-chat`'s `BUDGET`. */
export const BUDGET = 60_000;

const bytes = (text: string) => new TextEncoder().encode(text).length;

/** The conversation as the model is sent it: the newest `frames` frames, and the older ones
 * named in their place, since each costs as much as a page of text; and the newest results whole
 * within `budget` bytes, the last round's always, and the older ones named in their place from the
 * first that would pass it on (lift-up PERF-3), so what is cached before them stays. */
export function recent(conversation: Message[], frames: number, budget = BUDGET): Message[] {
  let kept = 0;
  let used = 0;
  const last = conversation.findLastIndex((m) => m.role === "tool");
  return conversation
    .map((m, i) => [m, i] as const)
    .reverse()
    .map(([m, i]) => {
      if (m.role !== "tool") return m;
      const results = m.results
        .slice()
        .reverse()
        .map((r) => {
          used += bytes(r.json);
          if (i !== last && used > budget) {
            used = Infinity;
            return { ...r, png: undefined, json: `(What ${r.name} returned is no longer shown: call it again to see it.)` };
          }
          if (!r.png) return r;
          if (kept++ < frames) return r;
          return { ...r, png: undefined, json: `${r.json}\n(The frame is no longer shown: render it again to see it.)` };
        })
        .reverse();
      return { ...m, results };
    })
    .reverse();
}

export async function converse(o: Options): Promise<void> {
  const rounds = o.rounds ?? 32;
  const stopped = () => o.signal.aborted;
  // Halved for the rest of the question where the provider finds the conversation too long.
  let budget = BUDGET;
  for (let round = 0; ; round++) {
    if (stopped()) return o.emit({ kind: "done", stop: "stopped" });
    if (round === rounds) return o.emit({ kind: "done", stop: "steps" });
    let answered: Turn;
    try {
      answered = await turn(o, recent(o.conversation, o.frames ?? 2, budget));
    } catch (e) {
      if (stopped()) return o.emit({ kind: "done", stop: "stopped" });
      // Too long for the model: sent again once with half as much of tools' results (lift-up
      // PERF-3), before the person is told to start again.
      const answer = answerOf(e);
      if (budget === BUDGET && answer && unanswered(answer.status, answer.words) === "tooLong") {
        budget = BUDGET / 2;
        round--;
        continue;
      }
      // A refusal, or no way to the provider, in a person's words (PLAN 3.18), as the Mac's
      // assistant says it: never the provider's own, nor its status.
      return o.emit({ kind: "failed", message: unasked(o.provider.name, e) });
    }
    o.conversation.push({ role: "assistant", text: answered.text, calls: answered.calls, raw: answered.raw });
    if (answered.usage) o.emit({ kind: "usage", ...answered.usage });
    if (answered.text) o.emit({ kind: "text", text: answered.text });
    // A call the answer's token limit cut off is not run: its arguments are not what the model
    // meant. It goes back with the others, told to make the edit in smaller patches, as
    // scaena-chat's `answer` tells it (lift-up REL-1).
    const { run: calls, owed } = owing(answered.calls, answered.stop);
    if (!calls.length && !owed.length) return o.emit({ kind: "done", stop: answered.stop });
    const results: Result[] = [];
    for (const call of calls) {
      // A call the user stopped before it ran is answered as stopped: every call has an answer.
      if (stopped()) {
        results.push({ id: call.id, name: call.name, json: JSON.stringify({ message: "the user stopped the assistant" }), error: true });
        continue;
      }
      o.emit({ kind: "call", id: call.id, name: call.name, args: call.args });
      // A call that throws is answered as failed, saying why: a call left with no answer makes the
      // provider refuse every turn after it (lift-up 10-25b STAB-3).
      const { result, summary, made } = await o.call(call).catch((e: unknown) => {
        const message = e instanceof Error ? e.message : String(e);
        return { result: { id: call.id, name: call.name, json: JSON.stringify({ message }), error: true } as Result, summary: `Stopped: ${message}`, made: false };
      });
      results.push(result);
      o.emit({ kind: "result", id: call.id, name: call.name, error: result.error, summary, made, json: result.json, png: result.png });
    }
    results.push(...owed);
    // Each result, errors too, reaches the model fenced as data under its tool's name (lift-up
    // SEC-3), as scaena-chat's `results` fences it: what a tool reads is the deck's words or a
    // source's, and an error can quote them.
    const told = results.map((r) => ({ ...r, json: fenced(r.name.replace(/_/g, "-"), r.json) }));
    o.conversation.push({ role: "tool", results: told });
  }
}
