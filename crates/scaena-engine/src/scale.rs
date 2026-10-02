//! Scales (SPEC §3.7): continuous values to positions, and the "nice" domains and ticks
//! d3 makes for them. Only `+ − × ÷`, `floor`, `ceil`, and `round` touch a value, with
//! powers of ten from a table of exact literals and magnitudes read from Rust's float
//! formatting, so an axis lands on the same bits on every platform (SPEC §13), which a
//! platform `log10` or `pow` would not promise.

use scaena_core::format::exponent_of;

/// √50, √10, √2: where d3 rounds a tick step up to 10, 5, and 2 times its magnitude.
const E10: f64 = 7.071_067_811_865_475_5;
const E5: f64 = 3.162_277_660_168_379_5;
const E2: f64 = std::f64::consts::SQRT_2;

/// 10^e: exact for 0 ≤ e ≤ 22, and the correctly rounded literal for −22 ≤ e < 0.
fn pow10(e: i32) -> f64 {
    const TABLE: [f64; 45] = [
        1e-22, 1e-21, 1e-20, 1e-19, 1e-18, 1e-17, 1e-16, 1e-15, 1e-14, 1e-13, 1e-12, 1e-11, 1e-10, 1e-9, 1e-8, 1e-7,
        1e-6, 1e-5, 1e-4, 1e-3, 1e-2, 1e-1, 1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13,
        1e14, 1e15, 1e16, 1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
    ];
    TABLE[(e.clamp(-22, 22) + 22) as usize]
}

/// d3's `tickSpec`: ticks are `i × inc` for `i` in `i1..=i2`, or `i / −inc` when `inc`
/// is negative (a step below 1, kept as its exact reciprocal so `0.1 × 3` reads `0.3`).
fn tick_spec(start: f64, stop: f64, count: f64) -> (f64, f64, f64) {
    let step = (stop - start) / count.max(0.0);
    let power = exponent_of(step);
    let error = step / pow10(power);
    let factor = if error >= E10 {
        10.0
    } else if error >= E5 {
        5.0
    } else if error >= E2 {
        2.0
    } else {
        1.0
    };
    let (mut i1, mut i2, inc);
    if power < 0 {
        let k = pow10(-power) / factor;
        i1 = (start * k).round();
        i2 = (stop * k).round();
        if i1 / k < start {
            i1 += 1.0;
        }
        if i2 / k > stop {
            i2 -= 1.0;
        }
        inc = -k;
    } else {
        let k = pow10(power) * factor;
        i1 = (start / k).round();
        i2 = (stop / k).round();
        if i1 * k < start {
            i1 += 1.0;
        }
        if i2 * k > stop {
            i2 -= 1.0;
        }
        inc = k;
    }
    if i2 < i1 && (0.5..2.0).contains(&count) {
        return tick_spec(start, stop, count * 2.0);
    }
    (i1, i2, inc)
}

/// About `count` round values from `start` to `stop` (d3's `ticks`), in order.
pub fn ticks(start: f64, stop: f64, count: usize) -> Vec<f64> {
    if count == 0 || !start.is_finite() || !stop.is_finite() {
        return Vec::new();
    }
    if start == stop {
        return vec![start];
    }
    let (lo, hi) = if stop < start { (stop, start) } else { (start, stop) };
    let (i1, i2, inc) = tick_spec(lo, hi, count as f64);
    if i2 < i1 {
        return Vec::new();
    }
    let mut out: Vec<f64> = (0..=(i2 - i1) as i64)
        .map(|i| {
            let i = i1 + i as f64;
            if inc < 0.0 { i / -inc } else { i * inc }
        })
        .collect();
    if stop < start {
        out.reverse();
    }
    out
}

/// The distance between the ticks [`ticks`] makes (d3's `tickStep`).
pub fn tick_step(start: f64, stop: f64, count: usize) -> f64 {
    let (lo, hi) = if stop < start { (stop, start) } else { (start, stop) };
    let inc = tick_spec(lo, hi, count as f64).2;
    let step = if inc < 0.0 { 1.0 / -inc } else { inc };
    if stop < start { -step } else { step }
}

/// `[lo, hi]` widened to round values at the tick step (d3's `nice`), each side only
/// when `widen` says so: a bound the author set stays.
pub fn nice(mut lo: f64, mut hi: f64, count: usize, widen: [bool; 2]) -> (f64, f64) {
    // Also no range when either end is NaN.
    if lo.partial_cmp(&hi) != Some(std::cmp::Ordering::Less) || count == 0 {
        return (lo, hi);
    }
    let mut previous = None;
    for _ in 0..10 {
        let inc = tick_spec(lo, hi, count as f64).2;
        if previous == Some(inc) {
            break;
        }
        if inc > 0.0 {
            if widen[0] {
                lo = (lo / inc).floor() * inc;
            }
            if widen[1] {
                hi = (hi / inc).ceil() * inc;
            }
        } else if inc < 0.0 {
            if widen[0] {
                lo = (lo * inc).ceil() / inc;
            }
            if widen[1] {
                hi = (hi * inc).floor() / inc;
            }
        } else {
            break;
        }
        previous = Some(inc);
    }
    (lo, hi)
}

/// Values in `domain` to positions in `range`, linearly: `range[0]` at `domain[0]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearScale {
    pub domain: [f64; 2],
    pub range: [f32; 2],
}

impl LinearScale {
    /// Where `v` sits; beyond the domain, on the same line. `domain[0]` lands on
    /// `range[0]` exactly and `domain[1]` on `range[1]` to within float rounding: the
    /// formula is the one Phase 0's bars were blessed with.
    pub fn map(&self, v: f64) -> f32 {
        let ([d0, d1], [r0, r1]) = (self.domain, self.range);
        (f64::from(r0) - (v - d0) / (d1 - d0) * f64::from(r0 - r1)) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_are_the_round_values_d3_picks() {
        assert_eq!(ticks(0.0, 38.0, 5), [0.0, 10.0, 20.0, 30.0]);
        assert_eq!(ticks(0.0, 1.0, 5), [0.0, 0.2, 0.4, 0.6, 0.8, 1.0]);
        assert_eq!(ticks(0.0, 0.3, 3), [0.0, 0.1, 0.2, 0.3], "0.3, not 0.30000000000000004");
        assert_eq!(ticks(-12.0, 7.0, 5), [-10.0, -5.0, 0.0, 5.0]);
        assert_eq!(ticks(1_200.0, 5_600.0, 5), [2_000.0, 3_000.0, 4_000.0, 5_000.0]);
        assert_eq!(ticks(5.0, 0.0, 5), [5.0, 4.0, 3.0, 2.0, 1.0, 0.0], "reversed domains tick backwards");
        assert_eq!(ticks(3.0, 3.0, 5), [3.0]);
        assert_eq!(tick_step(0.0, 38.0, 5), 10.0);
        assert_eq!(tick_step(0.0, 1.0, 5), 0.2);
    }

    #[test]
    fn nice_widens_to_the_tick_step_but_not_past_a_set_bound() {
        assert_eq!(nice(0.0, 38.0, 5, [true, true]), (0.0, 40.0));
        assert_eq!(nice(3.0, 97.0, 5, [true, true]), (0.0, 100.0));
        assert_eq!(nice(-0.13, 0.87, 5, [true, true]), (-0.2, 1.0));
        assert_eq!(nice(0.0, 38.0, 5, [true, false]), (0.0, 38.0), "the author's top stays");
        assert_eq!(nice(5.0, 5.0, 5, [true, true]), (5.0, 5.0));
    }

    #[test]
    fn a_linear_scale_maps_the_domain_onto_the_range() {
        let s = LinearScale { domain: [0.0, 40.0], range: [600.0, 100.0] };
        assert_eq!((s.map(0.0), s.map(40.0), s.map(20.0)), (600.0, 100.0, 350.0));
        assert_eq!(s.map(50.0), -25.0);
        assert_eq!(pow10(0), 1.0);
        assert_eq!(pow10(-1), 0.1);
    }
}
