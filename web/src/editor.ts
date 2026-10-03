// The source editor (PLAN 2.3–2.4, SPEC §9.2): the deck as `.scn` (SPEC §4) in CodeMirror.
// The engine compiles it as it is typed and lints what compiles; findings sit in the gutter
// and under the source, each placed where the source wrote what it is about, and a fix is one
// click. The preview shows the state the cursor is in, at rest, and the inspector its nodes
// resolved, each text node's look, what overrides set, and its cue.
//
// The bundle opens from a URL, a folder on disk, a `.scaena` file, or the browser's own
// storage, and saves where it is kept, or into the browser's storage; a download is a
// `.scaena` zip with its fonts subset. A file dropped on the source joins the bundle, and its
// path goes where it was dropped (PLAN 2.4).
//
// The assistant (PLAN 2.6) works on the deck with the user's own key: the source is read-only
// while it works, and each edit it makes comes into the source as it is made.
//
// `?bundle=` is a bundle's directory or its deck file (by default the bundle the build names,
// the site's demo deck (PLAN 2.7), or else the revenue example), `opfs:NAME` for one the
// browser keeps, or `folder:NAME` for a folder opened before; `?painter=` chooses who paints,
// as in the player. Play opens the player on the bundle as it was last saved.
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { syntaxHighlighting } from "@codemirror/language";
import { type Diagnostic, lintGutter, linter, setDiagnostics } from "@codemirror/lint";
import { Compartment, EditorState } from "@codemirror/state";
import {
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from "@codemirror/view";
import { panel } from "./assistant/panel";
import { sourceOf } from "./bundle";
import { keptNames } from "./folders";
import type { Edited, Finding, FromWorker, Inspected, Linted, Painter, Source, Where } from "./protocol";
import { scn, scnHighlight } from "./scn";
import { worker } from "./spawn";
import { Stage } from "./stage";

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

/** The page's address names where the bundle is kept, so a reload opens it there. */
function address(where: Where | undefined) {
  if (!where) return;
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
}

/** The bundle `?bundle=` names. A folder kept from before asks again for leave to write,
 * which takes a click. */
async function first(): Promise<Source> {
  const bundle = params.get("bundle");
  if (!bundle?.startsWith("folder:")) return sourceOf(bundle, fallback);
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
  const statesPicker = $<HTMLSelectElement>("#state");
  const formatPicker = $<HTMLSelectElement>("#format");
  const problems = $<HTMLUListElement>("#problems");
  const inspector = $("#inspector");
  const whereLine = $("#where");
  const play = $<HTMLAnchorElement>("#play");
  const stage = await Stage.open($<HTMLCanvasElement>("#stage"), source, painter, worker);
  stage.onError = failed;
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
    const kept = where ? (where.kind === "folder" ? `folder ${where.name}` : `kept in this browser as ${where.name}`) : `${name}, not saved`;
    whereLine.textContent = `${kept}${dirty() ? " · changed" : ""}`;
    // The player opens a bundle by its address or from the browser's storage, not a folder.
    const playing = new URLSearchParams({ bundle: where ? `${where.kind}:${where.name}` : (params.get("bundle") ?? fallback) });
    if (params.has("painter")) playing.set("painter", painter);
    play.href = `index.html?${playing}`;
    play.hidden = where?.kind === "folder";
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
  /** The source the assistant's last edit made, and what the worker found in it: the lint of
   * that source takes it as it is, rather than asking the worker again. */
  let assisted: { source: string; edited: Edited } | undefined;

  const view = new EditorView({
    parent: $("#code"),
    state: EditorState.create({
      doc: await stage.source(),
      extensions: [
        lineNumbers(),
        highlightActiveLineGutter(),
        history(),
        drawSelection(),
        highlightActiveLine(),
        keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
        scn,
        syntaxHighlighting(scnHighlight),
        lintGutter(),
        linter(lint, { delay: 150 }),
        locked.of([]),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            version++;
            clearTimeout(pending);
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

  /** Compile what is in the editor, show it, and lint the state shown: CodeMirror's lint
   * source. Once typing stops, every state is linted. */
  async function lint(view: EditorView): Promise<Diagnostic[]> {
    const source = view.state.doc.toString();
    const read = version;
    const sent = performance.now();
    let edited: Edited;
    try {
      edited = assisted?.source === source ? assisted.edited : await stage.edit(source, shown, format());
    } catch (e) {
      status.textContent = `error: ${said(e)}`;
      return [];
    }
    trips.push({ ms: performance.now() - sent, ...edited.ms });
    last = edited;
    if (edited.valid) {
      statesPicker.replaceChildren(...edited.states.map(([id]) => new Option(id, id)));
      if (edited.at) shown = edited.at.index;
      statesPicker.selectedIndex = shown;
      void inspect();
    }
    const findings = edited.error ? [edited.error] : edited.findings;
    report(findings, edited);
    if (!edited.whole) {
      clearTimeout(pending);
      pending = setTimeout(() => void lintAll(read), pause);
    }
    return findings.map((f) => diagnostic(f, view.state.doc.length));
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
    view.dispatch(setDiagnostics(view.state, linted.findings.map((f) => diagnostic(f, view.state.doc.length))));
  }

  /** The findings under the source, and the status line: what was found, and how long each
   * step took; `whole`, how long the lint of every state took. */
  function report(findings: Finding[], edited: Edited, whole?: number) {
    list(findings);
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
    status.textContent = `${errors} error${errors === 1 ? "" : "s"}, ${findings.length - errors} other · compiled in ${ms(edited.ms.compile)}, shown in ${ms(edited.ms.paint)}, ${linted}`;
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
      actions: f.fix ? [{ name: "Fix", apply: () => void fix(f) }] : [],
    };
  }

  /** Apply `f`'s fix: the engine patches the deck and writes it back as source, and the
   * editor takes only what changed. */
  async function fix(f: Finding) {
    const next = await stage.fix(f.fix!);
    view.dispatch({ changes: change(view.state.doc.toString(), next), userEvent: "input.fix" });
  }

  /** Every finding under the source; a click goes to it. */
  function list(findings: Finding[]) {
    problems.replaceChildren(
      ...findings.map((f) => {
        const li = document.createElement("li");
        li.className = f.severity;
        const where = f.at ? `${f.at.line}:${f.at.col}` : (f.file ?? "");
        li.innerHTML = `<span class="code">${f.code}</span>${html(f.message)}<span class="where">${html(
          [f.state, f.format, where].filter(Boolean).join(" · "),
        )}</span>`;
        if (f.at) li.onclick = () => view.dispatch({ selection: { anchor: f.at!.from }, scrollIntoView: true });
        return li;
      }),
    );
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
    await stage.seek(index, undefined, format());
    await inspect();
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
      inspector.textContent = said(e);
      return;
    }
    const cue = found.timeline;
    const rows = Object.keys(found.nodes).map((id) => {
      const look = found.looks?.[id];
      const overrides = found.overrides?.[id]?.length ?? 0;
      const text = look
        ? `<span class="swatch" style="background:${look.hex}"></span>${html(look.role)} · ${html(look.family)} ${look.size}/${look.weight} · ${html(look.color)}`
        : "";
      return `<tr><td>${html(id)}</td><td>${text}</td><td>${overrides || ""}</td></tr>`;
    });
    const motions = (cue?.motions ?? []).map(
      (m) => `<tr><td>${html(m.node)}</td><td>${html(m.motion)}${m.units > 1 ? ` × ${m.units}` : ""}</td><td>${m.start.toFixed(0)}–${m.end.toFixed(0)} ms</td></tr>`,
    );
    inspector.innerHTML = `
      <h2>${html(found.state_id)}${found.layout ? ` · ${html(found.layout)}` : ""}</h2>
      ${cue ? `<p>starts at ${cue.start.toFixed(0)} ms · cue ${cue.span.toFixed(0)} ms · holds ${cue.hold.toFixed(0)} ms · transition ${cue.transition.duration.toFixed(0)} ms, matched by ${html(cue.transition.match)}</p>` : ""}
      ${motions.length ? `<table><tr><th>moves</th><th></th><th></th></tr>${motions.join("")}</table>` : ""}
      <h2>Nodes</h2>
      <table><tr><th>node</th><th>look</th><th>overrides</th></tr>${rows.join("")}</table>`;
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
    where = done.where;
    name = done.where.name;
    quiet = true;
    view.dispatch({ changes: change(view.state.doc.toString(), renamedIn(view.state.doc.toString(), done.renamed)) });
    quiet = false;
    saved = at;
    address(where);
    tell();
    void listKept();
    status.textContent = `saved ${done.files} files${done.renamed.length ? `, ${done.renamed.length} named by their content` : ""}`;
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
    apply: (source, edited) => {
      assisted = { source, edited };
      view.dispatch({ changes: change(view.state.doc.toString(), source), userEvent: "input.assistant" });
    },
    lock: (on) =>
      view.dispatch({ effects: locked.reconfigure(on ? [EditorState.readOnly.of(true), EditorView.editable.of(false)] : []) }),
  });

  $<HTMLButtonElement>("#save").onclick = () => void save().catch(failed);
  $<HTMLButtonElement>("#download").onclick = () => void download().catch(failed);
  onkeydown = (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void save().catch(failed);
    }
  };
  onbeforeunload = (e) => {
    if (dirty()) e.preventDefault();
  };
  current = {
    close: () => {
      clearTimeout(pending);
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
  formatPicker.onchange = () => void show(shown);

  // For tests and the console.
  Object.assign(window, {
    scaena: {
      opened: stage.opened,
      painter: stage.opened.painter,
      /** Open a bundle: `{ opfs }`, `{ folder }` (any directory handle), or `{ zip, name }`. */
      open: (source: Source) => open(source),
      save,
      download,
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
    },
  });
}

/** The tabs under the preview: the inspector, and the assistant. */
function tabs() {
  const buttons = [...document.querySelectorAll<HTMLButtonElement>('#tabs [role="tab"]')];
  for (const button of buttons)
    button.onclick = () => {
      for (const other of buttons) {
        const on = other === button;
        other.setAttribute("aria-selected", String(on));
        $(`#${other.getAttribute("aria-controls")}`).hidden = !on;
      }
    };
}

tabs();
controls()
  .then(first)
  .then(edit)
  .catch(failed);
