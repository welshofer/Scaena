//! What a motion does to a unit, and when (SPEC §3.9). A [`Clock`] says how far along a
//! motion is at a time, along its [`Curve`]; a [`Motion`] says what [`Look`] its unit has
//! then. A unit is what a cue moves: a whole node, or one of the lines, words, glyphs,
//! children, or marks its cue splits it into.

use super::{CubicBezier, Spring};
use crate::displaylist::Color;

/// How a motion moves through its time: along an easing, or as a spring.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Curve {
    Ease(CubicBezier),
    /// A spring, and the seconds it takes to settle: a motion on it lasts that long.
    Spring(Spring, f64),
}

impl Curve {
    /// A spring's curve, which lasts its settle time.
    pub fn spring(spring: Spring) -> Curve {
        Curve::Spring(spring, spring.settle_time(0.0))
    }

    /// How long a motion along this curve runs, ms: a spring's settle time, else `eased`.
    pub fn duration(&self, eased: f64) -> f64 {
        match *self {
            Curve::Ease(_) => eased,
            Curve::Spring(_, settle) => 1000.0 * settle,
        }
    }

    /// Progress `u` of the way through a motion's time: eased, or where the spring is
    /// then, which may be past 1.
    fn at(&self, u: f64) -> f64 {
        match *self {
            Curve::Ease(e) => e.ease(u),
            Curve::Spring(s, settle) => s.position(u * settle, 0.0),
        }
    }

    /// A there-and-back `u` of the way through: 0 at both ends and 1 at the peak. Eased,
    /// it goes out along the easing to the middle and back the same way; a spring
    /// answers a tap ([`Spring::impulse`]).
    fn pulse(&self, u: f64) -> f64 {
        match *self {
            Curve::Ease(e) => e.ease(1.0 - (2.0 * u - 1.0).abs()),
            Curve::Spring(s, settle) => s.impulse(u * settle),
        }
    }
}

/// When one motion runs, ms into its state's cue (the transition into the state starts
/// at 0), and along what curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clock {
    pub start: f64,
    pub duration: f64,
    pub curve: Curve,
}

impl Clock {
    pub fn end(&self) -> f64 {
        self.start + self.duration
    }

    /// How far along the motion is `t` ms into the cue: exactly 1 at or past its end (and
    /// for a NaN `t`, so a bad time shows rest), exactly 0 before that at or before its
    /// start, so one that takes no time is done when it starts; between, along its
    /// curve, which a spring carries past 1 and back.
    pub fn progress(&self, t: f64) -> f64 {
        if t.is_nan() || t >= self.end() {
            1.0
        } else if t <= self.start {
            0.0
        } else {
            self.curve.at((t - self.start) / self.duration)
        }
    }

    /// A there-and-back `t` ms into the cue: exactly 0 outside the motion's time.
    pub fn pulse(&self, t: f64) -> f64 {
        self.fraction(t).map_or(0.0, |u| self.curve.pulse(u))
    }

    /// The share of the motion's time gone at `t`, strictly inside it; `None` outside.
    fn fraction(&self, t: f64) -> Option<f64> {
        (t > self.start && t < self.end()).then(|| (t - self.start) / self.duration)
    }
}

/// A unit's look against itself at rest: `opacity` multiplies its own, `translate`
/// moves it (canvas units), and `scale` and `rotate` (degrees, clockwise) turn it about
/// `anchor`, a point given in fractions of its box (`[0.5, 1]` is its bottom middle).
/// `tint` mixes every paint it draws toward a color in Oklab, so far (0–1), and
/// `progress` is how much of each outline it strokes is drawn, from where the outline
/// starts (0–1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub opacity: f64,
    pub translate: [f64; 2],
    pub scale: [f64; 2],
    pub rotate: f64,
    pub anchor: [f64; 2],
    pub tint: Option<(Color, f64)>,
    pub progress: f64,
}

impl Default for Look {
    fn default() -> Self {
        Look::REST
    }
}

impl Look {
    /// The unit as it is laid out.
    pub const REST: Look = Look {
        opacity: 1.0,
        translate: [0.0; 2],
        scale: [1.0; 2],
        rotate: 0.0,
        anchor: [0.5; 2],
        tint: None,
        progress: 1.0,
    };

    /// `w` of the way from rest to this look. Past 1 (or below 0) the transform goes on
    /// past it, as a spring overshoots; opacity, tint, and progress stay within 0–1.
    pub fn toward(&self, w: f64) -> Look {
        let mix = |rest: f64, to: f64| rest + (to - rest) * w;
        Look {
            opacity: mix(1.0, self.opacity).clamp(0.0, 1.0),
            translate: self.translate.map(|v| mix(0.0, v)),
            scale: self.scale.map(|v| mix(1.0, v)),
            rotate: mix(0.0, self.rotate),
            anchor: self.anchor,
            tint: self.tint.map(|(color, q)| (color, mix(0.0, q).clamp(0.0, 1.0))),
            progress: mix(1.0, self.progress).clamp(0.0, 1.0),
        }
    }

    /// Whether this look leaves the unit as it is laid out.
    pub fn is_rest(&self) -> bool {
        self.opacity == 1.0
            && self.translate == [0.0; 2]
            && self.scale == [1.0; 2]
            && self.rotate == 0.0
            && self.tint.is_none_or(|(_, q)| q == 0.0)
            && self.progress == 1.0
    }

    /// The look as a map of canvas points, `[a, b, c, d, e, f]` (`x' = a·x + c·y + e`), for
    /// a unit whose box at rest is `rect` (`[x, y, width, height]`): scale, then rotate,
    /// about the anchor, then translate. The sine and cosine are `libm`'s (SPEC §13).
    pub fn affine(&self, rect: [f32; 4]) -> [f64; 6] {
        let [x, y, w, h] = rect.map(f64::from);
        let (ax, ay) = (x + self.anchor[0] * w, y + self.anchor[1] * h);
        let turn = self.rotate.to_radians();
        let (sin, cos) = if self.rotate == 0.0 { (0.0, 1.0) } else { (libm::sin(turn), libm::cos(turn)) };
        let (a, b) = (cos * self.scale[0], sin * self.scale[0]);
        let (c, d) = (-sin * self.scale[1], cos * self.scale[1]);
        let e = ax + self.translate[0] - (a * ax + c * ay);
        let f = ay + self.translate[1] - (b * ax + d * ay);
        [a, b, c, d, e, f]
    }
}

/// A keyframe: a value `t` ms after its track starts, reached from the key before along
/// `curve`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key<T> {
    pub t: f64,
    pub v: T,
    pub curve: Curve,
}

/// A unit's keyframe tracks (a node's `anim`, SPEC §3.9), one per property of its look,
/// keys in time order. Values are looks, as a preset's are: `opacity` multiplies the
/// node's own, `translate` moves it, and `scale` and `rotate` turn it about `anchor`.
#[derive(Debug, Clone, PartialEq)]
pub struct Keys {
    pub opacity: Vec<Key<f64>>,
    pub translate: Vec<Key<[f64; 2]>>,
    pub scale: Vec<Key<[f64; 2]>>,
    pub rotate: Vec<Key<f64>>,
    pub progress: Vec<Key<f64>>,
    pub anchor: [f64; 2],
}

impl Default for Keys {
    fn default() -> Self {
        Keys {
            opacity: vec![],
            translate: vec![],
            scale: vec![],
            rotate: vec![],
            progress: vec![],
            anchor: Look::REST.anchor,
        }
    }
}

/// What a keyframe value mixes as.
trait Mix: Copy {
    fn mix(self, to: Self, q: f64) -> Self;
}

impl Mix for f64 {
    fn mix(self, to: f64, q: f64) -> f64 {
        self + (to - self) * q
    }
}

impl Mix for [f64; 2] {
    fn mix(self, to: [f64; 2], q: f64) -> [f64; 2] {
        [self[0].mix(to[0], q), self[1].mix(to[1], q)]
    }
}

/// A track's value `t` ms after it starts: its first key's before it, its last key's
/// after it, and between two keys, the later one's curve from the one to the other.
/// `None` for an empty track.
fn sample<T: Mix>(keys: &[Key<T>], t: f64) -> Option<T> {
    let (first, last) = (keys.first()?, keys.last()?);
    if t <= first.t {
        return Some(first.v);
    }
    if t >= last.t {
        return Some(last.v);
    }
    let next = keys.iter().position(|k| k.t > t)?;
    let (a, b) = (&keys[next - 1], &keys[next]);
    let clock = Clock { start: a.t, duration: b.t - a.t, curve: b.curve };
    Some(a.v.mix(b.v, clock.progress(t)))
}

impl Keys {
    /// When the last key falls, ms after the tracks start.
    pub fn duration(&self) -> f64 {
        let last = |ts: &mut dyn Iterator<Item = f64>| ts.fold(0.0, f64::max);
        last(
            &mut [
                self.opacity.last().map(|k| k.t),
                self.translate.last().map(|k| k.t),
                self.scale.last().map(|k| k.t),
                self.rotate.last().map(|k| k.t),
                self.progress.last().map(|k| k.t),
            ]
            .into_iter()
            .flatten(),
        )
    }

    /// The look `t` ms after the tracks start: each property its track's value, or its
    /// rest without one.
    pub fn look(&self, t: f64) -> Look {
        Look {
            opacity: sample(&self.opacity, t).unwrap_or(1.0).clamp(0.0, 1.0),
            translate: sample(&self.translate, t).unwrap_or([0.0; 2]),
            scale: sample(&self.scale, t).unwrap_or([1.0; 2]),
            rotate: sample(&self.rotate, t).unwrap_or(0.0),
            anchor: self.anchor,
            tint: None,
            progress: sample(&self.progress, t).unwrap_or(1.0).clamp(0.0, 1.0),
        }
    }
}

/// What a cue does to its units (SPEC §3.9).
#[derive(Debug, Clone, PartialEq)]
pub enum Motion {
    /// From this look to rest. Before its clock starts, the unit is not drawn.
    Enter(Look),
    /// From rest to this look. Once its clock ends, the unit is gone.
    Exit(Look),
    /// From rest to this look and back.
    Emphasis(Look),
    /// Keyframe tracks, from the clock's start.
    Keys(Keys),
}

impl Motion {
    /// The unit's look `t` ms into its state's cue, on `clock`; `None` where it is not
    /// drawn (an entrance that has not started, an exit that has ended).
    pub fn look(&self, clock: &Clock, t: f64) -> Option<Look> {
        match self {
            Motion::Enter(from) => (t >= clock.start).then(|| from.toward(1.0 - clock.progress(t))),
            Motion::Exit(to) => (t < clock.end()).then(|| to.toward(clock.progress(t))),
            Motion::Emphasis(peak) => Some(peak.toward(clock.pulse(t))),
            Motion::Keys(keys) => Some(keys.look(t - clock.start)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUT: Curve = Curve::Ease(CubicBezier(0.0, 0.0, 0.2, 1.0));
    const LINEAR: Curve = Curve::Ease(CubicBezier::LINEAR);
    const SNAPPY: Spring = Spring { stiffness: 420.0, damping: 34.0, mass: 1.0 };

    fn rise() -> Look {
        Look { opacity: 0.0, translate: [0.0, 24.0], ..Look::REST }
    }

    #[test]
    fn a_clock_is_exactly_at_its_ends_outside_its_time() {
        let c = Clock { start: 100.0, duration: 200.0, curve: OUT };
        assert_eq!((c.progress(0.0), c.progress(100.0)), (0.0, 0.0));
        assert_eq!((c.progress(300.0), c.progress(1e9), c.progress(f64::NAN)), (1.0, 1.0, 1.0));
        assert!(c.progress(200.0) > 0.5, "ease-out is past halfway at half time");
        let sprung =
            Clock { start: 0.0, duration: Curve::spring(SNAPPY).duration(420.0), curve: Curve::spring(SNAPPY) };
        assert!(sprung.duration > 100.0 && sprung.duration < 1000.0, "a spring lasts its settle time");
        let most = (1..100).map(|i| sprung.progress(i as f64 * sprung.duration / 100.0)).fold(0.0, f64::max);
        assert!(most > 1.0, "snappy overshoots: {most}");
        let instant = Clock { start: 50.0, duration: 0.0, curve: LINEAR };
        assert_eq!((instant.progress(49.0), instant.progress(50.0)), (0.0, 1.0));
    }

    #[test]
    fn a_pulse_goes_out_and_comes_back() {
        let c = Clock { start: 0.0, duration: 400.0, curve: LINEAR };
        assert_eq!((c.pulse(0.0), c.pulse(200.0), c.pulse(400.0)), (0.0, 1.0, 0.0));
        assert_eq!(c.pulse(100.0), 0.5);
        let gentle = Curve::spring(Spring { stiffness: 170.0, damping: 26.0, mass: 1.0 });
        let s = Clock { start: 0.0, duration: gentle.duration(0.0), curve: gentle };
        let samples: Vec<f64> = (0..=200).map(|i| s.pulse(i as f64 * s.duration / 200.0)).collect();
        let peak = samples.iter().copied().fold(0.0, f64::max);
        assert!((peak - 1.0).abs() < 0.01, "peaks at 1: {peak}");
        assert!(samples.last().unwrap().abs() < 1e-9 && samples[190].abs() < 0.02, "settles back");
    }

    #[test]
    fn springs_answer_a_tap_in_every_damping_regime() {
        for damping in [10.0, 20.0, 40.0] {
            let s = Spring { stiffness: 100.0, damping, mass: 1.0 };
            let at: Vec<f64> = (0..3000).map(|i| s.impulse(i as f64 / 1000.0)).collect();
            let peak = at.iter().copied().fold(f64::MIN, f64::max);
            assert!((peak - 1.0).abs() < 1e-3, "damping {damping}: peak {peak}");
            assert_eq!(at[0], 0.0);
            assert!(at[2999].abs() < 0.05, "damping {damping} settles: {}", at[2999]);
        }
        let under = Spring { stiffness: 100.0, damping: 4.0, mass: 1.0 };
        assert!((0..2000).any(|i| under.impulse(i as f64 / 1000.0) < -0.1), "under-damped swings past rest");
    }

    #[test]
    fn enter_exit_and_emphasis_move_between_rest_and_the_preset() {
        let clock = Clock { start: 100.0, duration: 400.0, curve: LINEAR };
        let enter = Motion::Enter(rise());
        assert_eq!(enter.look(&clock, 99.0), None, "not drawn before its entrance");
        assert_eq!(enter.look(&clock, 100.0), Some(rise()));
        let half = enter.look(&clock, 300.0).unwrap();
        assert_eq!((half.opacity, half.translate), (0.5, [0.0, 12.0]));
        assert_eq!(enter.look(&clock, 500.0), Some(Look::REST));
        let exit = Motion::Exit(rise());
        assert_eq!(exit.look(&clock, 0.0), Some(Look::REST), "at rest until it leaves");
        assert_eq!(exit.look(&clock, 300.0).unwrap().opacity, 0.5);
        assert_eq!(exit.look(&clock, 500.0), None, "gone once it has left");
        let pulse = Motion::Emphasis(Look { scale: [1.04, 1.04], ..Look::REST });
        assert_eq!(pulse.look(&clock, 300.0).unwrap().scale, [1.04, 1.04]);
        assert_eq!(pulse.look(&clock, 600.0), Some(Look::REST));
    }

    #[test]
    fn a_look_turns_about_its_anchor() {
        let rect = [10.0, 20.0, 100.0, 40.0];
        let map = |m: [f64; 6], p: [f64; 2]| [m[0] * p[0] + m[2] * p[1] + m[4], m[1] * p[0] + m[3] * p[1] + m[5]];
        assert_eq!(Look::REST.affine(rect), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        // Grown from the bottom middle: the anchor stays put, the top comes down.
        let flat = Look { scale: [1.0, 0.0], anchor: [0.5, 1.0], ..Look::REST };
        assert_eq!(map(flat.affine(rect), [60.0, 60.0]), [60.0, 60.0]);
        assert_eq!(map(flat.affine(rect), [10.0, 20.0]), [10.0, 60.0]);
        // A quarter turn clockwise about the center: right of it goes below it.
        let turned = Look { rotate: 90.0, ..Look::REST };
        let p = map(turned.affine(rect), [70.0, 40.0]);
        assert!((p[0] - 60.0).abs() < 1e-9 && (p[1] - 50.0).abs() < 1e-9, "{p:?}");
        let moved = Look { translate: [5.0, -5.0], ..Look::REST };
        assert_eq!(map(moved.affine(rect), [0.0, 0.0]), [5.0, -5.0]);
        assert!(Look::REST.is_rest() && !moved.is_rest());
    }

    #[test]
    fn a_tint_and_a_draw_on_stay_between_rest_and_the_look() {
        let accent = Color([255, 80, 0, 255]);
        let look = Look { tint: Some((accent, 1.0)), progress: 0.0, ..Look::REST };
        assert_eq!(look.toward(0.25).tint, Some((accent, 0.25)));
        assert_eq!(look.toward(0.25).progress, 0.75);
        // A spring past the look carries neither past its end.
        assert_eq!((look.toward(1.3).tint, look.toward(1.3).progress), (Some((accent, 1.0)), 0.0));
        assert_eq!((look.toward(-0.2).tint, look.toward(-0.2).progress), (Some((accent, 0.0)), 1.0));
        assert!(look.toward(0.0).is_rest() && !look.toward(0.01).is_rest());
    }

    #[test]
    fn keyframes_hold_their_ends_and_ease_between() {
        let keys = Keys {
            opacity: vec![Key { t: 0.0, v: 0.0, curve: LINEAR }, Key { t: 400.0, v: 1.0, curve: LINEAR }],
            translate: vec![
                Key { t: 100.0, v: [0.0, 24.0], curve: LINEAR },
                Key { t: 300.0, v: [0.0, 0.0], curve: OUT },
            ],
            ..Keys::default()
        };
        assert_eq!(keys.duration(), 400.0);
        assert_eq!(keys.look(0.0).translate, [0.0, 24.0], "before its first key, the first value");
        assert_eq!(keys.look(200.0).opacity, 0.5);
        assert!(keys.look(200.0).translate[1] < 12.0, "ease-out is past halfway");
        assert_eq!(keys.look(1000.0), Look::REST);
        let clock = Clock { start: 50.0, duration: keys.duration(), curve: LINEAR };
        assert_eq!(Motion::Keys(keys.clone()).look(&clock, 250.0).unwrap().opacity, 0.5);
    }
}
