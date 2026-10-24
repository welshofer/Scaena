// The assistant's panel in the editor (PLAN 2.6, SPEC §9.2, §11): who answers (a provider,
// the user's key, a model the key can use), the conversation, and the question. The source is
// read-only while the assistant works; each edit it makes comes into the source as it is made,
// as a fix does, and undoes as one. Under each call that changed what a state draws, each such
// state before and after, as the strip draws it (PLAN 2.93): a click shows it.
import type { AssistantEvent, Edited, ProviderId, Rewritten, Seeing, Thumb } from "../protocol";
import type { Stage } from "../stage";
import type { Who } from "../writing";
import * as keys from "./keys";

/** The editor, as the panel works with it. */
export interface Editor {
  /** The source as it stands. */
  source(): string;
  /** Take `source`, which the assistant's edit made, and what the worker found compiling,
   * showing, and linting it; and select what the question has changed so far, `touched`, once
   * the canvas stands in it (PLAN 2.52). */
  apply(source: string, edited: Edited, touched?: string[], files?: Rewritten[]): void;
  /** What the editor shows: the state, what is selected there, and the characters selected in
   * a text typed in, which a question is about (PLAN 2.52). */
  seeing(): Seeing | undefined;
  /** Make the source read-only, or not. */
  lock(on: boolean): void;
  /** Each state's picture as the strip holds it once it has drawn the deck as it stands, by
   * state, and how many pixels high they are; and the deck's states (PLAN 2.93). */
  drawings(): Promise<{ height: number; drawn: Map<string, Drawing>; states: string[] }>;
  /** Show `state`. */
  show(state: string): void;
  /** `state` as a person counts it (PLAN 2.99): Slide 3, or Slide 3, step 2. */
  slide?(state: string): string;
}

/** A state's picture, and the digest of what it draws: none where it could not be painted. */
export interface Drawing {
  digest: string;
  image?: ImageData;
}

/** How many states an edit's pictures show before it says how many more it changed. */
const SHOWN = 4;

const $ = <T extends HTMLElement>(selector: string) => document.querySelector<T>(selector)!;

/** Each provider's name, its own address, and where its keys come from. */
const PROVIDERS: Record<ProviderId, { name: string; base: string; keys: string }> = {
  anthropic: { name: "Anthropic", base: "https://api.anthropic.com", keys: "console.anthropic.com" },
  openai: { name: "OpenAI", base: "https://api.openai.com", keys: "platform.openai.com, or the server that speaks its API at the address below" },
  gemini: { name: "Gemini", base: "https://generativelanguage.googleapis.com", keys: "aistudio.google.com" },
};

/** What each of the tools does, in a person's words (PLAN 2.99), while it runs, once it has,
 * and where it did not (stopped, or an edit refused or only tried): no tool's name reaches the
 * conversation, as ScaenaKit's `ToolWords` says them on the Mac. */
const TOOL_WORDS: Record<string, [doing: string, done: string, not: string]> = {
  background_set: ["Setting a background…", "Set a background", "Didn't set a background"],
  data_attach: ["Adding data…", "Added data", "Didn't add data"],
  data_edit: ["Changing the data…", "Changed the data", "Didn't change the data"],
  deck_create: ["Making a deck…", "Made a deck", "Didn't make a deck"],
  deck_diff: ["Comparing versions…", "Compared versions", "Didn't compare versions"],
  deck_export: ["Exporting the deck…", "Exported the deck", "Didn't export the deck"],
  deck_find: ["Searching the deck…", "Searched the deck", "Didn't search the deck"],
  deck_generate: ["Making a deck from slides…", "Made a deck from slides", "Didn't make a deck from slides"],
  deck_health: ["Reading the deck's story…", "Read the deck's story", "Didn't read the deck's story"],
  deck_history: ["Reading the history…", "Looked at the history", "Didn't look at the history"],
  deck_inspect: ["Looking over the deck…", "Looked over the deck", "Didn't look over the deck"],
  deck_lint: ["Checking the deck…", "Checked the deck", "Didn't check the deck"],
  deck_patch: ["Editing the deck…", "Edited the deck", "Didn't edit the deck"],
  deck_read: ["Reading the deck…", "Went through the deck", "Didn't go through the deck"],
  deck_render: ["Looking at a slide…", "Looked at a slide", "Didn't look at a slide"],
  icon_find: ["Finding an icon…", "Found an icon", "Didn't find an icon"],
  map_fetch: ["Fetching a map…", "Put a map on the slide", "Didn't fetch a map"],
  spine_read: ["Reading the outline…", "Went through the outline", "Didn't go through the outline"],
  spine_update: ["Changing the outline…", "Changed the outline", "Didn't change the outline"],
  state_start: ["Adding a slide…", "Added a slide", "Didn't add a slide"],
  theme_apply: ["Changing the theme…", "Changed the theme", "Didn't change the theme"],
  theme_edit: ["Editing the theme…", "Edited the theme", "Didn't edit the theme"],
  resource_read: ["Reading how decks are written…", "Read how decks are written", "Didn't read how decks are written"],
};
const doing = (tool: string) => TOOL_WORDS[tool]?.[0] ?? "Working…";
const done = (tool: string) => TOOL_WORDS[tool]?.[1] ?? "Used a tool";
const undone = (tool: string) => TOOL_WORDS[tool]?.[2] ?? "Didn't use a tool";

/** What the conversation shows of each stop that is not the end of an answer. */
const STOPS: Record<string, string> = {
  stopped: "Stopped.",
  length: "The answer ran out of room.",
  steps: "Stopped after as many rounds of tools as one question allows: ask it to go on.",
  other: "The model stopped.",
};

export interface Panel {
  ask(text: string): Promise<AssistantEvent>;
  /** Who the writing tools ask (PLAN 2.101): the provider, model, and key the panel holds; or why
   * there is none to ask. */
  who(): Who | string;
  /** What the conversation shows, entry by entry, as text. */
  transcript(): string[];
  /** Tokens in and out so far. */
  usage(): { input: number; output: number };
  /** `provider` chosen, as the list chooses it: its key, its address, and its models in the form,
   * and the one the assistant asks from then on. */
  choose(provider: ProviderId): Promise<void>;
}

export function panel(stage: Stage, editor: Editor): Panel {
  const provider = $<HTMLSelectElement>("#provider");
  const key = $<HTMLInputElement>("#key");
  const model = $<HTMLSelectElement>("#model");
  const kept = $<HTMLInputElement>("#keep-key");
  const base = $<HTMLInputElement>("#base");
  const note = $("#keynote");
  const status = $("#keystatus");
  const hintLine = $("#keyhint");
  const reveal = $<HTMLButtonElement>("#reveal-key");
  const transcript = $<HTMLOListElement>("#transcript");
  const question = $<HTMLTextAreaElement>("#question");
  const askButton = $<HTMLButtonElement>("#ask");
  const stop = $<HTMLButtonElement>("#stop");
  const usageLine = $("#usage");
  const thinking = $("#thinking");
  const settings = keys.settings();
  const used = { input: 0, output: 0 };
  let working = false;
  /** The question being asked, to stop. Stop shows as it is asked, before the worker has it,
   * while the strip finishes drawing: a press then stops it before it is sent. */
  let stopping: AbortController | undefined;

  const which = () => provider.value as ProviderId;
  const tell = (text: string) => (note.textContent = text);
  /** Whether the key works, as the line under it says: none, checking, works, refused, or not
   * checked. */
  const say = (state: "none" | "checking" | "works" | "refused" | "unreachable", text: string) => {
    status.dataset.state = state;
    status.textContent = text;
  };
  /** What looks wrong with the key's form, as it is typed. */
  const hinted = () => {
    const said = keys.hint(which(), key.value);
    hintLine.hidden = !said;
    hintLine.textContent = said ?? "";
  };
  /** The key shown or masked. */
  const showing = (shown: boolean) => {
    key.type = shown ? "text" : "password";
    reveal.setAttribute("aria-pressed", String(shown));
    reveal.setAttribute("aria-label", shown ? "Hide the key" : "Show the key");
    reveal.title = shown ? "Hide the key" : "Show the key";
  };
  reveal.onclick = () => {
    showing(key.type === "password");
    key.focus();
  };
  const risk = () =>
    `Your key goes from this page to ${PROVIDERS[which()].name} and nowhere else; get one at ${PROVIDERS[which()].keys}. ` +
    (kept.checked
      ? "It is kept on this device, encrypted, until you forget it: any script this page runs could still use it."
      : "It is kept for this tab alone.");

  /** The provider's settings into the form: its key, its address, and its models. */
  async function choose(p: ProviderId) {
    provider.value = p;
    base.placeholder = PROVIDERS[p].base;
    base.value = settings.bases[p] ?? "";
    const found = await keys.load(p);
    key.value = found?.key ?? "";
    kept.checked = found?.kept ?? false;
    showing(false);
    hinted();
    tell(risk());
    await listModels();
  }

  /** The models the key can use, the one picked last chosen. */
  let listing = 0;
  async function listModels() {
    const asked = ++listing;
    const p = which();
    model.replaceChildren();
    if (!key.value) return say("none", `No key yet: paste ${PROVIDERS[p].name}'s to ask its models.`);
    say("checking", `Checking the key with ${PROVIDERS[p].name}…`);
    let models: string[];
    try {
      models = await stage.models(p, key.value, base.value || undefined);
    } catch (e) {
      if (asked !== listing) return;
      const { state, said } = keys.verdict(p, e instanceof Error ? e : new Error(String(e)));
      say(state, said);
      return;
    }
    if (asked !== listing) return;
    const { state, said } = keys.verdict(p, models);
    say(state, said);
    model.replaceChildren(...models.map((m) => new Option(m, m)));
    const picked = settings.models[p];
    if (picked && models.includes(picked)) model.value = picked;
    tell(risk());
  }

  const remember = () => {
    const p = which();
    settings.provider = p;
    if (model.value) settings.models[p] = model.value;
    if (base.value) settings.bases[p] = base.value;
    else delete settings.bases[p];
    keys.keep(settings);
  };

  provider.onchange = () => void choose(which()).then(remember);
  model.onchange = remember;
  let typing: ReturnType<typeof setTimeout> | undefined;
  key.oninput = base.oninput = () => {
    hinted();
    clearTimeout(typing);
    typing = setTimeout(() => {
      remember();
      if (key.value) void keys.store(which(), key.value, kept.checked);
      void listModels();
    }, 400);
  };
  kept.onchange = () => {
    if (key.value) void keys.store(which(), key.value, kept.checked);
    tell(risk());
  };
  $<HTMLButtonElement>("#forget-key").onclick = () => {
    keys.forget(which());
    key.value = "";
    kept.checked = false;
    model.replaceChildren();
    showing(false);
    hinted();
    say("none", `No key yet: paste ${PROVIDERS[which()].name}'s to ask its models.`);
    tell(`The ${PROVIDERS[which()].name} key is forgotten. ${risk()}`);
  };
  $<HTMLButtonElement>("#new").onclick = () => {
    stage.forget();
    transcript.replaceChildren();
    used.input = used.output = 0;
    usageLine.textContent = "";
  };
  stop.onclick = () => stopping?.abort();
  $<HTMLFormElement>("#asking").onsubmit = (e) => {
    e.preventDefault();
    void ask(question.value);
  };
  question.onkeydown = (e) => {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      void ask(question.value);
    }
  };

  const entry = (className: string, text = "") => {
    const li = document.createElement("li");
    li.className = className;
    li.textContent = text;
    transcript.append(li);
    li.scrollIntoView({ block: "end" });
    return li;
  };

  /** Each call as it is made, then what it came to. */
  const calls = new Map<string, HTMLElement>();
  /** The call made last: an edit is that call's. */
  let calling: HTMLElement | undefined;
  /** Each state's picture as the question found it, kept up to each edit since; and the states
   * there were then (PLAN 2.93). */
  let drawn = new Map<string, Drawing>();
  let were = new Set<string>();
  function hear(event: AssistantEvent) {
    switch (event.kind) {
      case "text":
        return void entry("said", event.text);
      case "call": {
        const li = entry("call");
        // What the call does, in a person's words; the tool and what it was given, opened.
        li.innerHTML = "<details><summary><span class=doing></span> <span class=outcome></span></summary><pre class=args></pre></details>";
        li.dataset.tool = event.name;
        li.querySelector(".doing")!.textContent = doing(event.name);
        li.querySelector(".args")!.textContent = `${event.name} ${JSON.stringify(event.args, null, 1)}`;
        calling = li;
        return void calls.set(event.id, li);
      }
      case "result": {
        const li = calls.get(event.id) ?? entry("call");
        li.classList.toggle("stopped", event.error);
        // Headed by what it came to: an edit refused, only tried, or stopped did not edit.
        const said = li.querySelector(".doing");
        if (said) said.textContent = event.made ? done(event.name) : undone(event.name);
        li.querySelector(".outcome")!.textContent = event.summary;
        const result = document.createElement("pre");
        result.textContent = event.json.length > 4000 ? `${event.json.slice(0, 4000)}…` : event.json;
        li.querySelector("details")?.append(result);
        if (event.png) li.append(Object.assign(document.createElement("img"), { src: `data:image/png;base64,${event.png}`, alt: `The slide the assistant looked at, ${event.summary}` }));
        return;
      }
      case "edited":
        editor.apply(event.source, event.edited, event.touched, event.files);
        if (calling && (event.drawn?.length || event.gone?.length)) changes(calling, event.drawn ?? [], event.gone ?? []);
        return;
      case "usage":
        used.input += event.input;
        used.output += event.output;
        usageLine.textContent = `${used.input.toLocaleString()} tokens in, ${used.output.toLocaleString()} out`;
        return;
      case "done":
        if (event.stop !== "end" && event.stop !== "tools") entry("end", STOPS[event.stop] ?? STOPS.other);
        return;
      case "failed":
        return void entry("failed", event.message);
    }
  }

  /** What an edit changed, under its call `li` (PLAN 2.93): each state it drew otherwise, before
   * and after, the first few, how many more, and those it took away. */
  function changes(li: HTMLElement, after: Thumb[], gone: string[]) {
    const box = document.createElement("div");
    box.className = "changed";
    box.setAttribute("role", "group");
    box.setAttribute("aria-label", "What it changed");
    const picture = (image: ImageData) => {
      const canvas = document.createElement("canvas");
      [canvas.width, canvas.height] = [image.width, image.height];
      canvas.getContext("2d")?.putImageData(image, 0, 0);
      canvas.setAttribute("aria-hidden", "true");
      return canvas;
    };
    const now = after.flatMap((t) =>
      t.pixels && t.width && t.height ? [{ state: t.state, digest: t.digest, image: new ImageData(new Uint8ClampedArray(t.pixels), t.width, t.height) }] : [],
    );
    for (const { state, image } of now.slice(0, SHOWN)) {
      const was = drawn.get(state)?.image;
      const button = document.createElement("button");
      button.type = "button";
      button.className = "change";
      button.dataset.state = state;
      const added = !were.has(state);
      const slide = editor.slide?.(state) ?? "the slide";
      button.setAttribute("aria-label", added ? `Show ${slide}, which it added` : `Show ${slide}, as it drew it before and after`);
      const before = was ? picture(was) : Object.assign(document.createElement("span"), { className: "none", textContent: added ? "new" : "" });
      const arrow = Object.assign(document.createElement("span"), { className: "arrow", textContent: "→" });
      arrow.setAttribute("aria-hidden", "true");
      const name = Object.assign(document.createElement("span"), { className: "name", textContent: slide });
      button.append(before, arrow, picture(image), name);
      button.onclick = () => editor.show(state);
      box.append(button);
    }
    const more = now.length - SHOWN;
    if (more > 0) box.append(Object.assign(document.createElement("span"), { className: "more", textContent: `and ${more} more` }));
    if (gone.length) box.append(Object.assign(document.createElement("span"), { className: "more", textContent: `took away ${gone.length === 1 ? "a slide" : `${gone.length} slides`}` }));
    li.append(box);
    // The next edit is drawn against this one.
    for (const { state, digest, image } of now) {
      drawn.set(state, { digest, image });
      were.add(state);
    }
    for (const state of gone) {
      drawn.delete(state);
      were.delete(state);
    }
  }

  async function ask(text: string): Promise<AssistantEvent> {
    text = text.trim();
    if (!text || working) return { kind: "failed", message: working ? "the assistant is working" : "nothing asked" };
    if (!key.value) return failure(`Enter your ${PROVIDERS[which()].name} key first.`);
    if (!model.value) return failure("Pick a model first: the list fills once the provider has your key.");
    remember();
    entry("user", text);
    question.value = "";
    // What the editor shows as it is asked: the source locks below, and shows nothing then.
    const seeing = editor.seeing();
    const stopped = (stopping = new AbortController()).signal;
    working = true;
    thinking.hidden = false;
    editor.lock(true);
    askButton.disabled = true;
    stop.hidden = false;
    try {
      // Each state as the question finds it: each edit's pictures are drawn against it (PLAN 2.93).
      const drawings = await editor.drawings();
      drawn = new Map(drawings.drawn);
      were = new Set(drawings.states);
      const known = Object.fromEntries([...drawn].map(([state, d]) => [state, d.digest]));
      // The Geoapify key kept, which `map_fetch` fetches a map with (ADR-0030): none, and it says so.
      const mapKey = (await keys.load("geoapify").catch(() => undefined))?.key;
      if (stopped.aborted) {
        const done: AssistantEvent = { kind: "done", stop: "stopped" };
        hear(done);
        return done;
      }
      // Sent after the question, so the worker stops this one.
      stopped.addEventListener("abort", () => stage.stop(), { once: true });
      return await stage.ask(
        editor.source(),
        { provider: which(), model: model.value, key: key.value, base: base.value || undefined, text, seeing, mapKey },
        hear,
        { height: drawings.height, known },
      );
    } catch (e) {
      return failure(e instanceof Error ? e.message : String(e));
    } finally {
      stopping = undefined;
      working = false;
      thinking.hidden = true;
      editor.lock(false);
      askButton.disabled = false;
      stop.hidden = true;
    }
  }

  function failure(message: string): AssistantEvent {
    entry("failed", message);
    return { kind: "failed", message };
  }

  // A bundle opened anew starts a conversation anew: the worker that held the last is gone.
  transcript.replaceChildren();
  usageLine.textContent = "";
  void choose(settings.provider);
  return {
    ask,
    who: () => {
      const p = which();
      const name = PROVIDERS[p].name;
      if (!key.value) return `Add your ${name} key in the Assistant tab first: the model you pick there writes this, with your key.`;
      if (!model.value) return `Pick a model in the Assistant tab first: the list fills once ${name} has your key.`;
      remember();
      return { provider: p, name, model: model.value, key: key.value, base: base.value || undefined };
    },
    transcript: () => [...transcript.children].map((li) => `${li.className}: ${li.querySelector("summary")?.textContent ?? li.textContent}`),
    usage: () => ({ ...used }),
    choose: (p: ProviderId) => choose(p).then(remember),
  };
}
