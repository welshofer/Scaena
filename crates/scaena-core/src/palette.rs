//! A theme's colors from a photo (PLAN 2.94): the photo's hues on the theme's own tones. And the
//! colors code is drawn in (PLAN 2.132), each held to read where it is drawn.
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
//! - A color the theme sets code in (`code.syntax`, ADR-0029) reads on the surfaces as body text
//!   does: one of the theme's colors is held so with the colors text is set in, and one written
//!   out is a neutral or turns with the accent, then is held so.
//!
//! It also colors code where the theme names no color for a kind of token ([`syntax`]): each kind
//! a color derived from the accent, and every color code is drawn in held to read on the ground it
//! is drawn on by the least step of lightness ([`held`]), as the engine draws it.
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
    // Each color of the theme code is set in (ADR-0029) reads on the surfaces as body text does.
    let syntax = object("/code/syntax");
    for look in syntax.into_iter().flat_map(|s| s.values()) {
        let Some(asked) = look.get("color").and_then(Value::as_str) else { continue };
        let Some(name) = named(asked).or_else(|| has(asked).then(|| asked.to_string())) else { continue };
        match needs.iter_mut().find(|(n, _)| *n == name) {
            Some((_, r)) => *r = r.max(BODY),
            None => needs.push((name, BODY)),
        }
    }
    let lin = linear();
    let bytes_now = |now: &[(String, Lch)], name: &str| now.iter().find(|(n, _)| n == name).map(|(_, c)| bytes(*c));
    let grounds: Vec<[u8; 3]> = surfaces.iter().filter_map(|s| bytes_now(&now, s)).collect();
    let dark = grounds.iter().map(|g| luminance(&lin, *g)).fold(0.0, f64::max) < 0.18;
    let least = |c: Lch| grounds.iter().map(|g| contrast(&lin, bytes(c), *g)).fold(f64::INFINITY, f64::min);
    // `c` moved the least step of lightness away from the surfaces that reads at `ratio`.
    let reading = |c: Lch, ratio: f64| {
        if least(c) >= ratio {
            return c;
        }
        let (mut near, mut far) = (c.l, if dark { 1.0 } else { 0.0 });
        for _ in 0..32 {
            let mid = (near + far) / 2.0;
            match least(Lch { l: mid, ..c }) >= ratio {
                true => far = mid,
                false => near = mid,
            }
        }
        Lch { l: far, ..c }
    };
    let mut reads: Vec<(String, [f64; 2])> = Vec::new();
    for (name, ratio) in &needs {
        if surfaces.contains(name) {
            continue;
        }
        let Some(slot) = now.iter_mut().find(|(n, _)| n == name) else { continue };
        slot.1 = reading(slot.1, *ratio);
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
    // A color code is set in that the theme writes out, not one of its colors, turns with the
    // accent as a theme color would, or takes the cast where it is a neutral, then reads on the
    // surfaces as body text does (ADR-0029).
    for (kind, look) in syntax.into_iter().flatten() {
        let Some(literal) = look.get("color").and_then(Value::as_str) else { continue };
        if named(literal).is_some() || has(literal) {
            continue;
        }
        let Ok(parsed) = color::parse(literal) else { continue };
        let [r, g, b, a] = parsed.0;
        let c = lch_of([r, g, b]);
        let to = reading(if c.c < NEUTRAL { tint(c) } else { turn(c) }, BODY);
        let now = match near(bytes(to), [r, g, b]) {
            true => literal.to_string(),
            false => hex(bytes(to), (a < 255).then_some(a)),
        };
        let path = format!("/code/syntax/{}/color", kind.replace('~', "~0").replace('/', "~1"));
        out.push(Set { path, was: json!(literal), now: json!(now), reads: Some([least(to), BODY]) });
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

/// WCAG's ratio for body text, which every color code is drawn in reads at on its ground
/// (ADR-0029).
pub const READS: f64 = BODY;

/// The ratio a code text's keywords read at where their ground allows it (ADR-0029): WCAG's
/// enhanced ratio, so the words that shape a block stand out from the rest.
pub const STRONG: f64 = 7.0;

/// The WCAG contrast ratio of two sRGB colors, as lint reads it (E110, E111).
pub fn ratio(x: [u8; 3], y: [u8; 3]) -> f64 {
    contrast(&linear(), x, y)
}

/// `color` held to read at `ratio`:1 on `ground` by the least step of lightness (ADR-0029): as it
/// is where it reads; else moved in Oklch lightness, its hue kept and its chroma lessened where
/// sRGB shows less there, to the nearest lightness on either side that reads. Where none does, it
/// is the end, black or white, that reads best. Each side is bisected as [`theme`] bisects, 32
/// steps, so the step is the least to within a 4-billionth of the lightness.
pub fn held(color: [u8; 3], ground: [u8; 3], ratio: f64) -> [u8; 3] {
    let lin = linear();
    let reads = |c: Lch| contrast(&lin, bytes(c), ground) >= ratio;
    let start = lch_of(color);
    if contrast(&lin, color, ground) >= ratio {
        return color;
    }
    // Away from where it is, toward `end`: the nearest lightness that reads, if any does. From a
    // color that does not read, the colors toward either end read nowhere, then everywhere past
    // where they first do, so the side bisects.
    let side = |end: f64| -> Option<f64> {
        if !reads(Lch { l: end, ..start }) {
            return None;
        }
        let (mut near, mut far) = (start.l, end);
        for _ in 0..32 {
            let mid = (near + far) / 2.0;
            match reads(Lch { l: mid, ..start }) {
                true => far = mid,
                false => near = mid,
            }
        }
        Some(far)
    };
    let l = match (side(0.0), side(1.0)) {
        (Some(dark), Some(light)) if start.l - dark <= light - start.l => dark,
        (_, Some(light)) => light,
        (Some(dark), None) => dark,
        // Nothing reads: the end that reads best.
        (None, None) => match contrast(&lin, [0; 3], ground) >= contrast(&lin, [255; 3], ground) {
            true => 0.0,
            false => 1.0,
        },
    };
    bytes(Lch { l, ..start })
}

/// The ratio a line of code out of focus is held to (ADR-0029, call 3; SPEC §7.5): set back on
/// purpose by a walkthrough, it reads at 3:1, WCAG's ratio for large text, and lint holds it so.
pub const SET_BACK: f64 = LARGE;

/// How far a walkthrough sets back the lines out of focus where the theme says not (ADR-0029):
/// each token's color 0.55 of the way toward its ground.
pub const DIM: f64 = 0.55;

/// `color` set back toward `ground` by `dim`, 0 to 1, as a walkthrough sets back the lines out of
/// focus (PLAN 2.140, ADR-0029): its Oklch lightness and chroma moved that far toward the
/// ground's, its hue kept, so a keyword stays a keyword's color, only fainter. It goes no further
/// than [`SET_BACK`]:1 on `ground`: where the whole way would go under, as far as it can and read
/// so, bisected as [`held`] bisects. A color already under it is as it is.
pub fn dimmed(color: [u8; 3], ground: [u8; 3], dim: f64) -> [u8; 3] {
    let lin = linear();
    if contrast(&lin, color, ground) <= SET_BACK || dim <= 0.0 {
        return color;
    }
    let (from, to) = (lch_of(color), lch_of(ground));
    let at = |t: f64| bytes(Lch { l: from.l + t * (to.l - from.l), c: from.c + t * (to.c - from.c), h: from.h });
    let dim = dim.min(1.0);
    if contrast(&lin, at(dim), ground) >= SET_BACK {
        return at(dim);
    }
    let (mut near, mut far) = (0.0, dim);
    for _ in 0..32 {
        let mid = (near + far) / 2.0;
        match contrast(&lin, at(mid), ground) >= SET_BACK {
            true => near = mid,
            false => far = mid,
        }
    }
    at(near)
}

/// [`held`] at [`STRONG`] where `ground` allows it, else at [`READS`].
pub fn held_strong(color: [u8; 3], ground: [u8; 3]) -> [u8; 3] {
    let strong = held(color, ground, STRONG);
    match ratio(strong, ground) >= STRONG {
        true => strong,
        false => held(color, ground, READS),
    }
}

/// The color each kind of token in code takes where the theme names none (ADR-0029), in
/// `crate::code::Class::KINDS`' order, each held to read on `ground`: keywords at [`STRONG`]:1
/// where it allows, every other kind at [`READS`]:1. `text` is the color the code's plain words
/// are set in, `accent` and `second` the theme's accent and `accent-2`, and `muted` its muted
/// text's color.
///
/// - **Keywords** are the accent, and a tag is a keyword's color.
/// - **Numbers and constants** are `accent-2` where it is chromatic and 45° or more from the
///   accent; else the accent turned 60°.
/// - **Strings, types, functions, properties, and variables** are the accent turned 150°, 210°,
///   270°, 330°, and 90°, at its lightness, its chroma held to 0.08–0.16. An attribute is a
///   function's color.
/// - **Comments and meta** are the muted color; **operators and punctuation** the text's color,
///   a third of the way to the muted one in Oklab.
/// - **Inserted and deleted** lines are green and red whatever the accent: hues 145° and 25°.
///
/// A neutral accent gives neutral kinds, each the gray it would be at its lightness: they are
/// told apart by weight and italic, which the engine gives keywords and comments.
pub fn syntax(
    ground: [u8; 3],
    text: [u8; 3],
    accent: [u8; 3],
    second: Option<[u8; 3]>,
    muted: [u8; 3],
) -> [[u8; 3]; 16] {
    let lin = linear();
    let a = lch_of(accent);
    let neutral = a.c < NEUTRAL;
    let chroma = if neutral { 0.0 } else { a.c.clamp(0.08, 0.16) };
    let turned = |degrees: f64| bytes(Lch { l: a.l, c: chroma, h: (a.h + degrees).rem_euclid(360.0) });
    // A diff's lines keep their green and red on a neutral accent too.
    let hue = |degrees: f64| bytes(Lch { l: a.l, c: if neutral { 0.12 } else { chroma }, h: degrees });
    let keyword = match neutral {
        true => bytes(Lch { c: 0.0, ..a }),
        false => accent,
    };
    let numbers = match second.map(|s| (s, lch_of(s))) {
        Some((s, c)) if !neutral && c.c >= NEUTRAL && apart(c.h, a.h) >= 45.0 => s,
        _ => turned(60.0),
    };
    // The text's color a third of the way to the muted one, in Oklab.
    let between = {
        let [t, m] = [text, muted].map(|c| oklab(c.map(|v| lin[v as usize])));
        let mixed = [0, 1, 2].map(|i| t[i] + (m[i] - t[i]) / 3.0);
        color::oklab_to_linear(mixed).map(color::encode)
    };
    let function = turned(270.0);
    // Class::KINDS' order: keyword, type, function, string, number, constant, comment, operator,
    // punctuation, property, attribute, tag, variable, meta, inserted, deleted.
    let kinds = [
        keyword,
        turned(210.0),
        function,
        turned(150.0),
        numbers,
        numbers,
        muted,
        between,
        between,
        turned(330.0),
        function,
        keyword,
        turned(90.0),
        muted,
        hue(145.0),
        hue(25.0),
    ];
    let mut out = [[0; 3]; 16];
    for (i, kind) in kinds.into_iter().enumerate() {
        out[i] = match i {
            0 | 11 => held_strong(kind, ground),
            _ => held(kind, ground, READS),
        };
    }
    out
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

    /// A line out of focus is set back toward its ground (PLAN 2.140, ADR-0029): fainter, its hue
    /// kept, never under 3:1; the whole way where that reads so, else as far as it can and still
    /// read; and a color already under 3:1 as it is.
    #[test]
    fn a_line_out_of_focus_is_set_back_never_under_three_to_one() {
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut byte = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 32) as u8
        };
        for ground in [rgb("#101014"), rgb("#FBFAF6"), rgb("#29292E"), rgb("#DFDDD5")] {
            for _ in 0..300 {
                let color = held([byte(), byte(), byte()], ground, READS);
                let set = dimmed(color, ground, DIM);
                let was = ratio(color, ground);
                if was <= SET_BACK {
                    assert_eq!(set, color);
                    continue;
                }
                assert!(ratio(set, ground) >= SET_BACK, "{color:?} on {ground:?}: {set:?}");
                assert!(ratio(set, ground) < was, "{color:?} on {ground:?} is set back: {set:?}");
                let (from, to) = (lch_of(color), lch_of(set));
                if from.c > 0.05 && to.c > 0.02 {
                    assert!(apart(from.h, to.h) < 6.0, "{color:?} keeps its hue: {set:?}");
                }
            }
            // No dim, no change; a full dim stops at 3:1.
            let text = held(rgb("#C0C0C0"), ground, STRONG);
            assert_eq!(dimmed(text, ground, 0.0), text);
            let all = dimmed(text, ground, 1.0);
            assert!(ratio(all, ground) >= SET_BACK && ratio(all, ground) < SET_BACK + 0.25, "{all:?}");
        }
    }

    /// A color code is drawn in is held to read on its ground by the least step of lightness
    /// (ADR-0029): kept where it reads, else moved to the nearest lightness that reads, its hue
    /// kept; black or white, whichever reads best, where no lightness reads.
    #[test]
    fn a_code_color_is_held_to_read_by_the_least_step() {
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut byte = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 32) as u8
        };
        let grounds = [rgb("#101014"), rgb("#FBFAF6"), rgb("#808080"), rgb("#2F6FD0"), rgb("#F2C81E")];
        for ground in grounds {
            for _ in 0..300 {
                let color = [byte(), byte(), byte()];
                for wanted in [READS, STRONG] {
                    let held = held(color, ground, wanted);
                    if ratio(color, ground) >= wanted {
                        assert_eq!(held, color, "{color:?} reads on {ground:?} as it is");
                        continue;
                    }
                    if ratio([0; 3], ground).max(ratio([255; 3], ground)) < wanted {
                        assert!(held == [0; 3] || held == [255; 3], "{color:?} on {ground:?}: {held:?}");
                        continue;
                    }
                    assert!(ratio(held, ground) >= wanted, "{color:?} on {ground:?} at {wanted}: {held:?}");
                    // The least step: most of the way back toward where it was does not read.
                    let (from, to) = (lch_of(color), lch_of(held));
                    if (from.l - to.l).abs() > 0.02 {
                        let back = bytes(Lch { l: from.l + (to.l - from.l) * 0.9, ..from });
                        assert!(
                            ratio(back, ground) < wanted,
                            "{color:?} on {ground:?}: {held:?} is past the least step"
                        );
                    }
                    if from.c > 0.08 && to.c > 0.04 {
                        assert!(apart(from.h, to.h) < 8.0, "{color:?} keeps its hue: {from:?} → {to:?}");
                    }
                }
            }
        }
        // Where both sides read, the nearer: a mid gray on a mid ground goes the shorter way.
        let ground = rgb("#777777");
        let held = held([0x80, 0x80, 0x80], ground, READS);
        assert!(lch_of(held).l < lch_of(ground).l, "black reads on it and white does not: {held:?}");
    }

    /// Each kind of token takes a color derived from the theme that reads on its ground (ADR-0029):
    /// keywords at 7:1 where the ground allows, the rest at 4.5:1; keywords in the accent's hue,
    /// numbers in `accent-2`'s where it is 45° or more from it, a diff's lines green and red, and
    /// grays for the rest where the accent is a neutral.
    #[test]
    fn each_kind_of_token_takes_a_color_that_reads_on_its_ground() {
        use crate::code::Class;
        let at = |class: Class| Class::KINDS.iter().position(|k| *k == class).unwrap();
        let themes = [
            // Dusk, Daybreak, Ember, and a neutral accent on a mid gray, where 7:1 cannot be had.
            ("#101014", "#F2F0E9", "#FF6A3D", Some("#FFC857"), "#8A8A96"),
            ("#FBFAF6", "#16140F", "#0F766E", Some("#C2410C"), "#6B675E"),
            ("#0E0E12", "#F6F4EF", "#FF4100", Some("#FFB27A"), "#BEBDC3"),
            ("#808080", "#000000", "#444444", None, "#202020"),
        ];
        for (ground, text, accent, second, muted) in themes {
            let (ground, accent) = (rgb(ground), rgb(accent));
            let second = second.map(rgb);
            let kinds = syntax(ground, rgb(text), accent, second, rgb(muted));
            let strong = ratio([0; 3], ground).max(ratio([255; 3], ground)) >= STRONG;
            for (class, color) in Class::KINDS.into_iter().zip(kinds) {
                let wanted = match class {
                    Class::Keyword | Class::Tag if strong => STRONG,
                    _ => READS,
                };
                assert!(ratio(color, ground) >= wanted, "{class:?} on {ground:?}: {color:?}");
            }
            let hue = |class: Class| lch_of(kinds[at(class)]).h;
            let (inserted, deleted) = (hue(Class::Inserted), hue(Class::Deleted));
            assert!(apart(inserted, 145.0) < 12.0 && apart(deleted, 25.0) < 12.0, "{inserted} {deleted}");
            if lch_of(accent).c < NEUTRAL {
                for class in [Class::Keyword, Class::Type, Class::String, Class::Number, Class::Function] {
                    assert!(lch_of(kinds[at(class)]).c < 0.02, "{class:?} is a gray on a neutral accent");
                }
                continue;
            }
            assert!(apart(hue(Class::Keyword), lch_of(accent).h) < 8.0, "keywords are the accent");
            assert_eq!(kinds[at(Class::Tag)], kinds[at(Class::Keyword)]);
            assert_eq!(kinds[at(Class::Attribute)], kinds[at(Class::Function)]);
            let second = lch_of(second.unwrap());
            if apart(second.h, lch_of(accent).h) >= 45.0 {
                assert!(apart(hue(Class::Number), second.h) < 8.0, "numbers are accent-2");
            }
            // Strings, types, and functions stand apart from the keywords and from each other.
            for (x, y) in
                [(Class::String, Class::Keyword), (Class::Type, Class::String), (Class::Function, Class::Type)]
            {
                assert!(apart(hue(x), hue(y)) >= 30.0, "{x:?} and {y:?}");
            }
        }
    }

    /// A photo turns a theme's code colors with its others (ADR-0029): a color the theme names for
    /// code reads on the surfaces as body text does, and one written out turns with the accent, then
    /// reads; the photo taken again changes nothing more.
    #[test]
    fn a_photo_turns_the_code_colors_and_each_reads() {
        let mut theme: Value = serde_json::from_str(DUSK).unwrap();
        theme["code"] = json!({ "syntax": { "comment": { "color": "line" }, "string": { "color": "#2E8B57" } } });
        let (rgba, w, h) = photo(200, &[(BLUE, 120), (ORANGE, 60), (GRAY, 20)]);
        let read = read(&rgba, w, h);
        let set = super::theme(&theme, &read).unwrap();
        let get = |path: &str| set.iter().find(|s| s.path == path).unwrap_or_else(|| panic!("{path}: {set:?}"));
        // The rule color, which code's comments are set in, now reads as body text does.
        let [ratio, needs] = get("/tokens/color/line").reads.unwrap();
        assert!(needs == BODY && ratio >= BODY, "{ratio} {needs}");
        let string = get("/code/syntax/string/color");
        assert_eq!(string.was, json!("#2E8B57"));
        let [ratio, needs] = string.reads.unwrap();
        assert!(needs == BODY && ratio >= BODY, "{string:?}");
        let accent = get("/tokens/color/accent");
        let turned = lch_of(rgb(accent.now.as_str().unwrap())).h - lch_of(rgb(accent.was.as_str().unwrap())).h;
        let (was, now) = (lch_of(rgb("#2E8B57")), lch_of(rgb(string.now.as_str().unwrap())));
        assert!(apart(now.h, (was.h + turned).rem_euclid(360.0)) < 3.0, "{was:?} → {now:?}, turned {turned}");
        let mut edited = theme.clone();
        crate::patch::apply(&mut edited, &ops(&set)).unwrap();
        assert_eq!(super::ops(&super::theme(&edited, &read).unwrap()), Vec::<Value>::new());
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
