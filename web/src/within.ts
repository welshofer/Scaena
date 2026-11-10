/** Where a bundle's file at `path` is, given its deck at `deck`, or why it may not be fetched
 * (lift-up 20261031 SEC-1): a bundle names only its own files, as `scaena_store`'s `inside`
 * holds natively, so a path that resolves to another origin (`https://…`, `//host/…`) or out of
 * the deck's folder (`../…`) is refused, naming it, before anything is asked of the network.
 * The deck's own address is under its folder, so it passes too. */
export function within(path: string, deck: string): URL {
  const base = new URL(deck);
  const folder = new URL(".", base);
  let url: URL;
  try {
    url = new URL(path, base);
  } catch {
    throw new Error(`${path}: not a path in the bundle`);
  }
  if (url.origin !== base.origin) throw new Error(`${path}: a bundle's file is under its folder, not at ${url.origin === "null" ? url.protocol : url.origin}`);
  if (!url.pathname.startsWith(folder.pathname)) throw new Error(`${path}: a bundle's file is under its folder, not ${url.pathname}`);
  return url;
}
