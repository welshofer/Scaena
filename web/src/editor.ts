// The source editor (PLAN 2.3, SPEC §9.2): the deck as `.scn` (SPEC §4) in CodeMirror. The
// engine compiles it as it is typed and lints what compiles; findings sit in the gutter and
// under the source, each placed where the source wrote what it is about, and a fix is one
// click. The preview shows the state the cursor is in, at rest, and the inspector its nodes
// resolved, each text node's look, what overrides set, and its cue.
//
// `?bundle=` is a bundle's directory or its deck file (the revenue example by default);
// `?painter=` chooses who paints, as in the player.
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { syntaxHighlighting } from "@codemirror/language";
import { type Diagnostic, lintGutter, linter, setDiagnostics } from "@codemirror/lint";
import { EditorState } from "@codemirror/state";
import {
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from "@codemirror/view";
import { deckFile } from "./bundle";
import type { Edited, Finding, Inspected, Linted, Painter } from "./protocol";
import { scn, scnHighlight } from "./scn";
import { Stage } from "./stage";

const params = new URLSearchParams(location.search);
const deck = deckFile(new URL(params.get("bundle") ?? "../../docs/examples/revenue.deck.json", location.href));
const painter = (params.get("painter") ?? "auto") as Painter;

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

async function edit() {
  const status = $("#status");
  const statesPicker = $<HTMLSelectElement>("#state");
  const formatPicker = $<HTMLSelectElement>("#format");
  const problems = $<HTMLUListElement>("#problems");
  const inspector = $("#inspector");
  const stage = await Stage.open($<HTMLCanvasElement>("#stage"), deck, painter);
  for (const format of stage.opened.formats) formatPicker.add(new Option(format, format));
  const format = () => formatPicker.value || undefined;

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
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            version++;
            clearTimeout(pending);
          }
          if (update.selectionSet) follow();
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
      edited = await stage.edit(source, shown, format());
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
    },
  });
}

edit().catch((e) => {
  $("#status").textContent = `error: ${said(e)}`;
  console.error(e);
});
