// Lint's findings on the canvas (PLAN 2.49, ADR-0013): each stands on what it is about, in the
// state shown and the format shown. The page judges nothing itself: lint says what it found, where
// the source writes it, and whether it holds in the format shown (`Finding.shown`).
//
// - Findings about a node stand on its box: one mark at its top right corner, which counts them
//   and takes the color of the worst. A document rule's finding about a node, in no state, stands
//   on it in every state that shows it. Findings about the state, or about a node it does not
//   show, stand on the state: a mark at the canvas's top left corner.
// - A mark opened says each finding, its hint, and where the source writes it, with its fix where
//   lint has one. Lint keeps a fix only once laying the state out again with it took the finding
//   away (SPEC §7.4); one taken is one patch, one step to undo. A node's mark selects it too.
// - A finding no fix can make, about a property the inspector offers, takes the person there:
//   W410's image without alt text opens the inspector on the image, its description in focus
//   (PLAN 2.56).
// - The marks step aside while a drag, a marquee, or a drawing goes, and while the cue plays.
import type { Finding, NodeBox, Rect } from "./protocol";

/** What the marks ask of the canvas they stand on. */
export interface MarksHost {
  /** The state shown, by its id; none while the canvas waits. */
  shown(): { state: string; index: number } | undefined;
  /** What stands where in the state shown, canvas units; `undefined` until it is known. */
  boxes(): NodeBox[] | undefined;
  /** The part of the canvas the preview shows, canvas units. */
  view(): Rect;
  /** Select `node`, as a click on it does. */
  select(node: string): void;
  /** Take `f`'s fix: one patch, one step to undo. */
  fix(f: Finding): Promise<void>;
  /** Show where the source writes what `f` is about. */
  go(f: Finding): void;
  /** Open the inspector on `f`'s node, its field for `prop` in focus (PLAN 2.56). */
  edit(f: Finding, prop: string): void;
  say(text: string): void;
}

/** What a finding no fix can make asks a person to set, by its code: the property, and the button
 * that takes them there. */
const EDITS: Record<string, [prop: string, label: string]> = { W410: ["alt", "Describe it"] };

const SEVERITIES = ["error", "warning", "info"] as const;
type Severity = (typeof SEVERITIES)[number];
const html = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
/** How far a mark's middle stands from the edge of what is shown, CSS px, and how far apart two
 * marks' middles must stand. */
const [EDGE, APART] = [12, 20];

/** What one mark stands for: a node's findings, or the state's. */
interface Mark {
  /** The node, or `undefined` for the state. */
  node?: string;
  findings: Finding[];
  worst: Severity;
}

/** `1 error, 2 warnings`. */
export function counted(findings: Finding[]): string {
  return SEVERITIES.map((s) => [s, findings.filter((f) => f.severity === s).length] as const)
    .filter(([, n]) => n > 0)
    .map(([s, n]) => `${n} ${s}${n === 1 ? "" : "s"}`)
    .join(", ");
}

/** The worst severity among `findings`. */
export const worst = (findings: Finding[]): Severity => SEVERITIES.find((s) => findings.some((f) => f.severity === s)) ?? "info";

/** A fix as words: what each of its operations sets, `fit: shrink`. */
export function fixing(fix: unknown[]): string {
  return fix
    .map((op) => {
      const { op: how, path, value } = op as { op?: string; path?: string; value?: unknown };
      const key = (path ?? "").split("/").pop()!.replace(/~1/g, "/").replace(/~0/g, "~");
      if (how === "remove") return `no ${key}`;
      return `${key}: ${typeof value === "string" ? value : JSON.stringify(value)}`;
    })
    .join(", ");
}

/** The marks in `layer`, over the canvas, and the popover `pop` a mark opens. */
export function marks(layer: HTMLElement, pop: HTMLElement, host: MarksHost) {
  let findings: Finding[] = [];
  /** What the marks were last drawn from: drawn again only when it changes. */
  let drawn = "";
  /** The mark the popover is open on: its node, or `null` for the state. */
  let opened: string | null | undefined;
  /** Whether the marks step aside. */
  let aside = false;

  /** The marks for the state shown, in the format shown: each node's, in paint order, then the
   * state's. None until what stands where in it is known. */
  function current(): Mark[] {
    const shown = host.shown();
    const boxes = host.boxes();
    if (!shown || !boxes) return [];
    const boxed = new Set(boxes.map((b) => b.node));
    // A document rule's finding about a node, in no state (W410's image without alt text), stands on
    // the node wherever it is shown.
    const here = findings.filter(
      (f) => f.shown && (f.state === shown.state || (f.state === undefined && f.node !== undefined && boxed.has(f.node))),
    );
    const by = new Map<string | undefined, Finding[]>();
    for (const f of here) {
      const node = f.node !== undefined && boxed.has(f.node) ? f.node : undefined;
      by.set(node, [...(by.get(node) ?? []), f]);
    }
    const order = boxes.map((b) => b.node);
    return [...by]
      .sort(([a], [b]) => (a === undefined ? 1 : b === undefined ? -1 : order.indexOf(a) - order.indexOf(b)))
      .map(([node, found]) => ({ node, findings: found, worst: worst(found) }));
  }

  /** Where each mark stands, CSS px in the layer: a node's at its box's top right corner, kept
   * inside what is shown and clear of the marks before it; none for a box out of view. */
  function placed(all: Mark[]): { mark: Mark; at: [number, number] }[] {
    const { width, height } = layer.getBoundingClientRect();
    const view = host.view();
    const px = ([x, y]: [number, number]): [number, number] => [((x - view[0]) / view[2]) * width, ((y - view[1]) / view[3]) * height];
    const clamp = (v: number, hi: number) => Math.min(Math.max(v, EDGE), Math.max(EDGE, hi - EDGE));
    const out: { mark: Mark; at: [number, number] }[] = [];
    for (const mark of all) {
      let at: [number, number];
      if (mark.node === undefined) at = [EDGE, EDGE];
      else {
        const box = host.boxes()!.find((b) => b.node === mark.node)!.rect;
        const [left, top] = px([box[0], box[1]]);
        const [right, bottom] = px([box[0] + box[2], box[1] + box[3]]);
        if (right < 0 || bottom < 0 || left > width || top > height) continue;
        at = [clamp(right, width), clamp(top, height)];
      }
      // Clear of the marks already placed: a step left, then a step down.
      for (let tries = 0; tries < 12 && out.some((o) => Math.hypot(o.at[0] - at[0], o.at[1] - at[1]) < APART); tries++) {
        at = at[0] - APART >= EDGE ? [at[0] - APART, at[1]] : [at[0], at[1] + APART];
      }
      out.push({ mark, at });
    }
    return out;
  }

  /** The words a mark is read by. */
  const named = (m: Mark) => `${m.node ?? `the state ${host.shown()?.state ?? ""}`}: ${counted(m.findings)}`;

  /** Draw the marks, if what they stand for or where moved. */
  function draw() {
    const all = aside ? [] : current();
    const at = placed(all);
    const key = JSON.stringify([at.map(({ mark, at }) => [mark.node ?? null, at, mark.findings.map((f) => [f.code, f.severity, f.message])])]);
    if (key === drawn) return;
    drawn = key;
    const focused = layer.contains(document.activeElement) ? (document.activeElement as HTMLElement).dataset.mark : undefined;
    layer.innerHTML = at
      .map(({ mark, at: [x, y] }) => {
        const id = mark.node ?? "";
        const tip = mark.findings.map((f) => `${f.code} ${f.message}`).join("\n");
        return `<button type="button" class="mark ${mark.worst}${mark.node === undefined ? " state" : ""}" data-mark="${html(id)}" style="left:${x.toFixed(1)}px;top:${y.toFixed(1)}px" aria-label="${html(named(mark))}" title="${html(tip)}">${mark.findings.length}</button>`;
      })
      .join("");
    if (focused !== undefined) layer.querySelector<HTMLElement>(`[data-mark="${CSS.escape(focused)}"]`)?.focus();
    // An open popover follows its mark, and says what it stands for now.
    if (opened !== undefined && pop.matches(":popover-open")) {
      const mark = all.find((m) => (m.node ?? null) === opened);
      if (mark) open(mark, false);
      else pop.hidePopover();
    }
  }

  /** The popover over `mark`: each finding, its hint, and its fix. */
  function open(mark: Mark, focus: boolean) {
    opened = mark.node ?? null;
    const button = layer.querySelector<HTMLElement>(`[data-mark="${CSS.escape(mark.node ?? "")}"]`);
    pop.innerHTML = `<h2>${html(mark.node ?? `The state ${host.shown()?.state ?? ""}`)}<span>${html(counted(mark.findings))}</span></h2><ol>${mark.findings
      .map(
        (f, i) => `<li class="${f.severity}"><p><span class="code">${html(f.code)}</span>${html(f.message)}${f.format ? ` <span class="where">(${html(f.format)})</span>` : ""}</p>${
          f.hint ? `<p class="hint">${html(f.hint)}</p>` : ""
        }<p class="acts">${
          f.fix && f.fixable
            ? `<button type="button" data-fix="${i}" aria-label="Fix ${html(f.code)}: ${html(fixing(f.fix))}">Fix</button><span class="change">${html(fixing(f.fix))}</span>`
            : ""
        }${f.node && EDITS[f.code] ? `<button type="button" data-edit="${i}">${html(EDITS[f.code][1])}</button>` : ""}${
          f.at ? `<button type="button" data-go="${i}">In the source</button>` : ""
        }</p></li>`,
      )
      .join("")}</ol>`;
    pop.querySelectorAll<HTMLButtonElement>("[data-fix]").forEach((b) => {
      b.onclick = () => void take(mark.findings[Number(b.dataset.fix)]);
    });
    pop.querySelectorAll<HTMLButtonElement>("[data-edit]").forEach((b) => {
      b.onclick = () => {
        pop.hidePopover();
        const f = mark.findings[Number(b.dataset.edit)];
        host.edit(f, EDITS[f.code][0]);
      };
    });
    pop.querySelectorAll<HTMLButtonElement>("[data-go]").forEach((b) => {
      b.onclick = () => {
        pop.hidePopover();
        host.go(mark.findings[Number(b.dataset.go)]);
      };
    });
    if (!pop.matches(":popover-open")) pop.showPopover();
    // Beside the mark, inside the window.
    const r = button?.getBoundingClientRect() ?? layer.getBoundingClientRect();
    const [w, h] = [pop.offsetWidth, pop.offsetHeight];
    pop.style.left = `${Math.max(8, Math.min(r.left, innerWidth - w - 8))}px`;
    pop.style.top = `${r.bottom + 6 + h > innerHeight - 8 ? Math.max(8, r.top - h - 6) : r.bottom + 6}px`;
    if (focus) pop.querySelector<HTMLElement>("button")?.focus();
  }

  /** Take `f`'s fix, and say so. */
  async function take(f: Finding) {
    pop.hidePopover();
    const how = fixing(f.fix ?? []);
    host.say(`fixing ${f.code}${f.node ? ` on ${f.node}` : ""}…`);
    try {
      await host.fix(f);
      host.say(`${f.code} fixed${f.node ? ` on ${f.node}` : ""}: ${how}`);
    } catch (e) {
      host.say(`not fixed: ${said(e)}`);
    }
  }

  layer.onclick = (e) => {
    const button = (e.target as Element).closest<HTMLElement>("[data-mark]");
    if (!button) return;
    const node = button.dataset.mark || undefined;
    const mark = current().find((m) => m.node === node);
    if (!mark) return;
    if (opened === (node ?? null) && pop.matches(":popover-open")) return pop.hidePopover();
    if (node !== undefined) host.select(node);
    // Opened by a key, the popover takes the focus; by the pointer, it is read where it stands.
    open(mark, e.detail === 0);
  };
  // Closed, the popover gives the focus back to its mark, if it had it.
  pop.addEventListener("toggle", (e) => {
    if ((e as ToggleEvent).newState !== "closed") return;
    const had = pop.contains(document.activeElement) || document.activeElement === document.body;
    const mark = opened === undefined ? undefined : layer.querySelector<HTMLElement>(`[data-mark="${CSS.escape(opened ?? "")}"]`);
    opened = undefined;
    if (had) mark?.focus();
  });

  return {
    /** What lint found now: every finding, each saying whether it holds in the format shown. */
    found(next: Finding[]) {
      findings = next;
      draw();
    },
    draw,
    /** Step aside, or come back. */
    aside(on: boolean) {
      if (aside === on) return;
      aside = on;
      if (on && pop.matches(":popover-open")) pop.hidePopover();
      draw();
    },
    /** The marks shown, for a test: each node's (or `null`, the state's) worst severity, how many
     * findings it holds, and where it stands, CSS px in the layer. */
    shown: () =>
      [...layer.querySelectorAll<HTMLElement>("[data-mark]")].map((b) => ({
        node: b.dataset.mark || null,
        severity: SEVERITIES.find((s) => b.classList.contains(s))!,
        count: Number(b.textContent),
        at: [parseFloat(b.style.left), parseFloat(b.style.top)] as [number, number],
      })),
  };
}
