// The inspector's edits (PLAN 2.33, ADR-0013): the node selected, its role, style, presets, and
// props, chosen from what the theme names for its type, or what the schema allows. The engine
// says what it offers, the value the state shows, and where that value lives (`Player.choices`);
// the page lays nothing out and keeps no vocabulary of its own.
//
// - Each choice is one `choose` patch, by the user, written where the value lives: the state
//   that sets it, or the node. The status says which states it changes, and it is one step to
//   undo. "Only in this state" keeps it to the state shown (`fork`).
// - A value written out where the theme has names (a color, a text size, a length in canvas
//   units) goes in the deck's `overrides`, the only place it is legal, and shows as an override,
//   in every state.
// - The × beside a value takes it away where it lives, so what is under it shows.
import type { Choices, Edited, Field, Lives } from "./protocol";
import type { Stage } from "./stage";

/** What the inspector's edits ask of the editor around them. */
export interface Around {
  /** The state shown, by its id and its slot's index. */
  shown(): { state: string; index: number } | undefined;
  format(): string | undefined;
  source(): string;
  /** Take `source`, the patch made: one step to undo. */
  apply(source: string, edited: Edited): void;
  say(text: string): void;
}

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

/** The inspector's edits, in `into`, for the node the canvas selects in `stage`'s state shown. */
export function looks(stage: Stage, into: HTMLElement, around: Around) {
  /** The node shown, and what the deck offers for it in the state shown. */
  let node: string | undefined;
  let offered: Choices | undefined;
  /** Choices kept to the state shown (`choose`'s `fork`). */
  let keep = false;
  /** One choice at a time, each made on the source the one before it left. */
  let queue: Promise<unknown> = Promise.resolve();
  let asked = 0;

  /** Show what the deck offers for `next` in the state shown; nothing without one. */
  async function show(next: string | undefined) {
    node = next;
    const now = around.shown();
    const ask = ++asked;
    let found: Choices | undefined;
    if (next !== undefined && now) found = await stage.choices(now.state, next).catch(() => undefined);
    if (ask !== asked) return;
    offered = found;
    render();
  }

  /** The control for field `f`, its id `id`. */
  function control(f: Field, id: string): string {
    const prop = `data-prop="${html(f.prop)}"`;
    const value = f.value;
    // A value no control here sets (a preset with parameters, a gradient) shows as the source has it.
    const kept = (known: unknown[]) =>
      value !== undefined && !known.includes(value) ? `<option value="" selected disabled>${html(spoken(value))}</option>` : "";
    const none = `<option value=""${value === undefined ? " selected" : ""}>${value === undefined ? "the theme's" : "—"}</option>`;
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
        const min = t.min ?? t.above;
        const bounds = `${min !== undefined ? ` min="${min}"` : ""}${t.max !== undefined ? ` max="${t.max}"` : ""}`;
        const step = t.whole ? "1" : t.max !== undefined && t.max <= 1 ? "0.05" : "any";
        const shown = typeof value === "number" ? String(value) : "";
        return `<input id="${id}" type="number" ${prop}${bounds} step="${step}" value="${shown}" placeholder="the theme's">`;
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
    return `<span class="lives">${html(where(f.lives))}</span>${flag}${away}`;
  }

  /** Draw what is offered, the control that had focus keeping it. */
  function render() {
    const now = around.shown();
    if (!offered || !now) {
      into.replaceChildren();
      return;
    }
    const focused = into.contains(document.activeElement) ? document.activeElement?.id : undefined;
    const rows = offered.fields.map((f) => {
      const id = `look-${f.prop.replace(/\//g, "-")}`;
      return `<label for="${id}">${html(f.prop)}</label><span class="control">${control(f, id)}</span><span>${note(f)}</span>`;
    });
    into.innerHTML = `
      <h2>${html(offered.node)} · ${html(offered.type)}</h2>
      <p class="keep"><label><input type="checkbox" data-keep${keep ? " checked" : ""}> only in ${html(now.state)}</label></p>
      <div class="fields">${rows.join("")}</div>`;
    if (focused) into.querySelector<HTMLElement>(`#${CSS.escape(focused)}`)?.focus();
  }

  /** Choose `value` for the node shown's `prop`; `null` takes it away where it lives. */
  function choose(prop: string, value: unknown): Promise<unknown> {
    queue = queue.then(() => make(prop, value));
    return queue;
  }

  async function make(prop: string, value: unknown) {
    const now = around.shown();
    if (!now || node === undefined) return;
    const op = { op: "choose", node, prop, value, state: now.state, ...(keep ? { fork: true } : {}) };
    around.say(`choosing ${node}'s ${prop}…`);
    try {
      const states = await stage.reach([op]);
      const { source, edited } = await stage.make(around.source(), [op], now.index, around.format());
      around.apply(source, edited);
      const reach = states.length === 1 && states[0] === now.state ? "in this state" : `in ${states.length} states`;
      around.say(`${node}'s ${prop}: ${value === null ? "taken away" : spoken(value)} · ${reach}`);
    } catch (e) {
      around.say(`not chosen: ${said(e)}`);
      // The controls go back to what the deck says.
      render();
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
    if (target instanceof HTMLInputElement && target.type === "number" && value !== null) value = Number(value);
    void choose(prop, value);
  });
  into.addEventListener("click", (e) => {
    const away = (e.target as Element).closest<HTMLElement>("[data-away]");
    if (away?.dataset.away) void choose(away.dataset.away, null);
  });

  return {
    show,
    choose,
    /** The node shown, and what is offered for it, for a test. */
    offered: () => offered,
    /** Choices made so far are made: what a test waits for. */
    settled: () => queue,
  };
}
