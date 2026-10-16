//! The themes that ship (SPEC §3.6), with their fonts, carried in the binary: what a deck starts
//! from where no theme file is at hand (PLAN 2.13). The registry, `docs/themes/registry.json`,
//! lists them, each written from its spec (PLAN T.0a, ADR-0031). Dusk, Daybreak, Ember, and
//! Salon share one vocabulary on one grid, and the same body and mono fonts, each with its
//! italic: Inter and JetBrains Mono. The first three set display in Fraunces, and Salon in
//! Instrument Serif (PLAN 2.102). Each is found by its name, one word, in any case; Salon by
//! `gallery` too, the name it shipped under, so a deck that names Gallery opens as before (PLAN
//! T.0d). Each font carries its copyright and its license (the SIL Open Font License) in its own
//! name table.

/// A theme that ships.
#[derive(Debug, Clone, Copy)]
pub struct Shipped {
    /// Its name, as `create` takes it: `dusk`, `daybreak`, `ember`, or `salon`.
    pub name: &'static str,
    /// Its family, the section a chooser offers it under (PLAN T.0d): `Stage`, `Editorial`, ….
    pub family: &'static str,
    /// Its file's name in a bundle's `themes/`.
    pub file: &'static str,
    pub text: &'static str,
}

mod table;

use crate::theme::ALIASES;
use table::FONTS;
/// Every theme that ships, as `docs/themes/registry.json` lists them (PLAN T.0a, ADR-0031):
/// `scripts/build_themes.py` writes the table from it.
pub use table::THEMES;

/// The theme that ships as `name`, in any case, or by a name it shipped under before (PLAN T.0d):
/// `gallery` is Salon.
pub fn theme(name: &str) -> Option<Shipped> {
    let name = ALIASES.iter().find(|(alias, _)| alias.eq_ignore_ascii_case(name)).map_or(name, |(_, is)| *is);
    THEMES.into_iter().find(|t| t.name.eq_ignore_ascii_case(name))
}

/// Every font the themes that ship name, each by the path it gives it.
pub fn fonts() -> impl Iterator<Item = (&'static str, &'static [u8])> {
    FONTS.into_iter()
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
    use crate::theme::same;

    /// The themes that ship are the registry's, in its order (PLAN T.0a).
    #[test]
    fn the_themes_that_ship_are_the_registrys() {
        let registry: serde_json::Value =
            serde_json::from_str(include_str!("../../../docs/themes/registry.json")).unwrap();
        let slugs: Vec<&str> =
            registry["themes"].as_array().unwrap().iter().map(|t| t["slug"].as_str().unwrap()).collect();
        assert_eq!(THEMES.iter().map(|t| t.name).collect::<Vec<_>>(), slugs);
    }

    /// Each name is one word, its slug, and Salon answers to the name it shipped under (PLAN T.0d).
    #[test]
    fn each_theme_is_found_by_its_name_and_salon_by_gallery() {
        for t in THEMES {
            assert!(t.name.chars().all(|c| c.is_ascii_lowercase()), "{}", t.name);
            let named =
                serde_json::from_str::<serde_json::Value>(t.text).unwrap()["name"].as_str().unwrap().to_string();
            assert!(named.eq_ignore_ascii_case(t.name), "{}: {named}", t.name);
            assert_eq!(theme(&named).map(|s| s.name), Some(t.name));
        }
        for name in ["gallery", "Gallery", "GALLERY", "salon"] {
            assert_eq!(theme(name).map(|t| t.file), Some("salon.theme.json"), "{name}");
        }
        assert!(
            same("Gallery", "Salon") && same("Salon", "Salon") && !same("Gallery", "Dusk") && !same("Dusk", "Salon")
        );
    }

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

    /// The themes that ship offer the same builds, a presentation app's (PLAN 2.112), and the same
    /// springs they spring on, so a deck moves between them with its cue as it is.
    #[test]
    fn every_theme_that_ships_offers_the_same_builds() {
        let motion = |t: &Shipped| {
            let parsed: serde_json::Value = serde_json::from_str(t.text).unwrap();
            (parsed["motion"]["presets"].clone(), parsed["motion"]["springs"].clone())
        };
        let first = motion(&THEMES[0]);
        for theme in &THEMES[1..] {
            assert_eq!(motion(theme), first, "{}", theme.name);
        }
        let builds = first.0.as_object().unwrap();
        assert_eq!(builds.len(), 25);
        for name in
            ["appear", "bullets", "typewriter", "pop", "drop", "moveInLeft", "spin", "swing", "flip", "blink", "jiggle"]
        {
            assert!(builds.contains_key(name), "{name}");
        }
        assert!(first.1.get("bouncy").is_some(), "the spring Pop, Drop, Skid, Swing, and Jiggle spring on");
    }

    /// Every theme that ships casts the same shadows (PLAN 2.115), by the same names: a deck keeps
    /// its shadows through a re-theme.
    #[test]
    fn every_theme_that_ships_casts_the_same_shadows() {
        let shadows = |t: &Shipped| {
            let parsed: serde_json::Value = serde_json::from_str(t.text).unwrap();
            parsed["tokens"]["shadow"].clone()
        };
        let first = shadows(&THEMES[0]);
        let names: Vec<&String> = first.as_object().unwrap().keys().collect();
        assert_eq!(names, ["soft", "raised", "floating"]);
        for theme in &THEMES[1..] {
            assert_eq!(shadows(theme), first, "{}", theme.name);
        }
    }

    /// Every theme that ships sets verse (PLAN O.4b): upright, `wrap: none`, in a color that reads
    /// at 4.5:1 or more on each of its surfaces.
    #[test]
    fn every_theme_that_ships_sets_verse_that_reads() {
        fn luminance(c: scaena_core::displaylist::Color) -> f64 {
            let lin = |v: u8| {
                let v = f64::from(v) / 255.0;
                if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * lin(c.0[0]) + 0.7152 * lin(c.0[1]) + 0.0722 * lin(c.0[2])
        }
        for shipped in THEMES {
            let theme = scaena_engine::theme::Theme::from_json(shipped.text).unwrap();
            let verse = theme.typography.roles.get("verse").expect(shipped.name);
            assert_eq!(verse.wrap, Some(scaena_core::model::nodes::Wrap::None), "{}", shipped.name);
            assert_ne!(verse.italic, Some(true), "{}", shipped.name);
            let ink = luminance(theme.color(verse.color.as_deref().unwrap_or("onSurface")).unwrap());
            for surface in ["surface", "surface-2"] {
                let paper = luminance(theme.color(surface).unwrap());
                let ratio = (ink.max(paper) + 0.05) / (ink.min(paper) + 0.05);
                assert!(ratio >= 4.5, "{} verse on {surface}: {ratio:.2}:1", shipped.name);
            }
        }
    }
}
