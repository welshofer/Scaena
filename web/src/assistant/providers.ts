// The assistant's model, called from the browser with the user's own key (PLAN 2.6, SPEC
// §11, ADR-0006): Anthropic's Messages API, OpenAI's Chat Completions, and Gemini's
// generateContent. Each adapter turns one conversation, kept in the form below, into its
// provider's request, and its response back. No model is named here: each lists the models the
// key can use, and the user picks one.
import type { ProviderId } from "../protocol";

/** A tool the model may call: its name, what it does, and its arguments as JSON Schema. */
export interface Tool {
  name: string;
  description: string;
  schema: Record<string, unknown>;
}

/** A call the model made: its id (the provider's, or one made up where it gives none), the
 * tool, and the arguments. `raw` is the part of the model's answer that made it, kept to send
 * back as it came where the provider asks for that (Gemini's thought signatures). */
export interface Call {
  id: string;
  name: string;
  args: unknown;
  raw?: unknown;
}

/** What a tool returned: its result as JSON, whether it stopped, and a PNG, base64, for a
 * frame it drew. */
export interface Result {
  id: string;
  name: string;
  json: string;
  error: boolean;
  png?: string;
}

/** The conversation, in no provider's form. */
export type Message =
  | { role: "user"; text: string }
  | { role: "assistant"; text: string; calls: Call[]; raw?: unknown }
  | { role: "tool"; results: Result[] };

/** One answer of the model: what it said, the tools it calls, and why it stopped. */
export interface Turn {
  text: string;
  calls: Call[];
  /** `tools`: it waits for its calls' results. `end`: its answer is done. `length`: it ran
   * out of room. */
  stop: "tools" | "end" | "length" | "other";
  raw?: unknown;
  /** Tokens in and out, as the provider counts them. */
  usage?: { input: number; output: number };
}

export interface Ask {
  model: string;
  key: string;
  /** The API's address; the provider's own without it. */
  base?: string;
  system: string;
  tools: Tool[];
  messages: Message[];
  signal?: AbortSignal;
}

export interface Provider {
  name: string;
  /** The API's own address. */
  base: string;
  /** The models the key can use. */
  models(key: string, base?: string, signal?: AbortSignal): Promise<string[]>;
  turn(ask: Ask): Promise<Turn>;
}

export type { ProviderId };

/** A provider's answer, or why it gave none: its own message where it sends one. */
async function json(response: Response): Promise<any> {
  const text = await response.text();
  let body: any;
  try {
    body = text ? JSON.parse(text) : {};
  } catch {
    body = undefined;
  }
  if (!response.ok) {
    const said = body?.error?.message ?? body?.message ?? text.slice(0, 300);
    throw new Error(`${response.status} ${response.statusText}: ${said}`.trim());
  }
  if (body === undefined) throw new Error(`not JSON: ${text.slice(0, 300)}`);
  return body;
}

const at = (base: string | undefined, fallback: string, path: string) => `${(base || fallback).replace(/\/+$/, "")}${path}`;

/** `schema` with every `$ref` to its `$defs` written in place, and no `$schema`: what each
 * provider takes, whatever it makes of references. */
export function inlined(schema: Record<string, unknown>): Record<string, unknown> {
  const defs = (schema.$defs ?? {}) as Record<string, unknown>;
  const walk = (v: unknown, seen: string[]): unknown => {
    if (Array.isArray(v)) return v.map((x) => walk(x, seen));
    if (!v || typeof v !== "object") return v;
    const o = v as Record<string, unknown>;
    if (typeof o.$ref === "string" && o.$ref.startsWith("#/$defs/")) {
      const name = o.$ref.slice("#/$defs/".length);
      if (seen.includes(name)) return {};
      const { $ref: _, ...rest } = o;
      return { ...(walk(defs[name], [...seen, name]) as object), ...(walk(rest, seen) as object) };
    }
    const out: Record<string, unknown> = {};
    for (const [k, x] of Object.entries(o)) if (k !== "$defs" && k !== "$schema") out[k] = walk(x, seen);
    return out;
  };
  return walk(schema, []) as Record<string, unknown>;
}

/** A conversation with each run of one role's messages kept together, as providers that
 * alternate roles need: a tool's results, then the user's next words, are one user turn. */
function runs<T extends { role: string }>(items: T[]): T[][] {
  const out: T[][] = [];
  for (const item of items) {
    const last = out.at(-1);
    if (last && last[0].role === item.role) last.push(item);
    else out.push([item]);
  }
  return out;
}

/** Anthropic's Messages API, called from the browser with its opt-in header for that. */
export const anthropic: Provider = {
  name: "Anthropic",
  base: "https://api.anthropic.com",
  async models(key, base, signal) {
    const response = await fetch(at(base, this.base, "/v1/models?limit=100"), { headers: anthropicHeaders(key), signal });
    const body = await json(response);
    return (body.data ?? []).map((m: { id: string }) => m.id);
  },
  async turn(ask) {
    const messages = runs(
      ask.messages.map((m) => {
        if (m.role === "user") return { role: "user", content: [{ type: "text", text: m.text }] };
        if (m.role === "assistant") {
          const content: unknown[] = m.text ? [{ type: "text", text: m.text }] : [];
          for (const c of m.calls) content.push({ type: "tool_use", id: c.id, name: c.name, input: c.args ?? {} });
          return { role: "assistant", content };
        }
        return {
          role: "user",
          content: m.results.map((r) => ({
            type: "tool_result",
            tool_use_id: r.id,
            is_error: r.error,
            content: [
              { type: "text", text: r.json },
              ...(r.png ? [{ type: "image", source: { type: "base64", media_type: "image/png", data: r.png } }] : []),
            ],
          })),
        };
      }),
    ).map((run) => ({ role: run[0].role, content: run.flatMap((m) => m.content) }));
    const body = {
      model: ask.model,
      max_tokens: 8192,
      // The system prompt is the same each turn: cached, it costs a tenth on every turn after the first.
      system: [{ type: "text", text: ask.system, cache_control: { type: "ephemeral" } }],
      tools: ask.tools.map((t) => ({ name: t.name, description: t.description, input_schema: inlined(t.schema) })),
      messages,
    };
    const response = await fetch(at(ask.base, this.base, "/v1/messages"), {
      method: "POST",
      headers: { ...anthropicHeaders(ask.key), "content-type": "application/json" },
      body: JSON.stringify(body),
      signal: ask.signal,
    });
    const answer = await json(response);
    const content = (answer.content ?? []) as { type: string; text?: string; id?: string; name?: string; input?: unknown }[];
    const stop = answer.stop_reason === "tool_use" ? "tools" : answer.stop_reason === "end_turn" ? "end" : answer.stop_reason === "max_tokens" ? "length" : "other";
    return {
      text: content.flatMap((c) => (c.type === "text" && c.text ? [c.text] : [])).join("\n\n"),
      calls: content.flatMap((c) => (c.type === "tool_use" ? [{ id: c.id!, name: c.name!, args: c.input }] : [])),
      stop,
      usage: answer.usage && { input: answer.usage.input_tokens ?? 0, output: answer.usage.output_tokens ?? 0 },
    };
  },
};

function anthropicHeaders(key: string): Record<string, string> {
  return {
    "x-api-key": key,
    "anthropic-version": "2023-06-01",
    // A browser calls the API with the user's own key, which never leaves this page but for
    // Anthropic (ADR-0006).
    "anthropic-dangerous-direct-browser-access": "true",
  };
}

/** OpenAI's Chat Completions, which OpenAI-compatible servers speak too: a base address of
 * one runs the assistant on it. */
export const openai: Provider = {
  name: "OpenAI",
  base: "https://api.openai.com",
  async models(key, base, signal) {
    const response = await fetch(at(base, this.base, "/v1/models"), { headers: { authorization: `Bearer ${key}` }, signal });
    const body = await json(response);
    return ((body.data ?? []) as { id: string }[]).map((m) => m.id).sort();
  },
  async turn(ask) {
    const messages: unknown[] = [{ role: "system", content: ask.system }];
    for (const m of ask.messages) {
      if (m.role === "user") messages.push({ role: "user", content: m.text });
      else if (m.role === "assistant") {
        messages.push({
          role: "assistant",
          content: m.text || null,
          ...(m.calls.length && {
            tool_calls: m.calls.map((c) => ({ id: c.id, type: "function", function: { name: c.name, arguments: JSON.stringify(c.args ?? {}) } })),
          }),
        });
      } else {
        for (const r of m.results) messages.push({ role: "tool", tool_call_id: r.id, content: r.json });
        // A tool's message holds text alone: the frames it drew follow as the user's.
        const frames = m.results.filter((r) => r.png);
        if (frames.length)
          messages.push({
            role: "user",
            content: [
              { type: "text", text: `The frame${frames.length > 1 ? "s" : ""} deck_render drew (${frames.map((r) => r.id).join(", ")}):` },
              ...frames.map((r) => ({ type: "image_url", image_url: { url: `data:image/png;base64,${r.png}` } })),
            ],
          });
      }
    }
    const body = {
      model: ask.model,
      messages,
      tools: ask.tools.map((t) => ({ type: "function", function: { name: t.name, description: t.description, parameters: inlined(t.schema) } })),
    };
    const response = await fetch(at(ask.base, this.base, "/v1/chat/completions"), {
      method: "POST",
      headers: { authorization: `Bearer ${ask.key}`, "content-type": "application/json" },
      body: JSON.stringify(body),
      signal: ask.signal,
    });
    const answer = await json(response);
    const choice = answer.choices?.[0] ?? {};
    const message = choice.message ?? {};
    const calls = ((message.tool_calls ?? []) as { id: string; function: { name: string; arguments: string } }[]).map((c) => ({
      id: c.id,
      name: c.function.name,
      args: parsed(c.function.arguments),
    }));
    const reason = choice.finish_reason;
    return {
      text: typeof message.content === "string" ? message.content : "",
      calls,
      stop: calls.length ? "tools" : reason === "stop" ? "end" : reason === "length" ? "length" : "other",
      usage: answer.usage && { input: answer.usage.prompt_tokens ?? 0, output: answer.usage.completion_tokens ?? 0 },
    };
  },
};

/** A call's arguments, which OpenAI sends as text: what they say, or the text itself, which
 * the tool then refuses as arguments that are not an object. */
function parsed(text: string): unknown {
  try {
    return text ? JSON.parse(text) : {};
  } catch {
    return text;
  }
}

/** The start of an id Gemini did not give a call: one of ours, which it is not sent back. */
const LOCAL = "local-";

/** Gemini's generateContent. A model's function calls carry thought signatures it needs back
 * as they came, so its answer's parts are kept and sent again. */
export const gemini: Provider = {
  name: "Gemini",
  base: "https://generativelanguage.googleapis.com",
  async models(key, base, signal) {
    const response = await fetch(at(base, this.base, "/v1beta/models?pageSize=1000"), { headers: { "x-goog-api-key": key }, signal });
    const body = await json(response);
    return ((body.models ?? []) as { name: string; supportedGenerationMethods?: string[] }[])
      .filter((m) => m.supportedGenerationMethods?.includes("generateContent") ?? true)
      .map((m) => m.name.replace(/^models\//, ""));
  },
  async turn(ask) {
    const contents = runs(
      ask.messages.map((m) => {
        if (m.role === "user") return { role: "user", parts: [{ text: m.text }] as unknown[] };
        if (m.role === "assistant") {
          if (Array.isArray(m.raw)) return { role: "model", parts: m.raw as unknown[] };
          const parts: unknown[] = m.text ? [{ text: m.text }] : [];
          for (const c of m.calls) parts.push({ functionCall: { name: c.name, args: c.args ?? {} } });
          return { role: "model", parts };
        }
        const parts: unknown[] = m.results.map((r) => ({
          functionResponse: {
            ...(!r.id.startsWith(LOCAL) && { id: r.id }),
            name: r.name,
            response: r.error ? { error: parsed(r.json) } : { result: parsed(r.json) },
          },
        }));
        for (const r of m.results) if (r.png) parts.push({ inlineData: { mimeType: "image/png", data: r.png } });
        return { role: "user", parts };
      }),
    ).map((run) => ({ role: run[0].role, parts: run.flatMap((m) => m.parts) }));
    const body = {
      systemInstruction: { parts: [{ text: ask.system }] },
      contents,
      tools: [{ functionDeclarations: ask.tools.map((t) => ({ name: t.name, description: t.description, parametersJsonSchema: inlined(t.schema) })) }],
    };
    const model = ask.model.replace(/^models\//, "");
    const response = await fetch(at(ask.base, this.base, `/v1beta/models/${encodeURIComponent(model)}:generateContent`), {
      method: "POST",
      headers: { "x-goog-api-key": ask.key, "content-type": "application/json" },
      body: JSON.stringify(body),
      signal: ask.signal,
    });
    const answer = await json(response);
    const candidate = answer.candidates?.[0] ?? {};
    const parts = (candidate.content?.parts ?? []) as { text?: string; thought?: boolean; functionCall?: { id?: string; name: string; args?: unknown } }[];
    const calls = parts.flatMap((p, i) =>
      p.functionCall ? [{ id: p.functionCall.id ?? `${LOCAL}${i}-${p.functionCall.name}`, name: p.functionCall.name, args: p.functionCall.args ?? {} }] : [],
    );
    const reason = candidate.finishReason;
    return {
      text: parts.flatMap((p) => (p.text && !p.thought ? [p.text] : [])).join(""),
      calls,
      stop: calls.length ? "tools" : reason === "STOP" ? "end" : reason === "MAX_TOKENS" ? "length" : "other",
      raw: parts,
      usage: answer.usageMetadata && { input: answer.usageMetadata.promptTokenCount ?? 0, output: answer.usageMetadata.candidatesTokenCount ?? 0 },
    };
  },
};

export const providers: Record<ProviderId, Provider> = { anthropic, openai, gemini };
