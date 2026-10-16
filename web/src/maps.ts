// Insert › Map… (ADR-0030, PLAN O.1c): a map of a place a person names, fetched once from
// Geoapify with their own key and kept in the bundle, so the deck draws it offline. It goes in the
// image selected, the empty place for a picture on the slide shown, or where they last pressed, at
// that place's size. The worker runs the plan from the generator's module and makes each GET by
// `fetch`; the picture joins the bundle and goes where it was asked for with its pin, its label,
// and the credit Geoapify's terms ask: one patch, one step to undo. Where is searched on Return,
// never as it is typed, and a name that matches several places offers up to five to pick from.
// Without a key, the sheet says where one goes and keeps one; offline, it says so and changes
// nothing.
import { load, store } from "./assistant/keys";
import { listen } from "./life";
import type { FetchedMap, MapAsk, MapPlace, MapTarget } from "./protocol";
import type { Stage } from "./stage";
import { ended } from "./words";

/** What the sheet asks of the editor around it. */
export interface MappingHost {
  /** Where the map goes on the slide shown; or why there is none yet. */
  target(): Promise<MapTarget | string>;
  /** The map fetched, put where it was asked for: one patch, one step to undo. */
  put(target: MapTarget, map: FetchedMap, label: string | undefined): Promise<void>;
  say(text: string): void;
}

export type Show = "block" | "neighborhood" | "city" | "region";

const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
const $ = <T extends HTMLElement>(selector: string) => document.querySelector<T>(selector)!;

export function mapper(stage: Stage, host: MappingHost) {
  const dialog = $<HTMLDialogElement>("#mapping");
  const form = dialog.querySelector("form")!;
  const where = $<HTMLInputElement>("#mapping-where");
  const show = $<HTMLSelectElement>("#mapping-show");
  const label = $<HTMLInputElement>("#mapping-label");
  const places = $<HTMLElement>("#mapping-places");
  const goes = $("#mapping-goes");
  const status = $("#mapping-status");
  const keyRow = $<HTMLElement>("#mapping-key-row");
  const keyField = $<HTMLInputElement>("#mapping-key");
  const keep = $<HTMLInputElement>("#mapping-keep");
  const fetchButton = $<HTMLButtonElement>("#mapping-fetch");
  /** The map being fetched: a later one, or the sheet closed, drops an earlier answer. */
  let turn = 0;
  let target: MapTarget | undefined;
  let key: string | undefined;
  /** The places the last name matched, to pick one of. */
  let found: MapPlace[] = [];
  let settled: Promise<void> = Promise.resolve();

  const busy = (on: boolean) => {
    for (const field of [where, show, label, fetchButton, keyField, keep]) field.disabled = on;
  };

  /** Offer the places a name matched, each a button that fetches the map of it. */
  function offer(matched: MapPlace[]) {
    found = matched.slice(0, 5);
    places.replaceChildren(
      ...found.map((p, n) => {
        const li = document.createElement("li");
        const button = document.createElement("button");
        button.type = "button";
        button.textContent = p.address;
        if (p.kind) button.title = p.kind;
        button.onclick = () => void (settled = fetching(n));
        li.append(button);
        return li;
      }),
    );
    places.hidden = !found.length;
    if (found.length) (places.querySelector("button") as HTMLButtonElement).focus();
  }

  async function open() {
    const found_ = await host.target();
    if (typeof found_ === "string") return host.say(found_);
    target = found_;
    ++turn;
    busy(false);
    offer([]);
    status.textContent = "";
    goes.textContent =
      found_.node !== undefined
        ? "It takes the place of the picture selected."
        : found_.slot !== undefined
          ? "It fills the empty place for a picture on this slide."
          : "It goes on this slide where you last clicked.";
    key = (await load("geoapify").catch(() => undefined))?.key;
    keyRow.hidden = !!key;
    if (!dialog.open) dialog.showModal();
    (key ? where : keyField).focus();
  }

  /** Fetch the map: of the place picked from those offered, `pick`, or of what Where says. */
  async function fetching(pick?: number) {
    const mine = ++turn;
    const place = target;
    const asked = where.value.trim();
    if (!place) return;
    if (!asked) return void (status.textContent = "Say where: an address, a place, or a latitude and longitude.");
    if (!key) {
      const typed = keyField.value.trim();
      if (!typed) return void (status.textContent = "Geoapify needs a key of your own, from geoapify.com: paste it above.");
      await store("geoapify", typed, keep.checked);
      key = typed;
      keyRow.hidden = true;
    }
    if (navigator.onLine === false) return void (status.textContent = "This browser is offline: nothing was fetched, and the slide is as it was.");
    busy(true);
    const [, , w, h] = place.rect;
    const ask: MapAsk = {
      place: asked,
      show: show.value as Show,
      style: place.style,
      size: [w, h],
      lang: place.lang,
      ...(pick !== undefined ? { pick } : {}),
    };
    status.textContent = pick === undefined && !/^\s*-?\d/.test(asked) ? "Finding the place…" : "Fetching the map…";
    try {
      const got = await stage.fetchMap(ask, key);
      if (mine !== turn || !dialog.open) return;
      busy(false);
      if (got.places) {
        status.textContent = `“${asked}” matches ${got.places.length} places: pick one.`;
        return offer(got.places);
      }
      if (!got.map) throw new Error("no map came back");
      dialog.close();
      const typed = label.value;
      await host.put(place, got.map, typed.trim() ? typed.trim() : undefined);
    } catch (e) {
      if (mine !== turn) return;
      busy(false);
      // A key Geoapify did not take is forgotten, so the next try asks for another.
      if (/Invalid apiKey|not taken|401|403/.test(said(e))) key = undefined;
      keyRow.hidden = !!key;
      status.textContent = `No map was fetched. ${ended(said(e))}`;
    }
  }

  listen(form, "submit", (e) => {
    e.preventDefault();
    settled = fetching();
  });
  // Where is searched on Return, never as it is typed: what was offered goes once it changes.
  listen(where, "input", () => offer([]));
  $<HTMLButtonElement>("#mapping-cancel").onclick = () => dialog.close();
  listen(dialog, "close", () => {
    ++turn;
    busy(false);
  });

  return {
    open: () => {
      settled = open();
      return settled;
    },
    /** The last map asked for, fetched and put, or failed: what a test waits for. */
    settled: () => settled,
  };
}
