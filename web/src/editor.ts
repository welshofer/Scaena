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
import { defaultKeymap, history, historyKeymap, indentWithTab, isolateHistory, redo, redoDepth, undo, undoDepth } from "@codemirror/commands";
import { syntaxHighlighting } from "@codemirror/language";
import { type Diagnostic, forceLinting, lintGutter, lintKeymap, linter, setDiagnostics } from "@codemirror/lint";
import { Compartment, EditorState } from "@codemirror/state";
import {
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from "@codemirror/view";
import { themes } from "@scaena/themes";
import { panel } from "./assistant/panel";
import { sourceOf } from "./bundle";
import { canvas, placed } from "./canvas";
import { type Command, MOD, MOD_ALT, menu, palette, SHIFT } from "./commands";
import { cue } from "./cue";
import { sheets } from "./data";
import { filesPanel } from "./files";
import { finder } from "./find";
import { keptNames } from "./folders";
import { layers } from "./layers";
import { looks } from "./look";
import type { Arrange, Edited, Export, Finding, FromWorker, Inspected, Linted, Painter, SaveTo, Seeing, Source, Where } from "./protocol";
import { scn, scnHighlight } from "./scn";
import { client, listen, served, status as onDisk } from "./served";
import { worker } from "./spawn";
import { Stage } from "./stage";
import { strip } from "./strip";
import type { Selected } from "./typing";

const params = new URLSearchParams(location.search);
const painter = (params.get("painter") ?? "auto") as Painter;
/** The bundle the editor opens when the address names none. */
const fallback = import.meta.env.VITE_BUNDLE ?? "../../docs/examples/revenue.deck.json";

const $ = <T extends HTMLElement>(selector: string) => document.querySelector<T>(selector)!;
const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
const html = (text: string) => text.replace(/[&<>"]/g, (c) => `&${{ "&": "amp", "<": "lt", ">": "gt", '"': "quot" }[c]};`);

/** The one change that turns `from` into `to`: what lies between their common start and
 * their common end. A fix rewrites the deck as canonical source, so most of it is unchanged. */
function change(from: string, to: string): { from: number; to: number; insert: string } {
  let start = 0;
  while (start < from.length && start < to.length && from[start] === to[start]) start++;
  let end = 0;
  while (end < from.length - start && end < to.length - start && from[from.length - 1 - end] === to[to.length - 1 - end]) end++;
  return { from: start, to: from.length - end, insert: to.slice(start, to.length - end) };
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

/** What the editor has open, to close when it opens another bundle. */
let current: { close: () => void; dirty: () => boolean } | undefined;

/** Open the bundle at `source`, closing what is open. */
async function open(source: Source) {
  if (current?.dirty() && !confirm("The bundle has changes not saved. Open another and lose them?")) return;
  current?.close();
  current = undefined;
  $("#status").textContent = "opening…";
  await edit(source);
}

/** The page's address names where the bundle is kept, so a reload opens it there. A served
 * bundle's address names it already. */
function address(where: Where | undefined) {
  if (!where || where.kind === "serve") return;
  const url = new URL(location.href);
  url.searchParams.set("bundle", `${where.kind}:${where.name}`);
  window.history.replaceState(null, "", url);
}

/** The bundles the browser keeps, in the picker that opens them: listed again after each save
 * and each open, which can add one. */
async function listKept() {
  const names = await keptNames().catch(() => []);
  $<HTMLSelectElement>("#kept").replaceChildren(new Option("Kept in this browser…", ""), ...names.map((n) => new Option(n, n)));
}

/** The bundle controls: open a folder, a `.scaena` file, or a bundle the browser keeps. */
async function controls() {
  const kept = $<HTMLSelectElement>("#kept");
  await listKept();
  kept.onchange = () => {
    if (kept.value) void open({ opfs: kept.value }).catch(failed);
    kept.value = "";
  };
  const folder = $<HTMLButtonElement>("#open-folder");
  folder.hidden = !window.showDirectoryPicker;
  folder.onclick = async () => {
    const handle = await window.showDirectoryPicker!({ id: "scaena", mode: "readwrite" }).catch(() => undefined);
    if (!handle) return;
    await folders("readwrite", (store) => store.put(handle, handle.name));
    await open({ folder: handle }).catch(failed);
  };
  const file = $<HTMLInputElement>("#open-file");
  file.onchange = async () => {
    const zip = file.files?.[0];
    file.value = "";
    if (zip) await open({ zip: await zip.arrayBuffer(), name: zip.name.replace(/\.scaena$/, "") }).catch(failed);
  };
  // New (PLAN 2.12): a title and a theme that ships.
  const making = $<HTMLDialogElement>("#making");
  const title = $<HTMLInputElement>("#making-name");
  const theme = $<HTMLSelectElement>("#making-theme");
  theme.replaceChildren(...Object.keys(themes).map((t) => new Option(t, t)));
  $<HTMLButtonElement>("#new-deck").onclick = () => {
    making.returnValue = "";
    making.showModal();
    title.select();
  };
  making.onclose = () => {
    if (making.returnValue !== "create") return;
    void open({ create: { theme: theme.value, title: title.value.trim() || "Untitled" } }).catch(failed);
  };
}

/** The bundle `?bundle=` names. A folder kept from before asks again for leave to write,
 * which takes a click. */
async function first(): Promise<Source> {
  const bundle = params.get("bundle");
  if (!bundle?.startsWith("folder:")) {
    const source = sourceOf(bundle, fallback);
    return served && "url" in source ? { ...source, serve: client } : source;
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
  $("#status").textContent = `error: ${said(e)}`;
  console.error(e);
};

async function edit(source: Source) {
  const status = $("#status");
  // A status too long for its two lines says the rest in its title.
  new MutationObserver(() => (status.title = status.textContent ?? "")).observe(status, { childList: true, characterData: true, subtree: true });
  const statesPicker = $<HTMLSelectElement>("#state");
  const formatPicker = $<HTMLSelectElement>("#format");
  const problems = $<HTMLUListElement>("#problems");
  const inspector = $("#inspector");
  const listing = $("#listing");
  const whereLine = $("#where");
  const play = $<HTMLAnchorElement>("#play");
  const stage = await Stage.open($<HTMLCanvasElement>("#stage"), source, painter, worker);
  stage.onError = failed;
  /** On a served page, the deck's source on disk, where the folder keeps one (PLAN 2.11). */
  const scnOnDisk = "url" in source && source.serve && (await onDisk()).source ? new URL("deck.scn", source.url) : undefined;
  const fromDisk = async () => (await fetch(scnOnDisk!, { cache: "no-store" })).text();
  formatPicker.replaceChildren(new Option("own canvas", ""));
  for (const format of stage.opened.formats) formatPicker.add(new Option(format, format));
  const format = () => formatPicker.value || undefined;
  /** The bundle's name, and where it is kept. */
  let { name, where } = stage.opened;
  /** Changes typed, fixed, or dropped, and how many of them the last save holds. */
  let edits = 0;
  let saved = 0;
  const dirty = () => edits !== saved;
  /** A change the editor makes itself, which is not an edit to save. */
  let quiet = false;
  const tell = () => {
    const kinds = {
      folder: `folder ${where?.name}`,
      opfs: `kept in this browser as ${where?.name}`,
      serve: `folder ${where?.name}, served${scnOnDisk ? ", from its deck.scn" : ""}`,
    };
    const kept = where ? kinds[where.kind] : `${name}, not saved`;
    whereLine.textContent = `${kept}${dirty() ? " · changed" : ""}`;
    // The player opens a bundle by its address or from the browser's storage, not a folder.
    const playing =
      where?.kind === "serve"
        ? new URLSearchParams({ bundle: params.get("bundle") ?? "/bundle/", serve: "" })
        : new URLSearchParams({ bundle: where ? `${where.kind}:${where.name}` : (params.get("bundle") ?? fallback) });
    if (params.has("painter")) playing.set("painter", painter);
    play.href = `index.html?${playing}`;
    // Nor a new deck, until it is saved.
    play.hidden = where?.kind === "folder" || (!where && "create" in source);
    // A served bundle saves to its folder alone.
    $<HTMLButtonElement>("#save-as").hidden = where?.kind === "serve";
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
  /** The source's version: one more with each change. A lint answers the version it read. */
  let version = 0;
  /** A lint of every state, waiting for typing to stop. */
  let pending: ReturnType<typeof setTimeout> | undefined;
  const pause = 500;
  /** Whether the source takes typing: not while the assistant works. */
  const locked = new Compartment();
  let assisting = false;
  /** The source the assistant's last edit, or the canvas's, made, and what the worker found in
   * it: the lint of that source takes it as it is, rather than asking the worker again. */
  let taken: { source: string; edited: Edited } | undefined;
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

  const view = new EditorView({
    parent: $("#code"),
    state: EditorState.create({
      doc: scnOnDisk ? await fromDisk() : await stage.source(),
      extensions: [
        lineNumbers(),
        highlightActiveLineGutter(),
        history(),
        drawSelection(),
        highlightActiveLine(),
        keymap.of([...defaultKeymap, ...historyKeymap, ...lintKeymap, indentWithTab]),
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
            // the cursor an undo puts back is in the state it was in.
            if (last) last = { ...last, states: last.states.map(([id, at]) => [id, update.changes.mapPos(at, 1)]) };
            if (!quiet) {
              edits++;
              tell();
            }
          }
          if (update.selectionSet) follow();
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

  /** What the last gesture on the canvas or choice in the inspector said: it stays in the status,
   * what lint finds after it, until the source is edited by hand. */
  let told: string | undefined;
  const say = (text: string) => {
    told = text;
    status.textContent = text;
  };
  /** The state shown, where a gesture on the canvas or a choice in the inspector makes a patch:
   * none while the source does not compile, nor while the assistant works. */
  const showing = () => {
    const state = last?.valid && !last.error ? last.states[shown]?.[0] : undefined;
    return state !== undefined && !assisting ? { state, index: shown } : undefined;
  };
  /** Take `source`, a patch made on the canvas or in the inspector: one change, one step to undo. */
  const made = (source: string, edited: Edited) => {
    taken = { source, edited };
    const changes = change(view.state.doc.toString(), source);
    view.dispatch({ changes, userEvent: "input.canvas", annotations: isolateHistory.of("full") });
  };
  /** The node selected, its look chosen from the theme in the inspector (PLAN 2.33): each choice a
   * patch, as each gesture on the canvas is. */
  const look = looks(stage, $("#look"), {
    shown: showing,
    format,
    source: () => view.state.doc.toString(),
    apply: made,
    say,
    style: (given) => board.style(given),
    arrange: (how) => board.arrange(how, false),
    group: () => board.group(),
    ungroup: () => board.ungroup(),
    pick: () => board.pick(),
    select: (node) => board.select(node),
  });

  /** The cue of the state shown (PLAN 2.44), under the preview: once the canvas is there. */
  let cueing: ReturnType<typeof cue> | undefined;
  /** Find and replace across the deck's texts (PLAN 2.47), once the canvas is there. */
  let finding: ReturnType<typeof finder> | undefined;
  /** The preview as a canvas (PLAN 2.31): each gesture a patch, which comes into the source as
   * one change, one step to undo. It waits while the source does not compile, and while the
   * assistant works. */
  const board = canvas(stage, $("#overlay"), {
    shown: showing,
    format,
    source: () => view.state.doc.toString(),
    version: () => version,
    at: (node) => inspected?.nodes[node]?.at as Record<string, unknown> | undefined,
    transform: (node) => inspected?.nodes[node]?.transform as { rotate?: number; anchor?: [number, number] } | undefined,
    apply: made,
    typed: (source, edited, joins) => {
      taken = { source, edited };
      const changes = change(view.state.doc.toString(), source);
      // A burst of typing is one step to undo. CodeMirror's history joins a change to the one
      // before it when both are `input.type.compose`; the burst's first starts a step of its own.
      view.dispatch(
        joins
          ? { changes, userEvent: "input.type.compose" }
          : { changes, userEvent: "input.type", annotations: isolateHistory.of("before") },
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
    keeps: (to) => to instanceof Node && ($("#look").contains(to) || $("#assistant").contains(to)),
    zoomed: (zoom) => {
      $("#zoom output").textContent = `${Math.round(zoom * 100)}%`;
    },
    ruled: (on) => $("#grid").setAttribute("aria-pressed", String(on)),
    // A finding's mark on the canvas (PLAN 2.49): its fix taken, or where the source writes it.
    fix: (f) => fix(f),
    go: (f) => go(f),
    // A finding no fix can make, about what the inspector sets (PLAN 2.56): its field in focus.
    edit: (f, prop) => {
      tab("inspector");
      if (f.node !== undefined) look.focus(f.node, prop);
    },
    // A right click, or the menu key (PLAN 2.53): what is done to the nodes selected, or on the
    // canvas where nothing is.
    menu: (x, y, on) =>
      void menu(x, y, on === "node" ? nodeCommands().filter((c) => c.where?.includes("node")) : canvasCommands(true), $("#overlay")),
  }, $("#marks"), $("#marked"));
  /** The layers of the state shown (PLAN 2.50): a tab beside the inspector, each change a patch. */
  const layering = layers(stage, $("#layers"), {
    shown: showing,
    format,
    source: () => view.state.doc.toString(),
    select: (node) => board.select(node),
    apply: made,
    say,
  });
  /** A data file written, the source as it stands (PLAN 2.55): its deck shown and linted again, as
   * `edited` says, and a change to save. */
  const wrote = (edited: Edited) => {
    const source = view.state.doc.toString();
    taken = { source, edited };
    edits++;
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
  });
  /** The bundle's files (PLAN 2.59): its images, fonts, and data, what uses each in which states,
   * and those nothing names taken out, each one change the panel's undo takes back. */
  const filing = filesPanel(stage, $("#files"), {
    shown: showing,
    format,
    source: () => view.state.doc.toString(),
    took: wrote,
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
      if (n < 0) return say(`${path} is not an image the deck can insert: a PNG in the bundle`);
      await board.insert(n, path);
    },
    say,
  });
  // The zoom's buttons (PLAN 2.46), as ⌘−, ⌘+, and ⌘0.
  $("#zoom").onclick = (e) => {
    const how = (e.target as Element).closest<HTMLElement>("[data-zoom]")?.dataset.zoom;
    if (how === "in" || how === "out" || how === "fit") void board.zoom(how);
  };
  // The theme's grid over the canvas (PLAN 2.57), as ⌘' draws it.
  $("#grid").onclick = () => void board.rule();
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
  /** Find and replace across the deck's texts, in every state (PLAN 2.47): a bar above the
   * preview. Each match shows its state, its node selected and its characters marked; each
   * replacement is a patch, written where each text lives. */
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
  });
  $("#find-open").onclick = () => finding?.open();
  /** The state strip (PLAN 2.35): the deck's states, each a thumbnail, and the patches that add,
   * move, rename, and remove them. */
  const states = strip(stage, $("#strip"), {
    shown: showing,
    format,
    source: () => view.state.doc.toString(),
    show: (index) => {
      const start = last?.states[index]?.[1];
      if (start !== undefined) view.dispatch({ selection: { anchor: start }, scrollIntoView: true });
      void show(index);
    },
    apply: made,
    say,
  });
  /** What may be inserted (PLAN 2.34), grouped by type, as the deck offers it: asked again after
   * each edit, since the theme and the bundle's images change. */
  const inserter = $<HTMLSelectElement>("#insert");
  let offered = "";
  /** What may be inserted, by label, as the deck last offered it: what the palette lists. */
  let insertable: string[] = [];
  async function offer() {
    const inserts = await stage.inserts().catch(() => undefined);
    const key = JSON.stringify(inserts?.map((i) => i.label));
    if (!inserts || key === offered) return;
    offered = key;
    insertable = inserts.map((i) => i.label);
    const groups = new Map<string, HTMLOptGroupElement>();
    for (const [n, insert] of inserts.entries()) {
      const [kind, name] = insert.label.includes(" · ") ? insert.label.split(" · ", 2) : [insert.node.type, insert.label];
      const group = groups.get(kind) ?? Object.assign(document.createElement("optgroup"), { label: kind });
      groups.set(kind, group);
      group.append(new Option(name, String(n)));
    }
    inserter.replaceChildren(new Option("Insert…", ""), ...groups.values());
  }
  inserter.onchange = () => {
    const chosen = inserter.selectedOptions[0];
    inserter.value = "";
    if (!chosen?.value) return;
    void board.insert(Number(chosen.value), chosen.textContent ?? undefined);
    $("#overlay").focus();
  };
  /** The deck's theme (PLAN 2.39): the theme files the bundle holds, the one it names chosen, and
   * the themes that ship. One chosen re-themes the deck, as `scaena theme --apply` does: refused,
   * with why, where the deck would not validate in it; else one change, one step to undo, and
   * the status says what lint finds in it that it did not before. */
  const themePicker = $<HTMLSelectElement>("#theme");
  let themesOffered = "";
  async function offerThemes() {
    const held = await stage.themes().catch(() => undefined);
    if (!held) return;
    const key = JSON.stringify([held, Object.keys(themes)]);
    if (key === themesOffered) return;
    themesOffered = key;
    const named = (path: string) => path.replace(/^.*\//, "").replace(/\.theme\.json$|\.json$/, "");
    const inBundle = Object.assign(document.createElement("optgroup"), { label: "In the bundle" });
    for (const file of held.files) inBundle.append(new Option(named(file), `path:${file}`, file === held.current, file === held.current));
    if (held.current === "(inline)") inBundle.append(new Option("(inline)", "", true, true));
    const ships = Object.assign(document.createElement("optgroup"), { label: "Ships" });
    for (const name of Object.keys(themes)) ships.append(new Option(name, `ships:${name}`));
    themePicker.replaceChildren(...[inBundle, ships].filter((g) => g.children.length > 0));
  }
  themePicker.onchange = async () => {
    const chosen = themePicker.value;
    // The theme the deck names: the picker goes back to it at once where the choice is not made.
    const named = [...themePicker.options].find((o) => o.defaultSelected)?.value ?? "";
    const now = showing();
    const [kind, name] = [chosen.slice(0, chosen.indexOf(":")), chosen.slice(chosen.indexOf(":") + 1)];
    if (!now || !name) {
      themePicker.value = named;
      return say("not re-themed while the source does not compile or the assistant works");
    }
    const label = kind === "ships" ? name : name.replace(/^.*\//, "");
    say(`re-theming in ${label}…`);
    try {
      const theme = kind === "ships" ? { ships: name } : { path: name };
      const { themed, source, edited } = await stage.retheme(view.state.doc.toString(), theme, now.index, format());
      if (themed.refused) {
        themePicker.value = named;
        const errors = themed.added.filter((f) => f.severity === "error");
        const more = errors.length > 1 ? ` (and ${errors.length - 1} more)` : "";
        say(`${label} refused: the deck would not validate in it: ${errors[0]?.message ?? ""}${more}`);
      } else if (source && edited) {
        made(source, edited);
        const [worse, better] = [themed.added.length, themed.removed.length];
        say(`theme ${label} · lint finds ${worse} new, ${better} gone`);
      }
    } catch (e) {
      themePicker.value = named;
      say(`not re-themed: ${said(e)}`);
    } finally {
      themesOffered = "";
      void offerThemes();
    }
  };
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
    let edited: Edited;
    if (taken?.source !== source) told = undefined;
    try {
      if (taken?.source === source) edited = taken.edited;
      else {
        const before = taken;
        edited = await stage.edit(source, shown, format());
        // The worker holds this source's deck now, not the one the patch left: an undo, then a
        // redo back to the patch's source, compiles it again. A patch taken meanwhile stays.
        if (taken === before) taken = undefined;
      }
    } catch (e) {
      status.textContent = `error: ${said(e)}`;
      return [];
    }
    trips.push({ ms: performance.now() - sent, ...edited.ms });
    return take(edited, source, read).map((f) => diagnostic(f, view.state.doc.length));
  }

  /** Show `edited`, what the worker made of `source`, version `read` of it: the deck shown and
   * linted, its findings listed, and every state linted once typing stops. The findings, to mark
   * in the source. */
  function take(edited: Edited, source: string, read: number): Finding[] {
    last = edited;
    if (edited.valid) {
      statesPicker.replaceChildren(...edited.states.map(([id]) => new Option(id, id)));
      if (edited.at) shown = edited.at.index;
      statesPicker.selectedIndex = shown;
      states.states(edited.slots, shown);
      void inspect();
      void layering.refresh();
      void data.refresh();
      void filing.refresh();
      void board.refresh().then(() => select(source), failed);
      finding?.changed();
      void offer();
      void offerThemes();
    }
    const findings = edited.error ? [edited.error] : edited.findings;
    report(findings, edited);
    if (!edited.whole) {
      clearTimeout(pending);
      pending = setTimeout(() => void lintAll(read), pause);
    }
    return findings;
  }

  /** Lint every state of the deck compiled from version `read`, and show what it finds if
   * the source is still that version. */
  async function lintAll(read: number) {
    if (read !== version) return;
    let linted: Linted;
    try {
      linted = await stage.lint();
    } catch (e) {
      status.textContent = `error: ${said(e)}`;
      return;
    }
    wholes.push(linted);
    if (read !== version || !last) return;
    last = { ...last, findings: linted.findings, laid: linted.laid, whole: true };
    report(linted.findings, last, linted.ms);
    // Every state is laid out: the strip's thumbnails that changed are painted again.
    void states.paint();
    view.dispatch(setDiagnostics(view.state, linted.findings.map((f) => diagnostic(f, view.state.doc.length))));
  }

  /** The findings under the source, and the status line: what was found, and how long each
   * step took; `whole`, how long the lint of every state took. */
  function report(findings: Finding[], edited: Edited, whole?: number) {
    list(findings);
    // Those about each state stand on the canvas and count in the strip (PLAN 2.49).
    board.found(findings);
    states.found(findings);
    if (edited.error) {
      status.textContent = "does not compile";
      return;
    }
    const errors = findings.filter((f) => f.severity === "error").length;
    const ms = (n: number) => `${n.toFixed(0)} ms`;
    const linted =
      whole !== undefined
        ? `every state linted in ${ms(whole)}`
        : `${edited.whole ? "linted" : "its state linted"} in ${ms(edited.ms.lint)}`;
    const found = `${errors} error${errors === 1 ? "" : "s"}, ${findings.length - errors} other · compiled in ${ms(edited.ms.compile)}, shown in ${ms(edited.ms.paint)}, ${linted}`;
    status.textContent = told ? `${told} — ${found}` : found;
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
      actions: f.fix ? [{ name: "Fix", apply: () => void fix(f).catch((e) => say(`not fixed: ${said(e)}`)) }] : [],
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

  /** Where the source writes what `f` is about, the cursor there. */
  function go(f: Finding) {
    if (!f.at) return;
    view.dispatch({ selection: { anchor: f.at.from }, scrollIntoView: true });
    view.focus();
  }

  /** Every finding under the source; one that stands in it is a button, which goes there. */
  function list(findings: Finding[]) {
    problems.replaceChildren(
      ...findings.map((f) => {
        const li = document.createElement("li");
        li.className = f.severity;
        const where = f.at ? `${f.at.line}:${f.at.col}` : (f.file ?? "");
        const said = `<span class="code">${f.code}</span>${html(f.message)}<span class="where">${html(
          [f.state, f.format, where].filter(Boolean).join(" · "),
        )}</span>`;
        if (!f.at) li.innerHTML = said;
        else {
          const button = li.appendChild(document.createElement("button"));
          button.innerHTML = said;
          button.onclick = () => go(f);
        }
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

  /** Show the state the cursor is in: the last whose declaration starts at or before it. */
  function follow() {
    if (!last?.valid) return;
    const at = view.state.selection.main.head;
    let index = -1;
    last.states.forEach(([, start], i) => {
      if (start <= at) index = i;
    });
    if (index < 0 || index === shown) return;
    show(index);
  }

  async function show(index: number) {
    shown = index;
    statesPicker.selectedIndex = index;
    states.select(index);
    await stage.seek(index, undefined, format());
    await inspect();
    void layering.refresh();
    await board.refresh();
  }

  /** The inspector: the state's cue, then each node, its look if it sets text, and how
   * many props its overrides set. */
  async function inspect() {
    const state = last?.states[shown]?.[0] ?? stage.opened.states[shown];
    if (!state) return;
    let found: Inspected;
    try {
      found = await stage.inspect(state, format());
    } catch (e) {
      listing.textContent = said(e);
      return;
    }
    inspected = found;
    cueing?.show(found);
    const cue = found.timeline;
    const rows = Object.keys(found.nodes).map((id) => {
      const look = found.looks?.[id];
      const overrides = found.overrides?.[id]?.length ?? 0;
      const text = look
        ? `<span class="swatch" style="background:${look.hex}"></span>${html(look.role)} · ${html(look.family)} ${look.size}/${look.weight} · ${html(look.color)}`
        : "";
      // A placement by `rect` on the template's grid is an override, as lint says (W301): once for
      // each place a `rect` is written, under the first state that shows it.
      const at = found.nodes[id].at as Record<string, unknown> | undefined;
      const off = at?.rect !== undefined && last?.findings.some((f) => f.code === "W301" && f.node === id);
      const place = at ? `${html(placed(at))}${off ? ' <span class="flag" title="W301: placed by a rect, an override">override</span>' : ""}` : "";
      return `<tr data-node="${html(id)}"${id === chosen || beside.includes(id) ? ' aria-selected="true"' : ""}><td>${html(id)}</td><td>${place}</td><td>${text}</td><td>${overrides || ""}</td></tr>`;
    });
    const motions = (cue?.motions ?? []).map(
      (m) => `<tr><td>${html(m.node)}</td><td>${html(m.motion)}${m.units > 1 ? ` × ${m.units}` : ""}</td><td>${m.start.toFixed(0)}–${m.end.toFixed(0)} ms</td></tr>`,
    );
    void look.show(chosen, beside);
    listing.innerHTML = `
      <h2>${html(found.state_id)}${found.layout ? ` · ${html(found.layout)}` : ""}</h2>
      ${cue ? `<p>starts at ${cue.start.toFixed(0)} ms · cue ${cue.span.toFixed(0)} ms · holds ${cue.hold.toFixed(0)} ms · transition ${cue.transition.duration.toFixed(0)} ms, matched by ${html(cue.transition.match)}</p>` : ""}
      ${motions.length ? `<table><tr><th>moves</th><th></th><th></th></tr>${motions.join("")}</table>` : ""}
      <h2>Nodes</h2>
      <table><tr><th>node</th><th>place</th><th>look</th><th>overrides</th></tr>${rows.join("")}</table>`;
  }

  /** Save the source as it stands where the bundle is kept, or into the browser's storage. A
   * source that does not compile is not saved, and the status says why. The paths the save
   * renamed are renamed in the source too, and nothing else in it changes. */
  async function save() {
    const at = edits;
    status.textContent = "saving…";
    let done: Extract<FromWorker, { type: "saved" }>;
    try {
      done = await stage.save(view.state.doc.toString());
    } catch (e) {
      status.textContent = `not saved: ${said(e)}`;
      return;
    }
    return took(done, at);
  }

  /** Save as (PLAN 2.12): the source as it stands, saved `to` a place of its own, where the
   * bundle is kept from then on: a folder on disk, kept by name as an opened one is, or the
   * browser's storage under a name (`name-2`, … where that is taken). */
  async function saveAs(to: SaveTo) {
    if (where?.kind === "serve") throw new Error("a served bundle saves to the folder scaena serve serves it from");
    const at = edits;
    status.textContent = "saving…";
    let done: Extract<FromWorker, { type: "saved" }>;
    try {
      done = await stage.saveAs(view.state.doc.toString(), to);
    } catch (e) {
      status.textContent = `not saved: ${said(e)}`;
      return;
    }
    if ("folder" in to) await folders("readwrite", (store) => store.put(to.folder, to.folder.name));
    return took(done, at);
  }

  /** A save `done` of the source as it stood at `at` edits: the bundle kept where it went, the
   * paths the save renamed renamed in the source, and the page's address naming it. */
  async function took(done: Extract<FromWorker, { type: "saved" }>, at: number) {
    where = done.where;
    name = done.where.name;
    quiet = true;
    view.dispatch({ changes: change(view.state.doc.toString(), renamedIn(view.state.doc.toString(), done.renamed)) });
    quiet = false;
    // The folder keeps the deck's source too: it is what the editor shows, renamed as the save
    // renamed files.
    if (scnOnDisk) {
      const headers = { "X-Scaena-Client": client };
      const response = await fetch(scnOnDisk, { method: "PUT", body: view.state.doc.toString(), headers });
      if (!response.ok) throw new Error(`deck.scn: ${(await response.text()).trim() || response.status}`);
    }
    saved = at;
    address(where);
    tell();
    void listKept();
    const named = done.renamed.length ? `, ${done.renamed.length} named by their content` : "";
    status.textContent = `saved ${done.files} files${named}${done.recorded ? ", and recorded in its history" : ""}`;
    return done;
  }

  /** Download the source as it stands as a `.scaena` zip, its fonts subset to what the deck
   * draws. */
  async function download() {
    let zipped: { bytes: ArrayBuffer; subset: [string, number, number][] };
    try {
      zipped = await stage.zip(view.state.doc.toString());
    } catch (e) {
      status.textContent = `not downloaded: ${said(e)}`;
      return;
    }
    const url = URL.createObjectURL(new Blob([zipped.bytes], { type: "application/zip" }));
    Object.assign(document.createElement("a"), { href: url, download: `${name}.scaena` }).click();
    setTimeout(() => URL.revokeObjectURL(url), 60_000);
    const [before, after] = zipped.subset.reduce(([b, a], [, x, y]) => [b + x, a + y], [0, 0]);
    const kb = (n: number) => `${Math.round(n / 1024)} KB`;
    status.textContent = `downloaded ${name}.scaena, ${kb(zipped.bytes.byteLength)}: fonts subset from ${kb(before)} to ${kb(after)}`;
    return zipped;
  }

  /** Export (PLAN 2.54) the deck the source compiles to, as `scaena export` writes it, and
   * download it: `png`, the state shown at rest in the format shown, `width` pixels wide; `pdf`,
   * each slide at its last state; `html`, the deck as one file that plays offline, the
   * single-file player's page beside this one filled in. Each is made in the worker: nothing is
   * sent anywhere. */
  async function exportAs(as: "png" | "pdf" | "html", width?: number): Promise<{ file: string; bytes: ArrayBuffer } | undefined> {
    let what: Export;
    let file: string;
    if (as === "png") {
      const now = showing();
      if (!now) {
        say("not exported: no state is shown");
        return;
      }
      const shownIn = format();
      what = { kind: "png", state: now.state, width: width ?? board.size()[0], format: shownIn };
      file = `${now.state}${shownIn ? `-${shownIn.replace(/[^\w.-]+/g, "x")}` : ""}.png`;
    } else if (as === "pdf") {
      what = { kind: "pdf" };
      file = `${name}.pdf`;
    } else {
      const page = await fetch(new URL("standalone.html", document.baseURI)).catch(() => undefined);
      if (!page?.ok) {
        say("not exported: this build of the editor carries no single-file page (build it with `just web`)");
        return;
      }
      what = { kind: "html", page: await page.text(), name };
      file = `${name}.html`;
    }
    say(`exporting ${file}…`);
    let bytes: ArrayBuffer;
    try {
      bytes = await stage.export(view.state.doc.toString(), what);
    } catch (e) {
      say(`not exported: ${said(e)}`);
      return;
    }
    const type = { png: "image/png", pdf: "application/pdf", html: "text/html" }[as];
    const url = URL.createObjectURL(new Blob([bytes], { type }));
    Object.assign(document.createElement("a"), { href: url, download: file }).click();
    setTimeout(() => URL.revokeObjectURL(url), 60_000);
    say(`downloaded ${file}, ${Math.max(1, Math.round(bytes.byteLength / 1024))} KB`);
    return { file, bytes };
  }

  /** Files dropped at `at` in the source: each joins the bundle, and its path, quoted, goes
   * there. A `.scaena` file opens instead. */
  async function drop(files: File[], at: number) {
    const zip = files.find((f) => f.name.endsWith(".scaena"));
    if (zip) return open({ zip: await zip.arrayBuffer(), name: zip.name.replace(/\.scaena$/, "") });
    const paths: string[] = [];
    for (const file of files) paths.push(await stage.drop(file.name, await file.arrayBuffer()));
    const insert = paths.map((p) => JSON.stringify(p)).join(" ");
    view.dispatch({ changes: { from: at, insert }, selection: { anchor: at + insert.length } });
    view.focus();
    status.textContent = `added ${paths.join(", ")}`;
    return paths;
  }

  const assistant = panel(stage, {
    source: () => view.state.doc.toString(),
    apply: (source, edited, touched) => {
      touching = touched?.length ? { source, nodes: touched } : undefined;
      // An edit that leaves the source as it is wrote a data file (`data_edit`).
      if (source === view.state.doc.toString()) return wrote(edited);
      taken = { source, edited };
      view.dispatch({ changes: change(view.state.doc.toString(), source), userEvent: "input.assistant" });
    },
    seeing,
    lock: (on) => {
      assisting = on;
      view.dispatch({ effects: locked.reconfigure(on ? [EditorState.readOnly.of(true), EditorView.editable.of(false)] : []) });
    },
  });

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
    if (!document.execCommand(how)) say(`not ${how === "copy" ? "copied" : "cut"}: the browser keeps its clipboard from this page; ${MOD}${how === "copy" ? "C" : "X"} does it`);
  };
  /** The clipboard's text, pasted on the canvas as ⌘V pastes it. */
  const pasted = async () => {
    overlay.focus();
    const text = await navigator.clipboard.readText().catch((e) => void say(`not pasted: ${said(e)}; ${MOD}V pastes`));
    if (text === undefined) return;
    if (!text) return say("the clipboard holds nothing to paste");
    await board.paste(text);
  };
  /** Whether the canvas takes a gesture: the source compiles, the assistant is not at work, and no
   * text is typed in. */
  const ready = () => showing() !== undefined && board.typing() === undefined;
  /** What the canvas has selected, where it takes a gesture. */
  const picked = () => (ready() ? board.chosen() : []);
  const typeOf = (node: string) => layering.type(node, showing()?.state ?? "");
  /** Show the tab `id` under the preview, as a click on it does. */
  const tab = (id: "inspector" | "layers" | "data" | "assistant") => {
    const button = $<HTMLButtonElement>(`#tab-${id}`);
    button.click();
    return button;
  };
  /** Show the state at `index`, its declaration at the cursor, as the strip does. */
  const goTo = (index: number) => {
    const start = last?.states[index]?.[1];
    if (start !== undefined) view.dispatch({ selection: { anchor: start }, scrollIntoView: true });
    void show(index);
  };

  /** What is done to the nodes selected: each its key on the canvas. */
  function nodeCommands(): Command[] {
    const any = () => picked().length > 0;
    const one = () => picked().length === 1;
    const pressing = (label: string, keys: string, key: string, init: KeyboardEventInit, applies = any): Command => ({
      label,
      keys,
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
      { label: "Type in it", keys: "Enter", where: ["node"], applies: () => one() && typeOf(picked()[0]) === "text", run: () => press("Enter") },
      pressing("Duplicate", `${MOD}D`, "d", mod()),
      { label: "Copy", keys: `${MOD}C`, where: ["node", "layer"], applies: any, run: () => clip("copy") },
      { label: "Cut", keys: `${MOD}X`, where: ["node", "layer"], applies: any, run: () => clip("cut") },
      // A look copied and pasted (PLAN 2.58): Option makes ⌥C a character, so the key goes by its place.
      pressing("Copy the look", `${MOD_ALT}C`, "c", mod({ altKey: true, code: "KeyC" }), one),
      pressing("Paste the look", `${MOD_ALT}V`, "v", mod({ altKey: true, code: "KeyV" }), () => any() && board.copiedLook() !== undefined),
      pressing("Group", `${MOD}G`, "g", mod()),
      pressing("Ungroup", `${MOD}${SHIFT}G`, "g", mod({ shiftKey: true }), () => one() && typeOf(picked()[0]) === "group"),
      pressing("Bring forward", `${MOD}]`, "]", mod({ code: "BracketRight" })),
      pressing("Send backward", `${MOD}[`, "[", mod({ code: "BracketLeft" })),
      pressing("Bring to front", `${MOD}${SHIFT}]`, "}", mod({ code: "BracketRight", shiftKey: true })),
      pressing("Send to back", `${MOD}${SHIFT}[`, "{", mod({ code: "BracketLeft", shiftKey: true })),
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
        where: ["node"],
        applies: () => one() && board.boxes().some((b) => b.node === picked()[0] && b.parent !== null),
        run: () => press("Escape"),
      },
      pressing("Delete", "Delete", "Delete", {}),
      pressing("Delete from every state", `${SHIFT}Delete`, "Delete", { shiftKey: true }),
    ];
  }

  /** What is done to `node` in the layers: shown or hidden in the state shown, and renamed. */
  function layerCommands(node: () => string | undefined, hidden: () => boolean): Command[] {
    return [
      {
        label: hidden() ? "Show it in this state" : "Hide it in this state",
        where: ["layer"],
        applies: () => node() !== undefined && showing() !== undefined,
        run: () => layering.toggle(node()!),
      },
      {
        label: "Rename it",
        keys: "F2",
        where: ["layer"],
        applies: () => node() !== undefined && showing() !== undefined,
        run: () => {
          tab("layers");
          layering.rename(node()!);
        },
      },
    ];
  }

  /** What is done on the canvas where nothing is: what goes there, and how close it is shown. In a
   * menu, Insert opens the palette on what may be inserted; the palette lists each. */
  function canvasCommands(inMenu: boolean): Command[] {
    const draws: [string, string][] = [
      ["t", "Draw a text"],
      ["r", "Draw a rectangle"],
      ["o", "Draw an ellipse"],
      ["l", "Draw a line"],
      ["a", "Draw an arrow"],
    ];
    const inserts: Command[] = inMenu
      ? [{ label: "Insert…", where: ["canvas"], applies: () => ready() && insertable.length > 0, run: () => commanding.open("insert ") }]
      : insertable.map((label, n) => ({ label: `Insert ${label}`, applies: ready, run: () => board.insert(n, label.split(" · ").at(-1)) }));
    return [
      { label: "Paste", keys: `${MOD}V`, where: ["canvas"], applies: ready, run: pasted },
      ...inserts,
      ...draws.map(([key, label]): Command => ({ label, keys: key.toUpperCase(), where: ["canvas"], applies: () => ready() && board.armed()?.key !== key, run: () => press(key) })),
      { label: "Zoom in", keys: `${MOD}+`, where: ["canvas"], applies: () => board.zoomed() < 8, run: () => board.zoom("in") },
      { label: "Zoom out", keys: `${MOD}−`, where: ["canvas"], applies: () => board.zoomed() > 1, run: () => board.zoom("out") },
      { label: "Zoom to fit", keys: `${MOD}0`, where: ["canvas"], applies: () => board.zoomed() > 1, run: () => board.zoom("fit") },
      { label: board.ruled() ? "Hide the grid" : "Show the grid", keys: `${MOD}'`, where: ["canvas"], run: () => board.rule() },
    ];
  }

  /** What is done to the state shown, as the strip does it, and which state is shown. */
  function stateCommands(): Command[] {
    const now = () => showing()?.state;
    const count = () => last?.states.length ?? 0;
    return [
      { label: "Add a step", where: ["state"], applies: () => now() !== undefined, run: () => states.add("step") },
      { label: "Add a slide", where: ["state"], applies: () => now() !== undefined, run: () => states.add("slide") },
      { label: "Rename the state", where: ["state"], applies: () => now() !== undefined, run: () => states.rename(now()!) },
      { label: "Delete the state", where: ["state"], applies: () => now() !== undefined && count() > 1, run: () => states.remove(now()!) },
      { label: "Export it as a PNG…", where: ["state"], applies: () => now() !== undefined, run: () => openExport("png") },
      { label: "Show the next state", applies: () => last?.valid === true && shown < count() - 1, run: () => goTo(shown + 1) },
      { label: "Show the state before", applies: () => last?.valid === true && shown > 0, run: () => goTo(shown - 1) },
    ];
  }

  /** What is done to the deck and the bundle, and where the page looks. */
  function deckCommands(): Command[] {
    const rethemes = [...themePicker.querySelectorAll("option")].filter((o) => o.value && !o.defaultSelected);
    return [
      { label: "Undo", keys: `${MOD}Z`, applies: () => undoDepth(view.state) > 0 && !assisting, run: () => undo(view) && forceLinting(view) },
      { label: "Redo", keys: `${MOD}${SHIFT}Z`, applies: () => redoDepth(view.state) > 0 && !assisting, run: () => redo(view) && forceLinting(view) },
      { label: "Find in the deck's texts", keys: `${MOD}F`, run: () => finding?.open() },
      ...rethemes.map(
        (o): Command => ({
          label: `Re-theme in ${o.textContent}`,
          applies: () => showing() !== undefined,
          run: () => {
            themePicker.value = o.value;
            themePicker.dispatchEvent(new Event("change"));
          },
        }),
      ),
      { label: "Save", keys: `${MOD}S`, run: () => save().catch(failed) },
      { label: "Save as…", applies: () => !$("#save-as").hidden, run: () => $("#save-as").click() },
      { label: "Download .scaena", run: () => download().catch(failed) },
      { label: "Export the state shown as a PNG…", applies: () => showing() !== undefined, run: () => openExport("png") },
      { label: "Export the deck as a PDF", applies: () => last?.valid === true, run: () => exportAs("pdf").catch(failed) },
      { label: "Export the deck as one HTML file", applies: () => last?.valid === true, run: () => exportAs("html").catch(failed) },
      { label: "Play", applies: () => !play.hidden, run: () => play.click() },
      { label: "New deck…", run: () => $("#new-deck").click() },
      { label: "Open a .scaena file…", run: () => $("#open-file").click() },
      { label: "Open a folder…", applies: () => !$("#open-folder").hidden, run: () => $("#open-folder").click() },
      { label: "Show the inspector", run: () => tab("inspector").focus() },
      { label: "Show the layers", run: () => tab("layers").focus() },
      { label: "Show the data", run: () => tab("data").focus() },
      { label: "Show the assistant", run: () => (tab("assistant"), $("#question").focus()) },
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

  const commanding = palette(
    $<HTMLDialogElement>("#palette"),
    () => {
      const node = () => (picked().length === 1 ? picked()[0] : undefined);
      return [...nodeCommands(), ...layerCommands(node, () => false), ...canvasCommands(false), ...stateCommands(), ...deckCommands()];
    },
    ask,
  );
  $("#commands-open").onclick = () => commanding.open();
  /** Where a menu opens for `e`: at the pointer, or, from the keyboard, under `el`. */
  const near = (e: MouseEvent, el: Element): [number, number] => {
    const r = el.getBoundingClientRect();
    const inside = e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
    return inside ? [e.clientX, e.clientY] : [r.left, r.bottom];
  };
  // A right click on a state in the strip shows it, then offers what is done to it.
  $("#strip ol").addEventListener("contextmenu", (e) => {
    const li = (e.target as Element).closest<HTMLElement>("li[data-state]");
    if (!li) return;
    e.preventDefault();
    const index = last?.states.findIndex(([id]) => id === li.dataset.state) ?? -1;
    if (index >= 0 && index !== shown) goTo(index);
    const [x, y] = near(e, li);
    menu(x, y, stateCommands().filter((c) => c.where?.includes("state")), li);
  });
  // A right click on a layer selects its node where it is shown, then offers what is done to it.
  $("#layers ul").addEventListener("contextmenu", (e) => {
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
      quiet = true;
      view.dispatch({ changes: change(view.state.doc.toString(), text) });
      quiet = false;
      saved = edits;
      tell();
      status.textContent = `read again from disk: ${paths.join(", ")}`;
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
  // Export…: the state shown as a PNG at a width asked, or the deck as a PDF or one HTML file.
  const exporting = $<HTMLDialogElement>("#exporting");
  const exportWidth = $<HTMLInputElement>("#exporting-width");
  const exportNote = $("#exporting-note");
  const chosenExport = () => exporting.querySelector<HTMLInputElement>("input[name=as]:checked")!.value as "png" | "pdf" | "html";
  /** What the export chosen makes, said under the choices. */
  const noteExport = () => {
    const as = chosenExport();
    $("#exporting-size").hidden = as !== "png";
    exportWidth.disabled = as !== "png";
    const [w, h] = board.size();
    const width = Number(exportWidth.value);
    const now = showing()?.state;
    exportNote.textContent = {
      png: Number.isInteger(width) && width > 0
        ? `${now ?? "The state shown"} at rest${format() ? ` in ${format()}` : ""}: ${width} × ${Math.round((h * width) / w)} pixels, painted by the CPU painter.`
        : "A width in whole pixels: the height keeps the canvas's aspect.",
      pdf: "A page for each slide at its last state: vector paths, text in subset fonts that copies and searches as the deck reads, tagged for a screen reader.",
      html: "The player and the deck in one file that plays in a browser with no network, its fonts subset to what the deck draws.",
    }[as];
  };
  exporting.oninput = noteExport;
  /** Open Export…, `as` chosen. */
  const openExport = (as: "png" | "pdf" | "html" = "png") => {
    exporting.querySelector<HTMLInputElement>(`input[name=as][value=${as}]`)!.checked = true;
    exportWidth.value = String(Math.round(board.size()[0]));
    noteExport();
    exporting.returnValue = "";
    exporting.showModal();
    (as === "png" ? exportWidth : $<HTMLButtonElement>("#exporting-go")).focus();
  };
  $<HTMLButtonElement>("#export").onclick = () => openExport();
  exporting.onclose = () => {
    if (exporting.returnValue !== "export") return;
    const as = chosenExport();
    void exportAs(as, as === "png" ? Number(exportWidth.value) : undefined).catch(failed);
  };
  onkeydown = (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void save().catch(failed);
    }
    // ⌘K: every command, by name (PLAN 2.53); again, it closes.
    if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "k" && !e.defaultPrevented) {
      e.preventDefault();
      if ($<HTMLDialogElement>("#palette").open) commanding.close();
      else commanding.open();
    }
    // ⌘F finds in the deck's texts (PLAN 2.47); in the source, the browser's find stays.
    if ((e.metaKey || e.ctrlKey) && !e.altKey && e.key.toLowerCase() === "f" && !(e.target instanceof Element && e.target.closest(".cm-editor"))) {
      e.preventDefault();
      finding?.open();
    }
  };
  onbeforeunload = (e) => {
    if (dirty()) e.preventDefault();
  };
  current = {
    close: () => {
      clearTimeout(pending);
      hearing?.close();
      board.close();
      view.destroy();
      stage.close();
    },
    dirty,
  };

  statesPicker.onchange = () => {
    const start = last?.states[statesPicker.selectedIndex]?.[1];
    if (start !== undefined) view.dispatch({ selection: { anchor: start }, scrollIntoView: true });
    void show(statesPicker.selectedIndex);
  };
  formatPicker.onchange = () => {
    void show(shown);
    states.reformat();
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
      /** Export (PLAN 2.54): `png` (the state shown, `width` pixels wide), `pdf`, or `html`, as the
       * Export dialog does; the file's name and bytes, also downloaded. */
      exportAs,
      /** Drop a file named `name` at `at` in the source (the cursor by default). */
      drop: (name: string, bytes: ArrayBuffer, at?: number) => drop([new File([bytes], name)], at ?? view.state.selection.main.head),
      where: () => ({ name, where, dirty: dirty() }),
      source: () => view.state.doc.toString(),
      /** Put `text` in the editor, as typing would: the cursor ends where the change does,
       * and the next lint sees it. */
      type: (text: string) => {
        const c = change(view.state.doc.toString(), text);
        view.dispatch({ changes: c, selection: { anchor: c.from + c.insert.length }, userEvent: "input.type" });
      },
      /** Put the cursor at `offset`. */
      cursor: (offset: number) => view.dispatch({ selection: { anchor: offset } }),
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
      at: () => stage.at,
      /** The assistant: ask it something, and read the conversation. */
      assistant,
      /** The canvas: what it selected, what stands where, and its drag. */
      canvas: board,
      /** The layers panel: what it lists, and the patches it makes. */
      layers: layering,
      /** The data panel (PLAN 2.55): the source shown and its sheet. */
      data,
      /** The files panel (PLAN 2.59): the bundle's files as it lists them, and its changes. */
      files: filing,
      /** The inspector's edits: what it offers for the node selected, and a choice made there. */
      look,
      /** The state strip: its states, thumbnails, and the patches it makes. */
      strip: states,
      /** The cue of the state shown: its bars, and the patches its drags make. */
      cue: cueing,
      /** Find and replace: what it finds, and the match shown. */
      find: finding,
      /** Commands by name (PLAN 2.53): the palette, and what it lists. */
      commands: commanding,
    },
  });
}

/** The tabs under the preview: the inspector, the layers, the data, and the assistant. The arrow keys, Home, and
 * End move between them, and only the one shown is in the tab order. */
function tabs() {
  const buttons = [...document.querySelectorAll<HTMLButtonElement>('#tabs [role="tab"]')];
  const pick = (button: HTMLButtonElement) => {
    for (const other of buttons) {
      const on = other === button;
      other.setAttribute("aria-selected", String(on));
      other.tabIndex = on ? 0 : -1;
      $(`#${other.getAttribute("aria-controls")}`).hidden = !on;
    }
  };
  buttons.forEach((button, i) => {
    button.tabIndex = button.getAttribute("aria-selected") === "true" ? 0 : -1;
    button.onclick = () => pick(button);
    button.onkeydown = (e) => {
      const to = { ArrowRight: i + 1, ArrowLeft: i - 1, Home: 0, End: buttons.length - 1 }[e.key];
      if (to === undefined) return;
      e.preventDefault();
      const next = buttons[(to + buttons.length) % buttons.length];
      pick(next);
      next.focus();
    };
  });
}

tabs();
controls()
  .then(first)
  .then(edit)
  .catch(failed);
