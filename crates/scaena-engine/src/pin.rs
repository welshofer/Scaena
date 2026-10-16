//! Phosphor Icons 2.1.1 (MIT, docs/icons/LICENSE-phosphor.txt) the engine draws itself, each
//! path on the icons' 256 by 256 view: `map-pin` in its fill weight, the pin a picture's `pin`
//! draws, and a forecast's icons in their regular and fill weights (ADR-0030). Written by
//! scripts/build_icons.py from docs/icons/phosphor.json, which its `--check` holds this file to.

/// `map-pin`, fill.
pub const MAP_PIN: &str = "M128,16C79.42,16.06,40.06,55.42,40,104C40,179.3,120,236.17,123.41,238.55C126.17,240.48,129.83,240.48,132.59,238.55C136,236.17,216,179.3,216,104C215.94,55.42,176.58,16.06,128,16ZM128,72C145.67,72,160,86.33,160,104C160,121.67,145.67,136,128,136C110.33,136,96,121.67,96,104C96,86.33,110.33,72,128,72Z";

/// The SHA-256 of `icons/weather.txt`, a forecast's icons in their regular and fill weights,
/// a line each,
/// `name path` (ADR-0030): what is handed over where they are not compiled in is held to it.
pub const WEATHER_SHA256: &str = "f0e49785e95869ba924d254f892f86f1a3bf9ef1e2162da17ff670df23d4847a";
