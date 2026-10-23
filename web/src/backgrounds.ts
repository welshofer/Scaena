// A slide's background (PLAN 2.148): with no node selected, the inspector offers None and each of
// the theme's shader presets, each a small swatch of the slide drawn with it by the CPU painter,
// moving a few frames a second while the row shows and still under `prefers-reduced-motion`
// (`Player.backgroundsPainted`, `Player.backgroundPixels`). The row is a listbox with roving focus:
// the arrows, Home, and End move along it, and an option pointed at or focused shows on the canvas,
// nothing made (`Player.preview`); a click, Enter, or Space gives the slide it, one patch and one
// step to undo, which enters in the slide's first state and stays on its slide
// (`scaena_ops::backgrounds`). A slide with a chart or a table says why backgrounds stay off it
// (lint W311): each is dimmed and refused with that reason, and None takes one away.

import type { Stage } from "./stage";
import type { Backgrounds, Edited } from "./protocol";

/** What the background row asks of the editor around it. */
export interface Around {
  shown(): { state: string; index: number } | undefined;
  slide(state: string): string;
  format(): string | undefined;
  source(): string;
  apply(source: string, edited: Edited, said?: string): void;
  say(text: string): void;
}

/** How high a swatch is painted, CSS pixels: about its column's width at 16:9, so it shows crisp. */
const SWATCH = 64;
/** How often a swatch moves on, milliseconds: a few frames a second. */
const FRAME = 250;

const html = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
const said = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** The backgrounds offered, as last drawn, for a state in a format. */
interface Drawn {
  state: string;
  format: string | undefined;
  backgrounds: Backgrounds;
}

export function backgrounds(stage: Stage, around: Around) {
  let drawn: Drawn | undefined;
  let box: HTMLElement | undefined;
  let asking = 0;
  /** The option the canvas shows, and the one it is to show: a preview at a time, the last asked
   * for winning. */
  let previewed: number | undefined;
  let previewing: number | undefined;
  let previews: Promise<void> | undefined;
  /** The swatches' clock, seconds, and what moves it. */
  let t = 0;
  let ticking: ReturnType<typeof setInterval> | undefined;
  let painting = false;
  let seen = false;
  const still = window.matchMedia("(prefers-reduced-motion: reduce)");
  const sight = new IntersectionObserver((entries) => {
    seen = entries.some((e) => e.isIntersecting);
    tick();
  });

  /** Into `section`, the row for the state shown: drawn now from what was drawn for it, and asked
   * for again where there is none. */
  function attach(section: HTMLElement) {
    if (box !== section) {
      sight.disconnect();
      box = section;
      sight.observe(section);
    }
    const now = around.shown();
    if (drawn && now && drawn.state === now.state && drawn.format === around.format()) fill(drawn);
    else void ask();
  }

  async function ask() {
    const now = around.shown();
    if (!now || !box?.isConnected) return;
    const turn = ++asking;
    const format = around.format();
    const ratio = Math.min(2, window.devicePixelRatio || 1);
    const got = await stage.backgrounds(now.state, Math.round(SWATCH * ratio), format).catch(() => undefined);
    if (turn !== asking || !got || around.shown()?.state !== now.state) return;
    drawn = { state: now.state, format, backgrounds: got };
    t = 0;
    fill(drawn);
  }

  /** The option focused, or the one chosen: the one that takes the row's focus. */
  const chosen = (b: Backgrounds) => Math.max(0, b.options.findIndex((o) => o.current));

  function fill(these: Drawn) {
    if (!box?.isConnected) return;
    const b = these.backgrounds;
    const focused = box.contains(document.activeElement) ? Number((document.activeElement as HTMLElement).dataset.background) : undefined;
    const rover = focused ?? chosen(b);
    const options = b.options.map((o, i) => {
      const held = b.held !== undefined && o.preset !== undefined;
      const what = o.said ? `${o.label}, ${o.said}` : o.label;
      return `<div role="option" id="background-${i}" data-background="${i}" aria-selected="${o.current === true}"${held ? ` aria-disabled="true"` : ""} tabindex="${i === rover ? 0 : -1}" title="${html(held ? `${what}: ${b.held}` : what)}">
        <canvas aria-hidden="true" width="${o.width}" height="${o.height}"></canvas><span class="name">${html(o.label)}</span>${o.said ? `<span class="kind">${html(o.said)}</span>` : ""}</div>`;
    });
    const held = b.held ? `<p class="held" id="backgrounds-held">${html(b.held)}</p>` : "";
    box.innerHTML = `<h3 id="backgrounds-title">Background</h3>${held}<div class="swatches" role="listbox" aria-orientation="horizontal" aria-labelledby="backgrounds-title"${held ? ` aria-describedby="backgrounds-held"` : ""}>${options.join("")}</div>`;
    box.hidden = b.options.length < 2;
    put(b.options.map((o) => o.pixels), b);
    if (focused !== undefined) box.querySelector<HTMLElement>(`[data-background="${focused}"]`)?.focus();
    tick();
  }

  /** Each swatch's pixels into its canvas. */
  function put(pixels: ArrayBuffer[], b: Backgrounds) {
    const canvases = box?.querySelectorAll("canvas") ?? [];
    canvases.forEach((canvas, i) => {
      const o = b.options[i];
      if (!o || !pixels[i] || pixels[i].byteLength !== o.width * o.height * 4) return;
      canvas.getContext("2d")?.putImageData(new ImageData(new Uint8ClampedArray(pixels[i]), o.width, o.height), 0, 0);
    });
  }

  /** The swatches move while the row shows, the page is seen, and motion is welcome. */
  function tick() {
    const moving = seen && !still.matches && !!box?.isConnected && !box.hidden && document.visibilityState === "visible" && drawn !== undefined;
    if (moving && ticking === undefined) ticking = setInterval(() => void frame(), FRAME);
    if (!moving && ticking !== undefined) {
      clearInterval(ticking);
      ticking = undefined;
    }
  }

  async function frame() {
    if (painting || !drawn) return;
    if (!box?.isConnected) return tick();
    painting = true;
    const these = drawn;
    t += FRAME / 1000;
    const pixels = await stage.backgroundFrames(t).catch(() => undefined);
    painting = false;
    if (pixels && drawn === these && pixels.length === these.backgrounds.options.length) put(pixels, these.backgrounds);
  }

  /** The slide shown with option `i` on the canvas, nothing made, or as it is with none. */
  function preview(i: number | undefined) {
    previewing = i;
    previews ??= (async () => {
      while (previewed !== previewing) {
        const want = previewing;
        const now = around.shown();
        const o = want !== undefined && drawn?.state === now?.state ? drawn?.backgrounds.options[want] : undefined;
        if (now) {
          const shown = o && !o.current ? stage.preview(now.state, o.patch, around.format()) : stage.rest(now.state, around.format());
          await shown.catch(() => {});
        }
        previewed = want;
      }
      previews = undefined;
    })();
  }

  /** The slide shown given option `i`: one patch, one step to undo. */
  async function choose(i: number) {
    const now = around.shown();
    const b = drawn?.state === now?.state ? drawn?.backgrounds : undefined;
    const o = b?.options[i];
    if (!now || !b || !o || o.current) return;
    const slide = around.slide(b.first);
    if (b.held && o.preset !== undefined) return around.say(`${slide}: ${b.held}`);
    preview(undefined);
    around.say(`Changing the background…`);
    try {
      // Its own patch shows it; the one made sets each text too faint over it in a color that reads.
      const patch = await stage.backgroundPatch(b.first, o.preset);
      const { source, edited } = await stage.make(around.source(), patch, now.index, around.format());
      const told = `${slide}: background ${o.preset === undefined ? "taken away" : o.label}`;
      around.apply(source, edited, told);
      around.say(`${told} · ⌘Z undoes it`);
      drawn = undefined;
      if (box?.isConnected) void ask();
    } catch (e) {
      around.say(`Not changed: ${said(e)}`);
    }
  }

  const option = (target: EventTarget | null) => (target instanceof Element ? target.closest<HTMLElement>(".backgrounds [data-background]") : null);
  const away = (target: EventTarget | null) => !(target instanceof Element && target.closest(".backgrounds"));

  /** The row's own keys, as a listbox's (PLAN 2.75): the arrows, Home, and End move the focus,
   * which previews, and Enter or Space chooses. */
  function keys(e: KeyboardEvent) {
    const on = option(e.target);
    if (!on || !box) return;
    const all = [...box.querySelectorAll<HTMLElement>("[data-background]")];
    const at = all.indexOf(on);
    const to =
      e.key === "ArrowRight" || e.key === "ArrowDown" ? Math.min(at + 1, all.length - 1)
      : e.key === "ArrowLeft" || e.key === "ArrowUp" ? Math.max(at - 1, 0)
      : e.key === "Home" ? 0
      : e.key === "End" ? all.length - 1
      : undefined;
    if (to !== undefined) {
      e.preventDefault();
      all.forEach((el, i) => (el.tabIndex = i === to ? 0 : -1));
      all[to].focus();
      return;
    }
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      void choose(at);
    }
  }

  /** Wire the row's events on `into`, which holds it. */
  function listen(into: HTMLElement) {
    into.addEventListener("keydown", keys);
    into.addEventListener("click", (e) => {
      const on = option(e.target);
      if (on) void choose(Number(on.dataset.background));
    });
    into.addEventListener("pointerover", (e) => {
      const on = option(e.target);
      if (on) preview(Number(on.dataset.background));
    });
    into.addEventListener("pointerout", (e) => {
      if (option(e.target) && away(e.relatedTarget) && previewing !== undefined) preview(undefined);
    });
    into.addEventListener("focusin", (e) => {
      const on = option(e.target);
      if (on) preview(Number(on.dataset.background));
    });
    into.addEventListener("focusout", (e) => {
      if (option(e.target) && away(e.relatedTarget) && previewing !== undefined) preview(undefined);
    });
    still.addEventListener("change", tick);
    document.addEventListener("visibilitychange", tick);
  }

  return {
    attach,
    listen,
    /** Every state is linted, edits having paused: the backgrounds are drawn again where they
     * show. */
    linted: () => {
      drawn = undefined;
      if (box?.isConnected && !box.closest("[hidden]")) void ask();
    },
    /** What is offered for the slide shown, for a test, once it is. */
    offered: () => (drawn?.state === around.shown()?.state ? drawn?.backgrounds.options.map((o) => ({ ...o, pixels: undefined })) : undefined),
    held: () => (drawn?.state === around.shown()?.state ? drawn?.backgrounds.held : undefined),
    /** The swatches' clock, seconds: what a test reads to see them move. */
    clock: () => t,
    /** The preview asked for is shown: what a test waits for. */
    previewed: async () => {
      await previews;
      return previewed;
    },
  };
}
