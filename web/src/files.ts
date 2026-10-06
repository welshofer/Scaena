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
//   the same. Undo puts it back and Redo takes it out again, as the Data panel's undo does a data
//   file's edits; the next save takes it out where the bundle is kept.
import { BUNDLE_PATH, type BundleFile, type Edited, type Named, PICTURE } from "./protocol";
import type { Stage } from "./stage";

/** What the panel asks of the editor around it. */
export interface FilesEditor {
  /** The state shown, by its id and its slot's index; none while the source does not compile. */
  shown(): { state: string; index: number } | undefined;
  format(): string | undefined;
  source(): string;
  /** Take an edit of the bundle's files: the source as it stands, shown and linted as `edited`
   * says, and a change to save. */
  took(edited: Edited): void;
  /** Show `state`, `node` selected on the canvas. */
  show(state: string, node: string): Promise<void>;
  /** Insert image `path` where the canvas was last pressed, as Insert does. */
  insert(path: string): Promise<void>;
  say(text: string): void;
}

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
const html = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

/** A size in bytes, as a person reads it. */
export const size = (bytes: number) =>
  bytes < 1024 ? `${bytes} B` : bytes < 1024 * 1024 ? `${Math.round(bytes / 1024)} KB` : `${(bytes / (1024 * 1024)).toFixed(1)} MB`;

/** What names a file, as `scaena files` says it. */
export function naming(n: Named): string {
  switch (n.by) {
    case "node":
      return `image ${n.node}`;
    case "evidence":
      return `beat ${n.beat}'s evidence`;
    case "font":
      return `the deck's fonts (${n.family}${n.style ? ` ${n.style}` : ""})`;
    case "theme":
      return `the theme's ${n.family} family`;
    case "source":
      return `data source ${n.source}`;
  }
}

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
  /** Listings asked for: the last answer wins. */
  let asking = 0;
  /** One change at a time, each made on the bundle the one before left. */
  let making: Promise<unknown> = Promise.resolve();

  /** List the bundle's files again, and show them, while the panel is shown. */
  async function refresh() {
    if (into.hidden) return;
    const asked = ++asking;
    try {
      const got = await stage.bundleFiles(editor.source());
      if (asked !== asking) return;
      listed = got;
      draw();
    } catch (e) {
      if (asked !== asking) return;
      listed = [];
      head.textContent = `the files wait for a source that compiles: ${said(e)}`;
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
      loose ? `${count(loose, "file", "files")} nothing names` : "",
    ]
      .filter(Boolean)
      .join(" · ");
    list.innerHTML = HEADINGS.filter(([type]) => of(type) > 0)
      .map(([type, heading]) => {
        const rows = listed.filter((f) => f.type === type).map(row);
        return `<h3>${heading}</h3><ul aria-label="${heading}">${rows.join("")}</ul>`;
      })
      .join("");
  }

  /** One file: its path and size, what names it, and the nodes drawn from it. */
  function row(f: BundleFile): string {
    const path = html(f.path);
    const drags = f.type === "image" && PICTURE.test(f.path);
    const named = f.named.length
      ? `<p class="named">named by ${html(f.named.map(naming).join(", "))}</p>`
      : `<p class="unnamed">Nothing names it. <button type="button" data-remove aria-label="Remove ${path}">Remove</button></p>`;
    const used = f.used
      .map(
        (u) =>
          `<li><button type="button" data-node="${html(u.node)}" data-state="${html(u.states[0])}" title="Show ${html(u.states[0])}, ${html(u.node)} selected">${html(u.node)}</button> in ${html(u.states.join(", "))}</li>`,
      )
      .join("");
    const insert = drags ? ` <button type="button" data-insert aria-label="Insert ${path}">Insert</button>` : "";
    return `<li data-path="${path}"${drags ? ' draggable="true"' : ""}><span class="name">${path}</span> <span class="size">${size(f.bytes)}</span>${insert}${named}${used ? `<ul class="used">${used}</ul>` : ""}</li>`;
  }

  /** `path` taken out of the bundle: one change, which Undo takes back. */
  function remove(path: string) {
    const run = async () => {
      try {
        const edited = await stage.removeFile(editor.source(), path, editor.shown()?.index ?? 0, editor.format());
        editor.took(edited);
        await refresh();
        editor.say(`${path} taken out of the bundle · Undo puts it back`);
      } catch (e) {
        editor.say(`not taken out: ${said(e)}`);
      }
    };
    making = making.then(run, run);
    return making;
  }

  /** The last change to the bundle's files undone, or with `redo` made again. */
  function undo(redo = false) {
    const run = async () => {
      try {
        const { name, edited } = await stage.dataUndo(editor.source(), redo, editor.shown()?.index ?? 0, editor.format());
        if (name === undefined || !edited) return editor.say(redo ? "no change to the files to redo" : "no change to the files to undo");
        editor.took(edited);
        await refresh();
        editor.say(redo ? `made again: the last change to ${name}` : `undone: the last change to ${name}`);
      } catch (e) {
        editor.say(`${redo ? "not made again" : "not undone"}: ${said(e)}`);
      }
    };
    making = making.then(run, run);
    return making;
  }

  into.addEventListener("click", (e) => {
    const target = e.target as Element;
    const at = target.closest<HTMLElement>("[data-path]")?.dataset.path;
    const button = target.closest<HTMLButtonElement>("button");
    if (!button) return;
    if (button.matches("[data-undo]")) return void undo(false);
    if (button.matches("[data-redo]")) return void undo(true);
    if (!at) return;
    if (button.matches("[data-remove]")) return void remove(at);
    if (button.matches("[data-insert]")) return void editor.insert(at);
    const node = button.dataset.node;
    const state = button.dataset.state;
    if (node && state) void editor.show(state, node);
  });
  // ⌘Z and ⇧⌘Z (Ctrl) in the panel undo and redo the last change to the files.
  into.addEventListener("keydown", (e) => {
    if (!(e.metaKey || e.ctrlKey) || e.altKey || (e.key.toLowerCase() !== "z" && e.key.toLowerCase() !== "y")) return;
    if ((e.target as Element).closest("input, textarea, select")) return;
    e.preventDefault();
    void undo(e.key.toLowerCase() === "y" || e.shiftKey);
  });
  // An image dragged out of the list carries its path: the canvas places it, and the source
  // takes its path, quoted, where it is dropped.
  into.addEventListener("dragstart", (e) => {
    const path = (e.target as Element).closest<HTMLElement>("[data-path][draggable]")?.dataset.path;
    if (!path || !e.dataTransfer) return;
    e.dataTransfer.setData(BUNDLE_PATH, path);
    e.dataTransfer.setData("text/plain", JSON.stringify(path));
    e.dataTransfer.effectAllowed = "copy";
  });
  new MutationObserver(() => {
    if (!into.hidden) void refresh();
  }).observe(into, { attributes: true, attributeFilter: ["hidden"] });

  return {
    refresh,
    /** The files as the panel last listed them, for a test. */
    listed: () => listed,
    /** Take `path` out, as Remove does; and undo or redo the last change to the files. */
    remove,
    undo,
    /** Once the panel's last change is made. */
    settled: () => making,
  };
}
