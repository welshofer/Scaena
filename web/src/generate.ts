// A deck made from a prompt, in an engine's worker of its own (PLAN G.5, ADR-0026): what
// `scaena generate` does, with `fetch`. The generator's own module (`scaena-generator`, loaded
// the first time) builds each request and reads each answer, reads the files and plans each
// link's GETs, and builds the deck the narrative makes in the theme; the engine's module makes it
// a bundle (`Making`), as the CLI makes one: the characters the fonts lack written plainly, lint's
// fixes applied, the texts that still run past their place rewritten by the model, and each sheet
// given kept as a data source. The user's key goes to the provider it is for, and a GitHub token
// to GitHub, and nowhere else. A deck in a script the theme's fonts do not draw takes the script's
// font, fetched from google/fonts at the commit the shipped fonts come from, held to its digest,
// and kept in the browser's cache for the next deck (PLAN G.22, ADR-0027).
import { Making, scriptChecked, scriptFor, scriptTaken, writable } from "@scaena/wasm";
import { findIcons, type IconSet, iconSet } from "./icon-set";
import { atMost } from "./most";
import type { Choices, FetchedMap, Generate, Generated, GeneratingEvent, Health, MapAsk, MapPlace, PictureJob, StoryOutline } from "./protocol";
import { answered } from "./words";

type Generator = typeof import("@scaena/generator");

let loaded: Promise<Generator> | undefined;

/** The generator's module, loaded and started the first time. */
function generator(): Promise<Generator> {
  loaded ??= import("@scaena/generator").then(async (module) => {
    await module.default();
    return module;
  });
  return loaded;
}

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
/** Who makes pictures, by name, as a person reads it. */
const PAINTERS = { openai: "OpenAI", gemini: "Gemini" } as const;

/** The narrative health of `deck`, as `deck.json` holds it (PLAN G.9, SPEC §7.7), beside `lint`, the
 * findings the last lint gave, as JSON (lift-up 10-24b STAB-1). */
export async function health(deck: string, lint?: string): Promise<Health> {
  return JSON.parse((await generator()).health(deck, lint)) as Health;
}

/** Where a slide goes in an open deck (lift-up FUNC-1): after the slide that holds a state, in
 * place of it, or, with neither, at the end. */
export interface SlidePlace {
  after?: string;
  replace?: string;
}

/** The generator's writer of one slide (lift-up FUNC-1), the bundle's data `files` handed to it:
 * which the rows of a chart the slide draws are read from. */
async function writing(files: Iterable<[string, Uint8Array]>) {
  const w = new (await generator()).SlideWriting();
  for (const [path, bytes] of files) w.add(path, bytes);
  return w;
}

/** What to ask a model for one slide of `deck` at `place`, with the person's `ask` (lift-up
 * FUNC-1), the bundle's data `files` read for the rows of a chart the slide draws: the request's
 * instructions and prompt. */
export async function oneAsked(
  deck: string,
  place: SlidePlace,
  ask: string,
  files: Iterable<[string, Uint8Array]>,
): Promise<{ system: string; prompt: string }> {
  const w = await writing(files);
  try {
    return JSON.parse(w.asked(deck, JSON.stringify(place), ask)) as { system: string; prompt: string };
  } finally {
    w.free();
  }
}

/** `slide` built for `deck` in `theme` at `place`, as the deck's generator builds a slide, its
 * icons `found` in the set the page carries and its data `files` the bundle's: what the editor's
 * module makes one patch of (`Player.slidePolishing`). */
export async function slideBuilt(
  deck: string,
  theme: string,
  slide: Record<string, unknown>,
  place: SlidePlace,
  found: Record<string, { name: string; path: string; view: number[] }>,
  files: Iterable<[string, Uint8Array]>,
): Promise<string> {
  const w = await writing(files);
  try {
    return w.built(deck, theme, JSON.stringify(slide), JSON.stringify(place), JSON.stringify(found));
  } finally {
    w.free();
  }
}

/** The one slide a model's `answer` holds, as Narrato writes a slide. */
export async function oneAnswered(answer: string): Promise<Record<string, unknown>> {
  return JSON.parse((await generator()).oneAnswered(answer)) as Record<string, unknown>;
}

/** What a brief may ask, as Narrato knows it. */
export async function choices(): Promise<Choices> {
  return JSON.parse((await generator()).choices()) as Choices;
}

/** A request a generation asks the client to make (`scaena_narrato::Request`). */
interface Request {
  method: string;
  url: string;
  headers: Record<string, string>;
  body?: unknown;
}

/** `request` made: its status, its status line's words, its body, and its `Retry-After`; status
 * 0 where it never came back, saying why, as the CLI reads a request that failed. */
async function send(request: Request): Promise<{ status: number; statusText: string; body: string; after?: string }> {
  try {
    const body = request.body == null ? undefined : JSON.stringify(request.body);
    const signal = AbortSignal.timeout(ANSWERED_WITHIN);
    const response = await fetch(request.url, { method: request.method, headers: request.headers, body, signal });
    const after = response.headers.get("retry-after") ?? undefined;
    return { status: response.status, statusText: response.statusText, body: await response.text(), after };
  } catch (e) {
    // A provider that takes the request and never answers is asked again, as a busy one is
    // (narrative REL-1), and the progress says why (lift-up REL-4).
    return { status: 0, statusText: late(e) ? LATE : said(e), body: "" };
  }
}

/** What a request that waited past its deadline is answered. */
const LATE = "The provider didn't answer in 15 minutes";

/** The providers that write a deck, by name. */
const WRITERS = { anthropic: "Anthropic", openai: "OpenAI", gemini: "Gemini" } as const;

/** What `take` reads of a provider's `reply`. Where the reply was a refusal, its error is said in
 * a person's words, as the pictures' are, "Anthropic didn't accept your key. Check it, or replace
 * it." in place of "401 Unauthorized: invalid x-api-key" (lift-up USE-3); the provider's own words
 * go to the console. An answer that came but did not read keeps its own words. */
function read<T>(provider: string, reply: { status: number; statusText: string; body: string }, take: () => T): T {
  try {
    return take();
  } catch (e) {
    if (reply.status >= 200 && reply.status < 300) throw e;
    console.warn(`${provider}: ${said(e)}`);
    const waited = reply.status === 0 && reply.statusText === LATE;
    throw new Error(waited ? `${provider} didn't answer in 15 minutes. Try again in a moment.` : answered(provider, reply.status, reply.body));
  }
}

/** How long a provider has to answer a request: 15 minutes, as the Mac's `URLSession` waits and
 * near the CLI's 20 (lift-up REL-4). */
const ANSWERED_WITHIN = 900_000;

/** How long a link's host has to answer a GET. */
const FETCHED_WITHIN = 60_000;

/** A GET a link's plan asks for: its status, its content type, and its body. A host that lets
 * no other page read it (CORS) fails as a network does, and says so. */
async function get(asked: { url: string; headers: Record<string, string>; most: number }) {
  let response: Response;
  try {
    response = await fetch(asked.url, { headers: asked.headers, signal: AbortSignal.timeout(FETCHED_WITHIN) });
  } catch (e) {
    const host = new URL(asked.url).host;
    if (late(e)) throw new Error(`${host} didn't answer in a minute`);
    throw new Error(
      `${host} could not be read from this page: it is out of reach, or it lets no other page read it. Download what it serves and add the file, or give it to \`scaena generate\` or the Mac app`,
    );
  }
  // The minute holds the body too, read no further than it may be: a host that answers at once
  // and sends slowly runs out of it here.
  let body: Uint8Array;
  try {
    body = await atMost(response, asked.most, asked.url);
  } catch (e) {
    if (!late(e)) throw e;
    const host = new URL(asked.url).host;
    throw new Error(`${host} took more than a minute to send ${asked.url}: download it and add the file`);
  }
  return { status: response.status, kind: response.headers.get("content-type") ?? "", body };
}

/** Whether `e` is a deadline's abort (`AbortSignal.timeout`). */
const late = (e: unknown) => e instanceof DOMException && e.name === "TimeoutError";

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** The theme a deck is made in: its file's name, its text, and the fonts it names. */
export interface Theme {
  file: string;
  text: string;
  fonts: Map<string, Uint8Array>;
}

/** A script's font, as `scriptFor` names it. */
interface Script {
  key: string;
  family: string;
  language: string;
  file: string;
  url: string;
  bytes: number;
}

/** Where the scripts' fonts are kept, by where they come from. */
const KEPT = "scaena-fonts";

/** `theme`, ready for a deck in `language`: where its fonts do not draw the language's letters,
 * with the script's font, kept here or else fetched, named last in its text families'
 * fallbacks (PLAN G.22). Asked before a file is read or a call is paid for. */
async function scripted(language: string | undefined, theme: Theme, hear: (event: GeneratingEvent) => void): Promise<Theme> {
  const named = scriptFor(language, theme.text, theme.fonts);
  if (!named) return theme;
  const script = JSON.parse(named) as Script;
  const bytes = await scriptFont(script, hear);
  const fonts = new Map(theme.fonts).set(script.file, bytes);
  return { file: theme.file, text: scriptTaken(theme.text, script.key), fonts };
}

/** `script`'s font: the one this browser keeps where it is still the font pinned, else fetched,
 * held to its digest, and kept. A browser that keeps nothing fetches it each time. */
async function scriptFont(script: Script, hear: (event: GeneratingEvent) => void): Promise<Uint8Array> {
  const cache = typeof caches === "undefined" ? undefined : await caches.open(KEPT).catch(() => undefined);
  const kept = await cache?.match(script.url).catch(() => undefined);
  if (kept) {
    const bytes = new Uint8Array(await kept.arrayBuffer());
    try {
      scriptChecked(script.key, bytes);
      return bytes;
    } catch {
      await cache?.delete(script.url).catch(() => false);
    }
  }
  hear({ kind: "fetchingFont", family: script.family, language: script.language, megabytes: script.bytes / 1e6 });
  const unfetched = (why: string) =>
    new Error(`a deck in ${script.language} is set in ${script.family}, which could not be fetched from ${script.url}: ${why}`);
  let response: Response;
  try {
    response = await fetch(script.url);
  } catch (e) {
    throw unfetched(said(e));
  }
  if (!response.ok) throw unfetched(`${response.status} ${response.statusText}`.trim());
  const bytes = new Uint8Array(await response.arrayBuffer());
  scriptChecked(script.key, bytes);
  await cache?.put(script.url, new Response(bytes)).catch(() => undefined);
  return bytes;
}

/** The outline as the model wrote it (narrative FUNC-1), shown for the person to settle: on `arc`,
 * chosen for `reasoning`, or the heuristic's where `fallback` says why. */
export interface Outlined {
  outline: StoryOutline;
  arc: string;
  reasoning: string;
  fallback?: string;
}

/** What the person decided of an outline: settled, so the slides are written to it; or another
 * arc, by name or the heuristic's where none, to outline again. */
export type Decision = { settled: StoryOutline } | { arc?: string };

/** The deck `job` asks for, made in `given`: the bundle's files by their paths, what it came
 * to, and its title. `hear` hears how far it has come. With `job.outline`, the outline is
 * written first and `settle` shows it to the person, whose decision the slides follow (narrative
 * FUNC-1); without, the slides are written at once. */
export async function generate(
  job: Generate,
  given: Theme,
  hear: (event: GeneratingEvent) => void,
  settle?: (outlined: Outlined) => Promise<Decision>,
): Promise<{ files: Map<string, Uint8Array>; made: Generated; title: string }> {
  // Before a file is read or a call is paid for: the script's font where the theme's fonts do
  // not draw the language (PLAN G.22), and a deck in letters they still cannot draw refused, as
  // it would be blank where its words stand (PLAN G.21).
  const theme = await scripted(job.brief.language || undefined, given, hear);
  writable(job.brief.language || undefined, theme.text, theme.fonts);
  const g = await generator();
  const sources = new g.Sources();
  const asking = JSON.stringify({ provider: job.provider, model: job.model, base: job.base });
  const brief = JSON.stringify(job.brief);
  try {
    for (const file of job.files) {
      hear({ kind: "reading", name: file.name });
      const read = JSON.parse(sources.add(file.name, new Uint8Array(file.bytes))) as { name: string; kind: string };
      hear({ kind: "read", name: read.name, what: read.kind });
    }
    for (const link of job.links) {
      hear({ kind: "reading", name: link });
      const fetching = g.Sources.fetching(link, job.github);
      if (!fetching) throw new Error(`${link}: a file is added as a file, not as a link`);
      for (let asked = fetching.request(); asked; asked = fetching.request()) {
        const got = await get(JSON.parse(asked));
        fetching.answer(got.status, got.kind, got.body);
      }
      const read = JSON.parse(sources.take(fetching)) as { name: string; kind: string };
      hear({ kind: "read", name: read.name, what: read.kind });
    }
    if (job.research) await research(g, brief, asking, job, sources, hear);
    const generation = new g.Generation(brief, asking, sources);
    // What each source gave, said before a call is paid for (narrative USE-5): the sources share
    // a million bytes, the first given taking it first, and one cut short, or given no room, says
    // so here, not after the deck is made.
    for (const given of JSON.parse(generation.given()) as Generated["sources"]) {
      hear({ kind: "grounded", name: given.name, what: given.kind, kilobytes: Math.ceil(given.bytes / 1024), cut: given.truncated });
    }
    try {
      const outlining = Boolean(job.outline && settle);
      if (job.arc) generation.chooseArc(job.arc);
      if (job.arc && outlining) generation.outlineFirst();
      for (let asked = generation.request(job.key); asked; asked = generation.request(job.key)) {
        const step = generation.step();
        hear(step === "arc" ? { kind: "choosing" } : step === "outline" ? { kind: "outlining" } : { kind: "writing", slides: generation.target() });
        const reply = await send(JSON.parse(asked));
        const answer = read(WRITERS[job.provider], reply, () => generation.answer(reply.status, reply.statusText, reply.body, reply.after));
        const at = JSON.parse(answer) as {
          arc?: string;
          reasoning?: string | null;
          fallback?: string | null;
          written?: number | null;
          truncated?: boolean | null;
          outline?: StoryOutline | null;
          wait?: number | null;
          tried?: number | null;
        };
        // An answer for now (busy, a server's failure, none): the same step asked again after
        // the wait the generator says, up to six times (narrative REL-1).
        if (at.wait != null) {
          hear({ kind: "retrying", seconds: at.wait, tried: at.tried ?? 0 });
          await new Promise((r) => setTimeout(r, at.wait! * 1000));
          continue;
        }
        if (step === "arc") {
          hear({ kind: "chose", arc: at.arc ?? "", fallback: at.fallback ?? undefined });
          // The outline before the slides (narrative FUNC-1): the next request is the outline's.
          if (outlining) generation.outlineFirst();
        } else if (step === "outline") {
          const outline = at.outline ?? { bigIdea: "", beats: [] };
          hear({ kind: "outlined", beats: outline.beats.length });
          const decided = await settle!({ outline, arc: at.arc ?? "", reasoning: at.reasoning ?? "", fallback: at.fallback ?? undefined });
          if ("settled" in decided) {
            const now = JSON.parse(generation.settle(JSON.stringify(decided.settled))) as { outline?: StoryOutline | null };
            hear({ kind: "settled", beats: now.outline?.beats.length ?? 0 });
          } else {
            // Another arc, chosen with no call, and outlined again.
            generation.chooseArc(decided.arc || undefined);
            generation.outlineFirst();
            const chosen = JSON.parse(generation.standing()) as { arc?: string };
            hear({ kind: "chose", arc: chosen.arc ?? decided.arc ?? "" });
          }
        } else hear({ kind: "wrote", slides: at.written ?? 0, truncated: at.truncated ?? false });
      }
      await giveIcons(generation, hear);
      const painted = job.pictures ? await paint(g, generation, job.pictures, theme, hear) : undefined;
      hear({ kind: "making" });
      const built = generation.deck(theme.text, painted?.placed ?? "{}");
      const making = new Making(built, theme.file, theme.text, theme.fonts, painted?.bytes ?? new Map());
      try {
        if (painted) making.painted(painted.made, JSON.stringify(painted.unmade));
        const name = WRITERS[job.provider];
        if (job.look !== false) await look(generation, making, job.key, name, theme, painted?.placed ?? "{}", hear);
        await fit(generation, making, job.key, name, hear);
        making.keep(sources.tables());
        const made = { ...JSON.parse(making.report()), sources: JSON.parse(generation.given()) } as Generated;
        const files = making.files();
        if (!files) {
          const errors = made.created.findings.filter((f) => f.severity === "error").map((f) => `${f.code} ${f.message}`);
          throw new Error(`the deck made would not validate: ${errors.join("; ")}`);
        }
        const deck = JSON.parse(new TextDecoder().decode(files.get("deck.json"))) as { meta?: { title?: string } };
        return { files, made, title: deck.meta?.title || job.brief.topic };
      } finally {
        making.free();
      }
    } finally {
      generation.free();
    }
  } finally {
    sources.free();
  }
}

/** The icons the narrative's slides name (PLAN G.23), each the first its word finds in the set
 * that ships, in its regular weight, as `scaena_ops::narrative::icons_found` finds them: the set
 * fetched only where a slide names one. A word that finds none is left out, and its slide set
 * without icons. A set that cannot be fetched finds none, which `hear` hears: the slides are
 * already paid for, and an icon is a decoration (lift-up 10-25 STAB-3). */
async function giveIcons(generation: InstanceType<Generator["Generation"]>, hear: (event: GeneratingEvent) => void) {
  const words = JSON.parse(generation.iconWords()) as string[];
  if (words.length === 0) return;
  let set: IconSet;
  try {
    set = await iconSet();
  } catch (e) {
    hear({ kind: "unIconed", why: said(e) });
    return generation.giveIcons("{}");
  }
  const found: Record<string, { name: string; path: string; view: number[] }> = {};
  for (const word of words) {
    const [first] = findIcons(set, word);
    if (first) found[word] = { name: first.name, path: first.regular, view: set.view };
  }
  generation.giveIcons(JSON.stringify(found));
}

/** The pictures the narrative's slides ask for (PLAN G.10), made by the user's image model, the
 * best its key lists: every one at once, each round's answers read by the generator, which says
 * how long to wait before asking a busy provider again. None where the key lists no image model,
 * which the progress says. */
async function paint(
  g: Generator,
  generation: InstanceType<Generator["Generation"]>,
  pictures: NonNullable<Generate["pictures"]>,
  theme: Theme,
  hear: (event: GeneratingEvent) => void,
): Promise<{ placed: string; bytes: Map<string, Uint8Array>; made: number; unmade: string[] } | undefined> {
  const asked = generation.pictures(theme.text, pictures.look);
  if (!(JSON.parse(asked) as unknown[]).length) return undefined;
  return painting(g, asked, pictures, hear);
}

/** The pictures `asked` (JSON, as `Painting` takes them) made by the best image model the key
 * lists at `pictures.provider`, every one at once, each round's answers read by the generator,
 * which says how long to wait before asking a busy provider again; `heard` hears each answer.
 * None where the key lists no image model, which `hear` hears. */
async function painting(
  g: Generator,
  asked: string,
  pictures: { provider: "openai" | "gemini"; key: string; base?: string; quality?: "low" | "medium" | "high" },
  hear: (event: GeneratingEvent) => void,
  heard?: (reply: { status: number; body: string }) => void,
): Promise<{ placed: string; bytes: Map<string, Uint8Array>; made: number; unmade: string[]; model: string } | undefined> {
  const wanted = (JSON.parse(asked) as unknown[]).length;
  const named = PAINTERS[pictures.provider];
  const listed = await send(JSON.parse(g.pictureModelsRequest(pictures.provider, pictures.key, pictures.base)));
  // A key the provider does not take, no way to it, or an answer that does not read, in a person's
  // words (PLAN 3.18), as the Mac says it, not in the provider's own nor in what the answer holds.
  const unlisted = () => hear({ kind: "painted", pictures: 0, unmade: [answered(named, listed.status, listed.body)] });
  if (listed.status < 200 || listed.status >= 300) {
    unlisted();
    return undefined;
  }
  let model: string | undefined;
  try {
    model = (JSON.parse(g.pictureModels(pictures.provider, listed.status, listed.statusText, listed.body)) as string[])[0];
  } catch {
    unlisted();
    return undefined;
  }
  if (!model) {
    hear({ kind: "painted", pictures: 0, unmade: [`your ${named} key lists no image model`] });
    return undefined;
  }
  hear({ kind: "painting", pictures: wanted, model });
  const settings = JSON.stringify({ provider: pictures.provider, model, base: pictures.base, quality: pictures.quality ?? "medium" });
  const painting = new g.Painting(asked, settings);
  let finished: ReturnType<typeof painting.finish> | undefined;
  try {
    for (;;) {
      const requests = JSON.parse(painting.requests(pictures.key)) as Request[];
      if (!requests.length) break;
      const replies = await Promise.all(requests.map(send));
      for (const r of replies) {
        painting.reply(r.status, r.statusText, r.body, r.after);
        heard?.(r);
      }
      const pause = painting.round();
      if (pause > 0) await wait(pause * 1000);
    }
    finished = painting.finish();
    const bytes = new Map<string, Uint8Array>();
    for (const path of finished.paths()) {
      const b = finished.bytes(path);
      if (b) bytes.set(path, b);
    }
    const unmade = JSON.parse(finished.unmade()) as string[];
    hear({ kind: "painted", pictures: bytes.size, unmade });
    return { placed: finished.placed(), bytes, made: bytes.size, unmade, model };
  } finally {
    finished?.free();
  }
}

/** One picture a person asks for in the editor (PLAN 2.107): of `job.subject`, made as `job.look`
 * in `theme`'s palette (the theme's JSON) for a place `job.width` × `job.height`, by the best image
 * model the key lists, as a made deck's pictures are. Its bytes, its file's kind (`png`, `jpg`),
 * what it shows, and the model that made it; or an error that says why none was made. */
export async function picture(
  job: PictureJob,
  theme: string,
): Promise<{ bytes: Uint8Array; ext: string; alt: string; model: string }> {
  const g = await generator();
  const asked = g.pictureAsked(theme, job.subject, job.look, job.width, job.height);
  let why: string | undefined;
  const seen: { last?: { status: number; body: string } } = {};
  const made = await painting(
    g,
    asked,
    job,
    (event) => {
      if (event.kind === "painted" && event.unmade.length) why = event.unmade.join("; ");
    },
    (reply) => (seen.last = reply),
  );
  // Why none was made: what the provider answered in its place, said by what it means (PLAN
  // 3.18), as the Mac says it, never in the provider's words; else the generator's reason.
  const unmade = () => new Error(seen.last ? answered(PAINTERS[job.provider], seen.last.status, seen.last.body) : (why ?? "no picture was made"));
  if (!made) throw unmade();
  const [path, bytes] = [...made.bytes][0] ?? [];
  if (!path || !bytes) throw unmade();
  const alt = (JSON.parse(made.placed) as Record<string, { alt: string }>)["0"]?.alt ?? job.subject.trim();
  return { bytes, ext: path.replace(/^.*\./, ""), alt, model: made.model };
}

/** A map from Geoapify (ADR-0030, PLAN O.1c), fetched by the generator's plan with the user's
 * `key`, each GET made here by `fetch`, the key in the slot the request names and never written
 * down: the places a name matched, to pick one of, or the map. A refusal is said in Geoapify's
 * own words; no answer, as the browser says why. */
export async function fetchMap(
  ask: MapAsk,
  key: string,
): Promise<{ places?: MapPlace[]; map?: FetchedMap }> {
  const g = await generator();
  const plan = new g.MapFetching(JSON.stringify(ask));
  try {
    let get = JSON.parse(plan.first()) as { url: string; headers: [string, string][]; most: number };
    for (;;) {
      let response: Response;
      try {
        response = await fetch(g.keyedUrl(JSON.stringify(get), key), { headers: Object.fromEntries(get.headers) });
      } catch {
        // The browser says no more than that it failed, which names no key.
        throw new Error(navigator.onLine === false ? "this browser is offline" : "Geoapify could not be reached");
      }
      const body = new Uint8Array(await response.arrayBuffer());
      if (body.length > get.most) throw new Error("Geoapify answered with more than a map holds");
      const kind = response.headers.get("content-type") ?? "";
      const at = new Date().toISOString().replace(/\.\d+Z$/, "Z");
      const step = JSON.parse(plan.answer(response.status, kind, body, at)) as {
        get?: typeof get;
        choose?: MapPlace[];
        done?: { fetched: unknown; pin: [number, number]; place: string };
      };
      if (step.get) get = step.get;
      else if (step.choose) return { places: step.choose };
      else if (step.done) {
        const png = plan.png().slice().buffer as ArrayBuffer;
        return { map: { ...step.done, png } };
      } else throw new Error("the map's plan said nothing");
    }
  } finally {
    plan.free();
  }
}

/** Deep research on the brief's topic (PLAN G.8), its report kept among `sources`: Gemini's agent
 * polled as the generator says, or Anthropic's search resumed, each wait kept here. */
async function research(
  g: Generator,
  brief: string,
  asking: string,
  job: Generate,
  sources: InstanceType<Generator["Sources"]>,
  hear: (event: GeneratingEvent) => void,
) {
  const researching = new g.Research(brief, asking);
  try {
    hear({ kind: "researching", topic: job.brief.topic });
    let asked = JSON.parse(researching.start(job.key)) as Request;
    let doing: string | undefined;
    for (;;) {
      const reply = await send(asked);
      const answer = read(WRITERS[job.provider], reply, () => researching.answer(job.key, reply.status, reply.statusText, reply.body));
      const next = JSON.parse(answer) as {
        send?: Request;
        wait?: number;
        done?: { sources: number; cut: boolean };
        progress?: string | null;
      };
      if (next.progress && next.progress !== doing) hear({ kind: "researchingNow", doing: (doing = next.progress) });
      if (next.done) {
        hear({ kind: "researched", ...next.done });
        break;
      }
      if (next.wait) await wait(next.wait);
      asked = next.send!;
    }
    sources.research(researching);
  } finally {
    researching.free();
  }
}

/** How wide each slide is drawn for the model to see, in pixels (`scaena_ops::narrative::LOOK_WIDTH`). */
const LOOK_WIDTH = 1024;
/** The most slides shown to the model in one look request (`scaena_ops::narrative::LOOK_BATCH`,
 * narrative PERF-5): a long deck goes in rounds, each small enough to upload and under a
 * provider's limit on images in one request. */
const LOOK_BATCH = 12;

/** How many of the look's rounds are asked at once, as `scaena_ops::narrative::LOOKS_AT_ONCE` (lift-up PERF-1). */
const LOOKS_AT_ONCE = 3;

/** What each of `requests` answered, a few asked at once (`LOOKS_AT_ONCE`), and those a busy
 * provider answered for now asked again after the wait the slides and pictures take, up to its
 * tries, the wait said: as `scaena_ops::narrative::asked_again` (lift-up REL-1). Each reply in its
 * request's place. */
async function askedAgain(
  generation: InstanceType<Generator["Generation"]>,
  requests: Request[],
  hear: (event: GeneratingEvent) => void,
): Promise<Awaited<ReturnType<typeof send>>[]> {
  const replies: Awaited<ReturnType<typeof send>>[] = new Array(requests.length);
  let open = requests.map((_, i) => i);
  for (let tried = 0; open.length; tried++) {
    const got: Awaited<ReturnType<typeof send>>[] = [];
    for (let at = 0; at < open.length; at += LOOKS_AT_ONCE) {
      got.push(...(await Promise.all(open.slice(at, at + LOOKS_AT_ONCE).map((i) => send(requests[i])))));
    }
    let wait = 0;
    const again: number[] = [];
    open.forEach((i, k) => {
      replies[i] = got[k];
      const w = generation.retryAfter(got[k].status, got[k].body, got[k].after, tried);
      if (w !== undefined) {
        wait = Math.max(wait, w);
        again.push(i);
      }
    });
    if (!again.length) break;
    hear({ kind: "retrying", seconds: wait, tried: tried + 1 });
    await new Promise((r) => setTimeout(r, wait * 1000));
    open = again;
  }
  return replies;
}

/** The deck shown to the model as it is drawn (PLAN G.13): each slide drawn by the editor's
 * module, a JPEG made of it here, and what the model asks to change (another of a slide's
 * layouts, its words tightened) built again by the generator and kept where lint finds no slide
 * worse for it; where it finds some, the deck is built once more without theirs. A look that
 * fails leaves the deck as it was, saying why. */
async function look(
  generation: InstanceType<Generator["Generation"]>,
  making: Making,
  key: string,
  name: string,
  theme: Theme,
  placed: string,
  hear: (event: GeneratingEvent) => void,
) {
  const layouts = JSON.stringify((JSON.parse(making.report()) as { layouts: string[] }).layouts);
  const seen = generation.seeing(theme.text, placed, layouts);
  // A slide that would not draw for the model to see, as a deck the model asked for that cannot be
  // made again, leaves the deck as it was, saying why (narrative STAB-3): the look is a polish.
  try {
    const drawn = [];
    for (let i = 0; i < making.count; i++) drawn.push(await jpeg(making.pixels(i, LOOK_WIDTH)));
    // A dozen slides a round (narrative PERF-5), each numbered as it stands in the deck; the
    // changes answered folded into one, and a round not answered losing its own slides' changes
    // alone.
    const slides = JSON.parse(seen) as { slide: number }[];
    const changes: { slide: number }[] = [];
    let unanswered: string | undefined;
    // Every round asked at once, a few at a time so that no provider's rate limit is tripped,
    // each told the slides just outside it, and each answer read in its round's order (lift-up
    // PERF-1).
    const rounds: { round: { slide: number }[]; part: string; asked: string }[] = [];
    for (let start = 0; start < slides.length; start += LOOK_BATCH) {
      const round = slides.slice(start, start + LOOK_BATCH);
      const part = JSON.stringify(round);
      const edges = JSON.stringify([slides[start - 1] ?? null, slides[start + LOOK_BATCH] ?? null]);
      const asked = generation.lookRequest(key, part, JSON.stringify(drawn.slice(start, start + LOOK_BATCH)), edges);
      if (asked) rounds.push({ round, part, asked });
    }
    const looking = rounds.length > 0;
    if (looking) hear({ kind: "looking", slides: drawn.length });
    const replies = await askedAgain(generation, rounds.map((r) => JSON.parse(r.asked) as Request), hear);
    for (const [i, { round, part }] of rounds.entries()) {
      const reply = replies[i];
      try {
        changes.push(...(JSON.parse(read(name, reply, () => generation.lookAnswer(reply.status, reply.statusText, reply.body, part))) as { slide: number }[]));
      } catch (e) {
        const which = round.length === 1 ? `slide ${round[0].slide}` : `slides ${round[0].slide}\u2013${round[round.length - 1].slide}`;
        const why = `${which} not looked at: ${said(e)}`;
        unanswered = unanswered ? `${unanswered}; ${why}` : why;
      }
    }
    if (!looking) return;
    if (!changes.length) {
      if (unanswered) {
        making.unlooked(unanswered);
        return hear({ kind: "unlooked", why: unanswered });
      }
      return hear({ kind: "looked", slides: 0 });
    }
    const tried = JSON.parse(
      making.relook(generation.deckChanged(theme.text, placed, JSON.stringify(changes)), JSON.stringify(changes), "[]"),
    ) as "kept" | { worse: number[] };
    if (tried === "kept") {
      if (unanswered) making.unlooked(unanswered);
      return hear({ kind: "looked", slides: changes.length });
    }
    const kept = changes.filter((c) => !tried.worse.includes(c.slide - 1));
    if (!tried.worse.length || !kept.length) {
      const why = tried.worse.length
        ? `lint found the slides the model changed worse for it`
        : "the deck the model asked for would not validate";
      making.unlooked(why);
      return hear({ kind: "unlooked", why });
    }
    const again = JSON.parse(
      making.relook(
        generation.deckChanged(theme.text, placed, JSON.stringify(kept)),
        JSON.stringify(kept),
        JSON.stringify(tried.worse),
      ),
    ) as "kept" | { worse: number[] };
    if (again === "kept") {
      if (unanswered) making.unlooked(unanswered);
      return hear({ kind: "looked", slides: kept.length });
    }
    making.unlooked("lint found a slide the model changed worse for it");
    hear({ kind: "unlooked", why: "lint found a slide the model changed worse for it" });
  } catch (e) {
    making.unlooked(said(e));
    hear({ kind: "unlooked", why: said(e) });
  }
}

/** `pixels`, a slide drawn, as a JPEG the model is shown: its media type and its bytes in
 * base64. */
async function jpeg(pixels: ImageData): Promise<{ mediaType: string; data: string }> {
  const canvas = new OffscreenCanvas(pixels.width, pixels.height);
  canvas.getContext("2d")!.putImageData(pixels, 0, 0);
  const blob = await canvas.convertToBlob({ type: "image/jpeg", quality: 0.82 });
  const bytes = new Uint8Array(await blob.arrayBuffer());
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return { mediaType: "image/jpeg", data: btoa(binary) };
}

/** How many times the model is asked to rewrite what runs past its place, as
 * `scaena_narrato::fit::ROUNDS`: once, then once more for what its rewrite left too long. */
const FIT_ROUNDS = 2;

/** Each text that still runs past its place, rewritten by the model to the length the engine
 * measured for it, and asked again once for what the rewrite left too long; a rewrite that fails
 * leaves the deck as it was, saying why, and what still runs over is said (STAB-4). */
async function fit(
  generation: InstanceType<Generator["Generation"]>,
  making: Making,
  key: string,
  name: string,
  hear: (event: GeneratingEvent) => void,
) {
  for (let round = 1; round <= FIT_ROUNDS; round++) {
    const longs = making.longs();
    const texts = (JSON.parse(longs) as unknown[]).length;
    if (!texts) return;
    const asked = generation.fitRequest(key, longs);
    if (!asked) return;
    hear({ kind: "fitting", texts });
    // A busy provider asked again for the fit, as for the look (lift-up REL-1).
    const [reply] = await askedAgain(generation, [JSON.parse(asked) as Request], hear);
    try {
      const rewrites = read(name, reply, () => generation.fitAnswer(reply.status, reply.statusText, reply.body, longs));
      making.fitted(rewrites);
      hear({ kind: "fitted", texts: (JSON.parse(rewrites) as unknown[]).length });
    } catch (e) {
      making.unfitted(said(e));
      hear({ kind: "unfitted", why: said(e) });
      return;
    }
  }
  making.stillLong();
}
