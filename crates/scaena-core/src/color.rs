//! Colors as a theme writes them (SPEC §3.6): `#rrggbb[aa]`, `oklch(L C H [/ A])`, and
//! `oklab(L a b [/ A])`, read as CSS Color 4 reads them, to the sRGB bytes a display list
//! carries (SPEC §6). Oklab goes to sRGB through `libm`, so a color is the same bytes on
//! every platform (SPEC §13). A color outside sRGB is clipped, channel by channel.

use crate::displaylist::Color;

/// `literal` as sRGB bytes, or why it is not a color.
pub fn parse(literal: &str) -> Result<Color, String> {
    let bad = |why: &str| format!("`{literal}` is not a color: {why}");
    let s = literal.trim();
    if s.starts_with('#') {
        return Color::from_hex(s).map_err(|_| bad("a hex color is #rrggbb or #rrggbbaa"));
    }
    let (lch, body) = match (s.strip_prefix("oklch("), s.strip_prefix("oklab(")) {
        (Some(body), _) => (true, body),
        (_, Some(body)) => (false, body),
        _ => return Err(bad("expected #rrggbb[aa], oklch(…), or oklab(…)")),
    };
    let body = body.strip_suffix(')').ok_or_else(|| bad("no closing `)`"))?;
    let (channels, alpha) = match body.split_once('/') {
        Some((channels, alpha)) => (channels, Some(alpha.trim())),
        None => (body, None),
    };
    let parts: Vec<&str> = channels.split_whitespace().collect();
    let [l, x, y] = parts[..] else { return Err(bad("expected three channels, separated by spaces")) };
    // Percentages: lightness and alpha scale to 1; chroma and a, b to 0.4 (CSS Color 4).
    let l = channel(l, 1.0).ok_or_else(|| bad("lightness is a number or a percentage"))?.clamp(0.0, 1.0);
    let (a, b) = if lch {
        let c = channel(x, 0.4).ok_or_else(|| bad("chroma is a number or a percentage"))?.max(0.0);
        let h = hue(y).ok_or_else(|| bad("hue is a number of degrees, or an angle in deg, rad, grad, or turn"))?;
        let h = h.to_radians();
        (c * libm::cos(h), c * libm::sin(h))
    } else {
        let a = channel(x, 0.4).ok_or_else(|| bad("a is a number or a percentage"))?;
        let b = channel(y, 0.4).ok_or_else(|| bad("b is a number or a percentage"))?;
        (a, b)
    };
    let alpha = match alpha {
        Some(alpha) => channel(alpha, 1.0).ok_or_else(|| bad("alpha is a number or a percentage"))?.clamp(0.0, 1.0),
        None => 1.0,
    };
    let [r, g, b] = oklab_to_linear([l, a, b]).map(encode);
    Ok(Color([r, g, b, (alpha * 255.0).round() as u8]))
}

/// A number, `none` (zero), or a percentage of `full`.
fn channel(v: &str, full: f64) -> Option<f64> {
    if v == "none" {
        return Some(0.0);
    }
    let n = match v.strip_suffix('%') {
        Some(p) => p.parse::<f64>().ok()? / 100.0 * full,
        None => v.parse::<f64>().ok()?,
    };
    n.is_finite().then_some(n)
}

/// A hue in degrees: a number, `none`, or an angle with its unit.
fn hue(v: &str) -> Option<f64> {
    if v == "none" {
        return Some(0.0);
    }
    let units = [("deg", 1.0), ("grad", 0.9), ("rad", 180.0 / std::f64::consts::PI), ("turn", 360.0)];
    let (n, scale) = units.iter().find_map(|(unit, scale)| Some((v.strip_suffix(unit)?, *scale))).unwrap_or((v, 1.0));
    let h = n.parse::<f64>().ok()? * scale;
    h.is_finite().then_some(h)
}

/// Oklab to linear sRGB (Ottosson).
fn oklab_to_linear([l, a, b]: [f64; 3]) -> [f64; 3] {
    let cube = |v: f64| v * v * v;
    let lms = [
        cube(l + 0.396_337_777_4 * a + 0.215_803_757_3 * b),
        cube(l - 0.105_561_345_8 * a - 0.063_854_172_8 * b),
        cube(l - 0.089_484_177_5 * a - 1.291_485_548_0 * b),
    ];
    let [l, m, s] = lms;
    [
        4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
        -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
        -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s,
    ]
}

/// Linear light, clipped to sRGB, to the nearest sRGB byte.
fn encode(v: f64) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let v = if v <= 0.003_130_8 { 12.92 * v } else { 1.055 * libm::pow(v, 1.0 / 2.4) - 0.055 };
    (v * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::parse;

    fn hex(literal: &str) -> String {
        parse(literal).unwrap().to_hex()
    }

    #[test]
    fn oklch_and_oklab_reach_srgb_as_css_does() {
        // CSS Color 4's own examples: sRGB red, white, black, and a mid gray.
        assert_eq!(hex("oklch(62.8% 0.2577 29.23)"), "#FF0000FF");
        assert_eq!(hex("oklab(0.62796 0.22486 0.12585)"), "#FF0000FF");
        assert_eq!(hex("oklch(100% 0 0)"), "#FFFFFFFF");
        assert_eq!(hex("oklch(0 0 none)"), "#000000FF");
        assert_eq!(hex("oklch(59.987% 0 0)"), "#808080FF");
        // Units and alpha.
        assert_eq!(hex("oklch(62.8% 0.2577 29.23deg / 50%)"), "#FF000080");
        assert_eq!(hex("oklch(0.628 64.4% 0.0812turn / 0.5)"), "#FF000080");
        assert_eq!(hex("#0F766E"), "#0F766EFF");
    }

    #[test]
    fn a_color_outside_srgb_is_clipped() {
        // Linear sRGB (-0.27, 1.25, -0.12): every channel out of range.
        assert_eq!(hex("oklch(90% 0.4 145)"), "#00FF00FF");
    }

    #[test]
    fn what_is_not_a_color_says_why() {
        assert!(parse("rgb(1 2 3)").unwrap_err().contains("oklch"));
        assert!(parse("oklch(1 2)").unwrap_err().contains("three channels"));
        assert!(parse("oklch(50% x 20)").unwrap_err().contains("chroma"));
        assert!(parse("oklch(50% 0.1 20").unwrap_err().contains("closing"));
        assert!(parse("#12345").unwrap_err().contains("#rrggbb"));
    }
}
