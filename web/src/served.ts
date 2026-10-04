// A page `scaena serve` serves (PLAN 2.11, ADR-0012), with `?serve` in its address. The bundle is
// a folder on disk: the page hears of each change to it at `/scaena/events`, made on disk or by
// another page's save, and of a `deck.scn` there that does not compile.

/** One thing the compiler found in `deck.scn`, at its line. */
export interface Problem {
  code?: string;
  message: string;
  line?: number;
  col?: number;
  /** Where it lands in the deck, a JSON pointer. */
  path?: string;
  /** The file it is about, when that is not the deck (the theme). */
  file?: string;
  hint?: string;
}

/** Why `deck.scn` does not compile. */
export interface Failed {
  problems: Problem[];
}

/** Where the bundle on disk stands: its folder's name, whether it keeps its deck's source as
 * `deck.scn`, and why that does not compile, while it does not. */
export interface Status {
  version: number;
  name: string;
  source: boolean;
  failed: Failed | null;
}

/** Whether `scaena serve` serves this page. */
export const served = new URLSearchParams(location.search).has("serve");

/** This page's id, which its writes carry, so it does not hear its own saves as changes. */
export const client = served ? crypto.randomUUID() : "";

/** Where the bundle on disk stands now. */
export async function status(): Promise<Status> {
  const response = await fetch("/scaena/state", { cache: "no-store" });
  if (!response.ok) throw new Error(`scaena serve: ${response.status}`);
  return response.json() as Promise<Status>;
}

/** Hear the folder: `changed`, with the paths, for each change the disk or another page made;
 * `broke` when `deck.scn` changes on disk and does not compile; and `status`, why `deck.scn` does
 * not compile (`null` when it does), as the page starts to hear, and with each change. */
export function listen(on: {
  changed?: (paths: string[]) => void;
  broke?: (failed: Failed) => void;
  status?: (failed: Failed | null) => void;
}): EventSource {
  const events = new EventSource("/scaena/events");
  const data = <T>(e: Event) => JSON.parse((e as MessageEvent<string>).data) as T;
  events.addEventListener("hello", (e) => on.status?.(data<Status>(e).failed));
  events.addEventListener("failed", (e) => {
    const { failed } = data<{ failed: Failed }>(e);
    on.status?.(failed);
    on.broke?.(failed);
  });
  events.addEventListener("changed", (e) => {
    const changed = data<{ by: string | null; paths: string[]; failed: Failed | null }>(e);
    on.status?.(changed.failed);
    if (changed.by !== client) on.changed?.(changed.paths);
  });
  return events;
}

/** A problem as one line: where, and what. */
export function line(p: Problem): string {
  const where = p.line ? `:${p.line}:${p.col}` : "";
  return `${p.file ?? "deck.scn"}${where}: ${p.code ? `${p.code} ` : ""}${p.message}`;
}
