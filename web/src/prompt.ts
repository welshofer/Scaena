// New from a prompt (PLAN G.5, ADR-0026): a deck written by Vzestup's Narrato model from what the
// user asks, grounded in the files and links they give, with their own key, as `scaena generate`
// makes one. It is made in an engine's worker of its own, which starts as the dialog opens and
// loads the generator's module while the brief is written; Cancel stops it, asking first while it
// runs, and the deck open is untouched until the new one opens, kept nowhere until it is saved.
import { familySections } from "./theme-families";
import { keep, load, settings, store, verdict } from "./assistant/keys";
import type { Choices, FromWorker, Generate, Generated, GeneratingEvent, ProviderId, Source, StoryOutline, ToWorker } from "./protocol";
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
    case "outlining":
      return "Outlining the story: the big idea, and what each beat says…";
    case "outlined":
      return `Outline: ${event.beats} beats. Read it below, change what you will, then Write the slides.`;
    case "settled":
      return `Outline settled: ${event.beats} beats`;
    case "writing":
      return `Writing the slides, at least ${event.slides}…`;
    case "continuing":
      return `The answer was cut short after ${event.written} slides: asking for the rest of ${event.slides}…`;
    case "unrested":
      return `The rest of the slides did not come back: ${event.why}`;
    case "retrying":
      return `The provider is busy or did not answer: asking again in ${Math.round(event.seconds)} s (try ${event.tried + 1} of 6)…`;
    case "wrote":
      return `${event.slides} slides written${event.truncated ? ", the answer cut short" : ""}`;
    case "unIconed":
      return `The slides that name icons are set without them: ${event.why}`;
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

/** Wire the dialog: `opener` opens it, and a deck made opens by `open`, its story shown by
 * `story`, which gives its score, and its line by `tell`. `models` lists the models a key can
 * use, as the Assistant tab lists them (lift-up 10-24 USE-1): with no key kept, the dialog takes
 * one in place, checks it, and keeps it as that tab does. Changes not saved are asked about
 * before a deck is made, not as it opens: what a model wrote is never thrown away for them.
 * `running` says whether a deck is being made, which leaving the page asks about first (lift-up
 * 10-25 REL-4). */
export function prompting(
  engine: Engine,
  opener: HTMLElement,
  unsaved: () => boolean,
  open: (source: Source) => Promise<void>,
  story: () => Promise<number | undefined>,
  tell: (line: string) => void,
  models: (provider: ProviderId, key: string, base?: string) => Promise<string[]>,
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
  const keying = $<HTMLFieldSetElement>("#prompting-key");
  const keyProvider = $<HTMLSelectElement>("#prompting-key-provider");
  const keyValue = $<HTMLInputElement>("#prompting-key-value");
  const keyModel = $<HTMLSelectElement>("#prompting-key-model");
  const keyKeep = $<HTMLInputElement>("#prompting-key-keep");
  const keySaid = $<HTMLParagraphElement>("#prompting-key-said");
  const showOutline = $<HTMLInputElement>("#prompting-show-outline");
  theme.replaceChildren(...familySections());
  // The outline before the slides (narrative FUNC-1): the arc, the big idea, and each beat's claim,
  // shown once the model writes it, to change before the slides are written.
  const outlinePane = $<HTMLElement>("#prompting-outline");
  const outlineArc = $<HTMLParagraphElement>("#prompting-outline-arc");
  const bigIdea = $<HTMLTextAreaElement>("#prompting-big-idea");
  const beats = $<HTMLOListElement>("#prompting-beats");
  const outlineArcs = $<HTMLSelectElement>("#prompting-outline-arcs");
  /** The outline pane's own fields, which stay live while the deck is made. */
  const outlining = (field: Element) => field.closest("#prompting-outline") !== null;

  /** A beat as a row: its name, its claim to type over, the slides it takes, a number to set
   * (lift-up FUNC-4), and Up, Down, and Remove. */
  const beatRow = (beat: { name: string; claim: string; slides: number }): HTMLLIElement => {
    const li = document.createElement("li");
    const name = document.createElement("input");
    name.className = "name";
    name.value = beat.name;
    name.setAttribute("aria-label", "The beat's name");
    const slides = document.createElement("input");
    slides.className = "slides";
    slides.type = "number";
    slides.min = "0";
    slides.max = "60";
    slides.step = "1";
    slides.value = String(beat.slides);
    slides.setAttribute("aria-label", "The slides the beat takes");
    const claim = document.createElement("textarea");
    claim.className = "claim";
    claim.rows = 2;
    claim.value = beat.claim;
    claim.placeholder = "What its slides leave the audience believing";
    claim.setAttribute("aria-label", "The beat's claim");
    const acts = document.createElement("div");
    acts.className = "acts";
    const act = (label: string, aria: string, on: () => void) => {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = label;
      button.setAttribute("aria-label", aria);
      button.onclick = on;
      acts.append(button);
    };
    act("↑", "Move the beat up", () => li.previousElementSibling?.before(li));
    act("↓", "Move the beat down", () => li.nextElementSibling?.after(li));
    act("Remove", "Remove the beat", () => li.remove());
    li.append(name, slides, claim, acts);
    return li;
  };

  /** The outline as the pane holds it now, as the person settled it. */
  const settledOutline = (): StoryOutline => ({
    bigIdea: bigIdea.value.trim(),
    beats: [...beats.querySelectorAll("li")].map((li) => ({
      name: li.querySelector<HTMLInputElement>(".name")!.value,
      claim: li.querySelector<HTMLTextAreaElement>(".claim")!.value,
      slides: Math.max(0, Math.round(Number(li.querySelector<HTMLInputElement>(".slides")!.value) || 0)),
    })),
  });

  /** The outline shown, to settle: the arc and why, the big idea, and the beats. */
  const showOutlined = (outlined: { outline: StoryOutline; arc: string; reasoning: string; fallback?: string }) => {
    outlineArc.textContent = outlined.fallback
      ? `Arc: ${outlined.arc}, the heuristic's (${outlined.fallback}).`
      : `Arc: ${outlined.arc}. ${outlined.reasoning}`.trim();
    bigIdea.value = outlined.outline.bigIdea;
    beats.replaceChildren(...outlined.outline.beats.map(beatRow));
    outlineArcs.replaceChildren(
      ...[...arc.options].map((o) => new Option(o.textContent ?? "", o.value, false, o.value === outlined.arc)),
    );
    outlinePane.hidden = false;
    for (const field of outlinePane.querySelectorAll<HTMLInputElement | HTMLButtonElement>("input, textarea, select, button")) field.disabled = false;
    bigIdea.focus();
  };

  $<HTMLButtonElement>("#prompting-add-beat").onclick = () => {
    const li = beatRow({ name: "", claim: "", slides: 0 });
    beats.append(li);
    li.querySelector<HTMLInputElement>(".name")!.focus();
  };

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
    // What was chosen stays chosen where the worker is started again, after a run failed.
    const options = (select: HTMLSelectElement, values: [string, string][], fallback: string) => {
      const was = select.value;
      const chosen = select.options.length && values.some(([value]) => value === was) ? was : fallback;
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
      who.textContent = `Narrato writes the deck with your own key: paste one below, and choose the model that writes it.`;
    } else {
      who.textContent = `Written by ${names[asking.provider]}'s ${asking.model}, with your key, which goes to ${names[asking.provider]} alone.`;
    }
    go.disabled = running || !asking.key || !asking.model;
  };

  // A key taken in place (lift-up 10-24 USE-1), where none is kept for the assistant's provider:
  // checked by listing its models, as the Assistant tab checks one, then kept as that tab keeps it,
  // for this tab or on this device, and the provider and model chosen become the assistant's.
  const names: Record<ProviderId, string> = { anthropic: "Anthropic", openai: "OpenAI", gemini: "Gemini" };
  let listing = 0;
  const checked = async () => {
    const asked = ++listing;
    const p = keyProvider.value as ProviderId;
    const key = keyValue.value.trim();
    keyModel.replaceChildren();
    if (!key) {
      keySaid.textContent = `Paste ${names[p]}'s key: it goes from this page to ${names[p]} alone.`;
      return whoWrites();
    }
    keySaid.textContent = `Checking the key with ${names[p]}…`;
    let listed: string[] | Error;
    try {
      listed = await models(p, key, settings().bases[p]);
    } catch (e) {
      listed = e instanceof Error ? e : new Error(String(e));
    }
    if (asked !== listing) return;
    keySaid.textContent = verdict(p, listed).said;
    if (listed instanceof Error || !listed.length) return whoWrites();
    keyModel.replaceChildren(...listed.map((m) => new Option(m, m)));
    const picked = settings().models[p];
    if (picked && listed.includes(picked)) keyModel.value = picked;
    await store(p, key, keyKeep.checked);
    chosen();
    await whoWrites();
  };
  /** The provider and model chosen here, the assistant's from now on, as its tab keeps them. */
  const chosen = () => {
    const s = settings();
    const p = keyProvider.value as ProviderId;
    s.provider = p;
    if (keyModel.value) s.models[p] = keyModel.value;
    keep(s);
  };
  let typing: ReturnType<typeof setTimeout> | undefined;
  keyValue.oninput = () => {
    clearTimeout(typing);
    typing = setTimeout(() => void checked(), 400);
  };
  keyProvider.onchange = async () => {
    const kept = await load(keyProvider.value as ProviderId).catch(() => undefined);
    keyValue.value = kept?.key ?? "";
    keyKeep.checked = kept?.kept ?? false;
    await checked();
  };
  keyModel.onchange = () => {
    chosen();
    void whoWrites();
  };
  keyKeep.onchange = () => {
    const key = keyValue.value.trim();
    if (key && keyModel.options.length) void store(keyProvider.value as ProviderId, key, keyKeep.checked);
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
      if (!outlining(field)) field.disabled = on;
    go.disabled = on;
    go.textContent = on ? "Generating…" : "Generate";
    if (!on) outlinePane.hidden = true;
  };

  opener.onclick = () => {
    progress.replaceChildren();
    outlinePane.hidden = true;
    dialog.returnValue = "";
    dialog.showModal();
    topic.focus();
    start();
    // With no key kept for the assistant's provider, or no model chosen, the dialog asks for one.
    void whoWrites().then(() => {
      keying.hidden = Boolean(asking?.key && asking.model);
      if (keying.hidden) return;
      keyProvider.value = settings().provider;
      void keyProvider.onchange?.(new Event("change"));
    });
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

  // Cancel is no submit button, so Enter in a field generates rather than cancels. While a deck
  // is being made, Cancel and Escape ask first: closing drops what has been made so far, and the
  // calls already paid for (lift-up 10-24 USE-2).
  const stopping = () => !running || confirm("Stop making the deck? What has been made so far is lost.");
  $<HTMLButtonElement>("#prompting-cancel").onclick = () => {
    if (stopping()) dialog.close("cancel");
  };
  dialog.oncancel = (e) => {
    if (!stopping()) e.preventDefault();
  };
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
    if (unsaved() && !confirm("The deck open is not saved. Make a new deck, and lose what is not saved when it opens?")) return;
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
      outline: showOutline.checked,
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
        else if (data.type === "storyOutlined") showOutlined(data);
        else if (data.type === "generated") resolve(data);
        else if (data.type === "error") reject(new Error(data.message));
      };
      making.onerror = (e) => reject(new Error(e.message || "the generator stopped"));
    });
    // The outline settled, the slides are written to it; another arc chosen, it is outlined again.
    $<HTMLButtonElement>("#prompting-write").onclick = () => {
      const settled = settledOutline();
      if (!settled.beats.some((b) => b.claim.trim())) return step("Keep at least one beat with a claim.", true);
      outlinePane.hidden = true;
      making.postMessage({ type: "storySettled", outline: settled } satisfies ToWorker);
    };
    $<HTMLButtonElement>("#prompting-outline-again").onclick = () => {
      outlinePane.hidden = true;
      making.postMessage({ type: "outlineAgain", arc: outlineArcs.value || undefined } satisfies ToWorker);
    };
    making.postMessage(
      { type: "generate", generate: job, engine: engine.module } satisfies ToWorker,
      job.files.map((f) => f.bytes),
    );
    try {
      const deck = await done;
      dialog.close("generated");
      await open({ files: deck.files, name: deck.name });
      // What was made is said at once; the deck lands on its story (narrative USE-2): the arc's
      // curve, the pacing against the brief, and each slide's fix, where Gamma lands on its
      // outline, its score in the line as soon as the story is read.
      tell(made(deck.made));
      const score = await story();
      if (score !== undefined) tell(`${made(deck.made)} · story ${score} of 100`);
    } catch (err) {
      // Closed while it ran, it says nothing more.
      if (worker !== making) return;
      step(err instanceof Error ? err.message : String(err), true);
      busy(false);
      // A worker that failed starts afresh.
      start();
    }
  };
  return { running: () => running };
}
