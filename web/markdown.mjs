// The assistant's markdown, as the panel reads it (narrative ATTR-4): the blocks an answer
// splits into, and the elements they render as, held under Node with a stand-in for the DOM.
//
//   node --experimental-strip-types web/markdown.mjs   (from the repository's root)
//
// Exits 1 on any failure. The Mac's `MarkdownBlocks` splits the same text into the same blocks,
// which `MarkdownTests.swift` holds to the cases here.
import { isDeepStrictEqual } from "node:util";

/** The least of a DOM: elements that keep their children and text, and print as HTML. */
class Element {
  constructor(tag) {
    this.tagName = tag;
    this.children = [];
    this.dataset = {};
    this.className = "";
    this.onclick = null;
  }
  append(...nodes) {
    this.children.push(...nodes);
  }
  replaceChildren() {
    this.children = [];
  }
  get lastElementChild() {
    for (let i = this.children.length - 1; i >= 0; i--) if (this.children[i] instanceof Element) return this.children[i];
    return null;
  }
  set textContent(text) {
    this.children = [text];
  }
  get textContent() {
    return this.children.map((c) => (c instanceof Element ? c.textContent : c)).join("");
  }
  get html() {
    const attrs = [this.href && ` href="${this.href}"`, this.className && ` class="${this.className}"`].filter(Boolean).join("");
    return `<${this.tagName}${attrs}>${this.children.map((c) => (c instanceof Element ? c.html : c)).join("")}</${this.tagName}>`;
  }
}
globalThis.document = { createElement: (tag) => new Element(tag) };

const { blocks, render } = await import("./src/assistant/markdown.ts");

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

/** The cases the Mac's test shares: each text and its blocks. */
export const CASES = [
  {
    text: "Three things stand out:\n\n- The **cover** is clean.\n- `revenue` runs long.\n  - its figure wraps\n- The close claims nothing.\n\nFix the second first.",
    blocks: [
      { kind: "paragraph", text: "Three things stand out:" },
      { kind: "list", ordered: false, items: [{ level: 0, text: "The **cover** is clean." }, { level: 0, text: "`revenue` runs long." }, { level: 1, text: "its figure wraps" }, { level: 0, text: "The close claims nothing." }] },
      { kind: "paragraph", text: "Fix the second first." },
    ],
  },
  {
    text: "## What I changed\n1. Tightened the headline\n2. Moved the note\n\n```json\n[{ \"op\": \"set_text\" }]\n```\nDone.",
    blocks: [
      { kind: "heading", level: 2, text: "What I changed" },
      { kind: "list", ordered: true, items: [{ level: 0, text: "Tightened the headline" }, { level: 0, text: "Moved the note" }] },
      { kind: "code", text: '[{ "op": "set_text" }]', language: "json" },
      { kind: "paragraph", text: "Done." },
    ],
  },
  { text: "A line\nand the next.\r\n\r\nAnother.", blocks: [{ kind: "paragraph", text: "A line\nand the next." }, { kind: "paragraph", text: "Another." }] },
  { text: "```\nunclosed", blocks: [{ kind: "code", text: "unclosed" }] },
  { text: "", blocks: [] },
];

for (const { text, blocks: want } of CASES) {
  const got = blocks(text);
  check(isDeepStrictEqual(got, want), `blocks of ${JSON.stringify(text.slice(0, 40))}: ${JSON.stringify(got)}`);
}

const into = new Element("li");
render(CASES[0].text, into, { states: ["revenue", "cover"], show: () => {} });
check(
  into.html ===
    '<li><p>Three things stand out:</p><ul><li>The <strong>cover</strong> is clean.</li><li><a href="#revenue" class="state">revenue</a> runs long.<ul><li>its figure wraps</li></ul></li><li>The close claims nothing.</li></ul><p>Fix the second first.</p></li>',
  `rendered: ${into.html}`,
);
render("See [the cover](state:cover), *now*, at https://example.com/a, and `code`.", into, { states: ["cover"], show: () => {} });
check(
  into.html ===
    '<li><p>See <a href="#cover" class="state">the cover</a>, <em>now</em>, at <a href="https://example.com/a">https://example.com/a</a>, and <code>code</code>.</p></li>',
  `inline: ${into.html}`,
);
// A link says where it goes: its host after words that do not name it, its address as its title
// (lift-up 20261025 SEC-5).
render("[Re-enter your key](https://Evil.example:8443/k?x=1) or [example.com](https://www.example.com/a).", into);
const [planted, named] = into.children[0].children.filter((c) => c instanceof Element);
check(
  into.html ===
    '<li><p><a href="https://Evil.example:8443/k?x=1">Re-enter your key<span class="host"> (evil.example)</span></a> or <a href="https://www.example.com/a">example.com</a>.</p></li>' &&
    planted.title === "https://Evil.example:8443/k?x=1" &&
    named.title === "https://www.example.com/a",
  `a link says where it goes: ${into.html}`,
);
// The host said is the one the click opens, read by the browser's own parser, which takes `\`
// as `/`; and words that only hold part of a host do not hide it.
render("[Your account](https://evil.example\\@good.example/x) and [paypal.com](https://pal.com/)", into);
const [spoofed, part] = into.children[0].children.filter((c) => c instanceof Element);
check(
  spoofed.textContent === "Your account (evil.example)" && part.textContent === "paypal.com (pal.com)",
  `the host said is the one opened, never hidden by part of the words: ${into.html}`,
);
render("<b>not html</b> & **bold**", into);
check(into.html === "<li><p><b>not html</b> & <strong>bold</strong></p></li>" && into.textContent === "<b>not html</b> & bold", `the model's text is text, never markup: ${into.html}`);
render("[js](javascript:alert(1)) and [state](state:nowhere)", into, { states: ["cover"], show: () => {} });
check(into.html === "<li><p><span>js</span> and <span>state</span></p></li>", `a link to nowhere is its words: ${into.html}`);

if (failures.length) {
  console.log(`\n${failures.length} failed`);
  process.exit(1);
}
