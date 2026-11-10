// The assistant's panel in the editor (PLAN 2.6, SPEC §9.2, §11): who answers (a provider,
// the user's key, a model the key can use), the conversation, and the question. The source is
// read-only while the assistant works; each edit it makes comes into the source as it is made,
// as a fix does, and undoes as one. Under each call that changed what a state draws, each such
// state before and after, as the strip draws it (PLAN 2.93): a click shows it.
import type { AssistantEvent, Edited, ProviderId, Rewritten, Seeing, Thumb } from "../protocol";
import type { Stage } from "../stage";
import type { Who } from "../writing";
import * as talks from "../conversations";
import * as keys from "./keys";
import { render } from "./markdown";
import { refusedBase } from "./providers";

/** What a question asked with no key kept says to do (editor dogfood 5), as Check the claim's
 * dialog says it: in the status where the Story asks it, and in the conversation where Ask is
 * pressed. */
export const NO_KEY = "Paste a key to ask this";

/** The editor, as the panel works with it. */
export interface Editor {
  /** The source as it stands. */
  source(): string;
  /** The states the deck has, by id: what an answer may link to (narrative ATTR-4). */
  states?(): string[];
  /** Take `source`, which the assistant's edit made, and what the worker found compiling,
   * showing, and linting it; and select what the question has changed so far, `touched`, once
   * the canvas stands in it (PLAN 2.52). */
  apply(source: string, edited: Edited, touched?: string[], files?: Rewritten[]): void;
  /** What the editor shows: the state, what is selected there, and the characters selected in
   * a text typed in, which a question is about (PLAN 2.52). */
  seeing(): Seeing | undefined;
  /** Make the source read-only, or not. */
  lock(on: boolean): void;
  /** An answer begins, or ends (lift-up 10-25 FUNC-4): its edits make one step of the source's
   * history once it ends. Ended, whether it edited anything. */
  answering(on: boolean): boolean;
  /** Take the last answer back, while its step is the last: whether it was. */
  undoAnswer(): boolean;
  /** Each state's picture as the strip holds it once it has drawn the deck as it stands, by
   * state, and how many pixels high they are; and the deck's states (PLAN 2.93). */
  drawings(): Promise<{ height: number; drawn: Map<string, Drawing>; states: string[] }>;
  /** Show `state`. */
  show(state: string): void;
  /** `state` as a person counts it (PLAN 2.99): Slide 3, or Slide 3, step 2. */
  slide?(state: string): string;
  /** The key the deck's conversation is kept by in this browser, its recovery copy's (lift-up
   * 20261110 FUNC-4). */
  conversationKey?(): string;
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
  slide_write: ["Writing a slide…", "Wrote a slide", "Didn't write a slide"],
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
  image_add: ["Putting a picture on the slide…", "Put a picture on the slide", "Didn't put a picture on the slide"],
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
  refused: "The model declined to answer that. Ask in other words.",
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
  const starters = $("#starters");
  const question = $<HTMLTextAreaElement>("#question");
  const askButton = $<HTMLButtonElement>("#ask");
  const stop = $<HTMLButtonElement>("#stop");
  const usageLine = $("#usage");
  const thinking = $("#thinking");
  /** How long a turn may go in silence before the provider is asked again (narrative REL-3):
   * two minutes, or what `?assistant-timeout=` says in ms, for a walk. */
  const timeoutMs = Number(new URLSearchParams(location.search).get("assistant-timeout")) || undefined;
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
    const refused = base.value ? refusedBase(base.value) : undefined;
    if (refused) return say("refused", refused);
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
    if (base.value && !refusedBase(base.value)) settings.bases[p] = base.value;
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
  const newConversation = $<HTMLButtonElement>("#new");
  newConversation.onclick = () => {
    // Never while a question works: its answer would go on into a conversation forgotten.
    if (working) return;
    stage.forget();
    transcript.replaceChildren();
    void offer();
    const key = editor.conversationKey?.();
    if (key) void talks.discard(key);
    // The words of an answer cut off went with the transcript (lift-up 20261031 STAB-4): the next
    // answer's go into an entry of their own, one the page shows.
    saying = undefined;
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
  /** What the model is saying, as it is written: the entry its words go into until they are whole. */
  let saying: HTMLElement | undefined;
  function hear(event: AssistantEvent) {
    switch (event.kind) {
      case "saying": {
        saying ??= entry("said");
        saying.append(event.text);
        saying.scrollIntoView({ block: "end" });
        return;
      }
      case "waiting": {
        // The provider is asked again (narrative REL-3): the words it sent so far go, and the
        // wait is said where Working… was.
        saying?.remove();
        saying = undefined;
        thinking.textContent = `${event.message} Trying again in ${event.seconds} s (${event.attempt} of ${event.of})…`;
        return;
      }
      case "text": {
        begun = true;
        thinking.textContent = "Working…";
        // Said whole, as it reads (narrative ATTR-4): its lists, code, emphasis, and links,
        // a link to a slide showing it; in the words' place, or in a new entry.
        const li = saying ?? entry("said");
        saying = undefined;
        // Its words as the model wrote them, which a conversation kept shows again (FUNC-4).
        li.dataset.words = event.text;
        render(event.text, li, { states: editor.states?.(), show: (state) => editor.show(state) });
        return;
      }
      case "call": {
        begun = true;
        thinking.textContent = "Working…";
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
      // An answer stopped, cut off, or failed ends its words where they stand: the next answer's
      // go into an entry of their own (lift-up 20261031 STAB-4).
      case "done":
        saying = undefined;
        if (event.stop !== "end" && event.stop !== "tools") entry("end", STOPS[event.stop] ?? STOPS.other);
        return;
      case "failed":
        saying = undefined;
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
      if (!redrawn.includes(state)) redrawn.push(state);
    }
    for (const state of gone) {
      drawn.delete(state);
      were.delete(state);
    }
  }

  /** Whether the answer to the question asked has begun: its first words or call. Until then, a
   * question that fails or is stopped goes back into the box (lift-up 20261110 USE-2). */
  let begun = false;
  /** The states the answer being given drew anew, in the order it did (editor dogfood 1). */
  let redrawn: string[] = [];

  async function ask(text: string): Promise<AssistantEvent> {
    text = text.trim();
    if (!text || working) return { kind: "failed", message: working ? "the assistant is working" : "nothing asked" };
    if (!key.value) {
      // The question waits in its box, the key's field taking the focus (editor dogfood 5).
      key.focus();
      return failure(`${NO_KEY}: paste ${PROVIDERS[which()].name}'s above.`);
    }
    if (!model.value) return failure("Pick a model first: the list fills once the provider has your key.");
    remember();
    saying = undefined;
    entry("user", text);
    starters.hidden = true;
    question.value = "";
    begun = false;
    redrawn = [];
    let outcome: AssistantEvent | undefined;
    // What the editor shows as it is asked: the source locks below, and shows nothing then.
    const seeing = editor.seeing();
    const stopped = (stopping = new AbortController()).signal;
    working = true;
    thinking.textContent = "Working…";
    thinking.hidden = false;
    editor.lock(true);
    editor.answering(true);
    askButton.disabled = true;
    newConversation.disabled = true;
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
        return (outcome = done);
      }
      // Sent after the question, so the worker stops this one.
      stopped.addEventListener("abort", () => stage.stop(), { once: true });
      return (outcome = await stage.ask(
        editor.source(),
        { provider: which(), model: model.value, key: key.value, base: base.value || undefined, text, seeing, mapKey, timeoutMs },
        hear,
        { height: drawings.height, known },
      ));
    } catch (e) {
      return (outcome = failure(e instanceof Error ? e.message : String(e)));
    } finally {
      // A question that failed, or was stopped before any answer came, is put back as it was asked,
      // to send again or mend: a refused key or a dropped connection loses nothing typed. One the
      // person has begun typing since is left as they typed it.
      const unanswered = outcome?.kind === "failed" || (outcome?.kind === "done" && outcome.stop === "stopped");
      if (unanswered && !begun && !question.value) {
        question.value = text;
        question.setSelectionRange(text.length, text.length);
      }
      stopping = undefined;
      working = false;
      thinking.hidden = true;
      editor.lock(false);
      if (editor.answering(false)) {
        undoable();
        // What it changed is shown (editor dogfood 1): an answer that drew none of the slide shown
        // anew shows the first slide it did, so its edit is seen, not left on another slide.
        const shown = editor.seeing()?.state;
        const first = redrawn.find((state) => drawn.has(state));
        if (first && shown && !redrawn.includes(shown)) editor.show(first);
      }
      void keepConversation();
      askButton.disabled = false;
      newConversation.disabled = false;
      stop.hidden = true;
    }
  }

  /** The answer that edited the deck, one step: Undo this answer under it, which an earlier
   * answer's gives way to (lift-up 10-25 FUNC-4). */
  function undoable() {
    for (const was of transcript.querySelectorAll<HTMLButtonElement>(".undo-answer button")) was.closest("li")?.remove();
    const li = entry("undo-answer");
    const button = Object.assign(document.createElement("button"), { type: "button", textContent: "Undo this answer" });
    button.onclick = () => {
      if (editor.undoAnswer()) li.replaceChildren("Answer undone. Redo makes it again.");
      else li.replaceChildren("Not undone: the deck was edited after this answer. Undo those edits first, or undo step by step.");
    };
    li.append(button);
  }

  /** Keep the conversation as it now stands, to go on with after a reload or when the deck opens
   * again (lift-up 20261110 FUNC-4): what the worker would send the model next, and the questions
   * and answers the panel shows. */
  async function keepConversation() {
    const key = editor.conversationKey?.();
    if (!key) return;
    const messages = await stage.conversation().catch(() => []);
    if (!messages.length) return void talks.discard(key);
    const said = [...transcript.querySelectorAll<HTMLElement>("li.user, li.said")].map((li) =>
      li.classList.contains("user") ? { who: "user" as const, text: li.textContent ?? "" } : { who: "said" as const, text: li.dataset.words ?? li.textContent ?? "" },
    );
    await talks.keep(key, { at: Date.now(), messages, said }).catch(() => {});
  }

  /** Offer the conversation kept for this deck, where one is: Go on with it, or Start anew. */
  async function offerKept() {
    const key = editor.conversationKey?.();
    const last = key ? await talks.find(key) : null;
    if (!key || !last || transcript.children.length || working) return;
    const asked = last.said.filter((s) => s.who === "user").length;
    const when = new Date(last.at).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
    const li = entry("resume", `Your last conversation about this deck, ${asked} ${asked === 1 ? "question" : "questions"}, from ${when}. `);
    const on = Object.assign(document.createElement("button"), { type: "button", textContent: "Continue the last conversation" });
    const anew = Object.assign(document.createElement("button"), { type: "button", textContent: "Start anew" });
    on.onclick = () => {
      stage.resume(last.messages);
      transcript.replaceChildren();
      starters.hidden = true;
      for (const s of last.said) {
        if (s.who === "user") entry("user", s.text);
        else {
          const said = entry("said");
          said.dataset.words = s.text;
          render(s.text, said, { states: editor.states?.(), show: (state) => editor.show(state) });
        }
      }
      question.focus();
    };
    anew.onclick = () => {
      void talks.discard(key);
      li.remove();
    };
    li.append(on, " ", anew);
  }

  function failure(message: string): AssistantEvent {
    entry("failed", message);
    return { kind: "failed", message };
  }

  /** Settled once the ways to begin have first been on the screen. */
  let looked: Promise<void> | undefined;
  /** Once a person may see the ways to begin: the deck's health is read only then, from the deck as
   * it stands. Read as the panel is made, it loaded the generator's module at every start of the
   * editor and set the worker's deck to the source the page held before the bundle opened, which a
   * request that carries no source (a slide asked for) was then answered from. */
  const onScreen = () =>
    (looked ??= new Promise<void>((seen) => {
      const watch = new IntersectionObserver((entries) => {
        if (!entries.some((e) => e.isIntersecting)) return;
        watch.disconnect();
        seen();
      });
      watch.observe(starters);
    }));

  /** Ways to begin, under an empty conversation (lift-up 20261110 ATTR-2): asks the deck's own
   * story suggests, its health first, each a button that asks it. The beat whose slides health
   * finds the most to look at is named, where the deck has beats, once the ways are on the screen;
   * they go once a question is asked, and come back with a new conversation. */
  async function offer() {
    const stronger: [string, string] = [
      "Make this talk stronger",
      "Make this talk stronger. Read its health first, then fix what it says matters most, one change at a time, and tell me what you changed and why.",
    ];
    const claims: [string, string] = [
      "Check every slide says its claim",
      "Check that every slide says its claim: read the deck's health and each slide's headline, and tell me the slides whose words don't carry their beat's claim, each with a headline that would.",
    ];
    const offering = (asks: [string, string][]) => {
      starters.replaceChildren(
        ...asks.map(([label, words]) => {
          const button = Object.assign(document.createElement("button"), { type: "button", textContent: label, title: words });
          button.onclick = () => void ask(words);
          return button;
        }),
      );
      starters.hidden = false;
    };
    offering([stronger, claims]);
    await onScreen();
    if (transcript.children.length || working) return;
    const health = await stage.health(editor.source()).catch(() => undefined);
    if (transcript.children.length || working || starters.hidden) return;
    // The part of the talk, by the arc's beat each slide was written for, whose slides health finds
    // the most to look at: an error weighs more than a warning, an info nothing.
    const weight = { error: 3, warning: 2, info: 0 };
    const parts = new Map<number, number>();
    for (const f of health?.findings ?? []) {
      const part = health?.slides[f.slide - 1]?.part;
      if (part !== undefined && weight[f.severity]) parts.set(part, (parts.get(part) ?? 0) + weight[f.severity]);
    }
    const weakest = [...parts].sort(([, a], [, b]) => b - a)[0]?.[0];
    const named = weakest === undefined ? undefined : health?.beats[weakest];
    if (!named) return;
    // Its name as the arc says it ("Proof: case studies, data, demos"), the part before a colon on
    // the button.
    const short = named.split(":")[0].trim();
    offering([
      stronger,
      [`Tighten \u201c${short}\u201d`, `Tighten the \u201c${named}\u201d part of the talk: fewer words on its slides, each slide saying its claim, nothing the audience needs lost.`],
      claims,
    ]);
  }

  // A bundle opened anew starts a conversation anew: the worker that held the last is gone. The
  // one kept for the deck is offered (FUNC-4), beside the ways to begin anew.
  transcript.replaceChildren();
  void offer();
  void offerKept();
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
