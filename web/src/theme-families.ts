// The themes that ship by family (PLAN T.0d, ADR-0031 decision 4): each family a section of a
// chooser, in the order docs/themes/registry.json first names it, each theme under its own.
import { themes } from "@scaena/themes";

/** Each family's section of a `<select>`, its themes each an option of the value `value` gives. */
export function familySections(value: (name: string) => string = (name) => name): HTMLOptGroupElement[] {
  const sections = new Map<string, HTMLOptGroupElement>();
  for (const [name, { family }] of Object.entries(themes)) {
    let section = sections.get(family);
    if (!section) sections.set(family, (section = Object.assign(document.createElement("optgroup"), { label: family })));
    section.append(new Option(name, value(name)));
  }
  return [...sections.values()];
}

