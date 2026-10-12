// A deck made from a prompt, in an engine's worker of its own (PLAN G.5, ADR-0026): what
// `scaena generate` does, with `fetch`. The generator's own module (`scaena-generator`, loaded
// the first time) builds each request and reads each answer, reads the files and plans each
// link's GETs, and builds the deck the narrative makes in the theme; the engine's module makes it
// a bundle (`Making`), as the CLI makes one: the characters the fonts lack written plainly, lint's
// fixes applied, the texts that still run past their place rewritten by the model, and each sheet
// given kept as a data source. The user's key goes to the provider it is for, and a GitHub token
// to GitHub, and nowhere else.
import { Making } from "@scaena/wasm";
import type { Choices, Generate, Generated, GeneratingEvent, Health } from "./protocol";

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

/** The narrative health of `deck`, as `deck.json` holds it (PLAN G.9, SPEC §7.7). */
export async function health(deck: string): Promise<Health> {
  return JSON.parse((await generator()).health(deck)) as Health;
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
    const response = await fetch(request.url, { method: request.method, headers: request.headers, body });
    const after = response.headers.get("retry-after") ?? undefined;
    return { status: response.status, statusText: response.statusText, body: await response.text(), after };
  } catch (e) {
    return { status: 0, statusText: said(e), body: "" };
  }
}

/** A GET a link's plan asks for: its status, its content type, and its body. A host that lets
 * no other page read it (CORS) fails as a network does, and says so. */
async function get(asked: { url: string; headers: Record<string, string>; most: number }) {
  let response: Response;
  try {
    response = await fetch(asked.url, { headers: asked.headers });
  } catch {
    const host = new URL(asked.url).host;
    throw new Error(
      `${host} could not be read from this page: it is out of reach, or it lets no other page read it. Download what it serves and add the file, or give it to \`scaena generate\` or the Mac app`,
    );
  }
  const body = new Uint8Array(await response.arrayBuffer());
  if (body.length > asked.most) throw new Error(`${asked.url} answered with more than ${asked.most >> 20} MB`);
  return { status: response.status, kind: response.headers.get("content-type") ?? "", body };
}

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** The theme a deck is made in: its file's name, its text, and the fonts it names. */
export interface Theme {
  file: string;
  text: string;
  fonts: Map<string, Uint8Array>;
}

/** The deck `job` asks for, made in `theme`: the bundle's files by their paths, what it came
 * to, and its title. `hear` hears how far it has come. */
export async function generate(
  job: Generate,
  theme: Theme,
  hear: (event: GeneratingEvent) => void,
): Promise<{ files: Map<string, Uint8Array>; made: Generated; title: string }> {
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
    try {
      if (job.arc) generation.chooseArc(job.arc);
      for (let asked = generation.request(job.key); asked; asked = generation.request(job.key)) {
        const step = generation.step();
        hear(step === "arc" ? { kind: "choosing" } : { kind: "writing", slides: generation.target() });
        const reply = await send(JSON.parse(asked));
        const at = JSON.parse(generation.answer(reply.status, reply.statusText, reply.body)) as {
          arc?: string;
          fallback?: string | null;
          written?: number | null;
          truncated?: boolean | null;
        };
        if (step === "arc") hear({ kind: "chose", arc: at.arc ?? "", fallback: at.fallback ?? undefined });
        else hear({ kind: "wrote", slides: at.written ?? 0, truncated: at.truncated ?? false });
      }
      const painted = job.pictures ? await paint(g, generation, job.pictures, theme, hear) : undefined;
      hear({ kind: "making" });
      const built = generation.deck(theme.text, painted?.placed ?? "{}");
      const making = new Making(built, theme.file, theme.text, theme.fonts, painted?.bytes ?? new Map());
      try {
        if (painted) making.painted(painted.made, JSON.stringify(painted.unmade));
        if (job.look !== false) await look(generation, making, job.key, theme, painted?.placed ?? "{}", hear);
        await fit(generation, making, job.key, hear);
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
  const wanted = (JSON.parse(asked) as unknown[]).length;
  if (!wanted) return undefined;
  const listed = await send(JSON.parse(g.pictureModelsRequest(pictures.provider, pictures.key, pictures.base)));
  let model: string | undefined;
  try {
    model = (JSON.parse(g.pictureModels(pictures.provider, listed.status, listed.statusText, listed.body)) as string[])[0];
  } catch (e) {
    hear({ kind: "painted", pictures: 0, unmade: [`the image models could not be listed: ${said(e)}`] });
    return undefined;
  }
  if (!model) {
    hear({ kind: "painted", pictures: 0, unmade: ["the key lists no image model"] });
    return undefined;
  }
  hear({ kind: "painting", pictures: wanted, model });
  const settings = JSON.stringify({ provider: pictures.provider, model, base: pictures.base, quality: "medium" });
  const painting = new g.Painting(asked, settings);
  let finished: ReturnType<typeof painting.finish> | undefined;
  try {
    for (;;) {
      const requests = JSON.parse(painting.requests(pictures.key)) as Request[];
      if (!requests.length) break;
      const replies = await Promise.all(requests.map(send));
      for (const r of replies) painting.reply(r.status, r.statusText, r.body, r.after);
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
    return { placed: finished.placed(), bytes, made: bytes.size, unmade };
  } finally {
    finished?.free();
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
      const next = JSON.parse(researching.answer(job.key, reply.status, reply.statusText, reply.body)) as {
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

/** The deck shown to the model as it is drawn (PLAN G.13): each slide drawn by the editor's
 * module, a JPEG made of it here, and what the model asks to change (another of a slide's
 * layouts, its words tightened) built again by the generator and kept where lint finds no slide
 * worse for it; where it finds some, the deck is built once more without theirs. A look that
 * fails leaves the deck as it was, saying why. */
async function look(
  generation: InstanceType<Generator["Generation"]>,
  making: Making,
  key: string,
  theme: Theme,
  placed: string,
  hear: (event: GeneratingEvent) => void,
) {
  const layouts = JSON.stringify((JSON.parse(making.report()) as { layouts: string[] }).layouts);
  const seen = generation.seeing(theme.text, placed, layouts);
  const drawn = [];
  for (let i = 0; i < making.count; i++) drawn.push(await jpeg(making.pixels(i, LOOK_WIDTH)));
  const asked = generation.lookRequest(key, seen, JSON.stringify(drawn));
  if (!asked) return;
  hear({ kind: "looking", slides: drawn.length });
  const reply = await send(JSON.parse(asked));
  try {
    const changes = JSON.parse(generation.lookAnswer(reply.status, reply.statusText, reply.body, seen)) as {
      slide: number;
    }[];
    if (!changes.length) return hear({ kind: "looked", slides: 0 });
    const tried = JSON.parse(
      making.relook(generation.deckChanged(theme.text, placed, JSON.stringify(changes)), JSON.stringify(changes), "[]"),
    ) as "kept" | { worse: number[] };
    if (tried === "kept") return hear({ kind: "looked", slides: changes.length });
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
    if (again === "kept") return hear({ kind: "looked", slides: kept.length });
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

/** Each text that still runs past its place, rewritten by the model to the length the engine
 * measured for it; a rewrite that fails leaves the deck as it was, saying why. */
async function fit(
  generation: InstanceType<Generator["Generation"]>,
  making: Making,
  key: string,
  hear: (event: GeneratingEvent) => void,
) {
  const longs = making.longs();
  const texts = (JSON.parse(longs) as unknown[]).length;
  if (!texts) return;
  const asked = generation.fitRequest(key, longs);
  if (!asked) return;
  hear({ kind: "fitting", texts });
  const reply = await send(JSON.parse(asked));
  try {
    const rewrites = generation.fitAnswer(reply.status, reply.statusText, reply.body, longs);
    making.fitted(rewrites);
    hear({ kind: "fitted", texts: (JSON.parse(rewrites) as unknown[]).length });
  } catch (e) {
    making.unfitted(said(e));
    hear({ kind: "unfitted", why: said(e) });
  }
}
