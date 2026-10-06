// Commands by name (PLAN 2.53, ADR-0013). Every command the editor has, with its key, in a palette
// that ⌘K opens and the words typed narrow; the words themselves, last, asked of the assistant. A
// right click on a node, on the canvas, on a state in the strip, or on a layer offers the commands
// for what is under the pointer, in a menu. A command is what its key or its button does, nothing
// else: each edit it makes is the patch that gesture makes.

/** A command the editor has. */
export interface Command {
  /** What the palette and a menu call it. */
  label: string;
  /** Its key, as the page shows it (`⌘D`, `Delete`). */
  keys?: string;
  /** What a right click offers it on, besides the palette. */
  where?: Where[];
  /** Whether it applies now: one that does not is not offered. */
  applies?(): boolean;
  run(): unknown;
}

/** What a right click is on: a node, on the canvas or in the layers; the canvas where nothing is;
 * a state in the strip; a layer, shown or not. */
export type Where = "node" | "canvas" | "state" | "layer";

/** The modifier key as this machine shows it: `⌘` on a Mac, `Ctrl+` elsewhere; and Shift. */
export const MOD = /Mac|iPhone|iPad/.test(navigator.platform) ? "⌘" : "Ctrl+";
export const SHIFT = MOD === "⌘" ? "⇧" : "Shift+";
/** ⌘ with Option, or Ctrl with Alt: what copies and pastes a look (PLAN 2.58). */
export const MOD_ALT = MOD === "⌘" ? "⌥⌘" : "Ctrl+Alt+";

/** The commands of `all` that apply now and name every word of `query`, in any order, case
 * folded: those whose label starts with the first word first, else in their order. */
export function narrowed(all: Command[], query: string): Command[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const live = all.filter((c) => c.applies?.() ?? true);
  if (!words.length) return live;
  const found = live.filter((c) => {
    const label = `${c.label} ${c.keys ?? ""}`.toLowerCase();
    return words.every((w) => label.includes(w));
  });
  const leads = (c: Command) => (c.label.toLowerCase().startsWith(words[0]) ? 0 : 1);
  return found
    .map((c, i) => [c, i] as const)
    .sort((a, b) => leads(a[0]) - leads(b[0]) || a[1] - b[1])
    .map(([c]) => c);
}

const html = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
const item = (c: Command) => `<span>${html(c.label)}</span>${c.keys ? `<kbd>${html(c.keys)}</kbd>` : ""}`;

/** The palette in `dialog` (a search box and a listbox): `commands` gives what the editor has now,
 * and `ask` takes words for the assistant. Focus goes back where it was when it closes. */
export function palette(dialog: HTMLDialogElement, commands: () => Command[], ask: (words: string) => void) {
  const input = dialog.querySelector<HTMLInputElement>("input")!;
  const list = dialog.querySelector<HTMLElement>("[role=listbox]")!;
  let shown: Command[] = [];
  let at = 0;
  let back: Element | null = null;

  function render() {
    const query = input.value.trim();
    shown = narrowed(commands(), query);
    if (query) shown = [...shown, { label: `Ask the assistant: “${query}”`, run: () => ask(query) }];
    at = Math.min(at, Math.max(0, shown.length - 1));
    list.innerHTML = shown.map((c, i) => `<li role="option" id="command-${i}" data-n="${i}" aria-selected="${i === at}">${item(c)}</li>`).join("");
    if (shown.length) input.setAttribute("aria-activedescendant", `command-${at}`);
    else input.removeAttribute("aria-activedescendant");
    list.querySelector(`[data-n="${at}"]`)?.scrollIntoView({ block: "nearest" });
  }

  /** Open it, the words `query` typed already. */
  function open(query = "") {
    if (!dialog.open) {
      back = document.activeElement;
      dialog.showModal();
    }
    input.value = query;
    at = 0;
    render();
    input.focus();
  }

  /** Focus back where it was when the palette opened. */
  const restore = () => {
    if (back instanceof HTMLElement && back.isConnected) back.focus();
  };

  function close() {
    if (!dialog.open) return;
    dialog.close();
    restore();
  }

  function run(n: number) {
    const command = shown[n];
    if (!command) return;
    close();
    void Promise.resolve(command.run());
  }

  input.oninput = () => {
    at = 0;
    render();
  };
  input.onkeydown = (e) => {
    const step = { ArrowDown: 1, ArrowUp: -1 }[e.key];
    if (step !== undefined) {
      e.preventDefault();
      at = (at + step + shown.length) % Math.max(1, shown.length);
      return render();
    }
    if (e.key === "Enter" && !e.isComposing) {
      e.preventDefault();
      return run(at);
    }
  };
  list.onclick = (e) => {
    const n = (e.target as Element).closest<HTMLElement>("[data-n]")?.dataset.n;
    if (n !== undefined) run(Number(n));
  };
  // Escape closes a modal dialog by itself: focus goes back where it was, unless what ran has
  // put it somewhere already (the close is told after).
  dialog.addEventListener("close", () => {
    const at = document.activeElement;
    if (!at || at === document.body || dialog.contains(at)) restore();
  });
  dialog.onclick = (e) => {
    if (e.target === dialog) close();
  };
  return {
    open,
    close,
    /** What the palette lists now, for a test: none while it is closed. */
    shown: () => (dialog.open ? shown.map((c) => c.label) : []),
  };
}

/** A menu of the `items` that apply, at `x`, `y` (client pixels), focus on its first: the arrow
 * keys, Home, and End move in it, Enter or a click runs one, and Escape, Tab, or a press elsewhere
 * closes it. Focus goes back to `back`. None where nothing applies. */
export function menu(x: number, y: number, items: Command[], back: HTMLElement | null) {
  document.querySelector(".context-menu")?.dispatchEvent(new Event("dismiss"));
  const live = items.filter((c) => c.applies?.() ?? true);
  if (!live.length) return undefined;
  const el = document.createElement("div");
  el.className = "context-menu";
  el.setAttribute("role", "menu");
  el.innerHTML = live.map((c, i) => `<button type="button" role="menuitem" tabindex="-1" data-n="${i}">${item(c)}</button>`).join("");
  document.body.append(el);
  // Inside the window, as near the pointer as it fits.
  const { width, height } = el.getBoundingClientRect();
  el.style.left = `${Math.max(0, Math.min(x, innerWidth - width))}px`;
  el.style.top = `${Math.max(0, Math.min(y, innerHeight - height))}px`;
  const buttons = [...el.querySelectorAll<HTMLButtonElement>("button")];
  buttons[0]?.focus();
  let open = true;
  const close = (refocus = true) => {
    if (!open) return;
    open = false;
    el.remove();
    removeEventListener("pointerdown", outside, true);
    if (refocus && back?.isConnected) back.focus();
  };
  const outside = (e: Event) => {
    if (!el.contains(e.target as Node)) close(false);
  };
  addEventListener("pointerdown", outside, true);
  // Another menu opened over it.
  el.addEventListener("dismiss", () => close(false));
  el.onkeydown = (e) => {
    const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const to = { ArrowDown: i + 1, ArrowUp: i - 1, Home: 0, End: buttons.length - 1 }[e.key];
    if (to !== undefined) {
      e.preventDefault();
      return buttons[(to + buttons.length) % buttons.length].focus();
    }
    if (e.key === "Escape" || e.key === "Tab") {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  };
  el.onclick = (e) => {
    const n = (e.target as Element).closest<HTMLElement>("[data-n]")?.dataset.n;
    if (n === undefined) return;
    close();
    void Promise.resolve(live[Number(n)].run());
  };
  return { close, items: () => live.map((c) => c.label) };
}
