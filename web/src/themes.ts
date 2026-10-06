// What the editor starts a new deck from (PLAN 2.12): the three themes that ship, which share
// one vocabulary on one grid (SPEC §3.6), and the fonts they name. The build copies each file
// beside the pages, and the worker fetches them only to make a deck. A single-file export's
// worker is built with `no-themes.ts`: its page plays a bundle, and never makes one.
import daybreak from "../../docs/examples/authorability/themes/daybreak.theme.json?url";
import frauncesItalic from "../../docs/examples/fonts/Fraunces-Italic-VF.ttf?url";
import fraunces from "../../docs/examples/fonts/Fraunces-VF.ttf?url";
import interItalic from "../../docs/examples/fonts/Inter-Italic-VF.ttf?url";
import inter from "../../docs/examples/fonts/Inter-VF.ttf?url";
import monoItalic from "../../docs/examples/fonts/JetBrainsMono-Italic-VF.ttf?url";
import mono from "../../docs/examples/fonts/JetBrainsMono-VF.ttf?url";
import dusk from "../../docs/examples/themes/dusk.theme.json?url";
import ember from "../../docs/examples/themes/ember.theme.json?url";

/** Each theme by its name: its file's name in a bundle (`themes/…`), and where the page has it. */
export const themes: Record<string, { file: string; url: string }> = {
  Dusk: { file: "dusk.theme.json", url: dusk },
  Daybreak: { file: "daybreak.theme.json", url: daybreak },
  Ember: { file: "ember.theme.json", url: ember },
};

/** The fonts the themes name, by the paths they give them, and where the page has each: each
 * family's, and its italic's (PLAN 2.40). */
export const fonts: Record<string, string> = {
  "fonts/Fraunces-VF.ttf": fraunces,
  "fonts/Inter-VF.ttf": inter,
  "fonts/JetBrainsMono-VF.ttf": mono,
  "fonts/Fraunces-Italic-VF.ttf": frauncesItalic,
  "fonts/Inter-Italic-VF.ttf": interItalic,
  "fonts/JetBrainsMono-Italic-VF.ttf": monoItalic,
};
