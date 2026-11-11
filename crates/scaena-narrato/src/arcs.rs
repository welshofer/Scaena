//! The arcs and the audiences Narrato writes for (ADR-0026): Vzestup's own data, extracted as
//! JSON beside this crate. The arcs keep Vzestup's order, which is the order the arc-selection
//! catalog lists them in, the heuristic breaks ties by, and an unknown name falls back to the
//! first of.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// One of the 13 canonical narrative arcs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NarrativeArc {
    /// Its name, exactly: the model echoes it, and its curve is looked up by it.
    pub arc: String,
    pub category: String,
    pub definition: String,
    pub premise: String,
    /// Each beat as "Name: what it does".
    pub beats: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_for: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risks: Option<String>,
    pub llm_hints: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enhanced_llm_hints: Vec<String>,
    /// The heuristic's weights, by goal, audience, and length; a key missing is 0.5.
    #[serde(default)]
    pub goal_weights: BTreeMap<String, f64>,
    #[serde(default)]
    pub audience_weights: BTreeMap<String, f64>,
    #[serde(default)]
    pub length_weights: BTreeMap<String, f64>,
    /// Its intensity curve, 100 points from the first slide to the last (narrative health).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<Point>,
    /// What else Vzestup keeps of it (its embedding, phases, slide patterns), as it is.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

/// A point of an arc's intensity curve: its place in the deck, 0 to 100, and its intensity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl NarrativeArc {
    /// Each beat's name, the part of it before its first colon.
    pub fn beat_names(&self) -> Vec<&str> {
        self.beats.iter().map(|b| b.split(':').next().unwrap_or(b).trim()).collect()
    }
}

/// An audience Narrato tailors a deck to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Persona {
    pub id: String,
    /// The audience's label, as the brief names it ("Investors").
    pub name: String,
    pub attention: Attention,
    /// What else Vzestup keeps of it (its vocabulary, goal profiles, hints), as it is.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

/// How an audience attends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attention {
    /// Minutes.
    pub average_span: f64,
    pub preferred_slide_count: u32,
    #[serde(default)]
    pub break_frequency: Option<f64>,
    /// "balanced", "visual-heavy", or "text-heavy".
    pub visual_preference: String,
}

/// The 13 arcs, in Vzestup's order.
pub fn arcs() -> &'static [NarrativeArc] {
    static ARCS: OnceLock<Vec<NarrativeArc>> = OnceLock::new();
    ARCS.get_or_init(|| serde_json::from_str(include_str!("../data/arcs.json")).expect("the arcs read"))
}

/// The 7 audiences, in Vzestup's order.
pub fn personas() -> &'static [Persona] {
    static PERSONAS: OnceLock<Vec<Persona>> = OnceLock::new();
    PERSONAS.get_or_init(|| serde_json::from_str(include_str!("../data/personas.json")).expect("the personas read"))
}

/// The arc named `name` exactly, else the first (Hero's Journey), as Vzestup falls back.
pub fn arc(name: &str) -> &'static NarrativeArc {
    named(name).unwrap_or(&arcs()[0])
}

/// The arc `name` names, matched as a person or a model writes it (narrative STAB-5): case
/// aside; a hyphen for an en dash or an arrow; a straight apostrophe for a curly one; the
/// spaces, slashes, and parentheses as they fall; and by a part, where `name` holds a catalog
/// name or starts one ("PARL", "the Problem-Solution arc"). Several matched, the longest name
/// wins, so "Hero's Journey for Change" is not "Hero's Journey". None where no catalog name is
/// so matched, which the caller says.
pub fn named(name: &str) -> Option<&'static NarrativeArc> {
    matched(arcs(), name, |a| &a.arc)
}

/// What of `all` `name` names, by `named`'s rules, each known by what `arc` gives of it.
fn matched<'a, T>(all: &'a [T], name: &str, arc: fn(&T) -> &str) -> Option<&'a T> {
    if let Some(exact) = all.iter().find(|a| arc(a) == name) {
        return Some(exact);
    }
    let wanted = key(name);
    if wanted.len() < 4 {
        return None;
    }
    if let Some(same) = all.iter().find(|a| key(arc(a)) == wanted) {
        return Some(same);
    }
    (all.iter())
        .map(|a| (a, key(arc(a))))
        .filter(|(_, k)| wanted.contains(k.as_str()) || k.starts_with(&wanted))
        .max_by_key(|(_, k)| k.len())
        .map(|(a, _)| a)
}

/// An arc's shape: its name, its beats' names, and its intensity curve, which is all narrative
/// health and a deck's making read of it. They come from `data/shapes.json`, which holds them as
/// `data/arcs.json` does (`the_shapes_are_the_arcs`), so the editor's module, which lays a deck
/// out and reads its health, carries them without the catalog's words: the prompts that read
/// those are the generator's module's (SPEC §15).
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    /// Its name, exactly, as [`NarrativeArc::arc`].
    pub arc: String,
    /// Each beat's name, as [`NarrativeArc::beat_names`] gives them.
    pub beats: Vec<String>,
    /// Its intensity curve, as [`NarrativeArc::points`].
    pub points: Vec<Point>,
}

/// An arc's shape as `data/shapes.json` writes it: its curve's intensities alone, a point for each
/// place from 0, as every arc's points stand.
#[derive(Serialize, Deserialize)]
struct Written {
    arc: String,
    beats: Vec<String>,
    curve: Vec<f64>,
}

/// The 13 arcs' shapes, in Vzestup's order.
pub fn shapes() -> &'static [Shape] {
    static SHAPES: OnceLock<Vec<Shape>> = OnceLock::new();
    SHAPES.get_or_init(|| {
        let written: Vec<Written> = serde_json::from_str(include_str!("../data/shapes.json")).expect("the shapes read");
        (written.into_iter())
            .map(|w| Shape {
                arc: w.arc,
                beats: w.beats,
                points: (w.curve.iter().enumerate()).map(|(x, &y)| Point { x: x as f64, y }).collect(),
            })
            .collect()
    })
}

/// The shape of the arc `name` names, matched as [`named`] matches it.
pub fn shape_named(name: &str) -> Option<&'static Shape> {
    matched(shapes(), name, |a| &a.arc)
}

/// The shape of the arc `name` names, else the first's, as [`arc`] falls back.
pub fn shape(name: &str) -> &'static Shape {
    shape_named(name).unwrap_or(&shapes()[0])
}

/// `name` as it is compared: its letters and digits alone, lowercase, so punctuation, spacing,
/// and the kind of dash or apostrophe fall away.
fn key(name: &str) -> String {
    name.chars().filter(char::is_ascii_alphanumeric).flat_map(char::to_lowercase).collect()
}

/// The persona a brief names by id, else the first whose name holds the audience or is held
/// by it, case aside; none where neither matches, and the prompt says nothing of attention.
pub fn persona(id: Option<&str>, audience: &str) -> Option<&'static Persona> {
    let all = personas();
    if let Some(found) = id.and_then(|id| all.iter().find(|p| p.id == id)) {
        return Some(found);
    }
    if audience.is_empty() {
        return None;
    }
    let lc = audience.to_lowercase();
    all.iter().find(|p| {
        let name = p.name.to_lowercase();
        name.contains(&lc) || lc.contains(&name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `data/shapes.json` should hold, from `data/arcs.json`.
    fn written() -> String {
        let shapes: Vec<Written> = (arcs().iter())
            .map(|a| Written {
                arc: a.arc.clone(),
                beats: a.beat_names().iter().map(|b| b.to_string()).collect(),
                curve: a.points.iter().map(|p| p.y).collect(),
            })
            .collect();
        let lines: Vec<String> = shapes.iter().map(|s| serde_json::to_string(s).unwrap()).collect();
        format!("[\n{}\n]\n", lines.join(",\n"))
    }

    /// The shapes are the catalog's arcs, their names, beats, and curves, so health and a deck's
    /// making read them as they read the arcs. `SCAENA_BLESS=1` writes them again.
    #[test]
    fn the_shapes_are_the_arcs() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/data/shapes.json");
        if std::env::var_os("SCAENA_BLESS").is_some() {
            std::fs::write(path, written()).unwrap();
        }
        assert_eq!(std::fs::read_to_string(path).unwrap(), written(), "data/shapes.json: `just bless` writes it again");
        assert_eq!(shapes().len(), arcs().len());
        for (shape, arc) in shapes().iter().zip(arcs()) {
            assert_eq!(shape.arc, arc.arc);
            assert_eq!(shape.points, arc.points, "{}'s curve stands a point at each place from 0", arc.arc);
            assert_eq!(shape.beats, arc.beat_names());
            // Health trims a beat's name as JavaScript does, and the catalog as Rust does: alike here.
            let js: Vec<&str> =
                arc.beats.iter().map(|b| crate::js::trim(b.split(':').next().unwrap_or_default())).collect();
            assert_eq!(shape.beats, js);
        }
        for name in
            ["Data Storytelling", "the problem-solution arc", "PARL", "Hero's Journey for Change", "no such arc", ""]
        {
            assert_eq!(shape(name).arc, arc(name).arc, "{name}");
            assert_eq!(shape_named(name).map(|s| &s.arc), named(name).map(|a| &a.arc), "{name}");
        }
    }

    #[test]
    fn the_arcs_and_personas_read_as_vzestup_keeps_them() {
        let names: Vec<&str> = arcs().iter().map(|a| a.arc.as_str()).collect();
        assert_eq!(names.len(), 13);
        assert_eq!(names[0], "Hero's Journey");
        assert_eq!(names[5], "Problem\u{2013}Solution");
        assert_eq!(names[12], "Hero\u{2019}s Journey for Change");
        assert!(arcs().iter().all(|a| a.points.len() == 100 && !a.beats.is_empty()));
        assert_eq!(arc("Data Storytelling").beat_names(), ["Context", "Data", "Insight", "Implication", "Action"]);
        assert_eq!(arc("no such arc").arc, "Hero's Journey");

        let ids: Vec<&str> = personas().iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "executives",
                "investors",
                "government",
                "customers",
                "colleagues",
                "students-educators",
                "conference-public"
            ]
        );
        let investors = persona(None, "Investors").unwrap();
        assert_eq!((investors.attention.average_span, investors.attention.preferred_slide_count), (7.0, 12));
        assert_eq!(persona(Some("customers"), "Investors").unwrap().name, "Customers");
        assert_eq!(persona(None, "the board").map(|p| p.id.as_str()), None);
        assert_eq!(persona(None, "Students").unwrap().id, "students-educators");
    }
}
