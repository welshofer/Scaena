//! Arc selection with no model (Narrato's `selectNarrativeArcHeuristic`): each arc scored by its
//! weights for the brief's goal (0.4), audience (0.3), and length (0.3), a weight it lacks 0.5.
//! What a failed arc-selection call falls back to.

use crate::arcs;
use crate::brief::Brief;
use crate::js;
use serde::{Deserialize, Serialize};

/// The arc chosen, why, and the candidates weighed, best first, each with its score (0–1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArcChoice {
    pub selection: String,
    pub reasoning: String,
    pub candidates: Vec<Candidate>,
}

/// An arc weighed, and its overall score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub arc: String,
    pub score: f64,
}

/// The heuristic's choice for `brief`.
pub fn choose(brief: &Brief) -> ArcChoice {
    let weight = |w: &std::collections::BTreeMap<String, f64>, key: &str| w.get(key).copied().unwrap_or(0.5);
    let mut scored: Vec<Candidate> = arcs::arcs()
        .iter()
        .map(|a| Candidate {
            arc: a.arc.clone(),
            score: weight(&a.goal_weights, &brief.goal) * 0.4
                + weight(&a.audience_weights, &brief.audience) * 0.3
                + weight(&a.length_weights, &brief.length) * 0.3,
        })
        .collect();
    // Best first, ties in catalog order: a stable insertion sort, as JavaScript's `sort` is
    // stable; 13 arcs.
    for i in 1..scored.len() {
        let mut j = i;
        while j > 0 && scored[j - 1].score < scored[j].score {
            scored.swap(j - 1, j);
            j -= 1;
        }
    }
    let candidates: Vec<Candidate> = scored.into_iter().take(5).collect();
    let listed: Vec<String> =
        candidates.iter().map(|c| format!("{} ({}%)", c.arc, js::round(c.score * 100.0) as i64)).collect();
    ArcChoice {
        selection: candidates[0].arc.clone(),
        reasoning: format!("Heuristic best-fit. Top candidates: {}.", listed.join(", ")),
        candidates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vzestup's own vectors (Narrato's Appendix F.2): goal | audience | length | selection |
    /// reasoning.
    #[test]
    fn the_heuristic_chooses_as_vzestups_does() {
        let vectors = include_str!("../tests/vzestup/f2-heuristic.txt");
        let mut n = 0;
        for line in vectors.lines().filter(|l| !l.is_empty()) {
            let parts: Vec<&str> = line.split(" | ").collect();
            let [goal, audience, length, selection, reasoning] = parts[..] else { panic!("a vector: {line}") };
            let brief = Brief {
                topic: "x".into(),
                goal: goal.into(),
                audience: audience.into(),
                length: length.into(),
                ..Brief::default()
            };
            let chosen = choose(&brief);
            assert_eq!(chosen.selection, selection, "{line}");
            assert_eq!(chosen.reasoning, reasoning, "{line}");
            n += 1;
        }
        assert_eq!(n, 27);
    }
}
