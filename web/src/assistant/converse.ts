// The assistant's loop (PLAN 2.6): the model answers, its calls run on the bundle, their
// results go back, and so on until it answers without a call, the user stops it, or it has
// called tools as many rounds as one question allows.
import type { AssistantEvent } from "../protocol";
import type { Call, Message, Provider, Result, Tool, Turn } from "./providers";

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
  /** Run a call on the bundle: its result, and a line saying what it came to. */
  call(call: Call): Promise<{ result: Result; summary: string }>;
  emit(event: Event): void;
  signal: AbortSignal;
  /** The most rounds of calls one question may take. */
  rounds?: number;
  /** The most frames the model is sent again: older ones are named, not shown. */
  frames?: number;
}

/** The conversation as the model is sent it: the newest `frames` frames, and the older ones
 * named in their place, since each costs as much as a page of text. */
export function recent(conversation: Message[], frames: number): Message[] {
  let kept = 0;
  return conversation
    .slice()
    .reverse()
    .map((m) => {
      if (m.role !== "tool") return m;
      const results = m.results
        .slice()
        .reverse()
        .map((r) => {
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
  for (let round = 0; ; round++) {
    if (stopped()) return o.emit({ kind: "done", stop: "stopped" });
    if (round === rounds) return o.emit({ kind: "done", stop: "steps" });
    let turn: Turn;
    try {
      turn = await o.provider.turn({
        model: o.model,
        key: o.key,
        base: o.base,
        system: o.system,
        tools: o.tools,
        messages: recent(o.conversation, o.frames ?? 2),
        signal: o.signal,
      });
    } catch (e) {
      if (stopped()) return o.emit({ kind: "done", stop: "stopped" });
      return o.emit({ kind: "failed", message: e instanceof Error ? e.message : String(e) });
    }
    o.conversation.push({ role: "assistant", text: turn.text, calls: turn.calls, raw: turn.raw });
    if (turn.usage) o.emit({ kind: "usage", ...turn.usage });
    if (turn.text) o.emit({ kind: "text", text: turn.text });
    if (!turn.calls.length) return o.emit({ kind: "done", stop: turn.stop });
    const results: Result[] = [];
    for (const call of turn.calls) {
      // A call the user stopped before it ran is answered as stopped: every call has an answer.
      if (stopped()) {
        results.push({ id: call.id, name: call.name, json: JSON.stringify({ message: "the user stopped the assistant" }), error: true });
        continue;
      }
      o.emit({ kind: "call", id: call.id, name: call.name, args: call.args });
      const { result, summary } = await o.call(call);
      results.push(result);
      o.emit({ kind: "result", id: call.id, name: call.name, error: result.error, summary, json: result.json, png: result.png });
    }
    o.conversation.push({ role: "tool", results });
  }
}
