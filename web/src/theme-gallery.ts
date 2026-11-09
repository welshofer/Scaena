// The themes that ship, chosen by their pictures (lift-up 10-25b ATTR-1), as the Mac's chooser
// and a presentation app's do: each theme's title slide, drawn at build by the CPU painter (PLAN
// T.0c), under its family (PLAN T.0d, ADR-0031 decision 4), in the order docs/themes/registry.json
// first names it. It is one radio group: Tab reaches the theme chosen, and the arrow keys move
// among them, as a radio group's do. What reads it reads `value`, as it read a `<select>`'s.
import { themes } from "@scaena/themes";

/** A gallery of the themes that ship in `host`, a radio group named `name`, the first chosen. */
export function themeGallery(host: HTMLElement, name: string) {
  host.setAttribute("role", "radiogroup");
  const sections = new Map<string, HTMLFieldSetElement>();
  const radios: HTMLInputElement[] = [];
  for (const [theme, { family, thumbnail }] of Object.entries(themes)) {
    let section = sections.get(family);
    if (!section) {
      section = document.createElement("fieldset");
      section.append(Object.assign(document.createElement("legend"), { textContent: family }));
      sections.set(family, section);
    }
    const radio = Object.assign(document.createElement("input"), { type: "radio", name, value: theme });
    const picture = Object.assign(document.createElement("img"), { src: thumbnail, alt: "", width: 240, height: 135 });
    const tile = document.createElement("label");
    tile.className = "tile";
    tile.append(radio, picture, Object.assign(document.createElement("span"), { textContent: theme }));
    section.append(tile);
    radios.push(radio);
  }
  host.replaceChildren(...sections.values());
  if (radios[0]) radios[0].checked = true;
  const gallery = {
    /** The theme chosen, by its name. */
    get value(): string {
      return radios.find((r) => r.checked)?.value ?? radios[0]?.value ?? "";
    },
    set value(theme: string) {
      const radio = radios.find((r) => r.value === theme);
      if (radio) radio.checked = true;
    },
    /** Called once another theme is chosen. */
    onchange: undefined as (() => void) | undefined,
  };
  host.addEventListener("change", () => gallery.onchange?.());
  return gallery;
}
