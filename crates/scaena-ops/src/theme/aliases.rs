//! The names a theme that ships answers to besides its own, as `docs/themes/registry.json` lists
//! them (PLAN T.0d). Written by `scripts/build_themes.py` from the registry; do not edit by hand.

/// Each name a theme that ships answers to besides its own, with the name it is: one it shipped
/// under before, which a deck or a bundle may still give it.
pub const ALIASES: [(&str, &str); 1] = [("gallery", "salon")];
