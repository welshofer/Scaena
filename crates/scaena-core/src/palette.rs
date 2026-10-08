//! A theme's colors from a photo (PLAN 2.94): the photo's hues on the theme's own tones.
//!
//! [`read`] reads a photo's colors from a sample of its pixels in Oklab, where a distance is a
//! difference seen:
//! - Its **hues**: the pixels colorful enough to have one, counted by hue in 10° bins. A bin
//!   scores by how much of the photo lies within 10° of it and by how colorful its pixels are,
//!   so a small vivid flower can outscore a wide dull field.
//! - Its **cast**: the hue its pixels lean to together, and how far.
//!
//! [`theme`] gives a theme's colors those hues and that cast, on the theme's tones:
//! - A **neutral** (chroma under 0.05) keeps its lightness and takes the cast, a little of it.
//! - The **accent**, the color the theme's `accent` role names, takes the photo's best hue; the
//!   next chromatic color, the best hue at least 60° from it. Each other chromatic color turns
//!   with the accent, by as many degrees. Each keeps its lightness and its chroma, the theme's
//!   tone, held to what sRGB shows there.
//! - A color a type role sets text in then moves away from the surfaces (the `surface` and
//!   `surface-2` roles) until it reads on each at WCAG's ratio: 4.5:1, or 3:1 where every role
//!   that sets text in it is display-sized, as lint judges text (E110, E111).
//! - A data palette's color that is one of the theme's colors becomes that color's new value;
//!   any other is a neutral or turns with the accent, as above.
//!
//! Every step is `f64` through `libm`, so a photo makes the same theme natively and in the
//! browser (SPEC §13).

use crate::color;
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Value, json};

/// About how many pixels [`read`] reads: every `step`-th in each direction, the photo's whole.
const SAMPLES: f64 = 20_000.0;
/// The least chroma a pixel has a hue at.
const COLORFUL: f64 = 0.04;
/// The lightness a pixel's hue is read between: near black or white, a hue is noise.
const LIGHTNESS: std::ops::RangeInclusive<f64> = 0.12..=0.96;
/// The most chroma a neutral color takes from the photo's cast.
const TINT: f64 = 0.016;
/// Under this chroma, a theme's color is a neutral.
const NEUTRAL: f64 = 0.05;
/// How far apart, in degrees, the hues [`read`] gives are, and how far `accent-2`'s is from the
/// accent's.
const HUES_APART: f64 = 30.0;
const SECOND_APART: f64 = 60.0;
/// How near in Oklab a color the photo gives is to the one the theme has, under which the theme
/// keeps its own: a quarter of a difference seen, so a photo taken again changes nothing.
const STILL: f64 = 0.005;
/// WCAG's ratios: body text, and large text (lint's E110 and E111).
const BODY: f64 = 4.5;
const LARGE: f64 = 3.0;

/// A hue a photo shows.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Hue {
    /// Its angle, in degrees of Oklch hue.
    pub degrees: f64,
    /// How colorful its pixels are: their mean Oklch chroma.
    pub chroma: f64,
    /// The share of the photo's pixels within 10° of it, 0 to 1.
    pub share: f64,
}

/// What a photo's colors are.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Read {
    /// Its hues, best first, each at least 30° from those before it: at most four.
    pub hues: Vec<Hue>,
    /// The hue its pixels lean to together, and how far.
    pub cast: Cast,
}

/// The hue a photo's pixels lean to together: the Oklch of their mean.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, JsonSchema)]
pub struct Cast {
    /// In degrees of Oklch hue.
    pub degrees: f64,
    /// How far: the chroma of the pixels' mean.
    pub chroma: f64,
}

/// A color, or a data palette, the photo sets in a theme.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Set {
    /// Where in the theme: a JSON Pointer (`/tokens/color/accent`, `/tokens/data/sequential`).
    pub path: String,
    /// The value it had: a color, or a palette's colors.
    pub was: Value,
    /// The value it takes.
    pub now: Value,
    /// For a color a type role sets text in: the least contrast it has on the surfaces, and
    /// the least it must have.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reads: Option<[f64; 2]>,
}

/// Read a photo's colors from its pixels, `width` × `height` RGBA bytes, row by row. A pixel
/// less than half opaque is not read.
pub fn read(rgba: &[u8], width: u32, height: u32) -> Read {
    let lin = linear();
    let (w, h) = (width as usize, height as usize);
    let pixels = (w * h).min(rgba.len() / 4);
    let step = ((pixels as f64 / SAMPLES).sqrt().ceil() as usize).max(1);
    #[derive(Clone, Copy, Default)]
    struct Bin {
        count: usize,
        a: f64,
        b: f64,
        chroma: f64,
    }
    let mut bins = [Bin::default(); 36];
    let (mut sum, mut read) = ([0.0; 2], 0usize);
    for y in (0..h).step_by(step) {
        for x in (0..w).step_by(step) {
            let i = (y * w + x) * 4;
            let Some(&[r, g, b, alpha]) = rgba.get(i..i + 4) else { continue };
            if alpha < 128 {
                continue;
            }
            let [l, a, bb] = oklab([lin[r as usize], lin[g as usize], lin[b as usize]]);
            read += 1;
            sum = [sum[0] + a, sum[1] + bb];
            let chroma = (a * a + bb * bb).sqrt();
            if chroma < COLORFUL || !LIGHTNESS.contains(&l) {
                continue;
            }
            let bin = &mut bins[(degrees(a, bb) / 10.0) as usize % 36];
            *bin = Bin { count: bin.count + 1, a: bin.a + a, b: bin.b + bb, chroma: bin.chroma + chroma };
        }
    }
    let Some(n) = (read > 0).then_some(read as f64) else {
        return Read { hues: Vec::new(), cast: Cast { degrees: 0.0, chroma: 0.0 } };
    };
    // A hue needs half a percent of the photo in its own bin.
    let floor = (read / 200).max(1);
    let mut scored: Vec<(f64, Hue)> = Vec::new();
    for (i, bin) in bins.iter().enumerate().filter(|(_, bin)| bin.count >= floor) {
        let near = bins[(i + 35) % 36].count + bin.count + bins[(i + 1) % 36].count;
        let share = near as f64 / n;
        let chroma = bin.chroma / bin.count as f64;
        let hue = Hue { degrees: degrees(bin.a, bin.b), chroma, share };
        scored.push((share.sqrt() * chroma * chroma.sqrt(), hue));
    }
    crate::sort::by(&mut scored, |a, b| b.0.total_cmp(&a.0));
    let mut hues: Vec<Hue> = Vec::new();
    for (_, hue) in scored {
        if hues.len() < 4 && hues.iter().all(|h| apart(h.degrees, hue.degrees) >= HUES_APART) {
            hues.push(hue);
        }
    }
    let mean = [sum[0] / n, sum[1] / n];
    Read {
        hues,
        cast: Cast { degrees: degrees(mean[0], mean[1]), chroma: (mean[0] * mean[0] + mean[1] * mean[1]).sqrt() },
    }
}

/// The colors `read` gives `theme`, the theme's JSON: each color and data palette, with what
/// it was and what it takes, in the theme's order. A value it keeps is listed too. The JSON is
/// read where it says what this needs, not as the typed theme, whose deserializer the browser's
/// module would carry a second time for a `Value`.
pub fn theme(theme: &Value, read: &Read) -> Result<Vec<Set>, String> {
    let object = |path: &str| theme.pointer(path).and_then(Value::as_object);
    let colors: Vec<(&String, &str)> =
        (object("/tokens/color").into_iter().flatten()).filter_map(|(name, v)| Some((name, v.as_str()?))).collect();
    let roles = object("/tokens/roles");
    let has = |name: &str| colors.iter().any(|(n, _)| *n == name);
    let named = |role: &str| roles.and_then(|r| r.get(role)?.as_str()).filter(|name| has(name)).map(String::from);
    let surface = named("surface").ok_or("the theme's `surface` role names none of its colors")?;
    let surfaces: Vec<String> = [Some(surface.clone()), named("surface-2")].into_iter().flatten().collect();
    let accent = named("accent").ok_or("the theme's `accent` role names none of its colors")?;

    // Each color, as it was: its bytes, in Oklch, and its alpha where it has one.
    let mut was: Vec<(String, [u8; 3], Lch, Option<u8>)> = Vec::new();
    for (name, literal) in &colors {
        let [r, g, b, a] = color::parse(literal).map_err(|e| format!("color `{name}`: {e}"))?.0;
        was.push((name.to_string(), [r, g, b], lch_of([r, g, b]), (a < 255).then_some(a)));
    }
    let base = |name: &str| was.iter().find(|(n, ..)| n == name).map(|(_, _, lch, _)| *lch);
    let accent_was = base(&accent).ok_or("no accent")?;

    // The accent's hue, and how the other chromatic colors turn: by as many degrees. A theme
    // whose accent is a neutral keeps its colors' hues.
    let first = read.hues.first().filter(|_| accent_was.c >= NEUTRAL);
    let accent_now = first.map_or(accent_was, |hue| Lch { h: hue.degrees, ..accent_was });
    let turn = |c: Lch| Lch { h: (c.h + accent_now.h - accent_was.h).rem_euclid(360.0), ..c };
    let tint = |c: Lch| Lch { l: c.l, c: read.cast.chroma.min(TINT), h: read.cast.degrees };
    // The second chromatic color takes the best hue far enough from the accent's.
    let second = read.hues.iter().skip(1).find(|h| first.is_some_and(|f| apart(f.degrees, h.degrees) >= SECOND_APART));
    let mut seconded = false;
    let mut now: Vec<(String, Lch)> = Vec::new();
    for (name, _, c, _) in &was {
        let c = *c;
        let to = if *name == accent {
            accent_now
        } else if c.c < NEUTRAL {
            tint(c)
        } else if let Some(hue) = second.filter(|_| !seconded) {
            seconded = true;
            Lch { h: hue.degrees, ..c }
        } else {
            turn(c)
        };
        now.push((name.clone(), to));
    }

    // Each color a type role sets text in reads on the surfaces: at the strictest ratio a role
    // that sets text in it asks.
    let mut needs: Vec<(String, f64)> = Vec::new();
    for role in object("/type/roles").into_iter().flat_map(|r| r.values()) {
        let asked = role.get("color").and_then(Value::as_str).unwrap_or("onSurface");
        let Some(name) = named(asked).or_else(|| has(asked).then(|| asked.to_string())) else { continue };
        let number = |key: &str| role.get(key).and_then(Value::as_f64).unwrap_or(0.0);
        let (size, weight) = (number("size"), number("weight"));
        let large = size >= 24.0 || (size >= 18.67 && weight >= 700.0);
        let ratio = if large { LARGE } else { BODY };
        match needs.iter_mut().find(|(n, _)| *n == name) {
            Some((_, r)) => *r = r.max(ratio),
            None => needs.push((name, ratio)),
        }
    }
    let lin = linear();
    let bytes_now = |now: &[(String, Lch)], name: &str| now.iter().find(|(n, _)| n == name).map(|(_, c)| bytes(*c));
    let grounds: Vec<[u8; 3]> = surfaces.iter().filter_map(|s| bytes_now(&now, s)).collect();
    let dark = grounds.iter().map(|g| luminance(&lin, *g)).fold(0.0, f64::max) < 0.18;
    let mut reads: Vec<(String, [f64; 2])> = Vec::new();
    for (name, ratio) in &needs {
        if surfaces.contains(name) {
            continue;
        }
        let Some(slot) = now.iter_mut().find(|(n, _)| n == name) else { continue };
        let least = |c: Lch| grounds.iter().map(|g| contrast(&lin, bytes(c), *g)).fold(f64::INFINITY, f64::min);
        if least(slot.1) < *ratio {
            // The least step of lightness away from the surfaces that reads.
            let (mut near, mut far) = (slot.1.l, if dark { 1.0 } else { 0.0 });
            for _ in 0..32 {
                let mid = (near + far) / 2.0;
                match least(Lch { l: mid, ..slot.1 }) >= *ratio {
                    true => far = mid,
                    false => near = mid,
                }
            }
            slot.1.l = far;
        }
        reads.push((name.clone(), [least(slot.1), *ratio]));
    }

    let mut out = Vec::new();
    for ((name, rgb, _, alpha), (_, to)) in was.iter().zip(&now) {
        let path = format!("/tokens/color/{}", name.replace('~', "~0").replace('/', "~1"));
        // A color that comes out as it was, or all but, keeps the way the theme writes it.
        let literal = colors.iter().find(|(n, _)| *n == name).map_or("", |(_, l)| l);
        let now = match near(bytes(*to), *rgb) {
            true => literal.to_string(),
            false => hex(bytes(*to), *alpha),
        };
        let reads = reads.iter().find(|(n, _)| n == name).map(|(_, r)| *r);
        out.push(Set { path, was: json!(literal), now: json!(now), reads });
    }
    // The data palettes: a theme color becomes its new value; any other color, neutral or
    // chromatic, as a theme color would.
    let colors_now: Vec<String> = out.iter().map(|s| s.now.as_str().unwrap_or_default().to_string()).collect();
    let became = |literal: &str| -> String {
        let Ok(parsed) = color::parse(literal) else { return literal.to_string() };
        if let Some(i) = colors.iter().position(|(_, v)| color::parse(v).is_ok_and(|c| c == parsed)) {
            return colors_now[i].clone();
        }
        let [r, g, b, a] = parsed.0;
        let c = lch_of([r, g, b]);
        let to = bytes(if c.c < NEUTRAL { tint(c) } else { turn(c) });
        match near(to, [r, g, b]) {
            true => literal.to_string(),
            false => hex(to, (a < 255).then_some(a)),
        }
    };
    for (key, palette) in object("/tokens/data").into_iter().flatten() {
        let Some(palette) = palette.as_array() else { continue };
        let was: Vec<&str> = palette.iter().filter_map(Value::as_str).collect();
        let now: Vec<String> = was.iter().map(|c| became(c)).collect();
        let path = format!("/tokens/data/{}", key.replace('~', "~0").replace('/', "~1"));
        out.push(Set { path, was: json!(was), now: json!(now), reads: None });
    }
    Ok(out)
}

/// What [`theme`] sets, as a theme edit's operations (ADR-0016): a `replace` of each value it
/// changes.
pub fn ops(sets: &[Set]) -> Vec<Value> {
    (sets.iter().filter(|s| s.was != s.now))
        .map(|s| json!({ "op": "replace", "path": s.path, "value": s.now }))
        .collect()
}

/// A color in Oklch: lightness, chroma, and hue in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Lch {
    l: f64,
    c: f64,
    h: f64,
}

/// Each sRGB byte in linear light, as WCAG and lint read it.
fn linear() -> [f64; 256] {
    std::array::from_fn(|byte| {
        let c = byte as f64 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { libm::pow((c + 0.055) / 1.055, 2.4) }
    })
}

/// Linear sRGB in Oklab (Ottosson).
fn oklab([r, g, b]: [f64; 3]) -> [f64; 3] {
    let l = libm::cbrt(0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b);
    let m = libm::cbrt(0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b);
    let s = libm::cbrt(0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b);
    [
        0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
    ]
}

/// The Oklch of sRGB bytes.
fn lch_of(rgb: [u8; 3]) -> Lch {
    let lin = linear();
    let [l, a, b] = oklab(rgb.map(|c| lin[c as usize]));
    Lch { l, c: (a * a + b * b).sqrt(), h: degrees(a, b) }
}

/// The hue of `a`, `b`, in degrees from 0 to 360.
fn degrees(a: f64, b: f64) -> f64 {
    libm::atan2(b, a).to_degrees().rem_euclid(360.0)
}

/// How far apart two hues are, the short way round.
fn apart(x: f64, y: f64) -> f64 {
    let d = (x - y).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// `c` as sRGB bytes, its chroma lessened, where it must be, to the most sRGB shows at its
/// lightness and hue.
fn bytes(c: Lch) -> [u8; 3] {
    let at = |chroma: f64| {
        let h = c.h.to_radians();
        color::oklab_to_linear([c.l, chroma * libm::cos(h), chroma * libm::sin(h)])
    };
    let inside = |rgb: [f64; 3]| rgb.iter().all(|v| (-1e-6..=1.0 + 1e-6).contains(v));
    let rgb = match inside(at(c.c)) {
        true => at(c.c),
        false => {
            let (mut lo, mut hi) = (0.0, c.c);
            for _ in 0..24 {
                let mid = (lo + hi) / 2.0;
                match inside(at(mid)) {
                    true => lo = mid,
                    false => hi = mid,
                }
            }
            at(lo)
        }
    };
    rgb.map(color::encode)
}

/// Whether two colors are nearer in Oklab than [`STILL`].
fn near(x: [u8; 3], y: [u8; 3]) -> bool {
    let lin = linear();
    let ([l1, a1, b1], [l2, a2, b2]) = (oklab(x.map(|c| lin[c as usize])), oklab(y.map(|c| lin[c as usize])));
    let (dl, da, db) = (l1 - l2, a1 - a2, b1 - b2);
    (dl * dl + da * da + db * db).sqrt() < STILL
}

/// WCAG relative luminance of sRGB bytes.
fn luminance(lin: &[f64; 256], [r, g, b]: [u8; 3]) -> f64 {
    0.2126 * lin[r as usize] + 0.7152 * lin[g as usize] + 0.0722 * lin[b as usize]
}

/// The WCAG contrast ratio of two colors.
fn contrast(lin: &[f64; 256], x: [u8; 3], y: [u8; 3]) -> f64 {
    let (a, b) = (luminance(lin, x), luminance(lin, y));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// sRGB bytes as a theme writes a color: `#RRGGBB`, and its alpha where it had one.
fn hex([r, g, b]: [u8; 3], alpha: Option<u8>) -> String {
    match alpha {
        Some(a) => format!("#{r:02X}{g:02X}{b:02X}{a:02X}"),
        None => format!("#{r:02X}{g:02X}{b:02X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUSK: &str = include_str!("../../../docs/examples/themes/dusk.theme.json");
    const DAYBREAK: &str = include_str!("../../../docs/examples/authorability/themes/daybreak.theme.json");

    /// A photo `width` pixels across made of bands of colors, each `rows` rows high.
    fn photo(width: u32, bands: &[([u8; 4], u32)]) -> (Vec<u8>, u32, u32) {
        let mut rgba = Vec::new();
        for (color, rows) in bands {
            for _ in 0..width * rows {
                rgba.extend_from_slice(color);
            }
        }
        let height = bands.iter().map(|(_, rows)| rows).sum();
        (rgba, width, height)
    }

    fn hue_of(rgb: [u8; 3]) -> f64 {
        lch_of(rgb).h
    }

    fn rgb(hex: &str) -> [u8; 3] {
        let [r, g, b, _] = color::parse(hex).unwrap().0;
        [r, g, b]
    }

    const BLUE: [u8; 4] = [0x2F, 0x6F, 0xD0, 0xFF];
    const ORANGE: [u8; 4] = [0xE0, 0x70, 0x20, 0xFF];
    const GRAY: [u8; 4] = [0x80, 0x80, 0x80, 0xFF];

    #[test]
    fn a_photo_reads_as_its_hues_best_first_and_the_hue_it_leans_to() {
        let (rgba, w, h) = photo(200, &[(BLUE, 120), (ORANGE, 60), (GRAY, 20)]);
        let read = read(&rgba, w, h);
        let hues: Vec<f64> = read.hues.iter().map(|h| h.degrees).collect();
        assert_eq!(hues.len(), 2, "{read:?}");
        assert!(apart(hues[0], hue_of([0x2F, 0x6F, 0xD0])) < 0.5, "{read:?}");
        assert!(apart(hues[1], hue_of([0xE0, 0x70, 0x20])) < 0.5, "{read:?}");
        assert!((read.hues[0].share - 0.6).abs() < 0.01 && (read.hues[1].share - 0.3).abs() < 0.01, "{read:?}");
        // Blue is most of it, so the photo leans blue.
        assert!(apart(read.cast.degrees, hues[0]) < 30.0 && read.cast.chroma > 0.02, "{read:?}");
    }

    #[test]
    fn a_gray_photo_has_no_hue_and_a_pixel_not_half_opaque_is_not_read() {
        let (rgba, w, h) = photo(100, &[(GRAY, 50), ([0x30, 0x30, 0x30, 0xFF], 50)]);
        let gray = read(&rgba, w, h);
        assert!(gray.hues.is_empty() && gray.cast.chroma < 1e-6, "{gray:?}");
        let clear = [0xE0, 0x70, 0x20, 0x40];
        let (rgba, w, h) = photo(100, &[(clear, 80), (BLUE, 20)]);
        let blue = read(&rgba, w, h);
        assert_eq!(blue.hues.len(), 1, "{blue:?}");
        assert!((blue.hues[0].share - 1.0).abs() < 1e-9, "only the opaque pixels count: {blue:?}");
    }

    /// Dusk, from a blue and orange photo: the accent takes the blue, `accent-2` the orange, the
    /// neutrals lean blue a little, each on its own tone, and every color text is set in reads.
    #[test]
    fn a_theme_takes_the_photos_hues_on_its_own_tones() {
        let theme: Value = serde_json::from_str(DUSK).unwrap();
        let (rgba, w, h) = photo(200, &[(BLUE, 120), (ORANGE, 60), (GRAY, 20)]);
        let read = read(&rgba, w, h);
        let set = super::theme(&theme, &read).unwrap();
        let now = |path: &str| set.iter().find(|s| s.path == path).unwrap().now.as_str().unwrap().to_string();
        let was = |path: &str| theme.pointer(path).unwrap().as_str().unwrap().to_string();
        let (accent, accent_2) =
            (lch_of(rgb(&now("/tokens/color/accent"))), lch_of(rgb(&now("/tokens/color/accent-2"))));
        assert!(apart(accent.h, read.hues[0].degrees) < 3.0, "{accent:?}");
        assert!(apart(accent_2.h, read.hues[1].degrees) < 3.0, "{accent_2:?}");
        // The theme's tones: each color's lightness, and an accent's chroma where sRGB shows it.
        for name in ["ink", "paper", "paper-2", "muted", "line", "accent-2"] {
            let path = format!("/tokens/color/{name}");
            let (a, b) = (lch_of(rgb(&was(&path))), lch_of(rgb(&now(&path))));
            assert!((a.l - b.l).abs() < 0.01, "{name}: {a:?} → {b:?}");
        }
        for name in ["ink", "paper", "paper-2", "muted", "line"] {
            let c = lch_of(rgb(&now(&format!("/tokens/color/{name}"))));
            assert!(c.c <= TINT + 0.005 && apart(c.h, read.cast.degrees) < 25.0, "{name} leans to the cast: {c:?}");
        }
        // Text reads on both surfaces: ink and muted at WCAG's, the accent at body text's, since
        // the kicker sets 20-pixel text in it.
        let reads: Vec<(&str, [f64; 2])> = set.iter().filter_map(|s| Some((s.path.as_str(), s.reads?))).collect();
        let names: Vec<&str> = reads.iter().map(|(p, _)| *p).collect();
        assert_eq!(names, ["/tokens/color/ink", "/tokens/color/accent", "/tokens/color/muted"], "{reads:?}");
        assert!(reads.iter().all(|(_, [ratio, needs])| ratio >= needs), "{reads:?}");
        assert_eq!(reads[1].1[1], BODY);
        // A data color that is a theme color is its new value: the sequential palette runs from
        // the line to the accent, the diverging one through paper-2.
        let palette = |key: &str| set.iter().find(|s| s.path == format!("/tokens/data/{key}")).unwrap().now.clone();
        assert_eq!(palette("sequential")[0], json!(now("/tokens/color/line")));
        assert_eq!(palette("sequential")[4], json!(now("/tokens/color/accent")));
        assert_eq!(palette("diverging")[1], json!(now("/tokens/color/paper-2")));
        // The edit replaces what changed, nothing else, and the photo taken again changes
        // nothing more.
        let ops = ops(&set);
        assert!(ops.iter().all(|op| op["op"] == "replace"));
        assert_eq!(ops.len(), set.iter().filter(|s| s.was != s.now).count());
        let mut edited = theme.clone();
        crate::patch::apply(&mut edited, &ops).unwrap();
        assert_eq!(super::ops(&super::theme(&edited, &read).unwrap()), Vec::<Value>::new());
    }

    /// On a light theme whose accent is too light to read on its paper, the photo's hue at that
    /// tone is darkened, the least step that reads.
    #[test]
    fn a_text_color_that_would_not_read_moves_away_from_the_surfaces() {
        let mut theme: Value = serde_json::from_str(DAYBREAK).unwrap();
        theme["tokens"]["color"]["accent"] = json!("#2BA197");
        let (rgba, w, h) = photo(100, &[([0xF2, 0xC8, 0x1E, 0xFF], 100)]);
        let read = read(&rgba, w, h);
        let set = super::theme(&theme, &read).unwrap();
        let accent = set.iter().find(|s| s.path == "/tokens/color/accent").unwrap();
        let [ratio, needs] = accent.reads.unwrap();
        assert!(ratio >= needs && ratio < needs + 0.1, "the least step that reads: {accent:?}");
        let (was, now) = (lch_of(rgb(accent.was.as_str().unwrap())), lch_of(rgb(accent.now.as_str().unwrap())));
        assert!(now.l < was.l - 0.05, "darker: {was:?} → {now:?}");
        assert!(apart(now.h, read.hues[0].degrees) < 3.0, "the photo's hue: {now:?}");
    }

    #[test]
    fn a_gray_photo_keeps_the_accents_and_grays_the_neutrals() {
        let theme: Value = serde_json::from_str(DUSK).unwrap();
        let (rgba, w, h) = photo(100, &[(GRAY, 100)]);
        let set = super::theme(&theme, &read(&rgba, w, h)).unwrap();
        let get = |path: &str| set.iter().find(|s| s.path == path).unwrap();
        for name in ["accent", "accent-2"] {
            let s = get(&format!("/tokens/color/{name}"));
            assert_eq!(s.was, s.now, "{name}");
        }
        let paper = lch_of(rgb(get("/tokens/color/paper").now.as_str().unwrap()));
        assert!(paper.c < 0.002, "{paper:?}");
    }
}
