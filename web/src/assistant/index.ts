// The assistant in the engine's worker (PLAN 2.6, SPEC §11): the user's question, with their
// own key, to the provider they chose; each tool the model calls runs on the bundle the worker
// holds, as the MCP server's tool of that name runs on one on disk (ADR-0009). The worker loads
// this the first time the user asks something, and it loads what the model reads (the MCP
// server's resources, `scaena-resources`) as a WASM module of its own: the editor's engine
// carries neither.
import initResources, { list, text } from "@scaena/resources";
import { findIcons, iconSet } from "../icon-set";
import type { Rewritten, Seeing } from "../protocol";
import { converse, type Event } from "./converse";
import { type Call, type Message, type ProviderId, providers, type Result } from "./providers";
import { type Listed, seen, system } from "./prompt";
import { summary, tools } from "./tools";

export type { Event } from "./converse";
export { providers } from "./providers";
export type { ProviderId } from "./providers";

/** The engine's session, as the assistant uses it. A tool is called by `author` at `at` (RFC
 * 3339): an edit it makes is theirs in the bundle's history (PLAN 2.9). */
export interface Session {
  tool(name: string, args: string, author?: string, at?: string): Called;
  source(): string;
  files(): string[];
  file(path: string): Uint8Array | undefined;
  states(): string[];
  formats(): string[];
  /** The theme frames are drawn in, as JSON `{ theme, text }`; none where the deck names none. */
  themeText(): string | undefined;
}

/** What a tool returned (`ToolResult`). */
export interface Called {
  readonly json: string;
  readonly error: boolean;
  readonly edited: boolean;
  /** The files it wrote beside the deck, before and after, as JSON: the theme `theme_edit` edited. */
  readonly rewritten: string;
  readonly size: Uint32Array | number[] | undefined;
  pixels(): Uint8ClampedArray;
  free(): void;
}

/** A question for the assistant, and who answers it; and what the editor shows as it is asked
 * (PLAN 2.52). */
export interface Asking {
  provider: ProviderId;
  model: string;
  key: string;
  base?: string;
  text: string;
  seeing?: Seeing;
}

/** The conversation, kept between questions until the page forgets it. */
let conversation: Message[] = [];
let loaded: Promise<unknown> | undefined;

/** Start a new conversation. */
export function forget() {
  conversation = [];
}

/** Ask `asking.text` of the model, which works on `session` with the tools `names` until it
 * answers, `signal` stops it, or it has called tools as many rounds as one question allows.
 * `edited` hears the deck's source after each call that changes it or a file beside it (the
 * theme), with that file before and after, before the next call. */
export async function ask(
  session: Session,
  names: string[],
  asking: Asking,
  emit: (e: Event) => void,
  edited: (source: string, files: Rewritten[]) => Promise<void>,
  signal: AbortSignal,
) {
  loaded ??= initResources();
  await loaded;
  const listed = JSON.parse(list()) as Listed[];
  const bundleSkills = session.files().flatMap((path) => /^skills\/([^/]+)\/SKILL\.md$/.exec(path)?.[1] ?? []);
  const prompt = system(facts(session), listed, bundleSkills, text("scaena://skills/author-deck") ?? "");
  conversation.push({ role: "user", text: seen(asking.seeing) + asking.text });
  await converse({
    provider: providers[asking.provider],
    model: asking.model,
    key: asking.key,
    base: asking.base,
    system: prompt,
    tools: tools(names),
    conversation,
    call: (call) => run(session, call, edited, `agent:${asking.model}`),
    emit,
    signal,
  });
}

/** Who writes: the provider, at its own address or at `base`, with the user's key, and the model
 * they picked; what it is told it does, `instructions`, and what it is asked, `prompt` (PLAN 2.101),
 * with the picture it is about, a PNG in base64, where it describes one (PLAN 2.119). */
export interface Writing {
  provider: ProviderId;
  model: string;
  key: string;
  base?: string;
  instructions: string;
  prompt: string;
  picture?: string;
}

/** Ask a model `writing.prompt` once, told `writing.instructions`, with nothing to call: what a
 * writing tool asks (PLAN 2.101). Its words, and the tokens it took; the conversation does not
 * keep it. */
export async function write(writing: Writing, signal?: AbortSignal): Promise<{ text: string; usage?: { input: number; output: number } }> {
  const turn = await providers[writing.provider].turn({
    model: writing.model,
    key: writing.key,
    base: writing.base,
    system: writing.instructions,
    tools: [],
    messages: [{ role: "user", text: writing.prompt, ...(writing.picture && { png: writing.picture }) }],
    signal,
  });
  return { text: turn.text, usage: turn.usage };
}

/** The open deck, in a few facts: its title, states, formats, and theme. */
function facts(session: Session) {
  const read = session.tool("deck_read", "{}");
  try {
    const deck = (JSON.parse(read.json) as { deck?: { meta?: { title?: string }; theme?: unknown } }).deck;
    const theme = typeof deck?.theme === "string" ? deck.theme.replace(/^.*\//, "").replace(/\.theme\.json$/, "") : undefined;
    return { title: deck?.meta?.title, states: session.states(), formats: session.formats(), theme };
  } finally {
    read.free();
  }
}

/** Run `call` on the session as `author`: what it returned, and a line saying what that came
 * to. A call that changes the deck, or the theme beside it, tells `edited` its source, and the
 * file it wrote, before it returns. A call that fails says why to the model, as an MCP tool's
 * error result does: every call has an answer. */
async function run(
  session: Session,
  call: Call,
  edited: (source: string, files: Rewritten[]) => Promise<void>,
  author: string,
): Promise<{ result: Result; summary: string }> {
  const answer = (json: string, error: boolean): { result: Result; summary: string } => ({
    result: { id: call.id, name: call.name, json, error },
    summary: summary(call.name, json, error),
  });
  if (call.name === "resource_read") return answer(...read(session, call.args));
  if (call.name === "icon_find") return answer(...(await iconFind(call.args)));
  let out: { result: Result; summary: string };
  let changed: boolean;
  let files: Rewritten[] = [];
  let frame: { pixels: Uint8ClampedArray; width: number; height: number } | undefined;
  try {
    const called = session.tool(call.name, JSON.stringify(call.args ?? {}), author, new Date().toISOString());
    try {
      out = answer(called.json, called.error);
      changed = called.edited;
      files = JSON.parse(called.rewritten || "[]") as Rewritten[];
      if (called.size) frame = { pixels: called.pixels(), width: called.size[0], height: called.size[1] };
    } finally {
      called.free();
    }
  } catch (e) {
    return answer(JSON.stringify({ message: e instanceof Error ? e.message : String(e) }), true);
  }
  if (frame) out.result.png = await png(frame.pixels, frame.width, frame.height);
  if (changed || files.length) await edited(session.source(), files);
  return out;
}

/** `resource_read`: a resource's text, or a skill the bundle carries; or why there is none. */
function read(session: Session, args: unknown): [string, boolean] {
  const uri = (args as { uri?: unknown } | undefined)?.uri;
  if (typeof uri !== "string") return [JSON.stringify({ message: "resource_read takes `uri`, a string" }), true];
  const skill = /^bundle:\/\/skills\/([^/]+)$/.exec(uri)?.[1];
  // The theme the deck is drawn in, as theme_edit edits it (PLAN 2.61).
  if (uri === "bundle://theme") {
    const held = session.themeText();
    if (held) return [(JSON.parse(held) as { text: string }).text, false];
  } else if (skill) {
    const bytes = session.file(`skills/${skill}/SKILL.md`);
    if (bytes) return [new TextDecoder().decode(bytes), false];
  } else {
    const found = text(uri);
    if (found !== undefined) return [found, false];
  }
  return [JSON.stringify({ message: `no resource \`${uri}\`: scaena://spec is the specification's index, and the system prompt lists the rest` }), true];
}

/** `icon_find`: the icons that ship (PLAN 2.104), found in the set the page carries beside it, by
 * the rule the MCP server finds them by, and returned as it returns them: the editor's module
 * carries none. Or why there are none. */
async function iconFind(args: unknown): Promise<[string, boolean]> {
  const a = (args ?? {}) as { query?: unknown; weight?: unknown; limit?: unknown };
  const known = Object.keys(a).every((k) => k === "query" || k === "weight" || k === "limit");
  const { query = "", weight = "regular", limit } = a;
  const whole = limit === undefined || limit === null || (typeof limit === "number" && Number.isInteger(limit) && limit >= 0);
  if (!known || typeof query !== "string" || (weight !== "regular" && weight !== "fill") || !whole) {
    return [JSON.stringify({ message: "icon_find takes `query`, words; `weight`, `regular` or `fill`; and `limit`, a whole number" }), true];
  }
  try {
    const set = await iconSet();
    const found = findIcons(set, query);
    const icons = found.slice(0, typeof limit === "number" ? limit : 20).map((e) => ({
      name: e.name,
      tags: e.tags,
      categories: e.categories,
      icon: { name: e.name, path: weight === "fill" ? e.fill : e.regular, view: set.view },
    }));
    return [JSON.stringify({ icons, found: found.length }), false];
  } catch (e) {
    return [JSON.stringify({ message: e instanceof Error ? e.message : String(e) }), true];
  }
}

/** RGBA pixels as a PNG, in base64: what a model takes as an image. */
async function png(pixels: Uint8ClampedArray, width: number, height: number): Promise<string> {
  const canvas = new OffscreenCanvas(width, height);
  canvas.getContext("2d")!.putImageData(new ImageData(new Uint8ClampedArray(pixels), width, height), 0, 0);
  const bytes = new Uint8Array(await (await canvas.convertToBlob({ type: "image/png" })).arrayBuffer());
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary);
}
