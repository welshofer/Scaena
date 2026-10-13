// The icons that ship (PLAN 2.104): Phosphor 2.1.1's, MIT, every icon but brands, in its regular
// and fill weights, each path on a view 256 square (`docs/icons/phosphor.json`, which
// `scripts/build_icons.py` writes). The build carries the set beside the pages, fetched the first
// time the picker opens. An icon chosen is inserted as a path shape on its view
// (`Player.insertingIcon`). The picker finds icons by the rule `scaena_ops::icons` finds them by.
import phosphor from "../../docs/icons/phosphor.json?url";

/** One icon of the set: its name, tags, and categories, and its path in each weight. */
export interface IconEntry {
  name: string;
  tags: string[];
  categories: string[];
  regular: string;
  fill: string;
}

export interface IconSet {
  view: [number, number, number, number];
  icons: IconEntry[];
}

export type Weight = "regular" | "fill";

/** An icon to insert, as `Player.insertingIcon` takes it. */
export interface Icon {
  name: string;
  path: string;
  view: [number, number, number, number];
}

let loading: Promise<IconSet> | undefined;

/** The set, fetched once. */
export function iconSet(): Promise<IconSet> {
  loading ??= fetch(phosphor).then((r) => {
    if (!r.ok) throw new Error(`the icons did not load (${r.status})`);
    return r.json() as Promise<IconSet>;
  });
  loading.catch(() => (loading = undefined));
  return loading;
}

/** `text`'s words, lowercase: what is not a letter or a digit parts them. */
const words = (text: string) =>
  text
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean)
    .map((w) => w.toLowerCase());

/** How well `icon` answers `asked`, as `scaena_ops::icons::rank` says, best 0; undefined where a
 * word is in none of its words. */
export function rank(asked: string[], icon: IconEntry): number | undefined {
  if (asked.length === 0 || asked.join("-") === icon.name) return 0;
  const all = (pool: string[], hit: (n: string, w: string) => boolean) => asked.every((w) => pool.some((n) => hit(n, w)));
  const every = words(icon.name);
  if (all(every, (n, w) => n === w)) return 1;
  if (all(every, (n, w) => n.startsWith(w))) return 2;
  // Inside a name, a word of five letters or more: `plane` in `airplane`, not `team` in `steam`.
  if (all(every, (n, w) => [...w].length >= 5 && n.includes(w))) return 3;
  const more = [...every, ...icon.tags.flatMap(words), ...icon.categories.flatMap(words)];
  if (all(more, (n, w) => n === w)) return 4;
  if (all(more, (n, w) => n.startsWith(w))) return 5;
  return undefined;
}

/** The icons `query` finds in `set`, best first: every icon, by name, for no words. */
export function findIcons(set: IconSet, query: string): IconEntry[] {
  const asked = words(query);
  const found = set.icons.flatMap((icon) => {
    const r = rank(asked, icon);
    return r === undefined ? [] : [{ r, icon }];
  });
  const long = (i: IconEntry) => (asked.length ? i.name.length : 0);
  const byName = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
  found.sort((a, b) => a.r - b.r || long(a.icon) - long(b.icon) || byName(a.icon.name, b.icon.name));
  return found.map((f) => f.icon);
}

/** An icon's name as a person reads it: `airplane-tilt` as Airplane tilt. */
export const iconSaid = (name: string) => {
  const said = name.replace(/-/g, " ");
  return said.charAt(0).toUpperCase() + said.slice(1);
};

/** How many icons the picker draws at once: a person narrows the rest by words. */
const SHOWN = 240;

/** The picker in `dialog`: a field that finds icons by their words, a weight, and the icons it
 * finds, drawn, the arrow keys going among them. `pick` opens it and gives the icon chosen, or
 * undefined where none is. */
export function iconPicker(dialog: HTMLDialogElement) {
  const field = dialog.querySelector<HTMLInputElement>("input.find")!;
  const solid = dialog.querySelector<HTMLInputElement>("input.solid")!;
  const grid = dialog.querySelector<HTMLElement>(".icons")!;
  const count = dialog.querySelector<HTMLElement>(".count")!;
  let set: IconSet | undefined;
  let found: IconEntry[] = [];
  let answer: ((icon?: Icon) => void) | undefined;

  const weight = (): Weight => (solid.checked ? "fill" : "regular");
  const icon = (entry: IconEntry): Icon => ({ name: entry.name, path: entry[weight()], view: set!.view });

  function show() {
    if (!set) return;
    found = findIcons(set, field.value);
    const view = set.view.join(" ");
    const buttons = found.slice(0, SHOWN).map((entry, i) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "icon";
      b.setAttribute("role", "option");
      b.setAttribute("aria-selected", "false");
      b.tabIndex = i === 0 ? 0 : -1;
      b.title = iconSaid(entry.name);
      b.setAttribute("aria-label", iconSaid(entry.name));
      b.dataset.icon = entry.name;
      // The path is the set's own, written in M, L, C, and Z alone: no markup reaches the page.
      b.innerHTML = `<svg viewBox="${view}" aria-hidden="true"><path d="${entry[weight()]}"/></svg>`;
      b.onclick = () => choose(entry);
      return b;
    });
    grid.replaceChildren(...buttons);
    const asked = field.value.trim();
    count.textContent =
      found.length === 0
        ? `No icon answers “${asked}”`
        : found.length > SHOWN
          ? `${SHOWN} of ${found.length} icons: type to find fewer`
          : `${found.length} icon${found.length === 1 ? "" : "s"}`;
  }

  function choose(entry: IconEntry) {
    const chosen = icon(entry);
    answer?.(chosen);
    answer = undefined;
    dialog.close();
  }

  field.addEventListener("input", show);
  solid.addEventListener("change", show);
  // Enter in the field takes the first icon found; the arrows go into the icons.
  field.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      if (found[0]) choose(found[0]);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      grid.querySelector<HTMLButtonElement>("button.icon")?.focus();
    }
  });
  // The icons are one stop for Tab, and the arrows go among them, a row at a time up and down.
  grid.addEventListener("keydown", (e) => {
    const buttons = [...grid.querySelectorAll<HTMLButtonElement>("button.icon")];
    const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (at < 0) return;
    const top = buttons[0]?.offsetTop;
    const across = Math.max(1, buttons.filter((b) => b.offsetTop === top).length);
    const step = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -across, ArrowDown: across, Home: -at, End: buttons.length - 1 - at }[e.key];
    if (step === undefined) return;
    e.preventDefault();
    if (e.key === "ArrowUp" && at < across) return field.focus();
    const next = buttons[Math.min(Math.max(at + step, 0), buttons.length - 1)];
    buttons[at].tabIndex = -1;
    next.tabIndex = 0;
    next.focus();
  });
  grid.addEventListener("focusin", (e) => {
    for (const b of grid.querySelectorAll("button.icon")) b.setAttribute("aria-selected", String(b === e.target));
  });
  dialog.addEventListener("close", () => {
    answer?.(undefined);
    answer = undefined;
  });

  return {
    /** Open the picker, and give the icon chosen, or undefined where none is. */
    async pick(): Promise<Icon | undefined> {
      count.textContent = "Loading the icons…";
      grid.replaceChildren();
      dialog.showModal();
      field.select();
      const chosen = new Promise<Icon | undefined>((done) => (answer = done));
      try {
        set = await iconSet();
        show();
      } catch (e) {
        count.textContent = `The icons did not load: ${e instanceof Error ? e.message : String(e)}`;
      }
      return chosen;
    },
    /** The icons the picker shows, by name, for a test. */
    shown: () => found.slice(0, SHOWN).map((e) => e.name),
  };
}
