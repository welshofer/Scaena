//! A deck's narrative health, as Vzestup measures it (`health.ts`, PLAN G.9): pacing against the
//! talk's length; the balance of bullets, notes, and what is shown; coherence between
//! neighbouring slides; the intensity each slide declares against its arc's curve; what its
//! words read against what it declares, in intensity and in tone; and drift from the beats the
//! slides were written for. Pure arithmetic over the slides, the same numbers Vzestup's gives:
//! no clock, no call, `Math.exp` and `toFixed` as JavaScript computes them.
//!
//! What the words read is a designed prior, not a calibrated model, so the gaps it finds are
//! warnings, never folded into the overall score.

pub mod arc;
pub mod coherence;
pub mod constants;
pub mod drift;
pub mod intensity;
mod lexicon;
pub mod valence;

pub use constants::{INTENSITY_GAP_THRESHOLD, TONE_GAP_THRESHOLD};
pub use intensity::DETECTED_INTENSITY_VERSION;
pub use valence::DETECTED_VALENCE_VERSION;

use crate::js;
use arc::{Deviant, FidelityOptions, IntensityOf};
use coherence::{Break, CoherenceOptions, DetectorCounts, Method, Severity};
use constants::{
    COMPOSITE_FALLBACK_WEIGHT_ARC, COMPOSITE_FALLBACK_WEIGHT_BALANCE, COMPOSITE_FALLBACK_WEIGHT_PACING,
    COMPOSITE_WEIGHT_ARC, COMPOSITE_WEIGHT_BALANCE, COMPOSITE_WEIGHT_COHERENCE, COMPOSITE_WEIGHT_PACING,
};
use serde::{Deserialize, Serialize};

/// A slide as narrative health reads it (`SlideForHealth`): its words, whether it is hidden or
/// left out of evaluation, its embedding, and what the narrative declared of it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SlideForHealth {
    pub id: String,
    pub title: String,
    pub body: String,
    pub notes: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    /// An appendix slide (a Q&A, a cue card) evaluated as if hidden, though shown.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub exclude_from_evaluation: bool,
    /// What its words embed as, for coherence; where any pair lacks one, their words are read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f64>>,
    /// A slide whose layout makes one point (a big number, a quote, a statement): Scaena's,
    /// not Vzestup's (ADR-0032, proposed). It is left out of the points a slide makes on
    /// average; every other measure reads it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub one_idea: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub narrative_meta: Option<NarrativeMeta>,
}

/// What the narrative declared of a slide as it wrote it (`narrativeMeta`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NarrativeMeta {
    /// The beat of the arc it was written for, from 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beat_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beat_name: Option<String>,
    /// What the slide does for the arc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    /// What the audience should feel, believe, or do after it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    /// 0 to 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_intensity: Option<f64>,
    /// −1 (grave) to +1 (hopeful).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_valence: Option<f64>,
}

impl SlideForHealth {
    /// Whether it is evaluated (`isEvalEligible`): neither hidden nor left out.
    pub fn eligible(&self) -> bool {
        !self.hidden && !self.exclude_from_evaluation
    }

    /// What the estimators read of it (`slideTextForDetection`): its title, body, and notes,
    /// those with words, a line apart, as its embedding reads them.
    pub fn text(&self) -> String {
        let parts: Vec<&str> = [&self.title, &self.body, &self.notes]
            .into_iter()
            .map(String::as_str)
            .filter(|p| !js::trim(p).is_empty())
            .collect();
        parts.join("\n")
    }

    fn beat(&self) -> Option<usize> {
        self.narrative_meta.as_ref()?.beat_index
    }

    fn declared_intensity(&self) -> Option<f64> {
        self.narrative_meta.as_ref()?.expected_intensity
    }

    fn declared_valence(&self) -> Option<f64> {
        self.narrative_meta.as_ref()?.expected_valence
    }
}

/// How grave an alert is (its `type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warning,
    Error,
}

/// What an alert is about, where a fix can be offered for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AlertKind {
    IntensityGap,
    ToneGap,
}

/// Something narrative health asks a person to look at (`HealthAlert`), and the slide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthAlert {
    #[serde(rename = "type")]
    pub level: Level,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slide_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<AlertKind>,
}

impl HealthAlert {
    fn on(slide: &str, level: Level, message: impl Into<String>, kind: Option<AlertKind>) -> HealthAlert {
        HealthAlert { level, message: message.into(), slide_id: Some(slide.to_string()), kind }
    }
}

/// A slide whose words read far from the intensity it declares: what it declares, held to 0–1,
/// what its words read, and the first less the second.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntensityGap {
    pub slide_id: String,
    pub declared: f64,
    pub detected: f64,
    pub gap: f64,
}

/// A slide whose words read in another tone than it declares: what it declares, held to −1–1,
/// what its words read, and the first less the second.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToneGap {
    pub slide_id: String,
    pub declared: f64,
    pub detected: f64,
    pub gap: f64,
}

/// How closely the slides' words deliver what they declare, over every slide that declares it
/// and has words enough to read: 100 where they agree exactly, 50 for a mean gap of half the
/// scale; 100 where none can be measured.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    pub score: f64,
    pub measured_slides: usize,
    pub mean_abs_gap: f64,
}

/// Whether the deck is short of its talk's length, about right, or long.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PacingStatus {
    Under,
    Optimal,
    Over,
}

/// The slides against the count the talk's length asks for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pacing {
    pub score: f64,
    pub current_slide_count: usize,
    pub target_slide_count: usize,
    pub status: PacingStatus,
    pub message: String,
}

/// Bullets, notes, and what is shown, slide by slide.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentBalance {
    pub score: f64,
    /// To a tenth.
    pub avg_bullets_per_slide: f64,
    /// In UTF-16 units, as JavaScript counts a string's length.
    pub avg_speaker_notes_length: f64,
    /// The share of slides with an embedding or a body of more than 30 units, to a hundredth.
    pub visual_coverage: f64,
    pub message: String,
}

/// The deck's coherence ([`coherence::CoherenceAnalysis`]), and what it says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Coherence {
    pub score: f64,
    pub weak_transitions: Vec<String>,
    pub method: Method,
    pub pair_similarities: Vec<f64>,
    pub mean_sim: f64,
    pub std_sim: f64,
    pub window_shifts: Vec<f64>,
    pub breaks: Vec<Break>,
    pub detector_counts: DetectorCounts,
    pub topic_tau: Option<f64>,
    pub topic_max_delta: Option<f64>,
    pub message: String,
}

/// How closely the declared intensity follows the curve, and the slides that bend it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArcFidelity {
    pub score: f64,
    pub deviant_slides: Vec<Deviant>,
    pub message: String,
}

/// How closely the intensity the words read follows the curve, and how far the declared
/// fidelity stands above it: where it is positive, the metadata flatters the prose.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RealizedArcFidelity {
    pub score: f64,
    pub declared_minus_realized: f64,
}

/// How closely the words deliver what the slides declare ([`Match`]), and what it says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchReport {
    pub score: f64,
    pub measured_slides: usize,
    pub mean_abs_gap: f64,
    pub message: String,
}

/// A deck's narrative health (`LocalHealthMetrics`): each score 0–100, the overall one of
/// pacing, balance, coherence, and arc fidelity; the gaps, matches, and realized fidelity beside
/// it, warnings that move no score; and the alerts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalHealthMetrics {
    pub overall_score: f64,
    pub pacing: Pacing,
    pub content_balance: ContentBalance,
    pub coherence: Coherence,
    pub arc_fidelity: ArcFidelity,
    pub realized_arc_fidelity: RealizedArcFidelity,
    pub intensity_gaps: Vec<IntensityGap>,
    pub tone_gaps: Vec<ToneGap>,
    pub intensity_match: MatchReport,
    pub tone_match: MatchReport,
    pub alerts: Vec<HealthAlert>,
}

/// Each evaluated slide that declares a finite value by `declared`, and what its words read by
/// `detected`, where they read: an absent signal is no disagreement.
fn measured(
    slides: &[SlideForHealth],
    declared: fn(&SlideForHealth) -> Option<f64>,
    detected: fn(&str, Option<&str>) -> Option<f64>,
) -> impl Iterator<Item = (&SlideForHealth, f64, f64)> {
    (slides.iter().filter(|s| s.eligible())).filter_map(move |s| {
        let raw = declared(s).filter(|v| v.is_finite())?;
        Some((s, raw, detected(&s.text(), None)?))
    })
}

/// The slides whose words read more than [`INTENSITY_GAP_THRESHOLD`] from the intensity they
/// declare, either way (`computeIntensityGaps`).
pub fn intensity_gaps(slides: &[SlideForHealth]) -> Vec<IntensityGap> {
    measured(slides, SlideForHealth::declared_intensity, intensity::detected)
        .filter_map(|(s, raw, detected)| {
            let declared = js::clamp(raw, 0.0, 1.0);
            let gap = declared - detected;
            (gap.abs() > INTENSITY_GAP_THRESHOLD).then(|| IntensityGap {
                slide_id: s.id.clone(),
                declared,
                detected,
                gap,
            })
        })
        .collect()
}

/// The slides whose words read more than [`TONE_GAP_THRESHOLD`] from the tone they declare,
/// either way (`computeToneGaps`): the right intensity, the wrong emotion.
pub fn tone_gaps(slides: &[SlideForHealth]) -> Vec<ToneGap> {
    measured(slides, SlideForHealth::declared_valence, valence::detected)
        .filter_map(|(s, raw, detected)| {
            let declared = js::clamp(raw, -1.0, 1.0);
            let gap = declared - detected;
            (gap.abs() > TONE_GAP_THRESHOLD).then(|| ToneGap { slide_id: s.id.clone(), declared, detected, gap })
        })
        .collect()
}

/// The mean distance between what slides declare, held to `lo`–`hi`, and what their words
/// read, and how many were measured.
fn mean_gap(measures: impl Iterator<Item = (f64, f64)>, lo: f64, hi: f64) -> Option<(f64, usize)> {
    let (mut sum, mut n) = (0.0, 0usize);
    for (raw, detected) in measures {
        sum += (js::clamp(raw, lo, hi) - detected).abs();
        n += 1;
    }
    (n > 0).then(|| (sum / n as f64, n))
}

/// How closely the words deliver the intensity each slide declares (`computeIntensityMatch`):
/// `100 × (1 − mean gap)`.
pub fn intensity_match(slides: &[SlideForHealth]) -> Match {
    let measures = measured(slides, SlideForHealth::declared_intensity, intensity::detected).map(|(_, r, d)| (r, d));
    match mean_gap(measures, 0.0, 1.0) {
        None => Match { score: 100.0, measured_slides: 0, mean_abs_gap: 0.0 },
        Some((mean, n)) => Match {
            score: js::round(js::clamp(100.0 * (1.0 - mean), 0.0, 100.0)),
            measured_slides: n,
            mean_abs_gap: mean,
        },
    }
}

/// How closely the words deliver the tone each slide declares (`computeToneMatch`):
/// `100 × (1 − mean gap ÷ 2)`, the tone's scale twice as wide as intensity's.
pub fn tone_match(slides: &[SlideForHealth]) -> Match {
    let measures = measured(slides, SlideForHealth::declared_valence, valence::detected).map(|(_, r, d)| (r, d));
    match mean_gap(measures, -1.0, 1.0) {
        None => Match { score: 100.0, measured_slides: 0, mean_abs_gap: 0.0 },
        Some((mean, n)) => Match {
            score: js::round(js::clamp(100.0 * (1.0 - mean / 2.0), 0.0, 100.0)),
            measured_slides: n,
            mean_abs_gap: mean,
        },
    }
}

/// The slides a talk asks for, by its minutes (`TARGET_SLIDE_COUNT_BY_LENGTH`).
const TARGET_SLIDES: [(u32, usize); 5] = [(15, 6), (30, 12), (45, 18), (90, 30), (120, 40)];

/// The slides a talk of `length_minutes` asks for (`expectedSlideCount`): 6 for 15 minutes, 12
/// for 30, 18 for 45, 30 for 90, and 40 for 120; 18 for any other length, or none.
pub fn expected_slides(length_minutes: Option<u32>) -> usize {
    let listed = length_minutes.and_then(|m| TARGET_SLIDES.iter().find(|(minutes, _)| *minutes == m));
    listed.map_or(18, |&(_, slides)| slides)
}

/// `n` and the noun, plural but for one.
fn counted(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

/// What coherence says (`coherenceMessage`).
fn coherence_message(method: Method, mean: f64, hard: usize, soft: usize) -> String {
    let method = match method {
        Method::Embedding => "embedding",
        Method::Keyword => "keyword",
    };
    let m = format!("\u{3bc}={} ({method})", js::to_fixed(mean, 2));
    if hard == 0 && soft == 0 {
        return format!("No narrative breaks \u{b7} {m}");
    }
    let mut parts = Vec::new();
    if hard > 0 {
        parts.push(counted(hard, "hard break"));
    }
    if soft > 0 {
        parts.push(format!("{soft} soft (at beat boundary)"));
    }
    format!("{} \u{b7} {m}", parts.join(", "))
}

/// A tone, signed: `+0.40`, `−0.25` (a minus sign, U+2212).
fn signed(v: f64) -> String {
    format!("{}{}", if v >= 0.0 { "+" } else { "\u{2212}" }, js::to_fixed(v.abs(), 2))
}

/// The narrative health of `slides`, a talk of `length_minutes` (`localHealthMetrics`): the
/// slides hidden or left out of evaluation are not read.
pub fn metrics(slides: &[SlideForHealth], length_minutes: Option<u32>) -> LocalHealthMetrics {
    let visible: Vec<SlideForHealth> = slides.iter().filter(|s| s.eligible()).cloned().collect();
    let count = visible.len();
    let target = expected_slides(length_minutes);

    // Pacing.
    let pacing_score = js::max(0.0, 100.0 - count.abs_diff(target) as f64 * 5.0);
    let status = if (count as f64) < target as f64 * 0.8 {
        PacingStatus::Under
    } else if count as f64 > target as f64 * 1.2 {
        PacingStatus::Over
    } else {
        PacingStatus::Optimal
    };
    let pacing_message = match status {
        PacingStatus::Under => format!(
            "{count} slides \u{2014} consider adding {} more for fuller coverage.",
            target.saturating_sub(count).max(1)
        ),
        PacingStatus::Optimal => format!("{count} slides \u{2014} well-paced for your target."),
        PacingStatus::Over => format!("{count} slides \u{2014} consider condensing {}.", count.saturating_sub(target)),
    };

    // Content balance.
    let units = |text: &str| text.encode_utf16().count();
    let pointed: Vec<&SlideForHealth> = visible.iter().filter(|s| !s.one_idea).collect();
    let bullets: usize = (pointed.iter())
        .map(|s| s.body.split('\n').map(js::trim).filter(|l| l.starts_with('\u{2022}') || l.starts_with('-')).count())
        .sum();
    let per_slide = |total: usize| if count > 0 { total as f64 / count as f64 } else { 0.0 };
    let avg_bullets = if pointed.is_empty() { 0.0 } else { bullets as f64 / pointed.len() as f64 };
    let avg_notes = per_slide(visible.iter().map(|s| units(&s.notes)).sum());
    let visual_coverage = per_slide(visible.iter().filter(|s| s.embedding.is_some() || units(&s.body) > 30).count());
    let bullet_score =
        if (2.0..=5.0).contains(&avg_bullets) { 100.0 } else { js::max(0.0, 100.0 - (avg_bullets - 3.5).abs() * 20.0) };
    let notes_score = if avg_notes >= 60.0 { 100.0 } else { (avg_notes / 60.0) * 100.0 };
    let balance_score = (bullet_score + notes_score + visual_coverage * 100.0) / 3.0;
    let mut balance = Vec::new();
    if avg_bullets < 2.0 {
        balance.push("Add more bullets to thin slides");
    }
    if avg_bullets > 5.0 {
        balance.push("Some slides are bullet-heavy \u{2014} consolidate");
    }
    if avg_notes < 30.0 {
        balance.push("Speaker notes are sparse");
    }

    // Coherence.
    let coh = coherence::analyze(&visible, &CoherenceOptions::default());
    let hard = coh.breaks.iter().filter(|b| b.severity == Severity::Hard).count();
    let soft = coh.breaks.iter().filter(|b| b.severity == Severity::Soft).count();

    // Arc fidelity, declared and realized: the same warp over what the words read.
    let canonical = arc::canonical_curve(count);
    let fit = arc::fidelity(&visible, &canonical, &FidelityOptions::default());
    let arc_score = js::round(fit.fidelity * 100.0);
    let words: IntensityOf = &|s, _, _| intensity::detected(&s.text(), None);
    let realized =
        arc::fidelity(&visible, &canonical, &FidelityOptions { intensity_of: Some(words), ..Default::default() });
    let realized_score = js::round(realized.fidelity * 100.0);

    // What the words read against what the slides declare.
    let intensity_gaps = intensity_gaps(&visible);
    let tone_gaps = tone_gaps(&visible);
    let intensity_match = intensity_match(&visible);
    let intensity_message = if intensity_match.measured_slides == 0 {
        "No declared intensities to compare yet \u{2014} run /analyze-narrative.".to_string()
    } else if intensity_gaps.is_empty() {
        "Words deliver the intensity each slide claims.".to_string()
    } else {
        format!("{} read far from the intensity they claim.", counted(intensity_gaps.len(), "slide"))
    };
    let tone_match = tone_match(&visible);
    let tone_message = if tone_match.measured_slides == 0 {
        "No declared tone to compare yet \u{2014} run /analyze-narrative.".to_string()
    } else if tone_gaps.is_empty() {
        "Words carry the tone each slide claims.".to_string()
    } else {
        format!("{} read in a different tone than they claim.", counted(tone_gaps.len(), "slide"))
    };

    // Alerts.
    let mut alerts = Vec::new();
    for s in &visible {
        if js::trim(&s.title).is_empty() {
            alerts.push(HealthAlert::on(&s.id, Level::Error, "Slide is missing a title", None));
        }
        if js::trim(&s.body).is_empty() && js::trim(&s.notes).is_empty() {
            alerts.push(HealthAlert::on(&s.id, Level::Warning, "Slide has no body or notes", None));
        }
    }
    for id in &coh.weak_transitions {
        alerts.push(HealthAlert::on(id, Level::Warning, "Weak transition into this slide", None));
    }
    for d in &fit.deviant_slides {
        let message = if d.delta > 0.0 {
            "Intensity feels high for its position in the arc"
        } else {
            "Intensity feels low for its position in the arc"
        };
        alerts.push(HealthAlert::on(&d.slide_id, Level::Info, message, None));
    }
    // A prompt to review, not a verdict: the estimator is a designed prior.
    for g in &intensity_gaps {
        let (declared, detected) = (js::to_fixed(g.declared, 2), js::to_fixed(g.detected, 2));
        let message = if g.gap > 0.0 {
            format!(
                "Declares intensity {declared} but the language reads flat (\u{2248}{detected}) \u{2014} the narrative metadata may be flattering the prose"
            )
        } else {
            format!(
                "Reads far more intense (\u{2248}{detected}) than its declared {declared} \u{2014} consider raising its declared intensity or softening the copy"
            )
        };
        alerts.push(HealthAlert::on(&g.slide_id, Level::Warning, message, Some(AlertKind::IntensityGap)));
    }
    for g in &tone_gaps {
        let (declared, detected) = (signed(g.declared), signed(g.detected));
        let message = if g.gap > 0.0 {
            format!(
                "Declares a more upbeat tone ({declared}) than its language carries (reads \u{2248}{detected}) \u{2014} the words land graver than the intended tone"
            )
        } else {
            format!(
                "Reads more upbeat (\u{2248}{detected}) than its declared tone ({declared}) \u{2014} consider warming the declared tone or grounding the copy"
            )
        };
        alerts.push(HealthAlert::on(&g.slide_id, Level::Warning, message, Some(AlertKind::ToneGap)));
    }

    // Overall: the gaps, matches, and realized fidelity stay out of it. Read by words,
    // coherence is noise until embeddings land, and the other three carry it alone.
    let overall_score = match coh.method {
        Method::Embedding => js::round(
            pacing_score * COMPOSITE_WEIGHT_PACING
                + balance_score * COMPOSITE_WEIGHT_BALANCE
                + coh.score * COMPOSITE_WEIGHT_COHERENCE
                + arc_score * COMPOSITE_WEIGHT_ARC,
        ),
        Method::Keyword => js::round(
            (pacing_score * COMPOSITE_FALLBACK_WEIGHT_PACING
                + balance_score * COMPOSITE_FALLBACK_WEIGHT_BALANCE
                + arc_score * COMPOSITE_FALLBACK_WEIGHT_ARC)
                / (COMPOSITE_FALLBACK_WEIGHT_PACING
                    + COMPOSITE_FALLBACK_WEIGHT_BALANCE
                    + COMPOSITE_FALLBACK_WEIGHT_ARC),
        ),
    };

    let deviants = fit.deviant_slides.len();
    LocalHealthMetrics {
        overall_score,
        pacing: Pacing {
            score: js::round(pacing_score),
            current_slide_count: count,
            target_slide_count: target,
            status,
            message: pacing_message,
        },
        content_balance: ContentBalance {
            score: js::round(balance_score),
            avg_bullets_per_slide: js::round(avg_bullets * 10.0) / 10.0,
            avg_speaker_notes_length: js::round(avg_notes),
            visual_coverage: js::round(visual_coverage * 100.0) / 100.0,
            message: if balance.is_empty() { "Content is balanced".to_string() } else { balance.join(". ") },
        },
        coherence: Coherence {
            message: coherence_message(coh.method, coh.mean_sim, hard, soft),
            score: coh.score,
            weak_transitions: coh.weak_transitions,
            method: coh.method,
            pair_similarities: coh.pair_similarities,
            mean_sim: coh.mean_sim,
            std_sim: coh.std_sim,
            window_shifts: coh.window_shifts,
            breaks: coh.breaks,
            detector_counts: coh.detector_counts,
            topic_tau: coh.topic_tau,
            topic_max_delta: coh.topic_max_delta,
        },
        arc_fidelity: ArcFidelity {
            score: arc_score,
            deviant_slides: fit.deviant_slides,
            message: if deviants == 0 {
                "Intensity curve aligns with the arc".to_string()
            } else {
                format!("{} bend the intensity curve away from the arc", counted(deviants, "slide"))
            },
        },
        realized_arc_fidelity: RealizedArcFidelity {
            score: realized_score,
            declared_minus_realized: arc_score - realized_score,
        },
        intensity_gaps,
        tone_gaps,
        intensity_match: MatchReport {
            score: intensity_match.score,
            measured_slides: intensity_match.measured_slides,
            mean_abs_gap: intensity_match.mean_abs_gap,
            message: intensity_message,
        },
        tone_match: MatchReport {
            score: tone_match.score,
            measured_slides: tone_match.measured_slides,
            mean_abs_gap: tone_match.mean_abs_gap,
            message: tone_message,
        },
        alerts,
    }
}

/// What the TypeScript tests build their decks from.
#[cfg(test)]
mod fixtures {
    use super::*;

    /// A unit vector of `dim` dimensions along `ones` (`unitVec`): cosines known by construction.
    pub fn unit(dim: usize, ones: &[usize]) -> Vec<f64> {
        let mut v = vec![0.0; dim];
        for &i in ones {
            v[i] = 1.0;
        }
        let mut n: f64 = 0.0;
        for x in &v {
            n += x * x;
        }
        let n = n.sqrt();
        if n == 0.0 { v } else { v.iter().map(|x| x / n).collect() }
    }

    /// A slide as `health.test.ts` makes one (`makeSlide`): three bullets and notes past sixty
    /// characters.
    pub fn slide(id: &str) -> SlideForHealth {
        SlideForHealth {
            id: id.into(),
            title: format!("Slide {id}"),
            body: "\u{2022} point one about the topic\n\u{2022} point two about the topic\n\u{2022} point three about the topic"
                .into(),
            notes: "Detailed speaker notes that exceed sixty characters so the notesScore can max out cleanly.".into(),
            ..SlideForHealth::default()
        }
    }

    /// What a slide declares: its intensity and its beat.
    pub fn meta(intensity: Option<f64>, beat: Option<usize>) -> Option<NarrativeMeta> {
        Some(NarrativeMeta { expected_intensity: intensity, beat_index: beat, ..NarrativeMeta::default() })
    }

    /// A slide as the sensitivity suite makes one (`mk`): seven words, and notes past sixty
    /// characters.
    pub fn plain(id: &str) -> SlideForHealth {
        SlideForHealth {
            id: id.into(),
            title: format!("Slide {id}"),
            body: "alpha bravo charlie delta echo foxtrot golf".into(),
            notes: "Speaker notes long enough to clear sixty chars cleanly here.".into(),
            ..SlideForHealth::default()
        }
    }

    /// The suite's first direction, and its second, orthogonal to it.
    pub fn directions() -> (Vec<f64>, Vec<f64>) {
        (unit(8, &[0, 1]), unit(8, &[4, 5]))
    }

    /// Twelve of the suite's slides along one direction, four a beat, each at the intensity
    /// `intensity` makes of the canonical curve's at its place.
    pub fn twelve(prefix: &str, intensity: impl Fn(f64) -> f64) -> Vec<SlideForHealth> {
        (arc::canonical_curve(12).into_iter().enumerate())
            .map(|(i, v)| SlideForHealth {
                embedding: Some(directions().0),
                narrative_meta: meta(Some(intensity(v)), Some(i / 4)),
                ..plain(&format!("{prefix}{i}"))
            })
            .collect()
    }

    /// High coherence, on the curve: no break, no deviant (`cleanDeck`).
    pub fn clean_deck() -> Vec<SlideForHealth> {
        twelve("c", |v| v)
    }

    /// High coherence, flat at 0.25, far below the climax (`flatDeck`).
    pub fn flat_deck() -> Vec<SlideForHealth> {
        twelve("f", |_| 0.25)
    }

    /// On the curve but for one slide pinned low midway (`deviantDeck`).
    pub fn deviant_deck() -> Vec<SlideForHealth> {
        let mut slides = twelve("d", |v| v);
        slides[5].narrative_meta = meta(Some(0.05), Some(1));
        slides
    }

    /// Eight slides along one direction and four along another, all in one beat: a hard topic
    /// shift (`digressionDeck`).
    pub fn digression_deck() -> Vec<SlideForHealth> {
        let (a, b) = directions();
        let along = |prefix: &str, n: usize, dir: &[f64]| -> Vec<SlideForHealth> {
            (0..n)
                .map(|i| SlideForHealth {
                    embedding: Some(dir.to_vec()),
                    narrative_meta: meta(None, Some(0)),
                    ..plain(&format!("{prefix}{i}"))
                })
                .collect()
        };
        [along("p", 8, &a), along("q", 4, &b)].concat()
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{meta, slide, unit};
    use super::*;
    use serde_json::{Map, Value};
    use std::collections::BTreeSet;

    /// Twelve slides of three bullets, notes past sixty characters, one embedding, and the
    /// canonical curve's intensities, four slides a beat (`buildBalancedDeck`).
    fn balanced() -> Vec<SlideForHealth> {
        let dir = unit(6, &[0, 1]);
        (arc::canonical_curve(12).into_iter().enumerate())
            .map(|(i, v)| SlideForHealth {
                embedding: Some(dir.clone()),
                narrative_meta: meta(Some(v), Some(i / 4)),
                ..slide(&format!("b{i}"))
            })
            .collect()
    }

    fn scores(m: &LocalHealthMetrics) -> [f64; 5] {
        [m.overall_score, m.pacing.score, m.content_balance.score, m.coherence.score, m.arc_fidelity.score]
    }

    #[test]
    fn a_slide_is_evaluated_unless_hidden_or_left_out_and_a_talk_asks_for_its_slides() {
        assert!(SlideForHealth::default().eligible());
        assert!(!SlideForHealth { hidden: true, ..SlideForHealth::default() }.eligible());
        assert!(!SlideForHealth { exclude_from_evaluation: true, ..SlideForHealth::default() }.eligible());
        assert_eq!((expected_slides(None), expected_slides(Some(7)), expected_slides(Some(0))), (18, 18, 18));
        let listed: Vec<usize> = [15, 30, 45, 90, 120].into_iter().map(|m| expected_slides(Some(m))).collect();
        assert_eq!(listed, [6, 12, 18, 30, 40]);
    }

    #[test]
    fn a_balanced_deck_scores_within_0_to_100_in_whole_numbers_and_is_paced() {
        let m = metrics(&balanced(), Some(30));
        for s in scores(&m) {
            assert!(s.is_finite() && (0.0..=100.0).contains(&s) && s.fract() == 0.0, "{s}");
        }
        assert_eq!(m.pacing.status, PacingStatus::Optimal);
        assert_eq!((m.pacing.current_slide_count, m.pacing.target_slide_count), (12, 12));
        let few: Vec<SlideForHealth> = ["s0", "s1", "s2"].into_iter().map(slide).collect();
        assert_eq!(metrics(&few, Some(30)).pacing.status, PacingStatus::Under);
        let many: Vec<SlideForHealth> = (0..30).map(|i| slide(&format!("s{i}"))).collect();
        assert_eq!(metrics(&many, Some(30)).pacing.status, PacingStatus::Over);
    }

    #[test]
    fn an_empty_deck_reads_safely() {
        let m = metrics(&[], Some(30));
        assert!(scores(&m).iter().all(|s| s.is_finite()));
        let b = &m.content_balance;
        assert_eq!((b.avg_bullets_per_slide, b.avg_speaker_notes_length, b.visual_coverage), (0.0, 0.0, 0.0));
    }

    #[test]
    fn hidden_and_left_out_slides_are_not_read() {
        let mut slides = balanced();
        slides.push(SlideForHealth { hidden: true, ..slide("hidden") });
        slides.push(SlideForHealth { exclude_from_evaluation: true, ..slide("excluded") });
        let m = metrics(&slides, Some(30));
        assert_eq!(m.pacing.current_slide_count, 12);
        assert!(m.alerts.iter().all(|a| !matches!(a.slide_id.as_deref(), Some("hidden" | "excluded"))));
    }

    #[test]
    fn with_no_embeddings_coherence_reads_words_and_the_overall_score_holds() {
        let slides: Vec<SlideForHealth> = (0..12)
            .map(|i| SlideForHealth {
                title: format!("{} topic", if i % 2 == 0 { "alpha bravo" } else { "alpha charlie" }),
                narrative_meta: meta(Some(0.5), None),
                ..slide(&format!("k{i}"))
            })
            .collect();
        let m = metrics(&slides, Some(30));
        assert_eq!(m.coherence.method, Method::Keyword);
        assert!(m.overall_score.is_finite() && (0.0..=100.0).contains(&m.overall_score));
    }

    #[test]
    fn a_slide_with_no_title_or_no_words_is_alerted() {
        let mut slides = balanced();
        slides[2].title = String::new();
        let m = metrics(&slides, Some(30));
        assert!(m.alerts.iter().any(|a| a.level == Level::Error && a.slide_id.as_deref() == Some("b2")));
        let mut slides = balanced();
        (slides[1].body, slides[1].notes) = (String::new(), String::new());
        let m = metrics(&slides, Some(30));
        assert!(m.alerts.iter().any(|a| a.level == Level::Warning && a.slide_id.as_deref() == Some("b1")));
    }

    #[test]
    fn a_break_is_a_weak_transition_alerted() {
        let (a, b) = (unit(8, &[0, 1]), unit(8, &[4, 5]));
        let canonical = arc::canonical_curve(7);
        let slides: Vec<SlideForHealth> = ["a0", "a1", "a2", "a3", "b0", "b1", "b2"]
            .into_iter()
            .enumerate()
            .map(|(i, id)| SlideForHealth {
                embedding: Some(if i < 4 { a.clone() } else { b.clone() }),
                narrative_meta: meta(Some(canonical[i]), Some(0)),
                ..slide(id)
            })
            .collect();
        let m = metrics(&slides, Some(15));
        assert!(m.alerts.iter().any(|a| a.level == Level::Warning && a.slide_id.as_deref() == Some("b0")));
        assert!(m.coherence.weak_transitions.iter().any(|id| id == "b0"));
    }

    #[test]
    fn visual_coverage_counts_long_bodies() {
        let long = "this is a long enough body to clear the thirty character threshold easily";
        let slides: Vec<SlideForHealth> =
            (0..6).map(|i| SlideForHealth { body: long.into(), ..slide(&format!("v{i}")) }).collect();
        assert_eq!(metrics(&slides, Some(15)).content_balance.visual_coverage, 1.0);
        let slides: Vec<SlideForHealth> =
            (0..6).map(|i| SlideForHealth { body: "tiny".into(), ..slide(&format!("t{i}")) }).collect();
        assert_eq!(metrics(&slides, Some(15)).content_balance.visual_coverage, 0.0);
    }

    #[test]
    fn flat_prose_under_well_shaped_metadata_realizes_less_of_the_arc() {
        let curve = arc::canonical_curve(6);
        let slides: Vec<SlideForHealth> = (0..6)
            .map(|i| SlideForHealth { narrative_meta: meta(Some(curve[i]), Some(i)), ..slide(&format!("s{i}")) })
            .collect();
        let m = metrics(&slides, None);
        assert!(m.arc_fidelity.score > 85.0, "{}", m.arc_fidelity.score);
        assert!(m.realized_arc_fidelity.score < m.arc_fidelity.score);
        assert_eq!(
            m.realized_arc_fidelity.declared_minus_realized,
            m.arc_fidelity.score - m.realized_arc_fidelity.score
        );
        assert!(m.realized_arc_fidelity.declared_minus_realized > 0.0);
        let slides: Vec<SlideForHealth> = (0..5)
            .map(|i| SlideForHealth { narrative_meta: meta(Some(0.5), None), ..slide(&format!("d{i}")) })
            .collect();
        let realized = metrics(&slides, None).realized_arc_fidelity.score;
        assert!((0.0..=100.0).contains(&realized) && realized.fract() == 0.0);
    }

    // `intensityGap.test.ts`: what the words read against what the slides declare.

    const STATUS_TITLE: &str = "Q3 status update";
    const STATUS_BODY: &str = "Items: A, B, C. No changes. Routine status, presented for reference.";
    const BREAKING: &str =
        "This is a crisis. Revenue is collapsing \u{2014} a catastrophic plunge. We must act NOW or the company fails!";
    const GRAVE: &str = "Revenue is collapsing. Layoffs, a security breach, and mounting losses have pushed us toward bankruptcy and disaster.";
    const HOPEFUL: &str = "Strong growth this quarter \u{2014} a real breakthrough. The team should be proud of an outstanding, winning result.";

    /// Twelve slides on the canonical curve whose words read about as intense as they declare
    /// (`buildCleanDeck`).
    fn clean() -> Vec<SlideForHealth> {
        let dir = unit(6, &[0, 1]);
        (arc::canonical_curve(12).into_iter().enumerate())
            .map(|(i, v)| {
                let body = if v < 0.4 {
                    "Overview of the current baseline. Standard context for the existing setup, presented for reference."
                } else if v < 0.7 {
                    "Our approach improves the workflow and helps the team move forward on the plan together."
                } else {
                    "This is critical. We must act now \u{2014} the stakes are enormous and the opportunity is a breakthrough!"
                };
                SlideForHealth {
                    body: body.into(),
                    embedding: Some(dir.clone()),
                    narrative_meta: meta(Some(v), Some(i / 4)),
                    ..slide(&format!("c{i}"))
                }
            })
            .collect()
    }

    fn toned(id: &str, body: &str, valence: Option<f64>) -> SlideForHealth {
        let narrative_meta = Some(NarrativeMeta { expected_valence: valence, ..NarrativeMeta::default() });
        SlideForHealth { body: body.into(), narrative_meta, ..slide(id) }
    }

    #[test]
    fn a_gap_is_found_either_way_and_only_where_both_readings_stand() {
        let flattering = SlideForHealth {
            title: STATUS_TITLE.into(),
            body: STATUS_BODY.into(),
            notes: "Standard quarterly status notes; nothing has changed since last period.".into(),
            narrative_meta: meta(Some(0.9), Some(0)),
            ..slide("flattering")
        };
        let gaps = intensity_gaps(&[flattering]);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].slide_id, "flattering");
        assert!((gaps[0].declared - 0.9).abs() < 5e-7);
        assert!(gaps[0].gap > INTENSITY_GAP_THRESHOLD);
        let understated = SlideForHealth {
            title: "The breaking point".into(),
            body: BREAKING.into(),
            notes: "Urgent, urgent, urgent \u{2014} the threat is severe and the stakes could not be higher.".into(),
            narrative_meta: meta(Some(0.25), Some(0)),
            ..slide("understated")
        };
        let gaps = intensity_gaps(&[understated]);
        assert_eq!((gaps.len(), gaps[0].slide_id.as_str()), (1, "understated"));
        assert!(gaps[0].gap < -INTENSITY_GAP_THRESHOLD);
        assert!(intensity_gaps(&clean()).is_empty());
        // No declaration, too few words, or not evaluated: no gap.
        let undeclared = SlideForHealth {
            body: "This is a crisis! We must act now \u{2014} the stakes are enormous!".into(),
            ..slide("nometa")
        };
        assert!(intensity_gaps(&[undeclared]).is_empty());
        let sparse = SlideForHealth {
            title: "Hi".into(),
            body: String::new(),
            notes: String::new(),
            narrative_meta: meta(Some(0.9), None),
            ..slide("sparse")
        };
        assert!(intensity_gaps(&[sparse]).is_empty());
        let unread = [
            SlideForHealth {
                hidden: true,
                body: "Routine status. No changes. Reference only.".into(),
                narrative_meta: meta(Some(0.95), None),
                ..slide("hidden")
            },
            SlideForHealth {
                exclude_from_evaluation: true,
                body: "Appendix. Reference table, presented for the record.".into(),
                narrative_meta: meta(Some(0.95), None),
                ..slide("excluded")
            },
        ];
        assert!(intensity_gaps(&unread).is_empty());
        let none = Match { score: 100.0, measured_slides: 0, mean_abs_gap: 0.0 };
        assert_eq!(intensity_match(&unread), none);
        let undeclared = SlideForHealth { body: "Some ordinary words for the deck content.".into(), ..slide("nometa") };
        assert_eq!(intensity_match(&[undeclared]), none);
    }

    #[test]
    fn the_match_is_100_less_the_mean_gap() {
        let r = intensity_match(&clean());
        assert!(r.measured_slides > 0 && r.score >= 75.0, "{r:?}");
        let notes = "Standard quarterly status; nothing changed since last period.";
        let detected = intensity::detected(&[STATUS_TITLE, STATUS_BODY, notes].join("\n"), None).unwrap();
        let flat = SlideForHealth {
            title: STATUS_TITLE.into(),
            body: STATUS_BODY.into(),
            notes: notes.into(),
            narrative_meta: meta(Some(0.95), None),
            ..slide("flat")
        };
        let r = intensity_match(&[flat]);
        assert_eq!(r.measured_slides, 1);
        assert!((r.mean_abs_gap - (0.95 - detected).abs()).abs() < 5e-11);
        assert_eq!(r.score, js::round(100.0 * (1.0 - (0.95 - detected).abs())));
        assert!(r.score < 80.0);
    }

    #[test]
    fn gaps_are_alerted_in_vzestups_words() {
        let m = metrics(&clean(), Some(30));
        assert!((0.0..=100.0).contains(&m.intensity_match.score) && !m.intensity_match.message.is_empty());
        assert!(m.intensity_gaps.is_empty());
        assert!(
            !m.alerts
                .iter()
                .any(|a| a.message.contains("Declares intensity") || a.message.contains("Reads far more intense"))
        );

        let mut slides = clean();
        slides.push(SlideForHealth {
            title: STATUS_TITLE.into(),
            body: STATUS_BODY.into(),
            notes: "Standard quarterly status; nothing has changed since the last period.".into(),
            embedding: Some(unit(6, &[0, 1])),
            narrative_meta: meta(Some(0.92), Some(2)),
            ..slide("flattering")
        });
        let m = metrics(&slides, Some(30));
        assert_eq!(m.intensity_gaps.iter().map(|g| g.slide_id.as_str()).collect::<Vec<_>>(), ["flattering"]);
        let alerts: Vec<&HealthAlert> = m.alerts.iter().filter(|a| a.message.contains("Declares intensity")).collect();
        assert_eq!(alerts.len(), 1);
        assert_eq!((alerts[0].slide_id.as_deref(), alerts[0].level), (Some("flattering"), Level::Warning));
        assert!(alerts[0].message.contains("may be flattering the prose"));
        assert_eq!(alerts[0].kind, Some(AlertKind::IntensityGap));

        let hot = SlideForHealth {
            title: "The breaking point".into(),
            body: BREAKING.into(),
            notes: "Urgent \u{2014} the threat is severe and the stakes could not be higher.".into(),
            narrative_meta: meta(Some(0.2), Some(0)),
            ..slide("hot")
        };
        let m = metrics(&[hot], Some(15));
        let alerts: Vec<&HealthAlert> =
            m.alerts.iter().filter(|a| a.message.contains("Reads far more intense")).collect();
        assert_eq!((alerts.len(), alerts[0].slide_id.as_deref()), (1, Some("hot")));
        assert!(alerts[0].message.contains("consider raising its declared intensity"));
    }

    #[test]
    fn a_gap_moves_no_score() {
        let dir = unit(6, &[0, 1]);
        let toggle = |body: &str, notes: &str| SlideForHealth {
            body: body.into(),
            notes: notes.into(),
            embedding: Some(dir.clone()),
            narrative_meta: meta(Some(0.95), Some(0)),
            ..slide("toggle")
        };
        let charged = toggle(
            "\u{2022} This is a crisis we cannot ignore\n\u{2022} Revenue is collapsing \u{2014} a catastrophic plunge\n\u{2022} We must act NOW or the company fails",
            "Urgent \u{2014} the threat is severe and the stakes could not be higher for everyone.",
        );
        let flat = toggle(
            "\u{2022} Overview of the current baseline status\n\u{2022} Items listed for reference, presented as standard\n\u{2022} No changes since the previous routine update",
            "Nothing has changed since the last period; this is presented for the record only.",
        );
        let companions: Vec<SlideForHealth> = (0..5)
            .map(|i| SlideForHealth {
                body: "Our approach improves the workflow and helps the team move forward on the plan together this period."
                    .into(),
                embedding: Some(dir.clone()),
                narrative_meta: meta(Some(0.45), Some(0)),
                ..slide(&format!("x{i}"))
            })
            .collect();
        let deck = |first: SlideForHealth| [vec![first], companions.clone()].concat();
        let no_gap = metrics(&deck(charged), Some(15));
        let with_gap = metrics(&deck(flat), Some(15));
        assert!(no_gap.intensity_gaps.is_empty());
        assert_eq!(with_gap.intensity_gaps.iter().map(|g| g.slide_id.as_str()).collect::<Vec<_>>(), ["toggle"]);
        assert_eq!(scores(&with_gap), scores(&no_gap));
    }

    #[test]
    fn a_tone_gap_is_the_right_intensity_in_the_wrong_emotion() {
        let gaps = tone_gaps(&[toned("t1", GRAVE, Some(0.6))]);
        assert_eq!((gaps.len(), gaps[0].slide_id.as_str()), (1, "t1"));
        assert!((gaps[0].declared - 0.6).abs() < 5e-11);
        assert!(gaps[0].detected < 0.0 && gaps[0].gap.abs() > TONE_GAP_THRESHOLD);
        assert!(tone_gaps(&[toned("t2", HOPEFUL, Some(0.7))]).is_empty());
        assert!(
            tone_gaps(&[toned("t3", GRAVE, None), SlideForHealth { body: GRAVE.into(), ..slide("t4") }]).is_empty()
        );
        let sparse = SlideForHealth {
            title: "Hi".into(),
            body: String::new(),
            notes: String::new(),
            ..toned("t5", "", Some(0.8))
        };
        assert!(tone_gaps(&[sparse]).is_empty());
        let hidden = SlideForHealth { hidden: true, ..toned("t6", GRAVE, Some(0.6)) };
        let excluded = SlideForHealth { exclude_from_evaluation: true, ..toned("t7", GRAVE, Some(0.6)) };
        assert!(tone_gaps(&[hidden, excluded]).is_empty());

        let mut slides = clean();
        slides.push(toned("tonebad", GRAVE, Some(0.6)));
        let m = metrics(&slides, Some(30));
        assert!(!m.tone_gaps.is_empty());
        let alerts: Vec<&HealthAlert> = m.alerts.iter().filter(|a| a.kind == Some(AlertKind::ToneGap)).collect();
        assert!(!alerts.is_empty() && alerts.iter().all(|a| a.level == Level::Warning));
    }

    #[test]
    fn the_tone_match_reads_against_a_scale_twice_as_wide() {
        let none = SlideForHealth { body: "Some neutral deck content here.".into(), ..slide("n1") };
        assert_eq!(tone_match(&[none]), Match { score: 100.0, measured_slides: 0, mean_abs_gap: 0.0 });
        let agree = tone_match(&[toned("agree", HOPEFUL, Some(0.7))]);
        assert!(agree.measured_slides == 1 && agree.score >= 80.0, "{agree:?}");
        let clash = tone_match(&[toned("clash", GRAVE, Some(0.6))]);
        assert_eq!(clash.measured_slides, 1);
        assert!(clash.score < 50.0);
        assert_eq!(clash.score, js::round(100.0 * (1.0 - clash.mean_abs_gap / 2.0)));
    }

    /// Whether `actual` is `expected` to the last bit: `expected` as Vzestup's code wrote it, each
    /// number as JavaScript prints it.
    fn same(actual: &Value, expected: &Value, at: &str) {
        match (actual, expected) {
            (Value::Number(a), Value::String(e)) => {
                let e: f64 = e.parse().unwrap_or_else(|_| panic!("{at}: {e} is no number"));
                let a = a.as_f64().expect("a number");
                assert!(a.to_bits() == e.to_bits() || (a.is_nan() && e.is_nan()), "{at}: {a:?}, Vzestup's {e:?}");
            }
            (Value::Array(a), Value::Array(e)) => {
                assert_eq!(a.len(), e.len(), "{at}: its length");
                for (i, (a, e)) in a.iter().zip(e).enumerate() {
                    same(a, e, &format!("{at}[{i}]"));
                }
            }
            (Value::Object(a), Value::Object(e)) => {
                let keys = |m: &Map<String, Value>| m.keys().cloned().collect::<BTreeSet<String>>();
                assert_eq!(keys(a), keys(e), "{at}: its keys");
                for (k, e) in e {
                    same(&a[k], e, &format!("{at}.{k}"));
                }
            }
            _ => assert_eq!(actual, expected, "{at}"),
        }
    }

    fn json(value: &impl Serialize) -> Value {
        serde_json::to_value(value).expect("it serializes")
    }

    /// Vzestup's own figures, from its TypeScript under Node (`tests/vzestup/health.json`): what
    /// the estimators read of each text, in each language; every deck's metrics, drift by either
    /// allocator, arc fidelity on its arc's curve, and fidelity and coherence read with other
    /// options; and every arc's curve.
    #[test]
    fn every_figure_is_vzestups_to_the_last_bit() {
        let v: Value = serde_json::from_str(include_str!("../../tests/vzestup/health.json")).expect("the vectors read");
        let detected = v["detected"].as_array().expect("texts");
        for (i, case) in detected.iter().enumerate() {
            let (text, language) = (case["text"].as_str().unwrap(), case["language"].as_str());
            let at = format!("detected[{i}] {text:?} in {language:?}");
            same(&json(&intensity::detected(text, language)), &case["intensity"], &format!("{at}: intensity"));
            same(&json(&valence::detected(text, language)), &case["valence"], &format!("{at}: valence"));
        }
        assert_eq!(detected.len(), 140);

        let o = &v["options"];
        let options = CoherenceOptions {
            topic_shift_floor: o["topicShiftFloor"].as_f64().unwrap(),
            topic_quintile: o["topicQuintile"].as_f64().unwrap(),
            stat_break_min_std: o["statBreakMinStd"].as_f64().unwrap(),
            keyword_overlap_ceiling: o["keywordOverlapCeiling"].as_f64().unwrap(),
            window_size_divisor: o["windowSizeDivisor"].as_u64().unwrap() as usize,
            window_size_min: o["windowSizeMin"].as_u64().unwrap() as usize,
            window_size_max: o["windowSizeMax"].as_u64().unwrap() as usize,
        };
        let f = &v["fidelityOptions"];
        let fidelity_options = FidelityOptions {
            fidelity_slope: f["fidelitySlope"].as_f64().unwrap(),
            deviant_delta: f["deviantDelta"].as_f64().unwrap(),
            stake_setup: f["stakeSetup"].as_f64().unwrap(),
            stake_climax: f["stakeClimax"].as_f64().unwrap(),
            stake_close: f["stakeClose"].as_f64().unwrap(),
            intensity_of: None,
        };
        let decks = v["decks"].as_array().expect("decks");
        for deck in decks {
            let name = deck["name"].as_str().unwrap();
            let slides: Vec<SlideForHealth> = serde_json::from_value(deck["slides"].clone()).expect("the slides read");
            let length = deck["length"].as_u64().map(|l| l as u32);
            same(&json(&metrics(&slides, length)), &deck["metrics"], &format!("{name}: metrics"));
            same(&json(&drift::detect(&slides, Some(1))), &deck["drift"][0], &format!("{name}: drift by v1"));
            same(&json(&drift::detect(&slides, Some(2))), &deck["drift"][1], &format!("{name}: drift by v2"));
            let visible: Vec<SlideForHealth> = slides.iter().filter(|s| s.eligible()).cloned().collect();
            let curve = arc::curve(deck["arc"].as_str(), visible.len());
            let fit = arc::fidelity(&slides, &curve, &FidelityOptions::default());
            same(&json(&fit), &deck["arcFidelity"], &format!("{name}: fidelity on its arc"));
            let fit = arc::fidelity(&slides, &curve, &fidelity_options);
            same(&json(&fit), &deck["arcFidelityWithOptions"], &format!("{name}: fidelity, other options"));
            let coherence = coherence::analyze(&visible, &options);
            same(&json(&coherence), &deck["coherenceWithOptions"], &format!("{name}: coherence, other options"));
        }
        assert_eq!(decks.len(), 8);

        for case in v["curves"].as_array().expect("curves") {
            let (name, count) = (case["arc"].as_str(), case["count"].as_u64().unwrap() as usize);
            same(&json(&arc::curve(name, count)), &case["curve"], &format!("{name:?}'s curve of {count}"));
        }
        for case in v["canonical"].as_array().expect("curves") {
            let count = case["count"].as_u64().unwrap() as usize;
            same(&json(&arc::canonical_curve(count)), &case["curve"], &format!("the canonical curve of {count}"));
        }
        for case in v["beatNames"].as_array().expect("beat names") {
            let name = case["arc"].as_str();
            same(&json(&arc::beat_names(name)), &case["names"], &format!("{name:?}'s beats"));
        }
    }
}
