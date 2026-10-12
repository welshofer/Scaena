//! The deck's intensity against its arc (Vzestup's arc fidelity, and `arcCurve.ts`): the curves
//! a deck is held to, and a dynamic time warping between each slide's intensity and the curve,
//! weighted by where the slide stands, so a flat climax costs more than a flat hook.

use super::SlideForHealth;
use super::constants::{
    DEVIANT_INTENSITY_DELTA, DTW_FIDELITY_SLOPE, STAKE_CLOSE_BOUNDARY, STAKE_SETUP_BOUNDARY, STAKE_WEIGHT_CLIMAX,
    STAKE_WEIGHT_CLOSE, STAKE_WEIGHT_SETUP,
};
use crate::arcs::{self, NarrativeArc, Point};
use crate::js;
use serde::{Deserialize, Serialize};

/// The cost of the best alignment of two sequences (`dtwCost`): the sum of squared differences
/// along the cheapest warping path; 0 where either is empty.
pub fn dtw_cost(a: &[f64], b: &[f64]) -> f64 {
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 {
        return 0.0;
    }
    let at = |i: usize, j: usize| i * (m + 1) + j;
    let mut dp = vec![f64::INFINITY; (n + 1) * (m + 1)];
    dp[0] = 0.0;
    for i in 1..=n {
        for j in 1..=m {
            let diff = a[i - 1] - b[j - 1];
            let cost = diff * diff;
            dp[at(i, j)] = cost + js::min(js::min(dp[at(i - 1, j)], dp[at(i, j - 1)]), dp[at(i - 1, j - 1)]);
        }
    }
    dp[at(n, m)]
}

/// The curve the metrics hold a deck of `n` slides to, whatever its arc
/// (`canonicalIntensityCurve`): an asymmetric bell, low at the ends, peaking 72% of the way
/// through, rising sharper than it falls, held to 0.2–1; ½ for one slide.
pub fn canonical_curve(n: usize) -> Vec<f64> {
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![0.5];
    }
    let peak = 0.72;
    (0..n)
        .map(|i| {
            let t = i as f64 / (n - 1) as f64;
            let dist = (t - peak).abs();
            let sigma = if t < peak { 0.32 } else { 0.4 };
            let intensity = 0.25 + 0.75 * js::exp(-(dist * dist) / (2.0 * sigma * sigma));
            js::clamp(intensity, 0.2, 1.0)
        })
        .collect()
}

/// A reading of each slide's intensity: by the slide, its place among those evaluated, and
/// their count; none where it reads nothing.
pub type IntensityOf<'a> = &'a dyn Fn(&SlideForHealth, usize, usize) -> Option<f64>;

/// What arc fidelity is read with (`ArcFidelityOptions`), each Vzestup's constant by default:
/// the seam the sensitivity suite perturbs, and the reading of each slide's intensity.
#[derive(Clone, Copy)]
pub struct FidelityOptions<'a> {
    pub fidelity_slope: f64,
    pub deviant_delta: f64,
    pub stake_setup: f64,
    pub stake_climax: f64,
    pub stake_close: f64,
    /// Each slide's intensity in place of what it declares: its words' reading gives realized
    /// fidelity. A slide it reads nothing of falls back as one that declares nothing does.
    pub intensity_of: Option<IntensityOf<'a>>,
}

impl Default for FidelityOptions<'_> {
    fn default() -> Self {
        FidelityOptions {
            fidelity_slope: DTW_FIDELITY_SLOPE,
            deviant_delta: DEVIANT_INTENSITY_DELTA,
            stake_setup: STAKE_WEIGHT_SETUP,
            stake_climax: STAKE_WEIGHT_CLIMAX,
            stake_close: STAKE_WEIGHT_CLOSE,
            intensity_of: None,
        }
    }
}

/// A slide whose intensity bends the curve: by how much it stands above it (below, negative).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deviant {
    pub slide_id: String,
    pub delta: f64,
}

/// How well a deck's intensity follows a curve: 1 for perfectly, 0 for not at all; and the
/// slides that bend it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alignment {
    pub fidelity: f64,
    pub deviant_slides: Vec<Deviant>,
}

/// What a cell of the warp weighs for the slide at `index` of `total` (`highStakeWeight`): the
/// setup's weight before half the deck, the close's past 85% of it, the climax's between.
fn stake(index: usize, total: usize, options: &FidelityOptions) -> f64 {
    let t = if total > 1 { index as f64 / (total - 1) as f64 } else { 0.5 };
    if t < STAKE_SETUP_BOUNDARY {
        options.stake_setup
    } else if t > STAKE_CLOSE_BOUNDARY {
        options.stake_close
    } else {
        options.stake_climax
    }
}

/// How well the evaluated slides' intensity follows `canonical` (`arcFidelityDTW`): each slide's
/// declared intensity, held to 0–1, or where it declares none a tent peaking 70% of the way
/// through; warped onto the curve, each cell weighted by where its slide stands (setup, climax,
/// or close); the cost over the path's length, rooted, and scaled by the slope. A slide strays
/// where it stands more than the deviant delta from the curve at its place. Nothing to compare
/// reads 1.
pub fn fidelity(slides: &[SlideForHealth], canonical: &[f64], options: &FidelityOptions) -> Alignment {
    let visible: Vec<&SlideForHealth> = slides.iter().filter(|s| s.eligible()).collect();
    let (n, m) = (visible.len(), canonical.len());
    if n == 0 || m == 0 {
        return Alignment { fidelity: 1.0, deviant_slides: Vec::new() };
    }
    let actual: Vec<f64> = (visible.iter().enumerate())
        .map(|(i, s)| {
            let read = match options.intensity_of {
                Some(intensity_of) => intensity_of(s, i, n),
                None => s.declared_intensity(),
            };
            match read.filter(|v| v.is_finite()) {
                Some(v) => js::clamp(v, 0.0, 1.0),
                // Old decks declare none: a baseline, not a blow to the score.
                None => {
                    let t = if n > 1 { i as f64 / (n - 1) as f64 } else { 0.5 };
                    0.25 + 0.5 * (1.0 - (t - 0.7).abs())
                }
            }
        })
        .collect();
    let at = |i: usize, j: usize| i * (m + 1) + j;
    let mut dtw = vec![f64::INFINITY; (n + 1) * (m + 1)];
    dtw[0] = 0.0;
    for i in 1..=n {
        let weight = stake(i - 1, n, options);
        for j in 1..=m {
            let diff = actual[i - 1] - canonical[j - 1];
            let cost = weight * diff * diff;
            dtw[at(i, j)] = cost + js::min(js::min(dtw[at(i - 1, j)], dtw[at(i, j - 1)]), dtw[at(i - 1, j - 1)]);
        }
    }
    let mut raw = dtw[at(n, m)];
    if !raw.is_finite() {
        raw = dtw[at(n, 0)..=at(n, m)].iter().copied().filter(|v| v.is_finite()).fold(f64::INFINITY, js::min);
    }
    let normalised = (raw / (n + m) as f64).sqrt();
    let fidelity = js::clamp(1.0 - normalised * options.fidelity_slope, 0.0, 1.0);
    let mut deviant_slides = Vec::new();
    for (i, slide) in visible.iter().enumerate() {
        let place = js::round((i as f64 / js::max(1.0, n as f64 - 1.0)) * (m - 1) as f64);
        let expected = canonical[js::min((m - 1) as f64, place) as usize];
        let delta = actual[i] - expected;
        if delta.abs() > options.deviant_delta {
            deviant_slides.push(Deviant { slide_id: slide.id.clone(), delta });
        }
    }
    Alignment { fidelity, deviant_slides }
}

/// The arc the catalog names `arc` exactly, if any.
fn named(arc: Option<&str>) -> Option<&'static NarrativeArc> {
    let name = arc.filter(|a| !a.is_empty())?;
    arcs::arcs().iter().find(|a| a.arc == name)
}

/// The curve's intensity at `t` (`sampleAt`), `points` in order: between the two points about
/// it, linearly; the end's beyond either end.
fn sample_at(points: &[Point], t: f64) -> f64 {
    let (Some(first), Some(last)) = (points.first(), points.last()) else { return 0.5 };
    if t <= first.x {
        return first.y;
    }
    if t >= last.x {
        return last.y;
    }
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if t >= a.x && t <= b.x {
            let span = b.x - a.x;
            if span <= 0.0 {
                return a.y;
            }
            let f = (t - a.x) / span;
            return a.y + f * (b.y - a.y);
        }
    }
    last.y
}

/// The three-act shape a deck is held to where its arc is not known (`fallbackThreeAct`): 0.3
/// rising to 0.95 at 70% of the way through, and back.
fn three_act(n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| {
            let t = if n > 1 { i as f64 / (n - 1) as f64 } else { 0.5 };
            let (peak, base) = (0.95, 0.3);
            let dist = (t - 0.7).abs();
            let v = base + (peak - base) * js::max(0.0, 1.0 - dist * 1.8);
            js::clamp(v, 0.0, 1.0)
        })
        .collect()
}

/// The intensity each of `count` slides is held to on `arc`'s own curve (`getArcCanonicalCurve`),
/// which the arc's graph draws: its 100 points read at each slide's place, from the first to the
/// last, held to 0–1; an arc the catalog does not name, or none, the three-act shape.
pub fn curve(arc: Option<&str>, count: usize) -> Vec<f64> {
    if count == 0 {
        return Vec::new();
    }
    let points = named(arc).map(|a| a.points.as_slice()).unwrap_or_default();
    let (Some(first), Some(last)) = (points.first(), points.last()) else { return three_act(count) };
    let (x_min, x_max) = (first.x, last.x);
    (0..count)
        .map(|i| {
            let norm = if count > 1 { i as f64 / (count - 1) as f64 } else { 0.5 };
            let t = x_min + norm * (x_max - x_min);
            js::clamp(sample_at(points, t), 0.0, 1.0)
        })
        .collect()
}

/// The names of `arc`'s beats (`getArcBeatNames`), each beat's words before its first colon;
/// none for an arc the catalog does not name.
pub fn beat_names(arc: Option<&str>) -> Vec<&'static str> {
    let beats = named(arc).map(|a| a.beats.as_slice()).unwrap_or_default();
    beats.iter().map(|b| js::trim(b.split(':').next().unwrap_or_default())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::fixtures::{clean_deck, deviant_deck, directions, flat_deck, meta, plain, slide};

    fn declaring(prefix: &str, intensities: &[f64]) -> Vec<SlideForHealth> {
        (intensities.iter().enumerate())
            .map(|(i, &v)| SlideForHealth { narrative_meta: meta(Some(v), None), ..slide(&format!("{prefix}{i}")) })
            .collect()
    }

    fn default_fit(slides: &[SlideForHealth], canonical: &[f64]) -> Alignment {
        fidelity(slides, canonical, &FidelityOptions::default())
    }

    /// The first index of the largest value, as `indexOf(Math.max(...))` finds it.
    fn peak(curve: &[f64]) -> usize {
        let top = curve.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        curve.iter().position(|&v| v == top).expect("a peak")
    }

    #[test]
    fn the_warp_costs_nothing_for_alike_and_more_for_further() {
        assert_eq!((dtw_cost(&[], &[1.0, 2.0, 3.0]), dtw_cost(&[1.0, 2.0, 3.0], &[])), (0.0, 0.0));
        let base = [0.2, 0.5, 0.8];
        assert_eq!(dtw_cost(&base, &base), 0.0);
        let shifted = |by: f64| base.map(|v| v + by);
        assert!(dtw_cost(&base, &shifted(0.5)) > dtw_cost(&base, &shifted(0.1)));
    }

    #[test]
    fn the_canonical_curve_is_a_bell_peaking_past_the_middle() {
        assert!(canonical_curve(0).is_empty());
        assert_eq!(canonical_curve(1), [0.5]);
        let curve = canonical_curve(20);
        assert_eq!(curve.len(), 20);
        assert!(curve.iter().all(|v| v.is_finite() && (0.2..=1.0).contains(v)));
        // 0.72 of the way through 50 slides is about the 35th, from 0.
        assert!((33..=37).contains(&peak(&canonical_curve(50))));
    }

    #[test]
    fn nothing_to_compare_reads_whole() {
        let whole = Alignment { fidelity: 1.0, deviant_slides: Vec::new() };
        assert_eq!(default_fit(&[], &[]), whole);
        assert_eq!(default_fit(&[slide("a")], &[]), whole);
        assert_eq!(default_fit(&[], &[0.3, 0.6, 0.9]), whole);
    }

    #[test]
    fn a_deck_on_the_curve_is_faithful_and_a_flat_one_less() {
        let canonical = canonical_curve(10);
        let matching = default_fit(&declaring("m", &canonical), &canonical);
        assert!(matching.fidelity > 0.95 && matching.deviant_slides.is_empty());
        let flat = default_fit(&declaring("f", &[0.25; 10]), &canonical);
        assert!(matching.fidelity > flat.fidelity && flat.fidelity < 0.8);
        // Out of range, held to it before it is scored.
        let wild = default_fit(&declaring("s", &[5.0; 10]), &canonical).fidelity;
        assert!((0.0..=1.0).contains(&wild));
    }

    #[test]
    fn a_slide_far_from_the_curve_bends_it() {
        let canonical = canonical_curve(8);
        let mut slides = declaring("s", &canonical);
        slides[3].narrative_meta = meta(Some(0.0), None);
        let fit = default_fit(&slides, &canonical);
        assert!(fit.deviant_slides.iter().any(|d| d.slide_id == "s3" && d.delta < -0.25));
        // Undeclared, it reads a baseline; hidden, it is not read.
        let undeclared: Vec<SlideForHealth> = (0..6).map(|i| slide(&format!("s{i}"))).collect();
        let fit = default_fit(&undeclared, &canonical_curve(6)).fidelity;
        assert!(fit.is_finite() && (0.0..=1.0).contains(&fit));
        let canonical = canonical_curve(4);
        let mut slides = declaring("k", &canonical);
        slides.insert(1, SlideForHealth { hidden: true, narrative_meta: meta(Some(0.0), None), ..slide("hidden") });
        assert!(!default_fit(&slides, &canonical).deviant_slides.iter().any(|d| d.slide_id == "hidden"));
        // A curve of one point.
        let fit = default_fit(&declaring("a", &[0.5, 0.5]), &[0.5]).fidelity;
        assert!(fit.is_finite() && (0.0..=1.0).contains(&fit));
    }

    // `healthConstants.sensitivity.test.ts`: each constant perturbed by a fifth moves what it
    // governs, leaves alone what it does not, and moves it by a bounded amount.

    fn fit_with(slides: &[SlideForHealth], options: FidelityOptions) -> Alignment {
        fidelity(slides, &canonical_curve(slides.len()), &options)
    }

    #[test]
    fn the_slope_governs_how_hard_a_flat_deck_is_punished() {
        let slope = |k: f64| FidelityOptions { fidelity_slope: DTW_FIDELITY_SLOPE * k, ..Default::default() };
        let flat = |k: f64| fit_with(&flat_deck(), slope(k)).fidelity;
        let (base, softer, harsher) = (flat(1.0), flat(0.8), flat(1.2));
        assert!(softer > base && harsher < base);
        assert!((harsher - base).abs() < 0.2);
        let clean = |k: f64| fit_with(&clean_deck(), slope(k)).fidelity;
        assert!((clean(0.8) - clean(1.0)).abs() < 0.05 && (clean(1.2) - clean(1.0)).abs() < 0.05);
    }

    #[test]
    fn the_deviant_delta_governs_which_slides_bend_the_curve_and_nothing_else() {
        let delta = |k: f64| FidelityOptions { deviant_delta: DEVIANT_INTENSITY_DELTA * k, ..Default::default() };
        // Every slide 0.28 below the curve: caught at 0.25 and 0.20, let go at 0.30.
        let canonical = canonical_curve(10);
        let below: Vec<SlideForHealth> = (canonical.iter().enumerate())
            .map(|(i, v)| SlideForHealth {
                embedding: Some(directions().0),
                narrative_meta: meta(Some(js::max(0.0, v - 0.28)), Some(0)),
                ..plain(&format!("b{i}"))
            })
            .collect();
        let count = |k: f64| fidelity(&below, &canonical, &delta(k)).deviant_slides.len();
        let (looser, base, tighter) = (count(1.2), count(1.0), count(0.8));
        assert!(tighter >= base && base >= looser && tighter > looser, "{tighter} {base} {looser}");
        let clean = |k: f64| fit_with(&clean_deck(), delta(k)).deviant_slides.len();
        assert_eq!((clean(1.0), clean(0.8)), (0, 0));
        let deviant = |k: f64| fit_with(&deviant_deck(), delta(k)).fidelity;
        assert_eq!(deviant(1.2), deviant(1.0));
    }

    #[test]
    fn the_climax_weight_amplifies_a_flat_peak() {
        let climax = |k: f64| FidelityOptions { stake_climax: STAKE_WEIGHT_CLIMAX * k, ..Default::default() };
        let flat = |k: f64| fit_with(&flat_deck(), climax(k)).fidelity;
        let (base, lighter, heavier) = (flat(1.0), flat(0.8), flat(1.2));
        assert!(lighter > base && heavier < base);
        assert!((heavier - base).abs() < 0.2);
        let clean = |k: f64| fit_with(&clean_deck(), climax(k)).fidelity;
        assert!((clean(1.2) - clean(1.0)).abs() < 0.05);
    }

    // `arcCurve.test.ts`.

    #[test]
    fn an_arcs_curve_is_sampled_at_each_slide() {
        assert!(curve(Some("Hero's Journey"), 0).is_empty());
        let unknown = curve(Some("Definitely Not An Arc"), 10);
        assert_eq!((unknown.len(), peak(&unknown)), (10, 6));
        let none = curve(None, 5);
        assert!(none.len() == 5 && none.iter().all(|v| (0.0..=1.0).contains(v)));
        let hero = curve(Some("Hero's Journey"), 20);
        assert!(hero.len() == 20 && hero.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
        let one = curve(Some("Hero's Journey"), 1);
        assert!(one.len() == 1 && (0.0..=1.0).contains(&one[0]));
        for n in [2, 7, 13, 50, 100] {
            assert_eq!(curve(Some("Hero's Journey"), n).len(), n);
        }
    }

    #[test]
    fn an_arcs_beats_are_named_before_their_colons() {
        assert!(beat_names(None).is_empty() && beat_names(Some("Definitely Not An Arc")).is_empty());
        let beats = beat_names(Some("Hero's Journey"));
        assert!(!beats.is_empty() && beats.iter().all(|b| !b.contains(':') && !b.is_empty()));
        assert_eq!(beats[0], "Ordinary World");
    }
}
