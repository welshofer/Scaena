//! The themes that ship (SPEC §3.6), with their fonts, carried in the binary: what a deck starts
//! from where no theme file is at hand (PLAN 2.13). Dusk, Daybreak, and Ember share one
//! vocabulary on one grid, and the same three fonts, each with its italic. Each font carries
//! its copyright and its license (the SIL Open Font License) in its own name table.

/// A theme that ships.
#[derive(Debug, Clone, Copy)]
pub struct Shipped {
    /// Its name, as `create` takes it: `dusk`, `daybreak`, or `ember`.
    pub name: &'static str,
    /// Its file's name in a bundle's `themes/`.
    pub file: &'static str,
    pub text: &'static str,
}

/// Every theme that ships.
pub const THEMES: [Shipped; 3] = [
    Shipped {
        name: "dusk",
        file: "dusk.theme.json",
        text: include_str!("../../../docs/examples/themes/dusk.theme.json"),
    },
    Shipped {
        name: "daybreak",
        file: "daybreak.theme.json",
        text: include_str!("../../../docs/examples/authorability/themes/daybreak.theme.json"),
    },
    Shipped {
        name: "ember",
        file: "ember.theme.json",
        text: include_str!("../../../docs/examples/themes/ember.theme.json"),
    },
];

/// The fonts the themes name, by the paths they give them: each family's, and its italic's
/// (PLAN 2.40).
const FONTS: [(&str, &[u8]); 6] = [
    ("fonts/Fraunces-VF.ttf", include_bytes!("../../../docs/examples/fonts/Fraunces-VF.ttf")),
    ("fonts/Inter-VF.ttf", include_bytes!("../../../docs/examples/fonts/Inter-VF.ttf")),
    ("fonts/JetBrainsMono-VF.ttf", include_bytes!("../../../docs/examples/fonts/JetBrainsMono-VF.ttf")),
    ("fonts/Fraunces-Italic-VF.ttf", include_bytes!("../../../docs/examples/fonts/Fraunces-Italic-VF.ttf")),
    ("fonts/Inter-Italic-VF.ttf", include_bytes!("../../../docs/examples/fonts/Inter-Italic-VF.ttf")),
    ("fonts/JetBrainsMono-Italic-VF.ttf", include_bytes!("../../../docs/examples/fonts/JetBrainsMono-Italic-VF.ttf")),
];

/// The theme that ships as `name`, in any case.
pub fn theme(name: &str) -> Option<Shipped> {
    THEMES.into_iter().find(|t| t.name.eq_ignore_ascii_case(name))
}

/// A font a theme that ships names, by the path it gives it.
pub fn font(path: &str) -> Option<&'static [u8]> {
    FONTS.iter().find(|(p, _)| *p == path).map(|(_, bytes)| *bytes)
}

/// The names of the themes that ship, as a sentence lists them.
pub fn names() -> String {
    let names: Vec<&str> = THEMES.iter().map(|t| t.name).collect();
    names.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_font_a_theme_that_ships_names_ships_with_it() {
        for theme in THEMES {
            let parsed: serde_json::Value = serde_json::from_str(theme.text).unwrap();
            let families = parsed["type"]["families"].as_object().unwrap();
            for family in families.values() {
                let file = family["file"].as_str().unwrap();
                assert!(font(file).is_some(), "{}: {file}", theme.name);
                let italic = family["italic"]["file"].as_str().expect("every family that ships has an italic");
                assert!(font(italic).is_some(), "{}: {italic}", theme.name);
            }
        }
        assert_eq!(theme("Ember").map(|t| t.file), Some("ember.theme.json"));
        assert!(theme("ember.theme.json").is_none());
    }
}
