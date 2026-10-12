//! Coherence between neighbouring slides (Vzestup's windowed analysis, after Splat's): each
//! pair's cosine between embeddings, or the overlap of their words where a slide has none, read
//! against the deck's own mean, with no curve invented to read it against. Three detectors mark
//! a break: a pair more than a spread below the mean; a strict valley below it; and a window's
//! centroid drifting in the top fifth of drifts. A break at a beat boundary is soft, since a
//! structural transition may jump; one inside a beat is hard.

use super::SlideForHealth;
use super::constants::{
    KEYWORD_OVERLAP_CEILING, STAT_BREAK_MIN_STD, TOPIC_QUINTILE, TOPIC_SHIFT_FLOOR, WINDOW_SIZE_DIVISOR,
    WINDOW_SIZE_MAX, WINDOW_SIZE_MIN,
};
use crate::js;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// What coherence is read with (`CoherenceOptions`), each Vzestup's constant by default: the
/// seam the sensitivity suite perturbs, which no caller of the metrics moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoherenceOptions {
    pub topic_shift_floor: f64,
    pub topic_quintile: f64,
    pub stat_break_min_std: f64,
    pub keyword_overlap_ceiling: f64,
    pub window_size_divisor: usize,
    pub window_size_min: usize,
    pub window_size_max: usize,
}

impl Default for CoherenceOptions {
    fn default() -> CoherenceOptions {
        CoherenceOptions {
            topic_shift_floor: TOPIC_SHIFT_FLOOR,
            topic_quintile: TOPIC_QUINTILE,
            stat_break_min_std: STAT_BREAK_MIN_STD,
            keyword_overlap_ceiling: KEYWORD_OVERLAP_CEILING,
            window_size_divisor: WINDOW_SIZE_DIVISOR,
            window_size_min: WINDOW_SIZE_MIN,
            window_size_max: WINDOW_SIZE_MAX,
        }
    }
}

/// How neighbours' similarity was read: by their embeddings, or, where any pair lacks them or
/// they differ in length, by their words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    Embedding,
    Keyword,
}

/// How hard a break is: soft at a beat boundary, hard inside a beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Soft,
    Hard,
}

/// A break in the deck's flow: the slide it comes into, the pair it falls between (from 0, as
/// the pairs' similarities are listed), their similarity, how hard it is, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Break {
    pub slide_id: String,
    pub pair_index: usize,
    pub sim: f64,
    pub severity: Severity,
    /// What each detector that fired saw, `; ` between.
    pub reason: String,
    pub at_beat_boundary: bool,
}

/// How often each detector fired, a pair counted by each one that fired on it; `topic_unique`
/// the pairs the topic detector alone caught, which says whether it earns its place.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectorCounts {
    pub stat: usize,
    pub valley: usize,
    pub topic: usize,
    pub topic_unique: usize,
}

/// The deck's coherence (`CoherenceAnalysis`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoherenceAnalysis {
    /// 0–100: the mean similarity, or, read by words, the mean against where overlap tops out.
    pub score: f64,
    /// Each pair of neighbours' similarity, one fewer than the slides.
    pub pair_similarities: Vec<f64>,
    pub mean_sim: f64,
    pub std_sim: f64,
    /// Each window's centroid's drift from the next's, `1 − cosine`; none read by words.
    pub window_shifts: Vec<f64>,
    /// The slide each break comes into.
    pub weak_transitions: Vec<String>,
    pub breaks: Vec<Break>,
    pub method: Method,
    pub detector_counts: DetectorCounts,
    /// The drift the topic detector held the windows to, where there were windows.
    pub topic_tau: Option<f64>,
    /// The largest drift, where there were windows: how far it stood from the floor.
    pub topic_max_delta: Option<f64>,
}

/// The cosine of the angle between `a` and `b` (`cosineSimilarity`): 0 where they differ in
/// length or either is all zeros.
pub fn cosine(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// The words two slides share (`keywordOverlap`), where they have no embeddings: of the words of
/// more than three letters in each one's title and body, those both use, over the larger set;
/// ½ where either has none.
pub fn keyword_overlap(a: &SlideForHealth, b: &SlideForHealth) -> f64 {
    let words = |s: &SlideForHealth| -> BTreeSet<String> {
        let text = format!("{} {}", s.title, s.body).to_lowercase();
        text.split(|c: char| !c.is_ascii_lowercase() && !c.is_ascii_digit())
            .filter(|w| w.len() > 3)
            .map(str::to_string)
            .collect()
    };
    let (wa, wb) = (words(a), words(b));
    if wa.is_empty() || wb.is_empty() {
        return 0.5;
    }
    let overlap = wa.intersection(&wb).count();
    overlap as f64 / wa.len().max(wb.len()) as f64
}

fn mean_of(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut sum = 0.0;
    for x in xs {
        sum += x;
    }
    sum / xs.len() as f64
}

fn std_of(xs: &[f64], mu: f64) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut sum = 0.0;
    for x in xs {
        sum += (x - mu) * (x - mu);
    }
    (sum / xs.len() as f64).sqrt()
}

/// A slide's embedding, or none.
fn embedding(s: &SlideForHealth) -> &[f64] {
    s.embedding.as_deref().unwrap_or_default()
}

/// The mean of `vectors`, each as long as the first.
fn centroid(vectors: &[&[f64]]) -> Vec<f64> {
    let Some(first) = vectors.first() else { return Vec::new() };
    let mut out = vec![0.0; first.len()];
    for v in vectors {
        for (i, x) in out.iter_mut().enumerate() {
            // A shorter vector reads as JavaScript's `undefined` would.
            *x += v.get(i).copied().unwrap_or(f64::NAN);
        }
    }
    for x in &mut out {
        *x /= vectors.len() as f64;
    }
    out
}

/// The coherence of `visible`, the slides evaluated, in their order (`coherenceAnalysis`).
pub fn analyze(visible: &[SlideForHealth], options: &CoherenceOptions) -> CoherenceAnalysis {
    let mut analysis = CoherenceAnalysis {
        score: 100.0,
        pair_similarities: Vec::new(),
        mean_sim: 0.0,
        std_sim: 0.0,
        window_shifts: Vec::new(),
        weak_transitions: Vec::new(),
        breaks: Vec::new(),
        method: Method::Embedding,
        detector_counts: DetectorCounts::default(),
        topic_tau: None,
        topic_max_delta: None,
    };
    if visible.len() < 2 {
        return analysis;
    }
    // Embeddings only where every pair has them, of one length: mixed scales read as nonsense.
    let embedded = visible.windows(2).all(|pair| match (&pair[0].embedding, &pair[1].embedding) {
        (Some(a), Some(b)) => !a.is_empty() && a.len() == b.len(),
        _ => false,
    });
    let method = if embedded { Method::Embedding } else { Method::Keyword };
    let pairs: Vec<f64> = (visible.windows(2))
        .map(|pair| match (method, &pair[0].embedding, &pair[1].embedding) {
            (Method::Embedding, Some(a), Some(b)) => cosine(a, b),
            _ => keyword_overlap(&pair[0], &pair[1]),
        })
        .collect();
    let mean = mean_of(&pairs);
    let std = std_of(&pairs, mean);

    // Each window's centroid against the next's, a slide on.
    let mut shifts = Vec::new();
    if method == Method::Embedding {
        let len = visible.len();
        let quotient = len.checked_div(options.window_size_divisor).unwrap_or(usize::MAX);
        let w = quotient.max(options.window_size_min).min(options.window_size_max);
        if len > w {
            for i in 0..len - w {
                let a: Vec<&[f64]> = visible[i..i + w].iter().map(embedding).collect();
                let b: Vec<&[f64]> = visible[i + 1..i + 1 + w].iter().map(embedding).collect();
                shifts.push(1.0 - cosine(&centroid(&a), &centroid(&b)));
            }
        }
    }

    // The top fifth of drifts, largest first and ties in order: a stable insertion sort, as
    // JavaScript sorts by `b − a`.
    let mut sorted = shifts.clone();
    for i in 1..sorted.len() {
        let mut j = i;
        while j > 0 && sorted[j] - sorted[j - 1] > 0.0 {
            sorted.swap(j - 1, j);
            j -= 1;
        }
    }
    let at = js::max(0.0, (sorted.len() as f64 * options.topic_quintile).floor() - 1.0);
    // None where JavaScript reads `undefined`, past the end: then no drift is at or above it.
    let threshold = if at.is_nan() { None } else { sorted.get(at as usize).copied() };

    let boundary = |i: usize| matches!((visible[i].beat(), visible[i + 1].beat()), (Some(a), Some(b)) if a != b);
    let counts = &mut analysis.detector_counts;
    for (i, &sim) in pairs.iter().enumerate() {
        let mut reasons = Vec::new();
        let stat = std > options.stat_break_min_std && sim < mean - std;
        if stat {
            reasons.push(format!(
                "{} cosine, below \u{3bc}\u{2212}\u{3c3} ({})",
                js::to_fixed(sim, 2),
                js::to_fixed(mean - std, 2)
            ));
            counts.stat += 1;
        }
        let valley = i > 0 && i + 1 < pairs.len() && pairs[i - 1] > sim && pairs[i + 1] > sim && sim < mean;
        if valley {
            reasons.push("local valley below mean".to_string());
            counts.valley += 1;
        }
        let topic = match (shifts.get(i), threshold) {
            (Some(&shift), Some(t)) => shift >= t && t > options.topic_shift_floor,
            _ => false,
        };
        if topic {
            reasons.push(format!("centroid drift {} (top quintile)", js::to_fixed(shifts[i], 2)));
            counts.topic += 1;
            if !stat && !valley {
                counts.topic_unique += 1;
            }
        }
        if !reasons.is_empty() {
            let at_beat_boundary = boundary(i);
            let id = visible[i + 1].id.clone();
            analysis.breaks.push(Break {
                slide_id: id.clone(),
                pair_index: i,
                sim,
                severity: if at_beat_boundary { Severity::Soft } else { Severity::Hard },
                reason: reasons.join("; "),
                at_beat_boundary,
            });
            analysis.weak_transitions.push(id);
        }
    }

    // The mean itself, as Splat reads it: a deck at μ = 0.81 reads 81, its breaks shown rather
    // than docked. Words overlap far less than embeddings align, so read against where they top
    // out.
    let scaled = match method {
        Method::Embedding => mean * 100.0,
        Method::Keyword => (mean / options.keyword_overlap_ceiling) * 100.0,
    };
    analysis.score = js::round(js::clamp(scaled, 0.0, 100.0));
    if !shifts.is_empty() {
        analysis.topic_tau = threshold.filter(|t| t.is_finite());
        analysis.topic_max_delta = Some(shifts.iter().copied().fold(f64::NEG_INFINITY, js::max));
    }
    analysis.pair_similarities = pairs;
    analysis.mean_sim = mean;
    analysis.std_sim = std;
    analysis.window_shifts = shifts;
    analysis.method = method;
    analysis
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::NarrativeMeta;
    use crate::health::constants::{KEYWORD_OVERLAP_CEILING, TOPIC_SHIFT_FLOOR};
    use crate::health::fixtures::{clean_deck, digression_deck, plain, slide, unit};

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 5e-7
    }

    fn analyze_default(slides: &[SlideForHealth]) -> CoherenceAnalysis {
        analyze(slides, &CoherenceOptions::default())
    }

    #[test]
    fn cosines_are_known_by_construction() {
        let v = unit(4, &[0, 1]);
        assert!(close(cosine(&v, &v), 1.0));
        assert!(close(cosine(&unit(4, &[0]), &unit(4, &[1])), 0.0));
        assert!(close(cosine(&[1.0, 0.0], &[-1.0, 0.0]), -1.0));
        // Mismatched or all zeros: 0, not NaN.
        assert_eq!(cosine(&[1.0, 0.0, 0.0], &[1.0, 0.0]), 0.0);
        assert_eq!((cosine(&[0.0; 3], &[1.0, 2.0, 3.0]), cosine(&[1.0, 2.0, 3.0], &[0.0; 3])), (0.0, 0.0));
    }

    #[test]
    fn keyword_overlap_is_the_share_of_long_words_both_use() {
        let titled = |id: &str, title: &str| SlideForHealth { title: title.into(), body: String::new(), ..slide(id) };
        assert_eq!(keyword_overlap(&titled("a", ""), &titled("b", "hello world test")), 0.5);
        assert_eq!(keyword_overlap(&titled("a", "alpha bravo charlie"), &titled("b", "alpha bravo charlie")), 1.0);
        assert!(close(
            keyword_overlap(&titled("a", "alpha bravo charlie"), &titled("b", "alpha bravo zulu")),
            2.0 / 3.0
        ));
    }

    #[test]
    fn a_deck_of_one_or_none_is_whole() {
        for slides in [Vec::new(), vec![slide("s1")]] {
            let a = analyze_default(&slides);
            assert_eq!(a.score, 100.0);
            assert!(a.pair_similarities.is_empty() && a.breaks.is_empty() && a.mean_sim.is_finite());
        }
    }

    #[test]
    fn slides_along_one_direction_score_100() {
        let base = unit(8, &[0, 1]);
        let slides: Vec<SlideForHealth> =
            (0..6).map(|i| SlideForHealth { embedding: Some(base.clone()), ..slide(&format!("s{i}")) }).collect();
        let a = analyze_default(&slides);
        assert_eq!(a.method, Method::Embedding);
        assert_eq!((a.pair_similarities.len(), a.score), (5, 100.0));
        assert!(a.pair_similarities.iter().all(|&s| close(s, 1.0)));
        assert!(a.breaks.is_empty());
    }

    /// Four slides along one direction, then three along another, each in the beat `beat` gives
    /// it.
    fn jump(beat: impl Fn(usize) -> Option<usize>) -> Vec<SlideForHealth> {
        let (a, b) = (unit(8, &[0, 1]), unit(8, &[4, 5]));
        let ids = ["a0", "a1", "a2", "a3", "b0", "b1", "b2"];
        (ids.iter().enumerate())
            .map(|(i, id)| SlideForHealth {
                embedding: Some(if i < 4 { a.clone() } else { b.clone() }),
                narrative_meta: beat(i).map(|b| NarrativeMeta { beat_index: Some(b), ..NarrativeMeta::default() }),
                ..slide(id)
            })
            .collect()
    }

    #[test]
    fn a_topic_jump_is_a_break_soft_at_a_beat_boundary_and_hard_inside_one() {
        let a = analyze_default(&jump(|_| None));
        assert_eq!(a.method, Method::Embedding);
        let seam = a.breaks.iter().find(|b| b.slide_id == "b0").expect("the seam");
        assert_eq!(seam.pair_index, 3);
        assert!(close(seam.sim, 0.0));
        let a = analyze_default(&jump(|i| Some(usize::from(i >= 4))));
        let seam = a.breaks.iter().find(|b| b.slide_id == "b0").expect("the seam");
        assert!(seam.at_beat_boundary && seam.severity == Severity::Soft);
        let a = analyze_default(&jump(|_| Some(0)));
        let seam = a.breaks.iter().find(|b| b.slide_id == "b0").expect("the seam");
        assert!(!seam.at_beat_boundary && seam.severity == Severity::Hard);
    }

    #[test]
    fn without_embeddings_of_one_length_words_are_read() {
        let titled = |id: &str, title: &str| SlideForHealth { title: title.into(), body: String::new(), ..slide(id) };
        let slides =
            [titled("k0", "alpha bravo charlie"), titled("k1", "alpha bravo delta"), titled("k2", "alpha bravo echo")];
        let a = analyze_default(&slides);
        assert_eq!(a.method, Method::Keyword);
        assert_eq!(a.pair_similarities.len(), 2);
        assert!(a.window_shifts.is_empty() && (0.0..=100.0).contains(&a.score));
        let embedded = |id: &str, e: &[f64]| SlideForHealth { embedding: Some(e.to_vec()), ..slide(id) };
        let slides = [embedded("m0", &[1.0, 0.0, 0.0]), embedded("m1", &[1.0, 0.0]), embedded("m2", &[1.0, 0.0, 0.0])];
        assert_eq!(analyze_default(&slides).method, Method::Keyword);
        // All-zero embeddings read as 0, never NaN.
        let slides: Vec<SlideForHealth> = (0..3).map(|i| embedded(&format!("z{i}"), &[0.0; 4])).collect();
        let a = analyze_default(&slides);
        assert!(a.score.is_finite() && a.mean_sim.is_finite() && a.std_sim.is_finite());
        assert!(a.pair_similarities.iter().all(|s| s.is_finite()));
    }

    // `healthConstants.sensitivity.test.ts`: each constant perturbed by a fifth moves what it
    // governs, leaves alone what it does not, and moves it by a bounded amount.

    fn with(options: CoherenceOptions) -> impl Fn(&[SlideForHealth]) -> CoherenceAnalysis {
        move |slides| analyze(slides, &options)
    }

    #[test]
    fn the_topic_floor_guards_against_weak_drift_and_leaves_a_strong_one_alone() {
        let floor = |k: f64| with(CoherenceOptions { topic_shift_floor: TOPIC_SHIFT_FLOOR * k, ..Default::default() });
        let topics = |k: f64| floor(k)(&digression_deck()).detector_counts.topic;
        let base = topics(1.0);
        assert!(base > 0);
        assert_eq!((topics(0.8), topics(1.2)), (base, base));
        assert_eq!(
            (floor(1.0)(&clean_deck()).detector_counts.topic, floor(0.8)(&clean_deck()).detector_counts.topic),
            (0, 0)
        );
        // The headroom is reported either way.
        let (clean, digression) = (analyze_default(&clean_deck()), analyze_default(&digression_deck()));
        let (Some(clean), Some(digression)) = (clean.topic_max_delta, digression.topic_max_delta) else {
            panic!("the largest drifts")
        };
        assert!(digression > clean);
    }

    /// Eight slides with no embedding, each sharing one word of five with the next.
    fn linked() -> Vec<SlideForHealth> {
        let titles = [
            "aaaa bbbb cccc dddd link1",
            "link1 eeee ffff gggg link2",
            "link2 hhhh iiii jjjj link3",
            "link3 kkkk llll mmmm link4",
            "link4 nnnn oooo pppp link5",
            "link5 qqqq rrrr ssss link6",
            "link6 tttt uuuu vvvv link7",
            "link7 wwww xxxx yyyy zzzz",
        ];
        (titles.iter().enumerate())
            .map(|(i, t)| SlideForHealth { title: (*t).into(), body: String::new(), ..plain(&format!("k{i}")) })
            .collect()
    }

    #[test]
    fn the_keyword_ceiling_moves_the_keyword_score_alone_and_boundedly() {
        let ceiling = |k: f64| {
            with(CoherenceOptions { keyword_overlap_ceiling: KEYWORD_OVERLAP_CEILING * k, ..Default::default() })
        };
        let base = analyze_default(&linked());
        assert_eq!(base.method, Method::Keyword);
        assert!(base.score > 0.0 && base.score < 100.0, "{}", base.score);
        let (lower, higher) = (ceiling(0.8)(&linked()).score, ceiling(1.2)(&linked()).score);
        assert!(lower > base.score && higher < base.score);
        assert!(lower <= 100.0 && lower - base.score < 30.0);
        let embedded = analyze_default(&clean_deck());
        assert_eq!(embedded.method, Method::Embedding);
        assert_eq!(ceiling(0.8)(&clean_deck()).score, embedded.score);
    }
}
