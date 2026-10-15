// The icon picker (PLAN 2.104): a field that finds icons in the set that ships (`icon-set.ts`),
// a weight, and the icons found, drawn. An icon chosen is inserted as a path shape on its view
// (`Player.insertingIcon`), or takes the place of the icon selected (PLAN G.23).
import { findIcons, type Icon, type IconEntry, type IconSet, iconSaid, iconSet, type Weight } from "./icon-set";
import { listen } from "./life";

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
  const heading = dialog.querySelector<HTMLElement>("h2")!;
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

  listen(field, "input", show);
  listen(solid, "change", show);
  // Enter in the field takes the first icon found; the arrows go into the icons.
  listen(field, "keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      if (found[0]) choose(found[0]);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      grid.querySelector<HTMLButtonElement>("button.icon")?.focus();
    }
  });
  // The icons are one stop for Tab, and the arrows go among them, a row at a time up and down.
  listen(grid, "keydown", (e) => {
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
  listen(grid, "focusin", (e) => {
    for (const b of grid.querySelectorAll("button.icon")) b.setAttribute("aria-selected", String(b === e.target));
  });
  listen(dialog, "close", () => {
    answer?.(undefined);
    answer = undefined;
  });

  return {
    /** Open the picker under `title` (Insert an icon where none is said), and give the icon
     * chosen, or undefined where none is. */
    async pick(title = "Insert an icon"): Promise<Icon | undefined> {
      heading.textContent = title;
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
