//! The themes that ship, and the fonts they name, as `docs/themes/registry.json` lists them.
//! Written by `scripts/build_themes.py` from the registry; do not edit by hand.

use super::Shipped;

/// Every theme that ships, in the registry's order.
pub const THEMES: [Shipped; 4] = [
    Shipped {
        name: "dusk",
        file: "dusk.theme.json",
        text: include_str!("../../../../docs/examples/themes/dusk.theme.json"),
    },
    Shipped {
        name: "daybreak",
        file: "daybreak.theme.json",
        text: include_str!("../../../../docs/examples/authorability/themes/daybreak.theme.json"),
    },
    Shipped {
        name: "ember",
        file: "ember.theme.json",
        text: include_str!("../../../../docs/examples/themes/ember.theme.json"),
    },
    Shipped {
        name: "gallery",
        file: "gallery.theme.json",
        text: include_str!("../../../../docs/examples/themes/gallery.theme.json"),
    },
];

/// The fonts the themes name, by the paths they give them: each family's, and its italic's
/// (PLAN 2.40).
pub(super) const FONTS: [(&str, &[u8]); 8] = [
    ("fonts/Fraunces-VF.ttf", include_bytes!("../../../../docs/examples/fonts/Fraunces-VF.ttf")),
    ("fonts/Inter-VF.ttf", include_bytes!("../../../../docs/examples/fonts/Inter-VF.ttf")),
    ("fonts/JetBrainsMono-VF.ttf", include_bytes!("../../../../docs/examples/fonts/JetBrainsMono-VF.ttf")),
    ("fonts/Fraunces-Italic-VF.ttf", include_bytes!("../../../../docs/examples/fonts/Fraunces-Italic-VF.ttf")),
    ("fonts/Inter-Italic-VF.ttf", include_bytes!("../../../../docs/examples/fonts/Inter-Italic-VF.ttf")),
    ("fonts/JetBrainsMono-Italic-VF.ttf", include_bytes!("../../../../docs/examples/fonts/JetBrainsMono-Italic-VF.ttf")),
    ("fonts/InstrumentSerif-Regular.ttf", include_bytes!("../../../../docs/examples/fonts/InstrumentSerif-Regular.ttf")),
    ("fonts/InstrumentSerif-Italic.ttf", include_bytes!("../../../../docs/examples/fonts/InstrumentSerif-Italic.ttf")),
];
