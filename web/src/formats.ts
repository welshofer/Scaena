// Every format at once (PLAN 2.62, SPEC §3.4): the state shown in each of the deck's formats,
// side by side under the canvas, each as the canvas would show it there.
//
// - A figure for the deck's own canvas, then one for each format the deck lists, in the order the
//   format menu lists them, all one height.
// - The worker paints each after each frame of the canvas, so they play as it plays (a cue, the
//   deck run), and come to rest with it (`Player.pixelsIn`): each format lays a state out once.
// - Each counts the findings about the state shown that hold in its format, in the color of the
//   worst, as the strip counts a state's (PLAN 2.49).
// - A click on one, or Enter, opens the canvas in its format, as the format menu does; the one the
//   canvas shows is pressed.
import { counted, worst } from "./marks";
import type { Finding } from "./protocol";
import type { Stage } from "./stage";

/** What the row asks of the editor around it. */
export interface FormatsEditor {
  /** The state shown, by id. */
  state(): string | undefined;
  /** The format the canvas shows, as the format menu names it: `""` for the deck's own canvas. */
  format(): string;
  /** Open the canvas in `format`, as the format menu does. */
  open(format: string): void;
}

/** How high each figure is, CSS pixels. */
const HEIGHT = 96;

const html = (text: string) => text.replace(/[&<>"]/g, (c) => `&${{ "&": "amp", "<": "lt", ">": "gt", '"': "quot" }[c]};`);

/** The row in `into`, over `stage`, shown and hidden by `toggle`. */
export function formatsRow(stage: Stage, into: HTMLElement, toggle: HTMLButtonElement, editor: FormatsEditor) {
  /** The formats the deck lists besides its own canvas, as the figures show them. */
  let formats: string[] = [];
  /** What lint found last. */
  let findings: Finding[] = [];
  /** What the figures last showed: the state, `t` ms into its cue. */
  let painted: { state: string; t: number; count: number } | undefined;

  stage.onBesides = (state, t) => {
    painted = { state, t, count: (painted?.count ?? 0) + 1 };
  };

  const open = () => !into.hidden;

  /** Make a figure for each format, and hand the worker their canvases to paint: or, hidden, none. */
  function build() {
    if (!open()) {
      into.replaceChildren();
      stage.besides([], 1);
      return;
    }
    const names = ["", ...formats];
    const figures = names.map((name) => {
      const button = document.createElement("button");
      button.type = "button";
      button.dataset.format = name;
      button.title = `Edit the deck ${name ? `in ${name}` : "on its own canvas"}`;
      const canvas = document.createElement("canvas");
      canvas.style.height = `${HEIGHT}px`;
      canvas.setAttribute("aria-hidden", "true");
      const label = Object.assign(document.createElement("span"), { className: "name", textContent: name || "own canvas" });
      button.append(canvas, label);
      button.addEventListener("click", () => editor.open(name));
      return { name, button, canvas };
    });
    into.replaceChildren(...figures.map((f) => f.button));
    stage.besides(
      figures.map(({ name, canvas }) => ({ format: name || undefined, canvas })),
      Math.round(HEIGHT * devicePixelRatio),
    );
    mark();
  }

  /** Mark the format the canvas shows, and count each format's findings about the state shown. */
  function mark() {
    if (!open()) return;
    const state = editor.state();
    const shown = editor.format();
    for (const button of into.querySelectorAll<HTMLButtonElement>("button[data-format]")) {
      const name = button.dataset.format!;
      button.setAttribute("aria-pressed", String(name === shown));
      const mine = findings.filter((f) => f.state === state && f.formats?.includes(name));
      button.querySelector(".found")?.remove();
      const said = mine.length ? counted(mine) : "no findings";
      button.setAttribute("aria-label", `${name || "own canvas"}: ${said}`);
      if (mine.length) {
        button.insertAdjacentHTML(
          "beforeend",
          `<span class="found ${worst(mine)}" title="${html(said)}" aria-hidden="true">${mine.length}</span>`,
        );
      }
    }
  }

  toggle.addEventListener("click", () => show(!open()));

  /** Show the row, or hide it. */
  function show(on: boolean) {
    into.hidden = !on;
    toggle.setAttribute("aria-pressed", String(on));
    build();
  }

  return {
    show,
    /** Whether the row is shown. */
    open,
    /** The deck's formats, as an edit leaves them: the figures follow where they changed. */
    formats(next: string[]) {
      if (next.length === formats.length && next.every((f, i) => f === formats[i])) return;
      formats = [...next];
      build();
    },
    /** What lint found now. */
    found(all: Finding[]) {
      findings = all;
      mark();
    },
    /** The state shown, or the format the canvas shows, changed. */
    shown: () => mark(),
    /** What the figures last showed, and how many times they were painted, for a test. */
    painted: () => painted,
  };
}
