// A model's answer as it is written (narrative ATTR-3): each provider streams its answer as
// server-sent events, and this assembles them into the answer the provider would have sent
// whole, so each adapter in `providers.ts` reads a streamed answer as it reads one that came at
// once; and it hands out the words as they arrive, for the panel to show. `scaena-chat`'s
// `stream.rs` does the same for the Mac, and the recorded exchanges hold the two together
// (ADR-0022).
import type { ProviderId } from "../protocol";

/** One streamed answer being assembled: fed the body in whatever pieces the network hands it,
 * it gives the words each piece completes, and at the end the answer as JSON. */
export class Stream {
  /** What was fed and not yet read: the last line, which may be cut mid-way. */
  private pending = "";
  private event: string | undefined;
  private data: string[] = [];
  /** An error the stream carried mid-way (Anthropic's `event: error`). */
  private error: unknown;
  private anthropic: { message: Record<string, unknown>; blocks: Block[] } = { message: {}, blocks: [] };
  private openai: { id?: unknown; content?: string; calls: [string, string, string][]; finish?: unknown; usage?: unknown } = { calls: [] };
  private gemini: { parts: Record<string, unknown>[]; finish?: unknown; usage?: unknown } = { parts: [] };

  private readonly provider: ProviderId;

  constructor(provider: ProviderId) {
    this.provider = provider;
  }

  /** Read `chunk`, the next piece of the body: the words of the answer it completes. */
  feed(chunk: string): string {
    this.pending += chunk;
    let said = "";
    for (let end = this.pending.indexOf("\n"); end >= 0; end = this.pending.indexOf("\n")) {
      const line = this.pending.slice(0, end).replace(/\r$/, "");
      this.pending = this.pending.slice(end + 1);
      said += this.line(line);
    }
    return said;
  }

  /** The answer, as the provider would have sent it whole, once the body has all been fed: an
   * error the stream carried, as the provider's error object, where it carried one. */
  finish(): any {
    if (this.pending) {
      const line = this.pending.replace(/\r$/, "");
      this.pending = "";
      this.line(line);
    }
    this.dispatch();
    if (this.error !== undefined) return this.error;
    switch (this.provider) {
      case "anthropic": {
        const content = this.anthropic.blocks.map((b) => {
          if (b.kind === "tool_use") return { type: "tool_use", id: b.id, name: b.name, input: parsed(b.json) };
          if (b.kind === "text") return { type: "text", text: b.text };
          return { type: b.kind };
        });
        return { ...this.anthropic.message, content };
      }
      case "openai": {
        const o = this.openai;
        const message: Record<string, unknown> = { role: "assistant", content: o.content ?? null };
        if (o.calls.length) message.tool_calls = o.calls.map(([id, name, args]) => ({ id, type: "function", function: { name, arguments: args } }));
        return {
          ...(o.id !== undefined && { id: o.id }),
          choices: [{ index: 0, finish_reason: o.finish ?? null, message }],
          ...(o.usage !== undefined && { usage: o.usage }),
        };
      }
      case "gemini": {
        const g = this.gemini;
        return {
          candidates: [{ content: { role: "model", parts: g.parts }, ...(g.finish !== undefined && { finishReason: g.finish }) }],
          ...(g.usage !== undefined && { usageMetadata: g.usage }),
        };
      }
    }
  }

  /** One line of the stream: a field of the event being read, or the blank line that ends it. */
  private line(line: string): string {
    if (!line) return this.dispatch();
    if (line.startsWith(":")) return "";
    const colon = line.indexOf(":");
    const field = colon < 0 ? line : line.slice(0, colon);
    let value = colon < 0 ? "" : line.slice(colon + 1);
    if (value.startsWith(" ")) value = value.slice(1);
    if (field === "event") this.event = value;
    else if (field === "data") this.data.push(value);
    return "";
  }

  /** The event read so far, taken: the words it adds. */
  private dispatch(): string {
    const event = this.event;
    this.event = undefined;
    const data = this.data.join("\n");
    this.data = [];
    if (!data || data === "[DONE]") return "";
    let value: any;
    try {
      value = JSON.parse(data);
    } catch {
      return "";
    }
    switch (this.provider) {
      case "anthropic":
        return this.anthropicEvent(event, value);
      case "openai":
        return this.openaiChunk(value);
      case "gemini":
        return this.geminiChunk(value);
    }
  }

  private anthropicEvent(event: string | undefined, v: any): string {
    const kind = event ?? v.type ?? "";
    const index = typeof v.index === "number" ? v.index : 0;
    const a = this.anthropic;
    switch (kind) {
      case "message_start":
        if (v.message && typeof v.message === "object") a.message = { ...v.message };
        return "";
      case "content_block_start": {
        const block = v.content_block ?? {};
        const made: Block = { kind: block.type ?? "", id: block.id ?? "", name: block.name ?? "", text: block.text ?? "", json: "" };
        while (a.blocks.length < index) a.blocks.push({ kind: "", id: "", name: "", text: "", json: "" });
        a.blocks[index] = made;
        return made.kind === "text" ? made.text : "";
      }
      case "content_block_delta": {
        const block = a.blocks[index];
        if (!block) return "";
        const delta = v.delta ?? {};
        if (delta.type === "text_delta") {
          const text = typeof delta.text === "string" ? delta.text : "";
          block.text += text;
          return text;
        }
        if (delta.type === "input_json_delta") block.json += typeof delta.partial_json === "string" ? delta.partial_json : "";
        return "";
      }
      case "message_delta":
        if (v.delta && typeof v.delta === "object") Object.assign(a.message, v.delta);
        if (v.usage && typeof v.usage === "object") a.message.usage = { ...((a.message.usage as object) ?? {}), ...v.usage };
        return "";
      case "error":
        this.error = v;
        return "";
      default:
        return "";
    }
  }

  private openaiChunk(v: any): string {
    const o = this.openai;
    if (o.id === undefined && v.id !== undefined) o.id = v.id;
    if (v.usage != null) o.usage = v.usage;
    if (v.error != null) {
      this.error = { error: v.error };
      return "";
    }
    const choice = v.choices?.[0];
    if (!choice) return "";
    if (choice.finish_reason != null) o.finish = choice.finish_reason;
    let said = "";
    const delta = choice.delta;
    if (delta) {
      if (typeof delta.content === "string") {
        o.content = (o.content ?? "") + delta.content;
        said += delta.content;
      }
      for (const call of delta.tool_calls ?? []) {
        const index = typeof call.index === "number" ? call.index : 0;
        while (o.calls.length <= index) o.calls.push(["", "", ""]);
        const slot = o.calls[index];
        if (typeof call.id === "string") slot[0] = call.id;
        if (typeof call.function?.name === "string") slot[1] += call.function.name;
        if (typeof call.function?.arguments === "string") slot[2] += call.function.arguments;
      }
    }
    return said;
  }

  private geminiChunk(v: any): string {
    const g = this.gemini;
    if (v.usageMetadata !== undefined) g.usage = v.usageMetadata;
    if (v.error != null) {
      this.error = { error: v.error };
      return "";
    }
    const candidate = v.candidates?.[0];
    if (!candidate) return "";
    if (candidate.finishReason != null) g.finish = candidate.finishReason;
    let said = "";
    for (const part of candidate.content?.parts ?? []) {
      const text = typeof part.text === "string" ? part.text : undefined;
      const thought = part.thought === true;
      if (text !== undefined && !thought) said += text;
      // A run of text parts is one part, as the answer sent whole has it, unless a part carries a
      // signature of its own, which goes back to the model as it came.
      const last = g.parts.at(-1);
      const alike = last && typeof last.text === "string" && (last.thought === true) === thought;
      if (text !== undefined && alike && part.thoughtSignature === undefined) last.text = (last.text as string) + text;
      else g.parts.push({ ...part });
    }
    return said;
  }
}

interface Block {
  kind: string;
  id: string;
  name: string;
  text: string;
  json: string;
}

function parsed(text: string): unknown {
  try {
    return text ? JSON.parse(text) : {};
  } catch {
    return {};
  }
}

/** Whether `body` begins a stream of server-sent events, rather than one JSON value. */
export function isSse(body: string): boolean {
  const first = body.trimStart();
  return first.startsWith("event:") || first.startsWith("data:") || first.startsWith(":");
}

/** A provider's answer to a streamed request, or why it gave none. The body is read as it
 * arrives: a stream of events is assembled, each word of the answer handed to `onText` as it
 * completes; a JSON body, an error's or an OpenAI-compatible server's that does not stream, is
 * read whole. An error the stream carried mid-way is thrown as a refusal with that error's status
 * (529 for Anthropic's overloaded, 500 otherwise). */
export async function answer(provider: ProviderId, response: Response, onText?: (text: string) => void, onChunk?: () => void): Promise<any> {
  const text = await (async () => {
    if (!response.ok || !response.body) return response.text();
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let stream: Stream | undefined;
    let whole = "";
    let head = "";
    let decided = false;
    for (;;) {
      const { value, done } = await reader.read();
      onChunk?.();
      const piece = decoder.decode(value, { stream: !done });
      if (!decided) {
        head += piece;
        const first = head.trimStart();
        if (first || done) {
          decided = true;
          if (isSse(head) || (!first.startsWith("{") && !first.startsWith("["))) stream = new Stream(provider);
          if (stream) {
            const said = stream.feed(head);
            if (said && onText) onText(said);
          } else whole = head;
        }
      } else if (stream) {
        const said = stream.feed(piece);
        if (said && onText) onText(said);
      } else whole += piece;
      if (done) break;
    }
    if (!stream) return whole;
    const assembled = stream.finish();
    if (assembled?.error && (assembled.type === "error" || !assembled.candidates)) {
      const overloaded = assembled.error.type === "overloaded_error";
      const message = typeof assembled.error.message === "string" ? assembled.error.message : "";
      throw new Error(`${overloaded ? "529 Overloaded" : "500 Error"}: ${message}`.trim());
    }
    return JSON.stringify(assembled);
  })();
  let body: any;
  try {
    body = text ? JSON.parse(text) : {};
  } catch {
    body = undefined;
  }
  if (!response.ok) {
    const said = body?.error?.message ?? body?.message ?? text.slice(0, 300);
    // With how long the provider asks to wait before asking again, where it says (REL-3).
    throw Object.assign(new Error(`${response.status} ${response.statusText}: ${said}`.trim()), { retryAfter: retryAfter(response) });
  }
  if (body === undefined) throw new Error(`not JSON: ${text.slice(0, 300)}`);
  return body;
}

/** The seconds a refusal asks the client to wait (`Retry-After`, as seconds or a date), where it
 * says; none otherwise. */
export function retryAfter(response: Response): number | undefined {
  const header = response.headers.get("retry-after");
  if (!header) return undefined;
  const seconds = Number(header);
  if (Number.isFinite(seconds)) return Math.max(0, seconds);
  const at = Date.parse(header);
  return Number.isFinite(at) ? Math.max(0, (at - Date.now()) / 1000) : undefined;
}
