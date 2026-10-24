// Speaker notes as a presenter reads them (SPEC §2.6, lift-up USE-1): the blocks the engine
// reads a state's notes as (`scaena_core::notes`, `Player.notes`), lines, a list, or a table,
// each word plain, bold, italic, or code, drawn as elements, so the presenter view and a
// rehearsal show the reading the Mac's, the iPad's, and the remote's show. The deck keeps its
// notes as written; nothing here changes them.
import type { NotesBlock, NotesSpan, SpeakerNotes } from "./protocol";

/** `blocks` drawn into `into`, in place of whatever it held; `empty` where there are none. */
export function renderNotes(into: HTMLElement, blocks: NotesBlock[], empty = "No notes."): void {
  into.replaceChildren();
  if (!blocks.length) return void into.append(empty);
  for (const block of blocks) {
    switch (block.kind) {
      case "lines": {
        const p = document.createElement("p");
        block.lines.forEach((line, i) => {
          if (i) p.append("\n");
          p.append(...spans(line));
        });
        into.append(p);
        break;
      }
      case "list": {
        // Each item in a list at its level, a deeper item's list inside the item before it.
        const root = document.createElement(block.ordered ? "ol" : "ul");
        const open: HTMLElement[] = [root];
        for (const item of block.items) {
          const level = Math.min(Math.max(item.level, 0), 7);
          while (open.length > level + 1) open.pop();
          while (open.length < level + 1) {
            const deeper = document.createElement(block.ordered ? "ol" : "ul");
            const last = open[open.length - 1].lastElementChild ?? open[open.length - 1].appendChild(document.createElement("li"));
            last.append(deeper);
            open.push(deeper);
          }
          const li = document.createElement("li");
          li.append(...spans(item.spans));
          open[open.length - 1].append(li);
        }
        into.append(root);
        break;
      }
      case "table": {
        const table = document.createElement("table");
        block.rows.forEach((row, i) => {
          const head = block.header && i === 0;
          const section = table.appendChild(document.createElement(head ? "thead" : "tbody"));
          const tr = section.appendChild(document.createElement("tr"));
          for (const cell of row) tr.appendChild(document.createElement(head ? "th" : "td")).append(...spans(cell));
        });
        into.append(table);
        break;
      }
    }
  }
}

/** A line's spans, each in its look. */
function spans(line: NotesSpan[]): (string | HTMLElement)[] {
  return line.map((span) => {
    let node: string | HTMLElement = span.text;
    if (span.code) node = wrap("code", node);
    if (span.italic) node = wrap("em", node);
    if (span.bold) node = wrap("strong", node);
    return node;
  });
}

function wrap(tag: string, inner: string | HTMLElement): HTMLElement {
  const element = document.createElement(tag);
  element.append(inner);
  return element;
}

/** The notes as words alone, as the engine's `plain` says them: each line on its own, a list's
 * items marked, a table's cells set apart by a middle dot. */
export function plainNotes(notes: SpeakerNotes): string {
  const words = (line: NotesSpan[]) => line.map((s) => s.text).join("");
  const out: string[] = [];
  for (const block of notes.blocks) {
    if (block.kind === "lines") out.push(...block.lines.map(words));
    else if (block.kind === "list") {
      const counts = new Array<number>(8).fill(0);
      for (const item of block.items) {
        const level = Math.min(Math.max(item.level, 0), 7);
        counts[level] += 1;
        counts.fill(0, level + 1);
        out.push(`${"  ".repeat(level)}${block.ordered ? `${counts[level]}.` : "•"} ${words(item.spans)}`);
      }
    } else out.push(...block.rows.map((row) => row.map(words).join(" · ")));
  }
  return out.join("\n");
}
