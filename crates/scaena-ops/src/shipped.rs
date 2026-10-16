//! The themes that ship (SPEC §3.6), with their fonts, carried in the binary: what a deck starts
//! from where no theme file is at hand (PLAN 2.13). The registry, `docs/themes/registry.json`,
//! lists them, each written from its spec (PLAN T.0a, ADR-0031). Dusk, Daybreak, Ember, and
//! Gallery share one vocabulary on one grid, and the same body and mono fonts, each with its
//! italic: Inter and JetBrains Mono. The first three set display in Fraunces, and Gallery in
//! Instrument Serif (PLAN 2.102). Each font carries its copyright and its license (the SIL Open
//! Font License) in its own name table.

/// A theme that ships.
#[derive(Debug, Clone, Copy)]
pub struct Shipped {
    /// Its name, as `create` takes it: `dusk`, `daybreak`, `ember`, or `gallery`.
    pub name: &'static str,
    /// Its file's name in a bundle's `themes/`.
    pub file: &'static str,
    pub text: &'static str,
}

mod table;

use table::FONTS;
/// Every theme that ships, as `docs/themes/registry.json` lists them (PLAN T.0a, ADR-0031):
/// `scripts/build_themes.py` writes the table from it.
pub use table::THEMES;

/// The theme that ships as `name`, in any case.
pub fn theme(name: &str) -> Option<Shipped> {
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

    /// The themes that ship are the registry's, in its order (PLAN T.0a).
    #[test]
    fn the_themes_that_ship_are_the_registrys() {
        let registry: serde_json::Value =
            serde_json::from_str(include_str!("../../../docs/themes/registry.json")).unwrap();
        let slugs: Vec<&str> =
            registry["themes"].as_array().unwrap().iter().map(|t| t["slug"].as_str().unwrap()).collect();
        assert_eq!(THEMES.iter().map(|t| t.name).collect::<Vec<_>>(), slugs);
    }

    /// Each family of a theme that has no italic and does not say it is upright by design: a
    /// family whose script sets no italic, as Japanese and Korean do not, says so (`upright`,
    /// ADR-0031, call 6), and one that merely lost its italic does not ship.
    fn without_italics(theme: &serde_json::Value) -> Vec<String> {
        let families = theme["type"]["families"].as_object().unwrap();
        (families.iter())
            .filter(|(_, f)| f.get("italic").is_none() && f.get("upright") != Some(&serde_json::Value::Bool(true)))
            .map(|(key, _)| key.clone())
            .collect()
    }

    #[test]
    fn every_font_a_theme_that_ships_names_ships_with_it() {
        for theme in THEMES {
            let parsed: serde_json::Value = serde_json::from_str(theme.text).unwrap();
            assert_eq!(without_italics(&parsed), Vec::<String>::new(), "{}: a family lost its italic", theme.name);
            let families = parsed["type"]["families"].as_object().unwrap();
            for family in families.values() {
                let file = family["file"].as_str().unwrap();
                assert!(font(file).is_some(), "{}: {file}", theme.name);
                if let Some(italic) = family.get("italic") {
                    let italic = italic["file"].as_str().unwrap();
                    assert!(font(italic).is_some(), "{}: {italic}", theme.name);
                }
            }
        }
        assert_eq!(theme("Ember").map(|t| t.file), Some("ember.theme.json"));
        assert!(theme("ember.theme.json").is_none());
    }

    /// A family upright by design ships without an italic, as Washi's and Hanji's will (ADR-0031,
    /// call 6); a Latin family that lost its italic, and says nothing, does not.
    #[test]
    fn only_a_family_upright_by_design_ships_without_an_italic() {
        let mut theme: serde_json::Value = serde_json::from_str(THEMES[0].text).unwrap();
        let families = theme["type"]["families"].as_object_mut().unwrap();
        let key = families.keys().next().unwrap().clone();
        families[&key].as_object_mut().unwrap().remove("italic");
        assert_eq!(without_italics(&theme), vec![key.clone()], "a family that lost its italic is refused");
        theme["type"]["families"][&key]["upright"] = serde_json::Value::Bool(true);
        assert_eq!(without_italics(&theme), Vec::<String>::new(), "a family upright by design ships");
        let parsed: scaena_core::model::theme::Theme = serde_json::from_value(theme).unwrap();
        assert_eq!(parsed.typography.families[&key].upright, Some(true));
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
