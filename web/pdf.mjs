// A PDF read as far as the walks need it, with Node's zlib alone (PLAN 2.78): its pages, the text
// each copies as (by each font's ToUnicode map, through the forms a page draws, a span's ActualText in
// place of what its glyphs show), and its outline.
// It reads what krilla writes for `scaena export --format pdf` and the editor's Export…: objects
// written out, not in object streams; streams by `/Length` and `FlateDecode`; text in Type0 fonts
// whose codes the ToUnicode map gives. It is no general PDF reader.
//
//   import { readPdf } from "./pdf.mjs";
//   const { pages, outline } = readPdf(bytes);   // pages: [{ text }], outline: [{ title, kids }]
import { inflateSync } from "node:zlib";

const white = (c) => c === 0x20 || c === 0x0a || c === 0x0d || c === 0x09 || c === 0x0c || c === 0x00;
const delimiter = (c) => "()<>[]{}/%".includes(String.fromCharCode(c));

/** A reference to object `n`. */
class Ref {
  constructor(n) {
    this.n = n;
  }
}
/** A name, `/Name`, apart from a string. */
class Name {
  constructor(name) {
    this.name = name;
  }
}

/** Parse objects from `b` at `i`: a parser that returns `[value, next]`, or, in a content stream,
 * an operator as `{ op }`. */
function parser(b) {
  const skip = (i) => {
    for (;;) {
      while (i < b.length && white(b[i])) i++;
      if (b[i] !== 0x25) return i; // %
      while (i < b.length && b[i] !== 0x0a && b[i] !== 0x0d) i++;
    }
  };
  const token = (i) => {
    let j = i;
    while (j < b.length && !white(b[j]) && !delimiter(b[j])) j++;
    return [b.toString("latin1", i, j), j];
  };
  const literal = (i) => {
    // `(…)`: balanced parentheses, backslash escapes.
    const out = [];
    let depth = 1;
    i++;
    while (i < b.length) {
      let c = b[i++];
      if (c === 0x5c) {
        c = b[i++];
        const esc = { 0x6e: 10, 0x72: 13, 0x74: 9, 0x62: 8, 0x66: 12 }[c];
        if (esc !== undefined) out.push(esc);
        else if (c >= 0x30 && c <= 0x37) {
          let v = c - 0x30;
          for (let k = 0; k < 2 && b[i] >= 0x30 && b[i] <= 0x37; k++) v = v * 8 + (b[i++] - 0x30);
          out.push(v & 0xff);
        } else if (c === 0x0d || c === 0x0a) {
          if (c === 0x0d && b[i] === 0x0a) i++;
        } else out.push(c);
        continue;
      }
      if (c === 0x28) depth++;
      if (c === 0x29 && --depth === 0) break;
      out.push(c);
    }
    return [Buffer.from(out), i];
  };
  const value = (i) => {
    i = skip(i);
    const c = b[i];
    if (c === 0x2f) {
      const [name, j] = token(i + 1);
      return [new Name(name), j];
    }
    if (c === 0x28) return literal(i);
    if (c === 0x3c && b[i + 1] === 0x3c) {
      const dict = {};
      i += 2;
      for (;;) {
        i = skip(i);
        if (b[i] === 0x3e && b[i + 1] === 0x3e) return [dict, i + 2];
        const [key, j] = value(i);
        const [v, k] = value(j);
        dict[key.name] = v;
        i = k;
      }
    }
    if (c === 0x3c) {
      const end = b.indexOf(0x3e, i);
      let hex = b.toString("latin1", i + 1, end).replace(/\s+/g, "");
      if (hex.length % 2) hex += "0";
      return [Buffer.from(hex, "hex"), end + 1];
    }
    if (c === 0x5b) {
      const list = [];
      i++;
      for (;;) {
        i = skip(i);
        if (b[i] === 0x5d) return [list, i + 1];
        const [v, j] = value(i);
        list.push(v);
        i = j;
      }
    }
    const [word, j] = token(i);
    if (/^[+-]?(\d+\.?\d*|\.\d+)$/.test(word)) {
      // `n 0 R` is a reference.
      const m = /^\s*(\d+)\s+R(?![^\s()<>[\]{}/%])/.exec(b.toString("latin1", j, j + 24));
      if (/^\d+$/.test(word) && m) return [new Ref(Number(word)), j + m[0].length];
      return [Number(word), j];
    }
    if (word === "true" || word === "false") return [word === "true", j];
    if (word === "null") return [null, j];
    return [{ op: word }, j];
  };
  return { value, skip };
}

/** The objects of the file `bytes`, by number: `{ dict, stream? }` or a value. */
function objects(bytes) {
  const b = Buffer.from(bytes);
  const { value, skip } = parser(b);
  const all = new Map();
  const head = /(\d+)\s+0\s+obj\b/g;
  const text = b.toString("latin1");
  for (let m; (m = head.exec(text)); ) {
    const [v, j] = value(m.index + m[0].length);
    let at = skip(j);
    if (text.startsWith("stream", at)) {
      at += 6;
      if (b[at] === 0x0d) at++;
      if (b[at] === 0x0a) at++;
      const length = v.Length instanceof Ref ? undefined : v.Length;
      const end = length ?? text.indexOf("endstream", at);
      const raw = b.subarray(at, length === undefined ? end : at + length);
      all.set(Number(m[1]), { dict: v, raw });
      head.lastIndex = at + raw.length;
    } else all.set(Number(m[1]), v);
  }
  // Lengths given by reference, once every object is read.
  for (const [n, o] of all)
    if (o?.raw && o.dict.Length instanceof Ref) {
      const length = all.get(o.dict.Length.n);
      all.set(n, { dict: o.dict, raw: o.raw.subarray(0, length) });
    }
  return all;
}

/** Read `bytes`, a PDF: each page's text, as it copies, and the outline. */
export function readPdf(bytes) {
  const all = objects(bytes);
  const get = (v) => (v instanceof Ref ? all.get(v.n) : v);
  const data = (o) => {
    const filters = [get(o.dict.Filter)].flat().filter(Boolean);
    let raw = o.raw;
    for (const f of filters) {
      if (f.name !== "FlateDecode") throw new Error(`a stream filtered by ${f.name}`);
      raw = inflateSync(raw);
    }
    return raw;
  };
  const trailer = [...all.values()].find((o) => o && !o.raw && o.Type?.name === "Catalog");
  if (!trailer) throw new Error("no catalog");

  // The pages, in order.
  const pages = [];
  const walk = (node) => {
    node = get(node);
    if (node.Type?.name === "Pages") for (const kid of node.Kids) walk(kid);
    else pages.push(node);
  };
  walk(trailer.Pages);

  // Each font's codes, as its ToUnicode map gives them.
  const maps = new Map();
  const unicode = (font) => {
    if (maps.has(font)) return maps.get(font);
    const map = { width: 1, codes: new Map() };
    const stream = get(font.ToUnicode);
    if (stream?.raw) {
      const text = data(stream).toString("latin1");
      const utf16 = (hex) => Buffer.from(hex, "hex").swap16().toString("utf16le");
      for (const [, block] of text.matchAll(/beginbfchar([\s\S]*?)endbfchar/g))
        for (const [, src, dst] of block.matchAll(/<([0-9A-Fa-f]+)>\s*<([0-9A-Fa-f]+)>/g)) {
          map.width = src.length / 2;
          map.codes.set(parseInt(src, 16), utf16(dst));
        }
      for (const [, block] of text.matchAll(/beginbfrange([\s\S]*?)endbfrange/g))
        for (const [, lo, hi, dst] of block.matchAll(/<([0-9A-Fa-f]+)>\s*<([0-9A-Fa-f]+)>\s*(<[0-9A-Fa-f]+>|\[[^\]]*\])/g)) {
          map.width = lo.length / 2;
          const from = parseInt(lo, 16);
          const to = parseInt(hi, 16);
          if (dst.startsWith("[")) {
            const each = [...dst.matchAll(/<([0-9A-Fa-f]+)>/g)].map((m) => utf16(m[1]));
            for (let c = from; c <= to; c++) map.codes.set(c, each[c - from] ?? "");
          } else {
            const base = Buffer.from(dst.slice(1, -1), "hex").swap16();
            for (let c = from; c <= to; c++) {
              const out = Buffer.from(base);
              out.writeUInt16LE(out.readUInt16LE(out.length - 2) + (c - from), out.length - 2);
              map.codes.set(c, out.toString("utf16le"));
            }
          }
        }
    } else map.simple = true;
    maps.set(font, map);
    return map;
  };
  const decode = (font, s) => {
    const map = unicode(font);
    if (map.simple) return s.toString("latin1");
    let out = "";
    for (let i = 0; i + map.width <= s.length; i += map.width) out += map.codes.get(s.readUIntBE(i, map.width)) ?? "";
    return out;
  };

  // A PDF text string: UTF-16 after its byte order mark, else one byte a character.
  const text = (s) => (s[0] === 0xfe && s[1] === 0xff ? Buffer.from(s.subarray(2)).swap16().toString("utf16le") : s.toString("latin1"));

  // A content stream's text: a line for each line of text it sets, through the forms it draws.
  const draw = (content, resources, lines, seen = new Set()) => {
    const { value, skip } = parser(content);
    let font;
    let line = "";
    let lastY;
    const end = () => {
      if (line.trim()) lines.push(line.trim());
      line = "";
    };
    const stack = [];
    // Marked content open, innermost last: a span's actual text (PLAN 2.88) stands for the text
    // its glyphs show, as a reader that copies takes it.
    const marked = [];
    const actual = () => marked.some((m) => m !== undefined);
    for (let i = skip(0); i < content.length; i = skip(i)) {
      const [v, j] = value(i);
      i = j;
      if (!v || typeof v !== "object" || !("op" in v)) {
        stack.push(v);
        continue;
      }
      const args = stack.splice(0);
      switch (v.op) {
        case "BI": {
          // An inline image: skip to its end.
          const at = content.indexOf("EI", i);
          i = at < 0 ? content.length : at + 2;
          break;
        }
        case "Tf":
          font = get(get(get(resources)?.Font)?.[args[0]?.name]);
          break;
        case "Td":
        case "TD":
          if (args[1] !== 0) end();
          break;
        case "Tm":
          if (lastY !== undefined && args[5] !== lastY) end();
          lastY = args[5];
          break;
        case "T*":
        case "ET":
          end();
          lastY = undefined;
          break;
        case "BMC":
          marked.push(undefined);
          break;
        case "BDC": {
          const said = args[1]?.ActualText;
          marked.push(Buffer.isBuffer(said) && !actual() ? text(said) : undefined);
          break;
        }
        case "EMC": {
          const said = marked.pop();
          if (said !== undefined) {
            end();
            line = said;
            end();
          }
          break;
        }
        case "Tj":
        case "'":
        case '"':
          if (font && !actual()) line += decode(font, args.at(-1));
          break;
        case "TJ":
          if (font && !actual()) for (const part of args[0]) if (Buffer.isBuffer(part)) line += decode(font, part);
          break;
        case "Do": {
          const xobject = get(get(get(resources)?.XObject)?.[args[0]?.name]);
          if (xobject?.raw && xobject.dict.Subtype?.name === "Form" && !seen.has(xobject)) {
            end();
            draw(data(xobject), xobject.dict.Resources ?? resources, lines, new Set([...seen, xobject]));
          }
          break;
        }
      }
    }
    end();
    return lines;
  };

  const read = pages.map((page) => {
    const contents = [get(page.Contents)].flat().map(get);
    const lines = [];
    for (const c of contents) draw(data(c), page.Resources, lines);
    return { text: lines.join("\n"), size: page.MediaBox };
  });

  // The outline: each item's title and the items under it.
  const items = (first) => {
    const out = [];
    for (let item = get(first); item; item = get(item.Next)) out.push({ title: text(item.Title), kids: items(item.First) });
    return out;
  };
  const outline = trailer.Outlines ? items(get(trailer.Outlines).First) : [];
  return { pages: read, outline };
}
