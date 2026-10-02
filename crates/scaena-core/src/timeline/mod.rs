//! Timeline math (SPEC §2.4, §3.9): cubic-Bézier easing, analytically solved springs,
//! the looks motions give, when each motion runs within its state's cue, and the deck's
//! states end to end.
//!
//! - [`CubicBezier`] and [`Spring`] shape a motion's progress; a [`Curve`] is either.
//! - A [`Clock`] says how far along one motion is at a time; a [`Motion`] says what
//!   [`Look`] its unit has then.
//! - [`schedule`] places a state's cues ([`Item`]s: a node's own enter, exit,
//!   emphasis, and anim, and its choreography) on the state's clock, and finds its span.
//! - A [`Timeline`] lays the states end to end, with their holds.
//!
//! Everything here is pure `f64` math with a fixed evaluation order, and `libm` for
//! every transcendental function, which is what the determinism contract (SPEC §13)
//! needs. Springs report a settle time, so the global timeline (and therefore video
//! export) knows every duration up front. Nothing here knows fonts or layout: what a
//! cue splits its node into is counted by the caller, after layout (SPEC §5).

mod look;
mod schedule;

pub use look::{Clock, Curve, Key, Keys, Look, Motion};
pub use schedule::{Cue, Item, Placed, Schedule, Slot, Timeline, Units, schedule};

use serde::{Deserialize, Serialize};

/// A CSS-style cubic Bézier easing `(x1, y1, x2, y2)` from (0,0) to (1,1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CubicBezier(pub f64, pub f64, pub f64, pub f64);

impl CubicBezier {
    pub const LINEAR: CubicBezier = CubicBezier(0.0, 0.0, 1.0, 1.0);

    fn sample_x(&self, t: f64) -> f64 {
        let (x1, x2) = (self.0, self.2);
        // Bernstein form with P0 = 0, P3 = 1.
        3.0 * (1.0 - t) * (1.0 - t) * t * x1 + 3.0 * (1.0 - t) * t * t * x2 + t * t * t
    }

    fn sample_y(&self, t: f64) -> f64 {
        let (y1, y2) = (self.1, self.3);
        3.0 * (1.0 - t) * (1.0 - t) * t * y1 + 3.0 * (1.0 - t) * t * t * y2 + t * t * t
    }

    fn sample_dx(&self, t: f64) -> f64 {
        let (x1, x2) = (self.0, self.2);
        3.0 * (1.0 - t) * (1.0 - t) * x1 + 6.0 * (1.0 - t) * t * (x2 - x1) + 3.0 * t * t * (1.0 - x2)
    }

    /// Eased progress for `x` in `[0, 1]` (clamped). Newton iterations with a
    /// bisection fallback; accurate to ~1e-7.
    pub fn ease(&self, x: f64) -> f64 {
        let x = x.clamp(0.0, 1.0);
        if x == 0.0 || x == 1.0 {
            return x;
        }
        let mut t = x;
        for _ in 0..8 {
            let dx = self.sample_dx(t);
            if dx.abs() < 1e-6 {
                break;
            }
            let err = self.sample_x(t) - x;
            if err.abs() < 1e-7 {
                return self.sample_y(t);
            }
            t -= err / dx;
        }
        // Bisection fallback.
        let (mut lo, mut hi) = (0.0, 1.0);
        t = x;
        for _ in 0..32 {
            let xt = self.sample_x(t);
            if (xt - x).abs() < 1e-7 {
                break;
            }
            if xt < x {
                lo = t;
            } else {
                hi = t;
            }
            t = 0.5 * (lo + hi);
        }
        self.sample_y(t)
    }
}

/// Damped harmonic oscillator parameters (SPEC §3.6 `motion.springs`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Spring {
    pub stiffness: f64,
    pub damping: f64,
    #[serde(default = "one")]
    pub mass: f64,
}

fn one() -> f64 {
    1.0
}

/// Default settle thresholds: position within 0.1% of the target and velocity
/// below 0.1%/s, in normalized units.
pub const SETTLE_EPSILON: f64 = 0.001;

impl Spring {
    /// Normalized position at time `t` seconds, moving from 0 to 1 with initial
    /// velocity `v0` (units of 1/s). Closed-form solution for all damping regimes, with
    /// `libm`'s exp, sin, and cos, so every platform gets the same bits (SPEC §13).
    pub fn position(&self, t: f64, v0: f64) -> f64 {
        if t <= 0.0 {
            return 0.0;
        }
        let m = self.mass.max(1e-9);
        let k = self.stiffness.max(1e-9);
        let c = self.damping.max(0.0);
        let w0 = (k / m).sqrt();
        let zeta = c / (2.0 * (k * m).sqrt());
        // Displacement from the target, starting at -1 with velocity v0.
        let x0 = -1.0;
        let x = if zeta < 1.0 {
            let wd = w0 * (1.0 - zeta * zeta).sqrt();
            let a = x0;
            let b = (v0 + zeta * w0 * x0) / wd;
            libm::exp(-zeta * w0 * t) * (a * libm::cos(wd * t) + b * libm::sin(wd * t))
        } else if (zeta - 1.0).abs() < 1e-9 {
            let a = x0;
            let b = v0 + w0 * x0;
            (a + b * t) * libm::exp(-w0 * t)
        } else {
            let s = w0 * (zeta * zeta - 1.0).sqrt();
            let r1 = -zeta * w0 + s;
            let r2 = -zeta * w0 - s;
            let b = (v0 - r1 * x0) / (r2 - r1);
            let a = x0 - b;
            a * libm::exp(r1 * t) + b * libm::exp(r2 * t)
        };
        1.0 + x
    }

    /// The spring's answer to a tap, `t` seconds after it: how far it swings from rest,
    /// as a share of the farthest it gets, which it reaches once. Under-damped, it then
    /// swings past rest and back as it settles. The derivative of [`Spring::position`],
    /// solved in closed form for each damping regime and scaled to peak at 1, with
    /// `libm` throughout (SPEC §13).
    pub fn impulse(&self, t: f64) -> f64 {
        if t <= 0.0 {
            return 0.0;
        }
        let m = self.mass.max(1e-9);
        let k = self.stiffness.max(1e-9);
        let c = self.damping.max(0.0);
        let w0 = (k / m).sqrt();
        let zeta = c / (2.0 * (k * m).sqrt());
        if zeta < 1.0 && (zeta - 1.0).abs() >= 1e-9 {
            // e^(−ζω₀t)·sin(ω_d t), which peaks where tan(ω_d t) = ω_d / ζω₀.
            let wd = w0 * (1.0 - zeta * zeta).sqrt();
            let peak = libm::atan2(wd, zeta * w0) / wd;
            libm::exp(-zeta * w0 * (t - peak)) * libm::sin(wd * t) / libm::sin(wd * peak)
        } else if (zeta - 1.0).abs() < 1e-9 {
            // t·e^(−ω₀t), which peaks at 1/ω₀.
            w0 * t * libm::exp(1.0 - w0 * t)
        } else {
            // e^(r₁t) − e^(r₂t), which peaks where r₁e^(r₁t) = r₂e^(r₂t).
            let s = w0 * (zeta * zeta - 1.0).sqrt();
            let (r1, r2) = (-zeta * w0 + s, -zeta * w0 - s);
            let peak = libm::log(r2 / r1) / (r1 - r2);
            let at = |t: f64| libm::exp(r1 * t) - libm::exp(r2 * t);
            at(t) / at(peak)
        }
    }

    /// Velocity at time `t` seconds (numerical derivative; adequate for settle checks).
    pub fn velocity(&self, t: f64, v0: f64) -> f64 {
        let h = 1e-4;
        (self.position(t + h, v0) - self.position((t - h).max(0.0), v0)) / (2.0 * h)
    }

    /// Time in seconds until the spring stays within [`SETTLE_EPSILON`] of the
    /// target with negligible velocity. Scanned at 1 ms resolution, capped at 10 s.
    pub fn settle_time(&self, v0: f64) -> f64 {
        let step = 0.001;
        let mut t = 0.0;
        let mut quiet_since: Option<f64> = None;
        while t < 10.0 {
            let x = self.position(t, v0);
            let v = self.velocity(t, v0);
            let quiet = (1.0 - x).abs() < SETTLE_EPSILON && v.abs() < SETTLE_EPSILON * 10.0;
            match (quiet, quiet_since) {
                (true, None) => quiet_since = Some(t),
                (true, Some(since)) if t - since >= 0.05 => return since,
                (false, _) => quiet_since = None,
                _ => {}
            }
            t += step;
        }
        10.0
    }
}

/// Linear interpolation helper with a fixed evaluation order.
#[inline]
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bezier_endpoints_and_monotonic() {
        let e = CubicBezier(0.2, 0.0, 0.0, 1.0);
        assert_eq!(e.ease(0.0), 0.0);
        assert_eq!(e.ease(1.0), 1.0);
        let mut last = 0.0;
        for i in 1..=100 {
            let y = e.ease(i as f64 / 100.0);
            assert!(y >= last - 1e-9, "non-monotonic at {i}");
            last = y;
        }
        assert!((CubicBezier::LINEAR.ease(0.37) - 0.37).abs() < 1e-6);
    }

    #[test]
    fn bezier_ease_out_is_above_linear() {
        let out = CubicBezier(0.0, 0.0, 0.2, 1.0);
        assert!(out.ease(0.5) > 0.5);
        let inn = CubicBezier(0.4, 0.0, 1.0, 1.0);
        assert!(inn.ease(0.5) < 0.5);
    }

    #[test]
    fn spring_settles_at_target() {
        let snappy = Spring { stiffness: 420.0, damping: 34.0, mass: 1.0 };
        assert!((snappy.position(0.0, 0.0)).abs() < 1e-12);
        assert!((snappy.position(5.0, 0.0) - 1.0).abs() < 1e-6);
        let settle = snappy.settle_time(0.0);
        assert!(settle > 0.1 && settle < 1.0, "settle = {settle}");
        // Overdamped and critically damped regimes also converge (the slow root of
        // an overdamped spring decays as e^{-2.68 t} here, so give it 10 s).
        let heavy = Spring { stiffness: 100.0, damping: 40.0, mass: 1.0 };
        assert!((heavy.position(10.0, 0.0) - 1.0).abs() < 1e-6);
        assert!(heavy.settle_time(0.0) < 3.0);
        let crit = Spring { stiffness: 100.0, damping: 20.0, mass: 1.0 };
        assert!((crit.position(5.0, 0.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn underdamped_overshoots_once() {
        let s = Spring { stiffness: 420.0, damping: 20.0, mass: 1.0 };
        let max = (0..400).map(|i| s.position(i as f64 / 400.0, 0.0)).fold(0.0, f64::max);
        assert!(max > 1.0, "expected overshoot, max = {max}");
    }
}
