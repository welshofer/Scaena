// The Data panel (PLAN 2.55, ADR-0014, SPEC §3.10): a data source as a table, as `scaena data`
// shows it, in a tab beside the inspector and the layers. Its cells are edited in place, and rows
// added and taken away, each change one `data_edit`: one write of the source's file that keeps
// every other byte, or one patch of rows written inline. Every chart and table that reads the
// source shows the change.
//
// - The picker chooses the source. The line beside it says the file it is, or that its rows are
//   written inline, and how many rows it has.
// - A cell is a field. Enter, or leaving it changed, sets it, and Escape puts back what it held.
//   Enter goes on to the cell below (Shift+Enter above), and ↑ and ↓ move between rows. A value its
//   column does not read is refused, with why, and the cell holds what it held.
// - A cell its column does not read is marked, with why, and listed under the table: a click on
//   it there goes to it. Setting it to what its column reads fixes it.
// - + Row adds a row after the row last focused, else at the end; − Row takes that row away.
// - Undo and Redo (⌘Z and ⇧⌘Z in the table) undo the source's last change: a file's as the page
//   keeps them, and rows written inline as the source's own undo does.
import type { Edited, RowEdit, Sheet } from "./protocol";
import type { Stage } from "./stage";

/** What the panel asks of the editor around it. */
export interface DataEditor {
  /** The state shown, by its id and its slot's index; none while the source does not compile. */
  shown(): { state: string; index: number } | undefined;
  format(): string | undefined;
  source(): string;
  /** Take `source`, the deck after an edit of rows written inline: one change, one step to undo. */
  apply(source: string, edited: Edited): void;
  /** Take an edit of a data file: the source as it stands, shown and linted as `edited` says, and a
   * change to save. */
  took(edited: Edited): void;
  /** The source's own undo and redo. */
  undo(): void;
  redo(): void;
  say(text: string): void;
}

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
const html = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
const rows = (n: number) => (n === 1 ? "1 row" : `${n} rows`);

/** The panel in `into`, over `stage`. */
export function sheets(stage: Stage, into: HTMLElement, editor: DataEditor) {
  const picker = into.querySelector<HTMLSelectElement>("[data-source]")!;
  const where = into.querySelector<HTMLElement>("[data-where]")!;
  const table = into.querySelector<HTMLTableElement>("table")!;
  const problems = into.querySelector<HTMLUListElement>("[data-problems]")!;
  /** The source shown, and its sheet as the engine last read it. */
  let shown: { name: string; file?: string; sheet: Sheet } | undefined;
  /** The row last focused: where + Row adds and − Row takes away. */
  let row: number | undefined;
  /** Sheets asked for: the last answer wins. */
  let asking = 0;
  /** One change at a time, each made on the source the one before left. */
  let making: Promise<unknown> = Promise.resolve();

  /** The field of row `r`, column `c`. */
  const cell = (r: number, c: number) => table.querySelector<HTMLInputElement>(`input[data-row="${r}"][data-col="${c}"]`);

  /** Read source `name` (the one shown, or the first) as a sheet, and show it. */
  async function refresh(name = shown?.name) {
    if (into.hidden) return;
    const asked = ++asking;
    let got: Awaited<ReturnType<Stage["sheet"]>>;
    try {
      got = await stage.sheet(editor.source(), name);
    } catch (e) {
      if (asked !== asking) return;
      shown = undefined;
      where.textContent = `the data waits for a source that compiles: ${said(e)}`;
      table.replaceChildren();
      problems.replaceChildren();
      return;
    }
    if (asked !== asking) return;
    picker.replaceChildren(...got.sources.map((s) => new Option(s.name, s.name)));
    picker.disabled = got.sources.length === 0;
    if (got.name !== undefined) picker.value = got.name;
    if (!got.sources.length) {
      shown = undefined;
      where.textContent = "The deck has no data source. Drop a CSV or JSON file on the source, or ask the assistant to attach one.";
      table.replaceChildren();
      problems.replaceChildren();
      return;
    }
    if (!got.sheet) {
      shown = undefined;
      where.textContent = got.why ?? "";
      table.replaceChildren();
      problems.replaceChildren();
      return;
    }
    if (got.name !== shown?.name) row = undefined;
    shown = { name: got.name!, file: got.file, sheet: got.sheet };
    draw();
  }

  /** The table: a column for each of the source's, its type under its name, and a row for each,
   * its index first; then the cells their column does not read. */
  function draw() {
    if (!shown) return;
    const { sheet, file } = shown;
    where.textContent = `${file ?? "rows written inline"} · ${rows(sheet.rows.length)}`;
    // The field focused, and what is typed in it not yet set: a redraw, after an edit of the deck,
    // keeps both.
    const active = document.activeElement as HTMLInputElement | null;
    const focus =
      active?.matches("input[data-row]") && table.contains(active)
        ? { r: +active.dataset.row!, c: +active.dataset.col!, typed: active.value !== active.defaultValue ? active.value : undefined }
        : undefined;
    const bad = new Map((sheet.problems ?? []).map((p) => [`${p.row} ${p.column}`, p.why]));
    const head = sheet.columns.map((c) => `<th scope="col" class="${c.type}">${html(c.name)}<span class="type">${c.type}</span></th>`).join("");
    const body = sheet.rows.map((cells, r) => {
      const fields = cells.map((value, c) => {
        const column = sheet.columns[c];
        const why = bad.get(`${r} ${column.name}`);
        // A field holds one line: a value with a line break in it is edited in its file.
        const lines = /[\r\n]/.test(value);
        const title = why ?? (lines ? "It holds a line break: edit it in its file" : "");
        return `<td class="${column.type}"><input data-row="${r}" data-col="${c}" value="${html(value)}" spellcheck="false" aria-label="${html(`${column.name}, row ${r}`)}"${
          why ? ' aria-invalid="true"' : ""
        }${lines ? " readonly" : ""}${title ? ` title="${html(title)}"` : ""} /></td>`;
      });
      return `<tr${r === row ? ' class="current"' : ""}><th scope="row" class="index">${r}</th>${fields.join("")}</tr>`;
    });
    table.innerHTML = `<thead><tr><th scope="col" class="index">row</th>${head}</tr></thead><tbody>${body.join("")}</tbody>`;
    problems.innerHTML = (sheet.problems ?? [])
      .map((p) => {
        const c = sheet.columns.findIndex((col) => col.name === p.column);
        return `<li><button type="button" data-row="${p.row}" data-col="${c}">${html(`${p.column}, row ${p.row}`)}</button> ${html(p.why)}</li>`;
      })
      .join("");
    if (!focus) return;
    const field = cell(Math.min(focus.r, sheet.rows.length - 1), focus.c);
    if (field && focus.typed !== undefined && focus.r < sheet.rows.length) field.value = focus.typed;
    field?.focus();
  }

  /** Make `edits` of the source shown, by the user: one change, then say `done`. Refused, `undo`
   * puts back what the panel showed before. */
  function make(edits: RowEdit[], done: string, undo?: () => void) {
    const run = async () => {
      const name = shown?.name;
      if (name === undefined) return;
      editor.say("editing the data…");
      try {
        const { result, source, edited } = await stage.dataEdit(editor.source(), name, edits, editor.shown()?.index ?? 0, editor.format());
        if (result.refused) {
          undo?.();
          const why = result.added.find((f) => f.severity === "error") ?? result.added[0];
          editor.say(`refused: ${why ? `${why.code} ${why.message}` : "the deck would not be valid"}`);
        } else if (source !== undefined && edited) {
          if (result.file) editor.took(edited);
          else editor.apply(source, edited);
          editor.say(done);
        }
        shown = { name, file: result.file, sheet: result.sheet };
        draw();
      } catch (e) {
        undo?.();
        editor.say(`not made: ${said(e)}`);
      }
    };
    making = making.then(run, run);
    return making;
  }

  /** Set the cell `field` stands for to what it holds, where that changed. */
  function commit(field: HTMLInputElement) {
    if (!shown || field.readOnly || field.value === field.defaultValue) return making;
    const before = field.defaultValue;
    // Taken now: the blur after Enter sets nothing again.
    field.defaultValue = field.value;
    const r = +field.dataset.row!;
    const column = shown.sheet.columns[+field.dataset.col!].name;
    return make([{ op: "set", row: r, column, value: field.value }], `${column} of row ${r} set`, () => {
      field.value = before;
      field.defaultValue = before;
    });
  }

  /** Undo the source's last change, or with `redo` make again the last undone. */
  function undo(redo = false) {
    if (!shown) return;
    if (!shown.file) return redo ? editor.redo() : editor.undo();
    const run = async () => {
      try {
        const { name, edited } = await stage.dataUndo(editor.source(), redo, editor.shown()?.index ?? 0, editor.format());
        if (name === undefined || !edited) return editor.say(redo ? "no data change to redo" : "no data change to undo");
        editor.took(edited);
        editor.say(redo ? `made again: the last change to ${name}` : `undone: the last change to ${name}`);
        // A file the Files panel took out is put back by its path (PLAN 2.59): the source shown
        // stays.
        await refresh(name.includes("/") ? undefined : name);
      } catch (e) {
        editor.say(`${redo ? "not made again" : "not undone"}: ${said(e)}`);
      }
    };
    making = making.then(run, run);
  }

  picker.onchange = () => void refresh(picker.value);
  table.addEventListener("focusin", (e) => {
    const field = (e.target as Element).closest<HTMLInputElement>("input[data-row]");
    if (!field) return;
    row = +field.dataset.row!;
    for (const tr of table.querySelectorAll("tbody tr")) tr.classList.toggle("current", tr === field.closest("tr"));
  });
  table.addEventListener("change", (e) => {
    const field = (e.target as Element).closest<HTMLInputElement>("input[data-row]");
    if (field) void commit(field);
  });
  table.addEventListener("keydown", (e) => {
    const field = (e.target as Element).closest<HTMLInputElement>("input[data-row]");
    if (!field) return;
    const [r, c] = [+field.dataset.row!, +field.dataset.col!];
    const key = e.key.toLowerCase();
    const mod = e.metaKey || e.ctrlKey;
    if (e.key === "Enter") {
      e.preventDefault();
      void commit(field).then(() => cell(r + (e.shiftKey ? -1 : 1), c)?.focus());
    } else if (e.key === "Escape" && field.value !== field.defaultValue) {
      e.preventDefault();
      e.stopPropagation();
      field.value = field.defaultValue;
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = cell(r + (e.key === "ArrowDown" ? 1 : -1), c);
      void commit(field);
      next?.focus();
    } else if (mod && (key === "z" || key === "y") && field.value === field.defaultValue) {
      // A field changed undoes its own typing; one as it was undoes the source's last change.
      e.preventDefault();
      undo(key === "y" || e.shiftKey);
    }
  });
  problems.onclick = (e) => {
    const go = (e.target as Element).closest<HTMLElement>("button[data-row]");
    if (go) cell(+go.dataset.row!, +go.dataset.col!)?.focus();
  };
  into.querySelector<HTMLButtonElement>("[data-add]")!.onclick = () => {
    if (!shown) return;
    const at = row === undefined ? undefined : row + 1;
    const where = at === undefined ? shown.sheet.rows.length : at;
    void make([at === undefined ? { op: "add" } : { op: "add", row: at }], `a row added at ${where}`).then(() => {
      row = where;
      cell(where, 0)?.focus();
    });
  };
  into.querySelector<HTMLButtonElement>("[data-remove]")!.onclick = () => {
    if (!shown) return;
    if (row === undefined || row >= shown.sheet.rows.length) return editor.say("focus a cell of the row to take away first");
    const at = row;
    void make([{ op: "remove", row: at }], `row ${at} taken away`).then(() => {
      if (shown && at >= shown.sheet.rows.length) row = shown.sheet.rows.length ? shown.sheet.rows.length - 1 : undefined;
    });
  };
  into.querySelector<HTMLButtonElement>("[data-undo]")!.onclick = () => undo();
  into.querySelector<HTMLButtonElement>("[data-redo]")!.onclick = () => undo(true);
  // Read again each time the tab is shown.
  new MutationObserver(() => {
    if (!into.hidden) void refresh();
  }).observe(into, { attributes: true, attributeFilter: ["hidden"] });

  return {
    /** Read the source shown again, as the deck now has it: after an edit of the deck. */
    refresh: () => refresh(),
    /** The source shown, by name, and its sheet: for tests. */
    shown: () => shown,
    /** Each change waits for the one before it: resolved once the last is made. */
    settled: () => making,
  };
}
