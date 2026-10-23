// The editor's toolbar (lift-up ATTR-5): one row of icon-and-label buttons in groups, as the
// Mac's toolbar has them (PLAN 3.18), over the controls `editor.html` declares with their ids, so
// every walk and key that reaches them by id goes on reaching them. Each button is given its icon
// from the set that ships (`toolbar-icons.ts`, written from docs/icons/phosphor.json), its words
// becoming the label under it; the File menu opens under its button and closes on a choice, on
// Escape, or on a press elsewhere; and the arrow keys move along the toolbar, as a toolbar's do.
import { TOOLBAR_ICONS, VIEW } from "./toolbar-icons";

/** Each control's icon, by the selector that finds it. */
const ICONS: [string, string][] = [
  ["#file-open", "file"],
  ["#new-deck", "file-plus"],
  ["#new-prompt", "sparkle"],
  ["#open-folder", "folder-open"],
  ["#open-scaena", "file-archive"],
  ["#kept", "bookmarks"],
  ["#save-as", "floppy-disk-back"],
  ["#download", "download-simple"],
  ["#save", "floppy-disk"],
  ["#export", "export"],
  ["#play", "play"],
  ["#source-toggle", "code"],
  ["#slides-open", "squares-four"],
  ["#formats-open", "device-mobile"],
  ["#grid", "grid-four"],
  ["#safe", "bounding-box"],
  ["#layout-edit", "layout"],
  ['[data-zoom="out"]', "magnifying-glass-minus"],
  ['[data-zoom="in"]', "magnifying-glass-plus"],
  ['[data-zoom="fit"]', "corners-in"],
  ["#rehearse-open", "timer"],
  ["#find-open", "magnifying-glass"],
  ["#commands-open", "command"],
  ["#keys-open", "keyboard"],
];

/** An icon's SVG, drawn in the text's color, which a reader passes over: the button's words name it. */
export function icon(name: string): SVGSVGElement {
  const path = TOOLBAR_ICONS[name];
  if (!path) throw new Error(`the toolbar has no icon ${name}`);
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", VIEW.join(" "));
  svg.setAttribute("aria-hidden", "true");
  const p = document.createElementNS("http://www.w3.org/2000/svg", "path");
  p.setAttribute("d", path);
  svg.append(p);
  return svg;
}

/** The controls in `bar` a key moves among: those shown, the menu's items apart. */
const along = (bar: HTMLElement) =>
  [...bar.querySelectorAll<HTMLElement>("button, select, a.button")].filter(
    (el) => !el.hidden && !el.closest("[role=menu]") && el.offsetParent !== null,
  );

/** Dress `bar`, the editor's toolbar: each control's icon over its label, the File menu, and the
 * arrow keys along the bar. */
export function toolbar(bar: HTMLElement) {
  for (const [selector, name] of ICONS) {
    const el = bar.querySelector<HTMLElement>(selector);
    if (!el) continue;
    const label = document.createElement("span");
    label.className = "label";
    label.append(...el.childNodes);
    el.replaceChildren(icon(name), label);
  }
  // The File menu (PLAN 3.18's File menu, in the page): its button opens it under itself, the
  // first item focused; a choice, Escape, Tab, or a press elsewhere closes it, Escape giving the
  // focus back to the button; Up and Down move among its items, Home and End to the ends.
  const opener = bar.querySelector<HTMLButtonElement>("#file-open");
  const fileMenu = bar.querySelector<HTMLElement>("#file-menu");
  if (opener && fileMenu) {
    const items = () => [...fileMenu.querySelectorAll<HTMLButtonElement>("button")].filter((b) => !b.hidden);
    const isOpen = () => !fileMenu.hidden;
    const outside = (e: Event) => {
      if (!fileMenu.contains(e.target as Node) && e.target !== opener) close(false);
    };
    const close = (refocus: boolean) => {
      if (!isOpen()) return;
      fileMenu.hidden = true;
      opener.setAttribute("aria-expanded", "false");
      removeEventListener("pointerdown", outside, true);
      if (refocus) opener.focus();
    };
    const open = (focusLast = false) => {
      if (isOpen()) return;
      // A context menu open elsewhere goes.
      document.querySelector(".context-menu")?.dispatchEvent(new Event("dismiss"));
      fileMenu.hidden = false;
      opener.setAttribute("aria-expanded", "true");
      addEventListener("pointerdown", outside, true);
      const all = items();
      (focusLast ? all.at(-1) : all[0])?.focus();
    };
    opener.onclick = () => (isOpen() ? close(true) : open());
    opener.onkeydown = (e) => {
      if (e.key === "ArrowDown") (e.preventDefault(), open());
      else if (e.key === "ArrowUp") (e.preventDefault(), open(true));
    };
    fileMenu.onkeydown = (e) => {
      const all = items();
      const i = all.indexOf(document.activeElement as HTMLButtonElement);
      const to = (n: number) => (e.preventDefault(), all[(n + all.length) % all.length]?.focus());
      if (e.key === "ArrowDown") to(i + 1);
      else if (e.key === "ArrowUp") to(i - 1);
      else if (e.key === "Home") to(0);
      else if (e.key === "End") to(all.length - 1);
      else if (e.key === "Escape") (e.preventDefault(), e.stopPropagation(), close(true));
      else if (e.key === "Tab") close(false);
    };
    // A choice closes the menu once the item has done its work.
    fileMenu.addEventListener("click", (e) => {
      if ((e.target as HTMLElement).closest("button")) close(false);
    });
    fileMenu.addEventListener("focusout", (e) => {
      const to = e.relatedTarget as Node | null;
      if (to && !fileMenu.contains(to) && to !== opener) close(false);
    });
  }
  // Left and Right move along the bar, Home and End to its ends, as a toolbar's keys do
  // (WAI-ARIA); a select keeps the keys that pick in it.
  bar.addEventListener("keydown", (e) => {
    const from = e.target as HTMLElement;
    if (from.closest("[role=menu]") || from instanceof HTMLSelectElement || from instanceof HTMLInputElement) return;
    const keys: Record<string, number | "start" | "end"> = { ArrowLeft: -1, ArrowRight: 1, Home: "start", End: "end" };
    const step = keys[e.key];
    if (step === undefined) return;
    const all = along(bar);
    const i = all.indexOf(from);
    if (i < 0) return;
    e.preventDefault();
    const n = step === "start" ? 0 : step === "end" ? all.length - 1 : (i + step + all.length) % all.length;
    all[n]?.focus();
  });
}
