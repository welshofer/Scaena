// The icons that ship (PLAN 2.104): Phosphor 2.1.1's, MIT, every icon but brands, in its regular
// and fill weights, each path on a view 256 square (`docs/icons/phosphor.json`, which
// `scripts/build_icons.py` writes). The build carries the set beside the pages, fetched the first
// time the picker opens or a deck made from a prompt names an icon (PLAN G.23). Icons are found
// by the rule `scaena_ops::icons` finds them by. Nothing here touches a page: the editor's picker
// (`icons.ts`) and the generator's worker (`generate.ts`) both read it.
import phosphor from "../../docs/icons/phosphor.json?url";

import type { IconSet } from "./icon-rank";
export type { IconEntry, IconSet } from "./icon-rank";
export { findIcons, rank } from "./icon-rank";

export type Weight = "regular" | "fill";

/** An icon to insert, as `Player.insertingIcon` takes it. */
export interface Icon {
  name: string;
  path: string;
  view: [number, number, number, number];
}

let loading: Promise<IconSet> | undefined;

/** The set, fetched once. */
export function iconSet(): Promise<IconSet> {
  loading ??= fetch(phosphor).then((r) => {
    if (!r.ok) throw new Error(`the icons did not load (${r.status})`);
    return r.json() as Promise<IconSet>;
  });
  loading.catch(() => (loading = undefined));
  return loading;
}

/** An icon's name as a person reads it: `airplane-tilt` as Airplane tilt. */
export const iconSaid = (name: string) => {
  const said = name.replace(/-/g, " ");
  return said.charAt(0).toUpperCase() + said.slice(1);
};
