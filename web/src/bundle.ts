// Where a page finds the bundle it opens (SPEC §3.1).

/** A bundle's deck file: the bundle's `deck.json`, or a deck file named outright, whose
 * directory is then the bundle. */
export function deckFile(bundle: URL): string {
  if (bundle.pathname.endsWith(".json")) return bundle.href;
  const dir = new URL(bundle);
  if (!dir.pathname.endsWith("/")) dir.pathname += "/";
  return new URL("deck.json", dir).href;
}
