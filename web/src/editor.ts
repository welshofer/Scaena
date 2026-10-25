// The source editor (PLAN 2.3–2.4, SPEC §9.2): the deck as `.scn` (SPEC §4) in CodeMirror.
// The engine compiles it as it is typed and lints what compiles; findings sit in the gutter
// and under the source, each placed where the source wrote what it is about, and a fix is one
// click. The preview shows the state the cursor is in, at rest, and the inspector its nodes
// resolved, each text node's look, what overrides set, and its cue.
//
// The bundle opens from a URL, a folder on disk, a `.scaena` file, or the browser's own
// storage, and saves where it is kept, or into the browser's storage; a download is a
// `.scaena` zip with its fonts subset. A file dropped on the source joins the bundle, and its
// path goes where it was dropped (PLAN 2.4). New starts a deck from a theme that ships, as
// `deck_create` makes one, kept nowhere until it is saved; Save as saves the bundle somewhere
// new, a folder on disk or the browser's storage under another name, and keeps it there
// (PLAN 2.12).
//
// The assistant (PLAN 2.6) works on the deck with the user's own key: the source is read-only
// while it works, and each edit it makes comes into the source as it is made.
//
// The preview is a canvas too (PLAN 2.31, ADR-0013, `canvas.ts`): a click selects a node, a drag
// or an arrow key moves it and a handle resizes it, each a `place` patch by the user that comes
// into the source as one change, and one step to undo.
//
// `?bundle=` is a bundle's directory or its deck file (by default the bundle the build names,
// the site's demo deck (PLAN 2.7), or else the revenue example), `opfs:NAME` for one the
// browser keeps, or `folder:NAME` for a folder opened before; `?painter=` chooses who paints,
// as in the player. Play opens the player on the bundle as it was last saved.
//
// On a page `scaena serve` serves (`?serve`, PLAN 2.11), the bundle is a folder on disk, which a
// save writes back to, in any browser. Where the folder keeps the deck's source as `deck.scn`,
// that is the source the editor shows and saves. A change on disk, from a text editor or
// another page, comes into the editor when it has no changes of its own not saved; over such
// changes, the editor offers to take it.
import {
  defaultKeymap,
  history,
  historyKeymap,
  indentWithTab,
  invertedEffects,
  isolateHistory,
  redo,
  redoDepth,
  undo,
  undoDepth,
} from "@codemirror/commands";
import { syntaxHighlighting } from "@codemirror/language";
import { type Diagnostic, forceLinting, lintGutter, lintKeymap, linter, nextDiagnostic, openLintPanel, previousDiagnostic, setDiagnostics } from "@codemirror/lint";
import { Compartment, EditorState, MapMode, StateEffect, Transaction } from "@codemirror/state";
import {
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from "@codemirror/view";
import { themes } from "@scaena/themes";
import { familySections } from "./theme-families";
import { panel } from "./assistant/panel";
import { TOOLS, writer } from "./writing";
import { load } from "./assistant/keys";
import { mapper } from "./maps";
import { picturer } from "./picturing";
import { sourceOf } from "./bundle";
import { canvas, canvasKeys, placed } from "./canvas";
import { markName, noteName } from "./notes";
import { slideAsker } from "./slide-ask";
import { type Command, type Group, type Key, keyOf, MOD, MOD_ALT, menu, opens, palette, SHIFT, sheet } from "./commands";
import { cue, cueKeys } from "./cue";
import { dataKeys, sheets } from "./data";
import { filesPanel } from "./files";
import { storyPanel } from "./story";
import { finder, findKeys } from "./find";
import { deleteKept, keptBundle, keptNames, persist, persisted, renameKept } from "./folders";
import { discard, find, keep, savedAt, type Copy } from "./recovery";
import { formatsRow } from "./formats";
import { layerKeys, layers } from "./layers";
import { ended, listen as heed, observing } from "./life";
import { looks } from "./look";
import { layoutPicker } from "./picker";
import { iconPicker } from "./icons";
import type {
  Arrange,
  DataMark,
  Edited,
  Export,
  Finding,
  FromWorker,
  Inspected,
  Lexicon,
  Linted,
  Painter,
  ProviderId,
  Rewritten,
  SaveTo,
  Seeing,
  Source,
  Where,
  WritingKind,
} from "./protocol";
import { fitting } from "./pictures";
import { PICTURE } from "./protocol";
import { counted } from "./marks";
import { counted as many, findings as why, inserting, moving, named, numbered, object, seconds, sentence, sourced } from "./words";
import { prompting } from "./prompt";
import { rehearsal, rehearsalKeys } from "./rehearse";
import { scn, scnHighlight } from "./scn";
import { client, listen, served, serving, status as onDisk, token } from "./served";
import { worker } from "./spawn";
import { EngineStopped, PROBE, Stage } from "./stage";
import { toolbar } from "./toolbar";
import { slides as lightTable, slidesKeys } from "./slides";
import { strip, stripKeys } from "./strip";
import { type Selected, typingKeys } from "./typing";
import { themePanel } from "./theme-panel";
import { versionsPanel } from "./versions";
import { offline } from "./offline";

const params = new URLSearchParams(location.search);
const painter = (params.get("painter") ?? "auto") as Painter;
/** The bundle the editor opens when the address names none. */
const fallback = import.meta.env.VITE_BUNDLE ?? "../../docs/examples/revenue.deck.json";

const $ = <T extends HTMLElement>(selector: string) => document.querySelector<T>(selector)!;
const said = (e: unknown) => (e instanceof Error ? e.message : String(e));

// What waited on the engine when it stopped is refused, and the restart says so: a request fired
// and forgotten has nothing more to say. Heard once, for every deck the page opens.
addEventListener("unhandledrejection", (e) => {
  if (e.reason instanceof EngineStopped) e.preventDefault();
});

/** A theme file's name as a person reads it (PLAN 2.99): `themes/dusk.theme.json` is Dusk. */
const themeName = (path: string) => sentence(path.replace(/^.*\//, "").replace(/\.theme\.json$|\.json$/, ""));
/** What an edit did to what lint finds, said: 2 new issues, 1 gone; or no new issues. */
const issuesChanged = (worse: number, better: number) => `${worse ? many(worse, "new issue") : "no new issues"}${better ? `, ${better} gone` : ""}`;
/** `rows` of a source, by index, as a sentence says them, counted from 1 as a person counts
 * them: `row 3`, or `rows 1 and 3`. */
const rowsSaid = (indices: number[]) => {
  const rows = indices.map((r) => r + 1);
  return rows.length === 1 ? `row ${rows[0]}` : `rows ${rows.slice(0, -1).join(", ")} and ${rows[rows.length - 1]}`;
};
const html = (text: string) => text.replace(/[&<>"]/g, (c) => `&${{ "&": "amp", "<": "lt", ">": "gt", '"': "quot" }[c]};`);

/** The source's keys for its findings: CodeMirror's, and Shift-F8 back to the one before. */
const findingKeys = [...lintKeymap, { key: "Shift-F8", run: previousDiagnostic }];
/** What the source answers beyond typing (PLAN 2.65), as the keys sheet lists it: its findings'
 * keys as the keymap binds them, then Tab and the way out. */
const sourceKeys = (): Key[] => {
  const does = new Map<unknown, string>([
    [nextDiagnostic, "In the source: go to the next finding"],
    [previousDiagnostic, "Go to the finding before"],
    [openLintPanel, "List the findings"],
  ]);
  return [
    ...[...does].flatMap(([run, label]): Key[] => {
      const bound = findingKeys.find((b) => b.run === run)?.key;
      return bound ? [{ keys: keyOf(bound), label, group: "The source" }] : [];
    }),
    { keys: "Tab", label: "Indent", group: "The source" },
    { keys: "Escape, then Tab", label: "Leave the source", group: "The source" },
  ];
};
/** What the editor answers anywhere beyond its commands' keys (PLAN 2.65). */
const editorKeys = (): Key[] => [{ keys: `${MOD}K`, label: "Commands by name, and words for the assistant", group: "The editor" }];
/** The keys held with C or F that put the focus on the slide or in the strip, as Google Slides'
 * do (PLAN 2.126): ⌥⇧⌘ on a Mac, Ctrl+Alt+Shift elsewhere. */
const GO = MOD === "⌘" ? "⌥⇧⌘" : "Ctrl+Alt+Shift+";
/** The draw tools (PLAN 2.48, 2.114): each its key on the slide, what Insert calls it (PLAN 2.123),
 * and what the palette calls drawing with it. */
const DRAWING = [
  { key: "t", tool: "Text", label: "Draw a text" },
  { key: "r", tool: "Rectangle", label: "Draw a rectangle" },
  { key: "o", tool: "Oval", label: "Draw an oval" },
  { key: "l", tool: "Line", label: "Draw a line" },
  { key: "a", tool: "Arrow", label: "Draw an arrow" },
  { key: "p", tool: "Pen", label: "Draw with the pen" },
];
/** Whether `at` takes what is typed: a field, or the source. */
const typingIn = (at: EventTarget | null) =>
  at instanceof Element && Boolean(at.closest("input, textarea, select, [contenteditable]:not([contenteditable='false']), .cm-editor"));

/** The one change that turns `from` into `to`: what lies between their common start and
 * their common end. A fix rewrites the deck as canonical source, so most of it is unchanged. */
function change(from: string, to: string): { from: number; to: number; insert: string } {
  let start = 0;
  while (start < from.length && start < to.length && from[start] === to[start]) start++;
  let end = 0;
  while (end < from.length - start && end < to.length - start && from[from.length - 1 - end] === to[to.length - 1 - end]) end++;
  return { from: start, to: from.length - end, insert: to.slice(start, to.length - end) };
}

/** Where the state shown at `index` of `before` stands among `after`, the states an edit left: the
 * same state, by its id; else, where states were taken out, the nearest before it that stays (a
 * slide whose step an undo took out); else, a state renamed, the same place. */
function reshown(before: [string, number][], after: [string, number][], index: number): number {
  const at = (id: string | undefined) => after.findIndex(([state]) => state === id);
  if (at(before[index]?.[0]) >= 0) return at(before[index][0]);
  if (after.length < before.length) {
    for (let i = Math.min(index, before.length) - 1; i >= 0; i--) if (at(before[i][0]) >= 0) return at(before[i][0]);
  }
  return Math.max(0, Math.min(index, after.length - 1));
}

/** `source` with each path a save renamed, quoted as the source quotes paths, renamed. */
function renamedIn(source: string, renamed: [string, string][]): string {
  return renamed.reduce((text, [from, to]) => text.split(JSON.stringify(from)).join(JSON.stringify(to)), source);
}

/** Folders the editor was given, kept by name across reloads: a File System Access handle
 * keeps in IndexedDB, and asks again for leave to write. */
function folders<T>(mode: IDBTransactionMode, act: (store: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    const db = indexedDB.open("scaena", 1);
    db.onupgradeneeded = () => db.result.createObjectStore("folders");
    db.onerror = () => reject(db.error);
    db.onsuccess = () => {
      const request = act(db.result.transaction("folders", mode).objectStore("folders"));
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    };
  });
}

/** The dialog that makes a deck from a prompt, once wired: whether one is being made. */
let generating: { running: () => boolean } | undefined;

/** What the editor has open, to close when it opens another bundle; `say` puts a line in its
 * status that stays, before what lint finds, until the source is edited by hand. `kept` is the
 * name the browser keeps it under, if it does, and `rekeep` renames a bundle the browser keeps, or
 * deletes it, through the bundle's worker, which writes where the page may not (PLAN 2.124).
 * `unsaved` says whether closing it loses something: an edit not saved, or a deck kept nowhere. */
let current:
  | {
      close: () => void;
      unsaved: () => boolean;
      say: (text: string) => void;
      /** The deck's story read: its score (narrative USE-2). */
      story: () => Promise<number>;
      models: (provider: ProviderId, key: string, base?: string) => Promise<string[]>;
      kept: () => string | undefined;
      rekeep: (from: string, to?: string) => Promise<void>;
      typeTitle: () => Promise<boolean>;
    }
  | undefined;

/** A bundle as a person names it when it does not open: its file, its folder, or its name. */
function namedFor(source: Source): string {
  if ("zip" in source) return `${source.name}.scaena`;
  if ("opfs" in source) return source.opfs;
  if ("folder" in source) return source.folder.name;
  if ("create" in source) return `“${source.create.title}”`;
  if ("files" in source || "recovered" in source) return source.name;
  return decodeURIComponent(new URL(source.url, location.href).pathname.replace(/\/$/, "").replace(/^.*\//, ""));
}

/** Open the bundle at `source`, closing what is open once it opens; `asked` where changes not saved
 * were asked about already. One that does not open leaves the bundle open as it was, in use, and
 * the status says why, by the file's name. */
async function open(source: Source, asked = false) {
  if (!asked && current?.unsaved() && !confirm("The deck open is not saved. Open another and lose what is not saved?")) return;
  $("#status").textContent = "Opening…";
  let stage: Stage;
  try {
    stage = await Stage.open($<HTMLCanvasElement>("#stage"), source, painter, worker);
  } catch (e) {
    const why = `${namedFor(source)} can't be opened: ${said(e)}`;
    if (current) current.say(why);
    else $("#status").textContent = why;
    return;
  }
  current?.close();
  current = undefined;
  await edit(source, stage);
}

/** The page's address names where the bundle is kept, so a reload opens it there. A served
 * bundle's address names it already. */
function address(where: Where | undefined) {
  if (!where || where.kind === "serve") return;
  const url = new URL(location.href);
  url.searchParams.set("bundle", `${where.kind}:${where.name}`);
  window.history.replaceState(null, "", url);
}

/** The bundles the browser keeps, in Kept in this browser (PLAN 2.4, 2.124), as a document
 * browser lists them: each opened by its name, renamed, or deleted, asked first. The deck open
 * here is marked, and is not deleted. Listed again after each save and each open, which can add
 * one. */
async function listKept() {
  const names = await keptNames().catch(() => []);
  const here = current?.kept();
  const keeping = $<HTMLDialogElement>("#keeping");
  const button = (text: string, label: string, act: () => unknown) => {
    const b = Object.assign(document.createElement("button"), { type: "button", textContent: text });
    b.setAttribute("aria-label", label);
    b.onclick = () => void act();
    return b;
  };
  const row = (name: string) => {
    const li = document.createElement("li");
    li.dataset.name = name;
    const opening = button(name, name === here ? `${name}, open here` : `Open ${name}`, () => {
      keeping.close();
      if (name !== here) void open({ opfs: name }).catch(failed);
    });
    opening.dataset.open = "";
    li.append(opening);
    if (name === here) li.append(Object.assign(document.createElement("span"), { className: "here", textContent: "Open here" }));
    const deleting = button("Delete…", `Delete ${name}`, () => unkeep(name));
    deleting.dataset.delete = "";
    if (name === here) {
      deleting.disabled = true;
      deleting.title = "This deck is open here: open another to delete it";
    }
    const renaming = button("Rename…", `Rename ${name}`, () => rename(li, name));
    renaming.dataset.rename = "";
    li.append(renaming, deleting);
    return li;
  };
  $("#keeping ul").replaceChildren(...names.map(row));
  $("#keeping [data-none]").hidden = names.length > 0;
  // Whether the browser keeps them for good, or may clear them when short of room (lift-up
  // 10-24b REL-4).
  $("#keeping [data-persisted]").textContent = (await persisted())
    ? "This browser keeps them until you delete them."
    : "This browser may clear them when it runs short of room: Download keeps a copy of your own.";
}

/** Say `text` in Kept in this browser, over the page, and in the status under it. */
const keptSaid = (text: string) => {
  $("#keeping [data-said]").textContent = text;
  $("#status").textContent = text;
};

/** Rename `from` to `to`, or with no `to` delete it, where the browser keeps it: through the open
 * bundle's worker, or here where no bundle is open. */
async function rekeep(from: string, to?: string) {
  if (current) return current.rekeep(from, to);
  await (to === undefined ? deleteKept(from) : renameKept(from, to));
}

/** The deck the browser keeps as `name`, renamed in its row: Enter or leaving the field renames
 * it, and Escape leaves it as it was. */
function rename(li: HTMLLIElement, name: string) {
  const field = Object.assign(document.createElement("input"), { value: name, autocomplete: "off", spellcheck: false });
  field.setAttribute("aria-label", `New name for ${name}`);
  li.querySelector("[data-open]")!.replaceWith(field);
  field.select();
  let done = false;
  const finish = async (take: boolean) => {
    if (done) return;
    done = true;
    const to = field.value.trim();
    if (take && to && to !== name) {
      try {
        await rekeep(name, to);
        keptSaid(`Renamed ${name} to ${to}`);
      } catch (e) {
        keptSaid(`Not renamed: ${said(e)}`);
      }
    }
    await listKept();
    $<HTMLButtonElement>(`#keeping li[data-name="${CSS.escape(take && to ? to : name)}"] [data-rename]`)?.focus();
  };
  field.onkeydown = (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      void finish(true);
    } else if (e.key === "Escape") {
      // The field's own: the list stays open.
      e.preventDefault();
      e.stopPropagation();
      void finish(false);
    }
  };
  field.onblur = () => void finish(true);
}

/** The deck the browser keeps as `name`, deleted, and every file of it, asked first. */
async function unkeep(name: string) {
  if (!confirm(`Delete “${name}” from this browser? Its files go with it, and it cannot be undone.`)) return;
  try {
    await rekeep(name);
    keptSaid(`Deleted ${name} from this browser`);
  } catch (e) {
    keptSaid(`Not deleted: ${said(e)}`);
  }
  await listKept();
  $<HTMLButtonElement>("#keeping li [data-open]")?.focus();
}

/** The bundle controls: open a folder, a `.scaena` file, or a bundle the browser keeps. */
async function controls() {
  const keeping = $<HTMLDialogElement>("#keeping");
  $<HTMLButtonElement>("#kept").onclick = async () => {
    $("#keeping [data-said]").textContent = "";
    await listKept();
    keeping.showModal();
    $<HTMLButtonElement>("#keeping li [data-open]")?.focus();
  };
  // Escape in a name being changed leaves the name as it was, and the list open.
  keeping.addEventListener("cancel", (e) => {
    if (document.activeElement instanceof HTMLInputElement && keeping.contains(document.activeElement)) e.preventDefault();
  });
  const folder = $<HTMLButtonElement>("#open-folder");
  folder.hidden = !window.showDirectoryPicker;
  folder.onclick = async () => {
    const handle = await window.showDirectoryPicker!({ id: "scaena", mode: "readwrite" }).catch(() => undefined);
    if (!handle) return;
    await folders("readwrite", (store) => store.put(handle, handle.name));
    await open({ folder: handle }).catch(failed);
  };
  const file = $<HTMLInputElement>("#open-file");
  // A button the keys reach, which opens the file chooser the input stands for (WCAG 2.1.1).
  $<HTMLButtonElement>("#open-scaena").onclick = () => file.click();
  file.onchange = async () => {
    const zip = file.files?.[0];
    file.value = "";
    if (zip) await open({ zip: await zip.arrayBuffer(), name: zip.name.replace(/\.scaena$/, "") }).catch(failed);
  };
  // New (PLAN 2.12): a title and a theme that ships.
  const making = $<HTMLDialogElement>("#making");
  const title = $<HTMLInputElement>("#making-name");
  const theme = $<HTMLSelectElement>("#making-theme");
  theme.replaceChildren(...familySections());
  // The theme chosen, as its title slide drawn at build (PLAN T.0c).
  const preview = $<HTMLImageElement>("#making-preview");
  const previewed = () => {
    const chosen = themes[theme.value];
    preview.hidden = !chosen;
    if (!chosen) return;
    preview.src = chosen.thumbnail;
    preview.alt = `A title slide in ${theme.value}`;
  };
  theme.onchange = previewed;
  previewed();
  $<HTMLButtonElement>("#new-deck").onclick = () => {
    making.returnValue = "";
    making.showModal();
    title.select();
  };
  making.onclose = () => {
    if (making.returnValue !== "create") return;
    // The deck opens typing in its title, its words selected, so the first keys replace them; else
    // the slide takes the keys, not New…, which a space would open again.
    const was = current;
    void open({ create: { theme: theme.value, title: title.value.trim() || "Untitled" } })
      .then(async () => current !== was && ((await current?.typeTitle()) || $<HTMLElement>("#overlay").focus()))
      .catch(failed);
  };
  // From a prompt (PLAN G.5): a deck Narrato writes, made in a worker of its own.
  generating = prompting(
    worker,
    $("#new-prompt"),
    () => current?.unsaved() ?? false,
    (source) => open(source, true),
    async () => {
      // The Story tab shown reads the deck's health itself; the score goes in the line.
      $<HTMLButtonElement>("#tab-story").click();
      try {
        return await current?.story();
      } catch {
        return undefined;
      }
    },
    (line) => current?.say(line),
    (provider, key, base) => (current ? current.models(provider, key, base) : Promise.reject(new Error("no deck is open"))),
  );
}

/** The bundle `?bundle=` names. A folder kept from before asks again for leave to write,
 * which takes a click. */
async function first(): Promise<Source> {
  const bundle = params.get("bundle");
  if (!bundle?.startsWith("folder:")) {
    const source = sourceOf(bundle, fallback);
    return served && "url" in source ? { ...source, serve: serving } : source;
  }
  const name = bundle.slice("folder:".length);
  const handle = await folders<FileSystemDirectoryHandle | undefined>("readonly", (store) => store.get(name));
  if (!handle) throw new Error(`no folder named ${name} was opened here: open it again`);
  if ((await handle.queryPermission({ mode: "readwrite" })) === "granted") return { folder: handle };
  const again = $<HTMLButtonElement>("#again");
  again.textContent = `Open the folder ${name} again`;
  again.hidden = false;
  $("#status").textContent = "";
  await new Promise<void>((resolve) => {
    again.onclick = async () => {
      if ((await handle.requestPermission({ mode: "readwrite" })) === "granted") resolve();
    };
  });
  again.hidden = true;
  return { folder: handle };
}

const failed = (e: unknown) => {
  $("#status").textContent = `Error: ${said(e)}`;
  console.error(e);
};

/** The files an edit wrote beside the deck, carried by its change to the source: the data files a
 * restore wrote (PLAN 2.60), or the theme a theme edit did (PLAN 2.61). The undo of that change
 * writes each back as it was, and its redo as the edit left it. */
const filesWritten = StateEffect.define<Rewritten[]>();
/** A change that carries files written is undone with each file's texts swapped. */
const rewrites = invertedEffects.of((tr) =>
  tr.effects.filter((e) => e.is(filesWritten)).map((e) => filesWritten.of(e.value.map(({ path, before, after }) => ({ path, before: after, after: before })))),
);
/** A file the Files panel took out (PLAN 2.59), carried by a change that leaves the source as it
 * is: its undo puts the file back, in the one history the source keeps, and its redo takes it out
 * again, so a node an undo brings back finds the file it draws. */
const fileTaken = StateEffect.define<{ path: string; out: boolean }>();
const takes = invertedEffects.of((tr) => tr.effects.filter((e) => e.is(fileTaken)).map((e) => fileTaken.of({ ...e.value, out: !e.value.out })));
/** What a change is, in a person's words, carried by its transaction (lift-up USE-4): an undo says
 * what it took back, and a redo what it made again. The inverse carries the same words. */
const described = StateEffect.define<string>();
const describes = invertedEffects.of((tr) => tr.effects.filter((e) => e.is(described)).map((e) => described.of(e.value)));
/** `words` as a change is described: without the status's "⌘Z undoes it", which an undo makes untrue. */
const describing = (words: string) => words.replace(/\s*·\s*⌘Z undoes it$/, "");

async function edit(source: Source, opened?: Stage) {
  const status = $("#status");
  // A status too long for its two lines says the rest in its title.
  observing(new MutationObserver(() => (status.title = status.textContent ?? ""))).observe(status, { childList: true, characterData: true, subtree: true });
  const statesPicker = $<HTMLSelectElement>("#state");
  const formatPicker = $<HTMLSelectElement>("#format");
  const problems = $<HTMLUListElement>("#problems");
  const inspector = $("#inspector");
  const listing = $("#listing");
  const whereLine = $("#where");
  // The toolbar's icons, its File menu, and its keys (lift-up ATTR-5).
  toolbar($("#toolbar"));
  const play = $<HTMLAnchorElement>("#play");
  const stage = opened ?? (await Stage.open($<HTMLCanvasElement>("#stage"), source, painter, worker));
  stage.onError = failed;
  /** On a served page, the deck's source on disk, where the folder keeps one (PLAN 2.11). */
  const scnOnDisk = "url" in source && source.serve && (await onDisk()).source ? new URL("deck.scn", source.url) : undefined;
  const fromDisk = async () => (await fetch(scnOnDisk!, { cache: "no-store", headers: { "X-Scaena-Token": token } })).text();
  /** The deck's own size, named by its shape where a format is it (`16:9`); a format it lists of
   * that shape is its own canvas, offered once. An edit of the canvas renames it. */
  let ownSize = stage.opened.own;
  const own = () => ownSize;
  const sizes = (formats: string[]) => formats.filter((f) => f !== own());
  formatPicker.replaceChildren(new Option(own() ?? "Own size", ""));
  for (const format of sizes(stage.opened.formats)) formatPicker.add(new Option(format, format));
  const format = () => formatPicker.value || undefined;
  /** The deck's formats and its own size as an edit leaves them: the format menu, and the formats
   * side by side, follow. A format the deck no longer lists, or one that is its own canvas now,
   * gives way to its own canvas, as the worker's does. */
  function reformats(all: string[], named: string | undefined) {
    ownSize = named;
    const formats = sizes(all);
    const listed = [...formatPicker.options].map((o) => [o.value, o.text]);
    const now = [["", own() ?? "Own size"], ...formats.map((f) => [f, f])];
    if (listed.length !== now.length || listed.some(([v, t], i) => v !== now[i][0] || t !== now[i][1])) {
      const chosen = formatPicker.value;
      formatPicker.replaceChildren(...now.map(([v, t]) => new Option(t, v)));
      formatPicker.value = formats.includes(chosen) ? chosen : "";
    }
    formatting.formats(formats, own());
  }
  /** The bundle's name, and where it is kept. */
  let { name, where } = stage.opened;
  /** The source as the bundle opened or the last save held it; and the edits of the bundle's files
   * beside it that the source does not show (a data file written, a file taken out), made and
   * saved. An undo back to what was saved leaves nothing to save. */
  let savedSource: string | undefined;
  let filed = 0;
  let filedSaved = 0;
  const dirty = () => filed !== filedSaved || (savedSource !== undefined && view.state.doc.toString() !== savedSource);
  /** Whether the deck is kept nowhere yet: made by New, from a prompt, or from files dropped. */
  const unkept = () => !where && ("create" in source || "files" in source);
  /** Whether closing the deck would lose something: an edit not saved, or a deck kept nowhere,
   * which opens with nothing edited and nothing of it kept (lift-up 10-24 REL-1). */
  const unsaved = () => dirty() || unkept();
  /** The folder on disk the bundle is kept in, where the page has its handle. */
  let folderHandle: FileSystemDirectoryHandle | undefined = "folder" in source ? source.folder : undefined;
  /** How many times the engine has been started again after a panic (lift-up STAB-2). */
  let restarts = 0;
  // The engine stopped on a panic, which wasm32 does not unwind from: the stage is started again
  // in a fresh worker, from the bundle's files as the old one held them and the source as it
  // stands here, kept where the bundle is kept, so Save still works; the state shown is shown
  // again, and the person is told in plain words.
  stage.onCrashed = (crash) => void restarted(crash);
  // A worker that died outside a message, as one out of memory does, hands nothing over (lift-up
  // 10-24b STAB-2): the stage is started again from the deck as last kept, or as it came, with the
  // source as it stands here, and the files written since the save, which went with it, are named.
  stage.onDied = (death) => void restarted(death);
  stage.onStalled = (message) => say(`The engine stopped answering after an error (${message}): waiting up to ${PROBE / 1000} seconds for it before starting it again…`);
  async function restarted(crash: { message: string; files?: Record<string, ArrayBuffer>; lost?: string[] }) {
    const why = crash.message ? ` (${crash.message})` : "";
    say(`The engine stopped on an error${why}: starting it again…`);
    const kept =
      where?.kind === "opfs"
        ? { opfs: where.name }
        : where?.kind === "folder" && folderHandle
          ? { folder: folderHandle }
          : where?.kind === "serve" && "url" in source && source.serve
            ? { url: source.url, serve: source.serve }
            : undefined;
    try {
      await stage.restart(crash.files ? { recovered: crash.files, name, kept } : (kept ?? source));
    } catch (e) {
      say(`The engine stopped on an error${why}, and could not be started again: ${said(e)}. Download the deck to keep what you changed.`);
      return;
    }
    restarts++;
    // What the old worker held of the page: the part of the canvas shown, the formats beside it,
    // the source it compiled, which showing the state shown and linting it again compiles in the
    // fresh one, the words, and the thumbnails.
    taken = undefined;
    lexicon = undefined;
    await board.restarted();
    formatting.restarted();
    const doc = view.state.doc;
    const found = await lint(view);
    if (view.state.doc === doc) view.dispatch(setDiagnostics(view.state, found));
    states.reformat();
    if (!crash.files) {
      const lost = crash.lost?.length ? ` These files were not saved, and are lost: ${crash.lost.join(", ")}.` : "";
      say(`The engine stopped on an error${why}, and was started again from the deck as last ${where ? "saved" : "opened"}, with the source as it stands.${lost} Save when you can.`);
      return;
    }
    say(`The engine stopped on an error${why}, and was started again from the deck as it stands: nothing is lost. Save when you can.`);
  }

  // The recovery copy (lift-up REL-4): what is not saved, the source and the files written since
  // the save, kept in this browser a few seconds after each change and when the page is hidden,
  // taken away by a save, and offered when the deck opens again (`recovery.ts`).
  /** What names the deck's copy: where it is kept, else where it came from. */
  const copyKey = () => {
    if (where) return `${where.kind}:${where.name}`;
    if ("url" in source) return `url:${new URL(source.url, location.href).href}`;
    if ("create" in source) return `new:${source.create.title}`;
    return `files:${name}`;
  };
  /** How long after a change the copy is written, ms. */
  const COPY_AFTER = 2000;
  let copyTimer: ReturnType<typeof setTimeout> | undefined;
  /** Each file the copy holds, with how many times it had been written when it was kept. */
  let copied = new Map<string, number>();
  /** One copy at a time. */
  let copying: Promise<void> = Promise.resolve();
  /** Keep the copy now: the source, and each file written since the save that the copy lacks; or,
   * with nothing to save, take it away. A browser that keeps nothing keeps no copy. */
  const keepCopy = () => {
    clearTimeout(copyTimer);
    copyTimer = undefined;
    const run = async () => {
      const key = copyKey();
      if (!unsaved()) {
        copied = new Map();
        return discard(key);
      }
      const { written, removed } = stage.unsaved();
      const files = new Map<string, Uint8Array>();
      for (const [path, n] of written) {
        if (copied.get(path) === n) continue;
        const bytes = await stage.file(path);
        if (bytes) files.set(path, new Uint8Array(bytes));
      }
      await keep(key, { at: Date.now(), name, source: view.state.doc.toString(), files: [...written.keys()], removed }, files);
      copied = new Map(written);
    };
    copying = copying.then(run, run).catch((e: unknown) => console.warn(`The recovery copy was not kept: ${said(e)}`));
    return copying;
  };
  /** Keep the copy once typing stops. */
  const scheduleCopy = () => {
    if (savedSource === undefined) return;
    clearTimeout(copyTimer);
    copyTimer = setTimeout(() => void keepCopy(), COPY_AFTER);
  };
  /** `at`, ms since the epoch, as the offer says it. */
  const when = (at: number) => new Date(at).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
  /** The Files panel's Undo and Redo are the editor's: with nothing to undo or redo, they wait. */
  const undoable = (state: EditorState) => {
    $<HTMLButtonElement>("#files [data-undo]").disabled = undoDepth(state) === 0;
    $<HTMLButtonElement>("#files [data-redo]").disabled = redoDepth(state) === 0;
  };
  /** The deck's title, as its source's `deck` line writes it. */
  const heading = () => {
    const header = /^deck\s+("(?:[^"\\\n]|\\.)*")/m.exec(view.state.doc.toString());
    try {
      return header ? (JSON.parse(header[1]) as string) : undefined;
    } catch {
      return undefined;
    }
  };
  /** The deck's title, once its source is read: what the tab names. */
  const titled = () => (savedSource === undefined ? undefined : heading());
  const tell = () => {
    const kinds = {
      folder: `In the folder ${where?.name}`,
      opfs: `Kept in this browser as ${where?.name}`,
      serve: `In the folder ${where?.name}, served${scnOnDisk ? ", from its deck.scn" : ""}`,
    };
    // A deck kept nowhere yet is named by its title, not the name it would be kept by.
    const kept = where ? kinds[where.kind] : `${titled() || name} · Not saved yet`;
    whereLine.textContent = `${kept}${dirty() ? " · edited" : ""}`;
    // The tab names the deck, as a window's title does, and says when it has changes not saved.
    document.title = `${titled() || name}${dirty() ? " — edited" : ""} · Scaena`;
    // The player opens a bundle by its address or from the browser's storage, not a folder.
    const playing =
      where?.kind === "serve"
        ? new URLSearchParams({ bundle: params.get("bundle") ?? "/bundle/", serve: token })
        : new URLSearchParams({ bundle: where ? `${where.kind}:${where.name}` : (params.get("bundle") ?? fallback) });
    if (params.has("painter")) playing.set("painter", painter);
    play.href = `index.html?${playing}`;
    play.hidden = where?.kind === "folder";
    play.title = unkept()
      ? "The player, on the deck kept in this browser first"
      : dirty()
        ? "The player, from the slide shown, once the deck is saved"
        : "The player, from the slide shown";
    // A served bundle saves to its folder alone.
    $<HTMLButtonElement>("#save-as").hidden = where?.kind === "serve";
    scheduleCopy();
  };
  address(where);
  tell();
  void listKept();

  /** What the last edit came to, and each edit's round trip for the page's own record. */
  let last: Edited | undefined;
  const trips: { ms: number; compile: number; paint: number; lint: number }[] = [];
  /** Each lint of every state, and how long it took in the worker. */
  const wholes: Linted[] = [];
  /** The state the preview and the inspector show, by index. */
  let shown = 0;
  /** The states `shown` counts among: the table as it stood when the state was shown. A change
   * drops from the table each state whose start it rewrote, until the source is compiled again,
   * though it may put those states back as they were (an undo of a delete at two places far
   * apart): the state shown is found again by its id here, not by its place in the table. */
  let among: [string, number][] | undefined;
  /** The states as the source last compiled them, which the picker lists: all of them, where a
   * change since may have taken some out of `last`. */
  let listed: [string, number][] = [];
  /** The cursor moved while a change had taken states out of the table, which cannot say which
   * state it is in: the preview follows it once the source is compiled again, unless a slide is
   * shown meanwhile. */
  let unfollowed = false;
  /** The states shown so far, counted: a compile asked for before the last answers with the state the
   * worker showed then, and the one shown since stays. */
  let showings = 0;
  /** The source's version: one more with each change. A lint answers the version it read. */
  let version = 0;
  stage.sourceVersion = () => version;
  /** A lint of every state, waiting for typing to stop. */
  let pending: ReturnType<typeof setTimeout> | undefined;
  const pause = 500;
  /** Whether the source takes typing: not while the assistant works. */
  const locked = new Compartment();
  let assisting = false;
  /** The source the assistant's last edit, or the canvas's, made, and what the worker found in
   * it: the lint of that source takes it as it is, rather than asking the worker again. With it,
   * how many states had been shown then (`showings`): one shown since is where the lint leaves
   * the canvas, since what the worker found names the state shown when the edit was made. */
  let taken: { source: string; edited: Edited; showings: number } | undefined;
  /** The sources the worker compiled lately, the latest first, each with where its states start:
   * an undo or a redo makes one of them the source again. */
  const compiled: { source: string; states: [string, number][] }[] = [];
  /** The state shown, inspected: each node's placement, which the canvas reads. */
  let inspected: Inspected | undefined;
  /** The node the canvas has selected, and those selected beside it (PLAN 2.42). */
  let chosen: string | undefined;
  let beside: string[] = [];
  /** The characters selected in the text typed in, if any are (PLAN 2.38). */
  let characters: Selected | undefined;
  /** What the assistant's question has changed so far, selected once the canvas stands in the
   * source its edit made (PLAN 2.52). */
  let touching: { source: string; nodes: string[] } | undefined;

  /** What the last change was, in a person's words, where it said (lift-up USE-4): what Undo
   * takes back; and what the last undo took back: what Redo makes again. */
  let lastDone: string | undefined;
  let lastUndone: string | undefined;
  const view = new EditorView({
    parent: $("#code"),
    state: EditorState.create({
      doc: scnOnDisk ? await fromDisk() : await stage.source(),
      extensions: [
        lineNumbers(),
        highlightActiveLineGutter(),
        history(),
        rewrites,
        takes,
        describes,
        drawSelection(),
        highlightActiveLine(),
        keymap.of([...defaultKeymap, ...historyKeymap, ...findingKeys, indentWithTab]),
        // Named for a screen reader, and in the tab order by its own attribute: Tab indents,
        // so Escape then Tab leaves it.
        EditorView.contentAttributes.of({ "aria-label": "The deck's source", tabindex: "0" }),
        scn,
        syntaxHighlighting(scnHighlight),
        lintGutter(),
        linter(lint, { delay: 150 }),
        locked.of([]),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            version++;
            clearTimeout(pending);
            // Where each state starts moves with the change until the source is compiled again:
            // the cursor an undo puts back is in the state it was in. The source the worker made
            // (a patch, the assistant's edit), which it holds, has its states where it says, those
            // the patch added or took away too. A source it compiled lately, which an undo or a
            // redo makes the source again, has its states where it found them, though the change
            // wrote a state's first line again, but for one the worker does not hold now.
            // Elsewhere a state whose start the change took away is gone, and one it brings back
            // waits for the worker to compile the source, so the cursor never asks the worker for
            // a state it does not hold.
            if (last) {
              const doc = update.state.doc;
              let text: string | undefined;
              const is = (source: string) => source.length === doc.length && source === (text ??= doc.toString());
              const held = new Set(last.states.map(([id]) => id));
              const made = taken?.edited.valid && is(taken.source) ? taken.edited.states : undefined;
              const again = made ? undefined : compiled.find((c) => is(c.source))?.states.filter(([id]) => held.has(id));
              const states =
                made ??
                again ??
                last.states.flatMap(([id, at]): [string, number][] => {
                  const to = update.changes.mapPos(at, 1, MapMode.TrackAfter);
                  return to === null ? [] : [[id, to]];
                });
              last = { ...last, states };
            }
            tell();
          }
          // An undo or a redo of a restore or a theme edit writes its files back, before the deck
          // is compiled again (PLAN 2.60, 2.61).
          const written = update.transactions.flatMap((tr) =>
            tr.isUserEvent("undo") || tr.isUserEvent("redo") ? tr.effects.filter((e) => e.is(filesWritten)).flatMap((e) => e.value) : [],
          );
          if (written.length) rewrite(written, update.docChanged ? undefined : update.state.doc.toString());
          // An undo or a redo of a file taken out puts it back, or takes it out again (PLAN 2.59).
          for (const tr of update.transactions) {
            if (!tr.isUserEvent("undo") && !tr.isUserEvent("redo")) continue;
            for (const e of tr.effects) if (e.is(fileTaken)) void filing.taken(e.value.path, e.value.out, tr.isUserEvent("redo"));
          }
          // An undo or a redo says what it took back or made again (lift-up USE-4), as the Data and
          // Files tabs say of theirs: a file taken out or put back is the Files tab's to say.
          for (const tr of update.transactions) {
            const words = tr.effects.find((e) => e.is(described))?.value;
            if (tr.isUserEvent("undo") || tr.isUserEvent("redo")) {
              if (tr.effects.some((e) => e.is(fileTaken))) continue;
              const redo = tr.isUserEvent("redo");
              const what = words ?? "an edit of the source";
              say(`${redo ? "Made again" : "Undone"}: ${what}`);
              // Said of the source the undo made: it stays through that source's lint.
              toldOf = update.state.doc.toString();
              lastUndone = redo ? undefined : what;
              lastDone = redo ? what : undefined;
            } else if (tr.docChanged || words !== undefined) {
              lastDone = words ?? (tr.isUserEvent("input.type") ? "what was typed" : "an edit of the source");
              lastUndone = undefined;
            }
          }
          if (update.selectionSet) follow();
          undoable(update.state);
        }),
        // A file dropped on the source joins the bundle; its path goes where it was dropped.
        EditorView.domEventHandlers({
          dragover: (e) => {
            if (!e.dataTransfer?.types.includes("Files")) return false;
            e.preventDefault();
            return true;
          },
          drop: (e, view) => {
            const files = [...(e.dataTransfer?.files ?? [])];
            if (!files.length) return false;
            e.preventDefault();
            void drop(files, view.posAtCoords({ x: e.clientX, y: e.clientY }) ?? view.state.selection.main.head);
            return true;
          },
        }),
        EditorView.theme(
          {
            "&": { height: "100%", fontSize: "13px", backgroundColor: "var(--surface)", color: "var(--ink)" },
            ".cm-scroller": { fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace", lineHeight: "1.5" },
            ".cm-gutters": { backgroundColor: "var(--panel)", color: "var(--muted)", border: "none" },
            ".cm-activeLine, .cm-activeLineGutter": { backgroundColor: "#2b2a26" },
            ".cm-cursor": { borderLeftColor: "var(--ink)" },
            "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": { backgroundColor: "#3b4a5e !important" },
          },
          { dark: true },
        ),
      ],
    }),
  });

  savedSource = view.state.doc.toString();
  tell();
  undoable(view.state);
  // The page hidden, a tab put behind or closed: the copy is kept now, not when typing stops.
  heed(document, "visibilitychange", () => {
    if (document.visibilityState === "hidden" && copyTimer) void keepCopy();
  });
  void offerCopy().catch(() => {});

  /** A copy of changes not saved, kept when this deck was open before (lift-up REL-4): offered,
   * unless the deck was saved after it, or it holds nothing the deck lacks. */
  async function offerCopy() {
    const key = copyKey();
    const copy = await find(key);
    if (!copy) return;
    const kept = where?.kind === "opfs" ? await keptBundle(where.name).catch(() => undefined) : folderHandle;
    const saved = await savedAt(kept);
    const same = copy.source === view.state.doc.toString() && !copy.files.length && !copy.removed.length;
    if ((saved !== undefined && saved > copy.at) || same) return discard(key);
    const dialog = $<HTMLDialogElement>("#recovering");
    dialog.querySelector("[data-said]")!.textContent = `Changes made to ${titled() || name} at ${when(copy.at)} were not saved. Recover them, or discard them?`;
    dialog.returnValue = "";
    dialog.onclose = () => {
      if (dialog.returnValue === "recover") void recover(copy).catch(failed);
      else if (dialog.returnValue === "discard") void discard(key);
    };
    dialog.showModal();
  }

  /** `copy` made the deck again: its files written into the bundle, then its source taken as one
   * change, one step to undo; the deck is then as it was, to save. */
  async function recover(copy: Copy & { file: (path: string) => Promise<Uint8Array | null> }) {
    say("Recovering…");
    const files: { path: string; text: string | null; bytes?: ArrayBuffer }[] = [];
    for (const path of copy.files) {
      const bytes = await copy.file(path);
      if (bytes) files.push({ path, text: "", bytes: bytes.buffer as ArrayBuffer });
    }
    for (const path of copy.removed) files.push({ path, text: null });
    if (files.length) {
      await stage.writeFiles(files);
      filed++;
    }
    copied = new Map();
    const changes = change(view.state.doc.toString(), copy.source);
    view.dispatch({ changes, userEvent: "input.recover", annotations: isolateHistory.of("full") });
    tell();
    say(`Recovered the changes from ${when(copy.at)}${files.length ? `, with ${many(files.length, "file")}` : ""}: save to keep them`);
  }

  // The canvas first (PLAN 2.80): the source is a pane that folds away. It opens as the address
  // asks (`?source=open` or `folded`), else as this browser last left it, else folded.
  const framed = $("main");
  const sourceToggle = $<HTMLButtonElement>("#source-toggle");
  const remembered = (() => {
    try {
      return localStorage.getItem("scaena.source");
    } catch {
      return null;
    }
  })();
  /** Open the source beside the canvas, or fold it away; `keep` remembers the choice. */
  const fold = (open: boolean, keep = true) => {
    framed.classList.toggle("folded", !open);
    sourceToggle.setAttribute("aria-pressed", String(open));
    if (open) {
      // It opens at the state the canvas shows: the cursor stays where it is in that state, else
      // goes to where the state starts, not where an edit made folded left it, at the end of a
      // long line. A scroll asked for folded was measured against no width; this one, the pane's.
      const head = view.state.selection.main.head;
      const start = last?.states[shown]?.[1];
      const end = last?.states[shown + 1]?.[1] ?? Infinity;
      const to = start === undefined || (start <= head && head < end) ? head : start;
      view.dispatch({ selection: to === head ? undefined : { anchor: to }, effects: EditorView.scrollIntoView(to, { y: "center" }) });
    }
    if (!keep) return;
    try {
      localStorage.setItem("scaena.source", open ? "open" : "folded");
    } catch {
      // A browser that keeps nothing folds it again next time.
    }
  };
  fold((new URLSearchParams(location.search).get("source") ?? remembered) === "open", false);
  const folded = () => framed.classList.contains("folded");
  sourceToggle.onclick = () => fold(folded());
  /** The inspector's column, or the animations under the canvas, hidden to give the slide their
   * room, as a presentation app's View menu hides them, or shown again. */
  const hiding = (what: "panel" | "cue", hide: boolean) => framed.classList.toggle(what === "panel" ? "bare" : "still", hide);
  const hidden = (what: "panel" | "cue") => framed.classList.contains(what === "panel" ? "bare" : "still");

  /** What the last gesture on the canvas or choice in the inspector said: it stays in the status,
   * what lint finds after it, until the source is edited by hand. */
  let told: string | undefined;
  /** The source `told` was said over: it stays through that source's lint, one an edit made before
   * it asked for among them, until the source is edited. */
  let toldOf: string | undefined;
  /** What lint's last report said, which a line taken back leaves standing. */
  let reported = "";
  const say = (text: string) => {
    told = text;
    toldOf = view.state.doc.toString();
    status.textContent = text;
  };
  /** Take back `text`, if the status still says it: `instead` in its place, or what lint said. */
  const unsay = (text: string, instead?: string) => {
    if (told !== text) return;
    if (instead !== undefined) return say(instead);
    told = undefined;
    status.textContent = reported;
  };
  /** The state shown, where a gesture on the canvas or a choice in the inspector makes a patch:
   * none while the source does not compile, nor while the assistant works. */
  const showing = () => {
    const state = last?.valid && !last.error ? last.states[shown]?.[0] : undefined;
    return state !== undefined && !assisting ? { state, index: shown } : undefined;
  };
  /** `state` as a person counts it: `Slide 3`, or `Slide 3, step 2` (PLAN 2.99), in the deck a
   * patch just made where the source has not shown it yet, so a step the patch adds is counted. */
  const slideOf = (state: string) =>
    (taken?.edited ?? last)?.slots?.find((s) => s.state === state)?.said ?? "the slide";
  /** The words for the state shown (PLAN 2.99): its objects by what they show, the theme's names
   * said. Asked for each time the state shown is inspected. */
  let lexicon: Lexicon | undefined;
  /** Take `source`, a patch made on the canvas or in the inspector: one change, one step to undo,
   * which `said`, the change in a person's words, names when it is undone (lift-up USE-4). */
  const made = (source: string, edited: Edited, said?: string) => {
    taken = { source, edited, showings };
    anchor();
    const changes = change(view.state.doc.toString(), source);
    const effects = said ? [described.of(describing(said))] : [];
    view.dispatch({ changes, effects, userEvent: "input.canvas", annotations: isolateHistory.of("full") });
  };
  /** The node selected, its look chosen from the theme in the inspector (PLAN 2.33): each choice a
   * patch, as each gesture on the canvas is. */
  /** The theme's layouts by their sections, each drawn (PLAN 3.31): + Slide's, and the inspector's
   * Layout. */
  const picking = layoutPicker(stage, $<HTMLDialogElement>("#starting"));
  /** The icons that ship, to insert one where the pointer last pressed (PLAN 2.104), or, with an
   * icon selected, to put another in its place (PLAN G.23). */
  const iconing = iconPicker($<HTMLDialogElement>("#iconing"));
  async function insertIcon() {
    const swapping = await board.selectedIcon();
    const icon = await iconing.pick(swapping ? "Replace the icon" : undefined);
    if (icon) await (swapping ? board.swapIcon(swapping, icon) : board.insertIcon(icon));
    $("#overlay").focus();
  }
  const look = looks(
    stage,
    $("#look"),
    {
      shown: showing,
      slide: slideOf,
      format,
      source: () => view.state.doc.toString(),
      apply: made,
      say,
      style: (given, words) => board.style(given, words),
      arrange: (how) => board.arrange(how, false),
      group: () => board.group(),
      ungroup: () => board.ungroup(),
      pick: () => board.pick(),
      select: (node) => board.select(node),
      pickLayout: (names, current) => {
        const now = showing();
        if (!now) return Promise.resolve(undefined);
        return picking.pick({ purpose: { names, current }, source: view.state.doc.toString(), state: now.state, format: format() });
      },
      write: (kind, node) => write(kind, node),
      // A map fetched again (PLAN O.1c), with the Geoapify key kept: none kept, Insert › Map… asks.
      remap: (node, how) =>
        void load("geoapify").then((kept) => (kept?.key ? board.refreshMap(node, kept.key, how) : say("No Geoapify key is kept: Insert › Map… keeps one"))),
      // The tab a finding's field stands in (lift-up ATTR-4), and the timeline under the canvas.
      reveal: (on) => tab(on === "animate" ? "animate" : "inspector"),
      timeline: (show) => {
        if (show !== undefined) hiding("cue", !show);
        return !hidden("cue");
      },
    },
    $("#animating"),
  );

  /** The cue of the state shown (PLAN 2.44), under the preview: once the canvas is there. */
  let cueing: ReturnType<typeof cue> | undefined;
  /** Find and replace across the deck's texts (PLAN 2.47), once the canvas is there. */
  let finding: ReturnType<typeof finder> | undefined;
  /** The preview as a canvas (PLAN 2.31): each gesture a patch, which comes into the source as
   * one change, one step to undo. It waits while the source does not compile, and while the
   * assistant works. */
  const board = canvas(stage, $("#overlay"), {
    shown: showing,
    slide: slideOf,
    numbered: (typed) => numbered(last?.slots ?? [], typed),
    format,
    source: () => view.state.doc.toString(),
    version: () => version,
    at: (node) => inspected?.nodes[node]?.at as Record<string, unknown> | undefined,
    type: (node) => typeOf(node),
    transform: (node) => inspected?.nodes[node]?.transform as { rotate?: number; anchor?: [number, number] } | undefined,
    apply: made,
    unsay,
    typed: (source, edited, joins) => {
      taken = { source, edited, showings };
      if (!joins) anchor();
      const changes = change(view.state.doc.toString(), source);
      // A burst of typing is one step to undo. CodeMirror's history joins a change to the one
      // before it when both are `input.type.compose`; the burst's first starts a step of its own.
      view.dispatch(
        joins
          ? { changes, userEvent: "input.type.compose" }
          : { changes, effects: [described.of("what was typed")], userEvent: "input.type", annotations: isolateHistory.of("before") },
      );
    },
    // The canvas shows what an undo made at once, not once the editor would lint.
    undo: () => {
      if (undo(view)) forceLinting(view);
    },
    redo: () => {
      if (redo(view)) forceLinting(view);
    },
    say,
    selected: (node, also) => {
      chosen = node;
      beside = node === undefined ? [] : also;
      const all = node === undefined ? [] : [node, ...also];
      for (const row of inspector.querySelectorAll("tr[data-node]")) row.setAttribute("aria-selected", String(all.includes(row.getAttribute("data-node") ?? "")));
      void look.show(node, also);
      void cueing?.offer();
      // The cue's bars of what is selected stand out, and of what a group or a container holds.
      const boxes = board.boxes() ?? [];
      const held = new Set(all);
      for (let more = true; more; ) {
        more = false;
        for (const b of boxes) {
          if (b.parent === null || !held.has(b.parent) || held.has(b.node)) continue;
          held.add(b.node);
          more = true;
        }
      }
      cueing?.selected([...held]);
      layering.selected(all);
    },
    // Characters selected in a text typed in: the inspector gives them a look (PLAN 2.38), and
    // focus there keeps the text typed in.
    chose: (selected) => {
      characters = selected;
      void look.characters(selected);
    },
    // Focus in the inspector, or in the assistant asked about them, keeps the text typed in and
    // its characters selected (PLAN 2.38, 2.52).
    // So does focus in the Data tab, whose Quote quotes a cell in them (PLAN 2.72).
    keeps: (to) =>
      to instanceof Node &&
      ($("#look").contains(to) || $("#assistant").contains(to) || $("#data").contains(to) || to === $("#tab-data") || to === $("#tab-document")),
    zoomed: (zoom) => {
      $("#zoom output").textContent = `${Math.round(zoom * 100)}%`;
    },
    ruled: (on) => $("#grid").setAttribute("aria-pressed", String(on)),
    safe: (on) => $("#safe").setAttribute("aria-pressed", String(on)),
    themeEdit: async (ops, what) => {
      const r = await themeEdit(ops, what);
      return Boolean(r && !r.refused);
    },
    layouting: (on) => $("#layout-edit").setAttribute("aria-pressed", String(on)),
    // A finding's mark on the canvas (PLAN 2.49): its fix taken, or where the source writes it.
    fix: (f) => fix(f),
    go: (f) => go(f),
    // A finding no fix can make, about what the inspector sets (PLAN 2.56): its field in focus.
    edit: (f, prop) => {
      tab("inspector");
      if (f.node !== undefined) look.focus(f.node, prop);
    },
    explain: (f) => explain(f),
    // A right click, or the menu key (PLAN 2.53): what is done to the nodes selected, or on the
    // canvas where nothing is.
    // What is locked under the pointer, which the canvas passes over, is offered to unlock (PLAN 2.95).
    menu: (x, y, on) => {
      const unlock: Command[] = board.lockedThere().map((n) => ({ label: `Unlock ${object(lexicon, n, true)}`, run: () => board.lock([n], false) }));
      const here = on === "node" ? nodeCommands().filter((c) => c.where?.includes("node")) : canvasCommands(true);
      void menu(x, y, [...here, ...unlock], $("#overlay"));
    },
    // A chart's mark, or a table's row (PLAN 2.64): a click chooses its rows where the data is
    // shown, and a double click opens the Data tab on them.
    // An annotation made from a mark keeps to the state shown as the inspector's choices do (PLAN 2.67).
    keeping: () => look.keeping(),
    pointedAt: async (at, open) => {
      if (!open && $("#data").hidden) return false;
      const now = showing();
      if (!now) return false;
      const mark = await stage.markAt(now.state, at, format()).catch(() => undefined);
      if (!mark) return false;
      if (open) tab("data");
      await data.select(mark.source, mark.rows);
      say(`${sentence(object(lexicon, mark.node, true))} is drawn from ${rowsSaid(mark.rows)} of the data “${sourced(lexicon, mark.source)}”`);
      return true;
    },
  }, $("#marks"), $("#marked"));
  // A new deck's title slide outlines its empty places, as a slide made here does (PLAN 3.30).
  if ("create" in source) for (const state of stage.opened.states) board.freshen(state);
  /** The layers of the state shown (PLAN 2.50): a tab beside the inspector, each change a patch. */
  const layering = layers(stage, $("#layers"), {
    shown: showing,
    format,
    source: () => view.state.doc.toString(),
    select: (node) => board.select(node),
    apply: made,
    lock: (node, on) => board.lock([node], on),
    say,
  });
  /** A data file written, the source as it stands (PLAN 2.55): its deck shown and linted again, as
   * `edited` says, and a change to save. */
  const wrote = (edited: Edited) => {
    const source = view.state.doc.toString();
    taken = { source, edited, showings };
    filed++;
    tell();
    // The source is as it was, so CodeMirror lints nothing: the edit is shown here.
    const findings = take(edited, source, version);
    view.dispatch(setDiagnostics(view.state, findings.map((f) => diagnostic(f, view.state.doc.length))));
  };
  /** The deck's data (PLAN 2.55): a source as a table in a tab of its own, each change one write of
   * its file, or one patch of rows written inline. */
  const data = sheets(stage, $("#data"), {
    shown: showing,
    format,
    source: () => view.state.doc.toString(),
    apply: made,
    took: wrote,
    undo: () => void undo(view),
    redo: () => void redo(view),
    say,
    words: () => lexicon,
    chose: (source, rows) => {
      rowsChosen = rows.length ? { source, rows } : undefined;
      void outlineRows();
    },
    quote: (quote, what) => board.style({ quote }, what),
  });
  /** The rows chosen in the data, and what they draw in the state shown: outlined on the canvas,
   * and said under the table, while the Data tab is shown (PLAN 2.64). */
  let rowsChosen: { source: string; rows: number[] } | undefined;
  let outlining = 0;
  async function outlineRows() {
    const asked = ++outlining;
    const now = showing();
    const chosen = rowsChosen;
    if (!now || !chosen || $("#data").hidden) {
      board.markRows(undefined, []);
      return data.drawn("");
    }
    const marks = await stage.marksOf(now.state, chosen.source, chosen.rows, format()).catch((): DataMark[] => []);
    if (asked !== outlining) return;
    board.markRows(now.state, marks);
    const what = marks.map((m) => `${object(lexicon, m.node, true)} (${m.key.replaceAll("\u001f", " · ")})`);
    const draws = chosen.rows.length === 1 ? "draws" : "draw";
    data.drawn(what.length ? `On this slide, ${rowsSaid(chosen.rows)} ${draws} ${what.join(", ")}.` : `On this slide, nothing draws ${rowsSaid(chosen.rows)}.`);
  }
  observing(new MutationObserver(() => void outlineRows())).observe($("#data"), { attributes: true, attributeFilter: ["hidden"] });
  /** The bundle's files (PLAN 2.59): its images, fonts, and data, what uses each in which states,
   * and those nothing names taken out, each one change the panel's undo takes back. */
  const filing = filesPanel(stage, $("#files"), {
    shown: showing,
    format,
    source: () => view.state.doc.toString(),
    took: wrote,
    // A file taken out is a change that leaves the source as it is, in its one history (PLAN 2.59).
    removed: (path, edited) => {
      anchor();
      view.dispatch({ effects: fileTaken.of({ path, out: true }), annotations: isolateHistory.of("full") });
      wrote(edited);
    },
    undo: () => {
      if (undo(view)) forceLinting(view);
    },
    redo: () => {
      if (redo(view)) forceLinting(view);
    },
    // The cursor goes into the state, as the strip puts it there: the preview stays on it.
    show: async (state, node) => {
      const index = last?.states.findIndex(([id]) => id === state) ?? -1;
      const start = last?.states[index]?.[1];
      if (start === undefined) return;
      view.dispatch({ selection: { anchor: start }, scrollIntoView: true });
      await show(index);
      board.select(node);
    },
    insert: async (path) => {
      const inserts = await stage.inserts().catch(() => []);
      const n = inserts.findIndex((i) => i.node.type === "image" && (i.node as { src?: unknown }).src === path);
      if (n < 0) return say(`Not inserted: ${path.replace(/^.*\//, "")} isn't a PNG or a JPEG the deck holds`);
      await board.insert(n, `the picture ${path.replace(/^.*\//, "")}`);
    },
    say,
    words: () => lexicon,
    slide: slideOf,
  });
  /** The deck's story (PLAN G.9): its narrative health, read by the generator's module, each finding
   * a slide to show, a fix that is one change to undo, and a question for the assistant. */
  const story = storyPanel(stage, $("#story"), {
    source: () => view.state.doc.toString(),
    check: (state) => {
      if (assisting) return say("Nothing asked: the assistant is at work on the deck");
      void writing.offer({ kind: "claim", state });
    },
    show: async (state) => {
      const index = last?.states.findIndex(([id]) => id === state) ?? -1;
      const start = last?.states[index]?.[1];
      if (start === undefined) return;
      view.dispatch({ selection: { anchor: start }, scrollIntoView: true });
      await show(index);
    },
    make: async (ops, done) => {
      const now = showing();
      if (!now) return;
      const { source, edited } = await stage.make(() => view.state.doc.toString(), ops, now.index, format());
      made(source, edited);
      say(done);
    },
    ask: (question) => {
      $<HTMLButtonElement>("#tab-assistant").click();
      const box = $<HTMLTextAreaElement>("#question");
      box.value = question;
      box.focus();
    },
    say,
  });
  /** The deck's versions (PLAN 2.60): what the bundle's history keeps, each shown as it was,
   * compared, and made the deck again as one change, which ⌘Z undoes with its data files. */
  const versioning = versionsPanel(stage, $("#versions"), {
    shown: showing,
    source: () => view.state.doc.toString(),
    keep: async () => {
      if (assisting) return void say("No history begun: the assistant is at work on the deck");
      await save(true);
    },
    restore: async (version) => {
      if (assisting) return void say("Not restored: the assistant is at work on the deck");
      try {
        const done = await stage.restoreVersion(view.state.doc.toString(), version, shown, format());
        const r = done.restored;
        if (!r.applied || done.source === undefined || !done.edited) {
          const errors = r.added.filter((f) => f.severity === "error");
          say(`Version ${version.n} not restored: ${errors.length ? why(lexicon, errors) : "the deck would not open with the files it has now"}`);
          return r;
        }
        // One change, which carries the data files the restore wrote for its undo.
        const effects = [filesWritten.of(done.files), described.of(`Version ${version.n} restored`)];
        anchor();
        if (done.source === view.state.doc.toString()) {
          view.dispatch({ effects, annotations: isolateHistory.of("full") });
          wrote(done.edited);
        } else {
          taken = { source: done.source, edited: done.edited, showings };
          const changes = change(view.state.doc.toString(), done.source);
          view.dispatch({ changes, effects, userEvent: "input.restore", annotations: isolateHistory.of("full") });
        }
        const files = r.files.length ? `, with ${r.files.map((f) => f.replace(/^.*\//, "")).join(", ")}` : "";
        say(`Version ${version.n} restored${files} · ⌘Z undoes it`);
        return r;
      } catch (e) {
        say(`Not restored: ${said(e)}`);
      }
    },
    say,
    words: () => lexicon,
    slide: (state) => last?.slots?.find((s) => s.state === state)?.said,
  });
  /** The state shown in each of the deck's formats, side by side under the canvas (PLAN 2.62): a
   * click opens the canvas in one, as the format menu does. */
  const formatting = formatsRow(stage, $("#formats"), $<HTMLButtonElement>("#formats-open"), {
    state: () => last?.states[shown]?.[0] ?? stage.opened.states[shown],
    format: () => formatPicker.value,
    own,
    open: (name) => {
      if (formatPicker.value === name) return;
      formatPicker.value = name;
      formatPicker.dispatchEvent(new Event("change"));
    },
  });
  formatting.formats(sizes(stage.opened.formats), own());
  /** The deck played as presented, each state's time kept, and kept as its hold on request: one
   * patch, one step to undo (PLAN 2.63). */
  const rehearsing = rehearsal(stage, $("#rehearsing"), $("#rehearsal-notes"), $<HTMLDialogElement>("#rehearsed"), $("#preview"), {
    slots: async () => (showing() || last?.valid ? stage.timeline(format()) : undefined),
    format,
    notes: (state) => stage.notes(state),
    show: (index) => void show(index),
    keep: async (ops, what) => {
      try {
        const { source, edited } = await stage.make(() => view.state.doc.toString(), ops, shown, format());
        made(source, edited);
        say(`Kept ${what} · ⌘Z undoes them`);
        return true;
      } catch (e) {
        say(`Not kept: ${said(e)}`);
        return false;
      }
    },
    say,
  });
  heed($("#rehearse-open"), "click", () => void rehearsing.start());
  /** The deck's theme, edited (PLAN 2.61, ADR-0016): its colors, type roles, and spacing, each
   * change one edit of the theme the deck names, the deck drawn in it. Refused, it says why; else
   * the source's history takes it as a change that carries the theme's text before and after, so
   * ⌘Z writes the theme back, as with a restore's data files. An inline theme is the deck's own:
   * its edit is a change of the source. */
  async function themeEdit(ops: unknown[], what: string, photo?: string) {
    if (assisting) return void say("Theme not changed: the assistant is at work on the deck");
    if (!showing()) return void say("Theme not changed: the source has an error to fix first");
    try {
      const done = await stage.themeEdit(view.state.doc.toString(), ops, shown, format(), photo);
      const r = done.result;
      if (r.refused || done.source === undefined || !done.edited) {
        const errors = r.added.filter((f) => f.severity === "error");
        say(`Theme not changed: ${errors.length ? why(lexicon, errors) : "the deck would not open in it"}`);
        return r;
      }
      const effects = done.files.length ? [filesWritten.of(done.files)] : [];
      if (done.source === view.state.doc.toString()) {
        if (!effects.length) return void say(`Theme: ${what}, as it was`);
        anchor();
        view.dispatch({ effects: [...effects, described.of(`Theme: ${what}`)], annotations: isolateHistory.of("full") });
        wrote(done.edited);
      } else {
        anchor();
        taken = { source: done.source, edited: done.edited, showings };
        const changes = change(view.state.doc.toString(), done.source);
        view.dispatch({ changes, effects: [...effects, described.of(`Theme: ${what}`)], userEvent: "input.theme", annotations: isolateHistory.of("full") });
      }
      const issues = r.added.length || r.removed.length ? ` · ${issuesChanged(r.added.length, r.removed.length)}` : "";
      say(`Theme: ${what}${issues} · ⌘Z undoes it`);
      return r;
    } catch (e) {
      say(`Theme not changed: ${said(e)}`);
    }
  }
  /** The images the bundle holds that a theme may take its colors from (PLAN 2.94): each PNG and
   * JPEG, with the nodes drawn from it. */
  const photos = async () =>
    (await stage.bundleFiles(view.state.doc.toString())).filter((f) => f.type === "image" && PICTURE.test(f.path));
  const theming = themePanel(stage, $("#theming"), { edit: themeEdit, photos, words: () => lexicon });
  /** The theme given the colors of the photo the image `node` draws in the state shown (PLAN 2.94). */
  async function colorsFrom(node: string) {
    const state = showing()?.state;
    const files = await photos().catch(() => []);
    const draws = (f: (typeof files)[number], here: boolean) => f.used.some((u) => u.node === node && (!here || !state || u.states.includes(state)));
    const file = files.find((f) => draws(f, true)) ?? files.find((f) => draws(f, false));
    if (!file) return void say(`Theme not changed: ${object(lexicon, node, true)} draws no photo the deck holds`);
    await theming.fromPhoto(file.path);
  }
  /** Files an undo or a redo of a restore or a theme edit writes back (PLAN 2.60, 2.61), each to
   * its text after. Where the source did not change, it is `source`, which nothing compiles
   * again: the deck is shown and linted again here, as after a data file's edit. */
  function rewrite(files: Rewritten[], source?: string) {
    const written = files.map(({ path, after }) => ({ path, text: after }));
    const edit = source === undefined ? undefined : { source, index: shown, format: format() };
    void stage.writeFiles(written, edit).then((edited) => {
      if (edited) wrote(edited);
    }, failed);
  }
  // The zoom's buttons (PLAN 2.46), as ⌘−, ⌘+, and ⌘0.
  $("#zoom").onclick = (e) => {
    const how = (e.target as Element).closest<HTMLElement>("[data-zoom]")?.dataset.zoom;
    if (how === "in" || how === "out" || how === "fit") void board.zoom(how);
  };
  // The theme's grid over the canvas (PLAN 2.57), as ⌘' draws it.
  $("#grid").onclick = () => void board.rule();
  $("#safe").onclick = () => void board.shield();
  $("#layout-edit").onclick = () => void board.layoutMode();
  /** The cue of the state shown (PLAN 2.44): a bar for its transition and each motion, which a
   * drag or a key times, each a patch; a press on its ruler shows the cue at that time. */
  cueing = cue(stage, $("#cue"), $("#preview"), {
    shown: showing,
    format,
    source: () => view.state.doc.toString(),
    apply: made,
    say,
    chosen: () => board.chosen()[0],
    select: (node) => board.select(node),
    states: () => last?.states.map(([id]) => id) ?? [],
  });
  /** Find and replace across the deck's words, in every state (PLAN 2.47, 2.83): a bar above the
   * preview. Each match shows its state, and in a text its node selected and its characters
   * marked, or the node a description describes; each replacement is a patch, written where the
   * words live. */
  finding = finder(stage, $("#find"), {
    shown: showing,
    states: () => last?.states.map(([id]) => id) ?? [],
    format,
    source: () => view.state.doc.toString(),
    apply: made,
    say,
    reveal: async (state, node, from, to) => {
      const index = last?.states.findIndex(([id]) => id === state) ?? -1;
      if (index >= 0 && index !== shown) await show(index);
      await board.mark(node, from, to);
    },
    unmark: () => void board.mark(),
    words: () => lexicon,
    slide: slideOf,
    home: () => $("#overlay").focus(),
  });
  $("#find-open").onclick = () => finding?.open();
  /** The state strip (PLAN 2.35): the deck's states, each a thumbnail, and the patches that add,
   * move, and remove them. */
  const states = strip(stage, $("#strip"), {
    shown: showing,
    format,
    source: () => view.state.doc.toString(),
    show: (index) => {
      // The slide shown, clicked again, is the slide itself: nothing on it is selected, and the
      // inspector shows its own settings.
      if (index === shown) board.select(undefined);
      const start = last?.states[index]?.[1];
      if (start !== undefined) view.dispatch({ selection: { anchor: start }, scrollIntoView: true });
      void show(index);
    },
    apply: made,
    say,
    started: (state) => board.freshen(state),
    pick: (picked) => picking.pick(picked),
  });
  /** The light table (PLAN 2.97): every slide in a grid, in place of the preview, to move, copy,
   * and take out across the deck. */
  const table = lightTable(stage, $("#slides"), $<HTMLButtonElement>("#slides-open"), {
    shown: showing,
    format,
    source: () => view.state.doc.toString(),
    show: (index) => {
      const start = last?.states[index]?.[1];
      if (start !== undefined) view.dispatch({ selection: { anchor: start }, scrollIntoView: true });
      void show(index);
    },
    apply: made,
    opened: (open) => {
      $("#work").classList.toggle("tabled", open);
      if (!open) $("#overlay").focus();
    },
    say,
    unsay,
  });
  // ⌘Z, ⇧⌘Z, and ⌘Y where nothing on the page takes them itself (PLAN 2.77): the strip, the layers,
  // the inspector's choices, a button. A field keeps its own typing's undo; the source, the canvas,
  // the text typed in, the Data tab, and the Files tab answer them first.
  heed(document, "keydown", (e) => {
    if (e.defaultPrevented || !(e.metaKey || e.ctrlKey) || e.altKey) return;
    const key = e.key.toLowerCase();
    if (key !== "z" && key !== "y") return;
    const at = e.target instanceof Element ? e.target : null;
    if (at?.closest('input, textarea, [contenteditable="true"], .cm-editor, #overlay, #data, #files')) return;
    e.preventDefault();
    if (assisting) return say("The assistant is at work on the deck: undo once it is done");
    if (key === "y" || e.shiftKey) {
      if (redo(view)) forceLinting(view);
    } else if (undo(view)) forceLinting(view);
  });
  /** Insert › Image… (the QA pass's J3): a PNG or a JPEG picked from the disk, put on the slide
   * as one dropped there is. */
  function pickImage() {
    const input = Object.assign(document.createElement("input"), { type: "file", accept: "image/png,image/jpeg,.png,.jpg,.jpeg" });
    input.addEventListener("change", () => {
      const file = input.files?.[0];
      if (file) void board.insertPicture(file);
    });
    input.click();
  }
  /** What may be inserted (PLAN 2.34), grouped by type, as the deck offers it: asked again after
   * each edit, since the theme and the bundle's images change. Insert opens it as a menu
   * (`opens`): browsing the menu inserts nothing, and a choice is one patch (lift-up USE-3). */
  const inserter = $<HTMLButtonElement>("#insert");
  let offered = "";
  /** What may be inserted, by label, as the deck last offered it: what the palette lists. */
  let insertable: string[] = [];
  /** The Insert menu's items, each under its kind. */
  let insertItems: Command[] = [];
  async function offer() {
    const state = last?.states[shown]?.[0];
    const [inserts, words] = await Promise.all([
      stage.inserts().catch(() => undefined),
      state === undefined ? Promise.resolve(lexicon) : stage.words(state).catch(() => lexicon),
    ]);
    const key = JSON.stringify([inserts?.map((i) => i.label), words?.types, words?.sources]);
    if (!inserts || key === offered) return;
    offered = key;
    insertable = [];
    // Each as a person names it (PLAN 2.99): its kind, then a text by its role, a shape by its
    // kind, an effect by the theme's preset, a source's chart or table by the source's file
    // (PLAN 3.18), and a picture by its name.
    const items: Command[] = [];
    for (const [n, insert] of inserts.entries()) {
      const { kind, said } = inserting(words, insert);
      items.push({
        label: said,
        section: kind,
        run: () => {
          void board.insert(n, said);
          $("#overlay").focus();
        },
      });
      insertable.push(`${kind} · ${said}`);
    }
    // Icons are the set's that ship, not the theme's or the bundle's: Icon… opens their picker. A
    // picture made by the user's image model (PLAN 2.107) comes after what the deck offers.
    items.push({ label: "Icon…", section: "Icon", run: () => void insertIcon() });
    // The draw tools, each with its key (PLAN 2.123): a drag on the slide draws what each draws. A
    // draw tool is its key's gesture: chosen again while it is taken up, it stays so.
    for (const { key, tool } of DRAWING) {
      items.push({
        label: `${tool} (${key.toUpperCase()})`,
        section: "Draw",
        run: () => {
          if (key === "p" ? board.pen() : board.armed()?.key === key) return void $("#overlay").focus();
          press(key);
        },
      });
    }
    // A picture from the disk (the QA pass's J3), as one dropped lands.
    items.push({ label: "Image…", run: pickImage });
    items.push({ label: "Make a picture…", run: () => void picturing.open() }, { label: "Map…", run: () => void mapping.open() });
    insertItems = items;
  }
  opens(inserter, () => insertItems);
  /** The deck's theme (PLAN 2.39): the theme files the bundle holds, the one it names chosen, and
   * the themes that ship, by family, in the dialog Theme… opens. Re-theme alone re-themes the deck
   * in the one chosen (lift-up USE-3), as `scaena theme --apply` does: refused, with why, where
   * the deck would not validate in it; else one change, one step to undo, and the status says
   * what lint finds in it that it did not before. */
  const themeButton = $<HTMLButtonElement>("#theme");
  const retheming = $<HTMLDialogElement>("#retheming");
  const themeList = retheming.querySelector<HTMLElement>("[data-themes]")!;
  const rethemeButton = $<HTMLButtonElement>("#retheming-apply");
  /** Each theme the dialog offers: its value (`path:…` for one the deck holds, `ships:…` for one
   * that ships, none for the deck's own), what it is called, its section, and whether the deck
   * is in it now. */
  let themeChoices: { value: string; label: string; family: string; current: boolean }[] = [];
  let themesOffered = "";
  /** Whether the dialog's list is the themes offered now. */
  let themesListed = false;
  async function offerThemes() {
    const held = await stage.themes().catch(() => undefined);
    if (!held) return;
    const key = JSON.stringify([held, Object.keys(themes)]);
    if (key === themesOffered) return;
    themesOffered = key;
    themeChoices = held.files.map((file) => ({ value: `path:${file}`, label: themeName(file), family: "In this deck", current: file === held.current }));
    if (held.current === "(inline)") themeChoices.push({ value: "", label: "The deck’s own", family: "In this deck", current: true });
    // The themes that ship, by family, each family a section (PLAN T.0d), each with its title
    // slide drawn at build (PLAN T.0c).
    for (const [name, { family }] of Object.entries(themes)) themeChoices.push({ value: `ships:${name}`, label: name, family, current: false });
    const current = themeChoices.find((c) => c.current);
    // The words under the toolbar's icon, which the toolbar keeps in its label.
    (themeButton.querySelector(".label") ?? themeButton).textContent = current ? `Theme: ${current.label}` : "Theme…";
    // The list is built as the dialog opens: built for each deck opened, its thumbnails would load
    // in a dialog no one sees, and each deck opened after would cut the last one's off.
    themesListed = false;
    if (retheming.open) listThemes();
  }
  /** The dialog's list of the themes offered, by family, each that ships with its thumbnail. */
  function listThemes() {
    if (themesListed) return;
    themesListed = true;
    const families = [...new Set(themeChoices.map((c) => c.family))];
    themeList.innerHTML = families
      .map(
        (family) =>
          `<fieldset><legend>${html(family)}</legend>${themeChoices
            .filter((c) => c.family === family)
            .map((c) => {
              const thumbnail = c.value.startsWith("ships:") ? themes[c.label]?.thumbnail : undefined;
              return `<label><input type="radio" name="theme" value="${html(c.value)}"${c.current ? " checked" : ""} />${
                thumbnail ? `<img src="${html(thumbnail)}" alt="" width="96" height="54" />` : ""
              }<span>${html(c.label)}</span>${c.current ? '<span class="now">Now</span>' : ""}</label>`;
            })
            .join("")}</fieldset>`,
      )
      .join("");
  }
  /** The theme chosen in the dialog. */
  const pickedTheme = () => themeList.querySelector<HTMLInputElement>("input:checked")?.value;
  themeButton.onclick = () => {
    if (!themeChoices.length) return say("The deck's theme is not read yet");
    retheming.returnValue = "";
    listThemes();
    for (const input of themeList.querySelectorAll<HTMLInputElement>("input")) input.checked = themeChoices.find((c) => c.value === input.value)?.current ?? false;
    rethemeButton.disabled = true;
    retheming.showModal();
  };
  // Browsing the themes changes nothing: Re-theme takes one other than the deck's.
  heed(themeList, "change", () => {
    const picked = pickedTheme();
    rethemeButton.disabled = picked === undefined || (themeChoices.find((c) => c.value === picked)?.current ?? false);
  });
  heed(retheming, "close", () => {
    const picked = pickedTheme();
    if (retheming.returnValue === "retheme" && picked !== undefined) void retheme(picked);
  });
  /** The deck re-themed in `chosen`: `ships:NAME`, or `path:FILE` of a theme it holds. */
  async function retheme(chosen: string) {
    const now = showing();
    const [kind, name] = [chosen.slice(0, chosen.indexOf(":")), chosen.slice(chosen.indexOf(":") + 1)];
    if (!now || !name) return say("Not re-themed: the source has an error to fix first, or the assistant is at work");
    const label = kind === "ships" ? name : themeName(name);
    say(`Re-theming in ${label}…`);
    try {
      const theme = kind === "ships" ? { ships: name } : { path: name };
      const { themed, source, edited } = await stage.retheme(view.state.doc.toString(), theme, now.index, format());
      if (themed.refused) {
        const errors = themed.added.filter((f) => f.severity === "error");
        say(`${label} not taken: the deck would not open in it. ${why(lexicon, errors, 1)}`);
      } else if (source && edited) {
        made(source, edited);
        const [worse, better] = [themed.added.length, themed.removed.length];
        const n = themed.recolored.length;
        const recolored = n ? ` · ${n} text${n === 1 ? "" : "s"} recolored to read on what lies behind ${n === 1 ? "it" : "them"}` : "";
        // A text the theme set past its box, or over another, fitted as lint's fix fits it.
        const r = themed.refitted.length;
        const refitted = r ? ` · ${r} text${r === 1 ? "" : "s"} fitted to ${r === 1 ? "its box" : "their boxes"}` : "";
        say(`Theme ${label}${recolored}${refitted} · ${issuesChanged(worse, better)}`);
      }
    } catch (e) {
      say(`Not re-themed: ${said(e)}`);
    } finally {
      themesOffered = "";
      void offerThemes();
    }
  }
  // A node's row in the inspector selects it on the canvas.
  inspector.onclick = (e) => {
    const row = (e.target as Element).closest("tr[data-node]");
    if (row) board.select(row.getAttribute("data-node") ?? undefined);
  };

  /** Compile what is in the editor, show it, and lint the state shown: CodeMirror's lint
   * source. Once typing stops, every state is linted. */
  async function lint(view: EditorView): Promise<Diagnostic[]> {
    const source = view.state.doc.toString();
    const read = version;
    const sent = performance.now();
    // An edit's own result was asked for when the edit was made: a state shown since is a move.
    const asked = taken?.source === source ? taken.showings : showings;
    let edited: Edited;
    if (taken?.source !== source && source !== toldOf) told = undefined;
    try {
      if (taken?.source === source) edited = taken.edited;
      else {
        // The worker holds this source's deck from now on, not the one the patch left: an undo,
        // then a redo back to the patch's source, compiles it again, even a redo made before
        // this answer comes. A patch taken meanwhile is the worker's after this, and stays.
        taken = undefined;
        const [before, index] = [among, shown];
        edited = await stage.edit(source, index, format());
        // The state shown stays shown by its id, wherever the edit put it; one taken out (an undo
        // of + Step) gives way to the state before it that stays, as its slide, not the next.
        const want = before && edited.valid && edited.at && asked === showings ? reshown(before, edited.states, index) : undefined;
        if (want !== undefined && want !== edited.at?.index) edited = { ...edited, at: await stage.seek(want, undefined, format()) };
      }
    } catch (e) {
      status.textContent = `Error: ${said(e)}`;
      return [];
    }
    trips.push({ ms: performance.now() - sent, ...edited.ms });
    return take(edited, source, read, asked !== showings).map((f) => diagnostic(f, view.state.doc.length));
  }

  /** Show `edited`, what the worker made of `source`, version `read` of it: the deck shown and
   * linted, its findings listed, and every state linted once typing stops; the state shown as it
   * says, unless another was `moved` to since it was asked for. The findings, to mark in the
   * source. */
  function take(edited: Edited, source: string, read: number, moved = false): Finding[] {
    const before = among;
    last = edited;
    if (edited.valid) {
      // Kept for an undo or a redo back to this source, with the seven compiled before it.
      const was = compiled.findIndex((c) => c.source === source);
      if (was >= 0) compiled.splice(was, 1);
      compiled.unshift({ source, states: edited.states });
      compiled.length = Math.min(compiled.length, 8);
      // Each as a person counts it (PLAN 2.99): Slide 3, Slide 3, step 2.
      statesPicker.replaceChildren(...edited.states.map(([id], i) => new Option(edited.slots[i]?.state === id ? (edited.slots[i].said ?? id) : (edited.slots.find((s) => s.state === id)?.said ?? id), id)));
      reformats(edited.formats, edited.own);
      if (edited.at && !moved) shown = edited.at.index;
      // A state picked since the edit was asked for, among the states before it: the same state
      // among those after it, wherever the edit put it.
      const picked = moved && before ? reshown(before, edited.states, shown) : shown;
      among = edited.states;
      listed = edited.states;
      statesPicker.selectedIndex = shown;
      states.states(edited.slots, shown);
      // The strip's thumbnails as the deck opens, before every state is linted: each is painted
      // again once lint has laid them all out.
      if (!thumbed) {
        thumbed = true;
        void states.paint();
      }
      table.states(edited.slots);
      void inspect();
      void layering.refresh();
      void data.refresh();
      void filing.refresh();
      void story.refresh();
      void versioning.edited();
      void theming.edited();
      void board.refresh().then(() => select(source), failed);
      finding?.changed();
      void offer();
      void offerThemes();
      if (picked !== shown) void show(picked);
      // A cursor the table could not place while this source compiled: placed now.
      if (unfollowed && source === view.state.doc.toString()) follow();
    }
    const findings = edited.error ? [edited.error] : edited.findings;
    report(findings, edited);
    if (!edited.whole) {
      clearTimeout(pending);
      pending = setTimeout(() => void lintAll(read), pause);
    }
    return findings;
  }

  /** Whether the strip has asked for its thumbnails yet. */
  let thumbed = false;

  /** Lint every state of the deck compiled from version `read`, and show what it finds if
   * the source is still that version. */
  async function lintAll(read: number) {
    if (read !== version) return;
    let linted: Linted;
    try {
      linted = await stage.lint();
    } catch (e) {
      status.textContent = `Error: ${said(e)}`;
      return;
    }
    wholes.push(linted);
    if (read !== version || !last) return;
    last = { ...last, findings: linted.findings, laid: linted.laid, whole: true };
    report(linted.findings, last, linted.ms);
    // Every state is laid out: the strip's thumbnails that changed are painted again, and the
    // layouts the inspector suggests are judged again (PLAN 2.92).
    void states.paint();
    void table.paint();
    look.linted();
    view.dispatch(setDiagnostics(view.state, linted.findings.map((f) => diagnostic(f, view.state.doc.length))));
  }

  /** The findings under the source, and the status line: what was found, and how long each
   * step took; `whole`, how long the lint of every state took. */
  function report(findings: Finding[], edited: Edited, whole?: number) {
    list(findings);
    // Those about each state stand on the canvas and count in the strip (PLAN 2.49).
    board.found(findings);
    states.found(findings);
    table.found(findings);
    formatting.found(findings);
    if (edited.error) {
      status.textContent = reported = "The source has an error to fix";
      status.title = "";
      return;
    }
    // What lint found, in a person's words (PLAN 2.99); how long each step took, on a pointer
    // over it.
    const linted =
      whole !== undefined ? `every slide checked in ${seconds(whole)}` : `${edited.whole ? "checked" : "the slide checked"} in ${seconds(edited.ms.lint)}`;
    const found = counted(findings);
    reported = found;
    status.textContent = told ? `${told} — ${found}` : found;
    status.title = `Read in ${seconds(edited.ms.compile)}, drawn in ${seconds(edited.ms.paint)}, ${linted}`;
  }

  /** A finding as CodeMirror shows it, with its fix as an action. One about the theme file
   * stands at the top of the source. */
  function diagnostic(f: Finding, length: number): Diagnostic {
    const from = Math.min(f.at?.from ?? 0, length);
    return {
      from,
      to: Math.min(Math.max(f.at?.to ?? 0, from), length),
      severity: f.severity,
      source: f.code,
      message: `${f.file ? `${f.file}: ` : ""}${f.message}${f.format ? ` (${f.format})` : ""}${f.hint ? `\n${f.hint}` : ""}`,
      actions: f.fix ? [{ name: "Fix", apply: () => void fix(f).catch((e) => say(`Not fixed: ${said(e)}`)) }] : [],
    };
  }

  /** Apply `f`'s fix: the engine patches the deck and writes it back as source, and the
   * editor takes only what changed, as a patch from the canvas: one step to undo, and what the
   * status said stays. */
  async function fix(f: Finding) {
    const read = version;
    const next = await stage.fix(f.fix!);
    const edited = await stage.edit(next, shown, format());
    // A source changed while the fix was made keeps the change: lint says again what to fix.
    if (read !== version) throw new Error("the source changed while the fix was made");
    made(next, edited);
  }

  /** Where the source writes what `f` is about, the cursor there: the source opened, if folded. */
  function go(f: Finding) {
    if (!f.at) return;
    if (folded()) fold(true);
    view.dispatch({ selection: { anchor: f.at.from }, scrollIntoView: true });
    view.focus();
  }

  /** Every finding under the canvas, each in a sentence and where it is, by its object, its slide,
   * and its size (PLAN 2.99); one that stands in the source is a button, which goes there. */
  function list(findings: Finding[]) {
    problems.replaceChildren(
      ...findings.map((f) => {
        const li = document.createElement("li");
        li.className = f.severity;
        const where = [
          f.node !== undefined ? object(lexicon, f.node) : "",
          f.state !== undefined ? slideOf(f.state) : f.file ? "The theme" : "The deck",
          f.format ? `in ${f.format}` : "",
        ];
        const said = `${html(f.said ?? f.message)}<span class="where">${html(where.filter(Boolean).join(" · "))}</span>`;
        // One that stands in the source goes there.
        const main = li.appendChild(document.createElement(f.at ? "button" : "span"));
        main.className = "said";
        main.innerHTML = said;
        if (f.at) main.onclick = () => go(f);
        // What it means, and what could be done, by the assistant's model (PLAN 2.101).
        const explaining = li.appendChild(document.createElement("button"));
        explaining.type = "button";
        explaining.className = "explain";
        explaining.textContent = "Explain";
        explaining.title = "What it means, and what could be done, in a few sentences by the assistant's model";
        explaining.setAttribute("aria-label", `Explain: ${f.said ?? f.message}`);
        explaining.onclick = () => explain(f);
        return li;
      }),
    );
  }

  /** What the editor shows (PLAN 2.52): the state, in the format shown, the nodes the canvas
   * selects, each with its type, and the characters selected in the text typed in. */
  function seeing(): Seeing | undefined {
    const at = showing();
    if (!at) return undefined;
    const nodes = (chosen === undefined ? [] : [chosen, ...beside]).map((node) => ({ node, type: layering.type(node, at.state) }));
    const c = characters && characters.state === at.state && board.typing() === characters.node ? characters : undefined;
    return {
      state: at.state,
      format: format(),
      nodes,
      characters: c && { node: c.node, from: c.from, to: c.to, text: c.text },
    };
  }

  /** Once the canvas stands in `source`, select what the assistant's question changed there
   * (PLAN 2.52): those the state shown shows, held where the first is; nothing it shows, and the
   * selection stays. */
  function select(source: string) {
    if (touching?.source !== source) return;
    const { nodes } = touching;
    touching = undefined;
    const boxes = board.boxes() ?? [];
    const holder = (node: string) => boxes.find((b) => b.node === node)?.parent ?? null;
    const shown = nodes.filter((node) => boxes.some((b) => b.node === node));
    if (!shown.length) return;
    board.selectAll(shown.filter((node) => holder(node) === holder(shown[0])));
  }

  /** The state the cursor is in: the last whose declaration starts at or before it. */
  function cursorIn(): number {
    const at = view.state.selection.main.head;
    let index = -1;
    last?.states.forEach(([, start], i) => {
      if (start <= at) index = i;
    });
    return index;
  }

  /** Show the state the cursor is in. A change that wrote over states' starts takes them out of
   * the table until the source is compiled again, and meanwhile the table cannot say which state
   * the cursor is in: an undo that puts the cursor back in such a state would show the one before
   * it. The preview follows the cursor once the source is compiled (`take`), unless a slide is
   * shown meanwhile (`show`). */
  function follow() {
    if (!last?.valid) return;
    unfollowed = last.states.length < listed.length;
    if (unfollowed) return;
    const index = cursorIn();
    if (index < 0 || index === shown) return;
    show(index);
  }

  /** Put the cursor in the state shown, where it stands in another, before a change made outside
   * the source goes in its history: an undo puts back the cursor the change found, and the preview
   * follows the cursor, so the change is undone on the slide it was made on. Find, Rehearse, the
   * assistant, and a step or a slide added each show a slide and leave the cursor where it was. */
  function anchor() {
    const states = last?.states;
    if (!last?.valid || !states?.[shown] || states[shown][0] !== among?.[shown]?.[0]) return;
    const at = cursorIn();
    if (at < 0 || at === shown) return;
    view.dispatch({ selection: { anchor: Math.min(states[shown][1], view.state.doc.length) } });
  }

  async function show(index: number) {
    shown = index;
    among = last?.states;
    showings++;
    // The slide asked for: a cursor the table could not place before it no longer moves the
    // preview once the source is compiled.
    unfollowed = false;
    statesPicker.selectedIndex = index;
    states.select(index);
    formatting.shown();
    await stage.seek(index, undefined, format());
    await inspect();
    void layering.refresh();
    await board.refresh();
    void outlineRows();
  }

  /** The inspector: the state's cue, then each node, its look if it sets text, and how
   * many props its overrides set. */
  async function inspect() {
    const state = last?.states[shown]?.[0] ?? stage.opened.states[shown];
    if (!state) return;
    let found: Inspected;
    try {
      [found, lexicon] = await Promise.all([stage.inspect(state, format()), stage.words(state).catch(() => lexicon)]);
    } catch (e) {
      listing.textContent = said(e);
      return;
    }
    inspected = found;
    cueing?.show(found, lexicon);
    // The Theme tab says the theme's names in the words just read.
    theming.worded();
    const cue = found.timeline;
    // The slide shown, its objects and its animations, in a person's words (PLAN 2.99).
    const words = lexicon;
    const rows = Object.keys(found.nodes).map((id) => {
      const look = found.looks?.[id];
      const own = found.overrides?.[id]?.length ?? 0;
      const weight = look ? (words?.weights[String(look.weight)] ?? String(look.weight)) : "";
      const text = look
        ? `<span class="swatch" style="background:${look.hex}"></span>${html(named(words, "text-role", look.role))} · ${html(look.family)} ${look.size} pt ${html(weight)} · ${html(named(words, "color", look.color))}`
        : "";
      // A box placed where it was let go, off the template's grid, as lint says (W301): once for
      // each place it is written, under the first state that shows it.
      const at = found.nodes[id].at as Record<string, unknown> | undefined;
      const off = at?.rect !== undefined && last?.findings.some((f) => f.code === "W301" && f.node === id);
      const place = at ? `${html(placed(at, undefined, words))}${off ? ' <span class="flag" title="Placed where it was let go, off the theme’s grid">off the grid</span>' : ""}` : "";
      return `<tr data-node="${html(id)}"${id === chosen || beside.includes(id) ? ' aria-selected="true"' : ""}><td>${html(object(words, id))}</td><td>${place}</td><td>${text}</td><td>${own || ""}</td></tr>`;
    });
    const motions = (cue?.motions ?? []).map(
      (m) => `<tr><td>${html(object(words, m.node))}</td><td>${html(moving(words, m.motion, m.split))}</td><td>${seconds(m.start).replace(" s", "")}–${seconds(m.end)}</td></tr>`,
    );
    void look.show(chosen, beside);
    const advance = cue && cue.hold > 0 ? `advances after ${seconds(cue.hold)}` : "advances on a click";
    const style = cue?.transition.style;
    const transition =
      cue && cue.transition.duration > 0
        ? style
          ? `${named(words, "transition/style", style)} transition ${seconds(cue.transition.duration)}${cue.transition.direction ? `, ${named(words, "transition/direction", cue.transition.direction)}` : ""}`
          : `transition ${seconds(cue.transition.duration)}, Magic Move ${cue.transition.match === "id" ? "on" : "off"}`
        : "no transition";
    listing.innerHTML = `
      <h2>${html(slideOf(found.state_id))}${found.layout ? ` · ${html(named(words, "layout", found.layout))}` : ""}</h2>
      ${cue ? `<p>Starts at ${seconds(cue.start)} · plays for ${seconds(cue.span)} · ${advance} · ${transition}</p>` : ""}
      ${motions.length ? `<table><tr><th>Animations</th><th></th><th></th></tr>${motions.join("")}</table>` : ""}
      <h2>Objects</h2>
      <table><tr><th>Object</th><th>Place</th><th>Look</th><th>Own settings</th></tr>${rows.join("")}</table>`;
  }

  /** Save the source as it stands where the bundle is kept, or into the browser's storage. A
   * source that does not compile is not saved, and the status says why. The paths the save
   * renamed are renamed in the source too, and nothing else in it changes. With `keep`, a bundle
   * that keeps no history begins one with this save (PLAN 2.87). */
  async function save(keep = false) {
    status.textContent = "Saving…";
    await settled();
    const [sent, files] = [view.state.doc.toString(), filed];
    let done: Extract<FromWorker, { type: "saved" }>;
    try {
      done = await stage.save(sent, keep);
    } catch (e) {
      status.textContent = `Not saved: ${said(e)}`;
      return;
    }
    return took(done, sent, files);
  }

  /** What a save, a download, or an export holds made first: what is typed in a text on the canvas,
   * and a field in the inspector typed in and not yet left, taken as leaving it takes it. The focus
   * stays in that field. */
  async function settled() {
    const field = document.activeElement;
    const typing = (field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement) && $("#look").contains(field);
    if (typing && field.value !== field.defaultValue) {
      // Leaving it makes its change, as the inspector takes it; the field then holds what the deck
      // reads, as one drawn again would.
      field.blur();
      await look.settled();
      const again = field.id ? document.getElementById(field.id) : null;
      if (!again || again === field) field.defaultValue = field.value;
      (again ?? field).focus({ preventScroll: true });
    }
    await board.idle();
    await look.settled();
  }

  /** Save as (PLAN 2.12): the source as it stands, saved `to` a place of its own, where the
   * bundle is kept from then on: a folder on disk, kept by name as an opened one is, or the
   * browser's storage under a name (`name-2`, … where that is taken). */
  async function saveAs(to: SaveTo) {
    if (where?.kind === "serve") throw new Error("a served bundle saves to the folder scaena serve serves it from");
    status.textContent = "Saving…";
    await settled();
    const [sent, files] = [view.state.doc.toString(), filed];
    let done: Extract<FromWorker, { type: "saved" }>;
    try {
      done = await stage.saveAs(sent, to);
    } catch (e) {
      status.textContent = `Not saved: ${said(e)}`;
      return;
    }
    if ("folder" in to) {
      folderHandle = to.folder;
      await folders("readwrite", (store) => store.put(to.folder, to.folder.name));
    }
    return took(done, sent, files);
  }

  /** A save `done` of `sent`, the source as it stood, and of the bundle's files as `files` edits of
   * them left them: the bundle kept where it went, the paths the save renamed renamed in the source,
   * and the page's address naming it. */
  async function took(done: Extract<FromWorker, { type: "saved" }>, sent: string, files: number) {
    // Saved: the copy of what was not, under the key before and after, goes.
    const was = copyKey();
    clearTimeout(copyTimer);
    copied = new Map();
    where = done.where;
    name = done.where.name;
    // The paths the save renamed, renamed in the source: no step to undo, since the files are named
    // so now, and an undo after the save takes back the edit before it, not them (#414).
    const renamed = change(view.state.doc.toString(), renamedIn(view.state.doc.toString(), done.renamed));
    view.dispatch({ changes: renamed, annotations: Transaction.addToHistory.of(false) });
    // The folder keeps the deck's source too: it is what the editor shows, renamed as the save
    // renamed files.
    if (scnOnDisk) {
      const headers = { "X-Scaena-Client": client, "X-Scaena-Token": token };
      const response = await fetch(scnOnDisk, { method: "PUT", body: view.state.doc.toString(), headers });
      if (!response.ok) throw new Error(`deck.scn: ${(await response.text()).trim() || response.status}`);
    }
    savedSource = renamedIn(sent, done.renamed);
    filedSaved = files;
    void Promise.all([discard(was), discard(copyKey())]).catch(() => {});
    address(where);
    tell();
    void listKept();
    // The save recorded the edits since the last: they are versions now.
    void versioning.refresh();
    const saved = `Saved, ${many(done.files, "file")}${done.recorded ? ", and the changes kept in its history" : ""}`;
    say(saved);
    // Kept in this browser: the browser is asked to keep it for good (lift-up 10-24b REL-4), and a
    // refusal is said after whatever the status says by then (what was saved, or what Play said of
    // keeping the deck to play it), never in its place.
    if (done.where.kind === "opfs")
      void persist((refused) => {
        const now = status.textContent ?? "";
        if (!now.includes(refused)) say(now ? `${now}. ${refused}` : refused);
      });
    return done;
  }

  /** Download the source as it stands as a `.scaena` zip, its fonts subset to what the deck
   * draws. */
  async function download() {
    await settled();
    let zipped: { bytes: ArrayBuffer; subset: [string, number, number][] };
    try {
      zipped = await stage.zip(view.state.doc.toString());
    } catch (e) {
      say(`Not downloaded: ${said(e)}`);
      return;
    }
    const url = URL.createObjectURL(new Blob([zipped.bytes], { type: "application/zip" }));
    Object.assign(document.createElement("a"), { href: url, download: `${name}.scaena` }).click();
    setTimeout(() => URL.revokeObjectURL(url), 60_000);
    const [before, after] = zipped.subset.reduce(([b, a], [, x, y]) => [b + x, a + y], [0, 0]);
    const kb = (n: number) => `${Math.round(n / 1024)} KB`;
    // Said, as a save is, so a lint that answers after it keeps it rather than a word said before.
    say(`Downloaded ${name}.scaena, ${kb(zipped.bytes.byteLength)}: its fonts cut to what it draws, ${kb(before)} to ${kb(after)}`);
    return zipped;
  }

  /** Export (PLAN 2.54) the deck the source compiles to, as `scaena export` writes it, and
   * download it: `png`, the state shown at rest in the format shown, `width` pixels wide;
   * `slides`, every slide once its steps have played, so, in one zip, each named by its number
   * (PLAN 2.122); `pdf`, each slide at its last state; `html`, the deck as one file that plays
   * offline, the single-file player's page beside this one filled in. Each is made in the worker:
   * nothing is sent anywhere. */
  async function exportAs(as: "png" | "slides" | "pdf" | "html", width?: number): Promise<{ file: string; bytes: ArrayBuffer } | undefined> {
    await settled();
    let what: Export;
    let file: string;
    if (as === "slides") {
      const shownIn = format();
      what = { kind: "slides", width: width ?? board.size()[0], format: shownIn, name };
      file = `${name} - Slides${shownIn ? ` (${shownIn.replace(/[^\w.-]+/g, "x")})` : ""}.zip`;
    } else if (as === "png") {
      const now = showing();
      if (!now) {
        say("Not exported: no slide is shown");
        return;
      }
      const shownIn = format();
      what = { kind: "png", state: now.state, width: width ?? board.size()[0], format: shownIn };
      // Named as a person counts the slide (PLAN 2.99): Q3 Review - Slide 3, step 2.png, by a
      // hyphen, which a browser keeps in a file's name where the system's own encoding is not UTF-8.
      file = `${name} - ${slideOf(now.state)}${shownIn ? ` (${shownIn.replace(/[^\w.-]+/g, "x")})` : ""}.png`;
    } else if (as === "pdf") {
      what = { kind: "pdf" };
      file = `${name}.pdf`;
    } else {
      const page = await fetch(new URL("standalone.html", document.baseURI)).catch(() => undefined);
      if (!page?.ok) {
        say("Not exported: this copy of the editor has no page to play a single file (build it with `just web`)");
        return;
      }
      what = { kind: "html", page: await page.text(), name };
      file = `${name}.html`;
    }
    say(`Exporting ${file}…`);
    let bytes: ArrayBuffer;
    try {
      bytes = await stage.export(view.state.doc.toString(), what);
    } catch (e) {
      say(`Not exported: ${said(e)}`);
      return;
    }
    const type = { png: "image/png", slides: "application/zip", pdf: "application/pdf", html: "text/html" }[as];
    const url = URL.createObjectURL(new Blob([bytes], { type }));
    Object.assign(document.createElement("a"), { href: url, download: file }).click();
    setTimeout(() => URL.revokeObjectURL(url), 60_000);
    say(`Downloaded ${file}, ${Math.max(1, Math.round(bytes.byteLength / 1024))} KB`);
    return { file, bytes };
  }

  /** Files dropped at `at` in the source: each joins the bundle, and its path, quoted, goes
   * there. A `.scaena` file opens instead. */
  async function drop(files: File[], at: number) {
    const zip = files.find((f) => f.name.endsWith(".scaena"));
    if (zip) return open({ zip: await zip.arrayBuffer(), name: zip.name.replace(/\.scaena$/, "") });
    const paths: string[] = [];
    for (const file of files) {
      // A picture too large goes in as a smaller copy, made with nothing asked.
      const taken = PICTURE.test(file.name) ? await fitting(stage, file, (words) => (status.textContent = words)) : file;
      if (taken) paths.push(await stage.drop(taken.name, await taken.arrayBuffer()));
    }
    if (!paths.length) return paths;
    const insert = paths.map((p) => JSON.stringify(p)).join(" ");
    view.dispatch({ changes: { from: at, insert }, selection: { anchor: at + insert.length } });
    view.focus();
    status.textContent = `Added ${paths.join(", ")}`;
    return paths;
  }

  const assistant = panel(stage, {
    source: () => view.state.doc.toString(),
    states: () => last?.states.map(([id]) => id) ?? [],
    apply: (source, edited, touched, files = []) => {
      touching = touched?.length ? { source, nodes: touched } : undefined;
      // A theme it edited goes in the source's history with the theme's text before and after,
      // as the Theme tab's edit does (PLAN 2.61): ⌘Z writes the theme back.
      const effects = [...(files.length ? [filesWritten.of(files)] : []), described.of("the assistant’s edit")];
      // An edit that leaves the source as it is wrote a file beside it: a data file (`data_edit`),
      // or the theme (`theme_edit`).
      if (source === view.state.doc.toString()) {
        if (files.length) {
          anchor();
          view.dispatch({ effects, annotations: isolateHistory.of("full") });
        }
        return wrote(edited);
      }
      anchor();
      taken = { source, edited, showings };
      view.dispatch({ changes: change(view.state.doc.toString(), source), effects, userEvent: "input.assistant" });
    },
    seeing,
    lock: (on) => {
      assisting = on;
      view.dispatch({ effects: locked.reconfigure(on ? [EditorState.readOnly.of(true), EditorView.editable.of(false)] : []) });
    },
    drawings: async () => {
      await states.painted();
      return { ...states.drawings(), states: last?.states.map(([id]) => id) ?? [] };
    },
    show: (state) => {
      const index = last?.states.findIndex(([id]) => id === state) ?? -1;
      if (index >= 0) void show(index);
    },
    slide: slideOf,
  });

  /** The writing tools (PLAN 2.101): a text's words, a slide's notes, and an issue, written by the
   * model the assistant uses, with the user's key, each offered to take or leave. */
  const writing = writer(stage, {
    who: () => assistant.who(),
    shown: showing,
    format,
    source: () => view.state.doc.toString(),
    apply: made,
    say,
    keys: () => {
      tab("assistant");
      $("#key").focus();
    },
    slide: slideOf,
  });
  /** Write This Slide Again… and Add a Slide About… (lift-up FUNC-1): one slide written by the model the
   * assistant uses, as the deck's generator builds slides; one step to undo. */
  const asking = slideAsker(stage, {
    who: () => assistant.who(),
    shown: showing,
    format,
    apply: made,
    show: (id) => {
      const k = last?.states.findIndex(([state]) => state === id) ?? -1;
      if (k >= 0) goTo(k);
    },
    say,
    keys: () => {
      tab("assistant");
      $("#key").focus();
    },
    slide: slideOf,
  });
  /** Make a picture (PLAN 2.107): of what the user says, by the image model their OpenAI or Gemini
   * key lists, in the theme's palette, for the image selected, the slide's empty place for a
   * picture, or where they last pressed; one patch, one step to undo. */
  const picturing = picturer(stage, {
    target: async () => {
      if (!ready()) return "Nothing to make a picture for yet: the deck must compile first, with no text typed in";
      return (await board.pictureTarget()) ?? "Nothing to make a picture for yet: the deck must compile first";
    },
    put: (target, made) => board.putPicture(target, made),
    say,
    // The assistant on the provider whose key the picture wants, so the key pasted is kept as its.
    keys: async (provider) => {
      tab("assistant");
      await assistant.choose(provider);
      $("#key").focus();
    },
  });
  /** Insert › Map… (ADR-0030, PLAN O.1c): a map of a place, fetched once from Geoapify with the
   * user's own key, in the image selected, the slide's empty place for a picture, or where they
   * last pressed; one patch, one step to undo. */
  const mapping = mapper(stage, {
    target: async () => {
      if (!ready()) return "Nothing to put a map in yet: the deck must compile first, with no text typed in";
      return (await board.mapTarget()) ?? "Nothing to put a map in yet: the deck must compile first";
    },
    put: (target, map, label) => board.putMap(target, map, label),
    say,
  });
  /** Ask the model the assistant uses to write `kind` for `node`, a text, or for the state shown. */
  function write(kind: WritingKind, node?: string) {
    if (assisting) return say("Nothing asked: the assistant is at work on the deck");
    const now = showing();
    if (!now) return say("Nothing to write yet: the deck must compile first");
    void writing.offer({ kind, state: now.state, node });
  }
  /** Explain `f` by the model the assistant uses: what it means for the slide, and what could be
   * done. One no state holds, as the source's own when it does not compile, is explained all the
   * same: a state only finds the words of what it is about. */
  function explain(f: Finding) {
    if (assisting) return say("Nothing asked: the assistant is at work on the deck");
    const state = f.state ?? showing()?.state;
    const { code, severity, message, said, hint, node } = f;
    void writing.offer({ kind: "explain", state, finding: { code, severity, message, said, hint, node } });
  }

  /** Every command the editor has, by name (PLAN 2.53): what its key or its button does, nothing
   * else, so each edit is the patch that gesture makes. ⌘K lists those that apply, narrowed by
   * the words typed; a right click offers those for what is under the pointer. */
  const overlay = $("#overlay");
  /** Press `key` on the canvas, as the keyboard does there. */
  const press = (key: string, keys: KeyboardEventInit = {}) => {
    overlay.focus();
    overlay.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...keys }));
  };
  const mod = (keys: KeyboardEventInit = {}): KeyboardEventInit => ({ ...keys, [MOD === "⌘" ? "metaKey" : "ctrlKey"]: true });
  /** The browser's own copy or cut, which the canvas takes while it has the focus. */
  const clip = (how: "copy" | "cut") => {
    overlay.focus();
    if (!document.execCommand(how)) say(`Not ${how === "copy" ? "copied" : "cut"}: the browser keeps its clipboard from this page; ${MOD}${how === "copy" ? "C" : "X"} does it`);
  };
  /** The clipboard's text, pasted on the canvas as ⌘V pastes it. */
  const pasted = async () => {
    overlay.focus();
    const text = await navigator.clipboard.readText().catch((e) => void say(`Not pasted: ${said(e)}; ${MOD}V pastes`));
    if (text === undefined) return;
    if (!text) return say("The clipboard holds nothing to paste");
    await board.paste(text);
  };
  /** The words of the code selected, as the slide shows them, put on the clipboard as text (PLAN
   * 2.133): no look, no clip. */
  const copyCode = async (node: string) => {
    const now = showing();
    if (!now) return;
    try {
      const words = await stage.codeWords(view.state.doc.toString(), now.state, node, format());
      await navigator.clipboard.writeText(words);
      say(`The code copied as text: ${words.split("\n").length} lines`);
    } catch (e) {
      say(`Not copied: ${said(e)}`);
    }
  };
  /** The clipboard's text pasted as words, never as code (PLAN 2.133). */
  const pastedAsText = async () => {
    overlay.focus();
    const text = await navigator.clipboard.readText().catch((e) => void say(`Not pasted: ${said(e)}`));
    if (text === undefined) return;
    if (!text) return say("The clipboard holds nothing to paste");
    await board.paste(text, { plain: true });
  };
  /** Whether the canvas takes a gesture: the source compiles, the assistant is not at work, and no
   * text is typed in. */
  const ready = () => showing() !== undefined && board.typing() === undefined;
  /** What the canvas has selected, where it takes a gesture. */
  const picked = () => (ready() ? board.chosen() : []);
  /** Whether `node` has a layout of its own in the format shown (ADR-0020). */
  const anewIn = (node: string) => {
    const formats = inspected?.nodes[node]?.formats as Record<string, unknown> | undefined;
    return !!format() && !!formats?.[format()!];
  };
  /** Give `node` a layout of its own in the format shown, where it stands now: one `place`
   * with `anew`, one step to undo. */
  const placeAnew = async (node: string) => {
    const [f, now] = [format(), showing()];
    if (!f || !now) return;
    const at = (inspected?.nodes[node]?.at as Record<string, unknown> | undefined) ?? {};
    const keys = ["col", "row", "in", "rect", "area", "index", "parent"].filter((k) => k in at);
    // A node with no placement fills the grid's margin box: the slot `grid`.
    const spot = keys.length ? Object.fromEntries(keys.map((k) => [k, at[k]])) : { in: "grid" };
    try {
      const op = { op: "place", node, at: spot, format: f, anew: true };
      const { source, edited } = await stage.make(() => view.state.doc.toString(), [op], now.index, f);
      made(source, edited);
      say(`${sentence(object(lexicon, node, true))} has a layout of its own in ${f}: a move there moves it there alone · ⌘Z undoes it`);
    } catch (e) {
      say(`Not placed anew: ${said(e)}`);
    }
  };
  const typeOf = (node: string) => layering.type(node, showing()?.state ?? "");
  /** Show the tab `id` under the preview, as a click on it does. */
  const tab = (id: "inspector" | "animate" | "document" | "layers" | "data" | "theming" | "files" | "versions" | "story" | "assistant") => {
    // A tab asked for shows the inspector's column, hidden or not.
    hiding("panel", false);
    const button = $<HTMLButtonElement>(`#tab-${id}`);
    button.click();
    // A panel chosen from More shows as More's own tab (lift-up ATTR-4): what the focus goes to.
    return button.hidden ? $<HTMLButtonElement>("#tab-more") : button;
  };
  /** Show the state at `index`, its declaration at the cursor, as the strip does. */
  const goTo = (index: number) => {
    const start = last?.states[index]?.[1];
    if (start !== undefined) view.dispatch({ selection: { anchor: start }, scrollIntoView: true });
    void show(index);
  };
  /** The focus on the slide, from anywhere on the page (PLAN 2.126): the light table, shown in its
   * place, gives way to it. */
  function toSlide() {
    if (table.open()) table.toggle(false);
    $("#overlay").focus();
    say(`${showing() ? slideOf(showing()!.state) : "The slide"}: Tab selects its objects`);
  }
  // The page's first stop for the keys goes to the slide, past the bar (WCAG 2.4.1).
  $<HTMLAnchorElement>("#skip").onclick = (e) => {
    e.preventDefault();
    toSlide();
  };
  /** The focus on the slide shown among the slides and their steps, or on the light table's. */
  function toStrip() {
    const at = table.open() ? "#slides [role=option][tabindex='0']" : "#strip li[tabindex='0']";
    document.querySelector<HTMLElement>(at)?.focus();
  }

  /** What is done to the nodes selected: each its key on the canvas. */
  function nodeCommands(): Command[] {
    const any = () => picked().length > 0;
    const one = () => picked().length === 1;
    /** One text selected, and it is code as the slide shows it (PLAN 2.133, 2.139). */
    const isCode = () => one() && !!board.boxes().find((b) => b.node === picked()[0])?.code;
    /** One text selected, and it is prose. */
    const prose = () => one() && typeOf(picked()[0]) === "text" && !isCode();
    const pressing = (label: string, keys: string, key: string, init: KeyboardEventInit, group: Group, applies = any): Command => ({
      label,
      keys,
      group,
      where: ["node", "layer"],
      applies,
      run: () => press(key, init),
    });
    const arranging = (label: string, how: Arrange, least: number): Command => ({
      label,
      where: ["node"],
      applies: () => picked().length >= least,
      run: () => board.arrange(how),
    });
    return [
      ...annotationCommands(),
      { label: "Type in it", keys: "Enter", group: "Type", where: ["node"], applies: () => one() && typeOf(picked()[0]) === "text", run: () => press("Enter") },
      // A list's marks, the writing tools, and the diagram makers are prose's, not code's (PLAN 2.139).
      { label: "Bulleted list", keys: `${MOD}${SHIFT}8`, group: "Type", where: ["node"], applies: () => prose(), run: () => board.list("bullet") },
      { label: "Numbered list", keys: `${MOD}${SHIFT}7`, group: "Type", where: ["node"], applies: () => prose(), run: () => board.list("number") },
      // A text made code, made words again, or tidied (PLAN 2.139): each one patch.
      { label: "Make Code", group: "Type", where: ["node"], applies: () => prose(), run: () => board.coded(picked()[0], "code") },
      { label: "Not Code", group: "Type", where: ["node"], applies: () => isCode(), run: () => board.coded(picked()[0], "prose") },
      { label: "Tidy Code", group: "Type", where: ["node"], applies: () => isCode(), run: () => board.coded(picked()[0], "tidy") },
      // A text's words, by the model the assistant uses, offered to take or leave (PLAN 2.101).
      ...(["proofread", "tighten", "rewrite"] as const).map(
        (kind): Command => ({
          label: TOOLS[kind],
          where: ["node"],
          applies: () => prose(),
          run: () => write(kind, picked()[0]),
        }),
      ),
      // A picture's description, from the picture itself, by the model the assistant uses (PLAN 2.119).
      {
        label: TOOLS.describe,
        where: ["node"],
        applies: () => one() && typeOf(picked()[0]) === "image",
        run: () => write("describe", picked()[0]),
      },
      // A picture made by the user's image model in the image's place (PLAN 2.107), unless a lock
      // holds it (PLAN 2.95), which no picture replaces.
      // A map in the image's place (PLAN O.1c), unless a lock holds it.
      {
        label: "Put a map in it…",
        where: ["node"],
        applies: () => one() && typeOf(picked()[0]) === "image" && !board.lockedBy(picked()[0]),
        run: () => void mapping.open(),
      },
      {
        label: "Make a picture to replace it…",
        where: ["node"],
        applies: () => one() && typeOf(picked()[0]) === "image" && !board.lockedBy(picked()[0]),
        run: () => void picturing.open(),
      },
      // A lock (PLAN 2.95): the canvas passes over what is locked, and moves and deletes nothing of it.
      {
        label: any() && picked().every((n) => board.lockedBy(n) === n) ? "Unlock" : "Lock",
        keys: `${MOD}${SHIFT}L`,
        group: "Edit",
        where: ["node", "layer"],
        applies: any,
        run: () => board.lock(picked()),
      },
      // In another of the deck's formats, a node placed there anew (ADR-0020): from then on a
      // move there moves it there alone.
      { label: "Place anew in this format", where: ["node"], applies: () => one() && !!format() && !anewIn(picked()[0]), run: () => placeAnew(picked()[0]) },
      // The theme's colors from the photo an image draws (PLAN 2.94), as the Theme tab gives them.
      { label: "Theme colors from this photo", where: ["node"], applies: () => one() && typeOf(picked()[0]) === "image", run: () => colorsFrom(picked()[0]) },
      pressing("Duplicate", `${MOD}D`, "d", mod(), "Edit"),
      { label: "Copy", keys: `${MOD}C`, group: "Edit", where: ["node", "layer"], applies: any, run: () => clip("copy") },
      { label: "Cut", keys: `${MOD}X`, group: "Edit", where: ["node", "layer"], applies: any, run: () => clip("cut") },
      // A code text's words as text, for an editor or a terminal (PLAN 2.133).
      { label: "Copy Code", where: ["node"], applies: () => isCode(), run: () => copyCode(picked()[0]) },
      // A look copied and pasted (PLAN 2.58): Option makes ⌥C a character, so the key goes by its place.
      pressing("Copy the look", `${MOD_ALT}C`, "c", mod({ altKey: true, code: "KeyC" }), "Edit", one),
      pressing("Paste the look", `${MOD_ALT}V`, "v", mod({ altKey: true, code: "KeyV" }), "Edit", () => any() && board.copiedLook() !== undefined),
      // Two objects joined by a connector that follows them (PLAN 2.109): from the first selected to the second.
      { label: "Connect with an arrow", group: "Arrange", where: ["node"], applies: () => picked().length === 2, run: () => board.connect(picked(), true) },
      { label: "Connect with a line", group: "Arrange", where: ["node"], applies: () => picked().length === 2, run: () => board.connect(picked(), false) },
      // A text's lines drawn as a diagram in its place (PLAN 2.110): steps joined by arrows, steps
      // around a ring, or a tree by their list levels.
      ...(["process", "cycle", "hierarchy"] as const).map((kind): Command => ({
        label: `Make a ${kind} diagram`,
        group: "Draw and insert",
        where: ["node"],
        applies: () => prose(),
        run: () => board.diagram(picked()[0], kind),
      })),
      pressing("Group", `${MOD}G`, "g", mod(), "Arrange"),
      pressing("Ungroup", `${MOD}${SHIFT}G`, "g", mod({ shiftKey: true }), "Arrange", () => one() && typeOf(picked()[0]) === "group"),
      pressing("Bring forward", `${MOD}]`, "]", mod({ code: "BracketRight" }), "Arrange"),
      pressing("Send backward", `${MOD}[`, "[", mod({ code: "BracketLeft" }), "Arrange"),
      pressing("Bring to front", `${MOD}${SHIFT}]`, "}", mod({ code: "BracketRight", shiftKey: true }), "Arrange"),
      pressing("Send to back", `${MOD}${SHIFT}[`, "{", mod({ code: "BracketLeft", shiftKey: true }), "Arrange"),
      // Aligning takes two, spreading three, as in the inspector (PLAN 2.42).
      arranging("Align left", { align: "left" }, 2),
      arranging("Align center", { align: "center" }, 2),
      arranging("Align right", { align: "right" }, 2),
      arranging("Align top", { align: "top" }, 2),
      arranging("Align middle", { align: "middle" }, 2),
      arranging("Align bottom", { align: "bottom" }, 2),
      arranging("Spread across", { spread: "across" }, 3),
      arranging("Spread down", { spread: "down" }, 3),
      {
        label: "Select what holds it",
        keys: "Escape",
        group: "Select",
        where: ["node"],
        applies: () => one() && board.boxes().some((b) => b.node === picked()[0] && b.parent !== null),
        run: () => press("Escape"),
      },
      pressing("Delete", "Delete", "Delete", {}, "Edit"),
      pressing("Delete from every slide", `${SHIFT}Delete`, "Delete", { shiftKey: true }, "Edit"),
    ];
  }

  /** What annotates the chart selected (PLAN 2.67): the mark picked, highlighted, called out, ruled
   * at its value, or banded from; the annotation selected, reworded or taken away. Each is one
   * `annotate` patch, kept to the state shown where the inspector's "Only on Slide N" is. */
  function annotationCommands(): Command[] {
    const mark = () => board.picked();
    const notes = () => mark()?.notes;
    const name = mark() ? markName(mark()!) : "the mark";
    const noted = board.noted();
    const said = noted ? `${noteName(noted)}` : "the annotation";
    const run = (how: Parameters<typeof board.annotate>[0]) => () => void board.annotate(how);
    return [
      { label: `Highlight ${name}`, where: ["node"], applies: () => notes() !== undefined, run: run("highlight") },
      {
        label: `Highlight ${notes()?.series ?? "its series"}`,
        where: ["node"],
        applies: () => notes()?.axes === true && (notes()?.series ?? null) !== null,
        run: run("series"),
      },
      { label: `Take the highlight off ${name}`, where: ["node"], applies: () => (notes()?.highlighted.length ?? 0) > 0, run: run("unhighlight") },
      { label: `Call out ${name}…`, where: ["node"], applies: () => notes()?.axes === true, run: run("callout") },
      { label: `Rule ${name}'s value`, where: ["node"], applies: () => notes()?.axes === true, run: run("rule") },
      { label: `Band from ${name} to…`, where: ["node"], applies: () => notes()?.axes === true, run: run("band") },
      { label: `Change what ${said} says…`, where: ["node"], applies: () => board.noted() !== undefined, run: () => void board.reword() },
      { label: `Take ${said} away`, keys: "Delete", group: "Annotate", where: ["node"], applies: () => board.noted() !== undefined, run: () => void board.unnote() },
    ];
  }

  /** What is done to `node` in the layers: shown or hidden on the slide shown. */
  function layerCommands(node: () => string | undefined, hidden: () => boolean): Command[] {
    return [
      {
        label: hidden() ? "Show it on this slide" : "Hide it on this slide",
        where: ["layer"],
        applies: () => node() !== undefined && showing() !== undefined,
        run: () => layering.toggle(node()!),
      },
    ];
  }

  /** What is done on the canvas where nothing is: what goes there, and how close it is shown. In a
   * menu, Insert opens the palette on what may be inserted; the palette lists each. */
  function canvasCommands(inMenu: boolean): Command[] {
    const inserts: Command[] = inMenu
      ? [{ label: "Insert…", where: ["canvas"], applies: () => ready() && insertable.length > 0, run: () => commanding.open("insert ") }]
      : insertable.map((label, n) => ({ label: `Insert ${label}`, applies: ready, run: () => board.insert(n, label.split(" · ").at(-1)) }));
    const icon: Command = { label: "Insert an icon…", group: "Draw and insert", where: ["canvas"], applies: ready, run: () => void insertIcon() };
    return [
      { label: "Paste", keys: `${MOD}V`, group: "Edit", where: ["canvas"], applies: ready, run: pasted },
      // Words that look like code pasted as words (PLAN 2.133).
      { label: "Paste as Text", where: ["canvas"], applies: ready, run: pastedAsText },
      ...inserts,
      icon,
      // A picture from the disk (the QA pass's J3), as one dropped lands.
      { label: "Insert an image…", where: ["canvas"], group: "Draw and insert", applies: () => ready(), run: pickImage },
      // A map (ADR-0030, PLAN O.1c): in the slide's empty place for a picture, else where the
      // pointer last pressed; for an image selected, the command that puts one in it.
      {
        label: "Insert a map…",
        where: ["canvas"],
        group: "Draw and insert",
        applies: () => ready(),
        run: () => void mapping.open(),
      },
      // A picture made by the user's image model (PLAN 2.107): in the slide's empty place for one,
      // else where the pointer last pressed; for an image selected, unless a lock holds it, the
      // command that replaces it.
      {
        label: "Make a picture…",
        where: ["canvas"],
        applies: () => ready() && !(picked().length === 1 && typeOf(picked()[0]) === "image" && !board.lockedBy(picked()[0])),
        run: () => void picturing.open(),
      },
      ...DRAWING.filter(({ key }) => key !== "p").map(
        ({ key, label }): Command => ({ label, keys: key.toUpperCase(), group: "Draw and insert", where: ["canvas"], applies: () => ready() && board.armed()?.key !== key, run: () => press(key) }),
      ),
      // The pen (PLAN 2.114): each drag a stroke, the strokes one drawing, until it is put down.
      { label: board.pen() ? "Put the pen down" : "Draw with the pen", keys: "P", group: "Draw and insert", where: ["canvas"], applies: ready, run: () => press("p") },
      { label: "Zoom in", keys: `${MOD}+`, group: "See", where: ["canvas"], applies: () => board.zoomed() < 8, run: () => board.zoom("in") },
      { label: "Zoom out", keys: `${MOD}−`, group: "See", where: ["canvas"], applies: () => board.zoomed() > 1, run: () => board.zoom("out") },
      { label: "Zoom to fit", keys: `${MOD}0`, group: "See", where: ["canvas"], applies: () => board.zoomed() > 1, run: () => board.zoom("fit") },
      { label: board.ruled() ? "Hide the grid" : "Show the grid", keys: `${MOD}'`, group: "See", where: ["canvas"], run: () => board.rule() },
      {
        label: board.shielded() ? "Hide the safe area" : "Show the safe area",
        keys: `${MOD_ALT}'`,
        group: "See",
        where: ["canvas"],
        run: () => board.shield(),
      },
    ];
  }

  /** What is done to the state shown, as the strip does it, and which state is shown. */
  function stateCommands(): Command[] {
    const now = () => showing()?.state;
    const count = () => last?.states.length ?? 0;
    /** The state shown is a step after its slide's first. */
    const step = () => slideOf(now() ?? "").includes(", step ");
    return [
      { label: "Add a step", where: ["state"], applies: () => now() !== undefined, run: () => states.add("step") },
      { label: "Add a slide…", where: ["state"], applies: () => now() !== undefined, run: () => void states.gallery() },
      { label: step() ? "Delete this step" : "Delete this slide", where: ["state"], applies: () => now() !== undefined && count() > 1, run: () => states.remove(now()!) },
      { label: "Duplicate this slide", where: ["state"], applies: () => now() !== undefined, run: () => states.duplicate(now()!) },
      { label: "Export it as a PNG…", where: ["state"], applies: () => now() !== undefined, run: () => openExport("png") },
      // Its notes, drafted from how it reads by the model the assistant uses (PLAN 2.101).
      { label: "Draft its notes", where: ["state"], applies: () => now() !== undefined, run: () => write("notes") },
      // Whether its words carry its beat's claim, judged by the same model, which offers the
      // headline rewritten to carry it (narrative FUNC-4).
      { label: "Check its claim", where: ["state"], applies: () => now() !== undefined, run: () => write("claim") },
      // Written again, or a new one after it, as the deck's generator writes slides, by the same
      // model (lift-up FUNC-1).
      { label: "Write this slide again…", where: ["state"], applies: () => now() !== undefined && !assisting, run: () => asking.open("redo") },
      { label: "Add a slide about…", where: ["state"], applies: () => now() !== undefined && !assisting, run: () => asking.open("add") },
      { label: "Show the light table", applies: () => now() !== undefined && !table.open(), run: () => table.toggle(true) },
      { label: "Close the light table", applies: () => table.open(), run: () => table.toggle(false) },
      // Page Down and Page Up on the slide (PLAN 2.126).
      { label: "Show the next slide or step", keys: "PageDown", group: "Slides", applies: () => last?.valid === true && shown < count() - 1, run: () => goTo(shown + 1) },
      { label: "Show the slide or step before", keys: "PageUp", group: "Slides", applies: () => last?.valid === true && shown > 0, run: () => goTo(shown - 1) },
      { label: "Go to the slide", keys: `${GO}C`, group: "The editor", run: toSlide },
      { label: "Go to the slides and their steps", keys: `${GO}F`, group: "The editor", run: toStrip },
    ];
  }

  /** What is done to the deck and the bundle, and where the page looks. */
  function deckCommands(): Command[] {
    const rethemes = themeChoices.filter((c) => c.value && !c.current);
    return [
      // Each named by the change it takes back or makes again, where the last change said what
      // it was (lift-up USE-4).
      { label: lastDone ? `Undo ${lastDone}` : "Undo", keys: `${MOD}Z`, group: "Edit", applies: () => undoDepth(view.state) > 0 && !assisting, run: () => undo(view) && forceLinting(view) },
      { label: lastUndone ? `Redo ${lastUndone}` : "Redo", keys: `${MOD}${SHIFT}Z`, group: "Edit", applies: () => redoDepth(view.state) > 0 && !assisting, run: () => redo(view) && forceLinting(view) },
      { label: "Find in the deck's texts", keys: `${MOD}F`, group: "Find and replace", run: () => finding?.open() },
      ...rethemes.map((c): Command => ({ label: `Re-theme in ${c.label}`, applies: () => showing() !== undefined, run: () => void retheme(c.value) })),
      { label: "Save", keys: `${MOD}S`, group: "The editor", run: () => save().catch(failed) },
      { label: "Show or fold the source", keys: `${MOD}\\`, group: "The editor", run: () => fold(folded()) },
      { label: "Save as…", applies: () => !$("#save-as").hidden, run: () => $("#save-as").click() },
      { label: "Download .scaena", run: () => download().catch(failed) },
      { label: "Export this slide as a PNG…", applies: () => showing() !== undefined, run: () => openExport("png") },
      { label: "Export every slide as PNGs…", applies: () => showing() !== undefined, run: () => openExport("slides") },
      { label: "Export the deck as a PDF", applies: () => last?.valid === true, run: () => exportAs("pdf").catch(failed) },
      { label: "Export the deck as one HTML file", applies: () => last?.valid === true, run: () => exportAs("html").catch(failed) },
      { label: "Play", applies: () => !play.hidden, run: () => play.click() },
      { label: "New deck…", run: () => $("#new-deck").click() },
      { label: "New deck from a prompt…", run: () => $("#new-prompt").click() },
      { label: "Open a .scaena file…", run: () => $("#open-file").click() },
      { label: "Open a folder…", applies: () => !$("#open-folder").hidden, run: () => $("#open-folder").click() },
      { label: "Decks kept in this browser…", run: () => $("#kept").click() },
      { label: "Show Format", run: () => tab("inspector").focus() },
      { label: "Show Animate", run: () => tab("animate").focus() },
      { label: "Show Document", run: () => tab("document").focus() },
      { label: "Hide the inspector", applies: () => !hidden("panel"), run: () => hiding("panel", true) },
      { label: hidden("cue") ? "Show the animations" : "Hide the animations", run: () => hiding("cue", !hidden("cue")) },
      { label: "Show the layers", run: () => tab("layers").focus() },
      { label: "Show the data", run: () => tab("data").focus() },
      { label: "Edit the theme", run: () => tab("theming").focus() },
      {
        label: formatting.open() ? "Hide the formats side by side" : "Show every format side by side",
        run: () => formatting.show(!formatting.open()),
      },
      { label: "Rehearse the deck", applies: () => last?.valid === true, run: () => void rehearsing.start() },
      // The last rehearsal's results again, to keep (lift-up USE-2).
      { label: "Rehearsal Results…", applies: () => rehearsing.kept().length > 0 && !rehearsing.running(), run: () => rehearsing.results() },
      { label: "Edit the layout", applies: () => showing() !== undefined, run: () => void board.layoutMode(true) },
      { label: "New layout from this one…", applies: () => showing() !== undefined, run: () => void board.newLayout() },
      { label: "Show the deck's files", run: () => tab("files").focus() },
      { label: "Show the versions", run: () => tab("versions").focus() },
      { label: "Show the deck's story", run: () => tab("story").focus() },
      { label: "Show the assistant", run: () => (tab("assistant"), $("#question").focus()) },
      { label: "Show the keys", keys: "?", group: "The editor", run: () => keying.open() },
    ];
  }

  /** Words for the assistant, asked from the palette: the Assistant tab shows them asked, with what
   * is selected (PLAN 2.52), or, where it has no key yet, in its question box, the key's field
   * focused. */
  function ask(words: string) {
    tab("assistant");
    const question = $<HTMLTextAreaElement>("#question");
    question.value = words;
    question.focus();
    void assistant.ask(words).then((answer) => {
      const key = $<HTMLInputElement>("#key");
      if (answer.kind === "failed" && !key.value) key.focus();
    });
  }

  /** Every command the editor has, as the palette lists them. */
  const commandsNow = () => {
    const node = () => (picked().length === 1 ? picked()[0] : undefined);
    return [...nodeCommands(), ...layerCommands(node, () => false), ...canvasCommands(false), ...stateCommands(), ...deckCommands()];
  };
  const commanding = palette($<HTMLDialogElement>("#palette"), commandsNow, ask);
  $("#commands-open").onclick = () => commanding.open();
  /** The keys (PLAN 2.65): every key the editor answers, by what it does, from the commands' own
   * list and from what each part of the page declares it answers. */
  const keying = sheet($<HTMLDialogElement>("#keys"), commandsNow, () => [
    ...canvasKeys(),
    ...typingKeys(),
    ...stripKeys(),
    ...slidesKeys(),
    ...layerKeys(),
    ...cueKeys(),
    ...findKeys(),
    ...dataKeys(),
    ...rehearsalKeys(),
    ...sourceKeys(),
    ...editorKeys(),
  ]);
  $("#keys-open").onclick = () => keying.open();
  /** Where a menu opens for `e`: at the pointer, or, from the keyboard, under `el`. */
  const near = (e: MouseEvent, el: Element): [number, number] => {
    const r = el.getBoundingClientRect();
    const inside = e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
    return inside ? [e.clientX, e.clientY] : [r.left, r.bottom];
  };
  // A right click on a state in the strip shows it, then offers what is done to it.
  heed($("#strip ol"), "contextmenu", (e) => {
    const li = (e.target as Element).closest<HTMLElement>("li[data-state]");
    if (!li) return;
    e.preventDefault();
    const index = last?.states.findIndex(([id]) => id === li.dataset.state) ?? -1;
    if (index >= 0 && index !== shown) goTo(index);
    const [x, y] = near(e, li);
    menu(x, y, stateCommands().filter((c) => c.where?.includes("state")), li);
  });
  // A right click on a layer selects its node where it is shown, then offers what is done to it.
  heed($("#layers ul"), "contextmenu", (e) => {
    const li = (e.target as Element).closest<HTMLElement>("li[data-layer]");
    if (!li) return;
    e.preventDefault();
    const node = li.dataset.layer!;
    const hidden = li.classList.contains("hidden");
    if (!hidden && !board.chosen().includes(node)) board.select(node);
    const items = [...layerCommands(() => node, () => hidden), ...(hidden ? [] : nodeCommands().filter((c) => c.where?.includes("layer")))];
    const [x, y] = near(e, li.querySelector(".row") ?? li);
    menu(x, y, items, li.querySelector<HTMLElement>("[data-pick]"));
  });

  /** Served (PLAN 2.11): what changed on disk comes in, when nothing here is changed and not
   * saved; otherwise it is offered. */
  let hearing: EventSource | undefined;
  if (served && "url" in source) {
    const again = $<HTMLButtonElement>("#again");
    /** The bundle as it is on disk now, and its source in the editor, which is then as saved. */
    const take = async (paths: string[]) => {
      again.hidden = true;
      if (paths.some((path) => path !== "deck.scn")) await stage.reload();
      const text = scnOnDisk ? await fromDisk() : await stage.source();
      view.dispatch({ changes: change(view.state.doc.toString(), text) });
      savedSource = view.state.doc.toString();
      filedSaved = filed;
      tell();
      status.textContent = `Read again from disk: ${paths.join(", ")}`;
    };
    const changed = (paths: string[]) => {
      if (!dirty()) return void take(paths).catch(failed);
      again.textContent = "Changed on disk: take it";
      again.hidden = false;
      again.onclick = () => {
        if (confirm("Take the deck as it is on disk, and lose the changes here not saved?")) void take(["deck.json"]).catch(failed);
      };
    };
    hearing = listen({ changed, broke: () => scnOnDisk && changed(["deck.scn"]) });
  }

  $<HTMLButtonElement>("#save").onclick = () => void save().catch(failed);
  // Save as: a name in the browser's storage, or a folder on disk where the browser can open one.
  const savingAs = $<HTMLDialogElement>("#saving-as");
  const asName = $<HTMLInputElement>("#saving-as-name");
  const asFolder = $<HTMLButtonElement>("#saving-as-folder");
  asFolder.hidden = !window.showDirectoryPicker;
  $<HTMLButtonElement>("#save-as").onclick = () => {
    asName.value = name;
    savingAs.returnValue = "";
    savingAs.showModal();
    asName.select();
  };
  savingAs.onclose = () => {
    if (savingAs.returnValue === "opfs") void saveAs({ opfs: asName.value.trim() || name }).catch(failed);
  };
  asFolder.onclick = async () => {
    savingAs.close();
    const folder = await window.showDirectoryPicker!({ id: "scaena", mode: "readwrite" }).catch(() => undefined);
    if (!folder) return;
    for await (const _ of folder.entries()) {
      if (!confirm(`${folder.name} is not empty. Save the bundle into it, over any files of the same names?`)) return;
      break;
    }
    await saveAs({ folder }).catch(failed);
  };
  $<HTMLButtonElement>("#download").onclick = () => void download().catch(failed);
  // Export…: the state shown as a PNG at a width asked, every slide so in one zip (PLAN 2.122), or
  // the deck as a PDF or one HTML file.
  const exporting = $<HTMLDialogElement>("#exporting");
  const exportWidth = $<HTMLInputElement>("#exporting-width");
  const exportNote = $("#exporting-note");
  type ExportAs = "png" | "slides" | "pdf" | "html";
  const chosenExport = () => exporting.querySelector<HTMLInputElement>("input[name=as]:checked")!.value as ExportAs;
  /** What the export chosen makes, said under the choices. */
  const noteExport = () => {
    const as = chosenExport();
    const sized = as === "png" || as === "slides";
    $("#exporting-size").hidden = !sized;
    exportWidth.disabled = !sized;
    const [w, h] = board.size();
    const width = Number(exportWidth.value);
    const pixels = `${width} × ${Math.round((h * width) / w)} pixels`;
    const now = showing()?.state;
    // A slide is counted where its first state is: its steps share its number.
    const slides = last?.states.filter(([state]) => !slideOf(state).includes(", step ")).length ?? 0;
    const whole = Number.isInteger(width) && width > 0;
    exportNote.textContent = {
      // The slide as a person counts it (PLAN 2.99), as the file is named.
      png: whole
        ? `${now === undefined ? "The slide shown" : slideOf(now)}${format() ? ` in ${format()}` : ""}: ${pixels}.`
        : "A width in whole pixels: the height keeps the canvas's aspect.",
      slides: whole
        ? `${slides} slide${slides === 1 ? "" : "s"}, each as it stands once its steps have played${format() ? `, in ${format()}` : ""}: a PNG of ${pixels} named by its number, all in one .zip.`
        : "A width in whole pixels: the height keeps the canvas's aspect.",
      pdf: "A page for each slide at its last state: vector paths, text in subset fonts that copies and searches as the deck reads, tagged for a screen reader.",
      html: "The player and the deck in one file that plays in a browser with no network, its fonts subset to what the deck draws.",
    }[as];
  };
  exporting.oninput = noteExport;
  /** Open Export…, `as` chosen. */
  const openExport = (as: ExportAs = "png") => {
    exporting.querySelector<HTMLInputElement>(`input[name=as][value=${as}]`)!.checked = true;
    exportWidth.value = String(Math.round(board.size()[0]));
    noteExport();
    exporting.returnValue = "";
    exporting.showModal();
    (as === "png" || as === "slides" ? exportWidth : $<HTMLButtonElement>("#exporting-go")).focus();
  };
  $<HTMLButtonElement>("#export").onclick = () => openExport();
  exporting.onclose = () => {
    if (exporting.returnValue !== "export") return;
    const as = chosenExport();
    void exportAs(as, as === "png" || as === "slides" ? Number(exportWidth.value) : undefined).catch(failed);
  };
  onkeydown = (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void save().catch(failed);
    }
    // ⌘\\: the source beside the canvas, or folded away (PLAN 2.80).
    if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && e.key === "\\") {
      e.preventDefault();
      fold(folded());
    }
    // ⌘K: every command, by name (PLAN 2.53); again, it closes.
    if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "k" && !e.defaultPrevented) {
      e.preventDefault();
      if ($<HTMLDialogElement>("#palette").open) commanding.close();
      else commanding.open();
    }
    // ?: the keys (PLAN 2.65), outside what takes typing, and over no other dialog; again, it closes.
    if (e.key === "?" && !e.metaKey && !e.ctrlKey && !e.altKey && !e.defaultPrevented && !typingIn(e.target)) {
      const over = document.querySelector("dialog[open]");
      if (!over || over.id === "keys") {
        e.preventDefault();
        keying.toggle();
      }
    }
    // The canvas's zoom (PLAN 2.46) wherever the focus is, never the page's: the canvas, and a text
    // typed in on it, take these keys first.
    if ((e.metaKey || e.ctrlKey) && !e.altKey && ["=", "+", "-", "_", "0"].includes(e.key) && !e.defaultPrevented) {
      e.preventDefault();
      void board.zoom(e.key === "0" ? "fit" : e.key === "-" || e.key === "_" ? "out" : "in");
    }
    // Page Down and Page Up on the slide show the next slide or step and the one before (PLAN
    // 2.126); a text typed in keeps them.
    if ((e.key === "PageDown" || e.key === "PageUp") && !e.metaKey && !e.ctrlKey && !e.altKey && !e.defaultPrevented && e.target === $("#overlay")) {
      e.preventDefault();
      const to = shown + (e.key === "PageDown" ? 1 : -1);
      // A source that does not compile shows no other slide, as the palette's commands have it.
      if (!last?.valid) return;
      if (to >= 0 && to < last.states.length) {
        goTo(to);
        say(`${slideOf(last.states[to][0])} shown`);
      } else say(e.key === "PageDown" ? "This is the last slide" : "This is the first slide");
    }
    // ⌥⇧⌘C puts the focus on the slide, and ⌥⇧⌘F among the slides (PLAN 2.126), over no dialog;
    // by the keys' places, as Option makes ⌥C a character of its own. AltGr, which Windows gives
    // as Ctrl and Alt, types a character with them (AltGr+Shift+C is Ć on a Polish keyboard): it
    // stays the character.
    const altGr = e.getModifierState("AltGraph");
    if ((e.metaKey || e.ctrlKey) && e.altKey && e.shiftKey && !altGr && (e.code === "KeyC" || e.code === "KeyF") && !document.querySelector("dialog[open]")) {
      e.preventDefault();
      if (e.code === "KeyC") toSlide();
      else toStrip();
    }
    // ⌘F finds in the deck's texts (PLAN 2.47); in the source, the browser's find stays.
    if ((e.metaKey || e.ctrlKey) && !e.altKey && e.key.toLowerCase() === "f" && !(e.target instanceof Element && e.target.closest(".cm-editor"))) {
      e.preventDefault();
      finding?.open();
    }
  };
  onbeforeunload = (e) => {
    // Changes not saved, among them what is typed and not yet made, and a field in the inspector
    // typed in and not yet left.
    const field = document.activeElement;
    const unsettled =
      board.unsettled() || ((field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement) && $("#look").contains(field) && field.value !== field.defaultValue);
    // And a deck being made from a prompt, whose calls are paid for and lost with the page (lift-up
    // 10-25 REL-4).
    if (unsaved() || unsettled || generating?.running()) e.preventDefault();
  };
  current = {
    close: () => {
      clearTimeout(pending);
      if (copyTimer) void keepCopy();
      hearing?.close();
      board.close();
      view.destroy();
      stage.close();
      // What it listened for on the page goes with it: the bundle opened next hears each key once.
      ended();
    },
    unsaved,
    say,
    story: async () => (await stage.health(view.state.doc.toString())).score,
    models: (provider, key, base) => stage.models(provider, key, base),
    kept: () => (where?.kind === "opfs" ? where.name : undefined),
    // New's deck opens typing in its title, its words selected (PLAN 3.21): whether it does.
    typeTitle: async () => {
      const node = /^\s*(\S+) text\b[^\n]*\bat:in\(title\)/m.exec(view.state.doc.toString())?.[1];
      if (node) await board.typeOver(node).catch(() => {});
      return node !== undefined && board.typing() === node;
    },
    // The deck open here, renamed where the browser keeps it, is kept under its new name: the
    // address and the bar follow.
    rekeep: async (from, to) => {
      const kept = await stage.rekeep(from, to);
      if (kept && where?.kind === "opfs" && where.name === from && kept.name !== from) {
        void discard(copyKey());
        where = kept;
        name = kept.name;
        address(where);
        tell();
      }
    },
  };

  /** The player's address, on the slide shown (lift-up FUNC-2): the link's, as `tell` set it where
   * the deck is kept, and the state shown by its id. */
  const playingFrom = () => {
    const url = new URL(play.href);
    const state = showing()?.state ?? last?.states[shown]?.[0];
    if (state === undefined) url.searchParams.delete("state");
    else url.searchParams.set("state", state);
    return url.href;
  };
  // Play opens the player on the slide shown, on the deck as the editor has it: the player plays
  // the deck as saved, so one with changes not saved is saved first, if the person says so, and
  // the player's window, opened in the click, goes to it once it is. Declined, the link plays the
  // deck as last saved. A new deck plays once it is kept: Play keeps it in this browser first, as
  // its first save does, with nothing asked.
  play.onclick = (e) => {
    play.href = playingFrom();
    const fresh = unkept();
    if (!fresh && !dirty()) return;
    if (!fresh && !confirm("The deck has changes not saved, which the player would not play. Save it first?")) return;
    e.preventDefault();
    const opened = window.open("about:blank", "_blank");
    void save().then(
      (done) => {
        if (!done) return void opened?.close();
        if (fresh) say("Kept in this browser to play it");
        if (opened) opened.location.href = playingFrom();
        else window.open(playingFrom(), "_blank");
      },
      (error: unknown) => {
        opened?.close();
        failed(error);
      },
    );
  };

  // A state picked by its id, not its place among the options: an edit not yet compiled, such as
  // an undo of + Step, may have moved it, and the edit's answer shows it where the edit put it.
  statesPicker.onchange = () => {
    const id = statesPicker.value;
    const index = last?.states.findIndex(([state]) => state === id) ?? -1;
    if (index < 0) return void (statesPicker.selectedIndex = shown);
    const start = last?.states[index]?.[1];
    if (start !== undefined) view.dispatch({ selection: { anchor: start }, scrollIntoView: true });
    void show(index);
  };
  formatPicker.onchange = () => {
    void show(shown);
    states.reformat();
    table.reformat();
    formatting.shown();
    // Which findings hold in the format shown is lint's to say again (PLAN 2.49): CodeMirror lints
    // again only once the source changes.
    taken = undefined;
    const doc = view.state.doc;
    void lint(view).then((found) => view.state.doc === doc && view.dispatch(setDiagnostics(view.state, found)));
  };

  // For tests and the console.
  Object.assign(window, {
    scaena: {
      opened: stage.opened,
      painter: stage.opened.painter,
      /** Open a bundle: `{ opfs }`, `{ folder }` (any directory handle), or `{ zip, name }`. */
      open: (source: Source) => open(source),
      save,
      /** Save as: `{ opfs: name }`, or `{ folder }` (any directory handle). */
      saveAs,
      download,
      /** Export (PLAN 2.54): `png` (the state shown, `width` pixels wide), `slides` (every slide so,
       * in one zip, PLAN 2.122), `pdf`, or `html`, as the Export dialog does; the file's name and
       * bytes, also downloaded. */
      exportAs,
      /** Drop a file named `name` at `at` in the source (the cursor by default). */
      drop: (name: string, bytes: ArrayBuffer, at?: number) => drop([new File([bytes], name)], at ?? view.state.selection.main.head),
      where: () => ({ name, where, dirty: dirty() }),
      /** Panic in the engine, for a test (lift-up STAB-2), and how many times it was started again. */
      crash: () => stage.trap(),
      /** The worker dies outside a message, or fails and is busy `busy` ms, for a test (lift-up
       * 10-24b STAB-2). */
      die: (busy?: number) => stage.die(busy),
      restarts: () => restarts,
      /** The recovery copy kept now, for a test (lift-up REL-4). */
      keepCopy,
      copyKey,
      source: () => view.state.doc.toString(),
      /** Put `text` in the editor, as typing would: the cursor ends where the change does,
       * and the next lint sees it. */
      type: (text: string) => {
        const c = change(view.state.doc.toString(), text);
        view.dispatch({ changes: c, selection: { anchor: c.from + c.insert.length }, userEvent: "input.type" });
      },
      /** Put the cursor at `offset`. */
      cursor: (offset: number) => view.dispatch({ selection: { anchor: offset } }),
      /** Where the cursor is, an offset in the source. */
      head: () => view.state.selection.main.head,
      /** Apply the fix of the first finding with `code`. */
      fix: async (code: string) => {
        const f = last?.findings.find((g) => g.code === code && g.fix);
        if (!f) throw new Error(`no ${code} with a fix`);
        await fix(f);
      },
      last: () => last,
      trips: () => trips,
      wholes: () => wholes,
      shown: () => shown,
      inspector: () => inspector.textContent,
      /** How a state reads (PLAN 2.8), the state shown by default, as HTML: a table's cells as
       * they print. */
      reading: (state?: string) => stage.reading(state ?? last?.states[shown]?.[0] ?? stage.opened.states[shown], format()),
      at: () => stage.at,
      /** The assistant: ask it something, and read the conversation. */
      assistant,
      /** The writing tools (PLAN 2.101): one asked, and its answer taken, settled. */
      writing,
      /** Write This Slide Again… and Add a Slide About… (lift-up FUNC-1): the dialog opened, and the last
       * slide written, settled. */
      slideAsk: asking,
      /** Make a picture (PLAN 2.107): the dialog opened, and the last picture made and put, settled. */
      picturing,
      /** Insert › Map… (PLAN O.1c): the sheet opened, and the last map fetched and put, settled. */
      mapping,
      /** The canvas: what it selected, what stands where, and its drag. */
      canvas: board,
      /** The layers panel: what it lists, and the patches it makes. */
      layers: layering,
      /** The data panel (PLAN 2.55): the source shown and its sheet. */
      data,
      /** The files panel (PLAN 2.59): the bundle's files as it lists them, and its changes. */
      files: filing,
      /** The story panel (PLAN G.9): the deck's narrative health as it last read it. */
      story,
      /** The versions panel (PLAN 2.60): the versions as it lists them, one shown, compared, and
       * restored. */
      versions: versioning,
      /** The theme panel (PLAN 2.61): the theme as it shows it, and an edit made there. */
      theme: theming,
      /** The Insert menu (PLAN 2.34): each item it offers now, under its kind. */
      inserts: () => insertItems.map(({ section, label }) => ({ section, label })),
      /** The theme dialog (PLAN 2.39): each theme it offers, by family, the deck's own marked. */
      themeChoices: () => themeChoices,
      /** The formats side by side (PLAN 2.62): shown or hidden, and what they last painted. */
      formats: formatting,
      /** The rehearsal (PLAN 2.63): started, stopped, and what each state took. */
      rehearsal: rehearsing,
      /** The inspector's edits: what it offers for the node selected, and a choice made there. */
      look,
      /** The state strip: its states, thumbnails, and the patches it makes. */
      strip: states,
      /** The light table (PLAN 2.97): its slides, those selected, and the patches it makes. */
      slides: table,
      /** The cue of the state shown: its bars, and the patches its drags make. */
      cue: cueing,
      /** Find and replace: what it finds, and the match shown. */
      find: finding,
      /** Commands by name (PLAN 2.53): the palette, and what it lists. */
      commands: commanding,
      /** The keys (PLAN 2.65): the sheet and what it lists, and each command's key and group. */
      keys: {
        ...keying,
        commands: () => commandsNow().flatMap((c) => (c.keys ? [{ label: c.label, keys: c.keys, group: c.group }] : [])),
      },
    },
  });
}

/** The inspector's tabs (lift-up ATTR-4), a presentation app's on one row (PLAN 3.18): Format, Animate,
 * and Document, a segmented control, and More, a menu of the rest (the layers, the assistant), which
 * shows the panel chosen as its own tab, named for it. Document's panels, Theme, Data, Files,
 * Versions, and Story, are a row of their own. In each row the arrow keys, Home, and End move between
 * the tabs shown, and only the one chosen is in the tab order; on More, ↓ opens its menu, as a click
 * does. A tab kept hidden under More, clicked (`tab()`, a test), shows its panel as the menu does,
 * and one of Document's shows Document first. */
function tabs() {
  const more = $<HTMLButtonElement>("#tab-more");
  /** One row of tabs: `pick` shows one's panel and hides the others'. `shown` are those a person
   * sees, which the arrow keys move between. */
  function row(list: HTMLElement, picked?: (button: HTMLButtonElement) => void) {
    const buttons = [...list.querySelectorAll<HTMLButtonElement>('[role="tab"]')].filter((b) => b !== more);
    const shown = () => [...list.querySelectorAll<HTMLButtonElement>('[role="tab"]')].filter((b) => !b.hidden);
    const pick = (button: HTMLButtonElement) => {
      for (const other of buttons) {
        const on = other === button;
        other.setAttribute("aria-selected", String(on));
        other.tabIndex = on ? 0 : -1;
        $(`#${other.getAttribute("aria-controls")}`).hidden = !on;
      }
      picked?.(button);
    };
    const keys = (button: HTMLButtonElement) => (e: KeyboardEvent) => {
      const ring = shown();
      const i = ring.indexOf(button);
      const to = { ArrowRight: i + 1, ArrowLeft: i - 1, Home: 0, End: ring.length - 1 }[e.key];
      if (to === undefined) return;
      e.preventDefault();
      const next = ring[(to + ring.length) % ring.length];
      if (next === more) {
        // More stands for no panel until one is chosen from it: the focus goes to it alone.
        next.focus();
        return;
      }
      pick(next);
      next.focus();
    };
    for (const button of buttons) {
      button.tabIndex = button.getAttribute("aria-selected") === "true" ? 0 : -1;
      button.onclick = () => pick(button);
      button.onkeydown = keys(button);
    }
    return { pick, keys };
  }
  /** Document's panel chosen is hidden and shown with Document: each panel reads its own `hidden`
   * to know whether it is seen, asks the worker nothing while it is not, and reads again as it is
   * shown. Kept in step from the start: a panel that thought itself seen would ask the worker for
   * the source on every edit, and a burst of edits could then compile an older source over the
   * deck the last one made. */
  const documentShown = () => {
    const chosen = $("#document .segments [aria-selected=\"true\"]").getAttribute("aria-controls");
    for (const panel of document.querySelectorAll<HTMLElement>('#document > [role="tabpanel"]')) panel.hidden = panel.id !== chosen || $("#document").hidden;
  };
  const top = row($("#tabs"), (button) => {
    // A panel chosen from More shows as More's own: named for it, selected, controlling it.
    const under = button.hidden;
    more.setAttribute("aria-selected", String(under));
    more.tabIndex = under ? 0 : -1;
    more.textContent = under ? button.textContent : "More";
    if (under) more.setAttribute("aria-controls", button.getAttribute("aria-controls")!);
    documentShown();
  });
  const under = [...document.querySelectorAll<HTMLButtonElement>('#tabs [role="tab"][hidden]')];
  more.tabIndex = -1;
  more.onclick = () => {
    const at = more.getBoundingClientRect();
    menu(at.left, at.bottom + 2, under.map((b) => ({ label: b.textContent ?? "", run: () => b.click() })), more);
  };
  const moreKeys = top.keys(more);
  more.onkeydown = (e) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      more.click();
    } else moreKeys(e);
  };
  // Document's row: one of its tabs shows Document too, so a test or a command reaches each by its id.
  row($("#document .segments"), () => {
    if ($("#document").hidden) top.pick($<HTMLButtonElement>("#tab-document"));
  });
  documentShown();
}

tabs();
controls()
  .then(first)
  .then(edit)
  .catch(failed);

offline();
