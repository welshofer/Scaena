//! How long a slide takes to present (Narrato's `calculateSlideDuration`): its notes, body, and
//! title spoken at 145 words a minute, a quarter more for a chart, a table, or a diagram and a
//! tenth more for a picture, and five seconds to change slides; a quarter minute at least.

const WORDS_PER_MINUTE: f64 = 145.0;
const TRANSITION_SECONDS: f64 = 5.0;
const HIGH: [&str; 16] = [
    "chart",
    "graph",
    "data",
    "table",
    "code",
    "architecture",
    "diagram",
    "statistics",
    "analysis",
    "financial",
    "math",
    "equation",
    "flowchart",
    "process",
    "workflow",
    "algorithm",
];
const MEDIUM: [&str; 10] =
    ["image", "photo", "visual", "timeline", "map", "illustration", "split", "comparison", "screenshot", "mockup"];

/// Seconds to present a slide with these words, in a structural `layout`, calling for `visual`.
pub fn seconds(notes: &str, body: &str, title: &str, layout: &str, visual: &str) -> f64 {
    let spoken = format!("{notes} {body} {title}");
    let words = spoken.split(crate::js::is_space).filter(|w| !w.is_empty()).count() as f64;
    let seen = format!("{layout} {visual} {title}").to_lowercase();
    let factor = if HIGH.iter().any(|t| seen.contains(t)) {
        0.25
    } else if MEDIUM.iter().any(|t| seen.contains(t)) || !visual.is_empty() {
        0.1
    } else {
        0.0
    };
    let minutes = (words * (1.0 + factor) / WORDS_PER_MINUTE + TRANSITION_SECONDS / 60.0).max(0.25);
    crate::js::round(minutes * 60.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slide_takes_as_long_as_its_words() {
        // 29 words at 145 a minute, and the change: 12 + 5 seconds.
        let notes = "one two three four five six seven eight nine ten ".repeat(2) + "a b c d e f g h i";
        assert_eq!(seconds(&notes, "", "", "body", ""), 17.0);
        // A chart's words take a quarter longer.
        assert_eq!(seconds(&notes, "", "", "bar-chart", ""), 20.0);
        assert_eq!(seconds("", "", "", "title", ""), 15.0);
    }
}
