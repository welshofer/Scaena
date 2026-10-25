//! The model's answers read back (Narrato's `parseArcJson`, `normalizeSlide`, and partial
//! recovery): the arc it chose, snapped to the catalog; and the slides, each normalized, those
//! complete in an answer cut short kept.

use crate::arcs;
use crate::heuristic::{ArcChoice, Candidate};
use crate::js;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One slide of the narrative, as Narrato normalizes it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Slide {
    pub slide_title: String,
    pub subtitle: String,
    /// The catalog's name for its layout, as the model wrote it.
    pub layout_suggestion: String,
    pub key_bullets: Vec<String>,
    /// The picture it calls for.
    pub visual_description: String,
    pub speaker_notes: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supporting_facts: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_data: Option<ChartData>,
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
    /// A word for each bullet naming a pictogram of it (Scaena's, PLAN G.23): `rocket`, `shield`,
    /// `leaf`. Where a slide's bullets are parallel features, values, or parts, the deck sets each
    /// under the icon its word finds.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub icons: Vec<String>,
    /// The name of the source the slide's facts come from, as the prompt's `=== SOURCE: … ===`
    /// names it (Scaena's, narrative FUNC-3): what its beat's evidence cites.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
}

/// The exact figures a chart plots.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartData {
    pub categories: Vec<String>,
    pub series: Vec<Series>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// The source the figures came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
}

/// One series of a chart: its name, and a value for each category.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Series {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub values: Vec<f64>,
}

/// The slides an answer holds: the executive summary, the slides, and whether they were
/// recovered from an answer cut short.
#[derive(Debug, Clone, PartialEq)]
pub struct Slides {
    pub executive_summary: String,
    pub slides: Vec<Slide>,
    pub recovered: bool,
}

/// `text` without a code fence around it, where a model wrote one.
pub(crate) fn unfenced(text: &str) -> &str {
    let t = js::trim(text);
    let Some(rest) = t.strip_prefix("```") else { return t };
    let rest = rest.trim_start_matches(|c: char| c.is_ascii_alphanumeric());
    js::trim(rest.strip_suffix("```").unwrap_or(rest))
}

/// The arc an answer to arc selection chose (Narrato's `parseArcJson`): one the catalog does
/// not name exactly is matched as a model writes it ([`arcs::named`]: case, dashes, and
/// apostrophes aside, or by a part). Text that does not read as the answer is an error, and so
/// is a name no arc of the catalog is matched by (narrative STAB-5): Narrato's `parseArcJson`
/// made it the catalog's first in silence; here the error says the name, and the generation
/// falls back to the heuristic's choice for the brief, saying so, as it does when the call fails.
pub fn arc(text: &str) -> Result<ArcChoice, String> {
    let parsed: Value = serde_json::from_str(unfenced(text)).map_err(|e| format!("arc selection did not read: {e}"))?;
    let selection = parsed.get("selection").and_then(Value::as_str).ok_or("arc selection named no arc")?;
    let selection = match arcs::named(selection) {
        Some(a) => a.arc.clone(),
        None => return Err(format!("the model named `{selection}`, not an arc of the catalog")),
    };
    let candidates = (parsed.get("candidates").and_then(Value::as_array).into_iter().flatten())
        .map(|c| Candidate {
            arc: c.get("arc").and_then(Value::as_str).unwrap_or_default().to_string(),
            score: c.pointer("/score/overall").and_then(Value::as_f64).unwrap_or(0.0),
        })
        .collect();
    let reasoning = parsed.get("reasoning").and_then(Value::as_str).unwrap_or_default().to_string();
    Ok(ArcChoice { selection, reasoning, candidates })
}

/// The slides an answer to generation holds: the whole document where it reads, else each slide
/// complete in an answer cut short, with the summary where its string closed. None at all is an
/// error.
pub fn slides(text: &str) -> Result<Slides, String> {
    let text = unfenced(text);
    if let Ok(parsed) = serde_json::from_str::<Value>(text) {
        let slides: Vec<Slide> =
            parsed.get("slides").and_then(Value::as_array).into_iter().flatten().map(normalize).collect();
        if slides.is_empty() {
            return Err("the model wrote no slides".into());
        }
        let summary = parsed.get("executiveSummary").and_then(Value::as_str).unwrap_or_default().to_string();
        return Ok(Slides { executive_summary: summary, slides, recovered: false });
    }
    let slides: Vec<Slide> = complete_slides(text).iter().map(normalize).collect();
    if slides.is_empty() {
        return Err("the model's answer held no complete slide".into());
    }
    Ok(Slides { executive_summary: string_property(text, "executiveSummary"), slides, recovered: true })
}

/// A slide as the model wrote it, normalized (Narrato's `normalizeSlide`): strings trimmed, a
/// layout it left out a Bullet List, intensity held to 0–1 and tone to −1–1, chart data with
/// what does not read taken out. A null, as OpenAI's strict mode writes an optional field it
/// leaves out, is absent.
pub fn normalize(raw: &Value) -> Slide {
    let text = |key: &str| raw.get(key).and_then(Value::as_str).map(|s| js::trim(s).to_string()).unwrap_or_default();
    let some = |key: &str| Some(text(key)).filter(|s| !s.is_empty());
    let finite = |key: &str| raw.get(key).and_then(Value::as_f64).filter(|v| v.is_finite());
    let index = |key: &str| finite(key).map(|v| v.max(0.0).floor() as usize);
    let layout = text("layoutSuggestion");
    Slide {
        slide_title: text("slideTitle"),
        subtitle: text("subtitle"),
        layout_suggestion: if layout.is_empty() { "Bullet List".into() } else { layout },
        key_bullets: (raw.get("keyBullets").and_then(Value::as_array).into_iter().flatten())
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        visual_description: text("visualDescription"),
        speaker_notes: text("speakerNotes"),
        supporting_facts: raw.get("supportingFacts").and_then(Value::as_str).map(|s| js::trim(s).to_string()),
        chart_data: raw.get("chartData").and_then(chart_data),
        beat_index: index("beatIndex"),
        beat_name: some("beatName"),
        purpose: some("purpose"),
        intent: some("intent"),
        expected_intensity: finite("expectedIntensity").map(|v| v.clamp(0.0, 1.0)),
        expected_valence: finite("expectedValence").map(|v| v.clamp(-1.0, 1.0)),
        icons: (raw.get("icons").and_then(Value::as_array).into_iter().flatten())
            .filter_map(Value::as_str)
            .map(|s| js::trim(s).to_string())
            .collect(),
        source_ref: some("sourceRef"),
    }
}

/// Chart data as Narrato's `normalizeChartData` keeps it: categories that are words, series with
/// a finite value at least, each series' name where it has one; none where no category or no
/// series is left.
fn chart_data(raw: &Value) -> Option<ChartData> {
    let words = |v: &Value| v.as_str().map(js::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let categories: Vec<String> =
        raw.get("categories").and_then(Value::as_array).into_iter().flatten().filter_map(words).collect();
    let series: Vec<Series> = (raw.get("series").and_then(Value::as_array).into_iter().flatten())
        .map(|s| Series {
            name: s.get("name").and_then(words),
            values: (s.get("values").and_then(Value::as_array).into_iter().flatten())
                .filter_map(Value::as_f64)
                .filter(|v| v.is_finite())
                .collect(),
        })
        .filter(|s| !s.values.is_empty())
        .collect();
    if categories.is_empty() || series.is_empty() {
        return None;
    }
    Some(ChartData {
        categories,
        series,
        unit: raw.get("unit").and_then(words),
        source_id: raw.get("sourceId").and_then(words),
    })
}

/// Each slide object complete in `text`, an answer cut short: the objects at the top of the
/// `slides` array that closed.
fn complete_slides(text: &str) -> Vec<Value> {
    let Some(key) = text.find("\"slides\"") else { return Vec::new() };
    let Some(open) = text[key..].find('[').map(|p| p + key) else { return Vec::new() };
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let (mut depth, mut start, mut in_string, mut escaped) = (0usize, None, false, false);
    for (i, &b) in bytes.iter().enumerate().skip(open + 1) {
        if in_string {
            match b {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0
                    && let Some(s) = start.take()
                    && let Ok(slide) = serde_json::from_str::<Value>(&text[s..=i])
                {
                    out.push(slide);
                }
            }
            b']' if depth == 0 => break,
            _ => {}
        }
    }
    out
}

/// The string `key` holds in `text`, an answer cut short, where its string closed.
fn string_property(text: &str, key: &str) -> String {
    let quoted = format!("\"{key}\"");
    let mut from = 0;
    while let Some(at) = text[from..].find(&quoted).map(|p| p + from) {
        let rest = text[at + quoted.len()..].trim_start();
        if let Some(value) = rest.strip_prefix(':') {
            let value = value.trim_start();
            let mut chars = value.char_indices();
            if !matches!(chars.next(), Some((_, '"'))) {
                return String::new();
            }
            let mut escaped = false;
            for (i, c) in chars {
                match c {
                    _ if escaped => escaped = false,
                    '\\' => escaped = true,
                    '"' => return serde_json::from_str(&value[..=i]).unwrap_or_default(),
                    _ => {}
                }
            }
            return String::new();
        }
        from = at + quoted.len();
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_arc_named_loosely_is_snapped_to_the_catalog() {
        let chosen = arc(r#"{"selection":"data storytelling","reasoning":"Numbers.","candidates":[{"arc":"Data Storytelling","score":{"overall":0.9}}]}"#).unwrap();
        assert_eq!(chosen.selection, "Data Storytelling");
        assert_eq!(chosen.candidates[0].score, 0.9);
        // As a model writes a name (narrative STAB-5): a hyphen for the en dash, a straight
        // apostrophe for the curly one, the arrows left out, a part of the name, or more around it.
        let snapped =
            |selection: &str| arc(&format!(r#"{{"selection":"{selection}","reasoning":"","candidates":[]}}"#));
        for (written, catalog) in [
            ("Problem-Solution", "Problem\u{2013}Solution"),
            ("problem solution", "Problem\u{2013}Solution"),
            ("Hero's Journey for Change", "Hero\u{2019}s Journey for Change"),
            ("Hero's Journey", "Hero's Journey"),
            ("PARL", "PARL (Problem, Action, Result, Learning)"),
            ("What Is / What Could Be", "What Is \u{2192} What Could Be"),
            ("Conflict - Struggle - Resolution", "Conflict \u{2192} Struggle \u{2192} Resolution"),
            ("Roadmap", "Roadmap / Future-State"),
            ("The Data Storytelling arc", "Data Storytelling"),
        ] {
            assert_eq!(snapped(written).unwrap().selection, catalog, "{written}");
        }
        // A name no arc of the catalog is matched by is an error that says it, never the first
        // arc in silence.
        let unknown = arc("```json\n{\"selection\":\"Kishōtenketsu\",\"reasoning\":\"\",\"candidates\":[]}\n```");
        assert_eq!(unknown.unwrap_err(), "the model named `Kishōtenketsu`, not an arc of the catalog");
        assert!(snapped("The").is_err(), "a word too short to name an arc names none");
        assert!(arc("not json").is_err());
    }

    #[test]
    fn a_slide_is_normalized_as_narrato_normalizes_it() {
        let s = normalize(&json!({
            "slideTitle": "  Emissions per lane, 2023–2025 ",
            "keyBullets": ["Down 41%", 3],
            "chartData": { "categories": ["2023", "2024", " ", "2025"], "series": [{ "name": "kt CO2", "values": [100, 80, "x", 59] }, { "values": [] }], "unit": "kt" },
            "beatIndex": 4.7,
            "expectedIntensity": 1.4,
            "expectedValence": -3,
            "supportingFacts": null,
            "purpose": "  ",
        }));
        assert_eq!(s.slide_title, "Emissions per lane, 2023–2025");
        assert_eq!(s.layout_suggestion, "Bullet List");
        assert_eq!(s.key_bullets, ["Down 41%"]);
        let chart = s.chart_data.unwrap();
        assert_eq!(chart.categories, ["2023", "2024", "2025"]);
        assert_eq!(chart.series, [Series { name: Some("kt CO2".into()), values: vec![100.0, 80.0, 59.0] }]);
        assert_eq!((s.beat_index, s.expected_intensity, s.expected_valence), (Some(4), Some(1.0), Some(-1.0)));
        assert_eq!((s.supporting_facts, s.purpose), (None, None));
        assert!(s.icons.is_empty(), "a slide that names no icon has none");
    }

    /// Scaena's `icons` (PLAN G.23): a word for each bullet, each trimmed, what is not a word left
    /// out; and a slide that names none, as OpenAI's strict mode writes it, null.
    #[test]
    fn a_slides_icon_words_are_read_as_written() {
        let s =
            normalize(&json!({ "keyBullets": ["Fast: a day", "Safe: sealed"], "icons": [" rocket ", "shield", 3] }));
        assert_eq!(s.icons, ["rocket", "shield"]);
        assert!(normalize(&json!({ "icons": null })).icons.is_empty());
    }

    /// Scaena's `sourceRef` (narrative FUNC-3): the source's name, trimmed; none where it is left
    /// out, blank, or null.
    #[test]
    fn a_slides_source_is_read_as_named() {
        let s = normalize(&json!({ "slideTitle": "Shipping", "sourceRef": " notes.md " }));
        assert_eq!(s.source_ref.as_deref(), Some("notes.md"));
        for none in [json!({}), json!({ "sourceRef": "  " }), json!({ "sourceRef": null })] {
            assert_eq!(normalize(&none).source_ref, None, "{none}");
        }
    }

    #[test]
    fn an_answer_cut_short_keeps_its_complete_slides() {
        let whole = slides(r#"{"executiveSummary":"Sum","slides":[{"slideTitle":"A"},{"slideTitle":"B"}]}"#).unwrap();
        assert_eq!((whole.slides.len(), whole.recovered, whole.executive_summary.as_str()), (2, false, "Sum"));
        let cut = slides(r#"{"executiveSummary":"A \"quoted\" sum","slides":[{"slideTitle":"A {x}","keyBullets":["]"]},{"slideTitle":"B"},{"slideTitle":"C","keyBu"#).unwrap();
        assert_eq!(cut.slides.iter().map(|s| s.slide_title.as_str()).collect::<Vec<_>>(), ["A {x}", "B"]);
        assert_eq!((cut.recovered, cut.executive_summary.as_str()), (true, "A \"quoted\" sum"));
        assert!(slides(r#"{"executiveSummary":"x","slides":[{"slideT"#).is_err());
        assert!(slides(r#"{"executiveSummary":"x","slides":[]}"#).is_err());
    }
}
