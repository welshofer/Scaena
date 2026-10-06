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
import type { Compared, NodeChange, Restored, StateChange, Version } from "./protocol";
import type { Stage } from "./stage";

/** What the panel asks of the editor around it. */
export interface VersionsEditor {
  /** The state shown, by its id and its slot's index; none while the source does not compile. */
  shown(): { state: string; index: number } | undefined;
  source(): string;
  /** Make `version` the deck again, as Restore does: what it did, said. */
  restore(version: Version): Promise<Restored | undefined>;
  say(text: string): void;
}

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
const html = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
/** A value as the comparison says it: gone, or as JSON, cut short. */
const value = (v: unknown) => {
  const text = v === null ? "gone" : JSON.stringify(v);
  return text.length > 80 ? `${text.slice(0, 79)}…` : text;
};

/** When a version was made, as a person reads it, in this browser's time zone. */
export function when(at: string | null | undefined): string {
  if (!at) return "at a time it did not keep";
  const date = new Date(at);
  if (Number.isNaN(date.getTime())) return at;
  return date.toLocaleString(undefined, { year: "numeric", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

/** A version as a person reads it: what was done, by whom, and when. */
export function saying(v: Version): string {
  return `${v.message || "a change"} · ${v.author || "someone"} · ${when(v.at)}`;
}

/** What changed, a line each: each state, then the deck's own fields, then the data files. */
export function changes(c: Compared): string[] {
  const out: string[] = [];
  const props = (p: Record<string, unknown>) =>
    Object.entries(p)
      .map(([k, v]) => `${k} ${value(v)}`)
      .join(", ");
  const node = (id: string, n: NodeChange) =>
    "enter" in n ? `${id} enters (${props(n.enter)})` : "exit" in n ? `${id} exits` : `${id}: ${props(n.change)}`;
  for (const [id, s] of Object.entries(c.states) as [string, StateChange][]) {
    if ("added" in s) out.push(`state ${id} added`);
    else if ("removed" in s) out.push(`state ${id} taken out`);
    else {
      const fields = Object.entries(s.changed.fields ?? {}).map(([k, v]) => `its ${k} ${value(v)}`);
      const nodes = Object.entries(s.changed.nodes ?? {}).map(([n, how]) => node(n, how));
      out.push(`state ${id}: ${[...fields, ...nodes].join("; ")}`);
    }
  }
  for (const [field, v] of Object.entries(c.deck)) {
    out.push(field === "order" ? `the states' order: ${(v as string[]).join(", ")}` : `the deck's ${field}: ${value(v)}`);
  }
  for (const path of c.files) out.push(`${path} changed`);
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
          head.textContent = `the versions could not be read: ${said(e)}`;
        }
        if (chosen && by(chosen)) await show(chosen);
        else hide();
      })(),
    );
  }

  function draw() {
    if (versions === null) {
      head.textContent = "This bundle keeps no history: `scaena save --history` begins one, and each save after records the edits since the last.";
      list.replaceChildren();
      return;
    }
    const n = versions.length;
    head.textContent = n
      ? `${n} ${n === 1 ? "version" : "versions"}, the last ${when(versions[n - 1].at)} · each save records the edits since the last`
      : "The history holds no versions yet.";
    list.innerHTML = versions
      .slice()
      .reverse()
      .map(
        (v) =>
          `<li><button type="button" data-id="${html(v.id)}" aria-pressed="${v.id === chosen}"><span class="n">${v.n}</span> ${html(v.message || "a change")} <span class="by">${html(v.author || "someone")} · ${html(when(v.at))}</span></button></li>`,
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
        ...versions!.filter((x) => x.id !== id).map((x) => new Option(`version ${x.n}: ${x.message || "a change"}`, x.id)),
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
      states.replaceChildren(...got.states.map((s) => new Option(s, s)));
      states.value = got.state;
      if (url) URL.revokeObjectURL(url);
      url = got.png && URL.createObjectURL(new Blob([got.png], { type: "image/png" }));
      depict(url, got.png ? `State ${got.state} as version ${v.n} drew it, at rest` : `Version ${v.n} is not drawn: ${got.why}`);
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
      const lines = changes(got);
      const them = to ? `version ${to.n}` : "the deck now";
      changed.setAttribute("aria-label", `What changed from version ${v.n} to ${them}`);
      changed.replaceChildren(...(lines.length ? lines : [`nothing: version ${v.n} draws and reads as ${them} does`]).map(item));
    } catch (e) {
      if (asked === comparing) changed.replaceChildren(item(`not compared: ${said(e)}`));
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

  list.addEventListener("click", (e) => {
    const id = (e.target as Element).closest<HTMLButtonElement>("button[data-id]")?.dataset.id;
    if (id) void settle(show(id));
  });
  states.addEventListener("change", () => {
    if (chosen) void settle(show(chosen, states.value));
  });
  against.addEventListener("change", () => void settle(compared()));
  restore.addEventListener("click", () => void settle(restoring()));
  new MutationObserver(() => {
    if (!into.hidden) void refresh();
  }).observe(into, { attributes: true, attributeFilter: ["hidden"] });

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
