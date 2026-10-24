// The bundle's files (PLAN 2.59, SPEC §9.2): its images, fonts, and data in a tab beside the data,
// each with what in the deck names it and the nodes drawn from it in the states that show them
// so, as `scaena files` lists them (`scaena_core::files`).
//
// - A node a file draws is a button: it shows the first state that shows it so, the node selected.
// - An image is dragged from the list onto the canvas: over an image node, it takes that image's
//   place (`choose` of `src`); anywhere else, it is inserted there, as Insert inserts it; onto the
//   source, its path goes where it is dropped. Insert does the same from the keyboard, where the
//   canvas was last pressed.
// - A file nothing names says so, and Remove takes it out of the bundle: the deck draws and reads
//   the same. It is a step in the editor's one undo, ⌘Z anywhere and the panel's Undo: undone, the
//   file is put back by its path, whatever came after it, so a node an undo brings back finds the
//   file it draws; redone, it is taken out again. The next save takes it out where the bundle is
//   kept.
import { listen, observing } from "./life";
import { BUNDLE_PATH, type BundleFile, type Edited, type Grounded, type Lexicon, type Named, PICTURE } from "./protocol";
import type { Stage } from "./stage";
import { named, object, sourced } from "./words";

/** What the panel asks of the editor around it. */
export interface FilesEditor {
  /** The state shown, by its id and its slot's index; none while the source does not compile. */
  shown(): { state: string; index: number } | undefined;
  format(): string | undefined;
  source(): string;
  /** Take an edit of the bundle's files: the source as it stands, shown and linted as `edited`
   * says, and a change to save. */
  took(edited: Edited): void;
  /** Take `path` taken out as such an edit, and a step in the editor's undo, which puts it back. */
  removed(path: string, edited: Edited): void;
  /** The editor's own undo and redo. */
  undo(): void;
  redo(): void;
  /** Show `state`, `node` selected on the canvas. */
  show(state: string, node: string): Promise<void>;
  /** Insert image `path` where the canvas was last pressed, as Insert does. */
  insert(path: string): Promise<void>;
  say(text: string): void;
  /** The words for the state shown (PLAN 2.99): its objects by what they show, the theme's names
   * said. */
  words?(): Lexicon | undefined;
  /** `state` as a person counts it: Slide 3, or Slide 3, step 2. */
  slide?(state: string): string;
}

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
const html = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

/** A size in bytes, as a person reads it. */
export const size = (bytes: number) =>
  bytes < 1024 ? `${bytes} B` : bytes < 1024 * 1024 ? `${Math.round(bytes / 1024)} KB` : `${(bytes / (1024 * 1024)).toFixed(1)} MB`;

/** What uses a file, as a person says it (PLAN 2.99): an image by what it shows, the story's
 * evidence, a font, a data source by its file's name in words (PLAN 3.18). */
export function naming(n: Named, words?: Lexicon): string {
  switch (n.by) {
    case "node":
      return object(words, n.node, true);
    case "evidence":
      return "the story, as evidence";
    case "font":
      return `the deck's fonts (${n.family}${n.style ? ` ${n.style}` : ""})`;
    case "theme":
      return `the theme's font ${named(words, "font-family", n.family)}`;
    case "source":
      return `the data “${sourced(words, n.source)}”`;
  }
}

/** A bundle file's name, without the folder it is in: `assets/ridge.jpg` is ridge.jpg. */
const fileName = (path: string) => path.replace(/^.*\//, "");

const HEADINGS: [BundleFile["type"], string][] = [
  ["image", "Images"],
  ["font", "Fonts"],
  ["data", "Data"],
];

/** The panel in `into`, over `stage`. */
export function filesPanel(stage: Stage, into: HTMLElement, editor: FilesEditor) {
  const head = into.querySelector<HTMLElement>("[data-summary]")!;
  const list = into.querySelector<HTMLElement>("[data-files]")!;
  /** The files as the worker last listed them. */
  let listed: BundleFile[] = [];
  /** What the deck was grounded in, as the worker last listed it (narrative FUNC-3). */
  let grounded: Grounded[] = [];
  /** Listings asked for: the last answer wins. */
  let asking = 0;
  /** One change at a time, each made on the bundle the one before left. */
  let making: Promise<unknown> = Promise.resolve();

  /** List the bundle's files again, and show them, while the panel is shown. */
  async function refresh() {
    if (into.hidden) return;
    const asked = ++asking;
    try {
      const got = await stage.listing(editor.source());
      if (asked !== asking) return;
      ({ files: listed, grounded } = got);
      draw();
    } catch (e) {
      if (asked !== asking) return;
      listed = [];
      grounded = [];
      head.textContent = `The files show once the source has no error to fix: ${said(e)}`;
      list.replaceChildren();
    }
  }

  function draw() {
    const loose = listed.filter((f) => !f.named.length).length;
    const count = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
    const of = (type: BundleFile["type"]) => listed.filter((f) => f.type === type).length;
    head.textContent = [
      count(of("image"), "image", "images"),
      count(of("font"), "font", "fonts"),
      count(of("data"), "data file", "data files"),
      loose ? `${count(loose, "file", "files")} nothing uses` : "",
    ]
      .filter(Boolean)
      .join(" · ");
    // A button focused in the list is focused again as the list is drawn anew, its row's by its
    // place there, so a key pressed as the worker answers goes where it was aimed.
    const focused = document.activeElement;
    const kept = focused instanceof HTMLButtonElement && list.contains(focused) ? focused.closest<HTMLElement>("[data-path]") : null;
    const at = kept && ([kept.dataset.path, [...kept.querySelectorAll("button")].indexOf(focused as HTMLButtonElement)] as const);
    list.innerHTML = HEADINGS.filter(([type]) => of(type) > 0)
      .map(([type, heading]) => {
        const rows = listed.filter((f) => f.type === type).map(row);
        return `<h3>${heading}</h3><ul aria-label="${heading}">${rows.join("")}</ul>`;
      })
      .join("");
    // What a deck made from a prompt was grounded in, which is no file of the bundle (FUNC-3).
    if (grounded.length) list.insertAdjacentHTML("beforeend", `<h3>Grounded in</h3><ul aria-label="Grounded in" data-grounded>${grounded.map(source).join("")}</ul>`);
    if (!at) return;
    const again = [...list.querySelectorAll<HTMLElement>("[data-path]")].find((r) => r.dataset.path === at[0]);
    again?.querySelectorAll("button")[at[1]]?.focus();
  }

  /** One source the deck was grounded in: its name, its kind and size, where it is, and the
   * slides whose beats cite it. */
  function source(g: Grounded): string {
    const slide = (state: string) => editor.slide?.(state) ?? state;
    const at = g.url && g.url !== g.name ? `<p class="at"><a href="${html(g.url)}" target="_blank" rel="noopener">${html(g.url)}</a></p>` : "";
    const cut = g.truncated ? ", cut short to fit" : "";
    const cited = g.states.length ? `<p class="named">cited by ${html([...new Set(g.states.map(slide))].join(", "))}</p>` : `<p class="unnamed">No slide cites it.</p>`;
    return `<li data-source="${html(g.name)}"><span class="name" title="${html(g.name)}">${html(g.name)}</span> <span class="size">${html(g.kind)}, ${size(g.bytes)}${cut}</span>${at}${cited}</li>`;
  }

  /** One file: its path and size, what names it, and the nodes drawn from it. */
  function row(f: BundleFile): string {
    const path = html(f.path);
    const drags = f.type === "image" && PICTURE.test(f.path);
    const words = editor.words?.();
    const slide = (state: string) => editor.slide?.(state) ?? state;
    const uses = f.named.length
      ? `<p class="named">used by ${html(f.named.map((n) => naming(n, words)).join(", "))}</p>`
      : `<p class="unnamed">Nothing in the deck uses it. <button type="button" data-remove aria-label="Remove ${html(fileName(f.path))}">Remove</button></p>`;
    const used = f.used
      .map((u) => {
        const [what, first] = [object(words, u.node), slide(u.states[0])];
        const slides = [...new Set(u.states.map(slide))].join(", ");
        return `<li><button type="button" data-node="${html(u.node)}" data-state="${html(u.states[0])}" title="Show ${html(first)}, ${html(what)} selected">${html(what)}</button> on ${html(slides)}</li>`;
      })
      .join("");
    const insert = drags ? ` <button type="button" data-insert aria-label="Insert ${html(fileName(f.path))}">Insert</button>` : "";
    const fetched = f.fetched ? `<p class="fetched">${html(f.fetched.said)}</p>` : "";
    return `<li data-path="${path}"${drags ? ' draggable="true"' : ""}><span class="name" title="${path}">${html(fileName(f.path))}</span> <span class="size">${size(f.bytes)}</span>${insert}${fetched}${uses}${used ? `<ul class="used">${used}</ul>` : ""}</li>`;
  }

  /** `path` taken out of the bundle: one change, which the editor's undo takes back. */
  function remove(path: string) {
    const run = async () => {
      try {
        const edited = await stage.removeFile(editor.source(), path, editor.shown()?.index ?? 0, editor.format());
        editor.removed(path, edited);
        await refresh();
        editor.say(`${fileName(path)} taken out of the deck · ⌘Z puts it back`);
      } catch (e) {
        editor.say(`Not taken out: ${said(e)}`);
      }
    };
    making = making.then(run, run);
    return making;
  }

  /** The editor's undo, or with `redo` its redo, of `path` taken out: put back (`out` false), or
   * taken out again. Asked of the worker at once, before any later change is, so the deck an undo
   * after it compiles finds the file. */
  function taken(path: string, out: boolean, redo: boolean) {
    const [source, index, format] = [editor.source(), editor.shown()?.index ?? 0, editor.format()];
    const asked = out ? stage.removeFile(source, path, index, format) : stage.putBack(source, path, index, format);
    const run = async () => {
      try {
        editor.took(await asked);
        await refresh();
        editor.say(`${redo ? "Made again" : "Undone"}: ${fileName(path)} ${out ? "taken out of the deck" : "put back"}`);
      } catch (e) {
        editor.say(`${redo ? "Not made again" : "Not undone"}: ${said(e)}`);
      }
    };
    making = making.then(run, run);
    return making;
  }

  listen(into, "click", (e) => {
    const target = e.target as Element;
    const at = target.closest<HTMLElement>("[data-path]")?.dataset.path;
    const button = target.closest<HTMLButtonElement>("button");
    if (!button) return;
    if (button.matches("[data-undo]")) return editor.undo();
    if (button.matches("[data-redo]")) return editor.redo();
    if (!at) return;
    if (button.matches("[data-remove]")) return void remove(at);
    if (button.matches("[data-insert]")) return void editor.insert(at);
    const node = button.dataset.node;
    const state = button.dataset.state;
    if (node && state) void editor.show(state, node);
  });
  // ⌘Z and ⇧⌘Z (Ctrl) in the panel are the editor's, as anywhere on the page.
  listen(into, "keydown", (e) => {
    if (!(e.metaKey || e.ctrlKey) || e.altKey || (e.key.toLowerCase() !== "z" && e.key.toLowerCase() !== "y")) return;
    if ((e.target as Element).closest("input, textarea, select")) return;
    e.preventDefault();
    if (e.key.toLowerCase() === "y" || e.shiftKey) editor.redo();
    else editor.undo();
  });
  // An image dragged out of the list carries its path: the canvas places it, and the source
  // takes its path, quoted, where it is dropped.
  listen(into, "dragstart", (e) => {
    const path = (e.target as Element).closest<HTMLElement>("[data-path][draggable]")?.dataset.path;
    if (!path || !e.dataTransfer) return;
    e.dataTransfer.setData(BUNDLE_PATH, path);
    e.dataTransfer.setData("text/plain", JSON.stringify(path));
    e.dataTransfer.effectAllowed = "copy";
  });
  observing(
    new MutationObserver(() => {
      if (!into.hidden) void refresh();
    }),
  ).observe(into, { attributes: true, attributeFilter: ["hidden"] });

  return {
    refresh,
    /** The files as the panel last listed them, for a test. */
    listed: () => listed,
    /** Take `path` out, as Remove does. */
    remove,
    /** Put `path` back, or take it out again, as the editor's undo or redo of its removal does. */
    taken,
    /** Once the panel's last change is made. */
    settled: () => making,
  };
}
