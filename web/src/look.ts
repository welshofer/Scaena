// The inspector's edits (PLAN 2.33, ADR-0013): the node selected, its role, style, presets, and
// props, chosen from what the theme names for its type, or what the schema allows. The engine
// says what it offers, the value the state shows, and where that value lives (`Player.choices`);
// the page lays nothing out and keeps no vocabulary of its own.
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
//   undo. "Only in this state" keeps it to the state shown (`fork`).
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
// With characters selected in a text typed in (PLAN 2.38, `Player.characterChoices`), their look:
// a run's role, emphasis, family, weight, italic (PLAN 2.40), and color, each the first
// character's. Each choice is one `style_text`, written where the text lives; × takes the run's
// own away, so the text's look shows there. A run takes the theme's names only.
import type { Arrange, Choices, Edited, Field, LayoutSuggestion, Lives, StateChoices } from "./protocol";
import type { Stage } from "./stage";
import type { Selected } from "./typing";

/** What the inspector's edits ask of the editor around them. */
export interface Around {
  /** The state shown, by its id and its slot's index. */
  shown(): { state: string; index: number } | undefined;
  format(): string | undefined;
  source(): string;
  /** Take `source`, the patch made: one step to undo. */
  apply(source: string, edited: Edited): void;
  say(text: string): void;
  /** Give the characters selected in the text typed in `look` (`style_text`): whether it was. */
  style(look: Record<string, unknown>): Promise<boolean>;
  /** Arrange what the canvas selects `how` (PLAN 2.42). */
  arrange(how: Arrange): Promise<void>;
  /** Put what the canvas selects in a new group, and take the group it selects apart (PLAN 2.43). */
  group(): Promise<void>;
  ungroup(): Promise<void>;
  /** Pick the focal point of the image the canvas selects: the next press on it (PLAN 2.45). */
  pick(): void;
  /** Select `node` on the canvas, as a click on it does: a part of the reading (PLAN 2.56). */
  select(node: string): void;
}

/** What several nodes offer alike: each field all of them have with the same choices, its value
 * where they agree. */
type Shared = Choices & { nodes: string[] };

function shared(all: Choices[]): Shared {
  const [first, ...rest] = all;
  const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
  const alike = (f: Field) => rest.every((c) => c.fields.some((g) => g.prop === f.prop && same(g.takes, f.takes)));
  const fields = first.fields.filter(alike).map((f) => {
    const agree = rest.every((c) => same(c.fields.find((g) => g.prop === f.prop)?.value, f.value));
    return agree ? f : { prop: f.prop, takes: f.takes };
  });
  const types = [...new Set(all.map((c) => c.type))];
  return { ...first, type: types.join(", "), fields, nodes: all.map((c) => c.node) };
}

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
const counted = (n: number, what: string) => `${n} ${what}${n === 1 ? "" : "s"}`;

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
      return { node, kind: `heading ${el.tagName[1]}`, says: quoted(el.textContent ?? "") };
    case "P":
      return { node, kind: "text", says: quoted(el.textContent ?? "") };
    case "TABLE": {
      const rows = el.querySelectorAll("tr").length;
      return { node, kind: "table", says: `${label ? `${quoted(label)}, ` : ""}${rows} row${rows === 1 ? "" : "s"}` };
    }
    default:
      return { node, kind: "figure", says: label ? quoted(label) : "not described" };
  }
}

/** A value as the inspector says it. */
const spoken = (v: unknown) => (typeof v === "string" ? v : JSON.stringify(v));

/** Where a value lives, for a person. */
export function where(lives: Lives | undefined): string {
  if (lives === undefined) return "the theme's";
  if (lives === "overrides") return "an override, in every state";
  if (lives === "node") return "on the node";
  return `set in ${lives.state}`;
}

/** A value as the status says it: a hold in seconds, a turn in degrees. */
const told = (prop: string, v: unknown) =>
  prop === "hold" && typeof v === "number" ? `${v / 1000} s` : prop === "transform/rotate" && typeof v === "number" ? `${v}°` : spoken(v);

/** The inspector's edits, in `into`, for the node the canvas selects in `stage`'s state shown, or
 * the state itself when it selects none. */
export function looks(stage: Stage, into: HTMLElement, around: Around) {
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
    const value = f.value;
    // A value no control here sets (a preset with parameters, a gradient) shows as the source has it.
    const kept = (known: unknown[]) =>
      value !== undefined && !known.includes(value) ? `<option value="" selected disabled>${html(spoken(value))}</option>` : "";
    const unset = f.prop === "transition/duration" && cuts() ? "cut" : f.prop === "layout" ? "none" : "the theme's";
    const none = `<option value=""${value === undefined ? " selected" : ""}>${value === undefined ? unset : "—"}</option>`;
    const options = (all: string[]) => all.map((o) => `<option${o === value ? " selected" : ""}>${html(o)}</option>`).join("");
    const t = f.takes;
    switch (t.kind) {
      case "name": {
        // A value written out (an override) shows as it is written, beside the theme's names.
        let select = `<select id="${id}" ${prop}>${none}${options(t.names)}${kept(t.names)}</select>`;
        const written = `id="${id}-written" ${prop} title="Written out: an override, in every state"`;
        if (t.overrides && t.of === "color") {
          const hex = typeof value === "string" && /^#[0-9a-f]{6}$/i.test(value) ? value : "#000000";
          select += ` <input type="color" ${written} value="${hex}" aria-label="${html(f.prop)} written out, an override">`;
        } else if (t.overrides) {
          const units = typeof value === "number" ? String(value) : "";
          select += ` <input type="number" ${written} min="0" step="any" value="${units}" placeholder="cu" aria-label="${html(f.prop)} in canvas units, an override">`;
        }
        return select;
      }
      case "word":
        return `<select id="${id}" ${prop}>${none}${options(t.words)}${kept(t.words)}</select>`;
      case "flag": {
        const yes = value === true ? " selected" : "";
        const no = value === false ? " selected" : "";
        return `<select id="${id}" ${prop} data-flag>${none}<option value="yes"${yes}>yes</option><option value="no"${no}>no</option></select>`;
      }
      case "number": {
        // A hold is milliseconds; a person sets it in seconds.
        if (f.prop === "hold") {
          const shown = typeof value === "number" ? String(value / 1000) : "";
          return `<input id="${id}" type="number" ${prop} data-scale="1000" min="0" step="0.5" value="${shown}" placeholder="none" aria-label="hold, seconds"> <span class="lives">s</span>`;
        }
        // How far a node is turned (PLAN 2.51): degrees, clockwise; none is upright.
        if (f.prop === "transform/rotate") {
          const shown = typeof value === "number" ? String(value) : "";
          return `<input id="${id}" type="number" ${prop} step="1" value="${shown}" placeholder="0" aria-label="turned, degrees clockwise"> <span class="lives">°</span>`;
        }
        const min = t.min ?? t.above;
        const bounds = `${min !== undefined ? ` min="${min}"` : ""}${t.max !== undefined ? ` max="${t.max}"` : ""}`;
        const step = t.whole ? "1" : t.max !== undefined && t.max <= 1 ? "0.05" : "any";
        const shown = typeof value === "number" ? String(value) : "";
        return `<input id="${id}" type="number" ${prop}${bounds} step="${step}" value="${shown}" placeholder="the theme's">`;
      }
      case "text": {
        // A text says its words unless it is described; anything else says nothing until it is.
        const words = offered && "node" in offered && offered.type === "text" ? "its words" : "not described";
        const [rows, empty] = f.prop === "alt" ? [2, words] : [3, "none"];
        return `<textarea id="${id}" ${prop} rows="${rows}" placeholder="${empty}">${html(typeof value === "string" ? value : "")}</textarea>`;
      }
      case "fractions": {
        // An image's focal point or crop (PLAN 2.45): what shows where nothing sets it.
        const rest = f.prop === "crop" ? [0, 0, 1, 1] : [0.5, 0.5];
        const now = Array.isArray(value) && value.length === t.names.length ? (value as number[]) : rest;
        const inputs = t.names.map(
          (name, k) =>
            `<input${k === 0 ? ` id="${id}"` : ""} type="number" class="fraction" ${prop} data-part="${k}" min="0" max="1" step="0.01" value="${now[k]}" aria-label="${html(f.prop)} ${name}" title="${name}, a fraction of the image">`,
        );
        // The focal point is picked on the image itself: where its subject is.
        const pick = f.prop === "focal" ? ' <button type="button" data-pick title="Then click the image where its subject is">Pick</button>' : "";
        return `${inputs.join("")}${pick}`;
      }
    }
  }

  /** What `f`'s value is, where it lives, and the × that takes it away there. */
  function note(f: Field): string {
    const flag =
      f.lives === "overrides"
        ? ' <span class="flag" title="In the deck\'s overrides: it wins in every state, and a theme does not change it">override</span>'
        : f.literal
          ? ' <span class="flag" title="W300: written out where the theme has names; it belongs in overrides">W300</span>'
          : "";
    const away =
      f.lives === undefined ? "" : ` <button type="button" class="away" data-away="${html(f.prop)}" aria-label="Take ${html(f.prop)} away, ${html(where(f.lives))}" title="Take it away where it lives">×</button>`;
    // A state's own property set nowhere: a cut, or none. Characters with no look of their own
    // show the text's.
    const lives =
      chars && f.lives === undefined
        ? "the text's"
        : f.lives !== undefined || "node" in (offered ?? {})
          ? where(f.lives)
          : f.prop.startsWith("transition/")
            ? cuts()
              ? "cuts in"
              : "the theme's"
            : "none";
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
    const focused = into.contains(document.activeElement) ? document.activeElement?.id : undefined;
    const rows = offered.fields.map((f) => {
      const id = `look-${f.prop.replace(/\//g, "-")}`;
      return `<label for="${id}">${html(f.prop)}</label><span class="control">${control(f, id)}</span><span>${note(f)}</span>`;
    });
    const nodes = "nodes" in offered ? (offered as Shared).nodes : "node" in offered ? [offered.node] : [];
    const title = chars
      ? `${html(chars.node)} · characters ${chars.from + 1}–${chars.to}`
      : nodes.length > 1
        ? `${nodes.length} selected · ${html(nodes.join(", "))}`
        : "node" in offered
          ? `${html(offered.node)} · ${html(offered.type)}`
          : `${html(offered.state)} · state`;
    // Aligning takes two, spreading three; ordering takes one or more.
    const offers = (kind: string) => (kind === "order" ? nodes.length >= 1 : kind === "align" ? nodes.length >= 2 : nodes.length >= 3);
    const groups = ["align", "spread", "order"].filter((kind) => !chars && offers(kind));
    const arrange = groups
      .map((kind) => {
        const buttons = ARRANGE.filter(([k]) => k === kind).map(
          ([, how, label]) => `<button type="button" data-arrange='${html(JSON.stringify(how))}'>${html(label)}</button>`,
        );
        return `<span class="arrange-kind">${kind}</span><span class="arrange-buttons" role="group" aria-label="${kind}">${buttons.join("")}</span>`;
      })
      .join("");
    // Grouping takes one or more, and a group alone comes apart (PLAN 2.43).
    const apart = nodes.length === 1 && "type" in offered && offered.type === "group";
    const grouping =
      !chars && nodes.length
        ? `<span class="arrange-kind">group</span><span class="arrange-buttons" role="group" aria-label="group"><button type="button" data-group>Group</button>${apart ? '<button type="button" data-ungroup>Ungroup</button>' : ""}</span>`
        : "";
    const kept = "node" in offered ? `only in ${html(now.state)}` : `layout only in ${html(now.state)}`;
    // Characters are kept to the state as their text is typed in: with Alt, or not.
    const keeping = chars ? "" : `<p class="keep"><label><input type="checkbox" data-keep${keep ? " checked" : ""}> ${kept}</label></p>`;
    // How it reads: for one node or the state, never for several or for characters.
    const reading = !chars && nodes.length <= 1 ? `<section class="reads" aria-label="How it reads"></section>` : "";
    // The layouts the state may take, drawn (PLAN 2.92): where there is more than one.
    const layout = !chars && !("node" in offered) ? offered.fields.find((f) => f.prop === "layout") : undefined;
    const choosing = layout?.takes.kind === "name" && layout.takes.names.length > 1;
    const layouts = choosing ? `<section class="layouts" aria-label="Layouts, best first" hidden></section>` : "";
    into.innerHTML = `
      <h2>${title}</h2>
      ${keeping}
      ${arrange || grouping ? `<div class="arrange">${arrange}${grouping}</div>` : ""}
      ${layouts}
      <div class="fields">${rows.join("")}</div>
      ${reading}`;
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
    if (focus) into.querySelector<HTMLElement>(`#${CSS.escape(focus)}`)?.focus();
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
            .map((p) => `<li><button type="button" data-read="${html(p.node)}" title="Select ${html(p.node)}">${html(p.kind)}</button> ${html(p.says)}</li>`)
            .join("")}</ol>`
        : "<h3>Reads</h3><p>nothing: no node here is read</p>";
      return;
    }
    const mine = parts.find((p) => p.node === node);
    const fields = offered && "fields" in offered ? offered.fields : [];
    const value = (prop: string) => fields.find((f) => f.prop === prop)?.value;
    const type = offered && "type" in offered ? offered.type : "";
    const why =
      value("semantic") === "decoration" || value("alt") === ""
        ? "decoration"
        : ["stack", "grid", "frame"].includes(type)
          ? "a container: what it holds reads on its own"
          : ["shape", "shader"].includes(type)
            ? "it says nothing until it is described"
            : "not on its own";
    box.innerHTML = mine
      ? `<h3>Reads as</h3><p><span class="kind">${html(mine.kind)}</span> ${html(mine.says)}</p>`
      : `<h3>Reads as</h3><p>not read: ${html(why)}</p>`;
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
    const buttons = these.layouts.map(({ suggestion: s }) => {
      const found = s.errors
        ? `<span class="error">${counted(s.errors, "error")}</span>${s.warnings ? `, <span class="warning">${counted(s.warnings, "warning")}</span>` : ""}`
        : s.warnings
          ? `<span class="warning">${counted(s.warnings, "warning")}</span>`
          : "nothing found";
      const reach = s.current ? "the layout it takes now" : keep || s.reach.length <= 1 ? `changes ${these.state}` : `changes ${s.reach.length} states`;
      // The layouts that draw it so, folded into this one.
      const alike = s.alike?.length ? `also ${s.alike.join(", ")}` : "";
      const said = `${s.layout}: ${reach}${alike ? `; ${s.alike!.join(" and ")} ${s.alike!.length === 1 ? "draws" : "draw"} it alike` : ""}`;
      // An id, so that the focus on one stays there when the inspector is drawn again.
      return `<button type="button" id="layout-suggested-${html(s.layout)}" data-layout="${html(s.layout)}" aria-pressed="${s.current === true}" title="${html(said)}">
        <canvas aria-hidden="true" width="${s.width}" height="${s.height}"></canvas><span class="name">${html(s.layout)}</span><span class="verdict">${found}</span>${alike ? `<span class="alike">${html(alike)}</span>` : ""}<span class="sr">, ${html(reach)}</span></button>`;
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
    const field = shows ? into.querySelector<HTMLElement>(`#look-${CSS.escape(prop.replace(/\//g, "-"))}`) : null;
    if (field) {
      pending = undefined;
      field.focus();
    }
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
      // The text says what it gave them; the look is read again once it is.
      if (!(await around.style({ [prop]: value }))) render(true);
      else if (chars === selected) await characters(selected);
      return;
    }
    const on = "node" in offered ? offered.node : undefined;
    const all = "nodes" in offered ? (offered as Shared).nodes : on !== undefined ? [on] : [];
    const ops = all.length
      ? all.map((n) => ({ op: "choose", node: n, prop, value, state: now.state, ...(keep ? { fork: true } : {}) }))
      : [{ op: "set_state", id: now.state, prop, value, ...(keep && prop === "layout" ? { fork: true } : {}) }];
    const who = all.length ? all.join(", ") : now.state;
    around.say(`choosing ${who}'s ${prop}…`);
    try {
      const states = await stage.reach(ops);
      const { source, edited } = await stage.make(around.source(), ops, now.index, around.format());
      around.apply(source, edited);
      const reach = states.length === 1 && states[0] === now.state ? "in this state" : `in ${states.length} states`;
      around.say(`${who}'s ${prop}: ${value === null ? "taken away" : told(prop, value)} · ${reach}`);
    } catch (e) {
      around.say(`not chosen: ${said(e)}`);
      // The controls go back to what the deck says.
      render(true);
    }
  }

  into.addEventListener("change", (e) => {
    const target = e.target as HTMLInputElement | HTMLSelectElement;
    if (target.matches("[data-keep]")) {
      keep = (target as HTMLInputElement).checked;
      // What each layout suggested changes, kept to the state or not.
      if (suggested && suggested.state === around.shown()?.state) fill(suggested);
      return;
    }
    const prop = target.dataset.prop;
    if (prop === undefined) return;
    // Fractions of the image, together (PLAN 2.45): a crop keeps some of the image, inside it.
    if (target.dataset.part !== undefined) {
      const inputs = [...into.querySelectorAll<HTMLInputElement>(`input[data-prop="${CSS.escape(prop)}"][data-part]`)];
      const parts = inputs.map((i) => (i.value.trim() === "" ? NaN : Number(i.value)));
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
    if (target.matches("[data-flag]") && value !== null) value = value === "yes";
    if (target instanceof HTMLInputElement && target.type === "number" && value !== null) value = Number(value) * Number(target.dataset.scale ?? 1);
    void choose(prop, value);
  });
  into.addEventListener("click", (e) => {
    const away = (e.target as Element).closest<HTMLElement>("[data-away]");
    if (away?.dataset.away) void choose(away.dataset.away, null);
    const arranging = (e.target as Element).closest<HTMLElement>("[data-arrange]");
    if (arranging?.dataset.arrange) void around.arrange(JSON.parse(arranging.dataset.arrange) as Arrange);
    if ((e.target as Element).closest("[data-group]")) void around.group();
    if ((e.target as Element).closest("[data-ungroup]")) void around.ungroup();
    if ((e.target as Element).closest("[data-pick]")) around.pick();
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
  into.addEventListener("pointerover", (e) => {
    const over = suggestion(e.target);
    if (over) preview(over.dataset.layout);
  });
  into.addEventListener("pointerout", (e) => {
    if (suggestion(e.target) && away(e.relatedTarget) && previewing !== undefined) preview(undefined);
  });
  into.addEventListener("focusin", (e) => {
    const on = suggestion(e.target);
    if (on) preview(on.dataset.layout);
  });
  into.addEventListener("focusout", (e) => {
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
    },
    /** The layouts last suggested for the state shown, best first (PLAN 2.92), once they are,
     * for a test. */
    suggested: () =>
      suggested?.state === around.shown()?.state ? suggested?.layouts.map((l) => ({ ...l.suggestion, pixels: undefined })) : undefined,
    /** The preview asked for is shown: what a test waits for. */
    previewed: async () => {
      await previews;
      return previewed;
    },
    /** Whether "Only in this state" is checked: what keeps an annotation made on the canvas to
     * the state shown too (PLAN 2.67). */
    keeping: () => keep,
  };
}
