// The assistant's panel in the editor (PLAN 2.6, SPEC §9.2, §11): who answers (a provider,
// the user's key, a model the key can use), the conversation, and the question. The source is
// read-only while the assistant works; each edit it makes comes into the source as it is made,
// as a fix does, and undoes as one. Under each call that changed what a state draws, each such
// state before and after, as the strip draws it (PLAN 2.93): a click shows it.
import type { AssistantEvent, Edited, ProviderId, Rewritten, Seeing, Thumb } from "../protocol";
import type { Stage } from "../stage";
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
}

/** A state's picture, and the digest of what it draws. */
export interface Drawing {
  digest: string;
  image: ImageData;
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

/** What the conversation shows of each stop that is not the end of an answer. */
const STOPS: Record<string, string> = {
  stopped: "Stopped.",
  length: "The answer ran out of room.",
  steps: "Stopped after as many rounds of tools as one question allows: ask it to go on.",
  other: "The model stopped.",
};

export interface Panel {
  ask(text: string): Promise<AssistantEvent>;
  /** What the conversation shows, entry by entry, as text. */
  transcript(): string[];
  /** Tokens in and out so far. */
  usage(): { input: number; output: number };
}

export function panel(stage: Stage, editor: Editor): Panel {
  const provider = $<HTMLSelectElement>("#provider");
  const key = $<HTMLInputElement>("#key");
  const model = $<HTMLSelectElement>("#model");
  const kept = $<HTMLInputElement>("#keep-key");
  const base = $<HTMLInputElement>("#base");
  const note = $("#keynote");
  const transcript = $<HTMLOListElement>("#transcript");
  const question = $<HTMLTextAreaElement>("#question");
  const askButton = $<HTMLButtonElement>("#ask");
  const stop = $<HTMLButtonElement>("#stop");
  const usageLine = $("#usage");
  const settings = keys.settings();
  const used = { input: 0, output: 0 };
  let working = false;

  const which = () => provider.value as ProviderId;
  const tell = (text: string) => (note.textContent = text);
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
    tell(risk());
    await listModels();
  }

  /** The models the key can use, the one picked last chosen. */
  let listing = 0;
  async function listModels() {
    const asked = ++listing;
    const p = which();
    model.replaceChildren();
    if (!key.value) return;
    let models: string[];
    try {
      models = await stage.models(p, key.value, base.value || undefined);
    } catch (e) {
      if (asked === listing) tell(`${PROVIDERS[p].name} did not list its models: ${e instanceof Error ? e.message : e}`);
      return;
    }
    if (asked !== listing) return;
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
    tell(`The ${PROVIDERS[which()].name} key is forgotten. ${risk()}`);
  };
  $<HTMLButtonElement>("#new").onclick = () => {
    stage.forget();
    transcript.replaceChildren();
    used.input = used.output = 0;
    usageLine.textContent = "";
  };
  stop.onclick = () => stage.stop();
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
        li.innerHTML = "<details><summary><code></code> <span class=outcome>…</span></summary><pre class=args></pre></details>";
        li.querySelector("code")!.textContent = event.name;
        li.querySelector(".args")!.textContent = JSON.stringify(event.args, null, 1);
        calling = li;
        return void calls.set(event.id, li);
      }
      case "result": {
        const li = calls.get(event.id) ?? entry("call");
        li.classList.toggle("stopped", event.error);
        li.querySelector(".outcome")!.textContent = event.summary;
        const result = document.createElement("pre");
        result.textContent = event.json.length > 4000 ? `${event.json.slice(0, 4000)}…` : event.json;
        li.querySelector("details")?.append(result);
        if (event.png) li.append(Object.assign(document.createElement("img"), { src: `data:image/png;base64,${event.png}`, alt: `deck_render: ${event.summary}` }));
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
      button.setAttribute("aria-label", added ? `Show ${state}, which it added` : `Show ${state}, as it drew it before and after`);
      const before = was ? picture(was) : Object.assign(document.createElement("span"), { className: "none", textContent: added ? "new" : "" });
      const arrow = Object.assign(document.createElement("span"), { className: "arrow", textContent: "→" });
      arrow.setAttribute("aria-hidden", "true");
      const name = Object.assign(document.createElement("span"), { className: "name", textContent: state });
      button.append(before, arrow, picture(image), name);
      button.onclick = () => editor.show(state);
      box.append(button);
    }
    const more = now.length - SHOWN;
    if (more > 0) box.append(Object.assign(document.createElement("span"), { className: "more", textContent: `and ${more} more` }));
    if (gone.length) box.append(Object.assign(document.createElement("span"), { className: "more", textContent: `took away ${gone.join(", ")}` }));
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
    working = true;
    editor.lock(true);
    askButton.disabled = true;
    stop.hidden = false;
    try {
      // Each state as the question finds it: each edit's pictures are drawn against it (PLAN 2.93).
      const drawings = await editor.drawings();
      drawn = new Map(drawings.drawn);
      were = new Set(drawings.states);
      const known = Object.fromEntries([...drawn].map(([state, d]) => [state, d.digest]));
      return await stage.ask(
        editor.source(),
        { provider: which(), model: model.value, key: key.value, base: base.value || undefined, text, seeing },
        hear,
        { height: drawings.height, known },
      );
    } catch (e) {
      return failure(e instanceof Error ? e.message : String(e));
    } finally {
      working = false;
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
    transcript: () => [...transcript.children].map((li) => `${li.className}: ${li.querySelector("summary")?.textContent ?? li.textContent}`),
    usage: () => ({ ...used }),
  };
}
