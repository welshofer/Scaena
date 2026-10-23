// The inspector's edits (PLAN 2.33, ADR-0013): the node selected, its role, style, presets, and
// props, chosen from what the theme names for its type, or what the schema allows. The engine
// says what it offers, the value the state shows, and where that value lives (`Player.choices`),
// each in a person's words (PLAN 2.99): the node named by what it shows, each property labeled as
// a presentation app labels it, each value said, a slide by its number. The page lays nothing
// out and keeps no vocabulary of its own.
//
// With no node selected, the state shown (PLAN 2.36, `Player.stateChoices`): its layout, from the
// theme's layouts with a slot for each node placed in one, written where it lives, so the states
// that take it from there change with it; its transition's duration, ease, spring, and match; its
// hold; and its notes. Each is one `set_state` patch. Above them, the layouts it may take (PLAN
// 2.92, `Player.layoutSuggestions`), each drawn small at rest and judged by lint in every format
// the deck lists, fewest errors first, then fewest warnings: a pointer over one, or the focus on
// it, shows the state laid out in it on the canvas, nothing made (`Player.preview`), and a click
// gives the state that layout as the layout field does. Layouts that draw the state alike are one
// drawing, which names the others; a state that places no node in a slot is offered none.
//
// - Each choice is one `choose` patch, by the user, written where the value lives: the state
//   that sets it, or the node. The status says which states it changes, and it is one step to
//   undo. "Only on Slide N" keeps it to the state shown (`fork`).
// - A value written out where the theme has names (a color, a text size, a length in canvas
//   units) goes in the deck's `overrides`, the only place it is legal, and shows as an override,
//   in every state.
// - The × beside a value takes it away where it lives, so what is under it shows.
//
// With several nodes selected (PLAN 2.42), what they share: each property all of them offer alike,
// its value where they agree, and a choice is a `choose` for each, one patch. Buttons align them on
// an edge or a middle, spread them so the gaps between them are equal, and put them in front of or
// behind what they overlap (`Player.arranging`), as they do for one node.
//
// What a reader hears (PLAN 2.56, SPEC §3.12): every node offers its description (`alt`) and its
// part in the story (`semantic`), each a `choose` as any choice is. Under the choices, how the
// state shown reads, as the player's live region and the PDF's tags say it (`Player.reading`): for
// the node shown, what it reads as; for the state, each part in order, each selecting its node.
//
// Writing (PLAN 2.101): a text offers its words proofread, tightened, or rewritten, and the state
// its notes drafted, by the model the assistant uses, each answer offered to take or leave.
//
// The Animate panel (lift-up ATTR-4, PLAN 3.18) shows what the Mac's Animate tab shows, from the same
// choices: for the slide, its transition and its hold (Advance); for an object, its builds, as it
// enters, for emphasis, and as it leaves; and nothing for the characters selected. Each choice is
// made as Format's are. A button there shows the slide's animations on the timeline under the canvas.
//
// With characters selected in a text typed in (PLAN 2.38, `Player.characterChoices`), their look:
// a run's role, emphasis, family, weight, italic (PLAN 2.40), and color, each the first
// character's. Each choice is one `style_text`, written where the text lives; × takes the run's
// own away, so the text's look shows there. A run takes the theme's names only.
import { listen } from "./life";
import type { Arrange, Choices, Edited, Field, LayoutSuggestion, Lives, StateChoices, WritingKind } from "./protocol";
import type { Stage } from "./stage";
import type { Selected } from "./typing";
import { counted, fractions, PARTS, quote, reaching, seconds } from "./words";
import { backgrounds } from "./backgrounds";

/** What the inspector's edits ask of the editor around them. */
export interface Around {
  /** The state shown, by its id and its slot's index. */
  shown(): { state: string; index: number } | undefined;
  /** `state` as a person counts it: `Slide 3, step 2` (PLAN 2.99). */
  slide(state: string): string;
  format(): string | undefined;
  source(): string;
  /** Take `source`, the patch made: one step to undo. */
  apply(source: string, edited: Edited): void;
  say(text: string): void;
  /** Give the characters selected in the text typed in `look` (`style_text`): whether it was. */
  style(look: Record<string, unknown>, words?: string): Promise<boolean>;
  /** Arrange what the canvas selects `how` (PLAN 2.42). */
  arrange(how: Arrange): Promise<void>;
  /** Put what the canvas selects in a new group, and take the group it selects apart (PLAN 2.43). */
  group(): Promise<void>;
  ungroup(): Promise<void>;
  /** Pick the focal point of the image the canvas selects: the next press on it (PLAN 2.45). */
  pick(): void;
  /** Select `node` on the canvas, as a click on it does: a part of the reading (PLAN 2.56). */
  select(node: string): void;
  /** One of `names` chosen from the theme's layouts by their sections, `current` outlined (PLAN
   * 3.31): undefined where none is. */
  pickLayout(names: string[], current?: string): Promise<string | null | undefined>;
  /** Ask the model the assistant uses to write `kind` for `node`, a text, or for the state shown
   * (PLAN 2.101): offered to take or leave. */
  write(kind: WritingKind, node?: string): void;
  /** Fetch the map image `node` shows again (ADR-0030, PLAN O.1c): `in` or `out` a step, centered
   * on its pin, or as it was asked. */
  remap?(node: string, how: "in" | "out" | "pin" | "again"): void;
  /** Show the tab a field stands in, Format or Animate (lift-up ATTR-4). */
  reveal?(tab: "format" | "animate"): void;
  /** The slide's animations on the timeline under the canvas: shown or hidden with `show`; whether
   * they are shown. */
  timeline?(show?: boolean): boolean;
}

/** The fields the Animate tab takes from a slide's choices and from an object's (lift-up ATTR-4), as
 * the Mac's does: the slide's transition and hold, and an object's builds. */
export const animated = (prop: string, node: boolean) =>
  node ? ["enter", "emphasis", "exit"].includes(prop) : prop === "hold" || prop.startsWith("transition/");

/** What fetches a map again (PLAN O.1c), and what each does: each one fetch, one step to undo. */
const MAPPING: ["in" | "out" | "pin" | "again", string, string][] = [
  ["in", "Zoom In", "Fetched again a step nearer, about where it is centered"],
  ["out", "Zoom Out", "Fetched again a step further, about where it is centered"],
  ["pin", "Center on the Pin", "Fetched again centered on the place its pin marks"],
  ["again", "Fetch Again", "Fetched again as it was asked: the map as Geoapify draws it now"],
];

/** The writing tools a text offers, and what each does (PLAN 2.101). */
const WRITING: [WritingKind, string, string][] = [
  ["proofread", "Proofread", "Its spelling, grammar, and punctuation put right, and nothing else"],
  ["tighten", "Tighten", "The same meaning in fewer words"],
  ["rewrite", "Rewrite", "Said more clearly and directly"],
];

/** What several nodes offer alike: each field all of them have with the same choices, its value
 * where they agree. */
type Shared = Choices & { nodes: string[] };

function shared(all: Choices[]): Shared {
  const [first, ...rest] = all;
  const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
  const alike = (f: Field) => rest.every((c) => c.fields.some((g) => g.prop === f.prop && same(g.takes, f.takes)));
  const fields = first.fields.filter(alike).map((f) => {
    const agree = rest.every((c) => same(c.fields.find((g) => g.prop === f.prop)?.value, f.value));
    return agree ? f : { prop: f.prop, takes: f.takes, words: f.words };
  });
  const types = [...new Set(all.map((c) => c.type))];
  return { ...first, type: types.join(", "), fields, nodes: all.map((c) => c.node) };
}

/** What each kind of arranging is called. */
const ARRANGING: Record<string, string> = { align: "Align", spread: "Distribute", order: "Arrange", group: "Group" };

/** The buttons that arrange what is selected: each how, and what it says. */
const ARRANGE: [string, Arrange, string][] = [
  ["align", { align: "left" }, "Left"],
  ["align", { align: "center" }, "Center"],
  ["align", { align: "right" }, "Right"],
  ["align", { align: "top" }, "Top"],
  ["align", { align: "middle" }, "Middle"],
  ["align", { align: "bottom" }, "Bottom"],
  ["spread", { spread: "across" }, "Across"],
  ["spread", { spread: "down" }, "Down"],
  ["order", { order: "front" }, "To front"],
  ["order", { order: "forward" }, "Forward"],
  ["order", { order: "backward" }, "Backward"],
  ["order", { order: "back" }, "To back"],
];

const html = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
const said = (e: unknown) => (e instanceof Error ? e.message : String(e));


/** How high a suggested layout is drawn, in CSS pixels (PLAN 2.92). */
const PICTURE = 72;

/** The layouts a state may take, as last suggested, each with its picture (PLAN 2.92). */
interface Suggested {
  state: string;
  format: string | undefined;
  layouts: { suggestion: LayoutSuggestion; picture: ImageData }[];
}

/** A part of how a state reads (PLAN 2.56): the node it names, what kind of part it is, and what a
 * reader hears of it. */
function part(el: HTMLElement): { node: string; kind: string; says: string } {
  const node = el.dataset.node ?? "";
  const label = el.getAttribute("aria-label");
  const quoted = (words: string) => `“${words.trim()}”`;
  switch (el.tagName) {
    case "H1":
    case "H2":
      return { node, kind: el.tagName === "H1" ? "Heading" : "Subheading", says: quoted(el.textContent ?? "") };
    case "P":
      return { node, kind: "Text", says: quoted(el.textContent ?? "") };
    case "TABLE": {
      const rows = el.querySelectorAll("tr").length;
      return { node, kind: "Table", says: `${label ? `${quoted(label)}, ` : ""}${rows} row${rows === 1 ? "" : "s"}` };
    }
    case "DIV":
      // A text whose paragraphs are a list's items (PLAN 2.69) reads as its list, item by item,
      // each without the items under it; a figure is the `div` that is an image.
      if (el.getAttribute("role") !== "img") {
        const own = (li: Element) =>
          [...li.childNodes].filter((n) => !(n instanceof HTMLElement && /^(UL|OL)$/.test(n.tagName))).map((n) => n.textContent ?? "").join("");
        const items = [...el.querySelectorAll("li")].map((li) => own(li).trim()).filter(Boolean);
        return { node, kind: "List", says: quoted(items.length ? items.join(" · ") : (el.textContent ?? "")) };
      }
      return { node, kind: "Picture", says: label ? quoted(label) : "not described" };
    default:
      return { node, kind: "Picture", says: label ? quoted(label) : "not described" };
  }
}

/** A property as a person labels it (PLAN 2.99): `style/family` is Font. */
const labeled = (f: Field) => f.words?.label ?? f.prop;

/** What has none where the deck sets nothing (PLAN 3.18): a chart's or a table's source, a chart's
 * channel that reads no column (no series, no small multiples), a picture's mask, and a shadow. The
 * choice that takes it away is None, as a presentation app calls it, not Default, unless the theme
 * gives one; the Mac's inspector calls it so (`Inspector.nothing`). */
const NOTHING = new Set([
  "data",
  "x/field",
  "y/field",
  "series/field",
  "color/field",
  "sizeEncoding/field",
  "interval/low",
  "interval/high",
  "facet/field",
  "mask",
  "shadow",
]);

/** A build written as a call to a preset (`{ preset, delay }`), as a drag of its bar in the cue
 * writes it (SPEC §7.3): the preset it plays. */
const called = (v: unknown): { preset: string; delay?: number } | undefined =>
  v !== null && typeof v === "object" && typeof (v as { preset?: unknown }).preset === "string" ? (v as { preset: string; delay?: number }) : undefined;

/** A value of `f`'s as a person says it (PLAN 2.99): one of its names or words as the engine says
 * it (`onSurface` is Text), a weight by its name, yes or no; a build called with a setting by its
 * preset and its wait; a value written out, a color or a gradient, is the deck's own. */
function spoken(f: Field, v: unknown): string {
  const call = called(v);
  if (call) {
    const name = f.words?.said[call.preset] ?? call.preset;
    return typeof call.delay === "number" && call.delay > 0 ? `${name} · waits ${seconds(call.delay)}` : name;
  }
  if (typeof v === "string") return f.words?.said[v] ?? (v.startsWith("#") ? v : "Custom");
  if (typeof v === "boolean") return v ? "Yes" : "No";
  if (typeof v === "number") return f.words?.said[String(v)] ?? String(Math.round(v * 1000) / 1000);
  return "Custom";
}

/** An adjustment from -1 to 1 as Adjust Image's slider reads it, and the status says it: "None",
 * or a signed percent. */
function adjustment(value: number): string {
  return value === 0 ? "None" : `${value > 0 ? "+" : "−"}${Math.round(Math.abs(value) * 100)}%`;
}

/** Each part of an image's crop or focal point as its field is captioned: the crop by its edges
 * and its size, the focal point by how far across and down. */
const CAPTIONS: Record<string, Record<string, string>> = {
  crop: { x: "Left", y: "Top", w: "Width", h: "Height" },
  focal: { x: "Across", y: "Down" },
};

/** Where a value lives, as a person says it: on the slide shown (`shown`), on another (`slide`
 * says which), on the object, on the object on every slide, or the theme's. */
function where(lives: Lives | undefined, shown: string | undefined, slide: (state: string) => string): string {
  if (lives === undefined) return "The theme's";
  if (lives === "overrides") return "Set on this object, on every slide";
  if (lives === "node") return "Set on this object";
  return lives.state === shown ? "Set on this slide" : `Set on ${slide(lives.state)}`;
}

/** A value as the status says it: a hold in seconds, a turn in degrees, words written (a
 * description, notes) quoted, the rest as `spoken`. */
const told = (f: Field, v: unknown) =>
  f.prop === "hold" && typeof v === "number"
    ? `after ${Math.round(v) / 1000} s`
    : f.prop === "transform/rotate" && typeof v === "number"
      ? `${v}°`
      : f.prop.startsWith("adjust/") && typeof v === "number"
        ? adjustment(v)
      : f.takes.kind === "text" && typeof v === "string"
        ? v.trim()
          ? quote(v)
          : "None"
        : f.takes.kind === "fractions" && Array.isArray(v)
          ? fractions(f.takes.names, v as number[])
          : spoken(f, v);

/** The inspector's edits, in `into`, for the node the canvas selects in `stage`'s state shown, or
 * the state itself when it selects none. */
export function looks(stage: Stage, into: HTMLElement, around: Around, animating: HTMLElement = document.createElement("div")) {
  /** Both panels, Format's and Animate's. */
  const panels = [into, animating];
  const find = <T extends Element>(selector: string) => into.querySelector<T>(selector) ?? animating.querySelector<T>(selector);
  /** What the deck offers for the node shown in the state shown, or, with none, for the state;
   * or for the characters selected in a text typed in (`chars`). */
  let offered: Choices | StateChoices | undefined;
  /** The characters selected in a text typed in, whose look is offered. */
  let chars: Selected | undefined;
  /** The node shown when no characters are. */
  let node: string | undefined;
  /** Selected beside it (PLAN 2.42). */
  let beside: string[] = [];
  /** Choices kept to the state shown (`choose`'s `fork`). */
  let keep = false;
  /** One choice at a time, each made on the source the one before it left. */
  let queue: Promise<unknown> = Promise.resolve();
  let asked = 0;
  /** What the controls show: drawn again only when it changes, so that a control in use (a
   * color picker open, a change on its way) is not swapped out from under it by an edit's
   * inspection that offers the same. */
  let drawn = "";
  /** A field to focus once the inspector shows `node` (PLAN 2.56): a finding's mark asks for it. */
  let pending: { node: string; prop: string } | undefined;
  /** Readings asked for: only the latest is shown. */
  let read = 0;
  /** The layouts suggested for the state shown (PLAN 2.92), asked for once edits pause: only the
   * latest is shown. */
  let suggested: Suggested | undefined;
  let suggesting = 0;
  let soon: ReturnType<typeof setTimeout> | undefined;
  /** The layout the canvas shows the state laid out in, and the one it is to show: a preview at a
   * time, the last asked for winning. */
  let previewed: string | undefined;
  let previewing: string | undefined;
  let previews: Promise<void> | undefined;
  /** The slide's background, offered with nothing selected (PLAN 2.148). */
  const backdrops = backgrounds(stage, around);
  backdrops.listen(into);

  /** Show what the deck offers for node `next` in the state shown, with `also` selected beside it,
   * or for the state with none. */
  async function show(next: string | undefined, also: string[] = []) {
    node = next;
    beside = next === undefined ? [] : also;
    if (chars) return;
    const now = around.shown();
    const ask = ++asked;
    // A node the state does not show offers nothing: the state shows instead.
    const state = (s: string) => stage.stateChoices(s).catch(() => undefined);
    let found: Choices | StateChoices | undefined;
    if (now && next !== undefined && beside.length) {
      const all = await Promise.all([next, ...beside].map((n) => stage.choices(now.state, n).catch(() => undefined)));
      found = all.every((c) => c !== undefined) ? shared(all as Choices[]) : await state(now.state);
    } else if (now) found = await (next !== undefined ? stage.choices(now.state, next).catch(() => state(now.state)) : state(now.state));
    if (ask !== asked) return;
    offered = found;
    // Notes being written are kept: the change they make draws the inspector again.
    const at = document.activeElement;
    if (at instanceof HTMLTextAreaElement && into.contains(at) && at.value !== at.defaultValue) return;
    // What the state reads may have changed though nothing offered did: a text typed in elsewhere.
    if (!render() && into.querySelector(".reads")) void reads(next !== undefined && !beside.length ? next : undefined);
  }

  /** Show what the deck offers for the characters `selected`, or, with none, for the node. */
  async function characters(selected: Selected | undefined) {
    chars = selected;
    if (!selected) return show(node);
    const ask = ++asked;
    const found = await stage.characterChoices(selected.state, selected.node, selected.from, selected.to).catch(() => undefined);
    if (ask !== asked || chars !== selected) return;
    offered = found;
    render();
  }

  /** The state shown cuts in: it sets no key of its transition. */
  const cuts = () => !("node" in (offered ?? {})) && (offered?.fields ?? []).every((f) => !f.prop.startsWith("transition/") || f.value === undefined);

  /** The control for field `f`, its id `id`. */
  function control(f: Field, id: string): string {
    const prop = `data-prop="${html(f.prop)}"`;
    // A build called with a setting is the preset it plays, its option chosen.
    const value = called(f.value)?.preset ?? f.value;
    const label = labeled(f);
    // A value no control here sets (a gradient) shows as the deck's own.
    const kept = (known: unknown[]) =>
      value !== undefined && !known.includes(value) ? `<option value="" selected disabled>${html(spoken(f, value))}</option>` : "";
    // What shows where nothing sets it: the theme's, by what it gives a text's look.
    const gives = f.theme === undefined ? "" : ` (${spoken(f, f.theme)})`;
    // Where nothing set leaves none, the choice that takes the value away is None (`NOTHING`), as
    // the Mac's `PickRow` says it; where the theme gives one, Default is called by it.
    const nothing = NOTHING.has(f.prop) && !gives;
    const unset =
      f.prop === "transition/duration" && cuts() ? "None: it cuts in" : f.prop === "layout" || nothing ? "None" : `Default${gives}`;
    const none = `<option value=""${value === undefined ? " selected" : ""}>${value === undefined || gives || nothing ? html(unset) : "—"}</option>`;
    const options = (all: string[]) =>
      all.map((o) => `<option value="${html(o)}"${o === value ? " selected" : ""}>${html(f.words?.said[o] ?? o)}</option>`).join("");
    const t = f.takes;
    // The state's layout opens the theme's layouts by their sections, each drawn (PLAN 3.31).
    if (f.prop === "layout" && t.kind === "name") {
      const now = typeof value === "string" ? spoken(f, value) : "None";
      return `<button type="button" id="${id}" ${prop} data-pick-layout title="The theme's layouts by their sections, each drawn as it is used">${html(now)} ▾</button>`;
    }
    switch (t.kind) {
      case "name": {
        // A value written out (one of its own, on every slide) shows as it is, beside the theme's names.
        let select = `<select id="${id}" ${prop}>${none}${options(t.names)}${kept(t.names)}</select>`;
        const written = `id="${id}-written" ${prop} title="One of its own, on every slide"`;
        if (t.overrides && t.of === "color") {
          // The well shows the color in use: one written out, or the one the theme's name is,
          // the value's or, where nothing sets it, the theme's (PLAN 2.99).
          const shown = typeof value === "string" ? value : typeof f.theme === "string" ? f.theme : undefined;
          const own = shown !== undefined && /^#[0-9a-f]{6}$/i.test(shown) ? shown : undefined;
          const hex = (own ?? (shown !== undefined ? f.words?.colors?.[shown] : undefined) ?? "#000000").toLowerCase();
          select += ` <input type="color" ${written} value="${hex}" aria-label="${html(label)} of its own, on every slide">`;
        } else if (t.overrides) {
          const units = typeof value === "number" ? String(value) : "";
          select += ` <input type="number" ${written} min="0" step="any" value="${units}" placeholder="pt" aria-label="${html(label)} of its own in points, on every slide">`;
        }
        return select;
      }
      case "word":
        return `<select id="${id}" ${prop}>${none}${options(t.words)}${kept(t.words)}</select>`;
      case "flag": {
        const yes = value === true ? " selected" : "";
        const no = value === false ? " selected" : "";
        return `<select id="${id}" ${prop} data-flag>${none}<option value="yes"${yes}>Yes</option><option value="no"${no}>No</option></select>`;
      }
      case "number": {
        // A hold is milliseconds; a person sets it in seconds, and none advances on a click.
        if (f.prop === "hold") {
          const shown = typeof value === "number" ? String(value / 1000) : "";
          return `<input id="${id}" type="number" ${prop} data-scale="1000" min="0" step="0.5" value="${shown}" placeholder="On click" aria-label="Advance after, in seconds"> <span class="lives">s</span>`;
        }
        // How far a node is turned (PLAN 2.51): degrees, clockwise; none is upright.
        if (f.prop === "transform/rotate") {
          const shown = typeof value === "number" ? String(value) : "";
          return `<input id="${id}" type="number" ${prop} step="1" value="${shown}" placeholder="0" aria-label="Rotation, degrees clockwise"> <span class="lives">°</span>`;
        }
        // An image's light and color (PLAN 2.117), as Adjust Image's sliders: from -1 to 1, set
        // when let go, the readout following the drag; none is the picture as it is. Let go at
        // none, it takes away the object's own; written on a slide, it is written as none there,
        // so that what the object or an earlier slide sets does not show through.
        if (f.prop.startsWith("adjust/")) {
          const now = typeof value === "number" ? value : 0;
          const away = f.lives === undefined || f.lives === "node" ? " data-away-at-none" : "";
          const said = adjustment(now);
          return `<input id="${id}" type="range" ${prop}${away} min="-1" max="1" step="0.01" value="${now}" aria-label="${html(label)}" aria-valuetext="${said}"> <span class="lives">${said}</span>`;
        }
        // Tracking, in em after each character (PLAN 1.38): on a pair's first letter, the space
        // between the two, on top of the font's kerning.
        if (f.prop === "style/tracking") {
          const shown = typeof value === "number" ? String(value) : "";
          return `<input id="${id}" type="number" ${prop} step="0.01" value="${shown}" placeholder="Default${html(gives)}" aria-label="${html(label)}, em"> <span class="lives">em</span>`;
        }
        const min = t.min ?? t.above;
        const bounds = `${min !== undefined ? ` min="${min}"` : ""}${t.max !== undefined ? ` max="${t.max}"` : ""}`;
        const step = t.whole ? "1" : t.max !== undefined && t.max <= 1 ? "0.05" : "any";
        const shown = typeof value === "number" ? String(value) : "";
        return `<input id="${id}" type="number" ${prop}${bounds} step="${step}" value="${shown}" placeholder="Default${html(gives)}">`;
      }
      case "text": {
        // A text says its words unless it is described; anything else says nothing until it is.
        const words = offered && "node" in offered && offered.type === "text" ? "Its words" : "Describe it for someone who can’t see it";
        const [rows, empty] = f.prop === "alt" ? [2, words] : [3, "Notes you see as you present"];
        return `<textarea id="${id}" ${prop} rows="${rows}" placeholder="${empty}">${html(typeof value === "string" ? value : "")}</textarea>`;
      }
      case "fractions": {
        // An image's focal point or crop (PLAN 2.45): what shows where nothing sets it. Each part
        // is a share of the picture, in percent, under its caption, as the status says it.
        const rest = f.prop === "crop" ? [0, 0, 1, 1] : [0.5, 0.5];
        const now = Array.isArray(value) && value.length === t.names.length ? (value as number[]) : rest;
        const inputs = t.names.map((name, k) => {
          const part = PARTS[name] ?? name;
          const caption = CAPTIONS[f.prop]?.[name] ?? part;
          const pct = Math.round(now[k] * 1000) / 10;
          return `<label class="part"><span>${html(caption)}</span><input${k === 0 ? ` id="${id}"` : ""} type="number" class="fraction" ${prop} data-part="${k}" min="0" max="100" step="0.1" value="${pct}" aria-label="${html(label)}, ${part}, percent" title="${html(caption)}, as a share of the picture">%</label>`;
        });
        // The focal point is picked on the image itself: where its subject is.
        const pick = f.prop === "focal" ? ' <button type="button" data-pick title="Then click the picture where its subject is">Pick</button>' : "";
        return `${inputs.join("")}${pick}`;
      }
    }
  }

  /** What `f`'s value is, where it lives, and the × that takes it away there. */
  function note(f: Field): string {
    const shown = around.shown()?.state;
    // A value written out where the theme has a name, outside what an object sets on every slide.
    const flag =
      f.literal && f.lives !== "overrides"
        ? ' <span class="flag" title="Written out where the theme has a name for it: a change of theme leaves it as it is">Not the theme’s</span>'
        : "";
    const at = where(f.lives, shown, around.slide);
    const away =
      f.lives === undefined
        ? ""
        : ` <button type="button" class="away" data-away="${html(f.prop)}" aria-label="Take away ${html(labeled(f))}: ${html(at)}" title="Take it away where it is set">×</button>`;
    // A state's own property set nowhere: a cut, or none. Characters with no look of their own
    // show the text's.
    const lives =
      chars && f.lives === undefined
        ? "The text’s"
        : f.lives !== undefined || "node" in (offered ?? {})
          ? at
          : f.prop.startsWith("transition/")
            ? cuts()
              ? "Cuts in"
              : "The theme's"
            : "None";
    return `<span class="lives">${html(lives)}</span>${flag}${away}`;
  }

  /** Draw what is offered, the control that had focus keeping it: when it changed, or, `again`,
   * to put the controls back to what the deck says. Whether it drew. */
  function render(again = false): boolean {
    const now = around.shown();
    if (!offered || !now) {
      drawn = "";
      into.replaceChildren();
      return false;
    }
    const showing = JSON.stringify([now.state, keep, offered, chars]);
    if (showing === drawn && !again) return false;
    drawn = showing;
    const focused = panels.some((p) => p.contains(document.activeElement)) ? document.activeElement?.id : undefined;
    // Words typed in a field and not yet taken (its change comes when it is left) outlast a
    // render, which the deck changing under it brings: the field drawn again keeps them. So does
    // what is selected in it, so what is typed next replaces the selection. A render that puts the
    // controls back, as a change refused asks, keeps none: the field shows what the deck says, and
    // leaving it sends nothing again.
    const active = document.activeElement;
    const typing =
      !again && (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement) && active.id && panels.some((p) => p.contains(active))
        ? { id: active.id, value: active.value, typed: active.value !== active.defaultValue, from: active.selectionStart, to: active.selectionEnd }
        : undefined;
    const row = (f: Field) => {
      const id = `look-${f.prop.replace(/\//g, "-")}`;
      return `<label for="${id}">${html(labeled(f))}</label><span class="control">${control(f, id)}</span><span>${note(f)}</span>`;
    };
    // The Animate tab takes the slide's transition and hold, and an object's builds (ATTR-4).
    const isNode = "node" in offered;
    const builds = chars ? [] : offered.fields.filter((f) => animated(f.prop, isNode));
    const rows = offered.fields.filter((f) => !builds.includes(f)).map(row);
    const nodes = "nodes" in offered ? (offered as Shared).nodes : "node" in offered ? [offered.node] : [];
    // What is shown, as a person names it (PLAN 2.99): the object by its kind and what it shows,
    // the characters selected in it, several by how many, or the slide by its number.
    const said = offered.said;
    const noun = "noun" in offered ? offered.noun : undefined;
    const count = chars ? chars.to - chars.from : 0;
    const [title, named] = chars
      ? ["Selected Text", `${count} character${count === 1 ? "" : "s"} of “${said ?? ""}”`]
      : nodes.length > 1
        ? [`${nodes.length} Objects`, ""]
        : "node" in offered
          ? [noun ?? "Object", said && said !== noun ? said : ""]
          : [said ?? around.slide(offered.state), ""];
    // Aligning takes two, spreading three; ordering takes one or more.
    const offers = (kind: string) => (kind === "order" ? nodes.length >= 1 : kind === "align" ? nodes.length >= 2 : nodes.length >= 3);
    const groups = ["align", "spread", "order"].filter((kind) => !chars && offers(kind));
    const arrange = groups
      .map((kind) => {
        const buttons = ARRANGE.filter(([k]) => k === kind).map(
          ([, how, label]) => `<button type="button" data-arrange='${html(JSON.stringify(how))}'>${html(label)}</button>`,
        );
        return `<span class="arrange-kind">${ARRANGING[kind]}</span><span class="arrange-buttons" role="group" aria-label="${ARRANGING[kind]}">${buttons.join("")}</span>`;
      })
      .join("");
    // Grouping takes one or more, and a group alone comes apart (PLAN 2.43).
    const apart = nodes.length === 1 && "type" in offered && offered.type === "group";
    const grouping =
      !chars && nodes.length
        ? `<span class="arrange-kind">${ARRANGING.group}</span><span class="arrange-buttons" role="group" aria-label="${ARRANGING.group}"><button type="button" data-group>Group</button>${apart ? '<button type="button" data-ungroup>Ungroup</button>' : ""}</span>`
        : "";
    const here = html(around.slide(now.state));
    const kept = "node" in offered ? `Only on ${here}` : `Its layout only on ${here}`;
    // Characters are kept to the state as their text is typed in: with Alt, or not.
    const keeping = chars ? "" : `<p class="keep"><label><input type="checkbox" data-keep${keep ? " checked" : ""}> ${kept}</label></p>`;
    // How it reads: for one node or the state, never for several or for characters.
    const reading = !chars && nodes.length <= 1 ? `<section class="reads" aria-label="How it reads"></section>` : "";
    // Writing (PLAN 2.101): one text's words, or the slide's notes, by the assistant's model; a
    // picture's description, from the picture itself (PLAN 2.119).
    const text = !chars && nodes.length === 1 && "type" in offered && offered.type === "text";
    const picture = !chars && nodes.length === 1 && "type" in offered && offered.type === "image";
    const writes = text
      ? WRITING.map(([kind, label, title]) => `<button type="button" data-write="${kind}" title="${html(title)}, by the assistant's model">${label}</button>`)
      : picture
        ? [`<button type="button" data-write="describe" title="What it shows, said for someone who can’t see it, by the assistant's model, which is shown the picture">Describe</button>`]
        : !chars && !nodes.length
          ? [`<button type="button" data-write="notes" title="What you say while it shows, drafted from its words by the assistant's model">Draft Notes</button>`]
          : [];
    const writing = writes.length
      ? `<div class="arrange writing"><span class="arrange-kind">Writing</span><span class="arrange-buttons" role="group" aria-label="Writing">${writes.join("")}</span></div>`
      : "";
    // A fetched map (ADR-0030): where it came from, and what fetches it again, a zoom step nearer
    // or further, centered on its pin, or as it was; its pin's label and color and its credit
    // are among its fields.
    const fetched = picture && "fetched" in offered ? (offered as { fetched?: string }).fetched : undefined;
    const map = fetched?.startsWith("Map ")
      ? `<h3 class="map-title">Map</h3><p class="named">${html(fetched)}</p>
      <div class="arrange"><span class="arrange-kind">Fetch</span><span class="arrange-buttons" role="group" aria-label="Fetch the map again">${MAPPING.map(([how, label, title]) => `<button type="button" data-remap="${how}" title="${html(title)}">${label}</button>`).join("")}</span></div>`
      : "";
    // The slide's number and footer (ADR-0028), where the theme places them: the deck's, and this
    // slide's own say.
    const furniture = !chars && !nodes.length && "furniture" in offered ? offered.furniture : undefined;
    const furnishing = furniture?.placed
      ? `<h3 class="furniture-title">Slide Number and Footer</h3>
      <div class="fields furniture">
        <label for="look-number">Number</label><span class="control"><select id="look-number" data-furnish="number"><option value="yes"${furniture.number ? " selected" : ""}>Each slide's</option><option value="no"${furniture.number ? "" : " selected"}>None</option></select></span><span></span>
        <label for="look-footer">Footer</label><span class="control"><input id="look-footer" type="text" data-furnish="footer" value="${html(furniture.footer ?? "")}" placeholder="Words at the foot of each slide" aria-label="Footer: words at the foot of each slide"></span><span></span>
        <label for="look-furniture">This slide</label><span class="control"><select id="look-furniture" data-furnish="slide"><option value=""${furniture.slide === undefined ? " selected" : ""}>As its layout has them</option><option value="yes"${furniture.slide === true ? " selected" : ""}>Shows them</option><option value="no"${furniture.slide === false ? " selected" : ""}>Hides them</option></select></span><span class="lives">${furniture.shown ? "Shown" : "Hidden"}</span>
      </div>`
      : "";
    // A picture's frame is its theme's (PLAN T.3a): said, and offered nowhere.
    const framed = !chars && nodes.length === 1 && "framed" in offered && offered.framed;
    const framing = framed ? `<p class="framed"><span class="arrange-kind">Frame</span> ${html(framed)}</p>` : "";
    // The layouts the state may take, drawn (PLAN 2.92): where there is more than one.
    const layout = !chars && !("node" in offered) ? offered.fields.find((f) => f.prop === "layout") : undefined;
    const choosing = layout?.takes.kind === "name" && layout.takes.names.length > 1;
    const layouts = choosing ? `<section class="layouts" aria-label="Layouts, best first" hidden></section>` : "";
    // The slide's background (PLAN 2.148): with nothing selected, above its layouts.
    const backdrop = !chars && !("node" in offered) ? `<section class="backgrounds" aria-label="Background" hidden></section>` : "";
    into.innerHTML = `
      <h2>${html(title)}</h2>
      ${named ? `<p class="named">${html(named)}</p>` : ""}
      ${keeping}
      ${arrange || grouping ? `<div class="arrange">${arrange}${grouping}</div>` : ""}
      ${text ? writing : ""}
      ${backdrop}
      ${layouts}
      ${map}
      <div class="fields">${rows.join("")}</div>
      ${framing}
      ${furnishing}
      ${text ? "" : writing}
      ${reading}`;
    const swatches = into.querySelector<HTMLElement>(".backgrounds");
    if (swatches) backdrops.attach(swatches);
    // Animate (ATTR-4): the slide's Transition and Advance, or the object's Builds, as the Mac's
    // Animate tab heads them; the characters selected have none. Then the timeline under the canvas.
    const sections: [string, Field[]][] = isNode
      ? [["Builds", builds]]
      : [
          ["Transition", builds.filter((f) => f.prop.startsWith("transition/"))],
          ["Advance", builds.filter((f) => f.prop === "hold")],
        ];
    const none = chars
      ? "The characters selected have no builds: select the text itself"
      : builds.length
        ? ""
        : isNode
          ? "No builds are offered for what is selected"
          : "This slide's transition is not offered";
    const timeline = around.timeline
      ? `<p class="arrange"><span class="arrange-kind">Timeline</span><span class="arrange-buttons"><button type="button" data-timeline aria-pressed="${around.timeline()}" title="The slide's animations on a timeline under the canvas: each on the slide's clock, to time and to add to">${around.timeline() ? "Hide the Timeline" : "Show the Timeline"}</button></span></p>`
      : "";
    animating.innerHTML = `
      <h2>${html(title)}</h2>
      ${named ? `<p class="named">${html(named)}</p>` : ""}
      ${isNode && builds.length ? keeping : ""}
      ${sections
        .filter(([, fields]) => fields.length)
        .map(([name, fields]) => `<h3>${name}</h3><div class="fields">${fields.map(row).join("")}</div>`)
        .join("")}
      ${none ? `<p class="named">${html(none)}</p>` : ""}
      ${timeline}`;
    // A layout previewed is let go: the canvas shows the state at rest as it is.
    if (previewing !== undefined || previewed !== undefined) preview(undefined);
    // The last suggested for the state shown stand: an edit has them judged again once every
    // state is linted, edits having paused (`linted`). Another state's are asked for now.
    if (layouts) {
      if (suggested?.state === now.state && suggested.format === around.format()) fill(suggested);
      else suggestSoon();
    }
    const wanted = pending && nodes.length === 1 && nodes[0] === pending.node ? `look-${pending.prop.replace(/\//g, "-")}` : undefined;
    if (wanted) pending = undefined;
    const focus = wanted ?? focused;
    if (focus) {
      const field = find<HTMLElement>(`#${CSS.escape(focus)}`);
      // A field a finding asks for is shown on its tab (ATTR-4).
      if (field && wanted) around.reveal?.(animating.contains(field) ? "animate" : "format");
      field?.focus();
    }
    if (typing) {
      const field = find<HTMLInputElement | HTMLTextAreaElement>(`#${CSS.escape(typing.id)}`);
      if (field && typing.typed) {
        field.value = typing.value;
        // A value set by a script is no change of the person's, which the field would say when
        // it is left: it says so itself, unless more typing has it said already.
        let said = false;
        field.addEventListener("change", () => (said = true), { once: true });
        field.addEventListener(
          "blur",
          () => {
            if (!said && field.value !== field.defaultValue) field.dispatchEvent(new Event("change", { bubbles: true }));
          },
          { once: true },
        );
      }
      if (field && field.value === typing.value && typing.from !== null && typing.to !== null) field.setSelectionRange(typing.from, typing.to);
    }
    if (reading) void reads(nodes[0]);
    return true;
  }

  /** How the state shown reads (PLAN 2.56), into the inspector's reading: what `node` reads as, or,
   * with none, each part of the state in order. */
  async function reads(node: string | undefined) {
    const now = around.shown();
    if (!now) return;
    const turn = ++read;
    const said = await stage.reading(now.state, around.format()).catch(() => undefined);
    const box = into.querySelector<HTMLElement>(".reads");
    if (turn !== read || said === undefined || !box) return;
    const parsed = document.createElement("template");
    parsed.innerHTML = said;
    const parts = [...parsed.content.querySelectorAll<HTMLElement>("[data-node]")].map(part);
    if (node === undefined) {
      box.innerHTML = parts.length
        ? `<h3>Reads, in order</h3><ol>${parts
            .map((p) => `<li><button type="button" data-read="${html(p.node)}" title="Select it">${html(p.kind)}</button> ${html(p.says)}</li>`)
            .join("")}</ol>`
        : "<h3>Reads</h3><p>Nothing on this slide is read aloud</p>";
      return;
    }
    const mine = parts.find((p) => p.node === node);
    const fields = offered && "fields" in offered ? offered.fields : [];
    const value = (prop: string) => fields.find((f) => f.prop === prop)?.value;
    const type = offered && "type" in offered ? offered.type : "";
    const why =
      value("semantic") === "decoration" || value("alt") === ""
        ? "it’s decoration"
        : ["stack", "grid", "frame"].includes(type)
          ? "it holds others, which each read on their own"
          : ["shape", "shader"].includes(type)
            ? "it says nothing until it’s described"
            : "not on its own";
    box.innerHTML = mine
      ? `<h3>Reads as</h3><p><span class="kind">${html(mine.kind)}</span> ${html(mine.says)}</p>`
      : `<h3>Reads as</h3><p>Not read aloud: ${html(why)}</p>`;
  }

  /** Ask for the layouts the state shown may take (PLAN 2.92), once the inspector has settled on
   * it: each is laid out, linted in every format, and drawn, which takes the worker a while. */
  function suggestSoon() {
    clearTimeout(soon);
    soon = setTimeout(() => void suggest(), 150);
  }

  async function suggest() {
    const now = around.shown();
    if (!now || !into.querySelector(".layouts")) return;
    const turn = ++suggesting;
    const format = around.format();
    const ratio = Math.min(2, window.devicePixelRatio || 1);
    const got = await stage.layoutSuggestions(now.state, Math.round(PICTURE * ratio), format).catch(() => undefined);
    if (turn !== suggesting || !got || around.shown()?.state !== now.state) return;
    const layouts = got.map((suggestion) => ({
      suggestion,
      picture: new ImageData(new Uint8ClampedArray(suggestion.pixels), suggestion.width, suggestion.height),
    }));
    suggested = { state: now.state, format, layouts };
    fill(suggested);
  }

  /** Draw `these` into the inspector's layouts, the one focused keeping the focus. */
  function fill(these: Suggested) {
    const box = into.querySelector<HTMLElement>(".layouts");
    if (!box) return;
    const focused = box.contains(document.activeElement) ? (document.activeElement as HTMLElement).dataset.layout : undefined;
    // One drawing is nothing to choose between.
    box.hidden = these.layouts.length < 2;
    // Each layout by what the theme calls it (PLAN 2.99), as the layout field says it.
    const field = offered && !("node" in offered) ? offered.fields.find((f) => f.prop === "layout") : undefined;
    const name = (layout: string) => field?.words?.said[layout] ?? layout;
    const buttons = these.layouts.map(({ suggestion: s }) => {
      // What lint finds, as issues: red where one is an error.
      const issues = s.errors + s.warnings;
      const found = issues ? `<span class="${s.errors ? "error" : "warning"}">${counted(issues, "issue")}</span>` : "No issues";
      const reach = s.current ? "The slide's layout now" : keep ? "Changes this slide" : `Changes ${reaching(s.reach, these.state, around.slide)}`;
      // The layouts that draw it so, folded into this one.
      const others = (s.alike ?? []).map(name);
      const alike = others.length ? `also ${others.join(", ")}` : "";
      const said = `${name(s.layout)}: ${reach}${alike ? `; ${others.join(" and ")} ${others.length === 1 ? "draws" : "draw"} it alike` : ""}`;
      // An id, so that the focus on one stays there when the inspector is drawn again.
      return `<button type="button" id="layout-suggested-${html(s.layout)}" data-layout="${html(s.layout)}" aria-pressed="${s.current === true}" title="${html(said)}">
        <canvas aria-hidden="true" width="${s.width}" height="${s.height}"></canvas><span class="name">${html(name(s.layout))}</span><span class="verdict">${found}</span>${alike ? `<span class="alike">${html(alike)}</span>` : ""}<span class="sr">, ${html(reach)}</span></button>`;
    });
    box.innerHTML = `<h3>Layouts, best first</h3><div class="suggested">${buttons.join("")}</div>`;
    box.querySelectorAll("canvas").forEach((canvas, i) => canvas.getContext("2d")?.putImageData(these.layouts[i].picture, 0, 0));
    if (focused !== undefined) box.querySelector<HTMLElement>(`[data-layout="${CSS.escape(focused)}"]`)?.focus();
  }

  /** Show the state shown laid out in `layout` on the canvas, nothing made, or at rest as it is
   * with none (PLAN 2.92): a preview at a time, the last asked for winning. */
  function preview(layout: string | undefined) {
    previewing = layout;
    previews ??= (async () => {
      while (previewed !== previewing) {
        const want = previewing;
        const now = around.shown();
        const s = suggested?.state === now?.state ? suggested?.layouts.find((l) => l.suggestion.layout === want)?.suggestion : undefined;
        if (now) {
          const shown = s && !s.current ? stage.preview(now.state, s.patch, around.format()) : stage.rest(now.state, around.format());
          await shown.catch(() => {});
        }
        previewed = want;
      }
      previews = undefined;
    })();
  }

  /** Focus `prop`'s field once the inspector shows `node`: now, if it does. */
  function focus(node: string, prop: string) {
    pending = { node, prop };
    const shows = offered && "node" in offered && offered.node === node && !("nodes" in offered);
    const field = shows ? find<HTMLElement>(`#look-${CSS.escape(prop.replace(/\//g, "-"))}`) : null;
    if (field) {
      pending = undefined;
      around.reveal?.(animating.contains(field) ? "animate" : "format");
      field.focus();
    }
  }

  /** The state's layout chosen from the theme's layouts by their sections (PLAN 3.31), of those
   * it may take, as the layout field chooses one. */
  async function pickLayout() {
    const field = offered && !("node" in offered) ? offered.fields.find((f) => f.prop === "layout") : undefined;
    if (!field || field.takes.kind !== "name") return;
    const current = typeof field.value === "string" ? field.value : undefined;
    const picked = await around.pickLayout(field.takes.names, current);
    into.querySelector<HTMLElement>("[data-pick-layout]")?.focus();
    if (picked && picked !== current) await choose("layout", picked).then(suggestSoon);
  }

  /** Choose `value` for the node shown's `prop`, or the state's with none shown; `null` takes it
   * away where it lives. */
  function choose(prop: string, value: unknown): Promise<unknown> {
    queue = queue.then(() => make(prop, value));
    return queue;
  }

  async function make(prop: string, value: unknown) {
    const now = around.shown();
    if (!now || !offered) return;
    if (chars) {
      const selected = chars;
      // The text says what it gave them, in a person's words; the look is read again once it is.
      const field = offered.fields.find((f) => f.prop === prop);
      const what = field && `${labeled(field)}: ${value === null ? "back to the text’s" : told(field, value)}`;
      if (!(await around.style({ [prop]: value }, what))) render(true);
      else if (chars === selected) await characters(selected);
      return;
    }
    const on = "node" in offered ? offered.node : undefined;
    const all = "nodes" in offered ? (offered as Shared).nodes : on !== undefined ? [on] : [];
    const ops = all.length
      ? all.map((n) => ({ op: "choose", node: n, prop, value, state: now.state, ...(keep ? { fork: true } : {}) }))
      : [{ op: "set_state", id: now.state, prop, value, ...(keep && prop === "layout" ? { fork: true } : {}) }];
    // What changed, as a person says it (PLAN 2.99): the property by its label, the value said,
    // several by how many, and the slides it reaches.
    const field = offered.fields.find((f) => f.prop === prop);
    const name = field ? labeled(field) : prop;
    const several = all.length > 1 ? ` · ${all.length} objects` : "";
    around.say(`Changing ${name}…`);
    try {
      const states = await stage.reach(ops);
      const { source, edited } = await stage.make(around.source, ops, now.index, around.format());
      around.apply(source, edited);
      const given = value === null ? "back to the theme’s" : field ? told(field, value) : String(value);
      around.say(`${name}: ${given}${several} · ${reaching(states, now.state, around.slide)}`);
    } catch (e) {
      around.say(`Not changed: ${said(e)}`);
      // The controls go back to what the deck says.
      render(true);
    }
  }

  /** The deck's slide numbers and footer, or the slide's own say, by `ops` (ADR-0028): one patch,
   * one step to undo, said as `told`. */
  function furnish(ops: unknown[], told: string) {
    queue = queue.then(async () => {
      const now = around.shown();
      if (!now) return;
      try {
        const { source, edited } = await stage.make(around.source, ops, now.index, around.format());
        around.apply(source, edited);
        around.say(`${told} · ⌘Z undoes it`);
      } catch (e) {
        around.say(`Not changed: ${said(e)}`);
        render(true);
      }
    });
  }

  for (const panel of panels) listen(panel, "change", (e) => {
    const target = e.target as HTMLInputElement | HTMLSelectElement;
    const furnishing = target.dataset.furnish;
    if (furnishing && offered && "furniture" in offered) {
      const f = offered.furniture;
      if (furnishing === "number") {
        const on = target.value === "yes";
        furnish([{ op: "furnish", number: on }], on ? "Slides numbered" : "Slides not numbered");
      } else if (furnishing === "footer") {
        const words = target.value.trim();
        // Words that trim to the footer the deck has change nothing: the field is put back.
        if (words === (f.footer ?? "")) {
          target.value = f.footer ?? "";
          return;
        }
        furnish([{ op: "furnish", footer: words || null }], words ? `Footer: “${words}”` : "No footer");
      } else {
        const value = target.value === "" ? null : target.value === "yes";
        const slide = around.slide(f.head);
        const told = value === null ? `${slide}: number and footer as its layout has them` : `${slide}: number and footer ${value ? "shown" : "hidden"}`;
        furnish([{ op: "set_state", id: f.head, prop: "furniture", value }], told);
      }
      return;
    }
    if (target.matches("[data-keep]")) {
      keep = (target as HTMLInputElement).checked;
      // Format's and Animate's say the same.
      for (const box of panels.flatMap((p) => [...p.querySelectorAll<HTMLInputElement>("[data-keep]")])) box.checked = keep;
      // What each layout suggested changes, kept to the state or not.
      if (suggested && suggested.state === around.shown()?.state) fill(suggested);
      return;
    }
    const prop = target.dataset.prop;
    if (prop === undefined) return;
    // Fractions of the image, together (PLAN 2.45): a crop keeps some of the image, inside it.
    if (target.dataset.part !== undefined) {
      const inputs = [...into.querySelectorAll<HTMLInputElement>(`input[data-prop="${CSS.escape(prop)}"][data-part]`)];
      const parts = inputs.map((i) => (i.value.trim() === "" ? NaN : Number(i.value) / 100));
      if (parts.some((p) => !Number.isFinite(p))) return render(true);
      const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), hi);
      // In whole thousandths, as a person writes them, so that a crop's edges add up exactly.
      let given = parts.map((p) => Math.round(clamp(p, 0, 1) * 1000));
      if (prop === "crop") {
        const [x, y] = [Math.min(given[0], 990), Math.min(given[1], 990)];
        given = [x, y, clamp(given[2], 10, 1000 - x), clamp(given[3], 10, 1000 - y)];
      }
      return void choose(prop, given.map((p) => p / 1000));
    }
    let value: unknown = target.value === "" ? null : target.value;
    // Another preset for a build called with a setting keeps the setting: its wait stays.
    const call = called(offered && "fields" in offered ? offered.fields.find((f) => f.prop === prop)?.value : undefined);
    if (call && typeof value === "string" && target instanceof HTMLSelectElement) value = { ...call, preset: value };
    if (target.matches("[data-flag]") && value !== null) value = value === "yes";
    if (target instanceof HTMLInputElement && (target.type === "number" || target.type === "range") && value !== null) {
      value = Number(value) * Number(target.dataset.scale ?? 1);
    }
    // An adjustment let go at none takes the object's own away, and is written as none on a
    // slide (PLAN 2.117).
    if (target instanceof HTMLInputElement && target.type === "range" && value === 0 && target.dataset.awayAtNone !== undefined) value = null;
    void choose(prop, value);
  });
  // A slider's readout follows it as it is dragged, before it is let go.
  for (const panel of panels) listen(panel, "input", (e) => {
    const target = e.target;
    if (!(target instanceof HTMLInputElement) || target.type !== "range") return;
    const said = adjustment(Number(target.value));
    target.setAttribute("aria-valuetext", said);
    const readout = target.nextElementSibling;
    if (readout?.classList.contains("lives")) readout.textContent = said;
  });
  for (const panel of panels) listen(panel, "click", (e) => {
    const timeline = (e.target as Element).closest<HTMLElement>("[data-timeline]");
    if (timeline && around.timeline) {
      const shown = around.timeline(timeline.getAttribute("aria-pressed") !== "true");
      timeline.setAttribute("aria-pressed", String(shown));
      timeline.textContent = shown ? "Hide the Timeline" : "Show the Timeline";
      return;
    }
    const away = (e.target as Element).closest<HTMLElement>("[data-away]");
    if (away?.dataset.away) void choose(away.dataset.away, null);
    const arranging = (e.target as Element).closest<HTMLElement>("[data-arrange]");
    if (arranging?.dataset.arrange) void around.arrange(JSON.parse(arranging.dataset.arrange) as Arrange);
    if ((e.target as Element).closest("[data-group]")) void around.group();
    if ((e.target as Element).closest("[data-ungroup]")) void around.ungroup();
    if ((e.target as Element).closest("[data-pick]")) around.pick();
    if ((e.target as Element).closest("[data-pick-layout]")) void pickLayout();
    const remap = (e.target as Element).closest<HTMLElement>("[data-remap]");
    if (remap?.dataset.remap && offered && "node" in offered) around.remap?.(offered.node, remap.dataset.remap as "in" | "out" | "pin" | "again");
    const writing = (e.target as Element).closest<HTMLElement>("[data-write]");
    if (writing?.dataset.write && offered) around.write(writing.dataset.write as WritingKind, "node" in offered ? offered.node : undefined);
    const reading = (e.target as Element).closest<HTMLElement>("[data-read]");
    if (reading?.dataset.read) around.select(reading.dataset.read);
    // A layout suggested is chosen as the layout field chooses it (PLAN 2.92), and the layouts are
    // judged again in the deck it makes.
    const layout = (e.target as Element).closest<HTMLElement>("[data-layout]");
    if (layout?.dataset.layout && layout.getAttribute("aria-pressed") !== "true") void choose("layout", layout.dataset.layout).then(suggestSoon);
  });
  // A layout suggested, pointed at or focused, shows on the canvas until the pointer or the focus
  // leaves the layouts (PLAN 2.92).
  const suggestion = (target: EventTarget | null) => (target instanceof Element ? target.closest<HTMLElement>(".layouts [data-layout]") : null);
  const away = (target: EventTarget | null) => !(target instanceof Element && target.closest(".layouts"));
  listen(into, "pointerover", (e) => {
    const over = suggestion(e.target);
    if (over) preview(over.dataset.layout);
  });
  listen(into, "pointerout", (e) => {
    if (suggestion(e.target) && away(e.relatedTarget) && previewing !== undefined) preview(undefined);
  });
  listen(into, "focusin", (e) => {
    const on = suggestion(e.target);
    if (on) preview(on.dataset.layout);
  });
  listen(into, "focusout", (e) => {
    if (suggestion(e.target) && away(e.relatedTarget) && previewing !== undefined) preview(undefined);
  });

  return {
    show,
    characters,
    choose,
    focus,
    /** What is offered for the node shown, or for the state, for a test. */
    offered: () => offered,
    /** Choices made so far are made: what a test waits for. */
    settled: () => queue,
    /** Every state is linted, edits having paused: the layouts suggested for the state shown are
     * judged again, and those of a state not shown are let go (PLAN 2.92). */
    linted: () => {
      if (into.querySelector(".layouts")) suggestSoon();
      else suggested = undefined;
      backdrops.linted();
    },
    /** The slide's backgrounds (PLAN 2.148), for a test. */
    backgrounds: backdrops,
    /** The layouts last suggested for the state shown, best first (PLAN 2.92), once they are,
     * for a test. */
    suggested: () =>
      suggested?.state === around.shown()?.state ? suggested?.layouts.map((l) => ({ ...l.suggestion, pixels: undefined })) : undefined,
    /** The preview asked for is shown: what a test waits for. */
    previewed: async () => {
      await previews;
      return previewed;
    },
    /** Whether "Only on Slide N" is checked: what keeps an annotation made on the canvas to
     * the state shown too (PLAN 2.67). */
    keeping: () => keep,
  };
}
