// The inspector's edits (PLAN 2.33, ADR-0013): the node selected, its role, style, presets, and
// props, chosen from what the theme names for its type, or what the schema allows. The engine
// says what it offers, the value the state shows, and where that value lives (`Player.choices`);
// the page lays nothing out and keeps no vocabulary of its own.
//
// With no node selected, the state shown (PLAN 2.36, `Player.stateChoices`): its layout, from the
// theme's layouts with a slot for each node placed in one, written where it lives, so the states
// that take it from there change with it; its transition's duration, ease, spring, and match; its
// hold; and its notes. Each is one `set_state` patch.
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
// With characters selected in a text typed in (PLAN 2.38, `Player.characterChoices`), their look:
// a run's role, emphasis, family, weight, italic (PLAN 2.40), and color, each the first
// character's. Each choice is one `style_text`, written where the text lives; × takes the run's
// own away, so the text's look shows there. A run takes the theme's names only.
import type { Arrange, Choices, Edited, Field, Lives, StateChoices } from "./protocol";
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

/** A value as the inspector says it. */
const spoken = (v: unknown) => (typeof v === "string" ? v : JSON.stringify(v));

/** Where a value lives, for a person. */
export function where(lives: Lives | undefined): string {
  if (lives === undefined) return "the theme's";
  if (lives === "overrides") return "an override, in every state";
  if (lives === "node") return "on the node";
  return `set in ${lives.state}`;
}

/** A value as the status says it: a hold in seconds. */
const told = (prop: string, v: unknown) => (prop === "hold" && typeof v === "number" ? `${v / 1000} s` : spoken(v));

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
    render();
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
        const min = t.min ?? t.above;
        const bounds = `${min !== undefined ? ` min="${min}"` : ""}${t.max !== undefined ? ` max="${t.max}"` : ""}`;
        const step = t.whole ? "1" : t.max !== undefined && t.max <= 1 ? "0.05" : "any";
        const shown = typeof value === "number" ? String(value) : "";
        return `<input id="${id}" type="number" ${prop}${bounds} step="${step}" value="${shown}" placeholder="the theme's">`;
      }
      case "text":
        return `<textarea id="${id}" ${prop} rows="3" placeholder="none">${html(typeof value === "string" ? value : "")}</textarea>`;
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
   * to put the controls back to what the deck says. */
  function render(again = false) {
    const now = around.shown();
    if (!offered || !now) {
      drawn = "";
      into.replaceChildren();
      return;
    }
    const showing = JSON.stringify([now.state, keep, offered, chars]);
    if (showing === drawn && !again) return;
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
    const kept = "node" in offered ? `only in ${html(now.state)}` : `layout only in ${html(now.state)}`;
    // Characters are kept to the state as their text is typed in: with Alt, or not.
    const keeping = chars ? "" : `<p class="keep"><label><input type="checkbox" data-keep${keep ? " checked" : ""}> ${kept}</label></p>`;
    into.innerHTML = `
      <h2>${title}</h2>
      ${keeping}
      ${arrange ? `<div class="arrange">${arrange}</div>` : ""}
      <div class="fields">${rows.join("")}</div>`;
    if (focused) into.querySelector<HTMLElement>(`#${CSS.escape(focused)}`)?.focus();
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
      return;
    }
    const prop = target.dataset.prop;
    if (prop === undefined) return;
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
  });

  return {
    show,
    characters,
    choose,
    /** What is offered for the node shown, or for the state, for a test. */
    offered: () => offered,
    /** Choices made so far are made: what a test waits for. */
    settled: () => queue,
  };
}
