// The deck's versions (PLAN 2.60, SPEC §8): a bundle that keeps a history lists its changes, the
// newest first, each by its author and when, as `scaena history` lists them.
//
// - A version chosen shows the deck as it was, read only: a state of it at rest, picked from its
//   own, and what changed from it to the deck as it is now, or to another version, as `scaena
//   history --diff` says it.
// - Restore makes it the deck again, with the data files it read, as one change by the user:
//   refused, with why, where the deck would not validate in the bundle as it is. The source takes
//   it as one change, which ⌘Z undoes with the data files, as any other edit.
// - The versions are what the history holds: an edit is one once a save records it.
import { listen, observing } from "./life";
import type { Compared, Lexicon, NodeChange, Restored, StateChange, Version } from "./protocol";
import type { Stage } from "./stage";
import { object, phrase, quote } from "./words";

/** What the panel asks of the editor around it. */
export interface VersionsEditor {
  /** The state shown, by its id and its slot's index; none while the source does not compile. */
  shown(): { state: string; index: number } | undefined;
  source(): string;
  /** Make `version` the deck again, as Restore does: what it did, said. */
  restore(version: Version): Promise<Restored | undefined>;
  /** Begin a history: save the bundle, the deck as saved its first version (PLAN 2.87). */
  keep(): Promise<void>;
  say(text: string): void;
  /** The words for the state shown (PLAN 2.99): the deck's objects by what they show. */
  words?(): Lexicon | undefined;
  /** `state` as a person counts it in the deck now: Slide 3, step 2; none for a state it has not. */
  slide?(state: string): string | undefined;
}

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
const html = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

/** An object's property as what changed says it. */
const PROPS: Record<string, string> = {
  text: "words",
  runs: "words",
  list: "list",
  role: "text style",
  style: "look",
  emphasis: "emphasis",
  at: "place",
  formats: "place in other sizes",
  transform: "turn and size",
  opacity: "opacity",
  fill: "fill",
  stroke: "border",
  radius: "corners",
  points: "shape",
  src: "picture",
  fit: "fit",
  focal: "focal point",
  crop: "crop",
  alt: "description",
  semantic: "part in the story",
  data: "data",
  encoding: "chart",
  kind: "chart",
  motion: "animation",
  enter: "build in",
  exit: "build out",
  preset: "effect",
  locked: "lock",
  z: "order",
};
/** A state's own field as what changed says it. */
const FIELDS: Record<string, string> = { layout: "layout", transition: "transition", hold: "advance", notes: "presenter notes" };
/** The deck's own field as what changed says it: its spine is its story, as the Story tab has it. */
const DECK: Record<string, string> = {
  spine: "story",
  canvas: "slide size",
  formats: "other sizes",
  data: "data sources",
  meta: "details",
  furniture: "slide numbers and footer",
};

/** Words that changed from `before` to `after`, quoted as they read now: the part that differs,
 * with a few characters either side, cut with an ellipsis where the text goes on, so that a
 * change at the end of a long text shows. */
export function retyped(before: string, after: string): string {
  const [was, now] = [[...before.trim()], [...after.trim()]];
  let head = 0;
  while (head < was.length && head < now.length && was[head] === now[head]) head++;
  let tail = 0;
  while (tail < was.length - head && tail < now.length - head && was[was.length - 1 - tail] === now[now.length - 1 - tail]) tail++;
  const around = 12;
  const from = Math.max(0, head - around);
  const to = Math.min(now.length, Math.max(now.length - tail + around, from + 40));
  const cut = now.slice(from, to).join("").replace(/\s+/g, " ").trim();
  return `“${from > 0 ? "…" : ""}${cut}${to < now.length ? "…" : ""}”`;
}

/** When a version was made, as a person reads it, in this browser's time zone. */
export function when(at: string | null | undefined): string {
  if (!at) return "at a time it did not keep";
  const date = new Date(at);
  if (Number.isNaN(date.getTime())) return at;
  return date.toLocaleString(undefined, { year: "numeric", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

/** A version as a person reads it: what was done, by whom, and when. What and who are the
 * engine's words (`scaena_core::words::change`, `author`), as the Mac's Versions says them. */
export function saying(v: Version): string {
  return `${v.said} · ${v.by} · ${when(v.at)}`;
}

/** What changed, a line each, in a person's words (PLAN 2.99): each slide, by its number in the
 * deck now (`slide`), and what changed on it, each object named as `words` names it; then the
 * deck's own settings, then the files. */
export function changes(c: Compared, words?: Lexicon, slide: (state: string) => string | undefined = () => undefined): string[] {
  const out: string[] = [];
  // An object by what it shows now; one whose words changed, by its words before, which tells two
  // texts apart, and the words that changed as they read now, or, where only their look changed,
  // so; else by its kind.
  const named = (n: string) => (words?.objects[n] === undefined ? "an object" : object(words, n, true));
  const kind = (n: string) => `the ${(words?.kinds[n] ?? "object").toLowerCase()}`;
  const parts = (p: Record<string, unknown>, typed?: [string, string]) =>
    [
      ...new Set(
        Object.entries(p).map(([k, v]) =>
          (k === "text" || k === "runs") && typed
            ? typed[0] === typed[1]
              ? "the look of its words changed"
              : `its words now ${retyped(typed[0], typed[1])}`
            : k === "text" && typeof v === "string"
              ? `its words now ${quote(v)}`
              : `its ${PROPS[k] ?? phrase(k).toLowerCase()} changed`,
        ),
      ),
    ].join(", ");
  const node = (state: string, n: string, how: NodeChange) => {
    if ("enter" in how) return `${named(n)} added`;
    if ("exit" in how) return `${named(n)} taken off`;
    if (!("text" in how.change || "runs" in how.change)) return `${named(n)}: ${parts(how.change)}`;
    const typed = c.texts?.[state]?.[n];
    return `${typed?.[0].trim() ? quote(typed[0]) : kind(n)}: ${parts(how.change, typed)}`;
  };
  for (const [id, s] of Object.entries(c.states) as [string, StateChange][]) {
    const which = slide(id);
    if ("added" in s) out.push(`${which ?? "A slide"} added`);
    else if ("removed" in s) out.push(which ? `${which} taken out` : "A slide taken out");
    else {
      const fields = Object.keys(s.changed.fields ?? {}).map((k) => `its ${FIELDS[k] ?? phrase(k).toLowerCase()} changed`);
      const nodes = Object.entries(s.changed.nodes ?? {}).map(([n, how]) => node(id, n, how));
      out.push(`${which ?? "A slide"}: ${[...fields, ...nodes].join("; ")}`);
    }
  }
  for (const field of Object.keys(c.deck)) {
    out.push(field === "order" ? "The slides' order changed" : `The deck's ${DECK[field] ?? phrase(field).toLowerCase()} changed`);
  }
  for (const path of c.files) out.push(`${path.replace(/^.*\//, "")} changed`);
  return out;
}

/** The panel in `into`, over `stage`. */
export function versionsPanel(stage: Stage, into: HTMLElement, editor: VersionsEditor) {
  const head = into.querySelector<HTMLElement>("[data-summary]")!;
  const list = into.querySelector<HTMLOListElement>("[data-versions]")!;
  const detail = into.querySelector<HTMLElement>("[data-version]")!;
  const title = detail.querySelector<HTMLElement>("[data-title]")!;
  const states = detail.querySelector<HTMLSelectElement>("[data-state]")!;
  const against = detail.querySelector<HTMLSelectElement>("[data-against]")!;
  const picture = detail.querySelector<HTMLImageElement>("img")!;
  const why = detail.querySelector<HTMLElement>("[data-why]")!;
  const changed = detail.querySelector<HTMLUListElement>("[data-changes]")!;
  const restore = detail.querySelector<HTMLButtonElement>("[data-restore]")!;
  const keep = into.querySelector<HTMLButtonElement>("[data-keep]")!;
  /** The versions as the history last listed them, oldest first; null where it keeps none. */
  let versions: Version[] | null = [];
  /** The version shown, by its id. */
  let chosen: string | undefined;
  /** Listings, drawings, and comparisons asked for, each counted apart: an answer to one before
   * the last of its kind is let go, so an edit's comparison leaves a drawing on its way. */
  let listing = 0;
  let drawing = 0;
  let comparing = 0;
  let url: string | undefined;
  /** Once the panel's last ask is answered. */
  let settling: Promise<unknown> = Promise.resolve();
  const settle = <T>(work: Promise<T>) => {
    settling = work;
    return work;
  };
  const by = (id: string | undefined) => versions?.find((v) => v.id === id);

  /** List the versions again, and show the one chosen, while the panel is shown. */
  function refresh() {
    if (into.hidden) return settling;
    const asked = ++listing;
    return settle(
      (async () => {
        try {
          const got = await stage.versions();
          if (asked !== listing) return;
          versions = got;
          draw();
        } catch (e) {
          if (asked !== listing) return;
          versions = [];
          draw();
          head.textContent = `The versions could not be read: ${said(e)}`;
        }
        if (chosen && by(chosen)) await show(chosen);
        else hide();
      })(),
    );
  }

  function draw() {
    keep.hidden = versions !== null;
    if (versions === null) {
      head.textContent =
        "This deck keeps no history. Keep a history saves it now, as its first version, and each save after keeps the edits since the last.";
      list.replaceChildren();
      return;
    }
    const n = versions.length;
    head.textContent = n
      ? `${n} ${n === 1 ? "version" : "versions"}, the last ${when(versions[n - 1].at)} · each save keeps the edits since the last`
      : "The history holds no versions yet.";
    list.innerHTML = versions
      .slice()
      .reverse()
      .map(
        (v) =>
          `<li><button type="button" data-id="${html(v.id)}" aria-pressed="${v.id === chosen}"><span class="n">${v.n}</span> ${html(v.said)} <span class="by">${html(v.by)} · ${html(when(v.at))}</span></button></li>`,
      )
      .join("");
  }

  function hide() {
    chosen = undefined;
    detail.hidden = true;
    for (const b of list.querySelectorAll("button")) b.setAttribute("aria-pressed", "false");
  }

  /** Version `id` shown: `state` of it at rest, and what changed from it. */
  async function show(id: string, state = states.value || editor.shown()?.state) {
    const v = by(id);
    if (!v) return;
    const asked = ++drawing;
    if (chosen !== id) {
      against.replaceChildren(
        new Option("the deck now", ""),
        ...versions!.filter((x) => x.id !== id).map((x) => new Option(`version ${x.n}: ${x.said}`, x.id)),
      );
    }
    chosen = id;
    for (const b of list.querySelectorAll<HTMLButtonElement>("button[data-id]")) b.setAttribute("aria-pressed", String(b.dataset.id === id));
    title.textContent = `Version ${v.n}: ${saying(v)}`;
    detail.hidden = false;
    try {
      const width = Math.round(Math.max(320, detail.clientWidth || 480) * Math.min(devicePixelRatio || 1, 2));
      const got = await stage.version(id, state, width);
      if (asked !== drawing) return;
      // Each of its states as its own slide list would number it (PLAN 2.99).
      const slideOf = (s: string) => got.said[got.states.indexOf(s)] ?? s;
      states.replaceChildren(...got.states.map((s) => new Option(slideOf(s), s)));
      states.value = got.state;
      if (url) URL.revokeObjectURL(url);
      url = got.png && URL.createObjectURL(new Blob([got.png], { type: "image/png" }));
      depict(url, got.png ? `${slideOf(got.state)} as version ${v.n} drew it, at rest` : `Version ${v.n} is not drawn: ${got.why}`);
    } catch (e) {
      if (asked !== drawing) return;
      depict(undefined, `Version ${v.n} is not shown: ${said(e)}`);
      changed.replaceChildren();
      return;
    }
    await compared();
  }

  /** The picture of the version shown, from `src`, `text` its description; or, where there is
   * none, `text` why. */
  function depict(src: string | undefined, text: string) {
    if (src) picture.src = src;
    else picture.removeAttribute("src");
    picture.alt = src ? text : "";
    picture.hidden = !src;
    why.textContent = src ? "" : text;
    why.hidden = Boolean(src);
  }

  function item(text: string) {
    const li = document.createElement("li");
    li.textContent = text;
    return li;
  }

  /** What changed from the version shown to the one it is compared with, or to the deck now:
   * listed, unless another comparison was asked for since. */
  async function compared() {
    const asked = ++comparing;
    const v = by(chosen);
    if (!v) return;
    const to = by(against.value);
    try {
      const got = await stage.compareVersions(editor.source(), v.id, to?.id);
      if (asked !== comparing) return;
      const lines = changes(got, editor.words?.(), (s) => editor.slide?.(s));
      const them = to ? `version ${to.n}` : "the deck now";
      changed.setAttribute("aria-label", `What changed from version ${v.n} to ${them}`);
      changed.replaceChildren(...(lines.length ? lines : [`Nothing: version ${v.n} draws and reads as ${them} does`]).map(item));
    } catch (e) {
      if (asked === comparing) changed.replaceChildren(item(`Not compared: ${said(e)}`));
    }
  }

  /** The version shown made the deck again, as one change. */
  async function restoring() {
    const v = by(chosen);
    if (!v) return;
    const done = await editor.restore(v);
    if (done?.applied) await compared();
    return done;
  }

  listen(list, "click", (e) => {
    const id = (e.target as Element).closest<HTMLButtonElement>("button[data-id]")?.dataset.id;
    if (id) void settle(show(id));
  });
  listen(states, "change", () => {
    if (chosen) void settle(show(chosen, states.value));
  });
  listen(against, "change", () => void settle(compared()));
  listen(restore, "click", () => void settle(restoring()));
  // A history begun with a save, the deck as saved its first version (PLAN 2.87); listed then.
  listen(keep, "click", () => {
    keep.disabled = true;
    void settle(editor.keep().then(refresh, (e) => editor.say(`No history begun: ${said(e)}`)))
      .finally(() => (keep.disabled = false));
  });
  observing(
    new MutationObserver(() => {
      if (!into.hidden) void refresh();
    }),
  ).observe(into, { attributes: true, attributeFilter: ["hidden"] });

  return {
    refresh,
    /** The deck changed: what changed from the version shown to it, again, where it is compared
     * with the deck now. */
    edited: () => (!into.hidden && chosen && !against.value ? settle(compared()) : settling),
    /** The versions as the panel last listed them, oldest first, for a test. */
    listed: () => versions,
    /** Show version `id`, and `state` of it. */
    show: (id: string, state?: string) => settle(show(id, state)),
    /** Compare the version shown with version `to`, or with the deck now. */
    compare: (to?: string) => {
      against.value = to ?? "";
      return settle(compared());
    },
    /** Restore the version shown, as its button does. */
    restore: () => settle(restoring()),
    /** What the panel says changed, a line each, for a test. */
    changes: () => [...changed.querySelectorAll("li")].map((li) => li.textContent ?? ""),
    /** Once the panel's last ask is answered. */
    settled: () => settling,
  };
}
