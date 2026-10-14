// Make a picture (PLAN 2.107): a picture of what a person says it shows, made by the best image
// model their own OpenAI or Gemini key lists, in the theme's palette as a made deck's pictures are
// (PLAN G.10), for the image selected, the empty place for a picture on the slide shown, or where
// they last pressed, framed as that place is. The worker asks for it with the generator's module,
// loaded the first time; the picture joins the bundle, named by its SHA-256, and goes where it was
// asked for, saying what it shows: one patch, one step to undo. With no key kept for either,
// nothing is asked: the dialog says where one goes. Cancel drops an answer still coming.
import { load, settings } from "./assistant/keys";
import type { MadePicture, PictureTarget } from "./canvas";
import type { Stage } from "./stage";

/** What the dialog asks of the editor around it. */
export interface PicturingHost {
  /** Where the picture goes, and the place it is framed for; or why there is none yet. */
  target(): Promise<PictureTarget | string>;
  /** The picture made, put where it was asked for: one patch, one step to undo. */
  put(target: PictureTarget, made: MadePicture): Promise<void>;
  say(text: string): void;
  /** Show the assistant on `provider`, its key in focus: where a key for it goes. */
  keys(provider: "openai" | "gemini"): void;
}

type Painter = "openai" | "gemini";
const NAMES: Record<Painter, string> = { openai: "OpenAI", gemini: "Gemini" };

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
const $ = <T extends HTMLElement>(selector: string) => document.querySelector<T>(selector)!;

/** How a place `width` × `height` frames a picture, as the generator frames it (`Frame::of`). */
const framed = (width: number, height: number) => {
  const ratio = height > 0 ? width / height : 1;
  return ratio >= 1.2 ? "wide" : ratio <= 0.84 ? "tall" : "square";
};

export function picturer(stage: Stage, host: PicturingHost) {
  const dialog = $<HTMLDialogElement>("#picturing");
  const form = dialog.querySelector("form")!;
  const subject = $<HTMLTextAreaElement>("#picturing-subject");
  const look = $<HTMLSelectElement>("#picturing-look");
  const by = $<HTMLSelectElement>("#picturing-by");
  const where = $("#picturing-where");
  const who = $("#picturing-who");
  const status = $("#picturing-status");
  const key = $<HTMLButtonElement>("#picturing-key");
  const make = $<HTMLButtonElement>("#picturing-make");
  /** The picture being made: a later one, or the dialog closed, drops an earlier answer. */
  let turn = 0;
  let target: PictureTarget | undefined;
  /** The keys kept for the image providers, by provider, at their addresses. */
  let painters: Partial<Record<Painter, { key: string; base?: string }>> = {};
  /** The last picture asked for, settled: what a test waits for. */
  let settled: Promise<void> = Promise.resolve();

  const busy = (on: boolean) => {
    for (const field of [subject, look, by, make]) field.disabled = on;
  };

  /** Who makes it, as chosen: the provider, with the key kept for it; none where none is kept,
   * which the dialog says, with where one goes. */
  const whoPaints = () => {
    const chosen = by.value as Painter;
    const kept = !!painters[chosen];
    key.hidden = kept;
    make.disabled = !kept;
    who.textContent = kept
      ? `Made by ${NAMES[chosen]}'s best image model your key lists: what you ask goes to ${NAMES[chosen]} and nowhere else.`
      : `No ${NAMES[chosen]} key is kept: keep one in the Assistant tab${painters[chosen === "openai" ? "gemini" : "openai"] ? `, or choose ${NAMES[chosen === "openai" ? "gemini" : "openai"]}` : ""}.`;
  };
  by.onchange = whoPaints;

  /** Open the dialog for where a picture goes now. */
  async function open() {
    const found = await host.target();
    if (typeof found === "string") return host.say(found);
    target = found;
    ++turn;
    busy(false);
    status.textContent = "";
    const shape = framed(found.width, found.height);
    where.textContent =
      found.node !== undefined
        ? `It takes the place of the picture selected, ${shape} as its place is.`
        : found.slot !== undefined
          ? `It fills the empty place for a picture on this slide, ${shape} as that place is.`
          : `It goes on this slide where you last clicked, ${shape}.`;
    if (found.shows) subject.value = found.shows;
    const s = settings();
    painters = {};
    for (const p of ["openai", "gemini"] as const) {
      const k = await load(p).catch(() => undefined);
      if (k?.key) painters[p] = { key: k.key, base: s.bases[p] };
    }
    // A provider whose key is kept: the one chosen last, else OpenAI's first.
    if (!painters[by.value as Painter]) by.value = painters.openai ? "openai" : painters.gemini ? "gemini" : by.value;
    whoPaints();
    if (!dialog.open) dialog.showModal();
    (make.disabled ? key : subject).focus();
  }

  /** Make the picture asked for, and put it where it goes. */
  async function making() {
    const mine = ++turn;
    const painter = by.value as Painter;
    const kept = painters[painter];
    const place = target;
    const shows = subject.value.trim();
    if (!kept || !place || !shows) return;
    busy(true);
    status.textContent = `Making the picture with ${NAMES[painter]}… (up to a minute)`;
    let made: MadePicture;
    try {
      made = await stage.makePicture({
        subject: shows,
        look: look.value as "illustration" | "photo",
        provider: painter,
        key: kept.key,
        base: kept.base,
        width: place.width,
        height: place.height,
      });
    } catch (e) {
      if (mine !== turn) return;
      busy(false);
      whoPaints();
      status.textContent = `No picture: ${said(e)}`;
      return;
    }
    if (mine !== turn || !dialog.open) return;
    dialog.close();
    await host.put(place, made);
  }

  form.addEventListener("submit", (e) => {
    e.preventDefault();
    settled = making();
  });
  $<HTMLButtonElement>("#picturing-cancel").onclick = () => dialog.close();
  key.onclick = () => {
    dialog.close();
    host.keys(by.value as Painter);
  };
  // Closed, by Cancel or Escape: a picture still coming is dropped.
  dialog.addEventListener("close", () => {
    ++turn;
    busy(false);
  });

  return {
    open: () => {
      settled = open();
      return settled;
    },
    /** The last picture asked for, made and put, or failed: what a test waits for. */
    settled: () => settled,
  };
}
