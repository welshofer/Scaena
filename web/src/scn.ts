// The `.scn` language for CodeMirror 6 (PLAN 2.3, SPEC §4): its tokens as the lexer
// (`scaena-core::dsl::lex`) reads them, for highlighting. Compiling is the engine's: the
// editor never parses `.scn` itself.
import { HighlightStyle, StreamLanguage, type StringStream } from "@codemirror/language";
import { tags } from "@lezer/highlight";

/** Words that start a declaration, or a line in one. */
const KEYWORDS = new Set([
  "deck",
  "font",
  "data",
  "node",
  "override",
  "section",
  "state",
  "beat",
  "notes",
  "choreo",
  "sequence",
  "parallel",
  "media",
  "props",
]);

/** Node types, as a declaration names them. */
const TYPES = new Set(["text", "image", "shape", "chart", "table", "shader", "stack", "grid", "frame", "group"]);

interface State {
  /** Inside a triple-quoted string that runs on past its line. */
  triple: boolean;
}

/** The rest of a triple-quoted string from where `stream` is: through its closing `"""`,
 * or to the line's end. */
function triple(stream: StringStream, state: State): string {
  while (!stream.eol()) {
    if (stream.match('"""')) {
      state.triple = false;
      return "string";
    }
    stream.next();
  }
  return "string";
}

export const scn = StreamLanguage.define<State>({
  name: "scn",
  startState: () => ({ triple: false }),
  copyState: (s) => ({ ...s }),
  token(stream, state) {
    if (state.triple) return triple(stream, state);
    if (stream.eatSpace()) return null;
    if (stream.peek() === "#") {
      stream.skipToEnd();
      return "comment";
    }
    if (stream.match('"""')) {
      state.triple = true;
      return triple(stream, state);
    }
    if (stream.peek() === '"') {
      stream.next();
      let escaped = false;
      while (!stream.eol()) {
        const c = stream.next();
        if (c === '"' && !escaped) break;
        escaped = c === "\\" && !escaped;
      }
      return "string";
    }
    // A ratio (16:9), a size (1920x1080), a range (1-7), a time (300ms, 4s), a percentage,
    // or a number in canvas units (12cu).
    if (stream.match(/^-?\d+(\.\d+)?(:\d+(\.\d+)?|x\d+|-\d+|ms|s|cu|%)?(?![A-Za-z_])/)) return "number";
    if (stream.match(/^@[A-Za-z_][A-Za-z0-9_.-]*/)) return "variableName.special";
    const word = stream.match(/^[A-Za-z_][A-Za-z0-9_.-]*/) as RegExpMatchArray | null;
    if (word) {
      const w = word[0];
      if (stream.peek() === ":") return "propertyName";
      if (w === "true" || w === "false" || w === "null") return "atom";
      if (KEYWORDS.has(w) && stream.column() - w.length === stream.indentation()) return "keyword";
      if (TYPES.has(w)) return "typeName";
      return "variableName";
    }
    const c = stream.next();
    return c === "-" || c === "=" ? "operator" : "punctuation";
  },
  languageData: { commentTokens: { line: "#" } },
});

/** Its colors, on the editor's dark surface. */
export const scnHighlight = HighlightStyle.define([
  { tag: tags.keyword, color: "#e0a96d", fontWeight: "600" },
  { tag: tags.string, color: "#a8c99a" },
  { tag: tags.number, color: "#9cc4e4" },
  { tag: tags.comment, color: "#8a8780", fontStyle: "italic" },
  { tag: tags.propertyName, color: "#c9b8e8" },
  { tag: tags.typeName, color: "#e8c46d" },
  { tag: tags.atom, color: "#9cc4e4" },
  { tag: tags.special(tags.variableName), color: "#e89a9a" },
  { tag: tags.operator, color: "#d6d3cc" },
  { tag: tags.punctuation, color: "#a9a59d" },
]);
