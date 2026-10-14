// The theme's layouts to choose from, as a presentation app's layout chooser shows them (PLAN
// 3.31): the sections down the side, tabs that the arrow keys go up and down, and each layout of
// the one chosen drawn as it is used, a picture where a picture goes and a chart or a table where a
// figure goes, under the name a person calls it; and a field that finds layouts in every section by
// their names, what they are for, and their sections. The layouts shown are drawn first, a few at a
// time, then the rest, each kept for the next time the picker opens while the theme and the format
// stay. + Slide starts a slide in the layout chosen, or a blank one; the inspector gives the state
// shown the layout chosen, of those it may take, the one it takes outlined.

import type { Starter } from "./protocol";
import type { Stage } from "./stage";

/** What the picker is for: a slide to start, or the state shown's layout, one of `names`. */
export type Purpose = "start" | { names: string[]; current?: string };

export interface Picking {
  purpose: Purpose;
  /** The deck's source, the state shown, and the format it is shown in. */
  source: string;
  state: string;
  format?: string;
}

/** How high a slide is drawn in the picker, CSS pixels. */
const TILE = 84;
/** How many slides are drawn between two answers to a person. */
const BATCH = 3;

const html = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
/** A layout's name as a person reads it where its theme gives none: `two-columns` as Two columns. */
const phrase = (name: string) => {
  const words = name.replace(/-/g, " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
};
const named = (one: Starter) => one.name ?? (one.layout ? phrase(one.layout) : "Blank");
/** A section's name as an id's part: `Titles` as `titles`. */
const slug = (title: string) => title.toLowerCase().replace(/[^a-z0-9]+/g, "-");

/** Whether `one` is found by `words`: each word, in any case, in its name, what it is for, its
 * section, or its key, as the source names it (`narrow-figure`). */
function matches(one: Starter, words: string): boolean {
  const text = `${named(one)} ${one.description ?? ""} ${one.group ?? ""} ${one.layout ?? ""}`.toLowerCase();
  return words
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => text.includes(w));
}

/** The picker in `dialog`, over `stage`. `pick` opens it and gives what is chosen: a layout, null
 * for a blank slide, or undefined where nothing is. */
export function layoutPicker(stage: Stage, dialog: HTMLDialogElement) {
  const heading = dialog.querySelector<HTMLElement>("h2")!;
  const find = dialog.querySelector<HTMLInputElement>("input.find")!;
  const tabs = dialog.querySelector<HTMLElement>(".sections")!;
  const box = dialog.querySelector<HTMLElement>(".starters")!;
  const blank = dialog.querySelector<HTMLButtonElement>("button.blank")!;
  /** Each slide's picture drawn so far, by its layout, for the theme, the format, and the height in `key`. */
  let kept: { key: string; drawn: Map<string, Starter> } = { key: "", drawn: new Map() };
  /** The section shown last, which a picker opened to start a slide shows again. */
  let shownLast: string | undefined;
  /** Which opening of the picker is current: a round of drawing for another stops. */
  let opened = 0;

  function pick(p: Picking): Promise<string | null | undefined> {
    const round = ++opened;
    const start = p.purpose === "start";
    const current = typeof p.purpose === "object" ? p.purpose.current : undefined;
    heading.textContent = start ? "Add a slide" : "Layout";
    blank.hidden = !start;
    find.value = "";
    tabs.innerHTML = "";
    box.innerHTML = `<p class="note">Listing the theme's layouts…</p>`;
    const height = Math.round(TILE * Math.min(2, window.devicePixelRatio || 1));
    let offered: Starter[] = [];
    let section: string | undefined;
    /** What each tile's canvas shows, by its layout. */
    const tiles = new Map<string, HTMLButtonElement>();

    const sections = () => [...new Set(offered.filter((o) => o.layout).map((o) => o.group ?? "Other"))];
    const shown = () => {
      const words = find.value.trim();
      const all = offered.filter((o) => o.layout);
      return words ? all.filter((o) => matches(o, words)) : all.filter((o) => (o.group ?? "Other") === section);
    };

    /** The tabs, the one shown selected where nothing is typed to find, which alone takes Tab. */
    function drawTabs() {
      const searching = find.value.trim() !== "";
      // A tab that had the focus gives it to the tab selected, drawn again.
      const focused = tabs.contains(document.activeElement);
      tabs.innerHTML = sections()
        .map((title) => {
          const on = !searching && title === section;
          const count = offered.filter((o) => o.layout && (o.group ?? "Other") === title).length;
          return `<button type="button" role="tab" id="starting-tab-${slug(title)}" data-section="${html(title)}" aria-selected="${on}" aria-controls="starting-panel" tabindex="${on || (searching && title === sections()[0]) ? 0 : -1}">${html(title)}<span class="count" aria-hidden="true">${count}</span></button>`;
        })
        .join("");
      if (focused) tabs.querySelector<HTMLElement>('[aria-selected="true"]')?.focus();
    }

    /** The tiles shown, each its picture where it is drawn, the one the state takes outlined. */
    function drawTiles() {
      const these = shown();
      const words = find.value.trim();
      // The panel is named by its tab, or, while searching, by what is found.
      if (words) {
        box.removeAttribute("aria-labelledby");
        box.setAttribute("aria-label", `Layouts found for ${words}`);
      } else {
        box.removeAttribute("aria-label");
        box.setAttribute("aria-labelledby", `starting-tab-${slug(section ?? "")}`);
      }
      tiles.clear();
      if (!these.length) {
        box.innerHTML = `<p class="note">${words ? `No layout matches “${html(words)}”.` : "No layouts here."}</p>`;
        return;
      }
      box.innerHTML = `<div class="group">${these
        .map((one) => {
          const name = named(one);
          const on = one.layout === current;
          return `<button type="button" data-start="${html(one.layout!)}" title="${html(one.description ?? name)}"${on ? ' aria-current="true"' : ""}><canvas aria-hidden="true" width="${Math.round((height * 16) / 9)}" height="${height}"></canvas><span>${html(name)}</span></button>`;
        })
        .join("")}</div>`;
      box.querySelectorAll<HTMLButtonElement>("button[data-start]").forEach((button) => {
        tiles.set(button.dataset.start!, button);
        paint(button);
      });
    }

    /** `button`'s picture, where it is drawn. */
    function paint(button: HTMLButtonElement) {
      const one = kept.drawn.get(button.dataset.start!);
      const canvas = button.querySelector("canvas");
      if (!one || !canvas || !one.width) return;
      canvas.width = one.width;
      canvas.height = one.height;
      canvas.getContext("2d")?.putImageData(new ImageData(new Uint8ClampedArray(one.pixels), one.width, one.height), 0, 0);
      button.classList.add("drawn");
    }

    function show(title: string) {
      section = title;
      if (start) shownLast = title;
      find.value = "";
      drawTabs();
      drawTiles();
      void draw();
    }

    /** The layouts shown drawn, a few at a time, then the rest, kept: until the picker closes, or
     * opens again, or what it shows changes, which begins another round. */
    let drawing = 0;
    async function draw() {
      const turn = ++drawing;
      const theme = await stage.themeText().catch(() => undefined);
      const key = `${theme?.text ?? ""}\u001f${p.format ?? ""}\u001f${height}`;
      if (kept.key !== key) kept = { key, drawn: new Map() };
      for (const button of tiles.values()) paint(button);
      const first = shown().map((o) => o.layout!);
      const rest = offered.map((o) => o.layout).filter((l): l is string => !!l && !first.includes(l));
      const todo = [...first, ...rest].filter((l) => !kept.drawn.has(l));
      for (let at = 0; at < todo.length; at += BATCH) {
        if (turn !== drawing || round !== opened || !dialog.open) return;
        const batch = todo.slice(at, at + BATCH);
        let got: Starter[];
        try {
          got = await stage.starters(p.source, p.state, height, p.format, batch);
        } catch (e) {
          box.insertAdjacentHTML("afterbegin", `<p class="note">not drawn: ${html(said(e))}</p>`);
          return;
        }
        if (kept.key !== key) return;
        for (const one of got) {
          if (!one.layout) continue;
          kept.drawn.set(one.layout, one);
          const button = tiles.get(one.layout);
          if (button) paint(button);
        }
      }
    }

    return new Promise((resolve) => {
      let chosen: string | null | undefined;
      const done = () => {
        tabs.onclick = tabs.onkeydown = box.onclick = box.onkeydown = find.oninput = find.onkeydown = blank.onclick = null;
        resolve(chosen);
      };
      dialog.addEventListener("close", done, { once: true });
      tabs.onclick = (e) => {
        const tab = (e.target as Element).closest<HTMLElement>("[data-section]");
        if (tab?.dataset.section) show(tab.dataset.section);
      };
      // Up and down go from tab to tab, Home and End to the first and the last.
      tabs.onkeydown = (e) => {
        const all = sections();
        const at = all.indexOf(section ?? "");
        const to = e.key === "ArrowDown" ? at + 1 : e.key === "ArrowUp" ? at - 1 : e.key === "Home" ? 0 : e.key === "End" ? all.length - 1 : undefined;
        if (to === undefined) return;
        e.preventDefault();
        const next = all[Math.max(0, Math.min(all.length - 1, to))];
        if (next === undefined) return;
        show(next);
        tabs.querySelector<HTMLElement>(`[data-section="${CSS.escape(next)}"]`)?.focus();
      };
      box.onclick = (e) => {
        const tile = (e.target as Element).closest<HTMLElement>("[data-start]");
        if (!tile?.dataset.start) return;
        chosen = tile.dataset.start;
        dialog.close();
      };
      // The arrows go among the layouts as they stand in the grid, a row at a time up and down, as
      // a presentation app's layout chooser and the icon picker do; up from the first row goes
      // back to the field.
      box.onkeydown = (e) => {
        const tiles = [...box.querySelectorAll<HTMLButtonElement>("button[data-start]")];
        const at = tiles.indexOf(document.activeElement as HTMLButtonElement);
        if (at < 0) return;
        const top = tiles[0]?.offsetTop;
        const across = Math.max(1, tiles.filter((t) => t.offsetTop === top).length);
        const step = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -across, ArrowDown: across, Home: -at, End: tiles.length - 1 - at }[e.key];
        if (step === undefined) return;
        e.preventDefault();
        if (e.key === "ArrowUp" && at < across) return find.focus();
        tiles[Math.min(Math.max(at + step, 0), tiles.length - 1)]?.focus();
      };
      blank.onclick = () => {
        chosen = null;
        dialog.close();
      };
      find.oninput = () => {
        drawTabs();
        drawTiles();
        void draw();
      };
      // Enter in the field chooses nothing: the form it is in would close the dialog.
      find.onkeydown = (e) => {
        if (e.key === "Enter") e.preventDefault();
      };
      if (!dialog.open) dialog.showModal();
      void stage
        .startersListed(p.source)
        .then((all) => {
          if (round !== opened || !dialog.open) return;
          const names = typeof p.purpose === "object" ? p.purpose.names : undefined;
          offered = names ? all.filter((o) => o.layout && names.includes(o.layout)) : all;
          const titles = sections();
          const mine = offered.find((o) => o.layout && o.layout === current)?.group ?? undefined;
          section = mine ?? (shownLast && titles.includes(shownLast) ? shownLast : titles[0]);
          drawTabs();
          drawTiles();
          void draw();
          // Typing finds a layout at once; Tab goes on to the sections and the layouts.
          find.focus();
        })
        .catch((e) => {
          box.innerHTML = `<p class="note">not listed: ${html(said(e))}</p>`;
        });
    });
  }

  return { pick };
}
