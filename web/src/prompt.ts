// New from a prompt (PLAN G.5, ADR-0026): a deck written by Vzestup's Narrato model from what the
// user asks, grounded in the files and links they give, with their own key, as `scaena generate`
// makes one. It is made in an engine's worker of its own, which starts as the dialog opens and
// loads the generator's module while the brief is written; Cancel stops it, and the deck open is
// untouched until the new one opens, kept nowhere until it is saved.
import { familySections } from "./theme-families";
import { load, settings } from "./assistant/keys";
import type { Choices, FromWorker, Generate, Generated, GeneratingEvent, Source, ToWorker } from "./protocol";
import type { Engine } from "./stage";

const $ = <T extends HTMLElement>(selector: string) => document.querySelector<T>(selector)!;

/** How far the deck has come, in words, as `scaena generate` says it. */
export function said(event: GeneratingEvent): string {
  switch (event.kind) {
    case "fetchingFont":
      return `Fetching ${event.family}, the font a deck in ${event.language} is set in (${event.megabytes.toFixed(1)} MB)…`;
    case "reading":
      return `Reading ${event.name}…`;
    case "read":
      return `Read ${event.name} (${event.what})`;
    case "grounded":
      if (event.kilobytes === 0) return `No room for ${event.name} (${event.what}): the sources before it took the million bytes they share`;
      return `Grounded in ${event.name} (${event.what}, ${event.kilobytes} KB${event.cut ? ", cut short to fit" : ""})`;
    case "researching":
      return `Researching ${event.topic}… (minutes)`;
    case "researchingNow":
      return event.doing;
    case "researched":
      return `Researched: ${event.sources} sources${event.cut ? ", cut short" : ""}`;
    case "choosing":
      return "Choosing the arc…";
    case "chose":
      return event.fallback ? `Arc: ${event.arc}, the heuristic's (${event.fallback})` : `Arc: ${event.arc}`;
    case "writing":
      return `Writing the slides, at least ${event.slides}…`;
    case "wrote":
      return `${event.slides} slides written${event.truncated ? ", the answer cut short" : ""}`;
    case "painting":
      return `Making ${event.pictures} pictures with ${event.model}… (a minute or two)`;
    case "painted":
      return event.unmade.length
        ? `${event.pictures} pictures made; not made: ${event.unmade.join("; ")}`
        : `${event.pictures} pictures made`;
    case "making":
      return "Making the deck in the theme…";
    case "looking":
      return `Showing the model the ${event.slides} slides as drawn…`;
    case "looked":
      return event.slides
        ? `${event.slides} ${event.slides === 1 ? "slide" : "slides"} changed by what the model saw`
        : "The model saw nothing to change";
    case "unlooked":
      return `Not changed by what the model saw: ${event.why}`;
    case "fitting":
      return `${event.texts} ${event.texts === 1 ? "text runs" : "texts run"} past ${event.texts === 1 ? "its place" : "their places"}: asking for them shorter…`;
    case "fitted":
      return `${event.texts} rewritten to fit`;
    case "unfitted":
      return `Not rewritten to fit: ${event.why}`;
  }
}

/** What was made, in a line for the status bar. */
export function made(m: Generated): string {
  const fixed = m.fixed.length ? `, ${m.fixed.length} ${m.fixed.length === 1 ? "issue" : "issues"} fixed` : "";
  const errors = m.created.errors ? `, ${m.created.errors} ${m.created.errors === 1 ? "problem" : "problems"} to look at` : "";
  const pictures = m.pictures ? `, ${m.pictures} ${m.pictures === 1 ? "picture" : "pictures"}` : "";
  const looked = m.looked?.length ? `, ${m.looked.length} changed by what the model saw` : "";
  return `Made: ${m.layouts.length} slides on the arc ${m.arc}${pictures}${looked}${fixed}${errors}`;
}

/** A source given: a file, or a link. */
type Given = { file: File } | { link: string };

/** Wire the dialog: `opener` opens it, and a deck made opens by `open`, its line by `tell`.
 * Changes not saved are asked about before a deck is made, not as it opens: what a model wrote
 * is never thrown away for them. */
export function prompting(
  engine: Engine,
  opener: HTMLElement,
  dirty: () => boolean,
  open: (source: Source) => Promise<void>,
  tell: (line: string) => void,
) {
  const dialog = $<HTMLDialogElement>("#prompting");
  const form = dialog.querySelector("form")!;
  const topic = $<HTMLTextAreaElement>("#prompting-topic");
  const audience = $<HTMLInputElement>("#prompting-audience");
  const goal = $<HTMLSelectElement>("#prompting-goal");
  const length = $<HTMLSelectElement>("#prompting-length");
  const theme = $<HTMLSelectElement>("#prompting-theme");
  const modality = $<HTMLSelectElement>("#prompting-modality");
  const outcome = $<HTMLInputElement>("#prompting-outcome");
  const language = $<HTMLInputElement>("#prompting-language");
  const voice = $<HTMLInputElement>("#prompting-voice");
  const arc = $<HTMLSelectElement>("#prompting-arc");
  const github = $<HTMLInputElement>("#prompting-github");
  const research = $<HTMLInputElement>("#prompting-research");
  const pictures = $<HTMLSelectElement>("#prompting-pictures");
  const look = $<HTMLSelectElement>("#prompting-look");
  const painter = $<HTMLParagraphElement>("#prompting-painter");
  const files = $<HTMLInputElement>("#prompting-files");
  const link = $<HTMLInputElement>("#prompting-link");
  const list = $<HTMLUListElement>("#prompting-sources");
  const who = $<HTMLParagraphElement>("#prompting-who");
  const progress = $<HTMLOListElement>("#prompting-progress");
  const go = $<HTMLButtonElement>("#prompting-go");
  theme.replaceChildren(...familySections());

  let given: Given[] = [];
  /** The worker the deck is made in, from the dialog's opening until it closes. */
  let worker: Worker | undefined;
  let running = false;
  let asking: { provider: Generate["provider"]; model: string; base?: string; key?: string } | undefined;
  /** The keys kept for the image providers, by provider. */
  let painters: Partial<Record<"openai" | "gemini", { key: string; base?: string }>> = {};

  const shown = () => {
    list.replaceChildren(
      ...given.map((g, i) => {
        const item = document.createElement("li");
        item.textContent = "file" in g ? g.file.name : g.link;
        const remove = document.createElement("button");
        remove.type = "button";
        remove.textContent = "Remove";
        remove.setAttribute("aria-label", `Remove ${item.textContent}`);
        remove.onclick = () => {
          given = given.filter((_, j) => j !== i);
          shown();
        };
        item.append(" ", remove);
        return item;
      }),
    );
  };

  const fill = (choices: Choices) => {
    const options = (select: HTMLSelectElement, values: [string, string][], chosen: string) => {
      select.replaceChildren(...values.map(([value, label]) => new Option(label, value, false, value === chosen)));
    };
    options(goal, choices.goals.map((g) => [g, g]), choices.defaults.goal);
    // A length as the brief shows it, its range with an en dash, `5–10 slides`, as the Mac shows it;
    // what is chosen stays Narrato's own words, `5-10 slides`, which its prompts carry byte for
    // byte (ADR-0026).
    options(length, choices.lengths.map((l) => [l, l.replaceAll("-", "\u2013")]), choices.defaults.length);
    options(modality, choices.modalities.map((m) => [m.id, m.label]), choices.modalities[0]?.id ?? "");
    options(arc, [["", "The model's choice"], ...choices.arcs.map((a): [string, string] => [a, a])], "");
    audience.placeholder = choices.defaults.audience;
    $<HTMLDataListElement>("#prompting-audiences").replaceChildren(...choices.audiences.map((a) => new Option(a)));
  };

  /** Who makes the pictures, as chosen: the provider, with the key kept for it; none where none is
   * kept, which the dialog says. */
  const whoPaints = () => {
    const chosen = pictures.value as "openai" | "gemini" | "";
    look.disabled = running || !chosen;
    if (!chosen) {
      painter.textContent = "No pictures: each slide is set in a layout that waits for none.";
    } else if (!painters[chosen]) {
      const name = chosen === "openai" ? "OpenAI" : "Gemini";
      painter.textContent = `No ${name} key is kept: keep one in the Assistant tab for pictures, or choose None.`;
    } else {
      const name = chosen === "openai" ? "OpenAI" : "Gemini";
      painter.textContent = `Pictures by ${name}'s best image model your key lists, two in five slides, ten at most; your key goes to ${name} alone.`;
    }
  };
  pictures.onchange = whoPaints;

  /** Who writes the deck: the assistant's provider and model, with the key kept for it. */
  const whoWrites = async () => {
    const s = settings();
    painters = {};
    for (const p of ["openai", "gemini"] as const) {
      const k = await load(p).catch(() => undefined);
      if (k?.key) painters[p] = { key: k.key, base: s.bases[p] };
    }
    // Pictures by a provider whose key is kept: OpenAI's first.
    if (pictures.value && !painters[pictures.value as "openai" | "gemini"]) pictures.value = painters.openai ? "openai" : painters.gemini ? "gemini" : "";
    whoPaints();
    const kept = await load(s.provider).catch(() => undefined);
    asking = { provider: s.provider, model: s.models[s.provider] ?? "", base: s.bases[s.provider], key: kept?.key };
    const names = { anthropic: "Anthropic", openai: "OpenAI", gemini: "Gemini" };
    if (!asking.key || !asking.model) {
      who.textContent = `Narrato writes the deck with your own key: choose a provider, a model, and a key in the Assistant tab first.`;
    } else {
      who.textContent = `Written by ${names[asking.provider]}'s ${asking.model}, with your key, which goes to ${names[asking.provider]} alone.`;
    }
    go.disabled = running || !asking.key || !asking.model;
  };

  const start = () => {
    worker?.terminate();
    worker = engine.spawn();
    worker.onmessage = ({ data }: MessageEvent<FromWorker>) => {
      if (data.type === "briefing") fill(data.choices);
    };
    worker.onerror = (e) => step(`The generator did not start: ${e.message}`, true);
    worker.postMessage({ type: "briefing", engine: engine.module } satisfies ToWorker);
  };

  const step = (line: string, failed = false) => {
    const item = document.createElement("li");
    item.textContent = line;
    if (failed) item.className = "failed";
    progress.append(item);
    item.scrollIntoView({ block: "nearest" });
  };

  const busy = (on: boolean) => {
    running = on;
    for (const field of form.querySelectorAll<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement | HTMLButtonElement>(
      "input, select, textarea, #prompting-sources button, #prompting-add-link",
    ))
      field.disabled = on;
    go.disabled = on;
    go.textContent = on ? "Generating…" : "Generate";
  };

  opener.onclick = () => {
    progress.replaceChildren();
    dialog.returnValue = "";
    dialog.showModal();
    topic.focus();
    start();
    void whoWrites();
  };
  files.onchange = () => {
    given.push(...[...(files.files ?? [])].map((file) => ({ file })));
    files.value = "";
    shown();
  };
  const addLink = () => {
    const l = link.value.trim();
    if (!l) return;
    given.push({ link: l });
    link.value = "";
    shown();
  };
  $<HTMLButtonElement>("#prompting-add-link").onclick = addLink;
  link.onkeydown = (e) => {
    if (e.key !== "Enter") return;
    e.preventDefault();
    addLink();
  };
  dialog.ondragover = (e) => {
    if (running || !e.dataTransfer?.types.includes("Files")) return;
    e.preventDefault();
  };
  dialog.ondrop = (e) => {
    if (running || !e.dataTransfer?.files.length) return;
    e.preventDefault();
    given.push(...[...e.dataTransfer.files].map((file) => ({ file })));
    shown();
  };

  // Cancel is no submit button, so Enter in a field generates rather than cancels.
  $<HTMLButtonElement>("#prompting-cancel").onclick = () => dialog.close("cancel");
  // Closed, by Cancel, Escape, or a deck made, the worker stops: what it was making is dropped.
  dialog.onclose = () => {
    worker?.terminate();
    worker = undefined;
    busy(false);
  };

  form.onsubmit = async (e) => {
    const submitter = (e as SubmitEvent).submitter as HTMLButtonElement | null;
    if (submitter?.value !== "generate") return;
    e.preventDefault();
    if (running || !worker) return;
    await whoWrites();
    if (!asking?.key || !asking.model) return;
    if (dirty() && !confirm("The bundle has changes not saved. Make a new deck, and lose them when it opens?")) return;
    const job: Generate = {
      brief: {
        topic: topic.value.trim(),
        audience: audience.value.trim() || undefined,
        goal: goal.value || undefined,
        length: length.value || undefined,
        modality: modality.value || undefined,
        desiredOutcomes: outcome.value.trim() || undefined,
        language: language.value.trim() || undefined,
        voice: voice.value.trim() || undefined,
      },
      arc: arc.value || undefined,
      theme: theme.value,
      provider: asking.provider,
      model: asking.model,
      key: asking.key,
      base: asking.base,
      files: await Promise.all(
        given.flatMap((g) => ("file" in g ? [g.file] : [])).map(async (f) => ({ name: f.name, bytes: await f.arrayBuffer() })),
      ),
      links: given.flatMap((g) => ("link" in g ? [g.link] : [])),
      github: github.value.trim() || undefined,
      research: research.checked,
      pictures: (() => {
        const chosen = pictures.value as "openai" | "gemini" | "";
        const kept = chosen ? painters[chosen] : undefined;
        return chosen && kept ? { provider: chosen, key: kept.key, base: kept.base, look: look.value as "illustration" | "photo" } : undefined;
      })(),
    };
    progress.replaceChildren();
    busy(true);
    const making = worker;
    const done = new Promise<{ files: Record<string, ArrayBuffer>; name: string; made: Generated }>((resolve, reject) => {
      making.onmessage = ({ data }: MessageEvent<FromWorker>) => {
        if (data.type === "generating") step(said(data.event));
        else if (data.type === "generated") resolve(data);
        else if (data.type === "error") reject(new Error(data.message));
      };
      making.onerror = (e) => reject(new Error(e.message || "the generator stopped"));
    });
    making.postMessage(
      { type: "generate", generate: job, engine: engine.module } satisfies ToWorker,
      job.files.map((f) => f.bytes),
    );
    try {
      const deck = await done;
      dialog.close("generated");
      await open({ files: deck.files, name: deck.name });
      tell(made(deck.made));
    } catch (err) {
      // Closed while it ran, it says nothing more.
      if (worker !== making) return;
      step(err instanceof Error ? err.message : String(err), true);
      busy(false);
      // A worker that failed starts afresh.
      start();
    }
  };
}
