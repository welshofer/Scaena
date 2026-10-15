// A chart annotated from its marks (PLAN 2.67, SPEC §3.7): a mark picked on the canvas offers to
// highlight it, to call it out with words typed there, to rule its value, or to band a range from
// it to another mark. Each is one `annotate` op (§7.3), which writes the chart's `annotations`
// where they live, as a choice in the inspector is written. The engine says where each kind made
// from a mark stands (`Player.markAt`'s `notes`), and which annotation is drawn where
// (`Player.noteAt`); this module makes the ops, names what they are about, and asks for the words.

import type { DataMark, MarkNotes, NoteMark } from "./protocol";

/** A mark as a person names it: its key's parts, `2026-Q3 · Pro`. */
export const markName = (m: DataMark) => m.key.split("\u001f").join(" · ");

/** An annotation as a person names it: `the callout`, `the rule`, `the band`. */
export const noteName = (n: NoteMark) => `the ${n.kind}`;

/** One `annotate` op on chart `node` in `state`: `annotation` added, or with `index`, merged into
 * the one there, or `null` there, that one taken away; kept to `state` with `fork`. */
export function annotate(node: string, state: string, annotation: Record<string, unknown> | null, index?: number, fork = false) {
  return { op: "annotate", node, state, ...(index === undefined ? {} : { index }), annotation, ...(fork ? { fork: true } : {}) };
}

/** What a mark picked offers, each the annotation it adds. */
export const highlight = (n: MarkNotes) => ({ kind: "highlight", at: n.highlight });
export const highlightSeries = (n: MarkNotes) => ({ kind: "highlight", at: { series: n.series } });
export const callout = (n: MarkNotes, text: string) => ({ kind: "callout", at: n.callout, text });
export const rule = (n: MarkNotes) => ({ kind: "rule", at: { y: n.value } });

/** A band from mark `a` to mark `b`: across their x, or, where they share it, from one's value to
 * the other's. */
export function band(a: MarkNotes, b: MarkNotes) {
  if (a.x === b.x) return { kind: "band", at: { y: [Math.min(a.value, b.value), Math.max(a.value, b.value)] } };
  return { kind: "band", at: { x: [a.x, b.x] } };
}

/** The highlights that pick `n`'s mark out, taken away: last first, so each index still names its
 * annotation when its op applies. */
export const unhighlight = (node: string, state: string, n: MarkNotes, fork = false) =>
  [...n.highlighted].sort((a, b) => b - a).map((i) => annotate(node, state, null, i, fork));

/** What a field that asks for words shows beside them: a caption over it, which a person reads
 * before typing; a placeholder in it while it is empty; where it goes under the words it is about
 * (client pixels), where there is no room above them; and a `check` of what Enter gives, which
 * says why it will not do and keeps the field open. */
export interface Asking {
  caption?: string;
  placeholder?: string;
  below?: number;
  check?: (words: string) => string | undefined;
}

/** Ask for words at `at` (client pixels), in a field over the canvas with `initial` in it: Enter, or
 * leaving the field, gives what it holds; Escape gives nothing. */
export function askWords(at: [number, number], initial: string, label: string, asking: Asking = {}): Promise<string | undefined> {
  return new Promise((resolve) => {
    const box = document.createElement("label");
    box.className = "note-words-box";
    const caption = box.appendChild(document.createElement("span"));
    caption.className = "caption";
    caption.textContent = asking.caption ?? "";
    caption.hidden = !asking.caption;
    const field = box.appendChild(document.createElement("input"));
    field.type = "text";
    field.className = "note-words";
    field.value = initial;
    field.placeholder = asking.placeholder ?? "";
    field.setAttribute("aria-label", label);
    // Above the words it is about, else under them, so they stay in sight.
    const under = asking.below !== undefined && at[1] < 64;
    box.classList.toggle("below", under);
    box.style.left = `${Math.round(at[0])}px`;
    box.style.top = `${Math.round(under ? asking.below! : at[1])}px`;
    document.body.append(box);
    let done = false;
    const finish = (words: string | undefined) => {
      if (done) return;
      done = true;
      box.remove();
      resolve(words);
    };
    field.addEventListener("keydown", (e) => {
      // The canvas's keys stay out of what is typed here.
      e.stopPropagation();
      if (e.key === "Enter") {
        e.preventDefault();
        const why = asking.check?.(field.value);
        if (why === undefined) return finish(field.value);
        caption.textContent = why;
        caption.hidden = false;
        field.setAttribute("aria-invalid", "true");
        field.setAttribute("aria-description", why);
      } else if (e.key === "Escape") {
        e.preventDefault();
        finish(undefined);
      }
    });
    field.addEventListener("input", () => {
      if (field.getAttribute("aria-invalid") !== "true") return;
      field.removeAttribute("aria-invalid");
      field.removeAttribute("aria-description");
      caption.textContent = asking.caption ?? "";
      caption.hidden = !asking.caption;
    });
    field.addEventListener("blur", () => finish(field.value));
    field.focus();
    field.select();
  });
}
