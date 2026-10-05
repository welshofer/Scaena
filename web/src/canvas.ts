// The editor's canvas (PLAN 2.31, ADR-0013): the preview takes the pointer and the keys, and a
// gesture ends in one `place` patch, made by the user: one step to undo. The engine answers
// every question about where things stand; the page lays nothing out.
//
// - A click selects what `hit` says is topmost there. A press in the node selected, or in what
//   holds it, keeps it, so a drag moves it; a click without a drag then selects what is topmost.
//   Escape selects what holds the node selected.
// - A drag moves the node's layer, painted from the state as laid out at rest, and shows where it
//   would land from its targets: the theme's tracks, the template's slots, a grid container's
//   areas, a stack's order. On drop, where it lands is the patch. With Shift, it goes off the grid,
//   placed by a `rect`, an override lint flags (W301); or, off it, back onto it.
// - A handle resizes it the same way, by tracks. While it is dragged, the text keeps its old
//   wrapping; when the pointer pauses, the preview shows it laid out as the patch would make it.
// - The arrow keys move it a track, or a place along its stack; off the grid, a canvas unit. With
//   Shift, they resize it the same.
// - Before a patch is made, the status says which states it changes ("in 3 states"). It changes
//   the placement where it lives; Alt keeps it to the state shown (`fork`).
import type { Edited, NodeBox, Rect, SnapMode, Snapped, Targets } from "./protocol";
import type { Stage } from "./stage";

/** What the canvas asks of the editor around it. */
export interface Editor {
  /** The state shown, by its id and its slot's index; none while the canvas waits: the source
   * does not compile, or the assistant is working on it. */
  shown(): { state: string; index: number } | undefined;
  format(): string | undefined;
  /** The source as it stands: what a patch is made on. */
  source(): string;
  /** One more with each change to the source: a drag the source changed under is dropped. */
  version(): number;
  /** How `node` is placed in the state shown, resolved: its `at`. */
  at(node: string): Placement | undefined;
  /** Take `source`, a patch's, as one change: one step to undo. */
  apply(source: string, edited: Edited): void;
  undo(): void;
  redo(): void;
  say(text: string): void;
  /** The node selected is now `node`. */
  selected(node: string | undefined): void;
}

/** A node's `at`, resolved. */
export type Placement = Record<string, unknown>;

/** A handle: the edge or corner of a box it moves. */
type Edge = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";
const EDGES: Edge[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];
const CURSORS: Record<Edge, string> = { n: "ns", s: "ns", e: "ew", w: "ew", ne: "nesw", sw: "nesw", nw: "nwse", se: "nwse" };

/** A drag under way: the node, where the pointer went down and is now, the keys held, where the
 * node may go, and where it lands as last asked. */
interface Drag {
  kind: "move" | "resize";
  node: string;
  edge?: Edge;
  from: [number, number];
  at: [number, number];
  shift: boolean;
  alt: boolean;
  /** The source's version when it began. */
  version: number;
  targets: Targets;
  how?: SnapMode;
  snapped?: Snapped | null;
  states?: string[];
  /** What was asked last, so a move that changes nothing asks nothing. */
  asked?: string;
  /** A resize's preview, once the pointer pauses. */
  pause?: ReturnType<typeof setTimeout>;
}

/** The pointer down, not yet moved far enough to drag: on `node` (by a handle, `edge`), and what a
 * click there selects. Until the engine says what is there, `node` is unknown, and a pointer let go
 * meanwhile (`released`) selects what it says once it does. */
interface Press {
  node?: string;
  edge?: Edge;
  from: [number, number];
  client: [number, number];
  click?: string;
  released?: boolean;
}

/** A drag asking where its node may go: the pointer as it is now, the keys held, and whether it
 * was let go. */
interface Starting {
  at: [number, number];
  shift: boolean;
  alt: boolean;
  up?: boolean;
}

/** How far the pointer moves, CSS pixels, before a press is a drag. */
const SLOP = 4;
/** How long a resize pauses before the preview shows it laid out, ms. */
const PAUSE = 300;

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
const has = (at: Placement | undefined, key: string) => at?.[key] !== undefined && at?.[key] !== null;

/** How a drag of a node placed `at`, held as `t` says, snaps: `resize` by a handle. Shift takes a
 * node on the theme's grid off it, by a `rect`, or one off it back onto it. `undefined`: nothing
 * places it that way. */
export function snapOf(t: Targets, at: Placement | undefined, resize: boolean, shift: boolean): SnapMode | undefined {
  switch (t.by) {
    case "stack":
      return resize ? undefined : "order";
    case "frame":
      return "free";
    case "cells":
      return has(at, "area") ? (resize ? undefined : "slot") : resize ? "resize" : "move";
    case "grid":
      if (has(at, "rect") !== shift) return "free";
      if (has(at, "in") && !shift) return resize ? undefined : "slot";
      return resize ? "resize" : "move";
  }
}

/** A placement as the status and the inspector say it. A `rect` on the theme's grid is off it: an
 * override, which lint flags (W301). */
export function placed(at: Placement, by?: Targets["by"]): string {
  const span = (v: unknown) => (Array.isArray(v) ? (v[0] === v[1] ? `${v[0]}` : `${v[0]}–${v[1]}`) : `${v}`);
  if (has(at, "in")) return `slot ${at.in}`;
  if (has(at, "area")) return `area ${at.area}`;
  if (has(at, "rect")) {
    const [x, y, w, h] = at.rect as number[];
    return `rect ${x}, ${y}, ${w} × ${h}${by === "grid" ? ", off the grid" : ""}`;
  }
  if (has(at, "index")) return `index ${at.index}`;
  const cells = [has(at, "col") ? `col ${span(at.col)}` : "", has(at, "row") ? `row ${span(at.row)}` : ""];
  return cells.filter(Boolean).join(", ") || "the grid";
}

const moved = ([x, y, w, h]: Rect, [dx, dy]: [number, number]): Rect => [x + dx, y + dy, w, h];

function resized([x, y, w, h]: Rect, edge: Edge, [dx, dy]: [number, number]): Rect {
  if (edge.includes("e")) w += dx;
  if (edge.includes("w")) [x, w] = [x + Math.min(dx, w - 1), w - Math.min(dx, w - 1)];
  if (edge.includes("s")) h += dy;
  if (edge.includes("n")) [y, h] = [y + Math.min(dy, h - 1), h - Math.min(dy, h - 1)];
  return [x, y, Math.max(1, w), Math.max(1, h)];
}

/** The canvas over `overlay`, which holds an `<svg>` the size of the preview, on `stage`'s state
 * shown. The editor makes one for each bundle it opens. */
export function canvas(stage: Stage, overlay: HTMLElement, editor: Editor) {
  const svg = overlay.querySelector("svg")!;
  /** The canvas in canvas units, and what stands where in the state shown. */
  let size: [number, number] = [1920, 1080];
  let boxes: NodeBox[] = [];
  let selected: string | undefined;
  /** Where the node selected may go: whether it has handles. */
  let aim: Targets | undefined;
  let hovered: string | undefined;
  let press: Press | undefined;
  let starting: Starting | undefined;
  let drag: Drag | undefined;
  /** A drag's request is with the worker: the next waits for it, so a fast drag never queues. */
  let busy = false;
  /** The gesture being made into a patch: the next waits for it, so each is made on the source the
   * one before left, as a key held down repeats. */
  let making: Promise<void> = Promise.resolve();
  const inTurn = (work: () => Promise<void>) => {
    const next = making.then(work);
    making = next.catch(() => {});
    return next;
  };

  const box = (node?: string) => (node === undefined ? undefined : boxes.find((b) => b.node === node));
  const point = (e: MouseEvent): [number, number] => {
    const r = overlay.getBoundingClientRect();
    return [((e.clientX - r.left) / r.width) * size[0], ((e.clientY - r.top) / r.height) * size[1]];
  };
  /** Canvas units to a CSS pixel: what handles and lines are sized in. */
  const unit = () => size[0] / Math.max(1, overlay.getBoundingClientRect().width);
  const inside = ([x, y, w, h]: Rect, [px, py]: [number, number]) => px >= x && px <= x + w && py >= y && py <= y + h;

  /** What stands where in the state shown, asked again: the deck, the state, or the format changed. */
  async function refresh() {
    const shown = editor.shown();
    if (!shown) return;
    ({ boxes, size } = await stage.boxes(shown.state, editor.format()));
    svg.setAttribute("viewBox", `0 0 ${size[0]} ${size[1]}`);
    if (selected !== undefined && !box(selected)) select(undefined);
    else if (selected !== undefined) aimAt(selected);
    draw();
  }

  function select(node: string | undefined) {
    if (node === selected) return;
    selected = node;
    aim = undefined;
    editor.selected(node);
    if (node !== undefined) {
      editor.say(`${node} selected: drag it, or move it with the arrow keys`);
      aimAt(node);
    }
    draw();
  }

  /** Where `node` may go, for its handles. */
  function aimAt(node: string) {
    const shown = editor.shown();
    if (!shown) return;
    stage
      .targets(shown.state, node, editor.format())
      .then((t) => {
        if (selected !== node) return;
        aim = t;
        draw();
      })
      .catch(() => (aim = undefined));
  }

  function draw() {
    const u = unit();
    const parts: string[] = [];
    const rect = ([x, y, w, h]: Rect, cls: string, extra = "") =>
      `<rect class="${cls}" x="${x}" y="${y}" width="${Math.max(0, w)}" height="${Math.max(0, h)}"${extra}/>`;
    const line = (x1: number, y1: number, x2: number, y2: number, cls: string) =>
      `<line class="${cls}" x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}"/>`;
    if (drag) {
      const t = drag.targets;
      const cols = t.columns ?? [];
      const rows = t.rows ?? [];
      if ((drag.how === "move" || drag.how === "resize") && cols.length && rows.length) {
        const [top, bottom] = [rows[0][0], rows[rows.length - 1][1]];
        const [left, right] = [cols[0][0], cols[cols.length - 1][1]];
        for (const [a, b] of cols) parts.push(line(a, top, a, bottom, "track"), line(b, top, b, bottom, "track"));
        for (const [a, b] of rows) parts.push(line(left, a, right, a, "track"), line(left, b, right, b, "track"));
      }
      if (drag.how === "slot") {
        for (const [name, r] of Object.entries(t.slots ?? {})) if (name !== "canvas" && name !== "grid") parts.push(rect(r, "slot"));
      }
      if (drag.how === "order") for (const id of t.flow ?? []) if (id !== drag.node && box(id)) parts.push(rect(box(id)!.rect, "flow"));
      if (drag.how === "free") parts.push(rect(t.within, "slot"));
      const land = drag.snapped?.cell;
      if (land && drag.how === "order") parts.push(line(land[0], land[1], land[0] + land[2], land[1] + land[3], "landing-line"));
      else if (land) parts.push(rect(land, "landing"));
    }
    const chosen = box(selected);
    if (chosen) {
      const r = drag?.kind === "move" ? moved(chosen.rect, [drag.at[0] - drag.from[0], drag.at[1] - drag.from[1]]) : chosen.rect;
      parts.push(rect(r, "selected"));
      if (!drag && aim && snapOf(aim, editor.at(chosen.node), true, false)) {
        const s = 8 * u;
        const [x, y, w, h] = r;
        const spot: Record<Edge, [number, number]> = {
          nw: [x, y], n: [x + w / 2, y], ne: [x + w, y], e: [x + w, y + h / 2],
          se: [x + w, y + h], s: [x + w / 2, y + h], sw: [x, y + h], w: [x, y + h / 2],
        };
        for (const edge of EDGES) {
          const [cx, cy] = spot[edge];
          parts.push(rect([cx - s / 2, cy - s / 2, s, s], "handle", ` data-edge="${edge}" style="cursor:${CURSORS[edge]}-resize"`));
        }
      }
    }
    const over = box(hovered);
    if (over && over.node !== selected && !drag) parts.push(rect(over.rect, "hover"));
    svg.innerHTML = parts.join("");
  }

  /** Say where the drag lands, and which states that changes. */
  function tell(d: Drag) {
    const state = editor.shown()?.state;
    const op = d.snapped?.patch[0] as { at?: Placement } | undefined;
    if (!d.how) return editor.say(`${d.node} is placed by name: drag it into another slot, or with Shift off the grid`);
    if (!op) return editor.say(`${d.node} stays where it is`);
    const n = d.states?.length ?? 0;
    const where = n === 1 && d.states?.[0] === state ? "in this state" : `in ${n} states`;
    const keep = d.alt ? ` · kept to ${state}` : n > 1 ? ` · Alt keeps it to ${state}` : "";
    editor.say(`${d.node} → ${placed(op.at ?? {}, d.targets.by)} · ${where}${keep}`);
  }

  /** The box `d` leaves, and how it snaps, with the pointer where it is now. */
  function aimed(d: Drag): { how?: SnapMode; to: Rect; by: [number, number] } {
    const by: [number, number] = [d.at[0] - d.from[0], d.at[1] - d.from[1]];
    const how = snapOf(d.targets, editor.at(d.node), d.kind === "resize", d.shift);
    const to = d.kind === "move" ? moved(d.targets.cell, by) : resized(d.targets.cell, d.edge!, by);
    return { how, to, by };
  }

  /** Ask where the drag lands now, and paint its node there: one request at a time, the latest
   * pointer's. */
  async function pump(d: Drag) {
    const shown = editor.shown();
    if (busy || drag !== d || !shown) return;
    const { how, to, by } = aimed(d);
    const asked = JSON.stringify([by, how, d.alt]);
    if (asked === d.asked) return;
    d.asked = asked;
    d.how = how;
    busy = true;
    try {
      const move = { by: d.kind === "move" ? by : undefined, snap: how ? { how, to, fork: d.alt } : undefined };
      const reply = await stage.drag(shown.state, d.node, move, editor.format());
      if (drag !== d) return;
      d.snapped = how ? reply.snapped : null;
      d.states = reply.states;
      draw();
      tell(d);
      if (d.kind === "resize") {
        clearTimeout(d.pause);
        d.pause = setTimeout(() => void preview(d), PAUSE);
      }
    } catch (e) {
      editor.say(`error: ${said(e)}`);
    } finally {
      busy = false;
      if (drag === d) void pump(d);
    }
  }

  /** A resize that paused: the preview shows the node laid out as its patch would make it. */
  async function preview(d: Drag) {
    const shown = editor.shown();
    if (busy || drag !== d || !shown || !d.how || !d.snapped?.patch.length) return;
    busy = true;
    try {
      const { how, to } = aimed(d);
      if (how) await stage.drag(shown.state, d.node, { snap: { how, to, fork: d.alt }, preview: true }, editor.format());
    } catch (e) {
      editor.say(`error: ${said(e)}`);
    } finally {
      busy = false;
      if (drag === d) void pump(d);
    }
  }

  /** The drag ends where the pointer let go: the patch where it lands, or, where it lands nowhere
   * new, the state as it stands. */
  async function drop(d: Drag) {
    clearTimeout(d.pause);
    const shown = editor.shown();
    if (!shown || editor.version() !== d.version) return still("the source changed under the drag: nothing is placed");
    const { how, to } = aimed(d);
    let snapped: Snapped | null | undefined;
    try {
      if (how) snapped = (await stage.drag(shown.state, d.node, { snap: { how, to, fork: d.alt } }, editor.format())).snapped;
    } catch (e) {
      return still(`not placed: ${said(e)}`);
    }
    if (!snapped?.patch.length) return still(`${d.node} stays where it is`);
    await commit(snapped.patch);
  }

  /** The state shown as it stands, after a drag that places nothing. */
  async function still(why: string) {
    drag = undefined;
    draw();
    editor.say(why);
    const shown = editor.shown();
    if (shown) await stage.rest(shown.state, editor.format()).catch((e) => editor.say(`error: ${said(e)}`));
  }

  /** Make `ops`, by the user, on the source as it stands: one change to undo. */
  async function commit(ops: unknown[]) {
    const shown = editor.shown();
    if (!shown) return;
    const op = ops[0] as { node?: string; at?: Placement } | undefined;
    editor.say("placing…");
    try {
      const { source, edited } = await stage.place(editor.source(), ops, shown.index, editor.format());
      editor.apply(source, edited);
      await refresh();
      editor.say(`${op?.node} placed: ${placed(op?.at ?? {}, aim?.by)}`);
    } catch (e) {
      await still(`not placed: ${said(e)}`);
    }
  }

  /** Move the node selected a step, or, to `grow` it, resize it: a track on a grid, a place along
   * its stack, a canvas unit off the grid. `fork` keeps it to the state shown. */
  async function nudge(node: string, [sx, sy]: [number, number], grow: boolean, fork: boolean) {
    const shown = editor.shown();
    if (!shown) return;
    const t = await stage.targets(shown.state, node, editor.format());
    const how = snapOf(t, editor.at(node), grow, false);
    const [x, y, w, h] = t.cell;
    let to: Rect | undefined;
    if (how === "order") {
      const flow = t.flow ?? [];
      const next = box(flow[flow.indexOf(node) + (sx || sy)])?.rect;
      // Past the middle of the one before or after it.
      const s = sx + sy;
      if (next) to = [next[0] + next[2] / 2 + s - w / 2, next[1] + next[3] / 2 + s - h / 2, w, h];
    } else if (how === "free") {
      to = grow ? [x, y, w + sx, h + sy] : [x + sx, y + sy, w, h];
    } else if (how === "move" || how === "resize") {
      // The next track's start (a move) or end (a resize) past the box's start or end.
      const step = (tracks: [number, number][], from: number, s: number, end: 0 | 1) => {
        const lines = tracks.map((track) => track[end]);
        const found = s > 0 ? lines.find((v) => v > from + 0.5) : [...lines].reverse().find((v) => v < from - 0.5);
        return found === undefined ? undefined : found - from;
      };
      const [cols, rows] = [t.columns ?? [], t.rows ?? []];
      const d = grow
        ? [sx ? step(cols, x + w, sx, 1) : 0, sy ? step(rows, y + h, sy, 1) : 0]
        : [sx ? step(cols, x, sx, 0) : 0, sy ? step(rows, y, sy, 0) : 0];
      if (d[0] !== undefined && d[1] !== undefined) to = grow ? [x, y, w + d[0], h + d[1]] : [x + d[0], y + d[1], w, h];
    }
    if (!how) return editor.say(`${node} is placed by name: drag it into another slot`);
    if (!to) return editor.say(`${node} is at the edge`);
    try {
      const { snapped } = await stage.drag(shown.state, node, { snap: { how, to, fork } }, editor.format());
      if (!snapped?.patch.length) return editor.say(`${node} stays where it is`);
      await commit(snapped.patch);
    } catch (e) {
      editor.say(`not placed: ${said(e)}`);
    }
  }

  overlay.onpointerdown = async (e) => {
    if (e.button !== 0) return;
    overlay.focus();
    const shown = editor.shown();
    if (!shown || drag) return editor.say(shown ? "" : "the canvas waits for a source that compiles");
    overlay.setPointerCapture(e.pointerId);
    const from = point(e);
    const client: [number, number] = [e.clientX, e.clientY];
    const edge = (e.target as Element).closest?.("[data-edge]")?.getAttribute("data-edge") as Edge | null;
    if (edge && selected !== undefined) {
      press = { node: selected, edge, from, client };
      return;
    }
    const mine: Press = { from, client };
    press = mine;
    const hits = await stage.hit(shown.state, from, editor.format()).catch(() => []);
    const top = hits[0];
    const chain = top ? [top.node, ...top.containers] : [];
    // A click let go before the engine answered selects what is topmost.
    if (mine.released) return select(top?.node);
    if (press !== mine) return;
    // In the node selected, or in what holds it, the press keeps it: a drag moves it, and a click
    // selects what is topmost.
    if (selected !== undefined && chain.includes(selected)) [mine.node, mine.click] = [selected, top?.node];
    else {
      select(top?.node);
      mine.node = top?.node;
    }
  };

  overlay.onpointermove = (e) => {
    const at = point(e);
    if (starting) {
      [starting.at, starting.shift, starting.alt] = [at, e.shiftKey, e.altKey];
      return;
    }
    if (drag) {
      [drag.at, drag.shift, drag.alt] = [at, e.shiftKey, e.altKey];
      if (drag.kind === "move") draw();
      void pump(drag);
      return;
    }
    if (!press) {
      // What a click would select, from the boxes the engine gave: nothing is asked.
      const under = boxes.filter((b) => b.draws && inside(b.rect, at)).at(-1)?.node;
      if (under !== hovered) {
        hovered = under;
        draw();
      }
      return;
    }
    if (!press.node || Math.hypot(e.clientX - press.client[0], e.clientY - press.client[1]) < SLOP) return;
    const shown = editor.shown();
    const begun = press;
    press = undefined;
    if (!shown) return;
    const version = editor.version();
    const now: Starting = { at, shift: e.shiftKey, alt: e.altKey };
    starting = now;
    void stage
      .targets(shown.state, begun.node!, editor.format())
      .then((targets) => {
        if (starting !== now) return;
        starting = undefined;
        const d: Drag = {
          kind: begun.edge ? "resize" : "move",
          node: begun.node!,
          edge: begun.edge,
          from: begun.from,
          at: now.at,
          shift: now.shift,
          alt: now.alt,
          version,
          targets,
        };
        hovered = undefined;
        if (now.up) return void inTurn(() => drop(d));
        drag = d;
        void pump(d);
      })
      .catch((err) => {
        starting = undefined;
        editor.say(`error: ${said(err)}`);
      });
  };

  overlay.onpointerup = (e) => {
    if (overlay.hasPointerCapture(e.pointerId)) overlay.releasePointerCapture(e.pointerId);
    if (starting) {
      [starting.at, starting.shift, starting.alt, starting.up] = [point(e), e.shiftKey, e.altKey, true];
      return;
    }
    if (drag) {
      const d = drag;
      drag = undefined;
      [d.at, d.shift, d.alt] = [point(e), e.shiftKey, e.altKey];
      void inTurn(() => drop(d));
      return;
    }
    const clicked = press;
    press = undefined;
    if (!clicked) return;
    if (clicked.node === undefined && !clicked.edge) clicked.released = true;
    else if (clicked.click !== undefined) select(clicked.click);
  };

  overlay.onpointercancel = () => {
    press = starting = undefined;
    if (drag) void still("the drag was cancelled");
  };

  overlay.onpointerleave = () => {
    if (hovered === undefined) return;
    hovered = undefined;
    draw();
  };

  overlay.onkeydown = (e) => {
    const mod = e.metaKey || e.ctrlKey;
    const key = e.key.toLowerCase();
    if (mod && (key === "z" || key === "y")) {
      e.preventDefault();
      return key === "y" || e.shiftKey ? editor.redo() : editor.undo();
    }
    if (e.key === "Escape") {
      if (starting) {
        e.preventDefault();
        starting = undefined;
        return editor.say("the drag was cancelled");
      }
      if (drag) {
        e.preventDefault();
        return void still("the drag was cancelled");
      }
      if (selected !== undefined) {
        e.preventDefault();
        select(box(selected)?.parent ?? undefined);
        if (selected === undefined) editor.say("nothing selected");
      }
      return;
    }
    const steps: Record<string, [number, number]> = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
    const step = steps[e.key];
    if (!step || selected === undefined || drag || mod) return;
    e.preventDefault();
    const [node, grow, fork] = [selected, e.shiftKey, e.altKey];
    void inTurn(() => nudge(node, step, grow, fork));
  };

  const sized = new ResizeObserver(() => draw());
  sized.observe(overlay);

  return {
    refresh,
    /** Let the overlay go: the editor opens another bundle, which makes a canvas of its own. */
    close: () => {
      sized.disconnect();
      drag = press = starting = undefined;
      svg.replaceChildren();
    },
    /** Select `node`, as a click on it does; nothing with `undefined`. */
    select,
    selected: () => selected,
    boxes: () => boxes,
    size: () => size,
    /** Where `node` may go in the state shown. */
    targets: (node: string) => {
      const shown = editor.shown();
      return shown ? stage.targets(shown.state, node, editor.format()) : Promise.reject(new Error("nothing is shown"));
    },
    /** Whether a drag is under way, or its request with the worker. */
    busy: () => busy || drag !== undefined || starting !== undefined,
  };
}
