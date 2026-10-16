//! The themes that ship, and the fonts they name, as `docs/themes/registry.json` lists them.
//! Written by `scripts/build_themes.py` from the registry; do not edit by hand.

use super::Shipped;

/// Every theme that ships, in the registry's order.
pub const THEMES: [Shipped; 4] = [
    Shipped {
        name: "dusk",
        file: "dusk.theme.json",
        text: include_str!("../../../../docs/examples/themes/dusk.theme.json"),
        font_files: &[
            "fonts/Fraunces-VF.ttf",
            "fonts/Inter-VF.ttf",
            "fonts/JetBrainsMono-VF.ttf",
            "fonts/Fraunces-Italic-VF.ttf",
            "fonts/Inter-Italic-VF.ttf",
            "fonts/JetBrainsMono-Italic-VF.ttf",
        ],
    },
    Shipped {
        name: "daybreak",
        file: "daybreak.theme.json",
        text: include_str!("../../../../docs/examples/authorability/themes/daybreak.theme.json"),
        font_files: &[
            "fonts/Fraunces-VF.ttf",
            "fonts/Inter-VF.ttf",
            "fonts/JetBrainsMono-VF.ttf",
            "fonts/Fraunces-Italic-VF.ttf",
            "fonts/Inter-Italic-VF.ttf",
            "fonts/JetBrainsMono-Italic-VF.ttf",
        ],
    },
    Shipped {
        name: "ember",
        file: "ember.theme.json",
        text: include_str!("../../../../docs/examples/themes/ember.theme.json"),
        font_files: &[
            "fonts/Fraunces-VF.ttf",
            "fonts/Inter-VF.ttf",
            "fonts/JetBrainsMono-VF.ttf",
            "fonts/Fraunces-Italic-VF.ttf",
            "fonts/Inter-Italic-VF.ttf",
            "fonts/JetBrainsMono-Italic-VF.ttf",
        ],
    },
    Shipped {
        name: "gallery",
        file: "gallery.theme.json",
        text: include_str!("../../../../docs/examples/themes/gallery.theme.json"),
        font_files: &[
            "fonts/InstrumentSerif-Regular.ttf",
            "fonts/Inter-VF.ttf",
            "fonts/JetBrainsMono-VF.ttf",
            "fonts/InstrumentSerif-Italic.ttf",
            "fonts/Inter-Italic-VF.ttf",
            "fonts/JetBrainsMono-Italic-VF.ttf",
        ],
    },
];
