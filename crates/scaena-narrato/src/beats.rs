//! How a deck's slides spread over its arc's beats (Narrato's beat allocator, v2): the plan the
//! generation prompt asks the model to follow, and the beat a slide at a place in the deck
//! belongs to.

use crate::arcs::{NarrativeArc, Persona};

/// `slides` slides over `beats` beats: ⌊N/B⌋ each, the rest handed out one per beat from the
/// last beat back. With fewer slides than beats, the first N beats take one each and the rest
/// none, so the deck opens on the arc's first beats.
pub fn allocate(slides: usize, beats: usize) -> Vec<usize> {
    if beats == 0 {
        return Vec::new();
    }
    if slides == 0 {
        return vec![0; beats];
    }
    let base = slides / beats;
    if base == 0 {
        let mut counts = vec![0; beats];
        counts[..slides].fill(1);
        return counts;
    }
    let mut counts = vec![base; beats];
    let mut remainder = slides - base * beats;
    for count in counts.iter_mut().rev() {
        if remainder == 0 {
            break;
        }
        *count += 1;
        remainder -= 1;
    }
    counts
}

/// The beat the slide at `index` (from 0) of `count` belongs to, as [`allocate`] spreads them.
pub fn beat_of(index: usize, count: usize, beats: usize) -> usize {
    if beats == 0 || count == 0 {
        return 0;
    }
    let mut cumulative = 0;
    for (b, n) in allocate(count, beats).into_iter().enumerate() {
        cumulative += n;
        if index < cumulative {
            return b;
        }
    }
    beats - 1
}

/// Each beat's slides, numbered from 1: `(beat, first, last)`, `last` below `first` where a
/// beat has none at this length.
pub fn plan(arc: &NarrativeArc, target: usize) -> Vec<(usize, usize, usize)> {
    let mut cursor = 1;
    let mut out = Vec::with_capacity(arc.beats.len());
    for (beat, count) in allocate(target, arc.beats.len()).into_iter().enumerate() {
        // A beat with no slides reads `cursor - 1` as its last, as Vzestup's `slideRange` does.
        out.push((beat, cursor, cursor + count - 1));
        cursor += count;
    }
    out
}

/// The plan as the prompt writes it: `  Beat 2 "Impact: …" → slides 3–4` (an en dash), or
/// `→ (no slides at this length)`.
pub fn format_plan(arc: &NarrativeArc, plan: &[(usize, usize, usize)]) -> String {
    let lines: Vec<String> = plan
        .iter()
        .map(|&(beat, from, to)| {
            let range =
                if to < from { "(no slides at this length)".to_string() } else { format!("slides {from}\u{2013}{to}") };
            format!("  Beat {} \"{}\" \u{2192} {range}", beat + 1, arc.beats[beat])
        })
        .collect();
    lines.join("\n")
}

/// The arc's beats as the prompt's Structure lists them (Narrato's `getAdaptedBeats`): each
/// numbered, then what the audience's attention asks of the pacing. No arc carries goal-specific
/// beat guidance, so none is added for the goal.
pub fn adapted(arc: &NarrativeArc, persona: Option<&Persona>) -> String {
    let lines: Vec<String> = arc.beats.iter().enumerate().map(|(i, beat)| format!("{}. {beat}", i + 1)).collect();
    let mut result = lines.join("\n");
    if let Some(persona) = persona {
        let span = persona.attention.average_span;
        if span < 8.0 {
            result.push_str("\n- COMPRESS early setup beats; get to the core message quickly");
        } else if span > 15.0 {
            result.push_str("\n- EXPAND key beats with additional detail and examples");
        }
        if persona.attention.visual_preference == "visual-heavy" {
            result.push_str("\n- Allow extra slide time at visual-heavy beats for imagery to resonate");
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arcs;

    /// Vzestup's own vectors (Narrato's Appendix F).
    #[test]
    fn slides_spread_over_beats_as_vzestup_spreads_them() {
        let cases: [(usize, [&[usize]; 4]); 7] = [
            (3, [&[1, 1, 1, 0], &[1, 1, 1, 0, 0], &[1, 1, 1, 0, 0, 0], &[1, 1, 1, 0, 0, 0, 0]]),
            (5, [&[1, 1, 1, 2], &[1, 1, 1, 1, 1], &[1, 1, 1, 1, 1, 0], &[1, 1, 1, 1, 1, 0, 0]]),
            (8, [&[2, 2, 2, 2], &[1, 1, 2, 2, 2], &[1, 1, 1, 1, 2, 2], &[1, 1, 1, 1, 1, 1, 2]]),
            (13, [&[3, 3, 3, 4], &[2, 2, 3, 3, 3], &[2, 2, 2, 2, 2, 3], &[1, 2, 2, 2, 2, 2, 2]]),
            (20, [&[5, 5, 5, 5], &[4, 4, 4, 4, 4], &[3, 3, 3, 3, 4, 4], &[2, 3, 3, 3, 3, 3, 3]]),
            (32, [&[8, 8, 8, 8], &[6, 6, 6, 7, 7], &[5, 5, 5, 5, 6, 6], &[4, 4, 4, 5, 5, 5, 5]]),
            (45, [&[11, 11, 11, 12], &[9, 9, 9, 9, 9], &[7, 7, 7, 8, 8, 8], &[6, 6, 6, 6, 7, 7, 7]]),
        ];
        for (n, by_beats) in cases {
            for (b, want) in (4..=7).zip(by_beats) {
                assert_eq!(allocate(n, b), want, "N={n} B={b}");
            }
        }
        let beats: Vec<usize> = (0..13).map(|i| beat_of(i, 13, 7)).collect();
        assert_eq!(beats, [0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6]);
    }

    #[test]
    fn the_plan_reads_as_the_prompt_writes_it() {
        let arc = arcs::arc("Problem\u{2013}Solution");
        let text = format_plan(arc, &plan(arc, 13));
        assert!(
            text.starts_with("  Beat 1 \"Problem: Define core challenge and stakes\" \u{2192} slides 1\u{2013}2\n")
        );
        assert!(text.ends_with("  Beat 6 \"Call to Action: Next steps\" \u{2192} slides 11\u{2013}13"));
        let short = format_plan(arc, &plan(arc, 3));
        assert!(short.ends_with("\"Call to Action: Next steps\" \u{2192} (no slides at this length)"));
    }
}
