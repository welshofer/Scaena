//! The themes that ship (SPEC §3.6), with their fonts, carried in the binary: what a deck starts
//! from where no theme file is at hand (PLAN 2.13). The registry, `docs/themes/registry.json`,
//! lists them, each written from its spec (PLAN T.0a, ADR-0031). Dusk, Daybreak, Ember, and
//! Salon share one vocabulary on one grid, and the same body and mono fonts, each with its
//! italic: Inter and JetBrains Mono. The first three set display in Fraunces, and Salon in
//! Instrument Serif (PLAN 2.102). Each is found by its name, one word, in any case; Salon by
//! `gallery` too, the name it shipped under, so a deck that names Gallery opens as before (PLAN
//! T.0d). Each font carries its copyright and its license (the SIL Open Font License) in its own
//! name table.
//!
//! Every font every theme in the registry names is carried, compressed, and inflated the first
//! time a deck needs it (PLAN T.0c, ADR-0031 call 3): the build script finds each in the
//! repository or the sha256 cache, by `shipped/fonts.tsv`, and reaches no network. A deck made or
//! re-themed takes only its theme's fonts.

use std::sync::OnceLock;

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
    /// The fonts it names, by the paths it gives them: each family's, then their italics'.
    pub font_files: &'static [&'static str],
}

impl Shipped {
    /// The fonts this theme names, each by the path it gives it: what a deck made in it, or put
    /// in it, takes, and no other family's.
    pub fn fonts(&self) -> impl Iterator<Item = (&'static str, &'static [u8])> {
        self.font_files.iter().filter_map(|path| Some((*path, font(path)?)))
    }
}

/// A font carried in the binary, deflated.
struct Font {
    path: &'static str,
    /// Its family's key in `scripts/build_bundle_fonts.py`'s pinned table.
    #[cfg_attr(not(test), allow(dead_code))]
    family: &'static str,
    /// Its size inflated.
    size: usize,
    packed: &'static [u8],
}

mod table;

use crate::theme::ALIASES;

include!(concat!(env!("OUT_DIR"), "/fonts.rs"));

/// Each font inflated, the first time it is asked for.
static INFLATED: [OnceLock<Vec<u8>>; FONTS.len()] = [const { OnceLock::new() }; FONTS.len()];

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
    FONTS.iter().filter_map(|f| Some((f.path, font(f.path)?)))
}

/// A font a theme that ships names, by the path it gives it.
pub fn font(path: &str) -> Option<&'static [u8]> {
    let i = FONTS.iter().position(|f| f.path == path)?;
    Some(INFLATED[i].get_or_init(|| inflated(&FONTS[i])))
}

/// What the binary carries of the fonts, `(inflated, deflated)` bytes: what T.0c records.
pub fn carried() -> (usize, usize) {
    FONTS.iter().fold((0, 0), |(a, b), f| (a + f.size, b + f.packed.len()))
}

fn inflated(font: &Font) -> Vec<u8> {
    use std::io::Read as _;
    let mut bytes = Vec::with_capacity(font.size);
    // The build deflated these bytes itself; a failure here is a broken build, not a deck's.
    flate2::read::DeflateDecoder::new(font.packed).read_to_end(&mut bytes).expect("a font the build deflated");
    bytes
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
            let mut named = Vec::new();
            for family in families.values() {
                let file = family["file"].as_str().unwrap();
                assert!(font(file).is_some(), "{}: {file}", theme.name);
                if !named.contains(&file) {
                    named.push(file);
                }
            }
            for family in families.values() {
                let Some(italic) = family.get("italic") else { continue };
                let italic = italic["file"].as_str().unwrap();
                assert!(font(italic).is_some(), "{}: {italic}", theme.name);
                if !named.contains(&italic) {
                    named.push(italic);
                }
            }
            // What a deck made in it takes: its own fonts, and no other theme's (PLAN T.0c).
            let given: Vec<&str> = theme.fonts().map(|(path, _)| path).collect();
            assert_eq!(given, named, "{}", theme.name);
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

    /// Every font the binary carries is one a theme in the registry names, of a family the
    /// registry gives that theme, carried deflated and inflated whole (PLAN T.0c).
    #[test]
    fn every_font_carried_is_a_registry_familys() {
        let registry: serde_json::Value =
            serde_json::from_str(include_str!("../../../docs/themes/registry.json")).unwrap();
        for entry in registry["themes"].as_array().unwrap() {
            let theme = theme(entry["slug"].as_str().unwrap()).unwrap();
            let keys: Vec<&str> = entry["fonts"].as_array().unwrap().iter().map(|k| k.as_str().unwrap()).collect();
            for path in theme.font_files {
                let carried = FONTS.iter().find(|f| f.path == *path).unwrap();
                assert!(keys.contains(&carried.family), "{}: {path} is of `{}`", theme.name, carried.family);
            }
        }
        for f in &FONTS {
            assert!(THEMES.iter().any(|t| t.font_files.contains(&f.path)), "{} is carried for no theme", f.path);
            let bytes = font(f.path).unwrap();
            assert_eq!(bytes.len(), f.size, "{}", f.path);
            assert!(bytes.starts_with(&[0, 1, 0, 0]) || bytes.starts_with(b"OTTO"), "{} is a font", f.path);
        }
        let (inflated, deflated) = carried();
        assert!(deflated < inflated, "{deflated} deflated of {inflated}");
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

    /// Every theme that ships gives a forecast its look (PLAN O.2f), whose words read at 4.5:1 on
    /// the surface and on the band a highlight sets its period on, and whose icons at 3:1.
    #[test]
    fn every_theme_that_ships_draws_a_forecast_that_reads() {
        for theme in THEMES {
            let parsed: serde_json::Value = serde_json::from_str(theme.text).unwrap();
            let hex = |name: &str| -> [u8; 3] {
                let name = parsed["tokens"]["roles"][name].as_str().unwrap_or(name);
                let literal = parsed["tokens"]["color"][name].as_str().unwrap_or(name);
                let c = scaena_core::color::parse(literal).unwrap_or_else(|e| panic!("{}: {name}: {e}", theme.name));
                [c.0[0], c.0[1], c.0[2]]
            };
            let look = &parsed["charts"]["weather"];
            assert!(look.is_object(), "{} gives a forecast its look", theme.name);
            let grounds = [hex("surface"), hex(look["band"].as_str().unwrap_or("surface-2"))];
            let texts = [("period", "onSurface"), ("high", "onSurface"), ("low", "onSurfaceMuted")];
            let texts = texts.into_iter().chain([("chance", "onSurfaceMuted"), ("caption", "onSurfaceMuted")]);
            for (part, default) in texts {
                let color = hex(look[part]["color"].as_str().unwrap_or(default));
                for ground in grounds {
                    let ratio = scaena_core::palette::ratio(color, ground);
                    assert!(ratio >= 4.5, "{}: the {part} reads at {ratio:.2}:1", theme.name);
                }
            }
            let icon = hex(look["icon"]["color"].as_str().unwrap_or("onSurface"));
            for ground in grounds {
                let ratio = scaena_core::palette::ratio(icon, ground);
                assert!(ratio >= 3.0, "{}: the icon stands at {ratio:.2}:1", theme.name);
            }
        }
    }
}
