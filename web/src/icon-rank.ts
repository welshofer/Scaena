// The icons that ship, found by the rule `scaena_ops::icons` finds them by (PLAN 2.104): the set's
// shape and its ranking, with no fetch and no page, which `icon-set.ts` carries on and node reads
// in `web/exchanges.mjs` (lift-up 20261025c SEC-3).

/** One icon of the set: its name, tags, and categories, and its path in each weight. */
export interface IconEntry {
  name: string;
  tags: string[];
  categories: string[];
  regular: string;
  fill: string;
}

export interface IconSet {
  view: [number, number, number, number];
  icons: IconEntry[];
}

/** `text`'s words, lowercase: what is not a letter or a digit parts them. */
const words = (text: string) =>
  text
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean)
    .map((w) => w.toLowerCase());

/** How well `icon` answers `asked`, as `scaena_ops::icons::rank` says, best 0; undefined where a
 * word is in none of its words. */
export function rank(asked: string[], icon: IconEntry): number | undefined {
  if (asked.length === 0 || asked.join("-") === icon.name) return 0;
  const all = (pool: string[], hit: (n: string, w: string) => boolean) => asked.every((w) => pool.some((n) => hit(n, w)));
  const every = words(icon.name);
  if (all(every, (n, w) => n === w)) return 1;
  if (all(every, (n, w) => n.startsWith(w))) return 2;
  // Inside a name, a word of five letters or more: `plane` in `airplane`, not `team` in `steam`.
  if (all(every, (n, w) => [...w].length >= 5 && n.includes(w))) return 3;
  const more = [...every, ...icon.tags.flatMap(words), ...icon.categories.flatMap(words)];
  if (all(more, (n, w) => n === w)) return 4;
  if (all(more, (n, w) => n.startsWith(w))) return 5;
  return undefined;
}

/** The icons `query` finds in `set`, best first: every icon, by name, for no words. */
export function findIcons(set: IconSet, query: string): IconEntry[] {
  const asked = words(query);
  const found = set.icons.flatMap((icon) => {
    const r = rank(asked, icon);
    return r === undefined ? [] : [{ r, icon }];
  });
  const long = (i: IconEntry) => (asked.length ? i.name.length : 0);
  const byName = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
  found.sort((a, b) => a.r - b.r || long(a.icon) - long(b.icon) || byName(a.icon.name, b.icon.name));
  return found.map((f) => f.icon);
}

