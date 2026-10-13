// The deck's theme, edited (PLAN 2.61, ADR-0016, SPEC §3.6): its colors, its type roles, and its
// spacing in a tab of their own, each change one edit of the theme the deck names, as `scaena
// theme --edit` makes it: RFC 6902 operations on the theme's JSON.
//
// - A color is its swatch and its value as the theme writes it (`#rrggbb[aa]`, `oklch(…)`): the
//   swatch picks one, the value takes any the theme reads. Beside it, the color roles that name it.
// - A type role is its family, one of the theme's, its size, weight, leading, and tracking.
// - The spacing is the grid's gutter, margins, and baseline, and the space unit.
// - From a photo (PLAN 2.94): each PNG and JPEG the bundle holds, a click giving the theme its
//   colors, as `scaena theme --from-photo` does: the photo's hues for the accents and the hue it
//   leans to for the neutrals, on the theme's own tones, each color text is set in kept where it
//   reads on the surfaces.
// - Each change is one edit, refused with why where the deck would not validate in the theme it
//   leaves (a value the theme's schema refuses is E106). ⌘Z in the source undoes it, the theme
//   written back.
// - The panel shows the theme frames are drawn in: an undo, a re-theme, or the assistant's edit
//   shows here as it does on the canvas.
import type { BundleFile, Lexicon, ThemeEdited } from "./protocol";
import type { Stage } from "./stage";
import { named, object as objectSaid } from "./words";

/** What the panel asks of the editor around it. */
export interface ThemeEditor {
  /** Edit the theme by `ops`, or to the colors of the image `photo` the bundle holds, as one change
   * the source's undo takes back; `what` says it in the status. What it did; none where it was not
   * asked, or failed (the editor says why). */
  edit(ops: unknown[], what: string, photo?: string): Promise<ThemeEdited | undefined>;
  /** The images the bundle holds that a theme may take its colors from. */
  photos(): Promise<BundleFile[]>;
  /** The words for the state shown (PLAN 2.99): the theme's names said, its objects by what they
   * show. */
  words(): Lexicon | undefined;
}

type Json = Record<string, unknown>;

/** A bundle file's name, without the folder it is in: `assets/ridge.jpg` is ridge.jpg. */
const fileName = (path: string) => path.replace(/^.*\//, "");
const object = (v: unknown): Json | undefined => (v && typeof v === "object" && !Array.isArray(v) ? (v as Json) : undefined);
/** A JSON Pointer to `keys`, each escaped. */
export const pointer = (...keys: (string | number)[]) => keys.map((k) => `/${String(k).replace(/~/g, "~0").replace(/\//g, "~1")}`).join("");
/** A hex color the swatch can show: `#rrggbb` of `#rrggbb[aa]`; none for another kind. */
const hex = (v: string) => (/^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(v) ? v.slice(0, 7).toLowerCase() : undefined);

/** A type role's fields the panel edits, each with how its input steps, labeled as a type menu
 * labels them (PLAN 2.99). */
const ROLE_FIELDS: { key: string; label: string; step: string; min?: string; max?: string }[] = [
  { key: "size", label: "Size", step: "1", min: "1" },
  { key: "weight", label: "Weight", step: "50", min: "1", max: "1000" },
  { key: "leading", label: "Line Spacing", step: "0.05", min: "0.5" },
  { key: "tracking", label: "Character Spacing", step: "0.005" },
];

/** What each of a margin's numbers is, by how many it has (CSS-style). */
const MARGIN_SIDES: Record<number, string[]> = {
  2: ["top and bottom", "left and right"],
  3: ["top", "left and right", "bottom"],
  4: ["top", "right", "bottom", "left"],
};

/** The panel in `into`, over `stage`. */
export function themePanel(stage: Stage, into: HTMLElement, editor: ThemeEditor) {
  const head = into.querySelector<HTMLElement>("[data-summary]")!;
  const colors = into.querySelector<HTMLUListElement>("[data-colors]")!;
  const photos = into.querySelector<HTMLUListElement>("[data-photos]")!;
  const roles = into.querySelector<HTMLTableSectionElement>("[data-roles] tbody")!;
  const spacing = into.querySelector<HTMLElement>("[data-spacing]")!;
  /** The theme as the panel last drew it, and its text and where it lives. */
  let theme: Json | undefined;
  let drawn: { theme: string; text: string } | undefined;
  /** The names the panel last said the theme's in, as a key. */
  let saidIn = "";
  const sayings = () => {
    const w = editor.words();
    return JSON.stringify([w?.names.color, w?.names["text-role"], w?.names["font-family"]]);
  };
  /** Reads asked for: an answer to one before the last is let go. */
  let reading = 0;
  /** Once the panel's last ask is answered. */
  let settling: Promise<unknown> = Promise.resolve();
  const settle = <T>(work: Promise<T>) => {
    settling = work;
    return work;
  };

  /** Read the theme frames are drawn in again, and draw it where it changed, while the panel is
   * shown. */
  function refresh() {
    if (into.hidden) return settling;
    const asked = ++reading;
    return settle(
      (async () => {
        const got = await stage.themeText().catch(() => null);
        if (asked !== reading) return;
        if (!got) {
          drawn = undefined;
          theme = undefined;
          head.textContent = "The deck names no theme: choose one above the canvas.";
          colors.replaceChildren();
          roles.replaceChildren();
          spacing.replaceChildren();
          return;
        }
        drawPhotos(await editor.photos().catch(() => []));
        if (asked !== reading) return;
        if (drawn && drawn.theme === got.theme && drawn.text === got.text) return;
        drawn = got;
        try {
          theme = object(JSON.parse(got.text));
        } catch {
          theme = undefined;
        }
        draw();
      })(),
    );
  }

  /** Edit the theme by `ops`, or to `photo`'s colors, then read it again: what the edit left, or,
   * refused, what it was. */
  async function change(ops: unknown[], what: string, photo?: string): Promise<ThemeEdited | undefined> {
    const done = await editor.edit(ops, what, photo);
    // An edit refused, or failed, leaves the theme as it was: its inputs show that again.
    drawn = undefined;
    await refresh();
    return done;
  }

  /** A change made in the panel, which `settled()` waits for. */
  const commit = (ops: unknown[], what: string) => void settle(change(ops, what));
  /** The theme given the colors of the image `path` the bundle holds. */
  const fromPhoto = (path: string) => settle(change([], `colors from ${fileName(path)}`, path));
  /** One of the theme's names of `kind` said (PLAN 2.99): `onSurface` is Text, `title` is Title. */
  const said = (kind: string, name: string) => named(editor.words(), kind, name);

  /** Each image the bundle holds, a button that gives the theme its colors. */
  function drawPhotos(files: BundleFile[]) {
    const held = files.map((f) => f.path).join("\n");
    if (photos.dataset.held === held) return;
    photos.dataset.held = held;
    if (!files.length) {
      const none = "The deck holds no photo: drop one on the canvas, and its colors can be the theme's.";
      photos.replaceChildren(Object.assign(document.createElement("li"), { className: "none", textContent: none }));
      return;
    }
    photos.replaceChildren(
      ...files.map((f) => {
        const li = document.createElement("li");
        const button = Object.assign(document.createElement("button"), { type: "button", textContent: fileName(f.path) });
        button.dataset.photo = f.path;
        button.setAttribute("aria-label", `Take the colors of ${fileName(f.path)}`);
        button.title = "Give the theme this photo's colors: its hues for the accents, the hue it leans to for the neutrals";
        button.addEventListener("click", () => void fromPhoto(f.path));
        li.append(button);
        const nodes = [...new Set(f.used.map((u) => objectSaid(editor.words(), u.node)))];
        if (nodes.length) li.append(Object.assign(document.createElement("span"), { className: "uses", textContent: nodes.join(", ") }));
        return li;
      }),
    );
  }

  function draw() {
    saidIn = sayings();
    const name = typeof theme?.name === "string" ? theme.name : "The theme";
    head.textContent =
      drawn?.theme === "(inline)"
        ? `${name}, written in the deck · each change is one edit of it, which ⌘Z undoes`
        : `${name} · each change is one edit of this deck's copy of it, which ⌘Z undoes`;
    drawColors();
    drawRoles();
    drawSpacing();
  }

  function drawColors() {
    const tokens = object(object(theme?.tokens)?.color) ?? {};
    const named = object(object(theme?.tokens)?.roles) ?? {};
    colors.replaceChildren(
      ...Object.entries(tokens).map(([key, value]) => {
        const li = document.createElement("li");
        li.dataset.color = key;
        const text = String(value);
        const id = `theme-color-${key.replace(/[^\w-]/g, "_")}`;
        const shown = hex(text);
        if (shown) {
          const swatch = Object.assign(document.createElement("input"), { type: "color", value: shown });
          swatch.setAttribute("aria-label", `${said("color", key)}: pick a color`);
          swatch.addEventListener("change", () => {
            // The picker keeps the color's alpha, where it has one, and its case.
            const picked = text === text.toUpperCase() ? swatch.value.toUpperCase() : swatch.value;
            const next = text.length === 9 ? picked + text.slice(7) : picked;
            if (next !== text) commit([{ op: "replace", path: pointer("tokens", "color", key), value: next }], `${said("color", key)} ${next}`);
          });
          li.append(swatch);
        } else {
          const swatch = document.createElement("span");
          swatch.className = "swatch";
          swatch.style.background = text;
          li.append(swatch);
        }
        const label = Object.assign(document.createElement("label"), { htmlFor: id, textContent: said("color", key) });
        const input = Object.assign(document.createElement("input"), { type: "text", id, value: text, spellcheck: false });
        input.addEventListener("change", () => {
          const next = input.value.trim();
          if (next && next !== text) commit([{ op: "replace", path: pointer("tokens", "color", key), value: next }], `${said("color", key)} ${next}`);
          else input.value = text;
        });
        li.append(label, input);
        const uses = Object.entries(named)
          .filter(([, color]) => color === key)
          .map(([role]) => said("color", role));
        if (uses.length) li.append(Object.assign(document.createElement("span"), { className: "uses", textContent: uses.join(", ") }));
        return li;
      }),
    );
  }

  function drawRoles() {
    const type = object(theme?.type);
    const families = Object.keys(object(type?.families) ?? {});
    roles.replaceChildren(
      ...Object.entries(object(type?.roles) ?? {}).map(([key, value]) => {
        const role = object(value) ?? {};
        const tr = document.createElement("tr");
        tr.dataset.role = key;
        const roleSaid = said("text-role", key);
        tr.append(Object.assign(document.createElement("th"), { scope: "row", textContent: roleSaid }));
        // The family: one of the theme's, by its font's name.
        const family = document.createElement("select");
        family.setAttribute("aria-label", `${roleSaid}: font`);
        const current = String(role.family ?? "");
        family.append(...families.map((f) => new Option(said("font-family", f), f, f === current, f === current)));
        if (current && !families.includes(current)) family.append(new Option(current, current, true, true));
        family.addEventListener("change", () => {
          const path = pointer("type", "roles", key, "family");
          commit([{ op: "family" in role ? "replace" : "add", path, value: family.value }], `${roleSaid}: font ${said("font-family", family.value)}`);
        });
        const cell = document.createElement("td");
        cell.append(family);
        tr.append(cell);
        for (const field of ROLE_FIELDS) {
          const input = Object.assign(document.createElement("input"), { type: "number", step: field.step });
          if (field.min) input.min = field.min;
          if (field.max) input.max = field.max;
          input.setAttribute("aria-label", `${roleSaid}: ${field.label}`);
          const held = role[field.key];
          input.value = typeof held === "number" ? String(held) : "";
          if (typeof held !== "number") input.placeholder = "—";
          input.addEventListener("change", () => {
            const n = input.valueAsNumber;
            if (!Number.isFinite(n) || n === held) {
              input.value = typeof held === "number" ? String(held) : "";
              return;
            }
            const path = pointer("type", "roles", key, field.key);
            commit([{ op: field.key in role ? "replace" : "add", path, value: n }], `${roleSaid}: ${field.label} ${n}`);
          });
          const td = document.createElement("td");
          td.append(input);
          tr.append(td);
        }
        return tr;
      }),
    );
  }

  /** A number the theme holds at `keys` (or none), as a labelled input that edits it. */
  function number(label: string, keys: (string | number)[], held: unknown, opts: { step: string; min?: string }) {
    const id = `theme-${keys.join("-")}`;
    const input = Object.assign(document.createElement("input"), { type: "number", id, step: opts.step });
    if (opts.min) input.min = opts.min;
    input.value = typeof held === "number" ? String(held) : "";
    if (typeof held !== "number") input.placeholder = "—";
    input.addEventListener("change", () => {
      const n = input.valueAsNumber;
      if (!Number.isFinite(n) || n === held) {
        input.value = typeof held === "number" ? String(held) : "";
        return;
      }
      commit([{ op: held === undefined ? "add" : "replace", path: pointer(...keys), value: n }], `${label}: ${n}`);
    });
    return [Object.assign(document.createElement("label"), { htmlFor: id, textContent: label }), input];
  }

  function drawSpacing() {
    const grid = object(theme?.grid) ?? {};
    const space = object(object(theme?.tokens)?.space) ?? {};
    const fields: HTMLElement[] = [];
    fields.push(...number("Space between columns", ["grid", "gutter"], grid.gutter, { step: "1", min: "0" }));
    const margin = grid.margin;
    if (Array.isArray(margin) && MARGIN_SIDES[margin.length]) {
      MARGIN_SIDES[margin.length].forEach((side, i) => {
        fields.push(...number(`Margin, ${side}`, ["grid", "margin", i], margin[i], { step: "1", min: "0" }));
      });
    } else fields.push(...number("Margins", ["grid", "margin"], margin, { step: "1", min: "0" }));
    fields.push(...number("Baseline grid", ["grid", "baseline"], grid.baseline, { step: "1", min: "1" }));
    fields.push(...number("Spacing step", ["tokens", "space", "unit"], space.unit, { step: "1", min: "1" }));
    spacing.replaceChildren(...fields);
  }

  new MutationObserver(() => {
    if (!into.hidden) void refresh();
  }).observe(into, { attributes: true, attributeFilter: ["hidden"] });

  return {
    refresh,
    /** The deck changed: the theme it is drawn in may have too. */
    edited: () => refresh(),
    /** The words for the theme's names were read again: what is shown says them so. */
    worded: () => {
      if (!into.hidden && theme && saidIn !== sayings()) draw();
    },
    /** The theme as the panel last drew it, for a test. */
    theme: () => theme,
    /** Edit the theme by `ops`, as a change in the panel does. */
    change: (ops: unknown[], what = "an edit") => settle(change(ops, what)),
    /** Give the theme the colors of the image `path` the bundle holds, as its button does. */
    fromPhoto,
    /** Once the panel's last ask is answered. */
    settled: () => settling,
  };
}
