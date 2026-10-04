// Where a page finds the bundle it opens (SPEC §3.1, §9.2).
import type { Source } from "./protocol";

/** A bundle's deck file: the bundle's `deck.json`, or a deck file named outright, whose
 * directory is then the bundle. */
export function deckFile(bundle: URL): string {
  if (bundle.pathname.endsWith(".json")) return bundle.href;
  const dir = new URL(bundle);
  if (!dir.pathname.endsWith("/")) dir.pathname += "/";
  return new URL("deck.json", dir).href;
}

/** The bundle a page's `?bundle=` names: `opfs:NAME`, one the browser keeps (PLAN 2.4), or a
 * bundle's directory or deck file by URL, `fallback` without one. */
export function sourceOf(bundle: string | null, fallback: string): Source {
  if (bundle?.startsWith("opfs:")) return { opfs: bundle.slice("opfs:".length) };
  return { url: deckFile(new URL(bundle ?? fallback, location.href)) };
}
