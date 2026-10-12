//! What Narrato's TypeScript means by its string and number primitives, so the port reads the
//! same input the same way: JavaScript's `\s` and `trim`, a `slice` counted in UTF-16 units,
//! `Math.round`, `Math.min` and `Math.max`, `toFixed`, and `Math.exp` and `Math.tanh` as V8
//! computes them, which are fdlibm's, the same bits on every platform.

/// Whether `c` is whitespace as JavaScript's `\s` and `trim` read it: the Unicode spaces and
/// line terminators, and the byte-order mark, but not U+0085.
pub fn is_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
    )
}

/// JavaScript's `\s` as a class regex-lite reads, whose own `\s` is ASCII's alone.
pub const SPACE: &str =
    r"[\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]";

/// `text.trim()`.
pub fn trim(text: &str) -> &str {
    text.trim_matches(is_space)
}

/// `text.slice(0, n)`: the first `n` UTF-16 units, short of a pair it would split.
pub fn first_units(text: &str, n: usize) -> String {
    let mut units = 0;
    let mut out = String::new();
    for c in text.chars() {
        units += c.len_utf16();
        if units > n {
            break;
        }
        out.push(c);
    }
    out
}

/// `text.replace(/\s+/g, " ").trim()`.
pub fn collapse_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for c in text.chars() {
        if is_space(c) {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    out
}

/// `Math.round(x)`: the nearest integer, halves toward +∞, and −0 for a negative that rounds
/// to zero.
pub fn round(x: f64) -> f64 {
    let floor = x.floor();
    // Exact: `floor` is within 1 of `x`, or `x` lies in (−1, 0) and either side of −½ reads.
    let rounded = if x - floor >= 0.5 { floor + 1.0 } else { floor };
    if rounded == 0.0 && x.is_sign_negative() { -0.0 } else { rounded }
}

/// `Math.min(a, b)`: NaN where either is, −0 below +0.
pub fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b || (a == b && a.is_sign_negative()) {
        a
    } else {
        b
    }
}

/// `Math.max(a, b)`: NaN where either is, +0 above −0.
pub fn max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b || (a == b && a.is_sign_positive()) {
        a
    } else {
        b
    }
}

/// `Math.max(lo, Math.min(hi, x))`.
pub fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    max(lo, min(hi, x))
}

/// `x.toFixed(digits)`, for up to 10 digits: the exact value rounded to that many decimals, a
/// tie to the larger, signed where it is below zero, even where it reads as zero.
pub fn to_fixed(x: f64, digits: u32) -> String {
    debug_assert!(digits <= 10);
    if x.is_nan() {
        return "NaN".into();
    }
    let sign = if x < 0.0 { "-" } else { "" };
    let x = x.abs();
    if x.is_infinite() {
        return format!("{sign}Infinity");
    }
    if x >= 1e21 {
        // `ToString(x)`: its shortest digits, the exponent signed.
        return format!("{sign}{}", format!("{x:e}").replacen('e', "e+", 1));
    }
    // x = m × 2^e, exactly.
    let bits = x.to_bits();
    let (biased, fraction) = (((bits >> 52) & 0x7FF) as i32, bits & ((1 << 52) - 1));
    let (m, e) = if biased == 0 { (fraction, -1074) } else { (fraction | (1 << 52), biased - 1075) };
    let scale = 10u128.pow(digits);
    let n = if e >= 0 {
        ((m as u128) << e) * scale
    } else {
        // m × 10^digits < 2^87: past a shift of 120 it rounds to 0.
        let shift = e.unsigned_abs();
        if shift > 120 { 0 } else { ((m as u128) * scale + (1u128 << (shift - 1))) >> shift }
    };
    let mut figures = n.to_string();
    if digits == 0 {
        return format!("{sign}{figures}");
    }
    let width = digits as usize + 1;
    if figures.len() < width {
        figures.insert_str(0, &"0".repeat(width - figures.len()));
    }
    let (whole, part) = figures.split_at(figures.len() - digits as usize);
    format!("{sign}{whole}.{part}")
}

/// The high 32 bits of `x`, as fdlibm reads them.
fn high(x: f64) -> u32 {
    (x.to_bits() >> 32) as u32
}

/// The double whose high 32 bits are `word` and low 32 bits zero.
fn from_high(word: u32) -> f64 {
    f64::from_bits((word as u64) << 32)
}

// fdlibm's constants, by their bits as fdlibm lists them beside each.
/// Past this, `exp` overflows: 709.78….
const O_THRESHOLD: f64 = f64::from_bits(0x4086_2E42_FEFA_39EF);
/// Below this, `exp` underflows: −745.13….
const U_THRESHOLD: f64 = f64::from_bits(0xC087_4910_D52D_3051);
/// ln 2 in two parts, the high one exact in a product with any reduction's `k`.
const LN2_HI: f64 = f64::from_bits(0x3FE6_2E42_FEE0_0000);
const LN2_LO: f64 = f64::from_bits(0x3DEA_39EF_3579_3C76);
/// 1 ÷ ln 2.
const INV_LN2: f64 = f64::from_bits(0x3FF7_1547_652B_82FE);
const HUGE: f64 = 1.0e300;
const TINY: f64 = 1.0e-300;
/// 2^−1000.
const TWO_M1000: f64 = f64::from_bits(0x0170_0000_0000_0000);
/// 2^1023.
const TWO_1023: f64 = f64::from_bits(0x7FE0_0000_0000_0000);

/// `Math.exp(x)`, as V8 computes it (`base::ieee754::exp`, fdlibm's `e_exp.c`, `exp(1)` read as
/// `Math.E`).
pub fn exp(x: f64) -> f64 {
    const HALF: [f64; 2] = [0.5, -0.5];
    // The remez polynomial on [0, 0.34658].
    const P1: f64 = f64::from_bits(0x3FC5_5555_5555_553E);
    const P2: f64 = f64::from_bits(0xBF66_C16C_16BE_BD93);
    const P3: f64 = f64::from_bits(0x3F11_566A_AF25_DE2C);
    const P4: f64 = f64::from_bits(0xBEBB_BD41_C5D2_6BF1);
    const P5: f64 = f64::from_bits(0x3E66_3769_72BE_A4D0);

    let mut x = x;
    let word = high(x);
    let sign = (word >> 31) as usize;
    let hx = word & 0x7FFF_FFFF;
    if hx >= 0x4086_2E42 {
        // |x| ≥ 709.78…
        if hx >= 0x7FF0_0000 {
            let low = x.to_bits() as u32;
            if ((hx & 0xF_FFFF) | low) != 0 {
                return x + x;
            }
            return if sign == 0 { x } else { 0.0 };
        }
        if x > O_THRESHOLD {
            return HUGE * HUGE;
        }
        if x < U_THRESHOLD {
            return TWO_M1000 * TWO_M1000;
        }
    }
    let (mut hi, mut lo, mut k) = (0.0, 0.0, 0i32);
    if hx > 0x3FD6_2E42 {
        // |x| > ½ ln 2: reduced by k ln 2.
        if hx < 0x3FF0_A2B2 {
            if x == 1.0 {
                return std::f64::consts::E;
            }
            (hi, lo) = if sign == 0 { (x - LN2_HI, LN2_LO) } else { (x + LN2_HI, -LN2_LO) };
            k = 1 - 2 * sign as i32;
        } else {
            k = (INV_LN2 * x + HALF[sign]) as i32;
            let t = k as f64;
            hi = x - t * LN2_HI;
            lo = t * LN2_LO;
        }
        x = hi - lo;
    } else if hx < 0x3E30_0000 {
        // |x| < 2^−28.
        if HUGE + x > 1.0 {
            return 1.0 + x;
        }
    }
    let t = x * x;
    let two_pk = if k >= -1021 {
        from_high((0x3FF0_0000 + (k << 20)) as u32)
    } else {
        from_high((0x3FF0_0000 + ((k + 1000) << 20)) as u32)
    };
    let c = x - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
    if k == 0 {
        return 1.0 - ((x * c) / (c - 2.0) - x);
    }
    let y = 1.0 - ((lo - (x * c) / (2.0 - c)) - hi);
    if k >= -1021 {
        if k == 1024 {
            return y * 2.0 * TWO_1023;
        }
        y * two_pk
    } else {
        y * two_pk * TWO_M1000
    }
}

/// `Math.expm1(x)`, as V8 computes it (fdlibm's `s_expm1.c`): what [`tanh`] is made of.
fn expm1(x: f64) -> f64 {
    // The scaled polynomial of R(2z), z = x²/2.
    const Q1: f64 = f64::from_bits(0xBFA1_1111_1111_10F4);
    const Q2: f64 = f64::from_bits(0x3F5A_01A0_19FE_5585);
    const Q3: f64 = f64::from_bits(0xBF14_CE19_9EAA_DBB7);
    const Q4: f64 = f64::from_bits(0x3ED0_CFCA_86E6_5239);
    const Q5: f64 = f64::from_bits(0xBE8A_FDB7_6E09_C32D);

    let mut x = x;
    let word = high(x);
    let negative = word & 0x8000_0000 != 0;
    let hx = word & 0x7FFF_FFFF;
    if hx >= 0x4043_687A {
        // |x| ≥ 56 ln 2.
        if hx >= 0x4086_2E42 {
            if hx >= 0x7FF0_0000 {
                let low = x.to_bits() as u32;
                if ((hx & 0xF_FFFF) | low) != 0 {
                    return x + x;
                }
                return if negative { -1.0 } else { x };
            }
            if x > O_THRESHOLD {
                return HUGE * HUGE;
            }
        }
        if negative && x + TINY < 0.0 {
            return TINY - 1.0;
        }
    }
    let (k, c);
    if hx > 0x3FD6_2E42 {
        // |x| > ½ ln 2: reduced by k ln 2.
        let (hi, lo);
        if hx < 0x3FF0_A2B2 {
            (hi, lo, k) = if negative { (x + LN2_HI, -LN2_LO, -1) } else { (x - LN2_HI, LN2_LO, 1) };
        } else {
            k = (INV_LN2 * x + if negative { -0.5 } else { 0.5 }) as i32;
            let t = k as f64;
            hi = x - t * LN2_HI;
            lo = t * LN2_LO;
        }
        x = hi - lo;
        c = (hi - x) - lo;
    } else if hx < 0x3C90_0000 {
        // |x| < 2^−54.
        let t = HUGE + x;
        return x - (t - (HUGE + x));
    } else {
        (k, c) = (0, 0.0);
    }
    let hfx = 0.5 * x;
    let hxs = x * hfx;
    let r1 = 1.0 + hxs * (Q1 + hxs * (Q2 + hxs * (Q3 + hxs * (Q4 + hxs * Q5))));
    let t = 3.0 - r1 * hfx;
    let mut e = hxs * ((r1 - t) / (6.0 - x * t));
    if k == 0 {
        return x - (x * e - hxs);
    }
    let two_pk = from_high((0x3FF0_0000 + (k << 20)) as u32);
    e = x * (e - c) - c;
    e -= hxs;
    if k == -1 {
        return 0.5 * (x - e) - 0.5;
    }
    if k == 1 {
        return if x < -0.25 { -2.0 * (e - (x + 0.5)) } else { 1.0 + 2.0 * (x - e) };
    }
    if k <= -2 || k > 56 {
        let y = 1.0 - (e - x);
        let y = if k == 1024 { y * 2.0 * TWO_1023 } else { y * two_pk };
        return y - 1.0;
    }
    if k < 20 {
        let t = from_high(0x3FF0_0000 - (0x20_0000 >> k));
        (t - (e - x)) * two_pk
    } else {
        let t = from_high(((0x3FF - k) << 20) as u32);
        ((x - (e + t)) + 1.0) * two_pk
    }
}

/// `Math.tanh(x)`, as V8 computes it (`base::ieee754::tanh`, fdlibm's `s_tanh.c`).
pub fn tanh(x: f64) -> f64 {
    let word = high(x);
    let positive = word & 0x8000_0000 == 0;
    let ix = word & 0x7FFF_FFFF;
    if ix >= 0x7FF0_0000 {
        // ±∞, or NaN.
        return if positive { 1.0 / x + 1.0 } else { 1.0 / x - 1.0 };
    }
    let z = if ix < 0x4036_0000 {
        // |x| < 22.
        if ix < 0x3E30_0000 && HUGE + x > 1.0 {
            // |x| < 2^−28.
            return x;
        }
        if ix >= 0x3FF0_0000 {
            let t = expm1(2.0 * x.abs());
            1.0 - 2.0 / (t + 2.0)
        } else {
            let t = expm1(-2.0 * x.abs());
            -t / (t + 2.0)
        }
    } else {
        1.0 - TINY
    };
    if positive { z } else { -z }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_and_numbers_read_as_javascript_reads_them() {
        assert_eq!(trim("\u{FEFF} a b \u{3000}"), "a b");
        assert_eq!(trim("\u{85}a"), "\u{85}a");
        assert_eq!(first_units("a😀b", 2), "a");
        assert_eq!(first_units("a😀b", 3), "a😀");
        assert_eq!(collapse_spaces("  ###\n\t Instruction  "), "### Instruction");
        assert_eq!((round(2.5), round(-2.5), round(83.49)), (3.0, -2.0, 83.0));
    }

    #[test]
    fn rounding_and_bounds_are_javascripts_to_the_last_bit() {
        // `x + 0.5` rounds these up before the floor would.
        assert_eq!(round(0.499_999_999_999_999_94), 0.0);
        assert_eq!(round(4_503_599_627_370_497.0), 4_503_599_627_370_497.0);
        assert!(round(-0.4).is_sign_negative() && round(-0.5).is_sign_negative());
        assert!(round(f64::NAN).is_nan());
        assert!(max(0.0, -0.0).is_sign_positive() && min(0.0, -0.0).is_sign_negative());
        assert!(max(f64::NAN, 1.0).is_nan() && min(1.0, f64::NAN).is_nan());
        assert!(clamp(-0.0, 0.0, 1.0).is_sign_positive());
        assert_eq!((clamp(1.5, 0.0, 1.0), clamp(-3.0, -1.0, 1.0)), (1.0, -1.0));
    }

    #[test]
    fn fixed_figures_round_the_exact_value_as_to_fixed_does() {
        // 0.125 is a tie, taken up; 1.005 is just below one, and 2.675 too.
        let cases = [
            (0.125, "0.13"),
            (1.005, "1.00"),
            (2.675, "2.67"),
            (0.615, "0.61"),
            (-0.125, "-0.13"),
            (-0.001, "-0.00"),
            (-0.0, "0.00"),
            (0.0, "0.00"),
            (0.5, "0.50"),
            (99.995, "100.00"),
            (1e-300, "0.00"),
            (123_456.789, "123456.79"),
            (1e21, "1e+21"),
            (f64::NAN, "NaN"),
            (f64::NEG_INFINITY, "-Infinity"),
        ];
        for (x, want) in cases {
            assert_eq!(to_fixed(x, 2), want, "{x}");
        }
        assert_eq!((to_fixed(2.5, 0), to_fixed(0.05, 1)), ("3".to_string(), "0.1".to_string()));
    }

    /// Node's own values, as bits.
    #[test]
    fn exp_and_tanh_are_v8s_to_the_last_bit() {
        let exps: [(f64, u64); 8] = [
            (1.0, 0x4005_BF0A_8B14_5769),
            (-0.5, 0x3FE3_68B2_FC6F_960A),
            (0.3, 0x3FF5_9905_8C8C_1A96),
            (-3.2, 0x3FA4_DEC8_99FF_FB46),
            (10.0, 0x40D5_829D_CF95_0560),
            (-745.2, 0x0),
            (709.8, 0x7FF0_0000_0000_0000),
            (2e-30, 0x3FF0_0000_0000_0000),
        ];
        for (x, bits) in exps {
            assert_eq!(exp(x).to_bits(), bits, "exp({x})");
        }
        let tanhs: [(f64, u64); 5] = [
            (0.7, 0x3FE3_56FB_17AF_2E92),
            (-1.3, 0xBFEB_933C_726E_9B3A),
            (3.0, 0x3FEF_D77D_111A_0B00),
            (25.0, 0x3FF0_0000_0000_0000),
            (1e-9, 0x3E11_2E0B_E826_D695),
        ];
        for (x, bits) in tanhs {
            assert_eq!(tanh(x).to_bits(), bits, "tanh({x})");
        }
        assert!(exp(f64::NAN).is_nan() && tanh(f64::NAN).is_nan());
        assert_eq!((exp(f64::NEG_INFINITY), tanh(f64::NEG_INFINITY)), (0.0, -1.0));
    }
}
