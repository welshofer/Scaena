// `icon_find` as the page answers it (PLAN 2.104): the icons found in the set the page carries, by
// the rule `scaena_ops::icons` finds them by, returned as the MCP server returns them. Nothing here
// fetches or touches a page, so `web/exchanges.mjs` holds it in node (lift-up 20261025c SEC-3).
import { findIcons, type IconSet } from "../icon-rank.ts";

/** The most icons `icon_find` returns at once, as `scaena_ops::icons::MOST` holds the MCP
 * server's: a call is a bounded read, whatever `limit` asks. */
export const MOST_ICONS = 50;

/** `icon_find`'s answer from `set` for `args`, and whether it is an error: 50 icons at most, each
 * one's path only where `paths` asks, so a first search reads names, tags, and categories alone. */
export function iconsFound(set: IconSet, args: unknown): [string, boolean] {
  const a = (args ?? {}) as { query?: unknown; weight?: unknown; limit?: unknown; paths?: unknown };
  const known = Object.keys(a).every((k) => k === "query" || k === "weight" || k === "limit" || k === "paths");
  const { query = "", weight = "regular", limit, paths = false } = a;
  const whole = limit === undefined || limit === null || (typeof limit === "number" && Number.isInteger(limit) && limit >= 0);
  if (!known || typeof query !== "string" || (weight !== "regular" && weight !== "fill") || !whole || typeof paths !== "boolean") {
    const message = "icon_find takes `query`, words; `weight`, `regular` or `fill`; `limit`, a whole number; and `paths`, true or false";
    return [JSON.stringify({ message }), true];
  }
  const found = findIcons(set, query);
  const most = Math.min(typeof limit === "number" ? limit : 20, MOST_ICONS);
  const icons = found.slice(0, most).map((e) => ({
    name: e.name,
    tags: e.tags,
    categories: e.categories,
    ...(paths ? { icon: { name: e.name, path: weight === "fill" ? e.fill : e.regular, view: set.view } } : {}),
  }));
  return [JSON.stringify({ icons, found: found.length }), false];
}
