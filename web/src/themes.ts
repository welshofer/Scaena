// What the editor starts a new deck from (PLAN 2.12): the themes that ship, which share one
// vocabulary on one grid (SPEC §3.6), and the fonts they name, as docs/themes/registry.json
// lists them. The build copies each file beside the pages, and the worker fetches them to make a
// deck, and a theme to bring its layout library into a bundle's older copy of it (PLAN 3.31). A
// single-file export's worker is built with `no-themes.ts`: its page plays a bundle, and never
// makes one. Each theme's thumbnail is its title slide drawn at build by the CPU painter, and a
// font git does not hold comes from the sha256 cache (PLAN T.0c). Written by
// scripts/build_themes.py from the registry; do not edit by hand.
import dusk from "../../docs/examples/themes/dusk.theme.json?url";
import daybreak from "../../docs/examples/authorability/themes/daybreak.theme.json?url";
import ember from "../../docs/examples/themes/ember.theme.json?url";
import gallery from "../../docs/examples/themes/gallery.theme.json?url";
import duskThumbnail from "../../target/themes/dusk.png?url";
import daybreakThumbnail from "../../target/themes/daybreak.png?url";
import emberThumbnail from "../../target/themes/ember.png?url";
import galleryThumbnail from "../../target/themes/gallery.png?url";
import frauncesVFFont from "../../docs/examples/fonts/Fraunces-VF.ttf?url";
import interVFFont from "../../docs/examples/fonts/Inter-VF.ttf?url";
import jetBrainsMonoVFFont from "../../docs/examples/fonts/JetBrainsMono-VF.ttf?url";
import frauncesItalicVFFont from "../../docs/examples/fonts/Fraunces-Italic-VF.ttf?url";
import interItalicVFFont from "../../docs/examples/fonts/Inter-Italic-VF.ttf?url";
import jetBrainsMonoItalicVFFont from "../../docs/examples/fonts/JetBrainsMono-Italic-VF.ttf?url";
import instrumentSerifRegularFont from "../../docs/examples/fonts/InstrumentSerif-Regular.ttf?url";
import instrumentSerifItalicFont from "../../docs/examples/fonts/InstrumentSerif-Italic.ttf?url";

/** Each theme by its name: its file's name in a bundle (`themes/…`), where the page has it, and
 * where it has its thumbnail, its title slide drawn small. */
export const themes: Record<string, { file: string; url: string; thumbnail: string }> = {
  Dusk: { file: "dusk.theme.json", url: dusk, thumbnail: duskThumbnail },
  Daybreak: { file: "daybreak.theme.json", url: daybreak, thumbnail: daybreakThumbnail },
  Ember: { file: "ember.theme.json", url: ember, thumbnail: emberThumbnail },
  Gallery: { file: "gallery.theme.json", url: gallery, thumbnail: galleryThumbnail },
};

/** The fonts the themes name, by the paths they give them, and where the page has each: each
 * family's, and its italic's (PLAN 2.40). */
export const fonts: Record<string, string> = {
  "fonts/Fraunces-VF.ttf": frauncesVFFont,
  "fonts/Inter-VF.ttf": interVFFont,
  "fonts/JetBrainsMono-VF.ttf": jetBrainsMonoVFFont,
  "fonts/Fraunces-Italic-VF.ttf": frauncesItalicVFFont,
  "fonts/Inter-Italic-VF.ttf": interItalicVFFont,
  "fonts/JetBrainsMono-Italic-VF.ttf": jetBrainsMonoItalicVFFont,
  "fonts/InstrumentSerif-Regular.ttf": instrumentSerifRegularFont,
  "fonts/InstrumentSerif-Italic.ttf": instrumentSerifItalicFont,
};
