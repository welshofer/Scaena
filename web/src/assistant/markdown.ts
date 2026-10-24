// The model's answer, which is markdown by nature, shown as it reads (narrative ATTR-4):
// paragraphs, headings, lists, code spans and fences, bold, italic, and links, built as elements,
// never as HTML from the model's text. A link to a slide (`[the cover](state:cover)`, or a state's
// id in a code span) shows that state. The Mac's `MarkdownBlocks` splits the same blocks.

/** How a rendered answer reaches the editor: the states there are, and what showing one does. */
export interface Linking {
  states?: string[];
  show?: (state: string) => void;
}

/** One block of an answer, as the text reads: what the Mac's `MarkdownBlock` is too. */
export type Block =
  | { kind: "paragraph"; text: string }
  | { kind: "heading"; level: number; text: string }
  | { kind: "list"; ordered: boolean; items: { level: number; text: string }[] }
  | { kind: "code"; text: string; language?: string };

const ITEM = /^(\s*)(?:([-*+])|(\d+)[.)])\s+(.*)$/;
const HEADING = /^(#{1,6})\s+(.*?)\s*#*\s*$/;
const FENCE = /^\s*(`{3,}|~{3,})\s*([\w+-]*)\s*$/;

/** `text` split into its blocks: a fence to its closing line, a heading, a list (nested by its
 * indent, two spaces a level), and runs of lines as paragraphs, blank lines between them. */
export function blocks(text: string): Block[] {
  const out: Block[] = [];
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  let paragraph: string[] = [];
  const flush = () => {
    if (paragraph.length) out.push({ kind: "paragraph", text: paragraph.join("\n") });
    paragraph = [];
  };
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const fence = FENCE.exec(line);
    if (fence) {
      flush();
      const close = new RegExp(`^\\s*${fence[1][0]}{${fence[1].length},}\\s*$`);
      const code: string[] = [];
      for (i++; i < lines.length && !close.test(lines[i]); i++) code.push(lines[i]);
      out.push({ kind: "code", text: code.join("\n"), ...(fence[2] && { language: fence[2] }) });
      continue;
    }
    const heading = HEADING.exec(line);
    if (heading) {
      flush();
      out.push({ kind: "heading", level: heading[1].length, text: heading[2] });
      continue;
    }
    const item = ITEM.exec(line);
    if (item) {
      flush();
      const ordered = item[3] !== undefined;
      const items: { level: number; text: string }[] = [];
      for (; i < lines.length; i++) {
        const next = ITEM.exec(lines[i]);
        if (next) {
          items.push({ level: Math.floor(next[1].replace(/\t/g, "  ").length / 2), text: next[4] });
        } else if (lines[i].trim() && /^\s+/.test(lines[i]) && items.length) {
          // A line indented under an item carries that item on.
          items[items.length - 1].text += ` ${lines[i].trim()}`;
        } else break;
      }
      i--;
      out.push({ kind: "list", ordered, items });
      continue;
    }
    if (!line.trim()) flush();
    else paragraph.push(line);
  }
  flush();
  return out;
}

/** `text`, an answer, rendered into `into` as elements. */
export function render(text: string, into: HTMLElement, linking: Linking = {}): void {
  into.replaceChildren();
  for (const block of blocks(text)) {
    switch (block.kind) {
      case "paragraph": {
        const p = document.createElement("p");
        inline(block.text, p, linking);
        into.append(p);
        break;
      }
      case "heading": {
        // An answer's headings are small: a transcript, not a page.
        const h = document.createElement(block.level <= 2 ? "h4" : block.level === 3 ? "h5" : "h6");
        inline(block.text, h, linking);
        into.append(h);
        break;
      }
      case "code": {
        const pre = document.createElement("pre");
        const code = document.createElement("code");
        if (block.language) code.dataset.language = block.language;
        code.textContent = block.text;
        pre.append(code);
        into.append(pre);
        break;
      }
      case "list": {
        // Nested by level: an item deeper than the last opens a list inside it.
        const root = document.createElement(block.ordered ? "ol" : "ul");
        const open: HTMLElement[] = [root];
        for (const item of block.items) {
          while (open.length - 1 > item.level) open.pop();
          while (open.length - 1 < item.level) {
            const last = open[open.length - 1].lastElementChild as HTMLElement | null;
            const inner = document.createElement(block.ordered ? "ol" : "ul");
            (last ?? open[open.length - 1]).append(inner);
            open.push(inner);
          }
          const li = document.createElement("li");
          inline(item.text, li, linking);
          open[open.length - 1].append(li);
        }
        into.append(root);
        break;
      }
    }
  }
}

const INLINE = /(`+)([^`]|[^`][\s\S]*?[^`])\1(?!`)|\[([^\]\n]+)\]\(((?:[^()\s]|\([^()\s]*\))+)\)|(\*\*|__)(?=\S)([\s\S]*?\S)\5|(\*|_)(?=\S)([^*_\n]*?\S)\7|(https?:\/\/[^\s<>()]+[^\s<>().,;:!?'"])/g;

/** `text`, with its spans, into `into`: code, links, bold, italic, and bare web addresses. */
export function inline(text: string, into: HTMLElement, linking: Linking = {}): void {
  let at = 0;
  for (const m of text.matchAll(INLINE)) {
    if (m.index > at) into.append(text.slice(at, m.index));
    at = m.index + m[0].length;
    if (m[2] !== undefined) {
      const code = m[2].trim();
      // A state's id in a code span is a link to that state.
      if (linking.states?.includes(code) && linking.show) into.append(stateLink(code, code, linking.show));
      else {
        const span = document.createElement("code");
        span.textContent = code;
        into.append(span);
      }
    } else if (m[3] !== undefined) {
      const words = m[3];
      const url = m[4];
      // A link to a state the deck has; to one it lacks, the words alone.
      const named = /^state:(.+)$/.exec(url)?.[1] ?? (url.startsWith("#") ? url.slice(1) : undefined);
      const state = named !== undefined && (linking.states?.includes(named) ?? true) ? named : undefined;
      if (state && linking.show) into.append(stateLink(words, state, linking.show));
      else if (/^https?:\/\//.test(url)) into.append(webLink(words, url));
      else {
        const span = document.createElement("span");
        inline(words, span, linking);
        into.append(span);
      }
    } else if (m[6] !== undefined) {
      const strong = document.createElement("strong");
      inline(m[6], strong, linking);
      into.append(strong);
    } else if (m[8] !== undefined) {
      const em = document.createElement("em");
      inline(m[8], em, linking);
      into.append(em);
    } else if (m[9] !== undefined) {
      into.append(webLink(m[9], m[9]));
    }
  }
  if (at < text.length) into.append(text.slice(at));
}

function webLink(words: string, url: string): HTMLAnchorElement {
  const a = document.createElement("a");
  a.href = url;
  a.target = "_blank";
  a.rel = "noopener noreferrer";
  a.textContent = words;
  return a;
}

function stateLink(words: string, state: string, show: (state: string) => void): HTMLAnchorElement {
  const a = document.createElement("a");
  a.href = `#${state}`;
  a.className = "state";
  a.dataset.state = state;
  a.textContent = words;
  a.onclick = (e) => {
    e.preventDefault();
    show(state);
  };
  return a;
}
