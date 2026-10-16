//! The themes that ship (SPEC §3.6), with their fonts, carried in the binary: what a deck starts
//! from where no theme file is at hand (PLAN 2.13). The registry, `docs/themes/registry.json`,
//! lists them, each written from its spec (PLAN T.0a, ADR-0031). Dusk, Daybreak, Ember, and
//! Gallery share one vocabulary on one grid, and the same body and mono fonts, each with its
//! italic: Inter and JetBrains Mono. The first three set display in Fraunces, and Gallery in
//! Instrument Serif (PLAN 2.102). Each font carries its copyright and its license (the SIL Open
//! Font License) in its own name table.
//!
//! Every font every theme in the registry names is carried, compressed, and inflated the first
//! time a deck needs it (PLAN T.0c, ADR-0031 call 3): the build script finds each in the
//! repository or the sha256 cache, by `shipped/fonts.tsv`, and reaches no network. A deck made or
//! re-themed takes only its theme's fonts.

use std::sync::OnceLock;

/// A theme that ships.
#[derive(Debug, Clone, Copy)]
pub struct Shipped {
    /// Its name, as `create` takes it: `dusk`, `daybreak`, `ember`, or `gallery`.
    pub name: &'static str,
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

include!(concat!(env!("OUT_DIR"), "/fonts.rs"));

/// Each font inflated, the first time it is asked for.
static INFLATED: [OnceLock<Vec<u8>>; FONTS.len()] = [const { OnceLock::new() }; FONTS.len()];

/// Every theme that ships, as `docs/themes/registry.json` lists them (PLAN T.0a, ADR-0031):
/// `scripts/build_themes.py` writes the table from it.
pub use table::THEMES;

/// The theme that ships as `name`, in any case.
pub fn theme(name: &str) -> Option<Shipped> {
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

    /// The themes that ship are the registry's, in its order (PLAN T.0a).
    #[test]
    fn the_themes_that_ship_are_the_registrys() {
        let registry: serde_json::Value =
            serde_json::from_str(include_str!("../../../docs/themes/registry.json")).unwrap();
        let slugs: Vec<&str> =
            registry["themes"].as_array().unwrap().iter().map(|t| t["slug"].as_str().unwrap()).collect();
        assert_eq!(THEMES.iter().map(|t| t.name).collect::<Vec<_>>(), slugs);
    }

    #[test]
    fn every_font_a_theme_that_ships_names_ships_with_it() {
        for theme in THEMES {
            let parsed: serde_json::Value = serde_json::from_str(theme.text).unwrap();
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
                let italic = family["italic"]["file"].as_str().expect("every family that ships has an italic");
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
}
